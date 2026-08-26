//! The conversation concept: threads, comments and the attachments that ride
//! them — models fused with their algorithms and their SQL (the layering law,
//! V5.sv thread 117). Only the ops layer calls in here; the daemon, the CLI
//! and the monitor never import this module directly.
//!
//! Threads own placement (page, target, anchor, quote/context, resolution);
//! comments own utterances. Multi-writer and bursty — exactly what the db is
//! for. Serialized shapes double as the watch event and snapshot JSON. Every
//! mutation bumps the generation counter inside its own transaction — see
//! `generation` for why that atomicity is the contract.

use anyhow::{Context as _, Result};
use rusqlite::{OptionalExtension, TransactionBehavior};

use crate::models::base::{now_ms, Store};

/// One conversation's placement: where it hangs, what it quoted at creation,
/// whether someone has resolved it. Serialized shape doubles as the daemon
/// snapshot and the watch event payload.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Thread {
    pub id: i64,
    pub page: String,
    pub target: String,
    /// '' = the block's tail. See V2.sv's anchor grammar.
    pub anchor: String,
    pub quote: Option<String>,
    pub context: Option<String>,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
    pub resolved_by: Option<String>,
    /// The agent's explicit "on it, will take a while" — cleared by its
    /// next reply or a resolve.
    pub working_at: Option<i64>,
    pub working_by: Option<String>,
}

/// One utterance within a thread.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Comment {
    pub id: i64,
    pub thread_id: i64,
    pub body: String,
    /// The role: 'user' | 'agent' (NULL in old rows and fixtures).
    pub author: Option<String>,
    /// The commenter's self-declared display name (migration v6) — beside
    /// the role, never instead of it. NULL is pre-v6 behavior.
    pub author_name: Option<String>,
    pub created_at: i64,
    pub seen_at: Option<i64>,
    pub seen_by: Option<String>,
    /// 'comment' | 'edit' (a proposed change awaiting the agent's merge) |
    /// 'edited' (the record of a direct prose splice). See migration v5.
    pub kind: String,
}

/// A file riding a comment: metadata here, bytes on disk (V3.sv). The
/// serialized shape doubles as the watch-event and snapshot JSON.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Attachment {
    pub id: i64,
    pub comment_id: i64,
    pub path: String,
    pub name: String,
    pub mime: String,
    pub bytes: i64,
    pub sha256: String,
    pub created_at: i64,
}

/// What the upload endpoint hands back and the comment send binds: the row
/// minus the identities the insert assigns.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct NewAttachment {
    pub path: String,
    pub name: String,
    pub mime: String,
    pub bytes: i64,
    pub sha256: String,
}

/// The confinement check every consumer of a stored attachment path shares:
/// gc's writ, page rm's unlink and the comment binding all refuse anything
/// outside `.sideview/attachments/` — a row naming `src/main.rs` must never
/// become a deletion of `src/main.rs`.
pub fn is_attachment_path(rel: &str) -> bool {
    rel.starts_with(ATTACHMENTS_PREFIX)
        && !rel.split('/').any(|seg| seg == ".." || seg.is_empty())
}
pub const ATTACHMENTS_DIR: &str = "attachments";
pub const ATTACHMENTS_PREFIX: &str = ".sideview/attachments/";

const GEN_KEY: &str = "conversation_gen";

/// The bump half of the atomicity contract: only ever called inside the
/// mutation's own transaction.
pub(crate) fn bump_gen(tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO meta(key, value) VALUES (?1, '1')
         ON CONFLICT(key) DO UPDATE SET value = CAST(value AS INTEGER) + 1",
        [GEN_KEY],
    )?;
    Ok(())
}

const THREAD_COLS: &str = "threads.id, threads.page, threads.target, threads.anchor, \
     threads.quote, threads.context, threads.created_at, threads.resolved_at, threads.resolved_by, \
     threads.working_at, threads.working_by";
const COMMENT_COLS: &str = "comments.id, comments.thread_id, comments.body, comments.author, \
     comments.author_name, comments.created_at, comments.seen_at, comments.seen_by, comments.kind";

fn thread_row(r: &rusqlite::Row) -> rusqlite::Result<Thread> {
    thread_row_at(r, 0)
}

fn thread_row_at(r: &rusqlite::Row, base: usize) -> rusqlite::Result<Thread> {
    Ok(Thread {
        id: r.get(base)?,
        page: r.get(base + 1)?,
        target: r.get(base + 2)?,
        anchor: r.get(base + 3)?,
        quote: r.get(base + 4)?,
        context: r.get(base + 5)?,
        created_at: r.get(base + 6)?,
        resolved_at: r.get(base + 7)?,
        resolved_by: r.get(base + 8)?,
        working_at: r.get(base + 9)?,
        working_by: r.get(base + 10)?,
    })
}

fn comment_row(r: &rusqlite::Row) -> rusqlite::Result<Comment> {
    Ok(Comment {
        id: r.get(0)?,
        thread_id: r.get(1)?,
        body: r.get(2)?,
        author: r.get(3)?,
        author_name: r.get(4)?,
        created_at: r.get(5)?,
        seen_at: r.get(6)?,
        seen_by: r.get(7)?,
        kind: r.get(8)?,
    })
}

const ATTACHMENT_COLS: &str = "attachments.id, attachments.comment_id, attachments.path, \
     attachments.name, attachments.mime, attachments.bytes, attachments.sha256, attachments.created_at";

fn attachment_row(r: &rusqlite::Row) -> rusqlite::Result<Attachment> {
    Ok(Attachment {
        id: r.get(0)?,
        comment_id: r.get(1)?,
        path: r.get(2)?,
        name: r.get(3)?,
        mime: r.get(4)?,
        bytes: r.get(5)?,
        sha256: r.get(6)?,
        created_at: r.get(7)?,
    })
}

/// Rows are born with the comment, inside its transaction — never at upload.
/// The confinement check here is load-bearing: a stored path later becomes a
/// deletion (page rm, gc), so nothing outside the attachments home may ever
/// be recorded as one.
fn insert_attachments(
    tx: &rusqlite::Transaction,
    comment_id: i64,
    items: &[NewAttachment],
    now: i64,
) -> Result<()> {
    for a in items {
        if !is_attachment_path(&a.path) {
            anyhow::bail!("attachment path {:?} is outside {}", a.path, ATTACHMENTS_PREFIX);
        }
        tx.execute(
            "INSERT INTO attachments(comment_id, path, name, mime, bytes, sha256, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![comment_id, a.path, a.name, a.mime, a.bytes, a.sha256, now],
        )?;
    }
    Ok(())
}

/// Start a thread with its first comment, atomically. An anchor-form
/// comment always creates a fresh thread — replies address a thread id,
/// which is what lets threads succeed each other at an anchor.
#[allow(clippy::too_many_arguments)]
pub fn create_thread(
    store: &mut Store,
    page: &str,
    target: &str,
    anchor: &str,
    quote: Option<&str>,
    context: Option<&str>,
    body: &str,
    author: Option<&str>,
    author_name: Option<&str>,
    kind: &str,
    attachments: &[NewAttachment],
) -> Result<(i64, i64)> {
    let now = now_ms();
    let tx = store.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO threads(page, target, anchor, quote, context, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![page, target, anchor, quote, context, now],
    )?;
    let thread_id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO comments(thread_id, body, author, author_name, created_at, kind)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![thread_id, body, author, author_name, now, kind],
    )?;
    let comment_id = tx.last_insert_rowid();
    insert_attachments(&tx, comment_id, attachments, now)?;
    bump_gen(&tx)?;
    tx.commit()?;
    Ok((thread_id, comment_id))
}

/// Add a comment to an existing thread. The foreign key makes a reply to
/// a nonexistent thread an error, not a silent orphan row.
pub fn reply(
    store: &mut Store,
    thread_id: i64,
    body: &str,
    author: Option<&str>,
    author_name: Option<&str>,
    kind: &str,
    attachments: &[NewAttachment],
) -> Result<i64> {
    let now = now_ms();
    let tx = store.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO comments(thread_id, body, author, author_name, created_at, kind)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![thread_id, body, author, author_name, now, kind],
    )
    .with_context(|| format!("no thread {thread_id}?"))?;
    let id = tx.last_insert_rowid();
    insert_attachments(&tx, id, attachments, now)?;
    // An agent's reply is the work arriving: the working marker retires.
    if author == Some("agent") {
        tx.execute("UPDATE threads SET working_at = NULL, working_by = NULL WHERE id = ?1", [thread_id])?;
    }
    bump_gen(&tx)?;
    tx.commit()?;
    Ok(id)
}

/// Resolve (or with `undo`, reopen) a thread. Undoable by design — never
/// a delete. Returns whether the thread existed in the opposite state.
pub fn resolve_thread(store: &mut Store, id: i64, by: Option<&str>, undo: bool) -> Result<bool> {
    let tx = store.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let n = if undo {
        tx.execute(
            "UPDATE threads SET resolved_at = NULL, resolved_by = NULL
             WHERE id = ?1 AND resolved_at IS NOT NULL",
            [id],
        )?
    } else {
        tx.execute(
            "UPDATE threads SET resolved_at = ?2, resolved_by = ?3
             WHERE id = ?1 AND resolved_at IS NULL",
            rusqlite::params![id, now_ms(), by],
        )?
    };
    if n > 0 {
        tx.execute("UPDATE threads SET working_at = NULL, working_by = NULL WHERE id = ?1", [id])?;
        bump_gen(&tx)?;
    }
    tx.commit()?;
    Ok(n > 0)
}

/// The agent's "this will take a while" — between the plumbing's sent
/// receipt and the eventual reply, the bar shows working.
pub fn set_working(store: &mut Store, id: i64, by: Option<&str>) -> Result<bool> {
    let tx = store.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let n = tx.execute(
        "UPDATE threads SET working_at = ?2, working_by = ?3 WHERE id = ?1 AND resolved_at IS NULL",
        rusqlite::params![id, now_ms(), by],
    )?;
    if n > 0 {
        bump_gen(&tx)?;
    }
    tx.commit()?;
    Ok(n > 0)
}

pub fn thread(store: &Store, id: i64) -> Result<Option<Thread>> {
    store
        .conn
        .query_row(
            &format!("SELECT {THREAD_COLS} FROM threads WHERE id = ?1"),
            [id],
            thread_row,
        )
        .optional()
        .map_err(Into::into)
}

pub fn threads_for_page(store: &Store, page: &str) -> Result<Vec<Thread>> {
    let mut stmt = store.conn.prepare(&format!(
        "SELECT {THREAD_COLS} FROM threads WHERE page = ?1 ORDER BY created_at ASC, id ASC"
    ))?;
    let rows = stmt.query_map([page], thread_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn comments_for_page(store: &Store, page: &str) -> Result<Vec<Comment>> {
    let mut stmt = store.conn.prepare(&format!(
        "SELECT {COMMENT_COLS} FROM comments
         JOIN threads ON threads.id = comments.thread_id
         WHERE threads.page = ?1 ORDER BY comments.created_at ASC, comments.id ASC"
    ))?;
    let rows = stmt.query_map([page], comment_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// All attachments riding a page's conversation — the snapshot's third
/// list, keyed client-side by comment_id.
pub fn attachments_for_page(store: &Store, page: &str) -> Result<Vec<Attachment>> {
    let mut stmt = store.conn.prepare(&format!(
        "SELECT {ATTACHMENT_COLS} FROM attachments
         JOIN comments ON comments.id = attachments.comment_id
         JOIN threads ON threads.id = comments.thread_id
         WHERE threads.page = ?1 ORDER BY attachments.id ASC"
    ))?;
    let rows = stmt.query_map([page], attachment_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn attachments_for_comment(store: &Store, comment_id: i64) -> Result<Vec<Attachment>> {
    let mut stmt = store.conn.prepare(&format!(
        "SELECT {ATTACHMENT_COLS} FROM attachments WHERE comment_id = ?1 ORDER BY id ASC"
    ))?;
    let rows =
        stmt.query_map([comment_id], attachment_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// gc's view: every referenced path, and whether *any* reference hangs
/// off an open thread. A path referenced only by resolved threads is
/// what --resolved widens to.
pub fn attachment_refs(store: &Store) -> Result<Vec<(String, bool)>> {
    let mut stmt = store.conn.prepare(
        "SELECT attachments.path,
                MAX(CASE WHEN threads.resolved_at IS NULL THEN 1 ELSE 0 END)
         FROM attachments
         JOIN comments ON comments.id = attachments.comment_id
         JOIN threads ON threads.id = comments.thread_id
         GROUP BY attachments.path",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? > 0)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Drop every attachment row on this path (the --resolved half of gc:
/// the file goes, so the rows must not dangle). Bumps the counter — open
/// bars lose the thumbnails live.
pub fn delete_attachment_rows(store: &mut Store, path: &str) -> Result<usize> {
    let tx = store.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let n = tx.execute("DELETE FROM attachments WHERE path = ?1", [path])?;
    if n > 0 {
        bump_gen(&tx)?;
    }
    tx.commit()?;
    Ok(n)
}

/// Does any remaining row reference this attachment path?
pub fn attachment_path_in_use(store: &Store, path: &str) -> Result<bool> {
    let n: i64 = store.conn.query_row(
        "SELECT COUNT(*) FROM attachments WHERE path = ?1",
        [path],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// Unlink an attachment file and prune its hash directory if emptied.
/// Confinement first — this is the only place conversation deletes a file
/// it was merely *told* about. Best-effort: a missing file is not an
/// error (gc backstops the other direction).
pub fn unlink_attachment(store: &Store, rel: &str) {
    if !is_attachment_path(rel) {
        return;
    }
    let abs = store.root.join(rel);
    let _ = std::fs::remove_file(&abs);
    if let Some(dir) = abs.parent() {
        let _ = std::fs::remove_dir(dir); // only succeeds when empty
    }
}

/// The page-delete cascade's conversation half: collect the attachment paths
/// the cascade will orphan, then drop the page's threads (comments and
/// attachment rows cascade). Runs inside the orchestrator's transaction —
/// `delete_binding` owns it, per the layering law's cross-concept rule.
pub fn delete_page_conversation(tx: &rusqlite::Transaction, page: &str) -> Result<()> {
    tx.execute("DELETE FROM threads WHERE page = ?1", [page])?;
    Ok(())
}

/// Attachment paths referenced by one page's conversation — what the delete
/// cascade must check for orphaning afterwards.
pub fn attachment_paths_for_page(store: &Store, page: &str) -> Result<Vec<String>> {
    let mut stmt = store.conn.prepare(
        "SELECT DISTINCT attachments.path FROM attachments
         JOIN comments ON comments.id = attachments.comment_id
         JOIN threads ON threads.id = comments.thread_id
         WHERE threads.page = ?1",
    )?;
    let rows = stmt.query_map([page], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---- the watch queries: cursors and resolve transitions ------------------------

/// The conversation generation counter: bumped **in the same transaction
/// as every conversation write**, which is the whole point — "gen moved"
/// and "the data is visible" are one atomic fact, the guarantee PRAGMA
/// data_version failed to give across processes under WAL (caught live,
/// 2026-08-08). Watchers poll this one row; a queue table of undelivered
/// items was considered and rejected — watchers are readers of one shared
/// history, not consumers of per-watcher queues.
pub fn generation(store: &Store) -> Result<i64> {
    Ok(store
        .meta(GEN_KEY)?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0))
}

pub fn max_comment_id(store: &Store) -> Result<i64> {
    store
        .conn
        .query_row("SELECT COALESCE(MAX(id), 0) FROM comments", [], |r| r.get(0))
        .map_err(Into::into)
}

/// Comments after the cursor, each with its thread — the watch event
/// carries placement so consumers never need a second query.
pub fn comments_after(store: &Store, cursor: i64) -> Result<Vec<(Comment, Thread)>> {
    let mut stmt = store.conn.prepare(&format!(
        "SELECT {COMMENT_COLS}, {THREAD_COLS} FROM comments
         JOIN threads ON threads.id = comments.thread_id
         WHERE comments.id > ?1 ORDER BY comments.id ASC"
    ))?;
    let rows = stmt
        .query_map([cursor], |r| Ok((comment_row(r)?, thread_row_at(r, 9)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The delivery receipt (watch --ack): stamp seen_at as the event flows
/// through the pipe — receipt, not cognition, so it costs the agent
/// nothing and the page can show "seen" in the silence before a reply.
/// It never suppresses emission, and it bumps the counter so the stamp
/// reaches open bars live.
pub fn ack_comment(store: &mut Store, id: i64, by: &str) -> Result<()> {
    let tx = store.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let n = tx.execute(
        "UPDATE comments SET seen_at = ?2, seen_by = ?3 WHERE id = ?1 AND seen_at IS NULL",
        rusqlite::params![id, now_ms(), by],
    )?;
    if n > 0 {
        bump_gen(&tx)?;
    }
    tx.commit()?;
    Ok(())
}

/// Pages that have any conversation at all — the daemon's snapshot set.
pub fn pages_with_conversation(store: &Store) -> Result<Vec<String>> {
    let mut stmt = store.conn.prepare("SELECT DISTINCT page FROM threads")?;
    let rows = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Every thread's resolution state — a watcher diffs successive readings
/// to turn stored state into resolve/unresolve events.
pub fn thread_resolutions(store: &Store) -> Result<Vec<(i64, String, Option<i64>, Option<String>)>> {
    let mut stmt = store
        .conn
        .prepare("SELECT id, page, resolved_at, resolved_by FROM threads ORDER BY id ASC")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The watch event for one comment: the line shape agents parse. Attachment
/// rows travel whole (path, name, mime, bytes) so an agent can decide whether
/// to pull a 2KB csv or leave a 200MB parquet unread without touching either.
pub fn comment_event(store: &Store, c: &Comment, t: &Thread) -> Result<serde_json::Value> {
    Ok(serde_json::json!({
        "type": "comment",
        // 'comment' | 'edit' (a proposed change to merge) |
        // 'edited' (a prose block was spliced from the page —
        // re-read the file before your next update/rm there).
        "kind": c.kind,
        "id": c.id,
        "thread": t.id,
        "page": t.page,
        "target": t.target,
        "anchor": t.anchor,
        "quote": t.quote,
        "body": c.body,
        "author": c.author,
        "author_name": c.author_name,
        "created_at": c.created_at,
        "attachments": attachments_for_comment(store, c.id)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::base::DIR_NAME;

    fn test_store() -> Store {
        let dir = std::env::temp_dir().join(format!("sideview-test-{}", uuid::Uuid::new_v4()));
        Store::open(&dir.join(DIR_NAME)).unwrap()
    }

    #[test]
    fn threads_carry_placement_and_comments_are_utterances() {
        let mut store = test_store();
        let (t1, c1) = create_thread(&mut store, "v2", "b3", "p:3f9c2a1b04d2", Some("the paragraph…"), None, "yay complete", None, None, "comment", &[])
            .unwrap();
        let c2 = reply(&mut store, t1, "second thoughts", None, None, "comment", &[]).unwrap();
        assert!(c2 > c1);
        let threads = threads_for_page(&store, "v2").unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].quote.as_deref(), Some("the paragraph…"));
        let comments = comments_for_page(&store, "v2").unwrap();
        assert_eq!(comments.len(), 2, "replies join their thread, not a new one");
        assert!(reply(&mut store, 999, "into the void", None, None, "comment", &[]).is_err(), "FK: no orphan utterances");
    }

    #[test]
    fn names_ride_comments_into_events_and_absence_is_pre_v6_behavior() {
        let mut store = test_store();
        let (t, _) = create_thread(
            &mut store, "v6", "b1", "", None, None, "hello", Some("user"), Some("Priya"),
            "comment", &[],
        )
        .unwrap();
        reply(&mut store, t, "and back", Some("agent"), None, "comment", &[]).unwrap();
        let cs = comments_for_page(&store, "v6").unwrap();
        assert_eq!(cs[0].author_name.as_deref(), Some("Priya"));
        assert_eq!(cs[1].author_name, None, "nameless is exactly the old shape");
        // The watch event line agents parse carries the name beside the role.
        let (c, th) = &comments_after(&store, 0).unwrap()[0];
        let ev = comment_event(&store, c, th).unwrap();
        assert_eq!(ev["author"], "user");
        assert_eq!(ev["author_name"], "Priya");
    }

    #[test]
    fn threads_succeed_each_other_at_an_anchor_and_resolve_is_undoable() {
        let mut store = test_store();
        let (t1, _) = create_thread(&mut store, "v2", "b3", "", None, None, "first concern", None, None, "comment", &[]).unwrap();
        assert!(resolve_thread(&mut store, t1, None, false).unwrap());
        assert!(!resolve_thread(&mut store, t1, None, false).unwrap(), "already resolved: no-op");
        // A fresh thread at the same spot — no uniqueness in the way…
        let (t2, _) = create_thread(&mut store, "v2", "b3", "", None, None, "new concern", None, None, "comment", &[]).unwrap();
        assert_ne!(t1, t2);
        // …and unresolving the first can never fail on an index.
        assert!(resolve_thread(&mut store, t1, None, true).unwrap());
        let open: Vec<_> = threads_for_page(&store, "v2")
            .unwrap()
            .into_iter()
            .filter(|t| t.resolved_at.is_none())
            .collect();
        assert_eq!(open.len(), 2);
    }

    #[test]
    fn attachments_ride_comments_and_files_die_with_their_last_reference() {
        let mut store = test_store();
        store.bind_page("v3", "V3.sv", "/tmp", "test").unwrap();
        store.bind_page("other", "O.sv", "/tmp", "test").unwrap();
        let rel = format!("{ATTACHMENTS_PREFIX}aaaaaaaa/shot.png");
        let abs = store.root.join(&rel);
        std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
        std::fs::write(&abs, b"png-bytes").unwrap();
        let a = NewAttachment {
            path: rel.clone(),
            name: "shot.png".into(),
            mime: "image/png".into(),
            bytes: 9,
            sha256: "aa".into(),
        };
        let (_, c1) =
            create_thread(&mut store, "v3", "b1", "", None, None, "see", None, None, "comment", &[a.clone()]).unwrap();
        assert_eq!(attachments_for_comment(&store, c1).unwrap().len(), 1);
        assert_eq!(attachments_for_page(&store, "v3").unwrap().len(), 1);

        // Deduped file shared with another page's conversation: the first
        // page's death must not take bytes a remaining row still protects.
        create_thread(&mut store, "other", "b1", "", None, None, "also", None, None, "comment", &[a.clone()]).unwrap();
        store.delete_binding("v3").unwrap();
        assert!(abs.exists(), "a remaining row protects its bytes");
        store.delete_binding("other").unwrap();
        assert!(!abs.exists(), "last reference gone: the file goes with the conversation");

        // Confinement: a row is a future deletion, so nothing outside the
        // attachments home may ever be recorded as one.
        let evil = NewAttachment { path: "src/main.rs".into(), ..a };
        assert!(create_thread(&mut store, "v3", "b1", "", None, None, "x", None, None, "comment", &[evil]).is_err());
    }

    #[test]
    fn attachment_paths_are_confined_and_unlink_refuses_to_leave_home() {
        assert!(is_attachment_path(".sideview/attachments/ab12cd34/x.png"));
        assert!(!is_attachment_path(".sideview/attachments/../../etc/passwd"));
        assert!(!is_attachment_path("src/main.rs"));
        assert!(!is_attachment_path("/etc/passwd"));
        let store = test_store();
        let outside = store.root.join("keep.txt");
        std::fs::write(&outside, "canon").unwrap();
        unlink_attachment(&store, "keep.txt");
        assert!(outside.exists(), "unlink never leaves .sideview/attachments/");
    }

    #[test]
    fn attachment_refs_tell_open_holders_from_resolved_only() {
        let mut store = test_store();
        let rel = format!("{ATTACHMENTS_PREFIX}bbbbbbbb/data.csv");
        let a = NewAttachment {
            path: rel.clone(),
            name: "data.csv".into(),
            mime: "text/csv".into(),
            bytes: 4,
            sha256: "bb".into(),
        };
        let (t1, _) =
            create_thread(&mut store, "v3", "b1", "", None, None, "csv", None, None, "comment", &[a.clone()]).unwrap();
        resolve_thread(&mut store, t1, None, false).unwrap();
        assert_eq!(
            attachment_refs(&store).unwrap(),
            vec![(rel.clone(), false)],
            "held only by a resolved thread — what --resolved widens to"
        );
        create_thread(&mut store, "v3", "b2", "", None, None, "again", None, None, "comment", &[a]).unwrap();
        assert_eq!(
            attachment_refs(&store).unwrap(),
            vec![(rel.clone(), true)],
            "any open holder protects the path outright"
        );
        assert_eq!(delete_attachment_rows(&mut store, &rel).unwrap(), 2);
        assert!(attachment_refs(&store).unwrap().is_empty());
    }

    #[test]
    fn the_gen_counter_moves_with_every_mutation_and_only_real_ones() {
        let mut store = test_store();
        let mut last = generation(&store).unwrap();
        let mut step = |store: &Store, what: &str, expect_move: bool| {
            let g = generation(store).unwrap();
            if expect_move {
                assert!(g > last, "gen must move after {what}");
            } else {
                assert_eq!(g, last, "gen must NOT move after {what}");
            }
            last = g;
        };
        let (t, _) = create_thread(&mut store, "v2", "b1", "", None, None, "hi", None, None, "comment", &[]).unwrap();
        step(&store, "create_thread", true);
        reply(&mut store, t, "again", None, None, "comment", &[]).unwrap();
        step(&store, "reply", true);
        resolve_thread(&mut store, t, None, false).unwrap();
        step(&store, "resolve", true);
        resolve_thread(&mut store, t, None, false).unwrap();
        step(&store, "no-op resolve", false); // watchers don't wake for nothing
        resolve_thread(&mut store, t, None, true).unwrap();
        step(&store, "unresolve", true);
        store.set_outline("v2", "[]").unwrap();
        step(&store, "set_outline", true);
        store.bind_page("v2", "V2.sv", "/tmp", "test").unwrap();
        store.delete_binding("v2").unwrap();
        step(&store, "page rm cascade", true);
    }

    #[test]
    fn page_rm_cascades_conversation() {
        let mut store = test_store();
        store.bind_page("v2", "V2.sv", "/tmp", "test").unwrap();
        create_thread(&mut store, "v2", "b1", "", None, None, "hello", None, None, "comment", &[]).unwrap();
        assert_eq!(comments_after(&store, 0).unwrap().len(), 1);
        assert!(store.delete_binding("v2").unwrap());
        assert!(threads_for_page(&store, "v2").unwrap().is_empty(), "threads die with the page");
        assert_eq!(comments_after(&store, 0).unwrap().len(), 0, "comments cascade via threads");
    }
}
