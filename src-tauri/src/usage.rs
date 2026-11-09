//! Cost & Usage Analytics backend (`usage.rs`).
//!
//! Indexes Pi session transcripts from `~/.pi/agent/sessions/**/*.jsonl` and
//! `<app_local_data_dir>/sessions/**/*.jsonl` into PiPan's local SQLite `usage_ledger`.
//! Uses `(path, mtime, byte_offset)` checkpoints for incremental rescans and
//! deduplicates entries via the `(session_id, message_id)` primary key.
//!
//! Provides SQL aggregations for Day, Week, Month, and Year timeframes, model & provider
//! breakdowns, workspace/session breakdowns, and overall KPI totals.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tauri::{AppHandle, Manager, State};

use crate::db::Database;
use crate::models::resolve_pi_agent_dir;
use crate::storage::now_unix_secs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageLedgerRecord {
    pub session_id: String,
    pub message_id: String,
    pub timestamp: i64,
    pub provider: String,
    pub model: String,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub cost_usd: f64,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordUsageInput {
    pub session_id: String,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub timestamp: Option<i64>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub input: Option<i64>,
    #[serde(default)]
    pub output: Option<i64>,
    #[serde(default)]
    pub cache_read: Option<i64>,
    #[serde(default)]
    pub cache_write: Option<i64>,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageBucket {
    pub key: String,
    pub label: String,
    pub start_timestamp: i64,
    pub end_timestamp: i64,
    pub cost_usd: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub total_tokens: i64,
    pub message_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayAnalytics {
    pub hourly_today: Vec<UsageBucket>,
    pub daily_30_days: Vec<UsageBucket>,
    pub total_cost_usd: f64,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WeekAnalytics {
    pub daily_this_week: Vec<UsageBucket>,
    pub weekly_12_weeks: Vec<UsageBucket>,
    pub total_cost_usd: f64,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MonthAnalytics {
    pub daily_this_month: Vec<UsageBucket>,
    pub monthly_12_months: Vec<UsageBucket>,
    pub total_cost_usd: f64,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct YearAnalytics {
    pub monthly_this_year: Vec<UsageBucket>,
    pub yearly_all: Vec<UsageBucket>,
    pub total_cost_usd: f64,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsageBreakdown {
    pub provider: String,
    pub model: String,
    pub cost_usd: f64,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub message_count: i64,
    pub percent_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsageBreakdown {
    pub session_id: String,
    pub session_name: String,
    pub cwd: Option<String>,
    pub cost_usd: f64,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub message_count: i64,
    pub last_active_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotals {
    pub total_spend_usd: f64,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub total_sessions: i64,
    pub active_sessions: i64,
    pub avg_cost_per_session: f64,
    pub total_messages: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageAnalytics {
    pub totals: UsageTotals,
    pub day: DayAnalytics,
    pub week: WeekAnalytics,
    pub month: MonthAnalytics,
    pub year: YearAnalytics,
    pub by_model: Vec<ModelUsageBreakdown>,
    pub by_session: Vec<SessionUsageBreakdown>,
    pub scanned_files_count: usize,
    pub newly_indexed_count: usize,
}

type UsageTotalsRow = (
    Option<f64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    i64,
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileCheckpoint {
    pub mtime: i64,
    pub byte_offset: u64,
}

fn normalize_ts(ts: i64) -> i64 {
    if ts > 10_000_000_000 { ts / 1000 } else { ts }
}

fn extract_session_id_from_path(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown_session".to_string())
}

/// Parses a single JSON line from a session `.jsonl` transcript.
/// Returns a `UsageLedgerRecord` if usage information is present.
pub fn parse_transcript_line(
    raw_line: &str,
    fallback_session_id: &str,
    fallback_ts: i64,
    fallback_cwd: Option<&str>,
    line_offset: u64,
) -> Option<UsageLedgerRecord> {
    let trimmed = raw_line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let val: Value = serde_json::from_str(trimmed).ok()?;
    let obj = val.as_object()?;

    // If this line is a session header, it does not have usage
    let line_type = obj.get("type").and_then(Value::as_str);
    if line_type == Some("session") || line_type == Some("init") {
        return None;
    }

    // Inspect either obj directly or obj["message"]
    let msg_obj = obj.get("message").and_then(Value::as_object).unwrap_or(obj);

    let raw_usage = msg_obj
        .get("usage")
        .or_else(|| obj.get("usage"))
        .and_then(Value::as_object)?;

    let input = raw_usage
        .get("input")
        .or_else(|| raw_usage.get("prompt_tokens"))
        .or_else(|| raw_usage.get("inputTokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0);

    let output = raw_usage
        .get("output")
        .or_else(|| raw_usage.get("completion_tokens"))
        .or_else(|| raw_usage.get("outputTokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0);

    let cache_read = raw_usage
        .get("cacheRead")
        .or_else(|| raw_usage.get("cache_read"))
        .or_else(|| raw_usage.get("cacheReadTokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0);

    let cache_write = raw_usage
        .get("cacheWrite")
        .or_else(|| raw_usage.get("cache_write"))
        .or_else(|| raw_usage.get("cacheWriteTokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0);

    let cost_usd = raw_usage
        .get("cost")
        .and_then(|c| {
            if let Some(total) = c.get("total").and_then(Value::as_f64) {
                Some(total)
            } else {
                c.as_f64()
            }
        })
        .or_else(|| raw_usage.get("cost_usd").and_then(Value::as_f64))
        .or_else(|| msg_obj.get("cost").and_then(Value::as_f64))
        .unwrap_or(0.0);

    if input == 0 && output == 0 && cache_read == 0 && cache_write == 0 && cost_usd <= 0.0 {
        return None;
    }

    let session_id = obj
        .get("sessionId")
        .or_else(|| obj.get("session_id"))
        .or_else(|| msg_obj.get("sessionId"))
        .or_else(|| msg_obj.get("session_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| fallback_session_id.to_string());

    let message_id = msg_obj
        .get("id")
        .or_else(|| msg_obj.get("messageId"))
        .or_else(|| msg_obj.get("message_id"))
        .or_else(|| obj.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}-byte-{}", session_id, line_offset));

    let timestamp = msg_obj
        .get("timestamp")
        .or_else(|| obj.get("timestamp"))
        .and_then(Value::as_i64)
        .map(normalize_ts)
        .unwrap_or(fallback_ts);

    let raw_model = msg_obj
        .get("model")
        .or_else(|| msg_obj.get("modelId"))
        .or_else(|| msg_obj.get("model_id"))
        .or_else(|| obj.get("model"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let raw_provider = msg_obj
        .get("provider")
        .or_else(|| obj.get("provider"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let (provider, model) = match (raw_provider, raw_model) {
        (Some(p), Some(m)) => (p.to_string(), m.to_string()),
        (None, Some(m)) => {
            if let Some((p, clean_m)) = m.split_once('/') {
                (p.to_string(), clean_m.to_string())
            } else {
                ("unknown".to_string(), m.to_string())
            }
        }
        (Some(p), None) => (p.to_string(), "unknown".to_string()),
        (None, None) => ("unknown".to_string(), "unknown".to_string()),
    };

    let cwd = obj
        .get("cwd")
        .or_else(|| msg_obj.get("cwd"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| fallback_cwd.map(str::to_string));

    Some(UsageLedgerRecord {
        session_id,
        message_id,
        timestamp,
        provider,
        model,
        input: input.max(0),
        output: output.max(0),
        cache_read: cache_read.max(0),
        cache_write: cache_write.max(0),
        cost_usd: if cost_usd.is_finite() && cost_usd >= 0.0 {
            cost_usd
        } else {
            0.0
        },
        cwd,
    })
}

/// Scans a single session `.jsonl` file incrementally starting from checkpoint `byte_offset`.
pub fn scan_transcript_file(
    path: &Path,
    checkpoint: Option<FileCheckpoint>,
) -> Result<(Vec<UsageLedgerRecord>, FileCheckpoint), String> {
    let mut file =
        File::open(path).map_err(|e| format!("Failed to open {}: {e}", path.display()))?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    let len = meta.len();
    let mtime = meta
        .modified()
        .and_then(|t| t.duration_since(UNIX_EPOCH).map_err(std::io::Error::other))
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let start_offset = match checkpoint {
        Some(cp) if cp.mtime == mtime && cp.byte_offset == len => {
            // Unchanged file
            return Ok((Vec::new(), cp));
        }
        Some(cp) if cp.byte_offset <= len => cp.byte_offset,
        _ => 0, // Truncated or new file
    };

    file.seek(SeekFrom::Start(start_offset))
        .map_err(|e| format!("Seek failed in {}: {e}", path.display()))?;

    let fallback_session_id = extract_session_id_from_path(path);
    let mut reader = BufReader::new(file);
    let mut current_offset = start_offset;
    let mut records = Vec::new();
    let mut line_buf = String::new();
    let mut session_cwd: Option<String> = None;

    loop {
        line_buf.clear();
        let bytes_read = reader
            .read_line(&mut line_buf)
            .map_err(|e| format!("Read line failed in {}: {e}", path.display()))?;
        if bytes_read == 0 {
            break;
        }

        let line_offset = current_offset;
        current_offset += bytes_read as u64;

        let trimmed = line_buf.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check if line defines session metadata/cwd
        if let Ok(v) = serde_json::from_str::<Value>(trimmed)
            && let Some(obj) = v.as_object()
            && let Some(c) = obj.get("cwd").and_then(Value::as_str)
        {
            session_cwd = Some(c.to_string());
        }

        if let Some(record) = parse_transcript_line(
            trimmed,
            &fallback_session_id,
            mtime,
            session_cwd.as_deref(),
            line_offset,
        ) {
            records.push(record);
        }
    }

    Ok((
        records,
        FileCheckpoint {
            mtime,
            byte_offset: current_offset,
        },
    ))
}

/// Recursively discovers `.jsonl` files in a directory up to a max depth.
pub fn discover_jsonl_files(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, max: usize, out: &mut Vec<PathBuf>) {
        if depth > max {
            return;
        }
        let Ok(read_dir) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || name == "node_modules" {
                    continue;
                }
                walk(&path, depth + 1, max, out);
            } else if path.is_file()
                && let Some(ext) = path.extension()
                && ext.eq_ignore_ascii_case("jsonl")
            {
                out.push(path);
            }
        }
    }

    let mut out = Vec::new();
    if root.exists() && root.is_dir() {
        walk(root, 0, max_depth, &mut out);
    }
    out
}

/// Standard paths where Pi sessions and PiPan transcripts live.
pub fn default_scan_directories(app_opt: Option<&AppHandle>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // 1. ~/.pi/agent/sessions
    if let Some(agent_dir) = resolve_pi_agent_dir() {
        let pi_sessions = agent_dir.join("sessions");
        if pi_sessions.exists() {
            dirs.push(pi_sessions);
        }
    }

    // 2. <app_local_data_dir>/sessions
    if let Some(app) = app_opt
        && let Ok(data_dir) = app.path().app_local_data_dir()
    {
        let local_sessions = data_dir.join("sessions");
        if local_sessions.exists() {
            dirs.push(local_sessions);
        }
    }

    // 3. Fallback temp session folder
    let temp_sessions = std::env::temp_dir().join("pipan-sessions");
    if temp_sessions.exists() {
        dirs.push(temp_sessions);
    }

    dirs
}

impl Database {
    pub fn get_usage_checkpoint(&self, path: &str) -> Result<Option<FileCheckpoint>, String> {
        let conn = self.lock();
        let cp = conn
            .query_row(
                "SELECT mtime, byte_offset FROM usage_file_checkpoints WHERE path = ?1",
                params![path],
                |row| {
                    Ok(FileCheckpoint {
                        mtime: row.get(0)?,
                        byte_offset: row.get::<_, i64>(1)? as u64,
                    })
                },
            )
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(cp)
    }

    pub fn set_usage_checkpoint(
        &self,
        path: &str,
        mtime: i64,
        byte_offset: u64,
    ) -> Result<(), String> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO usage_file_checkpoints (path, mtime, byte_offset) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET mtime = excluded.mtime, byte_offset = excluded.byte_offset",
            params![path, mtime, byte_offset as i64],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn insert_usage_records(&self, records: &[UsageLedgerRecord]) -> Result<usize, String> {
        if records.is_empty() {
            return Ok(0);
        }
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let mut inserted_count = 0;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO usage_ledger (
                        session_id, message_id, timestamp, provider, model,
                        input, output, cache_read, cache_write, cost_usd, cwd
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                    ON CONFLICT(session_id, message_id) DO NOTHING",
                )
                .map_err(|e| e.to_string())?;

            for r in records {
                let affected = stmt
                    .execute(params![
                        r.session_id,
                        r.message_id,
                        r.timestamp,
                        r.provider,
                        r.model,
                        r.input,
                        r.output,
                        r.cache_read,
                        r.cache_write,
                        r.cost_usd,
                        r.cwd
                    ])
                    .map_err(|e| e.to_string())?;
                if affected > 0 {
                    inserted_count += 1;
                }
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(inserted_count)
    }

    /// Scans a list of files or directories, parsing new lines and upserting into `usage_ledger`.
    pub fn scan_and_index_usage_files(
        &self,
        target_paths: &[PathBuf],
    ) -> Result<(usize, usize), String> {
        let mut files = Vec::new();
        for p in target_paths {
            if p.is_file() {
                if let Some(ext) = p.extension()
                    && ext.eq_ignore_ascii_case("jsonl")
                {
                    files.push(p.clone());
                }
            } else if p.is_dir() {
                files.extend(discover_jsonl_files(p, 10));
            }
        }

        files.sort();
        files.dedup();

        let scanned_count = files.len();
        let mut newly_indexed = 0;

        for file_path in &files {
            let path_str = file_path.to_string_lossy().to_string();
            let cp = self.get_usage_checkpoint(&path_str)?;
            if let Ok((records, new_cp)) = scan_transcript_file(file_path, cp) {
                if !records.is_empty() {
                    let count = self.insert_usage_records(&records)?;
                    newly_indexed += count;
                }
                let _ = self.set_usage_checkpoint(&path_str, new_cp.mtime, new_cp.byte_offset);
            }
        }

        Ok((scanned_count, newly_indexed))
    }

    pub fn query_usage_totals(&self) -> Result<UsageTotals, String> {
        let conn = self.lock();
        let (spend, total_toks, in_toks, out_toks, cr_toks, cw_toks, total_msgs): UsageTotalsRow =
            conn.query_row(
                "SELECT
                    SUM(cost_usd),
                    SUM(input + output + cache_read + cache_write),
                    SUM(input),
                    SUM(output),
                    SUM(cache_read),
                    SUM(cache_write),
                    COUNT(*)
                 FROM usage_ledger",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .map_err(|e| e.to_string())?;

        let ledger_sessions: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT session_id) FROM usage_ledger",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let sessions_table_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap_or(0);

        let active_sessions_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE status = 'running' OR status = 'starting'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let total_sessions = sessions_table_count.max(ledger_sessions);
        let total_spend_usd = spend.unwrap_or(0.0);
        let avg_cost = if total_sessions > 0 {
            total_spend_usd / total_sessions as f64
        } else {
            0.0
        };

        Ok(UsageTotals {
            total_spend_usd,
            total_tokens: total_toks.unwrap_or(0),
            input_tokens: in_toks.unwrap_or(0),
            output_tokens: out_toks.unwrap_or(0),
            cache_read_tokens: cr_toks.unwrap_or(0),
            cache_write_tokens: cw_toks.unwrap_or(0),
            total_sessions,
            active_sessions: active_sessions_count,
            avg_cost_per_session: avg_cost,
            total_messages: total_msgs,
        })
    }

    pub fn query_model_breakdown(&self) -> Result<Vec<ModelUsageBreakdown>, String> {
        let conn = self.lock();
        let total_spend: f64 = conn
            .query_row(
                "SELECT COALESCE(SUM(cost_usd), 0.0) FROM usage_ledger",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0.0);

        let mut stmt = conn
            .prepare(
                "SELECT
                    provider,
                    model,
                    SUM(cost_usd) as spend,
                    SUM(input + output + cache_read + cache_write) as total_t,
                    SUM(input) as in_t,
                    SUM(output) as out_t,
                    SUM(cache_read) as cr_t,
                    SUM(cache_write) as cw_t,
                    COUNT(*) as msgs
                 FROM usage_ledger
                 GROUP BY provider, model
                 ORDER BY spend DESC, total_t DESC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                let cost_usd: f64 = row.get(2)?;
                let percent = if total_spend > 0.0 {
                    (cost_usd / total_spend) * 100.0
                } else {
                    0.0
                };
                Ok(ModelUsageBreakdown {
                    provider: row.get(0)?,
                    model: row.get(1)?,
                    cost_usd,
                    total_tokens: row.get(3)?,
                    input_tokens: row.get(4)?,
                    output_tokens: row.get(5)?,
                    cache_read_tokens: row.get(6)?,
                    cache_write_tokens: row.get(7)?,
                    message_count: row.get(8)?,
                    percent_cost: percent,
                })
            })
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn query_session_breakdown(&self) -> Result<Vec<SessionUsageBreakdown>, String> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT
                    u.session_id,
                    COALESCE(s.name, s.title, u.session_id) as session_name,
                    COALESCE(u.cwd, s.cwd) as cwd,
                    SUM(u.cost_usd) as spend,
                    SUM(u.input + u.output + u.cache_read + u.cache_write) as total_t,
                    SUM(u.input) as in_t,
                    SUM(u.output) as out_t,
                    COUNT(*) as msgs,
                    MAX(u.timestamp) as last_active
                 FROM usage_ledger u
                 LEFT JOIN sessions s ON s.id = u.session_id
                 GROUP BY u.session_id
                 ORDER BY spend DESC, last_active DESC
                 LIMIT 50",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                Ok(SessionUsageBreakdown {
                    session_id: row.get(0)?,
                    session_name: row.get(1)?,
                    cwd: row.get(2)?,
                    cost_usd: row.get(3)?,
                    total_tokens: row.get(4)?,
                    input_tokens: row.get(5)?,
                    output_tokens: row.get(6)?,
                    message_count: row.get(7)?,
                    last_active_ts: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn query_day_analytics(&self, now: i64) -> Result<DayAnalytics, String> {
        let conn = self.lock();
        let today_midnight = now - (now % 86400);

        // 1. Hourly for today (24 slots)
        let mut hourly_map = HashMap::new();
        {
            let mut stmt = conn
                .prepare(
                    "SELECT
                        strftime('%H:00', timestamp, 'unixepoch') as hr,
                        SUM(cost_usd),
                        SUM(input + output + cache_read + cache_write),
                        SUM(input),
                        SUM(output),
                        SUM(cache_read),
                        SUM(cache_write),
                        COUNT(*)
                     FROM usage_ledger
                     WHERE timestamp >= ?1
                     GROUP BY hr",
                )
                .map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map(params![today_midnight], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        (
                            r.get::<_, f64>(1)?,
                            r.get::<_, i64>(2)?,
                            r.get::<_, i64>(3)?,
                            r.get::<_, i64>(4)?,
                            r.get::<_, i64>(5)?,
                            r.get::<_, i64>(6)?,
                            r.get::<_, i64>(7)?,
                        ),
                    ))
                })
                .map_err(|e| e.to_string())?;

            for item in rows.flatten() {
                hourly_map.insert(item.0, item.1);
            }
        }

        let mut hourly_today = Vec::with_capacity(24);
        let mut day_cost = 0.0;
        let mut day_toks = 0;
        let mut day_in = 0;
        let mut day_out = 0;
        let mut day_cr = 0;
        let mut day_cw = 0;

        for h in 0..24 {
            let label = format!("{:02}:00", h);
            let start_ts = today_midnight + h * 3600;
            let end_ts = start_ts + 3599;
            let (cost, total_t, in_t, out_t, cr_t, cw_t, msgs) = hourly_map
                .get(&label)
                .copied()
                .unwrap_or((0.0, 0, 0, 0, 0, 0, 0));

            day_cost += cost;
            day_toks += total_t;
            day_in += in_t;
            day_out += out_t;
            day_cr += cr_t;
            day_cw += cw_t;

            hourly_today.push(UsageBucket {
                key: format!("today-{}", label),
                label,
                start_timestamp: start_ts,
                end_timestamp: end_ts,
                cost_usd: cost,
                input_tokens: in_t,
                output_tokens: out_t,
                cache_read_tokens: cr_t,
                cache_write_tokens: cw_t,
                total_tokens: total_t,
                message_count: msgs,
            });
        }

        // 2. Daily for last 30 days
        let thirty_days_ago = today_midnight - (29 * 86400);
        let mut daily_map = HashMap::new();
        {
            let mut stmt = conn
                .prepare(
                    "SELECT
                        strftime('%Y-%m-%d', timestamp, 'unixepoch') as day_key,
                        SUM(cost_usd),
                        SUM(input + output + cache_read + cache_write),
                        SUM(input),
                        SUM(output),
                        SUM(cache_read),
                        SUM(cache_write),
                        COUNT(*)
                     FROM usage_ledger
                     WHERE timestamp >= ?1
                     GROUP BY day_key",
                )
                .map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map(params![thirty_days_ago], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        (
                            r.get::<_, f64>(1)?,
                            r.get::<_, i64>(2)?,
                            r.get::<_, i64>(3)?,
                            r.get::<_, i64>(4)?,
                            r.get::<_, i64>(5)?,
                            r.get::<_, i64>(6)?,
                            r.get::<_, i64>(7)?,
                        ),
                    ))
                })
                .map_err(|e| e.to_string())?;

            for item in rows.flatten() {
                daily_map.insert(item.0, item.1);
            }
        }

        let mut daily_30_days = Vec::with_capacity(30);
        for d in 0..30 {
            let day_start = thirty_days_ago + d * 86400;
            let day_end = day_start + 86399;
            let date_key = format_date_iso(day_start);
            let label = format_date_short(day_start);

            let (cost, total_t, in_t, out_t, cr_t, cw_t, msgs) = daily_map
                .get(&date_key)
                .copied()
                .unwrap_or((0.0, 0, 0, 0, 0, 0, 0));

            daily_30_days.push(UsageBucket {
                key: date_key,
                label,
                start_timestamp: day_start,
                end_timestamp: day_end,
                cost_usd: cost,
                input_tokens: in_t,
                output_tokens: out_t,
                cache_read_tokens: cr_t,
                cache_write_tokens: cw_t,
                total_tokens: total_t,
                message_count: msgs,
            });
        }

        Ok(DayAnalytics {
            hourly_today,
            daily_30_days,
            total_cost_usd: day_cost,
            total_tokens: day_toks,
            input_tokens: day_in,
            output_tokens: day_out,
            cache_read_tokens: day_cr,
            cache_write_tokens: day_cw,
        })
    }

    pub fn query_week_analytics(&self, now: i64) -> Result<WeekAnalytics, String> {
        let conn = self.lock();
        let today_midnight = now - (now % 86400);

        // 1. Daily for current week (last 7 days)
        let week_start = today_midnight - (6 * 86400);
        let mut daily_map = HashMap::new();
        {
            let mut stmt = conn
                .prepare(
                    "SELECT
                        strftime('%Y-%m-%d', timestamp, 'unixepoch') as day_key,
                        SUM(cost_usd),
                        SUM(input + output + cache_read + cache_write),
                        SUM(input),
                        SUM(output),
                        SUM(cache_read),
                        SUM(cache_write),
                        COUNT(*)
                     FROM usage_ledger
                     WHERE timestamp >= ?1
                     GROUP BY day_key",
                )
                .map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map(params![week_start], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        (
                            r.get::<_, f64>(1)?,
                            r.get::<_, i64>(2)?,
                            r.get::<_, i64>(3)?,
                            r.get::<_, i64>(4)?,
                            r.get::<_, i64>(5)?,
                            r.get::<_, i64>(6)?,
                            r.get::<_, i64>(7)?,
                        ),
                    ))
                })
                .map_err(|e| e.to_string())?;

            for item in rows.flatten() {
                daily_map.insert(item.0, item.1);
            }
        }

        let mut daily_this_week = Vec::with_capacity(7);
        let mut week_cost = 0.0;
        let mut week_toks = 0;
        let mut week_in = 0;
        let mut week_out = 0;
        let mut week_cr = 0;
        let mut week_cw = 0;

        for d in 0..7 {
            let day_start = week_start + d * 86400;
            let day_end = day_start + 86399;
            let date_key = format_date_iso(day_start);
            let label = format_day_of_week(day_start);

            let (cost, total_t, in_t, out_t, cr_t, cw_t, msgs) = daily_map
                .get(&date_key)
                .copied()
                .unwrap_or((0.0, 0, 0, 0, 0, 0, 0));

            week_cost += cost;
            week_toks += total_t;
            week_in += in_t;
            week_out += out_t;
            week_cr += cr_t;
            week_cw += cw_t;

            daily_this_week.push(UsageBucket {
                key: date_key,
                label,
                start_timestamp: day_start,
                end_timestamp: day_end,
                cost_usd: cost,
                input_tokens: in_t,
                output_tokens: out_t,
                cache_read_tokens: cr_t,
                cache_write_tokens: cw_t,
                total_tokens: total_t,
                message_count: msgs,
            });
        }

        // 2. Weekly for last 12 weeks
        let twelve_weeks_ago = today_midnight - (12 * 7 * 86400);
        let mut weekly_12_weeks = Vec::with_capacity(12);
        for w in 0..12 {
            let start_ts = twelve_weeks_ago + w * 7 * 86400;
            let end_ts = start_ts + (7 * 86400) - 1;
            let label = format!("Wk {}", w + 1);

            let row: UsageTotalsRow = conn
                .query_row(
                    "SELECT
                        SUM(cost_usd),
                        SUM(input + output + cache_read + cache_write),
                        SUM(input),
                        SUM(output),
                        SUM(cache_read),
                        SUM(cache_write),
                        COUNT(*)
                     FROM usage_ledger
                     WHERE timestamp >= ?1 AND timestamp <= ?2",
                    params![start_ts, end_ts],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                        ))
                    },
                )
                .unwrap_or((None, None, None, None, None, None, 0));

            weekly_12_weeks.push(UsageBucket {
                key: format!("week-{}", w + 1),
                label,
                start_timestamp: start_ts,
                end_timestamp: end_ts,
                cost_usd: row.0.unwrap_or(0.0),
                input_tokens: row.2.unwrap_or(0),
                output_tokens: row.3.unwrap_or(0),
                cache_read_tokens: row.4.unwrap_or(0),
                cache_write_tokens: row.5.unwrap_or(0),
                total_tokens: row.1.unwrap_or(0),
                message_count: row.6,
            });
        }

        Ok(WeekAnalytics {
            daily_this_week,
            weekly_12_weeks,
            total_cost_usd: week_cost,
            total_tokens: week_toks,
            input_tokens: week_in,
            output_tokens: week_out,
            cache_read_tokens: week_cr,
            cache_write_tokens: week_cw,
        })
    }

    pub fn query_month_analytics(&self, now: i64) -> Result<MonthAnalytics, String> {
        let conn = self.lock();
        let today_midnight = now - (now % 86400);

        // 1. Daily for current month (last 30 days)
        let thirty_days_ago = today_midnight - (29 * 86400);
        let mut daily_map = HashMap::new();
        {
            let mut stmt = conn
                .prepare(
                    "SELECT
                        strftime('%Y-%m-%d', timestamp, 'unixepoch') as day_key,
                        SUM(cost_usd),
                        SUM(input + output + cache_read + cache_write),
                        SUM(input),
                        SUM(output),
                        SUM(cache_read),
                        SUM(cache_write),
                        COUNT(*)
                     FROM usage_ledger
                     WHERE timestamp >= ?1
                     GROUP BY day_key",
                )
                .map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map(params![thirty_days_ago], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        (
                            r.get::<_, f64>(1)?,
                            r.get::<_, i64>(2)?,
                            r.get::<_, i64>(3)?,
                            r.get::<_, i64>(4)?,
                            r.get::<_, i64>(5)?,
                            r.get::<_, i64>(6)?,
                            r.get::<_, i64>(7)?,
                        ),
                    ))
                })
                .map_err(|e| e.to_string())?;

            for item in rows.flatten() {
                daily_map.insert(item.0, item.1);
            }
        }

        let mut daily_this_month = Vec::with_capacity(30);
        let mut month_cost = 0.0;
        let mut month_toks = 0;
        let mut month_in = 0;
        let mut month_out = 0;
        let mut month_cr = 0;
        let mut month_cw = 0;

        for d in 0..30 {
            let day_start = thirty_days_ago + d * 86400;
            let day_end = day_start + 86399;
            let date_key = format_date_iso(day_start);
            let label = format_date_short(day_start);

            let (cost, total_t, in_t, out_t, cr_t, cw_t, msgs) = daily_map
                .get(&date_key)
                .copied()
                .unwrap_or((0.0, 0, 0, 0, 0, 0, 0));

            month_cost += cost;
            month_toks += total_t;
            month_in += in_t;
            month_out += out_t;
            month_cr += cr_t;
            month_cw += cw_t;

            daily_this_month.push(UsageBucket {
                key: date_key,
                label,
                start_timestamp: day_start,
                end_timestamp: day_end,
                cost_usd: cost,
                input_tokens: in_t,
                output_tokens: out_t,
                cache_read_tokens: cr_t,
                cache_write_tokens: cw_t,
                total_tokens: total_t,
                message_count: msgs,
            });
        }

        // 2. Monthly for last 12 months (30-day blocks)
        let twelve_months_ago = today_midnight - (12 * 30 * 86400);
        let mut monthly_12_months = Vec::with_capacity(12);
        for m in 0..12 {
            let start_ts = twelve_months_ago + m * 30 * 86400;
            let end_ts = start_ts + (30 * 86400) - 1;
            let label = format!("M{}", m + 1);

            let row: UsageTotalsRow = conn
                .query_row(
                    "SELECT
                        SUM(cost_usd),
                        SUM(input + output + cache_read + cache_write),
                        SUM(input),
                        SUM(output),
                        SUM(cache_read),
                        SUM(cache_write),
                        COUNT(*)
                     FROM usage_ledger
                     WHERE timestamp >= ?1 AND timestamp <= ?2",
                    params![start_ts, end_ts],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                        ))
                    },
                )
                .unwrap_or((None, None, None, None, None, None, 0));

            monthly_12_months.push(UsageBucket {
                key: format!("month-{}", m + 1),
                label,
                start_timestamp: start_ts,
                end_timestamp: end_ts,
                cost_usd: row.0.unwrap_or(0.0),
                input_tokens: row.2.unwrap_or(0),
                output_tokens: row.3.unwrap_or(0),
                cache_read_tokens: row.4.unwrap_or(0),
                cache_write_tokens: row.5.unwrap_or(0),
                total_tokens: row.1.unwrap_or(0),
                message_count: row.6,
            });
        }

        Ok(MonthAnalytics {
            daily_this_month,
            monthly_12_months,
            total_cost_usd: month_cost,
            total_tokens: month_toks,
            input_tokens: month_in,
            output_tokens: month_out,
            cache_read_tokens: month_cr,
            cache_write_tokens: month_cw,
        })
    }

    pub fn query_year_analytics(&self, now: i64) -> Result<YearAnalytics, String> {
        let conn = self.lock();
        let today_midnight = now - (now % 86400);

        // 1. Monthly for current year (last 12 months)
        let twelve_months_ago = today_midnight - (12 * 30 * 86400);
        let mut monthly_this_year = Vec::with_capacity(12);
        let mut year_cost = 0.0;
        let mut year_toks = 0;
        let mut year_in = 0;
        let mut year_out = 0;
        let mut year_cr = 0;
        let mut year_cw = 0;

        for m in 0..12 {
            let start_ts = twelve_months_ago + m * 30 * 86400;
            let end_ts = start_ts + (30 * 86400) - 1;
            let label = format!("M{}", m + 1);

            let row: UsageTotalsRow = conn
                .query_row(
                    "SELECT
                        SUM(cost_usd),
                        SUM(input + output + cache_read + cache_write),
                        SUM(input),
                        SUM(output),
                        SUM(cache_read),
                        SUM(cache_write),
                        COUNT(*)
                     FROM usage_ledger
                     WHERE timestamp >= ?1 AND timestamp <= ?2",
                    params![start_ts, end_ts],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                        ))
                    },
                )
                .unwrap_or((None, None, None, None, None, None, 0));

            let cost = row.0.unwrap_or(0.0);
            let total_t = row.1.unwrap_or(0);
            let in_t = row.2.unwrap_or(0);
            let out_t = row.3.unwrap_or(0);
            let cr_t = row.4.unwrap_or(0);
            let cw_t = row.5.unwrap_or(0);
            let msgs = row.6;

            year_cost += cost;
            year_toks += total_t;
            year_in += in_t;
            year_out += out_t;
            year_cr += cr_t;
            year_cw += cw_t;

            monthly_this_year.push(UsageBucket {
                key: format!("year-month-{}", m + 1),
                label,
                start_timestamp: start_ts,
                end_timestamp: end_ts,
                cost_usd: cost,
                input_tokens: in_t,
                output_tokens: out_t,
                cache_read_tokens: cr_t,
                cache_write_tokens: cw_t,
                total_tokens: total_t,
                message_count: msgs,
            });
        }

        // 2. Yearly (grouped by year strftime)
        let mut yearly_all = Vec::new();
        {
            let mut stmt = conn
                .prepare(
                    "SELECT
                        strftime('%Y', timestamp, 'unixepoch') as yr,
                        MIN(timestamp),
                        MAX(timestamp),
                        SUM(cost_usd),
                        SUM(input + output + cache_read + cache_write),
                        SUM(input),
                        SUM(output),
                        SUM(cache_read),
                        SUM(cache_write),
                        COUNT(*)
                     FROM usage_ledger
                     GROUP BY yr
                     ORDER BY yr ASC",
                )
                .map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map([], |r| {
                    let yr: String = r.get(0)?;
                    Ok(UsageBucket {
                        key: yr.clone(),
                        label: yr,
                        start_timestamp: r.get(1)?,
                        end_timestamp: r.get(2)?,
                        cost_usd: r.get(3)?,
                        total_tokens: r.get(4)?,
                        input_tokens: r.get(5)?,
                        output_tokens: r.get(6)?,
                        cache_read_tokens: r.get(7)?,
                        cache_write_tokens: r.get(8)?,
                        message_count: r.get(9)?,
                    })
                })
                .map_err(|e| e.to_string())?;

            for item in rows.flatten() {
                yearly_all.push(item);
            }
        }

        Ok(YearAnalytics {
            monthly_this_year,
            yearly_all,
            total_cost_usd: year_cost,
            total_tokens: year_toks,
            input_tokens: year_in,
            output_tokens: year_out,
            cache_read_tokens: year_cr,
            cache_write_tokens: year_cw,
        })
    }

    pub fn compute_full_usage_analytics(
        &self,
        scanned_files_count: usize,
        newly_indexed_count: usize,
    ) -> Result<UsageAnalytics, String> {
        let now = now_unix_secs();
        let totals = self.query_usage_totals()?;
        let day = self.query_day_analytics(now)?;
        let week = self.query_week_analytics(now)?;
        let month = self.query_month_analytics(now)?;
        let year = self.query_year_analytics(now)?;
        let by_model = self.query_model_breakdown()?;
        let by_session = self.query_session_breakdown()?;

        Ok(UsageAnalytics {
            totals,
            day,
            week,
            month,
            year,
            by_model,
            by_session,
            scanned_files_count,
            newly_indexed_count,
        })
    }
}

// ---------------------------------------------------------------------------
// Date Helpers
// ---------------------------------------------------------------------------

fn format_date_iso(unix_secs: i64) -> String {
    // Days since unix epoch
    let days = unix_secs / 86400;
    let (y, m, d) = days_to_ymd(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn format_date_short(unix_secs: i64) -> String {
    let days = unix_secs / 86400;
    let (_, m, d) = days_to_ymd(days);
    format!("{:02}/{:02}", m, d)
}

fn format_day_of_week(unix_secs: i64) -> String {
    // 1970-01-01 was Thursday (index 4)
    let days = unix_secs / 86400;
    let dow = (days + 4) % 7;
    match dow {
        0 => "Sun",
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        _ => "Sat",
    }
    .to_string()
}

fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i32 + (era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ---------------------------------------------------------------------------
// Tauri Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_usage_analytics(
    db: State<'_, Database>,
    app: AppHandle,
) -> Result<UsageAnalytics, String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || {
        let scan_dirs = default_scan_directories(Some(&app));
        let (scanned_count, newly_indexed) =
            db.scan_and_index_usage_files(&scan_dirs).unwrap_or((0, 0));
        db.compute_full_usage_analytics(scanned_count, newly_indexed)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn rescan_usage_files(db: State<'_, Database>, app: AppHandle) -> Result<usize, String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || {
        let scan_dirs = default_scan_directories(Some(&app));
        let (_, newly_indexed) = db.scan_and_index_usage_files(&scan_dirs)?;
        Ok(newly_indexed)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn record_usage_entry(
    db: State<'_, Database>,
    entry: RecordUsageInput,
) -> Result<(), String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || {
        let sid = entry.session_id.trim();
        if sid.is_empty() {
            return Err("session_id must not be empty".to_string());
        }
        let now = now_unix_secs();
        let mid = entry
            .message_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}-live-{}", sid, now));

        let record = UsageLedgerRecord {
            session_id: sid.to_string(),
            message_id: mid,
            timestamp: entry.timestamp.map(normalize_ts).unwrap_or(now),
            provider: entry
                .provider
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("unknown")
                .to_string(),
            model: entry
                .model
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("unknown")
                .to_string(),
            input: entry.input.unwrap_or(0).max(0),
            output: entry.output.unwrap_or(0).max(0),
            cache_read: entry.cache_read.unwrap_or(0).max(0),
            cache_write: entry.cache_write.unwrap_or(0).max(0),
            cost_usd: entry
                .cost_usd
                .filter(|c| c.is_finite() && *c >= 0.0)
                .unwrap_or(0.0),
            cwd: entry
                .cwd
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        };

        db.insert_usage_records(&[record])?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_transcript_line_extracts_usage_and_cost() {
        let json_line = r#"{"type":"message","id":"msg-1","role":"assistant","timestamp":1715000000000,"model":"claude-3-7-sonnet","provider":"anthropic","usage":{"input":1000,"output":250,"cacheRead":500,"cacheWrite":100,"cost":{"total":0.0125}}}"#;
        let record = parse_transcript_line(json_line, "session-abc", 1715000000, None, 0).unwrap();
        assert_eq!(record.session_id, "session-abc");
        assert_eq!(record.message_id, "msg-1");
        assert_eq!(record.timestamp, 1715000000);
        assert_eq!(record.provider, "anthropic");
        assert_eq!(record.model, "claude-3-7-sonnet");
        assert_eq!(record.input, 1000);
        assert_eq!(record.output, 250);
        assert_eq!(record.cache_read, 500);
        assert_eq!(record.cache_write, 100);
        assert!((record.cost_usd - 0.0125).abs() < 1e-6);
    }

    #[test]
    fn parse_transcript_line_wrapped_message_end() {
        let json_line = r#"{"type":"message_end","message":{"id":"msg-wrap","role":"assistant","timestamp":1715000500,"model":"openai/gpt-4.1","usage":{"input":800,"output":120,"cost":{"total":0.003}}}}"#;
        let record = parse_transcript_line(json_line, "session-xyz", 1715000000, None, 10).unwrap();
        assert_eq!(record.session_id, "session-xyz");
        assert_eq!(record.message_id, "msg-wrap");
        assert_eq!(record.provider, "openai");
        assert_eq!(record.model, "gpt-4.1");
        assert_eq!(record.input, 800);
        assert_eq!(record.output, 120);
        assert!((record.cost_usd - 0.003).abs() < 1e-6);
    }

    #[test]
    fn parse_transcript_line_ignores_non_usage_lines() {
        assert!(
            parse_transcript_line(r#"{"type":"session","id":"s1"}"#, "s1", 100, None, 0).is_none()
        );
        assert!(parse_transcript_line(r#"{"type":"turn_start"}"#, "s1", 100, None, 0).is_none());
        assert!(
            parse_transcript_line(r#"{"role":"user","content":"hello"}"#, "s1", 100, None, 0)
                .is_none()
        );
    }

    #[test]
    fn incremental_file_scanning_with_checkpoints() {
        let dir = std::env::temp_dir().join(format!("pipan-usage-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("sess-test.jsonl");

        {
            let mut f = File::create(&file_path).unwrap();
            writeln!(f, r#"{{"type":"session","id":"sess-test","cwd":"/repo"}}"#).unwrap();
            writeln!(
                f,
                r#"{{"type":"message","id":"m1","usage":{{"input":500,"output":100,"cost":{{"total":0.005}}}}}}"#
            )
            .unwrap();
        }

        let (records1, cp1) = scan_transcript_file(&file_path, None).unwrap();
        assert_eq!(records1.len(), 1);
        assert_eq!(records1[0].message_id, "m1");
        assert_eq!(records1[0].input, 500);

        // Scan again with same checkpoint -> 0 new records
        let (records2, cp2) = scan_transcript_file(&file_path, Some(cp1)).unwrap();
        assert_eq!(records2.len(), 0);
        assert_eq!(cp2.byte_offset, cp1.byte_offset);

        // Append line
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&file_path)
                .unwrap();
            writeln!(
                f,
                r#"{{"type":"message","id":"m2","usage":{{"input":300,"output":50,"cost":{{"total":0.002}}}}}}"#
            )
            .unwrap();
        }

        let (records3, cp3) = scan_transcript_file(&file_path, Some(cp1)).unwrap();
        assert_eq!(records3.len(), 1);
        assert_eq!(records3[0].message_id, "m2");
        assert_eq!(records3[0].input, 300);
        assert!(cp3.byte_offset > cp1.byte_offset);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn database_usage_ledger_deduplication_and_analytics() {
        let db = Database::open_in_memory().unwrap();

        let r1 = UsageLedgerRecord {
            session_id: "s1".to_string(),
            message_id: "m1".to_string(),
            timestamp: 1715000000,
            provider: "anthropic".to_string(),
            model: "claude-3-7-sonnet".to_string(),
            input: 1000,
            output: 200,
            cache_read: 100,
            cache_write: 50,
            cost_usd: 0.015,
            cwd: Some("/repo/a".to_string()),
        };

        let r2 = UsageLedgerRecord {
            session_id: "s1".to_string(),
            message_id: "m2".to_string(),
            timestamp: 1715003600,
            provider: "anthropic".to_string(),
            model: "claude-3-7-sonnet".to_string(),
            input: 500,
            output: 100,
            cache_read: 50,
            cache_write: 0,
            cost_usd: 0.005,
            cwd: Some("/repo/a".to_string()),
        };

        let inserted = db.insert_usage_records(&[r1.clone(), r2]).unwrap();
        assert_eq!(inserted, 2);

        // Inserting r1 again should do nothing (ON CONFLICT DO NOTHING)
        let inserted_again = db.insert_usage_records(&[r1]).unwrap();
        assert_eq!(inserted_again, 0);

        let totals = db.query_usage_totals().unwrap();
        assert_eq!(totals.total_messages, 2);
        assert_eq!(totals.total_tokens, 2000); // (1000+200+100+50) + (500+100+50+0) = 1350 + 650 = 2000
        assert_eq!(totals.input_tokens, 1500);
        assert_eq!(totals.output_tokens, 300);
        assert_eq!(totals.cache_read_tokens, 150);
        assert_eq!(totals.cache_write_tokens, 50);
        assert!((totals.total_spend_usd - 0.020).abs() < 1e-6);

        let models = db.query_model_breakdown().unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model, "claude-3-7-sonnet");
        assert_eq!(models[0].provider, "anthropic");
        assert_eq!(models[0].message_count, 2);
        assert!((models[0].cost_usd - 0.020).abs() < 1e-6);

        let sessions = db.query_session_breakdown().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "s1");
        assert_eq!(sessions[0].message_count, 2);
        assert!((sessions[0].cost_usd - 0.020).abs() < 1e-6);

        let analytics = db.compute_full_usage_analytics(1, 2).unwrap();
        assert_eq!(analytics.scanned_files_count, 1);
        assert_eq!(analytics.newly_indexed_count, 2);
        assert_eq!(analytics.totals.total_messages, 2);
        assert_eq!(analytics.day.hourly_today.len(), 24);
        assert_eq!(analytics.day.daily_30_days.len(), 30);
        assert_eq!(analytics.week.daily_this_week.len(), 7);
        assert_eq!(analytics.week.weekly_12_weeks.len(), 12);
        assert_eq!(analytics.month.daily_this_month.len(), 30);
        assert_eq!(analytics.month.monthly_12_months.len(), 12);
        assert_eq!(analytics.year.monthly_this_year.len(), 12);
    }
}
