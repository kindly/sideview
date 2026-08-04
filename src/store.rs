//! The SQLite store: the only mandatory channel between agent and daemon.
//!
//! One `Store::open()` used by both the CLI and the daemon, since they are the
//! same binary. Migration is `PRAGMA user_version` stepping applied in-process
//! on open; nobody ever sees a `sideview migrate`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

use crate::spec::{self, Spec};

pub const DIR_NAME: &str = ".sideview";
pub const DB_FILE: &str = "sideview.db";
pub const SPAWN_LOCK: &str = "spawn.lock";

/// A daemon whose `last_seen` is older than this is doubted, and the doubt
/// path (ping/pong) decides.
pub const STALE_AFTER_MS: i64 = 10_000;

// Squashed to a single step 2026-08-04, before the first commit, per the
// pre-release rule in V0.md — the interim steps (meta table, an `outline`
// column immediately reshaped into `props`) had no store in existence worth
// migrating. Real migration history starts at the first release.
const MIGRATIONS: &[&str] = &[
    // v1: the v0 schema, verbatim from V0.md.
    r#"
    CREATE TABLE sessions(
        id             TEXT PRIMARY KEY,
        cwd            TEXT,
        detected_from  TEXT,
        props          TEXT NOT NULL DEFAULT '{}',  -- presentation bag: label, outline, …
        next_block_seq INTEGER NOT NULL DEFAULT 1,
        started_at     INTEGER NOT NULL,
        last_active_at INTEGER NOT NULL
    );
    CREATE TABLE blocks(
        id         INTEGER PRIMARY KEY,
        session_id TEXT NOT NULL REFERENCES sessions(id),
        short_id   TEXT NOT NULL,
        ord        TEXT NOT NULL,
        type       TEXT NOT NULL,
        version    INTEGER NOT NULL,
        spec_json  TEXT NOT NULL,
        rev        INTEGER NOT NULL,
        deleted_at INTEGER,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        UNIQUE(session_id, short_id)
    );
    CREATE INDEX blocks_by_rev ON blocks(rev);
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

#[derive(Debug, Clone)]
pub struct BlockRow {
    pub session_id: String,
    pub short_id: String,
    pub ord: String,
    #[allow(dead_code)] // mirrors the column; the renderer re-derives type from spec_json
    pub type_name: String,
    pub spec_json: String,
    pub rev: i64,
    pub deleted: bool,
}

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: String,
    pub last_active_at: i64,
    /// The presentation-preference bag (`label`, `outline`, …). Always a JSON
    /// object. Mutated only via json_set/json_remove in SQL — never a typed
    /// Rust round-trip, which would drop keys a newer binary wrote.
    pub props: serde_json::Value,
}

impl SessionRow {
    pub fn prop(&self, key: &str) -> Option<&str> {
        self.props.get(key).and_then(|v| v.as_str())
    }
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

    // ---- sessions ----------------------------------------------------------

    pub fn touch_session(&self, id: &str, cwd: &str, detected_from: &str) -> Result<()> {
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO sessions(id, cwd, detected_from, next_block_seq, started_at, last_active_at)
             VALUES (?1, ?2, ?3, 1, ?4, ?4)
             ON CONFLICT(id) DO UPDATE SET last_active_at = ?4",
            rusqlite::params![id, cwd, detected_from, now],
        )?;
        Ok(())
    }

    pub fn sessions(&self) -> Result<Vec<SessionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, last_active_at, props FROM sessions ORDER BY last_active_at DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                let props: String = r.get(2)?;
                Ok(SessionRow {
                    id: r.get(0)?,
                    last_active_at: r.get(1)?,
                    props: serde_json::from_str(&props)
                        .unwrap_or_else(|_| serde_json::json!({})),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Set (or with None, remove) one presentation preference. Mutation stays
    /// inside SQLite so keys this binary doesn't know survive untouched; the
    /// caller (the CLI, the bag's only writer) validates keys and values, and
    /// only ever passes fixed key names — the '$.' path is not user input.
    pub fn set_session_prop(&self, id: &str, key: &str, value: Option<&str>) -> Result<()> {
        match value {
            Some(v) => self.conn.execute(
                "UPDATE sessions SET props = json_set(props, '$.' || ?2, ?3) WHERE id = ?1",
                rusqlite::params![id, key, v],
            )?,
            None => self.conn.execute(
                "UPDATE sessions SET props = json_remove(props, '$.' || ?2) WHERE id = ?1",
                rusqlite::params![id, key],
            )?,
        };
        Ok(())
    }

    // ---- blocks ------------------------------------------------------------

    /// Insert a block and return its per-session short id (`b7`). The seq
    /// counter, the store-wide `rev` bump and the insert happen in one
    /// immediate transaction so concurrent writers serialise.
    pub fn insert_block(&mut self, session_id: &str, spec: &Spec) -> Result<String> {
        let spec_json = spec::encode(spec, None)?;
        let now = now_ms();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let seq: i64 = tx.query_row(
            "UPDATE sessions SET next_block_seq = next_block_seq + 1, last_active_at = ?2
             WHERE id = ?1 RETURNING next_block_seq - 1",
            rusqlite::params![session_id, now],
            |r| r.get(0),
        )?;
        let short_id = format!("b{seq}");
        // Lexicographic rank. v0 only appends, so a zero-padded counter is
        // enough; fractional ranks arrive with insert-between, without
        // renumbering anything that exists.
        let ord = format!("a{seq:012}");
        let rev = next_rev(&tx)?;
        tx.execute(
            "INSERT INTO blocks(session_id, short_id, ord, type, version, spec_json, rev, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            rusqlite::params![
                session_id,
                short_id,
                ord,
                spec.type_name(),
                spec::SPEC_VERSION,
                spec_json,
                rev,
                now
            ],
        )?;
        tx.commit()?;
        Ok(short_id)
    }

    /// Replace a block's content in place. Short ids resolve within the
    /// caller's own session, which is what makes two agents both holding a
    /// `b7` harmless — an agent cannot name another session's block at all.
    pub fn update_block(&mut self, session_id: &str, short_id: &str, spec: &Spec) -> Result<()> {
        let spec_json = spec::encode(spec, None)?;
        let now = now_ms();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let rev = next_rev(&tx)?;
        let n = tx.execute(
            "UPDATE blocks SET spec_json = ?1, type = ?2, version = ?3, rev = ?4, updated_at = ?5
             WHERE session_id = ?6 AND short_id = ?7 AND deleted_at IS NULL",
            rusqlite::params![
                spec_json,
                spec.type_name(),
                spec::SPEC_VERSION,
                rev,
                now,
                session_id,
                short_id
            ],
        )?;
        tx.commit()?;
        if n == 0 {
            bail!("no block {short_id} in this session");
        }
        Ok(())
    }

    /// A tombstone, not a DELETE — a client reconnecting with `Last-Event-ID`
    /// has to learn the block went away.
    pub fn rm_block(&mut self, session_id: &str, short_id: &str) -> Result<()> {
        let now = now_ms();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let rev = next_rev(&tx)?;
        let n = tx.execute(
            "UPDATE blocks SET deleted_at = ?1, rev = ?2, updated_at = ?1
             WHERE session_id = ?3 AND short_id = ?4 AND deleted_at IS NULL",
            rusqlite::params![now, rev, session_id, short_id],
        )?;
        tx.commit()?;
        if n == 0 {
            bail!("no block {short_id} in this session");
        }
        Ok(())
    }

    /// Blocks changed since `rev`, in rev order — the `Last-Event-ID` replay
    /// query, tombstones included.
    pub fn blocks_since(&self, rev: i64) -> Result<Vec<BlockRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, short_id, ord, type, spec_json, rev, deleted_at IS NOT NULL
             FROM blocks WHERE rev > ?1 ORDER BY rev",
        )?;
        let rows = stmt
            .query_map([rev], |r| {
                Ok(BlockRow {
                    session_id: r.get(0)?,
                    short_id: r.get(1)?,
                    ord: r.get(2)?,
                    type_name: r.get(3)?,
                    spec_json: r.get(4)?,
                    rev: r.get(5)?,
                    deleted: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn max_rev(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COALESCE(MAX(rev), 0) FROM blocks", [], |r| r.get(0))?)
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

fn next_rev(tx: &rusqlite::Transaction) -> Result<i64> {
    Ok(tx.query_row("SELECT COALESCE(MAX(rev), 0) + 1 FROM blocks", [], |r| r.get(0))?)
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
    // Copy the file before migrating an existing store — the difference
    // between an annoyance and a lost plan when a step is non-additive.
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
    fn short_ids_count_per_session_and_collide_harmlessly() {
        let mut store = test_store();
        store.touch_session("s1", "/tmp", "test").unwrap();
        store.touch_session("s2", "/tmp", "test").unwrap();
        let a = store
            .insert_block("s1", &Spec::Prose { text: "one".into() })
            .unwrap();
        let b = store
            .insert_block("s2", &Spec::Prose { text: "two".into() })
            .unwrap();
        assert_eq!(a, "b1");
        assert_eq!(b, "b1"); // same short id, different sessions
        store
            .update_block("s1", "b1", &Spec::Prose { text: "one, revised".into() })
            .unwrap();
        // s2's b1 is untouched
        let rows = store.blocks_since(0).unwrap();
        let s2_block = rows.iter().find(|r| r.session_id == "s2").unwrap();
        assert!(s2_block.spec_json.contains("two"));
    }

    #[test]
    fn rm_is_a_tombstone_and_rev_is_monotonic() {
        let mut store = test_store();
        store.touch_session("s", "/tmp", "test").unwrap();
        let id = store
            .insert_block("s", &Spec::Prose { text: "x".into() })
            .unwrap();
        let rev_before = store.max_rev().unwrap();
        store.rm_block("s", &id).unwrap();
        let rows = store.blocks_since(rev_before).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].deleted);
        assert!(rows[0].rev > rev_before);
    }

    #[test]
    fn session_props_are_a_bag_that_preserves_unknown_keys() {
        let store = test_store();
        store.touch_session("s", "/tmp", "test").unwrap();
        store.set_session_prop("s", "label", Some("My plan")).unwrap();
        store.set_session_prop("s", "outline", Some("off")).unwrap();
        // A key some future binary wrote must survive this binary's writes.
        store.set_session_prop("s", "theme", Some("solarized")).unwrap();
        store.set_session_prop("s", "outline", None).unwrap(); // back to auto = absent
        let s = &store.sessions().unwrap()[0];
        assert_eq!(s.prop("label"), Some("My plan"));
        assert_eq!(s.prop("outline"), None);
        assert_eq!(s.prop("theme"), Some("solarized"), "unknown keys survive");
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
