//! The SQLite store, demoted by v1's pages-are-files to what only it can do:
//! the daemon row (liveness, supersession, the remembered port), durable meta,
//! and session→file **bindings** — the daemon's watch list. Content lives in
//! `.sv` files; the test that keeps this honest is that deleting the database
//! loses no content (V1.md).
//!
//! One `Store::open()` used by both the CLI and the daemon, since they are the
//! same binary. Migration is `PRAGMA user_version` stepping applied in-process
//! on open; nobody ever sees a `sideview migrate`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

pub const DIR_NAME: &str = ".sideview";
pub const DB_FILE: &str = "sideview.db";
pub const SPAWN_LOCK: &str = "spawn.lock";
/// Where throwaway session pages live, under `.sideview/`. Deliberately
/// gitignored by the store's own `.gitignore`; promotion is `mv` into the repo.
pub const PAGES_DIR: &str = "pages";

/// A daemon whose `last_seen` is older than this is doubted, and the doubt
/// path (ping/pong) decides.
pub const STALE_AFTER_MS: i64 = 10_000;

// The v0 schema (blocks as rows, props bags, a store-wide rev) was replaced
// wholesale on 2026-08-05 when pages became files — pre-release, no users, no
// migration, the author's store deleted by hand with his own sign-off. Real
// migration history starts at the first release.
const MIGRATIONS: &[&str] = &[
    // v1: bindings, the daemon row, durable meta. No content.
    r#"
    CREATE TABLE sessions(
        id             TEXT PRIMARY KEY,
        path           TEXT NOT NULL,     -- the page file, relative to the project root
        cwd            TEXT,
        detected_from  TEXT,
        started_at     INTEGER NOT NULL,
        last_active_at INTEGER NOT NULL
    );
    CREATE TABLE daemon(
        instance_id TEXT PRIMARY KEY,
        pid         INTEGER,
        port        INTEGER,
        version     TEXT,
        started_at  INTEGER,
        last_seen   INTEGER,
        ping        INTEGER NOT NULL DEFAULT 0,
        pong        INTEGER NOT NULL DEFAULT 0,
        netns       INTEGER,
        reachable   INTEGER
    );
    CREATE TABLE meta(
        key   TEXT PRIMARY KEY,        -- durable settings; holds the remembered
        value TEXT NOT NULL            -- port, which must outlive the daemon row
    );
    "#,
    // v2: pages are the noun (this row was always a binding), and the
    // feedback loop's conversation tables. Full rationale in V2.sv's models
    // block — notably: threads own placement, comments own utterances;
    // anchor is '' for the block's tail (NULLs compare distinct in SQL);
    // and there is deliberately no UNIQUE(page, target, anchor), not even
    // partial on open threads — threads succeed each other at an anchor,
    // and an unresolve must never fail on an index.
    r#"
    ALTER TABLE sessions RENAME TO bindings;

    CREATE TABLE threads(
        id          INTEGER PRIMARY KEY,
        page        TEXT NOT NULL,     -- binding id
        target      TEXT NOT NULL,     -- block id ("b7")
        anchor      TEXT NOT NULL DEFAULT '',
        quote       TEXT,              -- the text commented on, captured at thread
        context     TEXT,              --   creation; what re-resolution matches against
        created_at  INTEGER NOT NULL,
        resolved_at INTEGER,           -- NULL = open; resolve is undoable, never delete
        resolved_by TEXT
    );
    CREATE INDEX threads_by_page ON threads(page, resolved_at);

    CREATE TABLE comments(
        id         INTEGER PRIMARY KEY,
        thread_id  INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
        body       TEXT NOT NULL,
        author     TEXT,               -- NULL locally; tailnet identity fills it at T2
        created_at INTEGER NOT NULL,
        seen_at    INTEGER,            -- claim marker, not a read marker
        seen_by    TEXT                -- which watcher claimed it
    );
    CREATE INDEX comments_by_thread ON comments(thread_id, created_at);

    CREATE TABLE outlines(
        page       TEXT PRIMARY KEY,
        spec       TEXT NOT NULL,      -- agent-supplied JSON, used verbatim by the rail
        updated_at INTEGER NOT NULL
    );
    "#,
];

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before 1970")
        .as_millis() as i64
}

/// Where `.sideview/` lives: walk up from `cwd` for an existing one, like
/// `.git`. If none is found, the nearest enclosing repository root — asking
/// git for `--git-common-dir` so a worktree resolves to the *main* checkout's
/// store, not its own. Failing that, `cwd` itself.
pub fn find_store_dir(cwd: &Path) -> PathBuf {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        let candidate = d.join(DIR_NAME);
        if candidate.is_dir() {
            return candidate;
        }
        dir = d.parent();
    }
    if let Some(common) = git_common_dir(cwd) {
        if let Some(root) = common.parent() {
            return root.join(DIR_NAME);
        }
    }
    cwd.join(DIR_NAME)
}

fn git_common_dir(cwd: &Path) -> Option<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8(out.stdout).ok()?;
    let path = PathBuf::from(path.trim());
    path.is_dir().then_some(path)
}

#[derive(Debug, Clone)]
pub struct DaemonRow {
    pub instance_id: String,
    pub pid: i64,
    pub port: u16,
    pub version: String,
    pub started_at: i64,
    pub last_seen: i64,
    #[allow(dead_code)] // mirrors the column; read by nothing yet
    pub ping: i64,
    #[allow(dead_code)]
    pub pong: i64,
    pub netns: Option<u64>,
    pub reachable: bool,
}

/// A session→file binding. Losing one costs nothing durable (the file is the
/// content); it exists so the daemon knows where to look without globbing the
/// project four times a second.
#[derive(Debug, Clone)]
pub struct Binding {
    pub id: String,
    /// Relative to the project root.
    pub path: String,
    pub last_active_at: i64,
}

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
}

/// One utterance within a thread.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Comment {
    pub id: i64,
    pub thread_id: i64,
    pub body: String,
    pub author: Option<String>,
    pub created_at: i64,
    pub seen_at: Option<i64>,
    pub seen_by: Option<String>,
}

const THREAD_COLS: &str = "threads.id, threads.page, threads.target, threads.anchor, \
     threads.quote, threads.context, threads.created_at, threads.resolved_at, threads.resolved_by";
const COMMENT_COLS: &str = "comments.id, comments.thread_id, comments.body, comments.author, \
     comments.created_at, comments.seen_at, comments.seen_by";

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
    })
}

fn comment_row(r: &rusqlite::Row) -> rusqlite::Result<Comment> {
    Ok(Comment {
        id: r.get(0)?,
        thread_id: r.get(1)?,
        body: r.get(2)?,
        author: r.get(3)?,
        created_at: r.get(4)?,
        seen_at: r.get(5)?,
        seen_by: r.get(6)?,
    })
}

pub struct Store {
    pub conn: Connection,
    /// The `.sideview/` directory itself.
    pub dir: PathBuf,
    /// The project root the daemon serves files from: `dir`'s parent.
    pub root: PathBuf,
}

impl Store {
    /// Open (creating if needed) the store for `dir`, and migrate. Prints the
    /// path the first time one is created, so nobody discovers a stray
    /// `.sideview/` weeks later and wonders what made it.
    pub fn open(dir: &Path) -> Result<Store> {
        let created = !dir.exists();
        std::fs::create_dir_all(dir)
            .with_context(|| format!("creating {}", dir.display()))?;
        let gitignore = dir.join(".gitignore");
        if !gitignore.exists() {
            std::fs::write(&gitignore, "*\n")?; // the directory ignores itself
        }
        if created {
            eprintln!("created {}", dir.display());
        }

        let mut conn = Connection::open(dir.join(DB_FILE))?;
        conn.pragma_update(None, "journal_mode", "WAL")?; // persists; harmless to re-set
        // Per-connection, off by default in SQLite: without it the comments
        // table's ON DELETE CASCADE is decoration.
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        migrate(&mut conn)?;

        let root = dir
            .parent()
            .context("store directory has no parent")?
            .to_path_buf();
        Ok(Store { conn, root, dir: dir.to_path_buf() })
    }

    pub fn db_path(&self) -> PathBuf {
        self.dir.join(DB_FILE)
    }

    /// The absolute path of the throwaway pages directory, created on demand.
    pub fn pages_dir(&self) -> Result<PathBuf> {
        let dir = self.dir.join(PAGES_DIR);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    // ---- bindings ----------------------------------------------------------

    /// Bind (or re-touch) a session to its page file. The path never changes
    /// through this — a moved page is re-bound explicitly, not by drift.
    pub fn bind_session(
        &self,
        id: &str,
        path: &str,
        cwd: &str,
        detected_from: &str,
    ) -> Result<()> {
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO bindings(id, path, cwd, detected_from, started_at, last_active_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(id) DO UPDATE SET last_active_at = ?5",
            rusqlite::params![id, path, cwd, detected_from, now],
        )?;
        Ok(())
    }

    pub fn bindings(&self) -> Result<Vec<Binding>> {
        // Creation order, oldest first: chips get stable positions and new
        // pages append on the right, like browser tabs. Which session the
        // page auto-shows is a separate question the client answers from
        // last_active_at — ordering stopped doing double duty after the
        // author watched chips reorder themselves mid-session.
        let mut stmt = self.conn.prepare(
            "SELECT id, path, last_active_at FROM bindings ORDER BY started_at ASC, id ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Binding { id: r.get(0)?, path: r.get(1)?, last_active_at: r.get(2)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn binding(&self, id: &str) -> Result<Option<Binding>> {
        self.conn
            .query_row(
                "SELECT id, path, last_active_at FROM bindings WHERE id = ?1",
                [id],
                |r| Ok(Binding { id: r.get(0)?, path: r.get(1)?, last_active_at: r.get(2)? }),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Drop a binding and its conversation — threads cascade their comments,
    /// and `page rm` is the one place conversation is destroyed on purpose.
    /// Returns whether a binding existed; deleting the file is the caller's
    /// job (the binding is bookkeeping; the file is the content).
    pub fn delete_binding(&mut self, id: &str) -> Result<bool> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("DELETE FROM threads WHERE page = ?1", [id])?;
        tx.execute("DELETE FROM outlines WHERE page = ?1", [id])?;
        let n = tx.execute("DELETE FROM bindings WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(n > 0)
    }

    /// Move a binding's page file path — `page promote` is the only caller;
    /// paths otherwise never change through re-binding.
    pub fn rebind_path(&self, id: &str, path: &str) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE bindings SET path = ?2, last_active_at = ?3 WHERE id = ?1",
            rusqlite::params![id, path, now_ms()],
        )?;
        Ok(n > 0)
    }

    // ---- conversation: threads and comments --------------------------------
    // Threads own placement (page, target, anchor, quote/context, resolution);
    // comments own utterances. Multi-writer and bursty — exactly what the db
    // is for. Serialized shapes double as the watch event and snapshot JSON.

    /// Start a thread with its first comment, atomically. An anchor-form
    /// comment always creates a fresh thread — replies address a thread id,
    /// which is what lets threads succeed each other at an anchor.
    pub fn create_thread(
        &mut self,
        page: &str,
        target: &str,
        anchor: &str,
        quote: Option<&str>,
        context: Option<&str>,
        body: &str,
        author: Option<&str>,
    ) -> Result<(i64, i64)> {
        let now = now_ms();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO threads(page, target, anchor, quote, context, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![page, target, anchor, quote, context, now],
        )?;
        let thread_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO comments(thread_id, body, author, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![thread_id, body, author, now],
        )?;
        let comment_id = tx.last_insert_rowid();
        tx.commit()?;
        Ok((thread_id, comment_id))
    }

    /// Add a comment to an existing thread. The foreign key makes a reply to
    /// a nonexistent thread an error, not a silent orphan row.
    pub fn reply(&self, thread_id: i64, body: &str, author: Option<&str>) -> Result<i64> {
        self.conn
            .execute(
                "INSERT INTO comments(thread_id, body, author, created_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![thread_id, body, author, now_ms()],
            )
            .with_context(|| format!("no thread {thread_id}?"))?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Resolve (or with `undo`, reopen) a thread. Undoable by design — never
    /// a delete. Returns whether the thread existed in the opposite state.
    pub fn resolve_thread(&self, id: i64, by: Option<&str>, undo: bool) -> Result<bool> {
        let n = if undo {
            self.conn.execute(
                "UPDATE threads SET resolved_at = NULL, resolved_by = NULL
                 WHERE id = ?1 AND resolved_at IS NOT NULL",
                [id],
            )?
        } else {
            self.conn.execute(
                "UPDATE threads SET resolved_at = ?2, resolved_by = ?3
                 WHERE id = ?1 AND resolved_at IS NULL",
                rusqlite::params![id, now_ms(), by],
            )?
        };
        Ok(n > 0)
    }

    pub fn thread(&self, id: i64) -> Result<Option<Thread>> {
        self.conn
            .query_row(
                &format!("SELECT {THREAD_COLS} FROM threads WHERE id = ?1"),
                [id],
                thread_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn threads_for_page(&self, page: &str) -> Result<Vec<Thread>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {THREAD_COLS} FROM threads WHERE page = ?1 ORDER BY created_at ASC, id ASC"
        ))?;
        let rows = stmt.query_map([page], thread_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn comments_for_page(&self, page: &str) -> Result<Vec<Comment>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COMMENT_COLS} FROM comments
             JOIN threads ON threads.id = comments.thread_id
             WHERE threads.page = ?1 ORDER BY comments.created_at ASC, comments.id ASC"
        ))?;
        let rows = stmt.query_map([page], comment_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ---- the watch queries: cursors, claims, and resolve transitions -------

    /// `PRAGMA data_version` — bumps when *another* connection commits, which
    /// is exactly a watcher's wake-up condition.
    pub fn data_version(&self) -> Result<i64> {
        self.conn
            .pragma_query_value(None, "data_version", |r| r.get(0))
            .map_err(Into::into)
    }

    pub fn max_comment_id(&self) -> Result<i64> {
        self.conn
            .query_row("SELECT COALESCE(MAX(id), 0) FROM comments", [], |r| r.get(0))
            .map_err(Into::into)
    }

    /// Comments after the cursor, each with its thread — the watch event
    /// carries placement so consumers never need a second query.
    pub fn comments_after(&self, cursor: i64) -> Result<Vec<(Comment, Thread)>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COMMENT_COLS}, {THREAD_COLS} FROM comments
             JOIN threads ON threads.id = comments.thread_id
             WHERE comments.id > ?1 ORDER BY comments.id ASC"
        ))?;
        let rows = stmt
            .query_map([cursor], |r| Ok((comment_row(r)?, thread_row_at(r, 7)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// The supersession pattern: claim if unclaimed. Zero rows means another
    /// watcher got there first — skip the event, exactly-once holds.
    pub fn claim_comment(&self, id: i64, by: &str) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE comments SET seen_at = ?2, seen_by = ?3 WHERE id = ?1 AND seen_at IS NULL",
            rusqlite::params![id, now_ms(), by],
        )?;
        Ok(n > 0)
    }

    /// Pages that have any conversation at all — the daemon's snapshot set.
    pub fn conversation_pages(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT page FROM threads")?;
        let rows = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Every thread's resolution state — a watcher diffs successive readings
    /// to turn stored state into resolve/unresolve events.
    pub fn thread_resolutions(&self) -> Result<Vec<(i64, String, Option<i64>, Option<String>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, page, resolved_at, resolved_by FROM threads ORDER BY id ASC")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ---- explicit outlines --------------------------------------------------
    // The agent's ordered list, used verbatim by the rail when present
    // (inference off). Coordination, not content — so it lives here.

    pub fn set_outline(&self, page: &str, spec: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO outlines(page, spec, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(page) DO UPDATE SET spec = excluded.spec, updated_at = excluded.updated_at",
            rusqlite::params![page, spec, now_ms()],
        )?;
        Ok(())
    }

    pub fn clear_outline(&self, page: &str) -> Result<bool> {
        let n = self.conn.execute("DELETE FROM outlines WHERE page = ?1", [page])?;
        Ok(n > 0)
    }

    pub fn outlines(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT page, spec FROM outlines")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ---- meta --------------------------------------------------------------

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .map_err(Into::into)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    // ---- the daemon row ----------------------------------------------------

    pub fn daemon(&self) -> Result<Option<DaemonRow>> {
        self.conn
            .query_row(
                "SELECT instance_id, pid, port, version, started_at, last_seen, ping, pong, netns, reachable
                 FROM daemon LIMIT 1",
                [],
                |r| {
                    Ok(DaemonRow {
                        instance_id: r.get(0)?,
                        pid: r.get(1)?,
                        port: r.get::<_, i64>(2)? as u16,
                        version: r.get(3)?,
                        started_at: r.get(4)?,
                        last_seen: r.get(5)?,
                        ping: r.get(6)?,
                        pong: r.get(7)?,
                        netns: r.get::<_, Option<i64>>(8)?.map(|n| n as u64),
                        reachable: r.get::<_, i64>(9)? != 0,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    /// Claim the daemon row. Called *after* binding, never before — binding is
    /// the step that can fail, and claim-then-bind would evict a healthy
    /// daemon on behalf of one that never started. The previous holder
    /// discovers the claim on its next heartbeat and evicts itself.
    pub fn claim_daemon(&mut self, row: &DaemonRow) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("DELETE FROM daemon", [])?;
        tx.execute(
            "INSERT INTO daemon(instance_id, pid, port, version, started_at, last_seen, ping, pong, netns, reachable)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0, ?7, ?8)",
            rusqlite::params![
                row.instance_id,
                row.pid,
                row.port,
                row.version,
                row.started_at,
                row.last_seen,
                row.netns.map(|n| n as i64),
                row.reachable
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The supersession rule, verbatim: refresh `last_seen` as the holder.
    /// Zero rows affected means somebody else claimed the row — stop serving.
    pub fn heartbeat(&self, instance_id: &str) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE daemon SET last_seen = ?1 WHERE instance_id = ?2",
            rusqlite::params![now_ms(), instance_id],
        )?;
        Ok(n == 1)
    }

    /// Ack a ping — only ever as the current holder, never unconditionally.
    pub fn answer_ping(&self, instance_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE daemon SET pong = ping WHERE instance_id = ?1 AND pong != ping",
            [instance_id],
        )?;
        Ok(())
    }

    /// Clean shutdown clears the row; a crash leaves it, which is exactly what
    /// the timestamp and the ping are for.
    pub fn clear_daemon(&self, instance_id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM daemon WHERE instance_id = ?1", [instance_id])?;
        Ok(())
    }

    /// The doubt path: bump `ping`, wait up to ~500ms for `pong` to match.
    /// Definitive rather than probabilistic — an unrelated process that
    /// inherited the port cannot forge a `pong` in our database.
    pub fn ping_daemon(&self) -> Result<bool> {
        let ping: Option<i64> = self
            .conn
            .query_row(
                "UPDATE daemon SET ping = ping + 1 RETURNING ping",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let Some(ping) = ping else { return Ok(false) };
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(50));
            let pong: Option<i64> = self
                .conn
                .query_row("SELECT pong FROM daemon", [], |r| r.get(0))
                .optional()?;
            if pong.map_or(false, |p| p >= ping) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The fast path: a live-looking row. Fresh `last_seen` means carry on and
    /// say nothing; stale means run the doubt path.
    pub fn daemon_alive(&self) -> Result<Option<DaemonRow>> {
        let Some(row) = self.daemon()? else { return Ok(None) };
        if now_ms() - row.last_seen < STALE_AFTER_MS {
            return Ok(Some(row));
        }
        if self.ping_daemon()? {
            return Ok(self.daemon()?);
        }
        Ok(None)
    }
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let latest = MIGRATIONS.len() as i64;
    if version > latest {
        bail!(
            "this store is schema v{version}, but this sideview only understands up to v{latest} — \
             upgrade sideview rather than proceeding"
        );
    }
    if version == latest {
        return Ok(());
    }
    // Copy the file before migrating an existing store — bindings and the
    // remembered port are cheap, but the copy is cheaper.
    if version > 0 {
        if let Some(path) = conn.path().map(str::to_string) {
            // Fold the WAL into the main file first, or the copy misses
            // everything not yet checkpointed.
            conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())).ok();
            let backup = format!("{path}-pre-v{latest}");
            if let Err(e) = std::fs::copy(&path, &backup) {
                eprintln!("note: could not back up store before migrating: {e}");
            } else {
                eprintln!("backed up store to {backup}");
            }
        }
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        tx.execute_batch(sql)
            .with_context(|| format!("applying schema migration {}", i + 1))?;
    }
    tx.pragma_update(None, "user_version", latest)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        let dir = std::env::temp_dir().join(format!("sideview-test-{}", uuid::Uuid::new_v4()));
        Store::open(&dir.join(DIR_NAME)).unwrap()
    }

    #[test]
    fn bindings_keep_stable_creation_order_despite_activity() {
        let store = test_store();
        store.bind_session("s1", ".sideview/pages/s1.sv", "/tmp", "test").unwrap();
        std::thread::sleep(Duration::from_millis(2));
        store.bind_session("s2", ".sideview/pages/s2.sv", "/tmp", "test").unwrap();
        std::thread::sleep(Duration::from_millis(2));
        // Re-touching s1 makes it the most active — its chip must not move.
        store.bind_session("s1", "ignored-on-conflict.sv", "/tmp", "test").unwrap();
        let b = store.bindings().unwrap();
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].id, "s1", "creation order, oldest first — stable positions");
        assert_eq!(b[1].id, "s2");
        assert!(b[0].last_active_at > b[1].last_active_at, "activity is data, not order");
        assert_eq!(b[0].path, ".sideview/pages/s1.sv", "re-touch never moves the file");
    }

    #[test]
    fn remembered_port_survives_clean_shutdown() {
        let store = test_store();
        store.set_meta("port", "33753").unwrap();
        store.clear_daemon("whoever").unwrap(); // clean shutdown clears the row…
        assert_eq!(store.meta("port").unwrap().as_deref(), Some("33753")); // …not the port
        store.set_meta("port", "34947").unwrap();
        assert_eq!(store.meta("port").unwrap().as_deref(), Some("34947"));
    }

    #[test]
    fn threads_carry_placement_and_comments_are_utterances() {
        let mut store = test_store();
        let (t1, c1) = store
            .create_thread("v2", "b3", "p:3f9c2a1b04d2", Some("the paragraph…"), None, "yay complete", None)
            .unwrap();
        let c2 = store.reply(t1, "second thoughts", None).unwrap();
        assert!(c2 > c1);
        let threads = store.threads_for_page("v2").unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].quote.as_deref(), Some("the paragraph…"));
        let comments = store.comments_for_page("v2").unwrap();
        assert_eq!(comments.len(), 2, "replies join their thread, not a new one");
        assert!(store.reply(999, "into the void", None).is_err(), "FK: no orphan utterances");
    }

    #[test]
    fn threads_succeed_each_other_at_an_anchor_and_resolve_is_undoable() {
        let mut store = test_store();
        let (t1, _) = store.create_thread("v2", "b3", "", None, None, "first concern", None).unwrap();
        assert!(store.resolve_thread(t1, None, false).unwrap());
        assert!(!store.resolve_thread(t1, None, false).unwrap(), "already resolved: no-op");
        // A fresh thread at the same spot — no uniqueness in the way…
        let (t2, _) = store.create_thread("v2", "b3", "", None, None, "new concern", None).unwrap();
        assert_ne!(t1, t2);
        // …and unresolving the first can never fail on an index.
        assert!(store.resolve_thread(t1, None, true).unwrap());
        let open: Vec<_> = store
            .threads_for_page("v2")
            .unwrap()
            .into_iter()
            .filter(|t| t.resolved_at.is_none())
            .collect();
        assert_eq!(open.len(), 2);
    }

    #[test]
    fn claims_are_exactly_once_and_page_rm_cascades_conversation() {
        let mut store = test_store();
        store.bind_session("v2", "V2.sv", "/tmp", "test").unwrap();
        let (_, c1) = store.create_thread("v2", "b1", "", None, None, "hello", None).unwrap();
        assert!(store.claim_comment(c1, "watch:1").unwrap());
        assert!(!store.claim_comment(c1, "watch:2").unwrap(), "second watcher sees zero rows");
        assert_eq!(store.comments_after(0).unwrap().len(), 1);
        assert!(store.delete_binding("v2").unwrap());
        assert!(store.threads_for_page("v2").unwrap().is_empty(), "threads die with the page");
        assert_eq!(store.comments_after(0).unwrap().len(), 0, "comments cascade via threads");
    }

    #[test]
    fn a_v1_store_migrates_to_v2_with_bindings_intact() {
        let dir = std::env::temp_dir().join(format!("sideview-mig-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join(DB_FILE);
        {
            let mut conn = Connection::open(&db).unwrap();
            let tx = conn.transaction().unwrap();
            tx.execute_batch(MIGRATIONS[0]).unwrap();
            tx.execute(
                "INSERT INTO sessions(id, path, cwd, detected_from, started_at, last_active_at)
                 VALUES ('s1', 'p.sv', '/tmp', 'test', 1, 1)",
                [],
            )
            .unwrap();
            tx.pragma_update(None, "user_version", 1).unwrap();
            tx.commit().unwrap();
        }
        let store = Store::open(&dir).unwrap();
        let b = store.bindings().unwrap();
        assert_eq!(b.len(), 1, "the rename kept the rows");
        assert_eq!(b[0].id, "s1");
        assert!(store.threads_for_page("any").unwrap().is_empty(), "v2 tables exist");
    }

    #[test]
    fn supersession_zero_rows_means_evicted() {
        let mut store = test_store();
        let row = DaemonRow {
            instance_id: "old".into(),
            pid: 1,
            port: 1234,
            version: "0.0.1".into(),
            started_at: now_ms(),
            last_seen: now_ms(),
            ping: 0,
            pong: 0,
            netns: None,
            reachable: true,
        };
        store.claim_daemon(&row).unwrap();
        assert!(store.heartbeat("old").unwrap());
        let newer = DaemonRow { instance_id: "new".into(), ..row };
        store.claim_daemon(&newer).unwrap();
        assert!(!store.heartbeat("old").unwrap(), "superseded holder must see zero rows");
        assert!(store.heartbeat("new").unwrap());
    }
}
