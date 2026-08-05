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
            "INSERT INTO sessions(id, path, cwd, detected_from, started_at, last_active_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(id) DO UPDATE SET last_active_at = ?5",
            rusqlite::params![id, path, cwd, detected_from, now],
        )?;
        Ok(())
    }

    pub fn bindings(&self) -> Result<Vec<Binding>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, last_active_at FROM sessions ORDER BY last_active_at DESC",
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
                "SELECT id, path, last_active_at FROM sessions WHERE id = ?1",
                [id],
                |r| Ok(Binding { id: r.get(0)?, path: r.get(1)?, last_active_at: r.get(2)? }),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Drop a binding. Returns whether one existed — deleting the file is the
    /// caller's job (the binding is bookkeeping; the file is the content).
    pub fn delete_binding(&self, id: &str) -> Result<bool> {
        let n = self.conn.execute("DELETE FROM sessions WHERE id = ?1", [id])?;
        Ok(n > 0)
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
    fn bindings_upsert_and_order_by_activity() {
        let store = test_store();
        store.bind_session("s1", ".sideview/pages/s1.sv", "/tmp", "test").unwrap();
        std::thread::sleep(Duration::from_millis(2));
        store.bind_session("s2", ".sideview/pages/s2.sv", "/tmp", "test").unwrap();
        std::thread::sleep(Duration::from_millis(2));
        store.bind_session("s1", "ignored-on-conflict.sv", "/tmp", "test").unwrap();
        let b = store.bindings().unwrap();
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].id, "s1", "most recently active first");
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
