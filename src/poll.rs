//! The poll loop and everything it feeds: page loading, the reparse diff,
//! and the SSE event builders (V5.sv step 3). A pipeline, not a CRUD
//! concept — so a top-level module, outside models/ and logic/ (round 3).
//! It runs on its own thread with its own Store and never touches actix
//! state: the daemon spawns it and subscribes to the same broadcast the
//! browser connections replay from.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use anyhow::Result;
use tokio::sync::broadcast;

use std::os::unix::fs::MetadataExt;

use crate::format;
use crate::logic::base;
use crate::logic::conversation;
use crate::models::base::Store;
use crate::render;

pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(250);
const HEARTBEAT_EVERY: u32 = 12; // × POLL_INTERVAL ≈ 3s

#[derive(Debug, Clone)]
pub(crate) struct Outgoing {
    pub(crate) kind: &'static str, // "block" | "pages" | "threads"
    pub(crate) data: String,
}

/// One rendered block as the page sees it. Comparing these decides whether an
/// upsert goes out — html, position and headings are all part of "changed".
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Rendered {
    id: String,
    ord: String,
    html: String,
    headings_json: serde_json::Value,
}

/// Everything the daemon knows about one page, derived from its
/// file. Rebuilt from scratch whenever the file changes; never persisted.
#[derive(Debug, Clone, Default)]
pub(crate) struct PageState {
    pub(crate) props: serde_json::Map<String, serde_json::Value>,
    /// The source format (V3.sv's three formats). The pages event carries
    /// it so the client can withhold affordances imported pages don't have —
    /// today, editing: block splices are an .sv concept (found live on a
    /// bound .md, 2026-08-23).
    fmt: crate::config::Format,
    pub(crate) blocks: Vec<Rendered>,
    /// (mtime, len) of the file this state was derived from.
    stamp: Option<(SystemTime, u64)>,
    /// Extension blocks' raw context — attrs and body — kept for the
    /// SIDEVIEW_BLOCK injection when the frame is served. Only blocks whose
    /// tag matched a registered extension appear here.
    pub(crate) ext_blocks: HashMap<String, (Vec<(String, String)>, String)>,
    /// Files the page's blocks reference (sv-csv src=), with the stamps they
    /// were rendered from — the poll loop stats these too, so overwriting a
    /// referenced CSV re-renders its block: reference-never-embed made live.
    file_refs: Vec<(String, Option<(SystemTime, u64)>)>,
}

#[derive(Default)]
pub(crate) struct Shared {
    /// Binding order (most recently active first), as of the last poll.
    pub(crate) order: Vec<(String, i64)>,
    pub(crate) pages: HashMap<String, PageState>,
    /// Per-page conversation snapshots (threads + comments), pre-serialized.
    /// Page-scale full resends, same trust story as block replay: the client
    /// resets on connect and every change ships the whole page's conversation.
    pub(crate) conversations: HashMap<String, String>,
    /// Explicit outlines (page → parsed spec), riding the pages event as a
    /// prop so the client needs no fourth event kind.
    pub(crate) outlines: HashMap<String, serde_json::Value>,
    /// Installed extensions, loaded from config (reloaded when it changes).
    pub(crate) extensions: Vec<crate::config::Extension>,
}

/// One page's conversation, serialized for the `threads` SSE event. Sent
/// whole on every change — page-scale, same reasoning as block replay.
pub(crate) fn conversation_json(store: &Store, page: &str) -> Result<String> {
    conversation::snapshot_json(store, page)
}

pub(crate) fn threads_event(data: String) -> Outgoing {
    Outgoing { kind: "threads", data }
}

/// Change detection, liveness, supersession and the deleted-underneath-us
/// check, all in one loop that is already running. Content change detection
/// is file stat, not `data_version`: the db no longer holds content.
pub(crate) fn poll_loop(
    store_dir: &Path,
    instance_id: &str,
    tx: &broadcast::Sender<Outgoing>,
    shared: &Arc<Mutex<Shared>>,
) -> Result<()> {
    let store = Store::open(store_dir)?;
    let db_path = store.db_path();
    let opened = std::fs::metadata(&db_path)?;
    let (dev, ino) = (opened.dev(), opened.ino());
    let mut ticks: u32 = 0;
    // Conversation change detection: the generation counter, bumped in the
    // same transaction as every conversation write — one O(1) row read.
    // (data_version was caught missing a cross-process commit under WAL
    // after a long idle, live 2026-08-08; an aggregate probe briefly stood
    // in before the author called for the counter.)
    let mut conversation_gen = -1i64;
    // The repo config is a file like any other: stat it on the same tick, so
    // a category or a new imported page lands live.
    let mut config_stamp: Option<Option<(SystemTime, u64)>> = None;
    let mut cfg = crate::config::Config::default();

    loop {
        ticks = ticks.wrapping_add(1);

        // `rm -rf .sideview/` under a live daemon: the open handle keeps the
        // inode alive and the daemon goes silently blind. Turn the silent
        // version into the noisy one.
        match std::fs::metadata(&db_path) {
            Ok(m) if m.dev() == dev && m.ino() == ino => {}
            _ => {
                eprintln!(
                    "store at {} was deleted or replaced underneath this daemon — exiting",
                    db_path.display()
                );
                std::process::exit(1);
            }
        }

        if ticks % HEARTBEAT_EVERY == 0 {
            // Zero rows affected means somebody else claimed the row: stop
            // serving and let the browser reconnect to whoever holds it now.
            if !base::heartbeat(&store, instance_id)? {
                eprintln!("superseded by another daemon — exiting");
                std::process::exit(0);
            }
        }

        // Answer pings, only ever as the current holder.
        let pending: Option<(i64, i64)> = store
            .conn
            .query_row("SELECT ping, pong FROM daemon", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .ok();
        if let Some((ping, pong)) = pending {
            if ping != pong {
                base::answer_ping(&store, instance_id)?;
            }
        }

        // Config: reload on change, then make its pages exist. This is the
        // half that survives a deleted db — .sv files announce themselves in
        // the startup scan, a repo's markdown never does, and sideview must
        // not colonize it by scanning (V3.sv).
        let cfg_file = store.root.join(crate::config::FILE);
        let cfg_now = std::fs::metadata(&cfg_file)
            .ok()
            .and_then(|m| m.modified().ok().map(|t| (t, m.len())));
        let config_changed = config_stamp != Some(cfg_now);
        if config_changed {
            config_stamp = Some(cfg_now);
            let (c, err) = crate::config::load(&store.root);
            if let Some(e) = err {
                eprintln!("config ignored — {e}");
            }
            cfg = c;
            let (exts, problems) = crate::config::load_extensions(&store.root, &cfg);
            for (path, why) in &problems {
                eprintln!("extension {path} not loaded — {why}");
            }
            shared.lock().unwrap().extensions = exts;
            for e in &cfg.pages {
                let id = e.page_id();
                if base::binding(&store, &id)?.is_some() {
                    continue;
                }
                if !store.root.join(&e.path).exists() {
                    eprintln!("config: no file {} (page {id})", e.path);
                    continue;
                }
                base::bind_page(&store, &id, &e.path, &store.root.display().to_string(), "config")?;
                eprintln!("config page {id} → {}", e.path);
            }
        }

        let mut changed_pages = false;
        let mut events: Vec<Outgoing> = Vec::new();
        let bindings = base::bindings(&store)?;
        {
            let mut shared = shared.lock().unwrap();

            let order: Vec<(String, i64)> =
                bindings.iter().map(|b| (b.id.clone(), b.last_active_at)).collect();
            if order != shared.order {
                shared.order = order;
                changed_pages = true;
            }

            let exts = shared.extensions.clone();
            for b in &bindings {
                let file = store.root.join(&b.path);
                let stamp = std::fs::metadata(&file)
                    .ok()
                    .and_then(|m| m.modified().ok().map(|t| (t, m.len())));
                let entry = cfg.pages.iter().find(|e| e.page_id() == b.id || e.path == b.path);
                let known = shared.pages.get(&b.id).map(|p| p.stamp);
                // A config edit can change how a page renders or what it is
                // called, so it re-reads even when the file itself is still —
                // and a changed *referenced* file (a csv a block points at)
                // re-renders even though the page file never moved.
                let refs_stale = shared
                    .pages
                    .get(&b.id)
                    .map(|p| {
                        p.file_refs.iter().any(|(rel, old)| file_stamp(&store.root.join(rel)) != *old)
                    })
                    .unwrap_or(false);
                if known == Some(stamp) && !config_changed && !refs_stale {
                    continue;
                }
                let fmt =
                    crate::config::format_of(&b.path, entry.and_then(|e| e.render.as_deref()));
                let mut fresh = load_page(&file, &b.path, stamp, fmt, &b.id, &exts, &store.root);
                // What the chip's ✕ may do, decided here where the tier and
                // the config are both in hand: a throwaway page is scratch
                // and closes by deleting; a committed one only unbinds; and
                // a page the config declares has no meaningful close at all,
                // since it returns the moment the config is read again.
                let tier = if crate::models::base::is_throwaway_page(&b.path) {
                    "throwaway"
                } else {
                    "committed"
                };
                fresh.props.insert("tier".into(), serde_json::Value::String(tier.into()));
                // The index shows where a page lives; the file is the page.
                fresh.props.insert("path".into(), serde_json::Value::String(b.path.clone()));
                fresh.props.insert(
                    "closable".into(),
                    serde_json::Value::String(
                        if entry.is_some() { "config" } else { "yes" }.into(),
                    ),
                );
                // Config supplies only what the file cannot say about itself:
                // an .sv page's own props always win.
                if let Some(e) = entry {
                    for (key, val) in [
                        ("label", e.label.clone()),
                        ("category", e.category.clone()),
                        ("order", e.order.map(|o| o.to_string())),
                    ] {
                        if let Some(v) = val {
                            fresh.props.entry(key.to_string()).or_insert(serde_json::Value::String(v));
                        }
                    }
                }
                let old = shared.pages.insert(b.id.clone(), fresh);
                let fresh = &shared.pages[&b.id];
                if old.as_ref().map(|o| &o.props) != Some(&fresh.props) {
                    changed_pages = true;
                }
                events.extend(diff_events(&b.id, old.as_ref(), fresh));
            }
            // A binding that vanished takes its page state with it. (Nothing
            // deletes bindings in this slice, but be tolerant of a reset.)
            let live: std::collections::HashSet<&String> =
                bindings.iter().map(|b| &b.id).collect();
            shared.pages.retain(|id, _| live.contains(id));

            // On any conversation mutation: re-serialize conversation
            // snapshots (shipping the pages that changed), and reload the
            // explicit outlines, which ride the pages event as a prop.
            let g = conversation::generation(&store).unwrap_or(conversation_gen);
            if g != conversation_gen {
                conversation_gen = g;

                let fresh: HashMap<String, serde_json::Value> = base::outlines(&store)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|(page, spec)| {
                        serde_json::from_str(&spec).ok().map(|v| (page, v))
                    })
                    .collect();
                if fresh != shared.outlines {
                    shared.outlines = fresh;
                    changed_pages = true;
                }

                let pages = conversation::pages_with_conversation(&store).unwrap_or_default();
                for page in &pages {
                    if let Ok(json) = conversation_json(&store, page) {
                        if shared.conversations.get(page) != Some(&json) {
                            shared.conversations.insert(page.clone(), json.clone());
                            events.push(threads_event(json));
                        }
                    }
                }
                let conversing: std::collections::HashSet<&String> = pages.iter().collect();
                shared.conversations.retain(|page, _| {
                    let keep = conversing.contains(page);
                    if !keep {
                        events.push(threads_event(
                            serde_json::json!({
                                "page": page, "threads": [], "comments": [], "attachments": [],
                            })
                            .to_string(),
                        ));
                    }
                    keep
                });
            }

            if changed_pages {
                events.insert(0, pages_event(&shared));
            }
        }
        for e in events {
            let _ = tx.send(e);
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Read and render a page file into daemon state. A missing or unreadable
/// file renders as one honest block rather than an empty page — a binding
/// pointing at nothing is a fact worth showing.
/// An imported file as a one-block page. One block and not one per heading,
/// deliberately: block ids are what comments target, so heading-derived ids
/// would orphan every thread under a renamed heading (V3.sv).
fn imported_page(source: &str, fmt: crate::config::Format) -> format::Page {
    use crate::config::Format;
    let type_name = match fmt {
        Format::Markdown => "sv-prose",
        Format::HtmlInline => "sv-markup",
        Format::HtmlFrame => "sv-html",
        Format::Sv => unreachable!("sv files are parsed, not imported"),
    };
    format::Page {
        props: Vec::new(), // label/category come from config: the file cannot say
        blocks: vec![format::Block {
            type_name: type_name.into(),
            // A stable id, so a doc's comments survive every edit to it: the
            // anchor inside does the finer work.
            attrs: vec![("id".into(), "doc".into())],
            body: source.to_string(),
            warnings: Vec::new(),
            lines: (0, 0), // never spliced: the CLI only edits .sv pages
        }],
        warnings: Vec::new(),
    }
}

/// (mtime, len) of a file, None when it is missing — the shared shape for
/// page stamps and referenced-file stamps.
fn file_stamp(path: &Path) -> Option<(SystemTime, u64)> {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok().map(|t| (t, m.len())))
}

pub(crate) fn load_page(
    file: &Path,
    rel: &str,
    stamp: Option<(SystemTime, u64)>,
    fmt: crate::config::Format,
    page_id: &str,
    exts: &[crate::config::Extension],
    root: &Path,
) -> PageState {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(_) => {
            let msg = format!("page file moved or deleted — was {rel}");
            return PageState {
                ext_blocks: HashMap::new(),
                file_refs: Vec::new(),
                fmt,
                props: serde_json::Map::new(),
                blocks: vec![Rendered {
                    id: "sv-missing".into(),
                    ord: "a000000000000".into(),
                    html: format!(
                        r#"<section class="sv-block sv-degraded" data-block="sv-missing" data-type="sv-missing"><p class="sv-degraded-note">{}</p></section>"#,
                        msg.replace('<', "&lt;")
                    ),
                    headings_json: serde_json::json!([]),
                }],
                stamp,
            }
        }
    };
    // Three page formats, one renderer (V3.sv). An imported file is a page
    // sideview did not compose, so it becomes exactly one block through the
    // path that already exists for its content — no new block type, and the
    // limitations belong to the format.
    let page = match fmt {
        crate::config::Format::Sv => format::parse(&source),
        other => imported_page(&source, other),
    };
    let mut props = serde_json::Map::new();
    for (k, v) in &page.props {
        props.insert(k.clone(), serde_json::Value::String(v.clone()));
    }

    // Effective ids: the id attribute, or a content hash for blocks without
    // one (stable across reorders; an edit reads as remove-and-add, which is
    // the accepted cost of not naming a block). Duplicate hashes — two
    // identical anonymous blocks — get an occurrence suffix.
    let mut seen: HashMap<String, u32> = HashMap::new();
    let mut blocks = Vec::new();
    let mut ext_blocks: HashMap<String, (Vec<(String, String)>, String)> = HashMap::new();
    let mut file_refs: Vec<(String, Option<(SystemTime, u64)>)> = Vec::new();
    for (i, b) in page.blocks.iter().enumerate() {
        let base = match b.id() {
            Some(id) => id.to_string(),
            None => {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                (&b.type_name, &b.body).hash(&mut h);
                format!("h{:012x}", h.finish() & 0xffff_ffff_ffff)
            }
        };
        let n = seen.entry(base.clone()).or_insert(0);
        *n += 1;
        let id = if *n == 1 { base } else { format!("{base}-{n}") };
        // sv-csv reads its referenced file here, where root and the poll
        // loop live: content confined to the project (the /f/ rule), the
        // stamp recorded so an overwritten csv re-renders the block.
        if b.type_name == "sv-csv" {
            let src = b.attr("src").unwrap_or("").trim().to_string();
            let content = if src.is_empty() {
                Err("sv-csv needs src=\"<project-relative .csv>\"".to_string())
            } else if src.starts_with('/') || src.split('/').any(|seg| seg == "..") {
                Err(format!("{src}: outside the project"))
            } else {
                let abs = root.join(&src);
                file_refs.push((src.clone(), file_stamp(&abs)));
                std::fs::read_to_string(&abs)
                    .map_err(|e| format!("{src}: {e} — the block heals when the file appears"))
            };
            blocks.push(Rendered {
                ord: format!("a{i:012}"),
                html: crate::csv::block(&id, b, content),
                headings_json: serde_json::json!([]),
                id,
            });
            continue;
        }
        // A tag a registered extension claims renders as that extension's
        // frame; everything else takes the normal path (which ends at the
        // honest unknown-tag block).
        if let Some(ext) = exts.iter().find(|x| x.tag() == b.type_name) {
            ext_blocks.insert(id.clone(), (b.attrs.clone(), b.body.clone()));
            blocks.push(Rendered {
                ord: format!("a{i:012}"),
                html: render::ext_block(&id, &ext.manifest.name, page_id, b.attr("height")),
                headings_json: serde_json::json!([]),
                id,
            });
            continue;
        }
        blocks.push(Rendered {
            ord: format!("a{i:012}"),
            html: render::block(&id, b),
            headings_json: serde_json::to_value(render::outline(&id, b))
                .unwrap_or_else(|_| serde_json::json!([])),
            id,
        });
    }
    PageState { props, fmt, blocks, stamp, ext_blocks, file_refs }
}

/// The reparse diff: upserts for new or changed blocks, removes for gone ones.
/// This is what keeps live patching surgical even though the source of truth
/// is a whole file.
fn diff_events(page: &str, old: Option<&PageState>, new: &PageState) -> Vec<Outgoing> {
    let mut events = Vec::new();
    let old_by_id: HashMap<&str, &Rendered> = old
        .map(|o| o.blocks.iter().map(|b| (b.id.as_str(), b)).collect())
        .unwrap_or_default();
    for b in &new.blocks {
        if old_by_id.get(b.id.as_str()) != Some(&&*b) {
            events.push(block_event(page, b));
        }
    }
    let new_ids: std::collections::HashSet<&str> =
        new.blocks.iter().map(|b| b.id.as_str()).collect();
    for b in old.map(|o| o.blocks.as_slice()).unwrap_or_default() {
        if !new_ids.contains(b.id.as_str()) {
            events.push(Outgoing {
                kind: "block",
                data: serde_json::json!({
                    "page": page,
                    "block": b.id,
                    "action": "remove",
                })
                .to_string(),
            });
        }
    }
    events
}

pub(crate) fn block_event(page: &str, b: &Rendered) -> Outgoing {
    Outgoing {
        kind: "block",
        data: serde_json::json!({
            "page": page,
            "block": b.id,
            "action": "upsert",
            "ord": b.ord,
            "html": b.html,
            "headings": b.headings_json,
        })
        .to_string(),
    }
}

pub(crate) fn pages_event(shared: &Shared) -> Outgoing {
    let pages: Vec<serde_json::Value> = shared
        .order
        .iter()
        .map(|(id, last_active_at)| {
            // Props pass through whole from the file, so a key a newer CLI
            // writes reaches the page even through a daemon that has never
            // heard of it. An explicit outline (db, not file) rides along
            // as outline_spec — used verbatim by the rail when present.
            let mut props = shared
                .pages
                .get(id)
                .map(|p| p.props.clone())
                .unwrap_or_default();
            if let Some(spec) = shared.outlines.get(id) {
                props.insert("outline_spec".into(), spec.clone());
            }
            // "sv" pages edit from the page; imported ones don't (their
            // canon is a foreign file, not spliceable blocks).
            let format = match shared.pages.get(id).map(|p| p.fmt) {
                Some(crate::config::Format::Markdown) => "markdown",
                Some(crate::config::Format::HtmlInline) => "html-inline",
                Some(crate::config::Format::HtmlFrame) => "html-frame",
                _ => "sv",
            };
            serde_json::json!({ "id": id, "last_active_at": last_active_at, "format": format, "props": props })
        })
        .collect();
    Outgoing {
        kind: "pages",
        data: serde_json::json!({ "pages": pages }).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_from(src: &str) -> PageState {
        let dir = std::env::temp_dir().join(format!("sv-pg-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("p.sv");
        std::fs::write(&file, src).unwrap();
        load_page(&file, "p.sv", None, crate::config::Format::Sv, "test", &[], &dir)
    }

    #[test]
    fn reparse_diff_is_surgical() {
        let old = page_from("<sv-prose id=\"b1\">\none\n</sv-prose>\n<sv-prose id=\"b2\">\ntwo\n</sv-prose>\n");
        let new = page_from("<sv-prose id=\"b1\">\none\n</sv-prose>\n<sv-prose id=\"b2\">\ntwo, revised\n</sv-prose>\n");
        let events = diff_events("s", Some(&old), &new);
        assert_eq!(events.len(), 1, "only the changed block goes out: {events:?}");
        assert!(events[0].data.contains("b2"));
        assert!(events[0].data.contains("revised"));
    }

    #[test]
    fn removed_blocks_emit_removes_and_anonymous_blocks_get_stable_ids() {
        let old = page_from("<sv-prose id=\"b1\">\none\n</sv-prose>\n<sv-markup>\n<b>anon</b>\n</sv-markup>\n");
        let new = page_from("<sv-markup>\n<b>anon</b>\n</sv-markup>\n");
        // b1 gone; the anonymous block keeps its hash id across the reorder,
        // so the only event is the remove.
        let events = diff_events("s", Some(&old), &new);
        let removes: Vec<_> = events.iter().filter(|e| e.data.contains("remove")).collect();
        assert_eq!(removes.len(), 1, "{events:?}");
        assert!(removes[0].data.contains("b1"));
        let upserts: Vec<_> = events.iter().filter(|e| e.data.contains("upsert")).collect();
        assert_eq!(upserts.len(), 1, "the anon block moved position (ord changed): {events:?}");
    }

    #[test]
    fn a_missing_file_renders_an_honest_block() {
        let page = load_page(Path::new("/nonexistent/nowhere.sv"), "nowhere.sv", None, crate::config::Format::Sv, "test", &[], Path::new("/tmp"));
        assert_eq!(page.blocks.len(), 1);
        assert!(page.blocks[0].html.contains("moved or deleted"), "{}", page.blocks[0].html);
    }
}
