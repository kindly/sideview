//! The logic layer (V5.sv, thread 117). Every conversation operation an
//! interface performs passes through here — written once, whichever of the
//! daemon, the CLI or the monitor calls it. Interfaces parse, call, and
//! format for their transport; they import this module and nothing below it
//! (checkable from the use lines). Decisions about the models live in the
//! concept module (`conversation`), sequencing and guards live here. Some
//! functions are single-line pass-throughs — accepted deliberately: with
//! three callers, uniform depth beats a per-call judgment about which layer
//! to import (grill thread 117).

use anyhow::{bail, Result};

pub use crate::conversation::{
    is_attachment_path, Attachment, Comment, NewAttachment, Thread, ATTACHMENTS_DIR,
    ATTACHMENTS_PREFIX,
};
use crate::conversation as conv;
use crate::store::Store;

/// Where a new comment lands: a fresh thread on a block, or a reply.
pub enum CommentTarget<'a> {
    NewThread {
        page: &'a str,
        target: &'a str,
        anchor: &'a str,
        quote: Option<&'a str>,
        context: Option<&'a str>,
    },
    Reply {
        thread: i64,
    },
}

/// Post a comment — the operation that existed twice (cli.rs and daemon.rs,
/// with drift) until v5. Validates the body, the kind vocabulary, every
/// claimed attachment, and the page guard on replies; returns (thread id,
/// comment id).
///
/// `kind` accepts only 'comment' and 'edit': 'edited' is the splice
/// endpoint's receipt and is minted by `record_edited` alone — accepting it
/// here would let a request forge a "the file changed" record.
pub fn post_comment(
    store: &mut Store,
    target: CommentTarget,
    body: &str,
    author: Option<&str>,
    kind: &str,
    attachments: &[NewAttachment],
    page_guard: Option<&str>,
) -> Result<(i64, i64)> {
    if body.trim().is_empty() {
        bail!("empty comment body");
    }
    if !matches!(kind, "comment" | "edit") {
        bail!("kind {kind:?} is not postable");
    }
    // A row is a future deletion (page rm, gc), so verify each claimed
    // attachment is a real file in the attachments home before binding it.
    for a in attachments {
        if !is_attachment_path(&a.path) || !store.root.join(&a.path).is_file() {
            bail!("attachment {:?} is not an uploaded file", a.path);
        }
    }
    match target {
        CommentTarget::Reply { thread } => {
            // The guard: watch events hand the agent page+thread together, so
            // asserting the pair costs nothing and catches the cross-project
            // id collision the FK can't (same id existing in both stores).
            if let (Some(p), Some(t)) = (page_guard, conv::thread(store, thread)?) {
                if t.page != p {
                    bail!("thread {thread} is on page {:?}, not {p:?} — wrong project?", t.page);
                }
            }
            let id = conv::reply(store, thread, body, author, kind, attachments)?;
            Ok((thread, id))
        }
        CommentTarget::NewThread { page, target, anchor, quote, context } => conv::create_thread(
            store,
            page,
            target,
            anchor,
            quote,
            context,
            body,
            author,
            kind,
            attachments,
        ),
    }
}

/// The machine-mail record of a direct prose splice: how watch stays honest
/// about the file moving under the agent. Only the splice endpoint calls
/// this — kind='edited' is never postable through `post_comment`.
pub fn record_edited(store: &mut Store, page: &str, block: &str) -> Result<()> {
    conv::create_thread(
        store,
        page,
        block,
        "",
        Some(&format!("edit {block}")),
        None,
        "edited from the page",
        Some("user"),
        "edited",
        &[],
    )?;
    Ok(())
}

/// Resolve (or with undo, reopen) a thread, with the optional page guard.
/// `None`: no such thread. `Some((thread, changed))`: changed=false means it
/// already held the asked-for state — success from the page's point of view,
/// said plainly by the CLI.
pub fn resolve(
    store: &mut Store,
    thread: i64,
    by: Option<&str>,
    undo: bool,
    page_guard: Option<&str>,
) -> Result<Option<(Thread, bool)>> {
    let Some(t) = conv::thread(store, thread)? else {
        return Ok(None);
    };
    if let Some(p) = page_guard {
        if t.page != p {
            bail!("thread {thread} is on page {:?}, not {p:?} — wrong project?", t.page);
        }
    }
    let changed = conv::resolve_thread(store, thread, by, undo)?;
    Ok(Some((t, changed)))
}

/// Mark a thread as being worked on. Same contract as `resolve`:
/// changed=false means the thread is resolved and there is nothing to work on.
pub fn working(
    store: &mut Store,
    thread: i64,
    by: Option<&str>,
    page_guard: Option<&str>,
) -> Result<Option<(Thread, bool)>> {
    let Some(t) = conv::thread(store, thread)? else {
        return Ok(None);
    };
    if let Some(p) = page_guard {
        if t.page != p {
            bail!("thread {thread} is on page {:?}, not {p:?} — wrong project?", t.page);
        }
    }
    let changed = conv::set_working(store, thread, by)?;
    Ok(Some((t, changed)))
}

/// One page's conversation, serialized for the `threads` SSE event. Sent
/// whole on every change — page-scale, same reasoning as block replay.
/// Comments travel rendered (body_html) beside their source: the card shows
/// comrak's safe-mode markdown, agents keep reading raw bodies.
pub fn snapshot_json(store: &Store, page: &str) -> Result<String> {
    let comments: Vec<serde_json::Value> = conv::comments_for_page(store, page)?
        .into_iter()
        .map(|c| {
            let mut v = serde_json::to_value(&c).expect("comment serializes");
            v["body_html"] = serde_json::Value::String(crate::render::comment_body(&c.body));
            v
        })
        .collect();
    Ok(serde_json::json!({
        "page": page,
        "threads": conv::threads_for_page(store, page)?,
        "comments": comments,
        "attachments": conv::attachments_for_page(store, page)?,
    })
    .to_string())
}

// ---- watch: the delivery loop's decisions ---------------------------------------

/// What a watcher wants: authors to skip, and — both repeatable, both
/// optional — pages and categories to keep. Empty pages+categories means
/// everything (the project-wide watch that existed first).
#[derive(Default)]
pub struct WatchFilter {
    pub skip_author: Option<String>,
    pub pages: Vec<String>,
    pub categories: Vec<String>,
}

/// A watcher's position: comment cursor, known resolution states, and the
/// generation its last read saw. One delivery concept — every watcher sees
/// everything (that its filter keeps); `--ack` receipts what it emits.
pub struct WatchState {
    cursor: i64,
    resolutions: std::collections::HashMap<i64, Option<i64>>,
    generation: i64,
}

/// Watch starts at its invocation moment; `since` reaches back (comments
/// only — resolution is state, so only transitions from here on out).
pub fn watch_start(store: &Store, since: Option<i64>) -> Result<WatchState> {
    let cursor = match since {
        Some(id) => id,
        None => conv::max_comment_id(store)?,
    };
    let resolutions = conv::thread_resolutions(store)?
        .into_iter()
        .map(|(id, _, at, _)| (id, at))
        .collect();
    Ok(WatchState { cursor, resolutions, generation: -1 })
}

/// One poll tick: if the generation moved, emit every new event the filter
/// keeps, acking emitted comments when `ack_by` is set. The cursor advances
/// over filtered comments too — skipped is delivered-elsewhere, not pending.
pub fn watch_tick(
    store: &mut Store,
    st: &mut WatchState,
    filter: &WatchFilter,
    ack_by: Option<&str>,
) -> Result<Vec<serde_json::Value>> {
    let g = conv::generation(store)?;
    if g == st.generation {
        return Ok(Vec::new());
    }
    st.generation = g;
    let keep_page = page_filter(store, filter)?;
    let mut out = Vec::new();

    for (c, t) in conv::comments_after(store, st.cursor)? {
        st.cursor = st.cursor.max(c.id);
        if filter.skip_author.as_deref().is_some_and(|a| c.author.as_deref() == Some(a)) {
            continue;
        }
        if keep_page.as_ref().is_some_and(|keep| !keep.contains(&t.page)) {
            continue;
        }
        if let Some(by) = ack_by {
            conv::ack_comment(store, c.id, by)?;
        }
        out.push(conv::comment_event(store, &c, &t)?);
    }

    for (id, page, at, by) in conv::thread_resolutions(store)? {
        let known = st.resolutions.insert(id, at);
        let skip = filter.skip_author.as_deref().is_some_and(|a| by.as_deref() == Some(a))
            || keep_page.as_ref().is_some_and(|keep| !keep.contains(&page));
        let event = match (known, at) {
            _ if skip => None,
            (Some(None), Some(when)) => Some(serde_json::json!({
                "type": "resolve", "thread": id, "page": page,
                "by": by, "created_at": when,
            })),
            (Some(Some(_)), None) => Some(serde_json::json!({
                "type": "unresolve", "thread": id, "page": page,
                "created_at": crate::store::now_ms(),
            })),
            // New threads announce themselves through their first
            // comment; a state seen at baseline is not an event.
            _ => None,
        };
        if let Some(e) = event {
            out.push(e);
        }
    }
    Ok(out)
}

/// The pages a filter keeps: the ones named directly, plus every binding
/// whose category matches. None = no filter. Category is a page prop (the
/// homepage groups by it): read from the sv-page tag for composed pages,
/// from config for imported ones — recomputed per tick, so a page changing
/// category is picked up live.
fn page_filter(
    store: &Store,
    filter: &WatchFilter,
) -> Result<Option<std::collections::HashSet<String>>> {
    if filter.pages.is_empty() && filter.categories.is_empty() {
        return Ok(None);
    }
    let mut keep: std::collections::HashSet<String> = filter.pages.iter().cloned().collect();
    if !filter.categories.is_empty() {
        let cfg = crate::config::load(&store.root).0;
        for b in store.bindings()? {
            let category = if crate::config::is_imported(&b.path) {
                cfg.pages
                    .iter()
                    .find(|e| e.path == b.path)
                    .and_then(|e| e.category.clone())
            } else {
                std::fs::read_to_string(store.root.join(&b.path))
                    .ok()
                    .and_then(|src| crate::format::parse(&src).prop("category").map(str::to_string))
            };
            if category.is_some_and(|c| filter.categories.contains(&c)) {
                keep.insert(b.id);
            }
        }
    }
    Ok(Some(keep))
}

// ---- pass-throughs: the uniform depth --------------------------------------------

pub fn generation(store: &Store) -> Result<i64> {
    conv::generation(store)
}

pub fn max_comment_id(store: &Store) -> Result<i64> {
    conv::max_comment_id(store)
}

pub fn comments_after(store: &Store, cursor: i64) -> Result<Vec<(Comment, Thread)>> {
    conv::comments_after(store, cursor)
}

pub fn ack_comment(store: &mut Store, id: i64, by: &str) -> Result<()> {
    conv::ack_comment(store, id, by)
}

pub fn thread_resolutions(
    store: &Store,
) -> Result<Vec<(i64, String, Option<i64>, Option<String>)>> {
    conv::thread_resolutions(store)
}

pub fn pages_with_conversation(store: &Store) -> Result<Vec<String>> {
    conv::pages_with_conversation(store)
}

pub fn attachments_for_comment(store: &Store, comment_id: i64) -> Result<Vec<Attachment>> {
    conv::attachments_for_comment(store, comment_id)
}

pub fn attachment_refs(store: &Store) -> Result<Vec<(String, bool)>> {
    conv::attachment_refs(store)
}

pub fn delete_attachment_rows(store: &mut Store, path: &str) -> Result<usize> {
    conv::delete_attachment_rows(store, path)
}

pub fn unlink_attachment(store: &Store, rel: &str) {
    conv::unlink_attachment(store, rel)
}
