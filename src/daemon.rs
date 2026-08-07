//! The daemon: serves the page, notices changed page files by polling their
//! mtimes (bindings are the watch list — that's what they're *for*), reparses,
//! and patches the open page over SSE with only the blocks that changed.
//! Long-lived, no idle exit (auto-exit depends on auto-restart, which is
//! precisely what a sandboxed agent cannot do).
//!
//! All render/replay state is in memory, derived from the files — the db
//! holds no content. A new connection always receives the full current state
//! (the client resets on connect), which dissolves `Last-Event-ID` replay,
//! tombstones and the rev counter in one move: at page scale a full resend
//! is cheaper than being clever, and a daemon restart can't strand a client
//! on a rev sequence that no longer exists.

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use actix_web::web::{self, Data};
use actix_web::{App, HttpResponse, HttpServer, Responder};
use actix_web_lab::sse;
use anyhow::{Context, Result};
use futures_util::StreamExt as _;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::format;
use crate::netcheck;
use crate::render;
use crate::store::{now_ms, DaemonRow, Store};

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const HEARTBEAT_EVERY: u32 = 12; // × POLL_INTERVAL ≈ 3s

#[derive(Debug, Clone)]
pub struct Opts {
    /// `--bind auto`: loopback plus the tailnet address when there is one.
    pub bind_auto: bool,
    /// Open the browser once the row is claimed (the auto-spawn "first use"
    /// path; bare foreground `sideview` also uses it).
    pub open_browser: bool,
}

/// A pre-serialized SSE event, fanned out to every connected page.
#[derive(Debug, Clone)]
struct Outgoing {
    kind: &'static str, // "block" | "sessions" | "threads"
    data: String,
}

/// One rendered block as the page sees it. Comparing these decides whether an
/// upsert goes out — html, position and headings are all part of "changed".
#[derive(Debug, Clone, PartialEq)]
struct Rendered {
    id: String,
    ord: String,
    html: String,
    headings_json: serde_json::Value,
}

/// Everything the daemon knows about one session's page, derived from its
/// file. Rebuilt from scratch whenever the file changes; never persisted.
#[derive(Debug, Clone, Default)]
struct PageState {
    props: serde_json::Map<String, serde_json::Value>,
    blocks: Vec<Rendered>,
    /// (mtime, len) of the file this state was derived from.
    stamp: Option<(SystemTime, u64)>,
}

#[derive(Default)]
struct Shared {
    /// Binding order (most recently active first), as of the last poll.
    sessions: Vec<(String, i64)>,
    pages: HashMap<String, PageState>,
    /// Per-page conversation snapshots (threads + comments), pre-serialized.
    /// Page-scale full resends, same trust story as block replay: the client
    /// resets on connect and every change ships the whole page's conversation.
    conversations: HashMap<String, String>,
    /// Explicit outlines (page → parsed spec), riding the sessions event as a
    /// prop so the client needs no fourth event kind.
    outlines: HashMap<String, serde_json::Value>,
}

struct AppState {
    shared: Arc<Mutex<Shared>>,
    root: PathBuf,
    tx: broadcast::Sender<Outgoing>,
    /// For the page's one write (session deletion) and the shutdown clear.
    /// The poll loop has its own connection; handlers otherwise never touch
    /// the store.
    store: Mutex<Store>,
}

#[derive(rust_embed::Embed)]
#[folder = "static/"]
struct Assets;

pub fn run(store_dir: &Path, opts: &Opts) -> Result<()> {
    let verdict = netcheck::verdict();
    if !verdict.reachable {
        // Started anyway (someone forced it); record the truth so bare
        // `sideview` on the host claims the row instead of deferring to us.
        eprintln!(
            "warning: this daemon is provably unreachable from outside ({}) — recording reachable=0",
            verdict.reasons.join(", ")
        );
    }

    let mut store = Store::open(store_dir)?;

    // Fresh-clone (and fresh-db) rediscovery: pages are files, so a deleted
    // store must not lose them. Committed .sv files re-bind by stem; the
    // throwaway pages under .sideview/pages/ re-bind by their encoded name.
    // The resurrection test (V2.sv's goal) leans on this running first.
    if let Err(e) = rediscover_pages(&store) {
        eprintln!("note: page rediscovery failed: {e:#}");
    }

    // Port: remembered so an open tab reconnects to the same origin across
    // restarts. Taken? Take another and print the new URL. Meta is the
    // durable copy (the row is cleared on clean shutdown); a crash-leftover
    // row is the fallback.
    let remembered = store
        .meta("port")?
        .and_then(|p| p.parse().ok())
        .or(store.daemon()?.map(|d| d.port))
        .unwrap_or(0);
    let loopback = TcpListener::bind((Ipv4Addr::LOCALHOST, remembered))
        .or_else(|_| TcpListener::bind((Ipv4Addr::LOCALHOST, 0)))
        .context("binding loopback")?;
    let port = loopback.local_addr()?.port();

    let mut listeners = vec![loopback];
    let mut urls = vec![format!("http://127.0.0.1:{port}")];
    if opts.bind_auto {
        for ip in netcheck::tailnet_addrs() {
            // EADDRNOTAVAIL (tailscaled came up after us, or is down): degrade
            // to loopback-only and say so, never die.
            match TcpListener::bind((ip, port)) {
                Ok(l) => {
                    urls.push(match ip {
                        IpAddr::V4(v4) => format!("http://{v4}:{port}"),
                        IpAddr::V6(v6) => format!("http://[{v6}]:{port}"),
                    });
                    listeners.push(l);
                }
                Err(e) => eprintln!("note: tailnet address {ip} not bindable ({e}); loopback only"),
            }
        }
    }

    // Claim the row *after* binding, never before: binding is the step that
    // can fail, and claim-then-bind would evict a healthy daemon on behalf of
    // one that never started.
    let instance_id = uuid::Uuid::new_v4().to_string();
    store.claim_daemon(&DaemonRow {
        instance_id: instance_id.clone(),
        pid: std::process::id() as i64,
        port,
        version: env!("CARGO_PKG_VERSION").to_string(),
        started_at: now_ms(),
        last_seen: now_ms(),
        ping: 0,
        pong: 0,
        netns: verdict.netns,
        reachable: verdict.reachable,
    })?;
    store.set_meta("port", &port.to_string())?;
    // The bound URLs, durably — so the CLI can print the tailnet addresses
    // instead of them living only in this log.
    store.set_meta("urls", &serde_json::to_string(&urls)?)?;

    for (i, url) in urls.iter().enumerate() {
        if i == 0 {
            eprintln!("local:   {url}");
        } else {
            eprintln!("tailnet: {url}      (any tailnet node can read this)");
        }
    }

    if opts.open_browser {
        crate::cli::open_browser(&format!("http://127.0.0.1:{port}/"));
    }

    let (tx, _) = broadcast::channel::<Outgoing>(1024);
    let shared = Arc::new(Mutex::new(Shared::default()));

    // The poll loop gets its own db connection on its own thread; the actix
    // handlers only ever touch the in-memory state.
    {
        let dir = store_dir.to_path_buf();
        let tx = tx.clone();
        let instance_id = instance_id.clone();
        let shared = shared.clone();
        std::thread::spawn(move || {
            if let Err(e) = poll_loop(&dir, &instance_id, &tx, &shared) {
                eprintln!("poll loop died: {e:#}");
                std::process::exit(1);
            }
        });
    }

    let root = store.root.clone();
    let state = Data::new(AppState { shared, root, tx, store: Mutex::new(store) });

    let server_state = state.clone();
    actix_web::rt::System::new().block_on(async move {
        let mut server = HttpServer::new(move || {
            App::new()
                .app_data(server_state.clone())
                .route("/", web::get().to(page))
                .route("/s/{session}", web::get().to(page))
                .route("/events", web::get().to(events))
                .route("/api/pages/{page}", web::delete().to(delete_session))
                // The old noun, one release of grace — same handler.
                .route("/api/sessions/{session}", web::delete().to(delete_session))
                .route("/api/comments", web::post().to(post_comment))
                .route("/api/threads/{id}/resolve", web::post().to(resolve_thread))
                .route("/api/threads/{id}/unresolve", web::post().to(unresolve_thread))
                .route("/f/{path:.*}", web::get().to(project_file))
                .route("/assets/{path:.*}", web::get().to(asset))
        })
        .workers(2)
        // SSE streams never finish on their own, so a graceful drain would
        // stall every shutdown for the full default 30s whenever a tab is
        // open. Nothing in flight is worth draining; drop and let the
        // browser's EventSource reconnect to the next daemon.
        .shutdown_timeout(1);
        for l in listeners {
            server = server.listen(l)?;
        }
        server.run().await
    })?;

    // Ctrl-C lands here: clean shutdown clears the row (only as its holder).
    state.store.lock().unwrap().clear_daemon(&instance_id)?;
    eprintln!("daemon stopped");
    Ok(())
}

/// The page's one write into the project: tidying power, not authoring power
/// (V1.md) — anyone who can see the page can delete a session, and cannot
/// create or alter content. Deleting a page is deleting its file; the poll
/// loop notices the binding is gone on its next tick and the sessions
/// snapshot converges every client.
async fn delete_session(path: web::Path<String>, state: Data<AppState>) -> impl Responder {
    let id = path.into_inner();
    let mut store = state.store.lock().unwrap();
    let Ok(Some(binding)) = store.binding(&id) else {
        return HttpResponse::NotFound().body(format!("no session {id:?}"));
    };
    let file = state.root.join(&binding.path);
    if let Err(e) = std::fs::remove_file(&file) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return HttpResponse::InternalServerError()
                .body(format!("removing {}: {e}", file.display()));
        }
    }
    let _ = std::fs::remove_file(file.with_extension("sv.lock"));
    match store.delete_binding(&id) {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(e) => HttpResponse::InternalServerError().body(format!("{e:#}")),
    }
}

/// Re-find every page file the db doesn't know: committed .sv files bind by
/// stem (V2.sv → "V2"), throwaway pages under .sideview/pages/ by decoding
/// their filename back to the session id. Chip order for rediscovered pages
/// comes from canon: the `order` attribute on `<sv-page>` when the author
/// cares, path order otherwise — binding insertion order carries it.
fn rediscover_pages(store: &Store) -> Result<()> {
    let known: std::collections::HashSet<String> =
        store.bindings()?.into_iter().map(|b| b.path).collect();
    let mut found: Vec<(f64, String, String, String)> = Vec::new(); // (order, path, rel, id)

    let pages_dir = store.dir.join(crate::store::PAGES_DIR);
    let mut stack = vec![store.root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                // Hidden trees, dependencies and build output stay unscanned —
                // except the store's own throwaway pages.
                if name.starts_with('.') || name == "node_modules" || name == "target" {
                    if path == store.dir || store.dir.starts_with(&path) {
                        stack.push(pages_dir.clone());
                    }
                    continue;
                }
                stack.push(path);
            } else if name.ends_with(".sv") {
                let Ok(rel) = path.strip_prefix(&store.root) else { continue };
                let rel = rel.to_string_lossy().to_string();
                if known.contains(&rel) {
                    continue;
                }
                let stem = name.trim_end_matches(".sv").to_string();
                let id = if path.starts_with(&pages_dir) {
                    // The throwaway filename is the encoded session id.
                    percent_encoding::percent_decode_str(&stem)
                        .decode_utf8_lossy()
                        .to_string()
                } else {
                    stem
                };
                let order = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|src| {
                        format::parse(&src).prop("order").and_then(|o| o.parse::<f64>().ok())
                    })
                    .unwrap_or(f64::MAX);
                found.push((order, rel.clone(), rel, id));
            }
        }
    }

    found.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
    for (_, _, rel, id) in found {
        if store.binding(&id)?.is_some() {
            eprintln!("note: {rel} not rebound — a different file already holds page id {id:?}");
            continue;
        }
        store.bind_session(&id, &rel, &store.root.display().to_string(), "rediscovered")?;
        eprintln!("rediscovered page {id} ({rel})");
        // Distinct started_at millis keep the canon order stable in the strip.
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}

/// The browser's write path for conversation: the page talks *about* the
/// document, never *as* it. A `thread` field replies; page+target starts a
/// fresh thread at the anchor (threads succeed each other — see V2.sv).
#[derive(serde::Deserialize)]
struct CommentBody {
    thread: Option<i64>,
    page: Option<String>,
    target: Option<String>,
    #[serde(default)]
    anchor: String,
    quote: Option<String>,
    context: Option<String>,
    body: String,
}

async fn post_comment(body: web::Json<CommentBody>, state: Data<AppState>) -> impl Responder {
    let b = body.into_inner();
    if b.body.trim().is_empty() {
        return HttpResponse::BadRequest().body("empty comment body");
    }
    let mut store = state.store.lock().unwrap();
    let result = match (b.thread, b.page.as_deref(), b.target.as_deref()) {
        (Some(tid), _, _) => store.reply(tid, &b.body, Some("user")).map(|cid| (tid, cid)),
        (None, Some(page), Some(target)) => store.create_thread(
            page,
            target,
            &b.anchor,
            b.quote.as_deref(),
            b.context.as_deref(),
            &b.body,
            Some("user"),
        ),
        _ => return HttpResponse::BadRequest().body("pass thread, or page and target"),
    };
    match result {
        Ok((thread, id)) => HttpResponse::Ok().json(serde_json::json!({
            "thread": thread, "id": id,
        })),
        Err(e) => HttpResponse::BadRequest().body(format!("{e:#}")),
    }
}

async fn resolve_thread(path: web::Path<i64>, state: Data<AppState>) -> impl Responder {
    set_resolution(path.into_inner(), false, &state)
}

async fn unresolve_thread(path: web::Path<i64>, state: Data<AppState>) -> impl Responder {
    set_resolution(path.into_inner(), true, &state)
}

/// Resolve is undoable and idempotent from the page's point of view: asking
/// for a state the thread already holds is success, not conflict.
fn set_resolution(id: i64, undo: bool, state: &Data<AppState>) -> HttpResponse {
    let store = state.store.lock().unwrap();
    match store.thread(id) {
        Ok(None) => HttpResponse::NotFound().body(format!("no thread {id}")),
        Ok(Some(_)) => match store.resolve_thread(id, Some("user"), undo) {
            Ok(_) => HttpResponse::NoContent().finish(),
            Err(e) => HttpResponse::InternalServerError().body(format!("{e:#}")),
        },
        Err(e) => HttpResponse::InternalServerError().body(format!("{e:#}")),
    }
}

/// One page's conversation, serialized for the `threads` SSE event. Sent
/// whole on every change — page-scale, same reasoning as block replay.
fn conversation_json(store: &Store, page: &str) -> Result<String> {
    Ok(serde_json::json!({
        "page": page,
        "threads": store.threads_for_page(page)?,
        "comments": store.comments_for_page(page)?,
    })
    .to_string())
}

fn threads_event(data: String) -> Outgoing {
    Outgoing { kind: "threads", data }
}

/// Change detection, liveness, supersession and the deleted-underneath-us
/// check, all in one loop that is already running. Content change detection
/// is file stat, not `data_version`: the db no longer holds content.
fn poll_loop(
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
    // Conversation change detection: `data_version` bumps when any other
    // connection commits — a browser comment through our handlers, or an
    // agent's `sideview comment` from another process entirely.
    let mut conversation_version: i64 = -1;

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
            if !store.heartbeat(instance_id)? {
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
                store.answer_ping(instance_id)?;
            }
        }

        let mut changed_sessions = false;
        let mut events: Vec<Outgoing> = Vec::new();
        let bindings = store.bindings()?;
        {
            let mut shared = shared.lock().unwrap();

            let order: Vec<(String, i64)> =
                bindings.iter().map(|b| (b.id.clone(), b.last_active_at)).collect();
            if order != shared.sessions {
                shared.sessions = order;
                changed_sessions = true;
            }

            for b in &bindings {
                let file = store.root.join(&b.path);
                let stamp = std::fs::metadata(&file)
                    .ok()
                    .and_then(|m| m.modified().ok().map(|t| (t, m.len())));
                let known = shared.pages.get(&b.id).map(|p| p.stamp);
                if known == Some(stamp) {
                    continue;
                }
                let fresh = load_page(&file, &b.path, stamp);
                let old = shared.pages.insert(b.id.clone(), fresh);
                let fresh = &shared.pages[&b.id];
                if old.as_ref().map(|o| &o.props) != Some(&fresh.props) {
                    changed_sessions = true;
                }
                events.extend(diff_events(&b.id, old.as_ref(), fresh));
            }
            // A binding that vanished takes its page state with it. (Nothing
            // deletes bindings in this slice, but be tolerant of a reset.)
            let live: std::collections::HashSet<&String> =
                bindings.iter().map(|b| &b.id).collect();
            shared.pages.retain(|id, _| live.contains(id));

            // On any db commit from elsewhere: re-serialize conversation
            // snapshots (shipping the pages that changed), and reload the
            // explicit outlines, which ride the sessions event as a prop.
            let v = store.data_version().unwrap_or(conversation_version);
            if v != conversation_version {
                conversation_version = v;

                let fresh: HashMap<String, serde_json::Value> = store
                    .outlines()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|(page, spec)| {
                        serde_json::from_str(&spec).ok().map(|v| (page, v))
                    })
                    .collect();
                if fresh != shared.outlines {
                    shared.outlines = fresh;
                    changed_sessions = true;
                }

                let pages = store.conversation_pages().unwrap_or_default();
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
                                "page": page, "threads": [], "comments": [],
                            })
                            .to_string(),
                        ));
                    }
                    keep
                });
            }

            if changed_sessions {
                events.insert(0, sessions_event(&shared));
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
fn load_page(file: &Path, rel: &str, stamp: Option<(SystemTime, u64)>) -> PageState {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(_) => {
            let msg = format!("page file moved or deleted — was {rel}");
            return PageState {
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
    let page = format::parse(&source);
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
        blocks.push(Rendered {
            ord: format!("a{i:012}"),
            html: render::block(&id, b),
            headings_json: serde_json::to_value(render::outline(&id, b))
                .unwrap_or_else(|_| serde_json::json!([])),
            id,
        });
    }
    PageState { props, blocks, stamp }
}

/// The reparse diff: upserts for new or changed blocks, removes for gone ones.
/// This is what keeps live patching surgical even though the source of truth
/// is a whole file.
fn diff_events(session: &str, old: Option<&PageState>, new: &PageState) -> Vec<Outgoing> {
    let mut events = Vec::new();
    let old_by_id: HashMap<&str, &Rendered> = old
        .map(|o| o.blocks.iter().map(|b| (b.id.as_str(), b)).collect())
        .unwrap_or_default();
    for b in &new.blocks {
        if old_by_id.get(b.id.as_str()) != Some(&&*b) {
            events.push(block_event(session, b));
        }
    }
    let new_ids: std::collections::HashSet<&str> =
        new.blocks.iter().map(|b| b.id.as_str()).collect();
    for b in old.map(|o| o.blocks.as_slice()).unwrap_or_default() {
        if !new_ids.contains(b.id.as_str()) {
            events.push(Outgoing {
                kind: "block",
                data: serde_json::json!({
                    "session": session,
                    "block": b.id,
                    "action": "remove",
                })
                .to_string(),
            });
        }
    }
    events
}

fn block_event(session: &str, b: &Rendered) -> Outgoing {
    Outgoing {
        kind: "block",
        data: serde_json::json!({
            "session": session,
            "block": b.id,
            "action": "upsert",
            "ord": b.ord,
            "html": b.html,
            "headings": b.headings_json,
        })
        .to_string(),
    }
}

fn sessions_event(shared: &Shared) -> Outgoing {
    let sessions: Vec<serde_json::Value> = shared
        .sessions
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
            serde_json::json!({ "id": id, "last_active_at": last_active_at, "props": props })
        })
        .collect();
    Outgoing {
        kind: "sessions",
        data: serde_json::json!({ "sessions": sessions }).to_string(),
    }
}

fn to_sse(o: Outgoing) -> sse::Event {
    sse::Event::Data(sse::Data::new(o.data).event(o.kind))
}

async fn page() -> impl Responder {
    match Assets::get("index.html") {
        Some(f) => HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(f.data.into_owned()),
        None => HttpResponse::InternalServerError().body("index.html missing from binary"),
    }
}

async fn asset(path: web::Path<String>) -> impl Responder {
    let rel = path.into_inner();
    match Assets::get(&rel) {
        Some(f) => HttpResponse::Ok()
            .content_type(
                mime_guess::from_path(&rel).first_or_octet_stream().to_string(),
            )
            // Embedded assets change on every binary upgrade with unchanged
            // URLs, and phones cache aggressively: force revalidation.
            .insert_header(("Cache-Control", "no-cache"))
            .body(f.data.into_owned()),
        None => HttpResponse::NotFound().body(format!("no embedded asset {rel}")),
    }
}

/// One long-lived stream per page. Every connection starts with the full
/// current state — the client resets on connect — so reconnection after any
/// gap (sleep, daemon restart, lagged stream) converges by construction.
async fn events(state: Data<AppState>) -> actix_web::Result<impl Responder> {
    // Subscribe before snapshotting: an event landing in between is delivered
    // twice, and upserts are idempotent; the other order loses it.
    let rx = state.tx.subscribe();

    let mut replay: Vec<sse::Event> = Vec::new();
    {
        let shared = state.shared.lock().unwrap();
        replay.push(to_sse(sessions_event(&shared)));
        for (id, _) in &shared.sessions {
            if let Some(page) = shared.pages.get(id) {
                for b in &page.blocks {
                    replay.push(to_sse(block_event(id, b)));
                }
            }
        }
        for json in shared.conversations.values() {
            replay.push(to_sse(threads_event(json.clone())));
        }
    }

    let stream = futures_util::stream::iter(replay)
        .chain(live_events(rx))
        .map(Ok::<_, Infallible>);

    Ok(sse::Sse::from_stream(stream)
        .with_keep_alive(Duration::from_secs(15))
        .customize()
        .insert_header(("X-Accel-Buffering", "no")))
}

/// Only paths inside the project root, ever. Resolve symlinks, compare
/// against the root, and return a clear error rather than a 404 so a
/// mistyped path is diagnosable.
async fn project_file(path: web::Path<String>, state: Data<AppState>) -> impl Responder {
    let rel = path.into_inner();
    if rel.starts_with('/') || rel.split('/').any(|c| c == "..") {
        return HttpResponse::Forbidden()
            .body(format!("refusing {rel:?}: paths must be relative, inside the project"));
    }
    let root = match state.root.canonicalize() {
        Ok(r) => r,
        Err(e) => return HttpResponse::InternalServerError().body(format!("project root: {e}")),
    };
    let full = match root.join(&rel).canonicalize() {
        Ok(f) => f,
        Err(_) => {
            return HttpResponse::NotFound()
                .body(format!("no file {rel:?} under {}", root.display()))
        }
    };
    if !full.starts_with(&root) {
        return HttpResponse::Forbidden().body(format!(
            "refusing {rel:?}: resolves outside the project root ({})",
            full.display()
        ));
    }
    // The store's internals are not project content — bindings and the daemon
    // row are nobody's business over the tailnet. Only the named internals
    // are refused: other files under .sideview/ still serve (pages, and the
    // dogfood comparison pages iframe from there).
    if let Ok(store_dir) = root.join(crate::store::DIR_NAME).canonicalize() {
        if full.starts_with(&store_dir) {
            let name = full.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with(crate::store::DB_FILE)
                || name == "daemon.log"
                || name == crate::store::SPAWN_LOCK
            {
                return HttpResponse::Forbidden()
                    .body(format!("refusing {rel:?}: the store's internals are not served"));
            }
        }
    }
    match std::fs::read(&full) {
        Ok(bytes) => HttpResponse::Ok()
            .content_type(mime_guess::from_path(&full).first_or_octet_stream().to_string())
            .body(bytes),
        Err(e) => HttpResponse::NotFound().body(format!("reading {rel:?}: {e}")),
    }
}

/// The live half of an SSE stream. A subscriber that falls behind the
/// broadcast buffer gets `Err(Lagged)` — it has *lost events*, so the stream
/// must end rather than resume: a dropped stream makes `EventSource`
/// reconnect, and the full-state connection heals the gap. Skipping the
/// error and carrying on would silently desynchronize the page.
fn live_events(
    rx: broadcast::Receiver<Outgoing>,
) -> impl futures_util::Stream<Item = sse::Event> {
    BroadcastStream::new(rx)
        .take_while(|r| std::future::ready(r.is_ok()))
        .filter_map(|r| async move { r.ok().map(to_sse) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lagged_subscriber_stream_ends_instead_of_resuming() {
        actix_web::rt::System::new().block_on(async {
            let (tx, rx) = broadcast::channel::<Outgoing>(1);
            // Three sends into a one-slot buffer: the receiver has lost events.
            for _ in 0..3 {
                tx.send(Outgoing { kind: "block", data: String::new() }).unwrap();
            }
            drop(tx);
            let got: Vec<_> = live_events(rx).collect().await;
            assert_eq!(
                got.len(),
                0,
                "a lagged stream must terminate (forcing a fresh full-state connection), not skip ahead"
            );
        });
    }

    fn page_from(src: &str) -> PageState {
        let dir = std::env::temp_dir().join(format!("sv-pg-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("p.sv");
        std::fs::write(&file, src).unwrap();
        load_page(&file, "p.sv", None)
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
        let page = load_page(Path::new("/nonexistent/nowhere.sv"), "nowhere.sv", None);
        assert_eq!(page.blocks.len(), 1);
        assert!(page.blocks[0].html.contains("moved or deleted"), "{}", page.blocks[0].html);
    }

    #[actix_web::test]
    async fn file_endpoint_refuses_store_internals_but_serves_neighbours() {
        let dir = std::env::temp_dir().join(format!("sv-fe-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::store::DIR_NAME)).unwrap();
        std::fs::write(store.root.join("ok.html"), "<p>ok</p>").unwrap();
        std::fs::write(store.dir.join("lab.html"), "<p>lab</p>").unwrap();
        std::fs::write(store.dir.join("sideview.db-pre-v4"), "backup").unwrap();
        std::fs::write(store.dir.join(crate::store::SPAWN_LOCK), "").unwrap();
        let (tx, _) = broadcast::channel(8);
        let state = Data::new(AppState {
            shared: Arc::new(Mutex::new(Shared::default())),
            root: store.root.clone(),
            tx,
            store: Mutex::new(store),
        });
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(state)
                .route("/f/{path:.*}", web::get().to(project_file)),
        )
        .await;
        for (path, expect_ok) in [
            ("/f/ok.html", true),
            ("/f/.sideview/lab.html", true),                // labs iframe from here
            ("/f/.sideview/sideview.db", false),            // bindings + daemon row
            ("/f/.sideview/sideview.db-pre-v4", false),     // backups too
            ("/f/.sideview/spawn.lock", false),
        ] {
            let req = actix_web::test::TestRequest::get().uri(path).to_request();
            let res = actix_web::test::call_service(&app, req).await;
            // Absent internals must still refuse, not 404 into existence checks.
            let ok = res.status().is_success();
            let refused = res.status() == actix_web::http::StatusCode::FORBIDDEN;
            if expect_ok {
                assert!(ok, "{path} should serve, got {}", res.status());
            } else {
                assert!(refused, "{path} must be forbidden, got {}", res.status());
            }
        }
    }

    #[actix_web::test]
    async fn delete_session_removes_file_and_binding_and_404s_on_unknown() {
        let dir = std::env::temp_dir().join(format!("sv-del-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::store::DIR_NAME)).unwrap();
        store.bind_session("s1", ".sideview/pages/s1.sv", "/tmp", "test").unwrap();
        let file = store.pages_dir().unwrap().join("s1.sv");
        std::fs::write(&file, "<sv-prose id=\"b1\">\nx\n</sv-prose>\n").unwrap();
        let (tx, _) = broadcast::channel(8);
        let state = Data::new(AppState {
            shared: Arc::new(Mutex::new(Shared::default())),
            root: store.root.clone(),
            tx,
            store: Mutex::new(store),
        });
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(state.clone())
                .route("/api/sessions/{session}", web::delete().to(delete_session)),
        )
        .await;

        let req = actix_web::test::TestRequest::delete().uri("/api/sessions/nope").to_request();
        let res = actix_web::test::call_service(&app, req).await;
        assert_eq!(res.status(), actix_web::http::StatusCode::NOT_FOUND);

        let req = actix_web::test::TestRequest::delete().uri("/api/sessions/s1").to_request();
        let res = actix_web::test::call_service(&app, req).await;
        assert_eq!(res.status(), actix_web::http::StatusCode::NO_CONTENT);
        assert!(!file.exists(), "the page file is deleted");
        assert!(
            state.store.lock().unwrap().binding("s1").unwrap().is_none(),
            "the binding is deleted"
        );
    }

    #[test]
    fn rediscovery_binds_committed_and_throwaway_pages_in_canon_order() {
        let dir = std::env::temp_dir().join(format!("sv-rd-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::store::DIR_NAME)).unwrap();
        // Two committed pages, order attribute inverting path order…
        std::fs::write(store.root.join("zebra.sv"), "<sv-page order=\"1\">\n</sv-page>\n").unwrap();
        std::fs::write(store.root.join("alpha.sv"), "<sv-page order=\"2\">\n</sv-page>\n").unwrap();
        // …and a throwaway whose filename encodes its session id.
        std::fs::write(
            store.pages_dir().unwrap().join("cwd%3A%2Ftmp%2Fp.sv"),
            "<sv-prose id=\"b1\">\nx\n</sv-prose>\n",
        )
        .unwrap();
        rediscover_pages(&store).unwrap();
        let bindings = store.bindings().unwrap();
        let ids: Vec<&str> = bindings.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["zebra", "alpha", "cwd:/tmp/p"],
            "order attr beats path order; throwaway ids decode"
        );
        // Idempotent: a second scan binds nothing new.
        rediscover_pages(&store).unwrap();
        assert_eq!(store.bindings().unwrap().len(), 3);
    }

    #[actix_web::test]
    async fn comment_endpoint_creates_threads_replies_and_resolves() {
        let dir = std::env::temp_dir().join(format!("sv-cm-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::store::DIR_NAME)).unwrap();
        let (tx, _) = broadcast::channel(8);
        let state = Data::new(AppState {
            shared: Arc::new(Mutex::new(Shared::default())),
            root: store.root.clone(),
            tx,
            store: Mutex::new(store),
        });
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(state.clone())
                .route("/api/comments", web::post().to(post_comment))
                .route("/api/threads/{id}/resolve", web::post().to(resolve_thread))
                .route("/api/threads/{id}/unresolve", web::post().to(unresolve_thread)),
        )
        .await;

        // First comment creates its thread…
        let req = actix_web::test::TestRequest::post()
            .uri("/api/comments")
            .set_json(serde_json::json!({
                "page": "v2", "target": "b3", "anchor": "p:3f9c2a1b04d2",
                "quote": "the paragraph…", "body": "yay complete"
            }))
            .to_request();
        let res: serde_json::Value =
            actix_web::test::call_and_read_body_json(&app, req).await;
        let thread = res["thread"].as_i64().unwrap();

        // …a reply names the thread, anchor-free…
        let req = actix_web::test::TestRequest::post()
            .uri("/api/comments")
            .set_json(serde_json::json!({ "thread": thread, "body": "second thoughts" }))
            .to_request();
        let res = actix_web::test::call_service(&app, req).await;
        assert!(res.status().is_success());

        // …resolve round-trips, and an unknown thread 404s.
        for (uri, expect) in [
            (format!("/api/threads/{thread}/resolve"), 204u16),
            (format!("/api/threads/{thread}/unresolve"), 204),
            ("/api/threads/999/resolve".to_string(), 404),
        ] {
            let req = actix_web::test::TestRequest::post().uri(&uri).to_request();
            let res = actix_web::test::call_service(&app, req).await;
            assert_eq!(res.status().as_u16(), expect, "{uri}");
        }
        let store = state.store.lock().unwrap();
        assert_eq!(store.comments_for_page("v2").unwrap().len(), 2);
        assert!(store.threads_for_page("v2").unwrap()[0].resolved_at.is_none());
    }

    /// Session ids can contain `/` and `%` (cwd and tmux rungs). The printed
    /// URLs percent-encode them; this pins that the encoded form actually
    /// matches the single-segment page route.
    #[actix_web::test]
    async fn encoded_session_ids_match_the_page_route() {
        let app = actix_web::test::init_service(
            actix_web::App::new().route("/s/{session}", actix_web::web::get().to(page)),
        )
        .await;
        for uri in ["/s/cwd%3A%2Fhome%2Fdavid%2Fproj", "/s/tmux%2542"] {
            let req = actix_web::test::TestRequest::get().uri(uri).to_request();
            let res = actix_web::test::call_service(&app, req).await;
            assert!(res.status().is_success(), "{uri} did not match the page route");
        }
    }
}
