//! Pipan's own SQLite store (`<app_local_data_dir>/pipan.db`, WAL mode).
//!
//! Schema is versioned through `PRAGMA user_version`; each migration runs once, in
//! order, inside a transaction. Later phases append migrations rather than editing
//! earlier ones.
//!
//! Version 1 (node graph): `sessions` (just enough to own a graph direction; the
//! persistent-sessions phase extends it) and `node_positions`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tauri::State;

const MIGRATIONS: &[&str] = &[
    // v1: node graph persistence.
    "CREATE TABLE sessions (
        id               TEXT PRIMARY KEY,
        graph_direction  TEXT CHECK (graph_direction IN ('TB', 'LR')),
        created_at       INTEGER NOT NULL DEFAULT (unixepoch()),
        updated_at       INTEGER NOT NULL DEFAULT (unixepoch())
    );
    CREATE TABLE node_positions (
        session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
        direction   TEXT NOT NULL CHECK (direction IN ('TB', 'LR')),
        node_id     TEXT NOT NULL,
        x           REAL NOT NULL,
        y           REAL NOT NULL,
        pinned      INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (session_id, direction, node_id)
    );",
];

/// A node coordinate as stored per `(session, direction)`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
    #[serde(default = "default_pinned")]
    pub pinned: bool,
}

fn default_pinned() -> bool {
    true
}

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
        Self::init(conn, true)
    }

    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
        Self::init(conn, false)
    }

    fn init(mut conn: Connection, wal: bool) -> Result<Self, String> {
        if wal {
            conn.pragma_update(None, "journal_mode", "WAL")
                .map_err(|e| e.to_string())?;
        }
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| e.to_string())?;
        migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        // A poisoned lock only means another command panicked mid-query; SQLite itself
        // stays consistent, so keep serving requests.
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn graph_direction(&self, session_id: &str) -> Result<Option<String>, String> {
        let conn = self.lock();
        let value: Option<Option<String>> = conn
            .query_row(
                "SELECT graph_direction FROM sessions WHERE id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(value.flatten())
    }

    pub fn set_graph_direction(&self, session_id: &str, direction: &str) -> Result<(), String> {
        check_direction(direction)?;
        let conn = self.lock();
        conn.execute(
            "INSERT INTO sessions (id, graph_direction) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET graph_direction = ?2, updated_at = unixepoch()",
            params![session_id, direction],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn load_node_positions(
        &self,
        session_id: &str,
        direction: &str,
    ) -> Result<HashMap<String, NodePosition>, String> {
        check_direction(direction)?;
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT node_id, x, y, pinned FROM node_positions
                 WHERE session_id = ?1 AND direction = ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![session_id, direction], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    NodePosition {
                        x: row.get(1)?,
                        y: row.get(2)?,
                        pinned: row.get::<_, i64>(3)? != 0,
                    },
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<HashMap<_, _>, _>>()
            .map_err(|e| e.to_string())
    }

    /// Upserts the given positions; nodes not mentioned keep what they had.
    pub fn save_node_positions(
        &self,
        session_id: &str,
        direction: &str,
        positions: &HashMap<String, NodePosition>,
    ) -> Result<(), String> {
        check_direction(direction)?;
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT OR IGNORE INTO sessions (id) VALUES (?1)",
            params![session_id],
        )
        .map_err(|e| e.to_string())?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO node_positions (session_id, direction, node_id, x, y, pinned)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(session_id, direction, node_id)
                     DO UPDATE SET x = ?4, y = ?5, pinned = ?6",
                )
                .map_err(|e| e.to_string())?;
            for (node_id, p) in positions {
                if !p.x.is_finite() || !p.y.is_finite() {
                    continue;
                }
                stmt.execute(params![session_id, direction, node_id, p.x, p.y, p.pinned])
                    .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())
    }

    /// Removes saved coordinates for one direction only.
    pub fn clear_node_positions(&self, session_id: &str, direction: &str) -> Result<(), String> {
        check_direction(direction)?;
        let conn = self.lock();
        conn.execute(
            "DELETE FROM node_positions WHERE session_id = ?1 AND direction = ?2",
            params![session_id, direction],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn check_direction(direction: &str) -> Result<(), String> {
    match direction {
        "TB" | "LR" => Ok(()),
        other => Err(format!("invalid graph direction `{other}` (expected TB or LR)")),
    }
}

fn migrate(conn: &mut Connection) -> Result<(), String> {
    let current: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    for (idx, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute_batch(sql)
            .map_err(|e| format!("migration v{}: {e}", idx + 1))?;
        tx.pragma_update(None, "user_version", (idx + 1) as i64)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_graph_direction(
    db: State<'_, Database>,
    session_id: String,
) -> Result<Option<String>, String> {
    db.graph_direction(&session_id)
}

#[tauri::command]
pub async fn set_graph_direction(
    db: State<'_, Database>,
    session_id: String,
    direction: String,
) -> Result<(), String> {
    db.set_graph_direction(&session_id, &direction)
}

#[tauri::command]
pub async fn load_node_positions(
    db: State<'_, Database>,
    session_id: String,
    direction: String,
) -> Result<HashMap<String, NodePosition>, String> {
    db.load_node_positions(&session_id, &direction)
}

#[tauri::command]
pub async fn save_node_positions(
    db: State<'_, Database>,
    session_id: String,
    direction: String,
    positions: HashMap<String, NodePosition>,
) -> Result<(), String> {
    db.save_node_positions(&session_id, &direction, &positions)
}

#[tauri::command]
pub async fn clear_node_positions(
    db: State<'_, Database>,
    session_id: String,
    direction: String,
) -> Result<(), String> {
    db.clear_node_positions(&session_id, &direction)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(x: f64, y: f64) -> NodePosition {
        NodePosition {
            x,
            y,
            pinned: true,
        }
    }

    #[test]
    fn direction_defaults_to_none_then_persists() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.graph_direction("s1").unwrap(), None);
        db.set_graph_direction("s1", "LR").unwrap();
        assert_eq!(db.graph_direction("s1").unwrap().as_deref(), Some("LR"));
        db.set_graph_direction("s1", "TB").unwrap();
        assert_eq!(db.graph_direction("s1").unwrap().as_deref(), Some("TB"));
        assert!(db.set_graph_direction("s1", "XY").is_err());
    }

    #[test]
    fn positions_are_keyed_by_direction() {
        let db = Database::open_in_memory().unwrap();
        let tb = HashMap::from([("a".to_string(), pos(1.0, 2.0))]);
        let lr = HashMap::from([("a".to_string(), pos(30.0, 40.0))]);
        db.save_node_positions("s1", "TB", &tb).unwrap();
        db.save_node_positions("s1", "LR", &lr).unwrap();
        assert_eq!(db.load_node_positions("s1", "TB").unwrap(), tb);
        assert_eq!(db.load_node_positions("s1", "LR").unwrap(), lr);
        assert!(db.load_node_positions("other", "TB").unwrap().is_empty());
    }

    #[test]
    fn save_merges_and_clear_is_per_direction() {
        let db = Database::open_in_memory().unwrap();
        db.save_node_positions("s1", "TB", &HashMap::from([("a".into(), pos(1.0, 1.0))]))
            .unwrap();
        db.save_node_positions(
            "s1",
            "TB",
            &HashMap::from([("a".into(), pos(5.0, 6.0)), ("b".into(), pos(7.0, 8.0))]),
        )
        .unwrap();
        db.save_node_positions("s1", "LR", &HashMap::from([("a".into(), pos(9.0, 9.0))]))
            .unwrap();

        let tb = db.load_node_positions("s1", "TB").unwrap();
        assert_eq!(tb.len(), 2);
        assert_eq!(tb["a"], pos(5.0, 6.0));

        db.clear_node_positions("s1", "TB").unwrap();
        assert!(db.load_node_positions("s1", "TB").unwrap().is_empty());
        assert_eq!(db.load_node_positions("s1", "LR").unwrap().len(), 1);
    }

    #[test]
    fn migrations_are_idempotent_on_reopen() {
        let dir = std::env::temp_dir().join(format!("pipan-db-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pipan.db");
        {
            let db = Database::open(&path).unwrap();
            db.set_graph_direction("s1", "LR").unwrap();
        }
        let db = Database::open(&path).unwrap();
        assert_eq!(db.graph_direction("s1").unwrap().as_deref(), Some("LR"));
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
