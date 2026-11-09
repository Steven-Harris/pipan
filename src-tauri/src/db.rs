//! PiPan's own SQLite store (`<app_local_data_dir>/pipan.db`, WAL mode).
//!
//! Schema is versioned through `PRAGMA user_version`; each migration runs once, in
//! order, inside a transaction. Later phases append migrations rather than editing
//! earlier ones.
//!
//! Version 1 (node graph): `sessions` (just enough to own a graph direction; the
//! persistent-sessions phase extends it) and `node_positions`.
//! Version 2 (persistent sessions): extends `sessions` with full session metadata,
//! adds `session_tags`, and creates indexes for search/filter/sort queries.
//! Version 3 (github integration): adds `github_views` for custom saved PR search views.
//! Version 4 (cost & usage analytics): adds `usage_ledger` and `usage_file_checkpoints`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

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
    // v2: persistent sessions metadata, tags, and search/sort indexes.
    "ALTER TABLE sessions ADD COLUMN name TEXT NOT NULL DEFAULT '';
    ALTER TABLE sessions ADD COLUMN title TEXT NOT NULL DEFAULT '';
    ALTER TABLE sessions ADD COLUMN status TEXT NOT NULL DEFAULT 'disconnected';
    ALTER TABLE sessions ADD COLUMN mode TEXT NOT NULL DEFAULT 'headless';
    ALTER TABLE sessions ADD COLUMN cwd TEXT;
    ALTER TABLE sessions ADD COLUMN session_file_path TEXT;
    ALTER TABLE sessions ADD COLUMN pi_session_file TEXT;
    ALTER TABLE sessions ADD COLUMN group_name TEXT;
    ALTER TABLE sessions ADD COLUMN branch TEXT;
    ALTER TABLE sessions ADD COLUMN worktree_path TEXT;
    ALTER TABLE sessions ADD COLUMN model TEXT;
    ALTER TABLE sessions ADD COLUMN model_id TEXT;
    ALTER TABLE sessions ADD COLUMN provider TEXT;
    ALTER TABLE sessions ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE sessions ADD COLUMN archived INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE sessions ADD COLUMN message_count INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE sessions ADD COLUMN tool_call_count INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE sessions ADD COLUMN total_tokens INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE sessions ADD COLUMN total_cost_usd REAL NOT NULL DEFAULT 0.0;

    CREATE TABLE session_tags (
        session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
        tag         TEXT NOT NULL,
        PRIMARY KEY (session_id, tag)
    );

    CREATE INDEX idx_sessions_updated_at ON sessions(updated_at);
    CREATE INDEX idx_sessions_archived ON sessions(archived);
    CREATE INDEX idx_sessions_cwd ON sessions(cwd);
    CREATE INDEX idx_session_tags_tag ON session_tags(tag);",
    // v3: persistent custom GitHub PR search views.
    "CREATE TABLE github_views (
        id          TEXT PRIMARY KEY,
        name        TEXT NOT NULL,
        query       TEXT NOT NULL,
        sort_order  INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX idx_github_views_sort_order ON github_views(sort_order);",
    // v4: usage ledger and JSONL file scan checkpoints for cost & token analytics.
    "CREATE TABLE usage_ledger (
        session_id   TEXT NOT NULL,
        message_id   TEXT NOT NULL,
        timestamp    INTEGER NOT NULL,
        provider     TEXT NOT NULL DEFAULT 'unknown',
        model        TEXT NOT NULL DEFAULT 'unknown',
        input        INTEGER NOT NULL DEFAULT 0,
        output       INTEGER NOT NULL DEFAULT 0,
        cache_read   INTEGER NOT NULL DEFAULT 0,
        cache_write  INTEGER NOT NULL DEFAULT 0,
        cost_usd     REAL NOT NULL DEFAULT 0.0,
        cwd          TEXT,
        PRIMARY KEY (session_id, message_id)
    );
    CREATE INDEX idx_usage_ledger_timestamp ON usage_ledger(timestamp);
    CREATE INDEX idx_usage_ledger_session_id ON usage_ledger(session_id);
    CREATE INDEX idx_usage_ledger_provider_model ON usage_ledger(provider, model);

    CREATE TABLE usage_file_checkpoints (
        path         TEXT PRIMARY KEY,
        mtime        INTEGER NOT NULL DEFAULT 0,
        byte_offset  INTEGER NOT NULL DEFAULT 0
    );",
];

/// A saved custom GitHub PR search view persisted in `github_views`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubView {
    pub id: String,
    pub name: String,
    pub query: String,
    #[serde(default, alias = "sort_order")]
    pub sort_order: i64,
}

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

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
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
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Connection> {
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

    /// Lists all custom GitHub PR search views ordered by `sort_order`, then `name`.
    pub fn list_github_views(&self) -> Result<Vec<GithubView>, String> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, query, sort_order
                 FROM github_views
                 ORDER BY sort_order ASC, name ASC, id ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(GithubView {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    query: row.get(2)?,
                    sort_order: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// Creates or updates a custom GitHub PR search view.
    pub fn save_github_view(&self, view: &GithubView) -> Result<GithubView, String> {
        let id = view.id.trim();
        let name = view.name.trim();
        let query = view.query.trim();
        if id.is_empty() {
            return Err("GitHub view id must not be empty".to_string());
        }
        if name.is_empty() {
            return Err("GitHub view name must not be empty".to_string());
        }
        if query.is_empty() {
            return Err("GitHub view query must not be empty".to_string());
        }
        let saved = GithubView {
            id: id.to_string(),
            name: name.to_string(),
            query: query.to_string(),
            sort_order: view.sort_order,
        };
        let conn = self.lock();
        conn.execute(
            "INSERT INTO github_views (id, name, query, sort_order)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 query = excluded.query,
                 sort_order = excluded.sort_order",
            params![saved.id, saved.name, saved.query, saved.sort_order],
        )
        .map_err(|e| e.to_string())?;
        Ok(saved)
    }

    /// Deletes a custom GitHub PR search view by `id`.
    pub fn delete_github_view(&self, id: &str) -> Result<bool, String> {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            return Err("GitHub view id must not be empty".to_string());
        }
        let conn = self.lock();
        let deleted = conn
            .execute("DELETE FROM github_views WHERE id = ?1", params![trimmed])
            .map_err(|e| e.to_string())?;
        Ok(deleted > 0)
    }
}

fn check_direction(direction: &str) -> Result<(), String> {
    match direction {
        "TB" | "LR" => Ok(()),
        other => Err(format!(
            "invalid graph direction `{other}` (expected TB or LR)"
        )),
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
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || db.graph_direction(&session_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_graph_direction(
    db: State<'_, Database>,
    session_id: String,
    direction: String,
) -> Result<(), String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || db.set_graph_direction(&session_id, &direction))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn load_node_positions(
    db: State<'_, Database>,
    session_id: String,
    direction: String,
) -> Result<HashMap<String, NodePosition>, String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || db.load_node_positions(&session_id, &direction))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn save_node_positions(
    db: State<'_, Database>,
    session_id: String,
    direction: String,
    positions: HashMap<String, NodePosition>,
) -> Result<(), String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || db.save_node_positions(&session_id, &direction, &positions))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn clear_node_positions(
    db: State<'_, Database>,
    session_id: String,
    direction: String,
) -> Result<(), String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || db.clear_node_positions(&session_id, &direction))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn list_github_views(db: State<'_, Database>) -> Result<Vec<GithubView>, String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || db.list_github_views())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn save_github_view(
    db: State<'_, Database>,
    view: GithubView,
) -> Result<GithubView, String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || db.save_github_view(&view))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn delete_github_view(db: State<'_, Database>, id: String) -> Result<bool, String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || db.delete_github_view(&id))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(x: f64, y: f64) -> NodePosition {
        NodePosition { x, y, pinned: true }
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

    #[test]
    fn migrates_v1_database_to_v2_without_losing_data() {
        let dir = std::env::temp_dir().join(format!("pipan-db-v1v2-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pipan.db");

        // Create a v1 database manually
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.pragma_update(None, "user_version", 1i64).unwrap();
            conn.execute(
                "INSERT INTO sessions (id, graph_direction) VALUES ('v1-session', 'LR')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO node_positions (session_id, direction, node_id, x, y, pinned)
                 VALUES ('v1-session', 'LR', 'node-1', 12.0, 34.0, 1)",
                [],
            )
            .unwrap();
        }

        // Reopen with Database::open so migration v2 runs
        let db = Database::open(&path).unwrap();
        assert_eq!(
            db.graph_direction("v1-session").unwrap().as_deref(),
            Some("LR")
        );
        let positions = db.load_node_positions("v1-session", "LR").unwrap();
        assert_eq!(positions.get("node-1"), Some(&pos(12.0, 34.0)));

        let version: i64 = db
            .lock()
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);

        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn github_views_crud_and_ordering() {
        let db = Database::open_in_memory().unwrap();
        assert!(db.list_github_views().unwrap().is_empty());

        let v1 = db
            .save_github_view(&GithubView {
                id: "view-bugs".into(),
                name: "Bug PRs".into(),
                query: "is:open label:bug draft:false".into(),
                sort_order: 20,
            })
            .unwrap();
        assert_eq!(v1.name, "Bug PRs");

        db.save_github_view(&GithubView {
            id: "view-urgent".into(),
            name: "Urgent".into(),
            query: "is:open label:urgent".into(),
            sort_order: 10,
        })
        .unwrap();

        let listed = db.list_github_views().unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, "view-urgent");
        assert_eq!(listed[1].id, "view-bugs");

        // Update existing view
        db.save_github_view(&GithubView {
            id: "view-bugs".into(),
            name: "All Bug PRs".into(),
            query: "is:pr label:bug".into(),
            sort_order: 5,
        })
        .unwrap();

        let listed = db.list_github_views().unwrap();
        assert_eq!(listed[0].id, "view-bugs");
        assert_eq!(listed[0].name, "All Bug PRs");
        assert_eq!(listed[0].query, "is:pr label:bug");

        // Validation
        assert!(
            db.save_github_view(&GithubView {
                id: "".into(),
                name: "x".into(),
                query: "is:open".into(),
                sort_order: 0,
            })
            .is_err()
        );
        assert!(
            db.save_github_view(&GithubView {
                id: "x".into(),
                name: "   ".into(),
                query: "is:open".into(),
                sort_order: 0,
            })
            .is_err()
        );

        assert!(db.delete_github_view("view-bugs").unwrap());
        assert!(!db.delete_github_view("view-bugs").unwrap());
        assert_eq!(db.list_github_views().unwrap().len(), 1);
    }

    #[test]
    fn migrates_to_v4_and_supports_usage_ledger() {
        let db = Database::open_in_memory().unwrap();
        let version: i64 = db
            .lock()
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 4);

        // Verify usage_ledger and usage_file_checkpoints exist and accept rows
        let conn = db.lock();
        conn.execute(
            "INSERT INTO usage_ledger (session_id, message_id, timestamp, provider, model, cost_usd)
             VALUES ('s-v4', 'm-v4', 1715000000, 'anthropic', 'claude-3-7-sonnet', 0.05)",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO usage_file_checkpoints (path, mtime, byte_offset)
             VALUES ('/path/test.jsonl', 1715000000, 1024)",
            [],
        )
        .unwrap();
    }
}
