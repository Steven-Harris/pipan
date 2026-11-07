//! Persistent session store backed by the shared SQLite [`Database`] in [`crate::db`].
//!
//! Provides session CRUD, SQL-pushed-down search/filter/sort queries, one-time
//! `pipan-metadata.json` migration, and non-blocking Tauri commands (`spawn_blocking`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params, types::Value as SqlValue};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Manager, State};

use crate::agent::AgentManager;
use crate::db::Database;
use crate::pi_rpc::PiRpcManager;

/// A persisted session row hydrated with its `session_tags`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedSession {
    pub id: String,
    pub name: String,
    pub title: String,
    pub status: String,
    pub mode: String,
    pub cwd: Option<String>,
    pub session_file_path: Option<String>,
    pub pi_session_file: Option<String>,
    pub group_name: Option<String>,
    pub branch: Option<String>,
    pub worktree_path: Option<String>,
    pub archived: bool,
    pub pinned: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: i64,
    pub tool_call_count: i64,
    pub total_tokens: i64,
    pub total_cost_usd: f64,
    pub model_id: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub graph_direction: Option<String>,
    pub tags: Vec<String>,
}

/// Upsert payload for [`Database::save_session_metadata`].
/// Any field left as `None` preserves the existing database value on conflict.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSessionMetadataInput {
    #[serde(alias = "sessionId", alias = "session_id")]
    pub id: String,
    #[serde(default, alias = "title", alias = "label")]
    pub name: Option<String>,
    #[serde(default, alias = "state")]
    pub status: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(
        default,
        alias = "session_file_path",
        alias = "piSessionFile",
        alias = "pi_session_file"
    )]
    pub session_file_path: Option<String>,
    #[serde(default, alias = "group_name", alias = "group")]
    pub group_name: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default, alias = "worktree_path")]
    pub worktree_path: Option<String>,
    #[serde(default)]
    pub archived: Option<bool>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(
        default,
        alias = "created_at",
        alias = "startedAt",
        alias = "started_at"
    )]
    pub created_at: Option<i64>,
    #[serde(default, alias = "updated_at")]
    pub updated_at: Option<i64>,
    #[serde(default, alias = "message_count")]
    pub message_count: Option<i64>,
    #[serde(default, alias = "tool_call_count")]
    pub tool_call_count: Option<i64>,
    #[serde(default, alias = "total_tokens")]
    pub total_tokens: Option<i64>,
    #[serde(default, alias = "total_cost_usd", alias = "cost")]
    pub total_cost_usd: Option<f64>,
    #[serde(default, alias = "model_id", alias = "model")]
    pub model_id: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default, alias = "graph_direction")]
    pub graph_direction: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

/// Filter, search, and sort parameters pushed down into SQL.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListQuery {
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default, alias = "sort_by")]
    pub sort_by: Option<String>,
    #[serde(default, alias = "sort_order")]
    pub sort_order: Option<String>,
}

pub fn now_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn normalize_timestamp(ts: i64) -> i64 {
    // Convert millisecond timestamps (e.g. JS Date.now()) to unix seconds.
    if ts > 10_000_000_000 { ts / 1000 } else { ts }
}

/// Sanitizes a session ID so it is safe to use as a filename component.
pub fn sanitize_session_id(raw: &str) -> String {
    let trimmed = raw.trim();
    let cleaned: String = trimmed
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "session".to_string()
    } else {
        cleaned
    }
}

/// Resolves `<sessions_root>/<safe_session_id>.jsonl`.
pub fn resolve_session_file_path(sessions_root: &Path, session_id: &str) -> PathBuf {
    sessions_root.join(format!("{}.jsonl", sanitize_session_id(session_id)))
}

pub fn default_session_name(mode: &str, cwd: Option<&str>, branch: Option<&str>) -> String {
    if mode == "worktree"
        && let Some(b) = branch.map(str::trim).filter(|s| !s.is_empty())
    {
        return format!("Worktree ({b})");
    }
    if mode == "headless" || cwd.map(str::trim).filter(|s| !s.is_empty()).is_none() {
        return "Headless Sandbox".to_string();
    }
    let raw = cwd.unwrap_or_default().trim_start_matches(r"\\?\");
    let parts: Vec<&str> = raw
        .split(['/', '\\'])
        .filter(|segment| !segment.is_empty())
        .collect();
    match parts.as_slice() {
        [] => "Directory Session".to_string(),
        [single] => (*single).to_string(),
        many => {
            let sep = if raw.contains('\\') { '\\' } else { '/' };
            format!("…{sep}{}", many[many.len() - 2..].join(&sep.to_string()))
        }
    }
}

fn load_tags_for_session(conn: &Connection, session_id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT tag FROM session_tags WHERE session_id = ?1 ORDER BY tag ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![session_id], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn replace_session_tags(
    conn: &Connection,
    session_id: &str,
    tags: &[String],
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM session_tags WHERE session_id = ?1",
        params![session_id],
    )
    .map_err(|e| e.to_string())?;

    let mut seen = HashSet::new();
    let mut stmt = conn
        .prepare("INSERT INTO session_tags (session_id, tag) VALUES (?1, ?2)")
        .map_err(|e| e.to_string())?;
    for raw in tags {
        let tag = raw.trim();
        if tag.is_empty() || !seen.insert(tag.to_string()) {
            continue;
        }
        stmt.execute(params![session_id, tag])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn read_session_row(conn: &Connection, session_id: &str) -> Result<Option<SavedSession>, String> {
    let session_opt = conn
        .query_row(
            "SELECT
                id,
                name,
                title,
                status,
                mode,
                cwd,
                session_file_path,
                pi_session_file,
                group_name,
                branch,
                worktree_path,
                archived,
                pinned,
                created_at,
                updated_at,
                message_count,
                tool_call_count,
                total_tokens,
                total_cost_usd,
                model_id,
                model,
                provider,
                graph_direction
             FROM sessions
             WHERE id = ?1",
            params![session_id],
            |row| {
                let id: String = row.get(0)?;
                let raw_name: String = row.get(1)?;
                let raw_title: String = row.get(2)?;
                let status: String = row.get(3)?;
                let mode: String = row.get(4)?;
                let cwd: Option<String> = row.get(5)?;
                let sfp: Option<String> = row.get(6)?;
                let psf: Option<String> = row.get(7)?;
                let group_name: Option<String> = row.get(8)?;
                let branch: Option<String> = row.get(9)?;
                let worktree_path: Option<String> = row.get(10)?;
                let archived: i64 = row.get(11)?;
                let pinned: i64 = row.get(12)?;
                let created_at: i64 = row.get(13)?;
                let updated_at: i64 = row.get(14)?;
                let message_count: i64 = row.get(15)?;
                let tool_call_count: i64 = row.get(16)?;
                let total_tokens: i64 = row.get(17)?;
                let total_cost_usd: f64 = row.get(18)?;
                let raw_model_id: Option<String> = row.get(19)?;
                let raw_model: Option<String> = row.get(20)?;
                let provider: Option<String> = row.get(21)?;
                let graph_direction: Option<String> = row.get(22)?;

                let effective_name = if !raw_name.trim().is_empty() {
                    raw_name
                } else if !raw_title.trim().is_empty() {
                    raw_title
                } else {
                    default_session_name(&mode, cwd.as_deref(), branch.as_deref())
                };
                let effective_file = sfp.or(psf);
                let effective_model = raw_model_id.or(raw_model);

                Ok(SavedSession {
                    id,
                    name: effective_name.clone(),
                    title: effective_name,
                    status,
                    mode,
                    cwd,
                    session_file_path: effective_file.clone(),
                    pi_session_file: effective_file,
                    group_name,
                    branch,
                    worktree_path,
                    archived: archived != 0,
                    pinned: pinned != 0,
                    created_at,
                    updated_at,
                    message_count,
                    tool_call_count,
                    total_tokens,
                    total_cost_usd,
                    model_id: effective_model.clone(),
                    model: effective_model,
                    provider,
                    graph_direction,
                    tags: Vec::new(),
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;

    match session_opt {
        Some(mut s) => {
            s.tags = load_tags_for_session(conn, &s.id)?;
            Ok(Some(s))
        }
        None => Ok(None),
    }
}

fn upsert_session_in_conn(
    conn: &Connection,
    input: &SaveSessionMetadataInput,
) -> Result<SavedSession, String> {
    let id = input.id.trim();
    if id.is_empty() {
        return Err("Session id must not be empty".to_string());
    }

    let existing = read_session_row(conn, id)?;
    let now = now_unix_secs();

    let mode = input
        .mode
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| existing.as_ref().map(|e| e.mode.clone()))
        .unwrap_or_else(|| "headless".to_string());

    let cwd = match &input.cwd {
        Some(c) if c.trim().is_empty() => None,
        Some(c) => Some(c.trim().to_string()),
        None => existing.as_ref().and_then(|e| e.cwd.clone()),
    };

    let branch = match &input.branch {
        Some(b) if b.trim().is_empty() => None,
        Some(b) => Some(b.trim().to_string()),
        None => existing.as_ref().and_then(|e| e.branch.clone()),
    };

    let worktree_path = match &input.worktree_path {
        Some(w) if w.trim().is_empty() => None,
        Some(w) => Some(w.trim().to_string()),
        None => existing.as_ref().and_then(|e| e.worktree_path.clone()),
    };

    let name = input
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            existing
                .as_ref()
                .map(|e| e.name.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| default_session_name(&mode, cwd.as_deref(), branch.as_deref()));

    let status = input
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| existing.as_ref().map(|e| e.status.clone()))
        .unwrap_or_else(|| "disconnected".to_string());

    let session_file_path = match &input.session_file_path {
        Some(p) if p.trim().is_empty() => None,
        Some(p) => Some(p.trim().to_string()),
        None => existing.as_ref().and_then(|e| e.session_file_path.clone()),
    };

    let group_name = match &input.group_name {
        Some(g) if g.trim().is_empty() => None,
        Some(g) => Some(g.trim().to_string()),
        None => existing.as_ref().and_then(|e| e.group_name.clone()),
    };

    let archived = input
        .archived
        .or_else(|| existing.as_ref().map(|e| e.archived))
        .unwrap_or(false);

    let pinned = input
        .pinned
        .or_else(|| existing.as_ref().map(|e| e.pinned))
        .unwrap_or(false);

    let created_at = input
        .created_at
        .map(normalize_timestamp)
        .or_else(|| existing.as_ref().map(|e| e.created_at))
        .unwrap_or(now);

    let updated_at = input.updated_at.map(normalize_timestamp).unwrap_or(now);

    let message_count = input
        .message_count
        .or_else(|| existing.as_ref().map(|e| e.message_count))
        .unwrap_or(0)
        .max(0);

    let tool_call_count = input
        .tool_call_count
        .or_else(|| existing.as_ref().map(|e| e.tool_call_count))
        .unwrap_or(0)
        .max(0);

    let total_tokens = input
        .total_tokens
        .or_else(|| existing.as_ref().map(|e| e.total_tokens))
        .unwrap_or(0)
        .max(0);

    let total_cost_usd = input
        .total_cost_usd
        .filter(|v| v.is_finite() && *v >= 0.0)
        .or_else(|| existing.as_ref().map(|e| e.total_cost_usd))
        .unwrap_or(0.0);

    let model_id = match &input.model_id {
        Some(m) if m.trim().is_empty() => None,
        Some(m) => Some(m.trim().to_string()),
        None => existing.as_ref().and_then(|e| e.model_id.clone()),
    };

    let provider = match &input.provider {
        Some(p) if p.trim().is_empty() => None,
        Some(p) => Some(p.trim().to_string()),
        None => existing.as_ref().and_then(|e| e.provider.clone()),
    };

    let graph_direction = match &input.graph_direction {
        Some(d) if d == "TB" || d == "LR" => Some(d.clone()),
        _ => existing.as_ref().and_then(|e| e.graph_direction.clone()),
    };

    conn.execute(
        "INSERT INTO sessions (
            id, name, title, status, mode, cwd,
            session_file_path, pi_session_file,
            group_name, branch, worktree_path,
            archived, pinned, created_at, updated_at,
            message_count, tool_call_count, total_tokens, total_cost_usd,
            model_id, model, provider, graph_direction
        ) VALUES (
            ?1, ?2, ?2, ?3, ?4, ?5,
            ?6, ?6,
            ?7, ?8, ?9,
            ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17,
            ?18, ?18, ?19, ?20
        )
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            title = excluded.title,
            status = excluded.status,
            mode = excluded.mode,
            cwd = excluded.cwd,
            session_file_path = excluded.session_file_path,
            pi_session_file = excluded.pi_session_file,
            group_name = excluded.group_name,
            branch = excluded.branch,
            worktree_path = excluded.worktree_path,
            archived = excluded.archived,
            pinned = excluded.pinned,
            created_at = excluded.created_at,
            updated_at = excluded.updated_at,
            message_count = excluded.message_count,
            tool_call_count = excluded.tool_call_count,
            total_tokens = excluded.total_tokens,
            total_cost_usd = excluded.total_cost_usd,
            model_id = excluded.model_id,
            model = excluded.model,
            provider = excluded.provider,
            graph_direction = COALESCE(excluded.graph_direction, sessions.graph_direction)",
        params![
            id,
            name,
            status,
            mode,
            cwd,
            session_file_path,
            group_name,
            branch,
            worktree_path,
            i64::from(archived),
            i64::from(pinned),
            created_at,
            updated_at,
            message_count,
            tool_call_count,
            total_tokens,
            total_cost_usd,
            model_id,
            provider,
            graph_direction,
        ],
    )
    .map_err(|e| e.to_string())?;

    if let Some(tags) = &input.tags {
        replace_session_tags(conn, id, tags)?;
    }

    read_session_row(conn, id)?.ok_or_else(|| format!("Failed to read saved session {id}"))
}

impl Database {
    pub fn get_session(&self, session_id: &str) -> Result<Option<SavedSession>, String> {
        let conn = self.lock();
        read_session_row(&conn, session_id)
    }

    pub fn save_session_metadata(
        &self,
        input: &SaveSessionMetadataInput,
    ) -> Result<SavedSession, String> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let saved = upsert_session_in_conn(&tx, input)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(saved)
    }

    pub fn rename_session(&self, session_id: &str, new_name: &str) -> Result<SavedSession, String> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return Err("Session name cannot be empty".to_string());
        }
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let now = now_unix_secs();
        let changed = tx
            .execute(
                "UPDATE sessions
                 SET name = ?2, title = ?2, updated_at = ?3
                 WHERE id = ?1",
                params![session_id, trimmed, now],
            )
            .map_err(|e| e.to_string())?;
        if changed == 0 {
            tx.execute(
                "INSERT INTO sessions (id, name, title, created_at, updated_at)
                 VALUES (?1, ?2, ?2, ?3, ?3)",
                params![session_id, trimmed, now],
            )
            .map_err(|e| e.to_string())?;
        }
        let session = read_session_row(&tx, session_id)?
            .ok_or_else(|| format!("Session not found after rename: {session_id}"))?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(session)
    }

    pub fn archive_session(
        &self,
        session_id: &str,
        archived: bool,
    ) -> Result<SavedSession, String> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let now = now_unix_secs();
        let changed = tx
            .execute(
                "UPDATE sessions
                 SET archived = ?2, updated_at = ?3
                 WHERE id = ?1",
                params![session_id, i64::from(archived), now],
            )
            .map_err(|e| e.to_string())?;
        if changed == 0 {
            return Err(format!("Session not found: {session_id}"));
        }
        let session = read_session_row(&tx, session_id)?
            .ok_or_else(|| format!("Session not found: {session_id}"))?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(session)
    }

    pub fn set_session_status(&self, session_id: &str, status: &str) -> Result<(), String> {
        let conn = self.lock();
        conn.execute(
            "UPDATE sessions SET status = ?2, updated_at = ?3 WHERE id = ?1",
            params![session_id, status, now_unix_secs()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Queries `sessions` with SQL pushdown for search (`LIKE` on name/cwd/group/branch/tags),
    /// filter (`active`, `running`, `archived`, `directory`, `headless`, `worktree`, `all`),
    /// and sort (`updated_at`, `created_at`, `name`, `total_cost_usd`, `message_count`).
    pub fn list_saved_sessions(
        &self,
        query: &SessionListQuery,
    ) -> Result<Vec<SavedSession>, String> {
        let conn = self.lock();

        let mut sql = String::from(
            "SELECT s.id
             FROM sessions s
             WHERE NOT (
                 s.name = ''
                 AND s.title = ''
                 AND s.cwd IS NULL
                 AND s.session_file_path IS NULL
                 AND s.pi_session_file IS NULL
                 AND s.message_count = 0
                 AND s.status = 'disconnected'
             )",
        );
        let mut bind_values: Vec<SqlValue> = Vec::new();

        let filter = query
            .filter
            .as_deref()
            .unwrap_or("active")
            .trim()
            .to_lowercase();
        match filter.as_str() {
            "archived" => sql.push_str(" AND s.archived = 1"),
            "running" => {
                sql.push_str(" AND s.archived = 0 AND s.status IN ('running', 'starting')")
            }
            "directory" => sql.push_str(" AND s.archived = 0 AND s.mode = 'directory'"),
            "headless" => sql.push_str(" AND s.archived = 0 AND s.mode = 'headless'"),
            "worktree" => sql.push_str(" AND s.archived = 0 AND s.mode = 'worktree'"),
            "all" => {}
            _ => sql.push_str(" AND s.archived = 0"),
        }

        if let Some(search_raw) = query
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let pattern = format!("%{}%", search_raw.to_lowercase());
            bind_values.push(SqlValue::Text(pattern));
            let idx = bind_values.len();
            sql.push_str(&format!(
                " AND (
                    LOWER(s.name) LIKE ?{idx}
                    OR LOWER(s.title) LIKE ?{idx}
                    OR LOWER(COALESCE(s.cwd, '')) LIKE ?{idx}
                    OR LOWER(COALESCE(s.group_name, '')) LIKE ?{idx}
                    OR LOWER(COALESCE(s.branch, '')) LIKE ?{idx}
                    OR EXISTS (
                        SELECT 1 FROM session_tags st
                        WHERE st.session_id = s.id AND LOWER(st.tag) LIKE ?{idx}
                    )
                )"
            ));
        }

        let sort_by = query
            .sort_by
            .as_deref()
            .unwrap_or("updated_at")
            .trim()
            .to_lowercase();
        let explicit_asc = query
            .sort_order
            .as_deref()
            .map(|o| o.trim().eq_ignore_ascii_case("asc"));

        let order_clause = match sort_by.as_str() {
            "created_at" | "created_date" | "created" => {
                if explicit_asc == Some(true) {
                    "s.pinned DESC, s.created_at ASC, s.id ASC"
                } else {
                    "s.pinned DESC, s.created_at DESC, s.id ASC"
                }
            }
            "name" | "title" | "name_asc" => {
                if explicit_asc == Some(false) {
                    "s.pinned DESC, LOWER(COALESCE(NULLIF(s.name, ''), s.title)) DESC, s.updated_at DESC"
                } else {
                    "s.pinned DESC, LOWER(COALESCE(NULLIF(s.name, ''), s.title)) ASC, s.updated_at DESC"
                }
            }
            "total_cost_usd" | "total_cost" | "cost" => {
                if explicit_asc == Some(true) {
                    "s.pinned DESC, s.total_cost_usd ASC, s.updated_at DESC"
                } else {
                    "s.pinned DESC, s.total_cost_usd DESC, s.updated_at DESC"
                }
            }
            "message_count" | "messages" => {
                if explicit_asc == Some(true) {
                    "s.pinned DESC, s.message_count ASC, s.updated_at DESC"
                } else {
                    "s.pinned DESC, s.message_count DESC, s.updated_at DESC"
                }
            }
            _ => {
                if explicit_asc == Some(true) {
                    "s.pinned DESC, s.updated_at ASC, s.id ASC"
                } else {
                    "s.pinned DESC, s.updated_at DESC, s.id ASC"
                }
            }
        };
        sql.push_str(" ORDER BY ");
        sql.push_str(order_clause);

        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let ids: Vec<String> = stmt
            .query_map(rusqlite::params_from_iter(bind_values.iter()), |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(session) = read_session_row(&conn, &id)? {
                out.push(session);
            }
        }
        Ok(out)
    }

    /// Removes the session row from `sessions` (child rows in `session_tags` and `node_positions`
    /// cascade-delete) and returns the removed [`SavedSession`] metadata if it existed.
    pub fn delete_session_record(&self, session_id: &str) -> Result<Option<SavedSession>, String> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let existing = read_session_row(&tx, session_id)?;
        tx.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(existing)
    }

    /// Deletes the session row from `pipan.db` (cascading child rows) and removes its `.jsonl`
    /// transcript file from disk if present.
    pub fn delete_session_and_artifacts(&self, session_id: &str) -> Result<bool, String> {
        let removed = self.delete_session_record(session_id)?;
        if let Some(ref session) = removed
            && let Some(ref file_path) = session.session_file_path
        {
            remove_jsonl_file_if_exists(Path::new(file_path));
        }
        Ok(removed.is_some())
    }

    /// One-time migration: if `json_path` (e.g. `<app_local_data_dir>/pipan-metadata.json`) exists,
    /// imports all sessions into `pipan.db` inside a single SQLite transaction, then renames
    /// the file to `pipan-metadata.json.migrated`.
    pub fn import_legacy_metadata_json(&self, json_path: &Path) -> Result<usize, String> {
        if !json_path.is_file() {
            return Ok(0);
        }

        let raw = std::fs::read_to_string(json_path)
            .map_err(|e| format!("read {}: {e}", json_path.display()))?;
        let parsed: JsonValue = serde_json::from_str(&raw)
            .map_err(|e| format!("parse {}: {e}", json_path.display()))?;

        let entries: Vec<SaveSessionMetadataInput> = extract_legacy_session_entries(&parsed)?;

        let count = {
            let mut conn = self.lock();
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            let mut imported = 0usize;
            for entry in &entries {
                if entry.id.trim().is_empty() {
                    continue;
                }
                upsert_session_in_conn(&tx, entry)?;
                imported += 1;
            }
            tx.commit().map_err(|e| e.to_string())?;
            imported
        };

        let migrated_path = PathBuf::from(format!("{}.migrated", json_path.display()));
        std::fs::rename(json_path, &migrated_path).map_err(|e| {
            format!(
                "rename {} -> {}: {e}",
                json_path.display(),
                migrated_path.display()
            )
        })?;

        Ok(count)
    }
}

fn extract_legacy_session_entries(
    parsed: &JsonValue,
) -> Result<Vec<SaveSessionMetadataInput>, String> {
    if let Some(arr) = parsed.as_array() {
        return arr
            .iter()
            .map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string()))
            .collect();
    }

    if let Some(obj) = parsed.as_object() {
        if let Some(sessions_val) = obj.get("sessions") {
            return extract_legacy_session_entries(sessions_val);
        }

        // Map of `{ "<session_id>": { ... } }`
        let mut out = Vec::new();
        for (k, v) in obj {
            if let Some(mut item_obj) = v.as_object().cloned() {
                item_obj
                    .entry("id".to_string())
                    .or_insert_with(|| JsonValue::String(k.clone()));
                let input: SaveSessionMetadataInput =
                    serde_json::from_value(JsonValue::Object(item_obj))
                        .map_err(|e| e.to_string())?;
                out.push(input);
            }
        }
        return Ok(out);
    }

    Ok(Vec::new())
}

pub(crate) fn remove_jsonl_file_if_exists(path: &Path) {
    let is_jsonl = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("jsonl"))
        .unwrap_or(false);
    if is_jsonl && path.is_file() {
        let _ = std::fs::remove_file(path);
    }
}

// ============================================================================
// Tauri Commands (all SQLite I/O runs via `tokio::task::spawn_blocking`)
// ============================================================================

#[tauri::command]
pub async fn list_saved_sessions(
    db: State<'_, Database>,
    query: Option<SessionListQuery>,
    search: Option<String>,
    filter: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
) -> Result<Vec<SavedSession>, String> {
    let mut effective = query.unwrap_or_default();
    if search.is_some() {
        effective.search = search;
    }
    if filter.is_some() {
        effective.filter = filter;
    }
    if sort_by.is_some() {
        effective.sort_by = sort_by;
    }
    if sort_order.is_some() {
        effective.sort_order = sort_order;
    }

    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || db.list_saved_sessions(&effective))
        .await
        .map_err(|e| format!("list_saved_sessions join error: {e}"))?
}

#[tauri::command]
pub async fn save_session_metadata(
    db: State<'_, Database>,
    metadata: Option<SaveSessionMetadataInput>,
    session: Option<SaveSessionMetadataInput>,
    input: Option<SaveSessionMetadataInput>,
) -> Result<SavedSession, String> {
    let payload = metadata
        .or(session)
        .or(input)
        .ok_or_else(|| "Missing session metadata payload".to_string())?;
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || db.save_session_metadata(&payload))
        .await
        .map_err(|e| format!("save_session_metadata join error: {e}"))?
}

#[tauri::command]
pub async fn rename_session(
    db: State<'_, Database>,
    pi_manager: State<'_, PiRpcManager>,
    session_id: String,
    name: String,
) -> Result<SavedSession, String> {
    let db_clone = db.inner().clone();
    let sid = session_id.clone();
    let new_name = name.clone();
    let updated = tokio::task::spawn_blocking(move || db_clone.rename_session(&sid, &new_name))
        .await
        .map_err(|e| format!("rename_session join error: {e}"))??;

    pi_manager
        .update_active_session_name(&session_id, &updated.name)
        .await;

    Ok(updated)
}

#[tauri::command]
pub async fn archive_session(
    db: State<'_, Database>,
    session_id: String,
    archived: bool,
) -> Result<SavedSession, String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || db.archive_session(&session_id, archived))
        .await
        .map_err(|e| format!("archive_session join error: {e}"))?
}

#[tauri::command]
pub async fn delete_session(
    app_handle: AppHandle,
    db: State<'_, Database>,
    pi_manager: State<'_, PiRpcManager>,
    agent_manager: State<'_, AgentManager>,
    session_id: String,
) -> Result<bool, String> {
    // 1. Stop active Pi RPC process if it is running this session
    if pi_manager.get_status().await.session_id.as_deref() == Some(session_id.as_str()) {
        pi_manager.stop_session(&app_handle).await;
    }

    // 2. Stop running worktree agent process & prune worktree if managed by AgentManager
    let _ = agent_manager.stop(&app_handle, &session_id, true).await;

    let app_data_dir = app_handle.path().app_local_data_dir().ok();
    let fallback_jsonl = app_data_dir
        .as_ref()
        .map(|d| resolve_session_file_path(&d.join("sessions"), &session_id));

    // 3. Delete session row in pipan.db (cascading session_tags and node_positions)
    //    and remove the .jsonl transcript file on disk via spawn_blocking.
    let db_clone = db.inner().clone();
    let sid = session_id.clone();
    let removed = tokio::task::spawn_blocking(move || {
        let record = db_clone.delete_session_record(&sid)?;
        if let Some(ref session) = record
            && let Some(ref path_str) = session.session_file_path
        {
            remove_jsonl_file_if_exists(Path::new(path_str));
        }
        if let Some(ref fb) = fallback_jsonl {
            remove_jsonl_file_if_exists(fb);
        }
        Ok::<Option<SavedSession>, String>(record)
    })
    .await
    .map_err(|e| format!("delete_session join error: {e}"))??;

    // 4. Prune any associated git worktree that was persisted on the session record
    if let Some(ref session) = removed {
        let wt_candidate = session.worktree_path.as_deref().or_else(|| {
            (session.mode == "worktree")
                .then_some(session.cwd.as_deref())
                .flatten()
        });
        if let (Some(wt_path), Some(data_dir)) = (wt_candidate, app_data_dir.as_ref()) {
            let wt_root = data_dir.join("worktrees");
            let wt_target = Path::new(wt_path);
            if wt_target.exists() {
                let _ = crate::worktree::remove_worktree(&wt_root, wt_target).await;
            }
        }
    }

    Ok(removed.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NodePosition;
    use std::collections::HashMap;

    #[test]
    fn crud_and_partial_metadata_updates_preserve_fields() {
        let db = Database::open_in_memory().unwrap();

        let created = db
            .save_session_metadata(&SaveSessionMetadataInput {
                id: "sess-1".to_string(),
                name: Some("Initial Title".to_string()),
                status: Some("running".to_string()),
                mode: Some("directory".to_string()),
                cwd: Some("/workspace/pipan".to_string()),
                session_file_path: Some("/tmp/sess-1.jsonl".to_string()),
                group_name: Some("Backend".to_string()),
                pinned: Some(true),
                message_count: Some(4),
                tool_call_count: Some(2),
                total_tokens: Some(1500),
                total_cost_usd: Some(0.045),
                model_id: Some("claude-sonnet-4".to_string()),
                provider: Some("anthropic".to_string()),
                tags: Some(vec!["rust".to_string(), "sqlite".to_string()]),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(created.id, "sess-1");
        assert_eq!(created.name, "Initial Title");
        assert_eq!(created.title, "Initial Title");
        assert_eq!(created.group_name.as_deref(), Some("Backend"));
        assert!(created.pinned);
        assert!(!created.archived);
        assert_eq!(created.tags, vec!["rust".to_string(), "sqlite".to_string()]);

        // Partial update (e.g., telemetry tick) must not clobber name, group, pinned, or tags
        let updated = db
            .save_session_metadata(&SaveSessionMetadataInput {
                id: "sess-1".to_string(),
                message_count: Some(10),
                total_tokens: Some(4200),
                total_cost_usd: Some(0.12),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(updated.name, "Initial Title");
        assert_eq!(updated.group_name.as_deref(), Some("Backend"));
        assert!(updated.pinned);
        assert_eq!(updated.message_count, 10);
        assert_eq!(updated.total_tokens, 4200);
        assert!((updated.total_cost_usd - 0.12).abs() < 1e-9);
        assert_eq!(updated.tags, vec!["rust".to_string(), "sqlite".to_string()]);

        // Rename
        let renamed = db.rename_session("sess-1", "Renamed Session").unwrap();
        assert_eq!(renamed.name, "Renamed Session");
        assert_eq!(renamed.title, "Renamed Session");

        // Archive & unarchive
        let archived = db.archive_session("sess-1", true).unwrap();
        assert!(archived.archived);
        let unarchived = db.archive_session("sess-1", false).unwrap();
        assert!(!unarchived.archived);
    }

    #[test]
    fn sql_search_filter_and_sort_pushdown() {
        let db = Database::open_in_memory().unwrap();

        db.save_session_metadata(&SaveSessionMetadataInput {
            id: "s-alpha".to_string(),
            name: Some("Alpha Refactor".to_string()),
            status: Some("running".to_string()),
            mode: Some("directory".to_string()),
            cwd: Some("/projects/frontend-app".to_string()),
            group_name: Some("UI".to_string()),
            created_at: Some(1000),
            updated_at: Some(2000),
            message_count: Some(5),
            total_cost_usd: Some(0.50),
            tags: Some(vec!["svelte".to_string()]),
            ..Default::default()
        })
        .unwrap();

        db.save_session_metadata(&SaveSessionMetadataInput {
            id: "s-beta".to_string(),
            name: Some("Beta Sandbox".to_string()),
            status: Some("disconnected".to_string()),
            mode: Some("headless".to_string()),
            created_at: Some(3000),
            updated_at: Some(4000),
            message_count: Some(20),
            total_cost_usd: Some(0.10),
            tags: Some(vec!["experiment".to_string()]),
            ..Default::default()
        })
        .unwrap();

        db.save_session_metadata(&SaveSessionMetadataInput {
            id: "s-gamma".to_string(),
            name: Some("Gamma Worktree".to_string()),
            status: Some("running".to_string()),
            mode: Some("worktree".to_string()),
            cwd: Some("/worktrees/repo/s-gamma".to_string()),
            branch: Some("pipan/s-gamma".to_string()),
            created_at: Some(2000),
            updated_at: Some(3000),
            message_count: Some(12),
            total_cost_usd: Some(1.25),
            ..Default::default()
        })
        .unwrap();

        db.save_session_metadata(&SaveSessionMetadataInput {
            id: "s-archived".to_string(),
            name: Some("Old Archived Run".to_string()),
            status: Some("disconnected".to_string()),
            mode: Some("directory".to_string()),
            archived: Some(true),
            created_at: Some(500),
            updated_at: Some(5000),
            ..Default::default()
        })
        .unwrap();

        // Default filter ("active") excludes archived and orders by updated_at DESC
        let active = db
            .list_saved_sessions(&SessionListQuery::default())
            .unwrap();
        let active_ids: Vec<&str> = active.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(active_ids, vec!["s-beta", "s-gamma", "s-alpha"]);

        // Filter: running
        let running = db
            .list_saved_sessions(&SessionListQuery {
                filter: Some("running".to_string()),
                ..Default::default()
            })
            .unwrap();
        let running_ids: Vec<&str> = running.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(running_ids, vec!["s-gamma", "s-alpha"]);

        // Filter: archived
        let archived = db
            .list_saved_sessions(&SessionListQuery {
                filter: Some("archived".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].id, "s-archived");

        // Filter: worktree
        let worktree = db
            .list_saved_sessions(&SessionListQuery {
                filter: Some("worktree".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(worktree.len(), 1);
        assert_eq!(worktree[0].id, "s-gamma");

        // Search by tag
        let by_tag = db
            .list_saved_sessions(&SessionListQuery {
                search: Some("svelte".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_tag.len(), 1);
        assert_eq!(by_tag[0].id, "s-alpha");

        // Search by branch
        let by_branch = db
            .list_saved_sessions(&SessionListQuery {
                search: Some("pipan/s-gamma".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_branch.len(), 1);
        assert_eq!(by_branch[0].id, "s-gamma");

        // Sort by total_cost_usd DESC
        let by_cost = db
            .list_saved_sessions(&SessionListQuery {
                sort_by: Some("total_cost_usd".to_string()),
                ..Default::default()
            })
            .unwrap();
        let cost_ids: Vec<&str> = by_cost.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(cost_ids, vec!["s-gamma", "s-alpha", "s-beta"]);

        // Sort by message_count DESC
        let by_msgs = db
            .list_saved_sessions(&SessionListQuery {
                sort_by: Some("message_count".to_string()),
                ..Default::default()
            })
            .unwrap();
        let msg_ids: Vec<&str> = by_msgs.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(msg_ids, vec!["s-beta", "s-gamma", "s-alpha"]);

        // Sort by name ASC
        let by_name = db
            .list_saved_sessions(&SessionListQuery {
                sort_by: Some("name".to_string()),
                ..Default::default()
            })
            .unwrap();
        let name_ids: Vec<&str> = by_name.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(name_ids, vec!["s-alpha", "s-beta", "s-gamma"]);
    }

    #[test]
    fn delete_session_cascades_tags_and_node_positions_and_removes_jsonl() {
        let db = Database::open_in_memory().unwrap();
        let temp_dir = std::env::temp_dir().join(format!(
            "pipan-storage-del-{}",
            crate::worktree::new_session_id()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let jsonl_path = resolve_session_file_path(&temp_dir, "sess-del");
        std::fs::write(&jsonl_path, b"{\"type\":\"session\"}\n").unwrap();
        assert!(jsonl_path.is_file());

        db.save_session_metadata(&SaveSessionMetadataInput {
            id: "sess-del".to_string(),
            name: Some("Delete Me".to_string()),
            session_file_path: Some(jsonl_path.to_string_lossy().to_string()),
            tags: Some(vec!["tag1".to_string(), "tag2".to_string()]),
            ..Default::default()
        })
        .unwrap();

        db.save_node_positions(
            "sess-del",
            "LR",
            &HashMap::from([(
                "node-1".to_string(),
                NodePosition {
                    x: 10.0,
                    y: 20.0,
                    pinned: true,
                },
            )]),
        )
        .unwrap();

        assert!(db.delete_session_and_artifacts("sess-del").unwrap());
        assert!(!jsonl_path.exists());
        assert!(db.get_session("sess-del").unwrap().is_none());
        assert!(db.load_node_positions("sess-del", "LR").unwrap().is_empty());

        let remaining_tags: i64 = db
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM session_tags WHERE session_id = 'sess-del'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining_tags, 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn imports_legacy_pipan_metadata_json_once_and_renames_file() {
        let db = Database::open_in_memory().unwrap();
        let temp_dir = std::env::temp_dir().join(format!(
            "pipan-legacy-import-{}",
            crate::worktree::new_session_id()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let json_path = temp_dir.join("pipan-metadata.json");
        let migrated_path = temp_dir.join("pipan-metadata.json.migrated");

        let legacy_payload = serde_json::json!({
            "sessions": [
                {
                    "id": "legacy-1",
                    "name": "Legacy Session One",
                    "mode": "directory",
                    "cwd": "/repo/one",
                    "groupName": "Core",
                    "messageCount": 14,
                    "totalCostUsd": 0.42,
                    "tags": ["imported", "v1"]
                },
                {
                    "id": "legacy-2",
                    "title": "Legacy Session Two",
                    "mode": "headless",
                    "archived": true
                }
            ]
        });
        std::fs::write(
            &json_path,
            serde_json::to_string_pretty(&legacy_payload).unwrap(),
        )
        .unwrap();

        let count = db.import_legacy_metadata_json(&json_path).unwrap();
        assert_eq!(count, 2);
        assert!(!json_path.exists());
        assert!(migrated_path.is_file());

        let s1 = db.get_session("legacy-1").unwrap().unwrap();
        assert_eq!(s1.name, "Legacy Session One");
        assert_eq!(s1.group_name.as_deref(), Some("Core"));
        assert_eq!(s1.message_count, 14);
        assert_eq!(s1.tags, vec!["imported".to_string(), "v1".to_string()]);

        let s2 = db.get_session("legacy-2").unwrap().unwrap();
        assert_eq!(s2.name, "Legacy Session Two");
        assert!(s2.archived);

        // Second call is a no-op since pipan-metadata.json was renamed
        let count_again = db.import_legacy_metadata_json(&json_path).unwrap();
        assert_eq!(count_again, 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
