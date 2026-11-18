//! The `openDiff` calls waiting for the user, keyed by `(connection, tab_name)`.
//!
//! Only three things take an entry out: the app's answer, Claude's `close_tab` /
//! `closeAllDiffTabs` or a newer `openDiff` with the same `tab_name` (each of those answers the
//! waiting call with an error, never a decision), and the connection ending. Nothing here
//! produces a result frame: the server does, and only for an entry the app answered.

use std::collections::HashMap;

use serde_json::Value;

use crate::server::{ConnId, DiffId};

struct Entry {
    conn: ConnId,
    tab_name: String,
    rpc_id: Value,
}

/// A waiting `openDiff` that left the table without an answer from the app.
pub struct Withdrawn {
    pub id: DiffId,
    pub rpc_id: Value,
}

/// What the app's answer needs to build the result frame.
pub struct Answered {
    pub conn: ConnId,
    pub tab_name: String,
    pub rpc_id: Value,
}

#[derive(Default)]
pub struct Pending {
    by_id: HashMap<DiffId, Entry>,
    by_name: HashMap<(ConnId, String), DiffId>,
}

impl Pending {
    /// Register a new `openDiff`; the one it replaces (same connection and `tab_name`) is returned.
    pub fn insert(&mut self, id: DiffId, conn: ConnId, tab_name: String, rpc_id: Value) -> Option<Withdrawn> {
        let replaced = self.by_name.insert((conn, tab_name.clone()), id.clone()).and_then(|old| {
            self.by_id.remove(&old).map(|e| Withdrawn { id: old, rpc_id: e.rpc_id })
        });
        self.by_id.insert(id, Entry { conn, tab_name, rpc_id });
        replaced
    }

    /// The app answered `id`: the entry leaves the table once, so a second answer finds nothing.
    pub fn answer(&mut self, id: &DiffId) -> Option<Answered> {
        let e = self.by_id.remove(id)?;
        self.by_name.remove(&(e.conn, e.tab_name.clone()));
        Some(Answered { conn: e.conn, tab_name: e.tab_name, rpc_id: e.rpc_id })
    }

    /// `close_tab`: the call waiting under `tab_name`, if any.
    pub fn close_tab(&mut self, conn: ConnId, tab_name: &str) -> Option<Withdrawn> {
        let id = self.by_name.remove(&(conn, tab_name.to_string()))?;
        let e = self.by_id.remove(&id)?;
        Some(Withdrawn { id, rpc_id: e.rpc_id })
    }

    /// `closeAllDiffTabs`: every call of `conn` (and only `conn`).
    pub fn close_all(&mut self, conn: ConnId) -> Vec<Withdrawn> {
        let ids: Vec<DiffId> =
            self.by_id.iter().filter(|(_, e)| e.conn == conn).map(|(id, _)| id.clone()).collect();
        ids.into_iter()
            .filter_map(|id| {
                let e = self.by_id.remove(&id)?;
                self.by_name.remove(&(e.conn, e.tab_name));
                Some(Withdrawn { id, rpc_id: e.rpc_id })
            })
            .collect()
    }

    /// The connection ended: nothing is owed to anyone any more.
    pub fn drop_conn(&mut self, conn: ConnId) {
        self.by_id.retain(|_, e| e.conn != conn);
        self.by_name.retain(|(c, _), _| *c != conn);
    }
}
