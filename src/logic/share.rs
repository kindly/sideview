//! The logic layer's share operations (V6.sv step 3). Thin by design — the
//! uniform depth: interfaces import logic and nothing below it.

use anyhow::Result;

pub use crate::models::share::{Role, Share};
use crate::models::base::Store;
use crate::models::share as share;

pub fn mint(store: &mut Store, page: Option<&str>) -> Result<Share> {
    share::mint(store, page)
}

pub fn lookup_live(store: &Store, token: &str) -> Result<Option<Share>> {
    share::lookup_live(store, token)
}

pub fn revoke(store: &mut Store, token: &str) -> Result<bool> {
    share::revoke(store, token)
}

pub fn list(store: &Store) -> Result<Vec<Share>> {
    share::list(store)
}
