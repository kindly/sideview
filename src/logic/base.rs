//! Logic for the base domains — bindings, outlines, and the daemon liveness
//! row (V5.sv, round 3). Thin by design: with these domains the operation
//! *is* the model call, so each wrapper is one line — accepted deliberately
//! so the import law stays total (interfaces import `logic` and nothing
//! below it, no per-domain exceptions). Holding the `Store` handle itself —
//! `open`, `.root`, `.dir`, paths, meta — stays direct: it is the argument
//! everything passes, not a decision.

use anyhow::Result;

pub use crate::models::base::{Binding, DaemonRow};
use crate::models::base::Store;

// ---- bindings -------------------------------------------------------------------

pub fn bind_page(store: &Store, id: &str, path: &str, cwd: &str, detected_from: &str) -> Result<()> {
    store.bind_page(id, path, cwd, detected_from)
}

pub fn bindings(store: &Store) -> Result<Vec<Binding>> {
    store.bindings()
}

pub fn binding(store: &Store, id: &str) -> Result<Option<Binding>> {
    store.binding(id)
}

pub fn delete_binding(store: &mut Store, id: &str) -> Result<bool> {
    store.delete_binding(id)
}

pub fn rebind_path(store: &Store, id: &str, path: &str) -> Result<bool> {
    store.rebind_path(id, path)
}

// ---- outlines -------------------------------------------------------------------

pub fn set_outline(store: &mut Store, page: &str, spec: &str) -> Result<()> {
    store.set_outline(page, spec)
}

pub fn clear_outline(store: &mut Store, page: &str) -> Result<bool> {
    store.clear_outline(page)
}

pub fn outlines(store: &Store) -> Result<Vec<(String, String)>> {
    store.outlines()
}

// ---- the daemon row: liveness, supersession, ping -------------------------------

pub fn daemon(store: &Store) -> Result<Option<DaemonRow>> {
    store.daemon()
}

pub fn daemon_alive(store: &Store) -> Result<Option<DaemonRow>> {
    store.daemon_alive()
}

pub fn claim_daemon(store: &mut Store, row: &DaemonRow) -> Result<()> {
    store.claim_daemon(row)
}

pub fn heartbeat(store: &Store, instance_id: &str) -> Result<bool> {
    store.heartbeat(instance_id)
}

pub fn clear_daemon(store: &Store, instance_id: &str) -> Result<()> {
    store.clear_daemon(instance_id)
}

pub fn ping_daemon(store: &Store) -> Result<bool> {
    store.ping_daemon()
}

pub fn answer_ping(store: &Store, instance_id: &str) -> Result<()> {
    store.answer_ping(instance_id)
}
