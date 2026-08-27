//! The share concept (V6.sv, step 3): capability tokens for funnel-origin
//! requests, and the roles they grant. Models fused with their SQL, per the
//! layering law; only the logic layer calls in here.
//!
//! Two kinds, settled in curation round 1: a page-scoped token is the guest
//! role on exactly that page; the page-NULL token is the owner's own-devices
//! link and carries everything the local browser can do. Requests not marked
//! funnel-origin never consult this table — loopback and the tailnet stay
//! open (round 1: security begins at the public web).

use anyhow::Result;
use rusqlite::{OptionalExtension, TransactionBehavior};

use crate::models::base::{now_ms, Store};

/// One minted link. `page = None` is the owner link.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Share {
    pub id: i64,
    pub token: String,
    pub page: Option<String>,
    pub created_at: i64,
    pub revoked_at: Option<i64>,
}

/// What a validated token grants. Owner is the author's own link — the full
/// surface; Guest is the conversation surface on one page, enforced
/// server-side by the daemon (never by hidden buttons).
#[derive(Debug, Clone, PartialEq)]
pub enum Role {
    Owner,
    Guest { page: String },
}

impl Share {
    pub fn role(&self) -> Role {
        match &self.page {
            None => Role::Owner,
            Some(p) => Role::Guest { page: p.clone() },
        }
    }
}

const COLS: &str = "id, token, page, created_at, revoked_at";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Share> {
    Ok(Share {
        id: r.get(0)?,
        token: r.get(1)?,
        page: r.get(2)?,
        created_at: r.get(3)?,
        revoked_at: r.get(4)?,
    })
}

/// Mint a token: 128 random bits, hex, unguessable — the capability itself.
/// One live owner link at a time is not enforced here; the CLI's surface
/// (step 4) decides whether minting again supersedes or coexists.
pub fn mint(store: &mut Store, page: Option<&str>) -> Result<Share> {
    let token = uuid::Uuid::new_v4().simple().to_string();
    let now = now_ms();
    let tx = store.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO shares(token, page, created_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![token, page, now],
    )?;
    let id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(Share { id, token, page: page.map(str::to_string), created_at: now, revoked_at: None })
}

/// The gate's question: does this token grant anything right now?
/// Revoked tokens answer no; the row itself stays, an audit fact.
pub fn lookup_live(store: &Store, token: &str) -> Result<Option<Share>> {
    store
        .conn
        .query_row(
            &format!("SELECT {COLS} FROM shares WHERE token = ?1 AND revoked_at IS NULL"),
            [token],
            row,
        )
        .optional()
        .map_err(Into::into)
}

/// Revoke one token. Returns whether it was live.
pub fn revoke(store: &mut Store, token: &str) -> Result<bool> {
    let n = store.conn.execute(
        "UPDATE shares SET revoked_at = ?2 WHERE token = ?1 AND revoked_at IS NULL",
        rusqlite::params![token, now_ms()],
    )?;
    Ok(n > 0)
}

/// Every share, live and revoked — the CLI's listing (step 4) and the
/// audit view.
pub fn list(store: &Store) -> Result<Vec<Share>> {
    let mut stmt = store
        .conn
        .prepare(&format!("SELECT {COLS} FROM shares ORDER BY created_at ASC, id ASC"))?;
    let rows = stmt.query_map([], row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
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
    fn tokens_grant_their_role_until_revoked_and_rows_outlive_revocation() {
        let mut store = test_store();
        let guest = mint(&mut store, Some("V6")).unwrap();
        let owner = mint(&mut store, None).unwrap();
        assert_ne!(guest.token, owner.token);
        assert_eq!(guest.token.len(), 32, "128 bits, hex, no hyphens");

        let g = lookup_live(&store, &guest.token).unwrap().unwrap();
        assert_eq!(g.role(), Role::Guest { page: "V6".into() });
        let o = lookup_live(&store, &owner.token).unwrap().unwrap();
        assert_eq!(o.role(), Role::Owner);
        assert!(lookup_live(&store, "not-a-token").unwrap().is_none());

        assert!(revoke(&mut store, &guest.token).unwrap());
        assert!(!revoke(&mut store, &guest.token).unwrap(), "already revoked: no-op");
        assert!(
            lookup_live(&store, &guest.token).unwrap().is_none(),
            "a revoked token grants nothing"
        );
        let all = list(&store).unwrap();
        assert_eq!(all.len(), 2, "revocation keeps the row — an audit fact");
        assert!(all[0].revoked_at.is_some() && all[1].revoked_at.is_none());
    }
}
