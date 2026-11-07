//! Workspace File Explorer backend commands (`fs_explorer.rs`).
//!
//! Provides directory listing (respecting `.gitignore` and hidden file toggles),
//! `git status --porcelain=v1` badge annotation (`M`, `A`, `D`, `R`, `?`),
//! and safe workspace file + `git diff` preview reading.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::UNIX_EPOCH;
use tokio::process::Command;

use crate::pi_rpc::get_augmented_path;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub const DEFAULT_PREVIEW_MAX_BYTES: usize = 256 * 1024; // 256 KiB
pub const MIN_PREVIEW_MAX_BYTES: usize = 1024; // 1 KiB
pub const MAX_PREVIEW_MAX_BYTES: usize = 4 * 1024 * 1024; // 4 MiB

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileEntry {
    pub name: String,
    pub path: String,
    pub relative_path: String,
    /// `"directory"` | `"file"` | `"symlink"`
    pub kind: String,
    pub is_dir: bool,
    pub is_hidden: bool,
    pub size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFilePreview {
    pub path: String,
    pub relative_path: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    pub size_bytes: u64,
    pub is_binary: bool,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_status: Option<String>,
}

fn clean_verbatim(raw: &str) -> String {
    raw.trim_start_matches(r"\\?\").to_string()
}

/// Validates `cwd` and resolves `relative` inside `cwd`, rejecting any `..` or root/prefix components.
pub fn resolve_safe_workspace_path(
    cwd: &Path,
    relative: Option<&str>,
) -> Result<(PathBuf, String), String> {
    if !cwd.exists() || !cwd.is_dir() {
        return Err(format!(
            "Workspace directory does not exist: {}",
            cwd.display()
        ));
    }

    let rel_raw = relative.unwrap_or("").trim();
    if rel_raw.is_empty() || rel_raw == "." || rel_raw == "/" {
        return Ok((cwd.to_path_buf(), String::new()));
    }

    let normalized_input = rel_raw.replace('\\', "/");
    let trimmed = normalized_input.trim_start_matches('/');
    let rel_path = Path::new(trimmed);

    let mut clean_parts: Vec<String> = Vec::new();
    for comp in rel_path.components() {
        match comp {
            Component::Normal(os) => {
                let s = os.to_string_lossy().to_string();
                if s.is_empty() {
                    continue;
                }
                clean_parts.push(s);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "Path '{}' is outside the workspace directory",
                    rel_raw
                ));
            }
        }
    }

    let mut joined = cwd.to_path_buf();
    for part in &clean_parts {
        joined.push(part);
    }

    // If the target exists on disk, verify its canonical path stays within canonical cwd
    if joined.exists()
        && let (Ok(canon_cwd), Ok(canon_target)) =
            (std::fs::canonicalize(cwd), std::fs::canonicalize(&joined))
        && !canon_target.starts_with(&canon_cwd)
    {
        return Err(format!(
            "Resolved path '{}' escapes workspace root",
            rel_raw
        ));
    }

    let normalized_rel = clean_parts.join("/");
    Ok((joined, normalized_rel))
}

#[derive(Debug, Clone)]
struct GitignoreRule {
    pattern: String,
    dir_only: bool,
    anchored: bool,
    negated: bool,
}

/// Lightweight `.gitignore` rule set for fast directory listing.
#[derive(Debug, Clone, Default)]
pub struct GitignoreMatcher {
    rules: Vec<GitignoreRule>,
}

impl GitignoreMatcher {
    pub fn from_root(cwd: &Path) -> Self {
        let mut matcher = Self::default();
        // Always ignore `.git` directory
        matcher.add_line(".git/");
        let gitignore_path = cwd.join(".gitignore");
        if let Ok(content) = std::fs::read_to_string(&gitignore_path) {
            matcher.add_str(&content);
        }
        matcher
    }

    pub fn add_str(&mut self, content: &str) {
        for line in content.lines() {
            self.add_line(line);
        }
    }

    pub fn add_line(&mut self, raw_line: &str) {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return;
        }
        let (negated, rest) = if let Some(stripped) = trimmed.strip_prefix('!') {
            (true, stripped.trim())
        } else {
            (false, trimmed)
        };
        if rest.is_empty() {
            return;
        }
        let dir_only = rest.ends_with('/');
        let without_trailing = rest.trim_end_matches('/');
        let anchored = without_trailing.starts_with('/') || without_trailing.contains('/');
        let pattern = without_trailing.trim_start_matches('/').to_string();
        if pattern.is_empty() {
            return;
        }
        self.rules.push(GitignoreRule {
            pattern,
            dir_only,
            anchored,
            negated,
        });
    }

    pub fn is_ignored(&self, relative_path: &str, name: &str, is_dir: bool) -> bool {
        if name == ".git" {
            return true;
        }
        let mut ignored = false;
        for rule in &self.rules {
            if rule.dir_only && !is_dir {
                continue;
            }
            let matched = if rule.anchored {
                glob_match(&rule.pattern, relative_path)
            } else {
                glob_match(&rule.pattern, name) || glob_match(&rule.pattern, relative_path)
            };
            if matched {
                ignored = !rule.negated;
            }
        }
        ignored
    }
}

fn glob_match(pattern: &str, text: &str) -> bool {
    let p_bytes = pattern.as_bytes();
    let t_bytes = text.as_bytes();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star_pi: Option<usize> = None;
    let mut star_ti: usize = 0;

    while ti < t_bytes.len() {
        if pi < p_bytes.len() && (p_bytes[pi] == b'?' || p_bytes[pi] == t_bytes[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p_bytes.len() && p_bytes[pi] == b'*' {
            star_pi = Some(pi);
            pi += 1;
            star_ti = ti;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }

    while pi < p_bytes.len() && p_bytes[pi] == b'*' {
        pi += 1;
    }

    pi == p_bytes.len()
}

pub fn list_workspace_files_sync(
    cwd: &Path,
    relative_dir: Option<&str>,
    include_hidden: bool,
) -> Result<Vec<WorkspaceFileEntry>, String> {
    let (target_dir, rel_dir) = resolve_safe_workspace_path(cwd, relative_dir)?;
    if !target_dir.exists() || !target_dir.is_dir() {
        return Err(format!(
            "Directory does not exist: {}",
            target_dir.display()
        ));
    }

    let mut gitignore = GitignoreMatcher::from_root(cwd);
    if !rel_dir.is_empty() {
        let local_gitignore = target_dir.join(".gitignore");
        if let Ok(local_content) = std::fs::read_to_string(&local_gitignore) {
            gitignore.add_str(&local_content);
        }
    }

    let read_dir = std::fs::read_dir(&target_dir)
        .map_err(|e| format!("Failed to read directory {}: {e}", target_dir.display()))?;

    let mut entries: Vec<WorkspaceFileEntry> = Vec::new();

    for item in read_dir {
        let Ok(entry) = item else { continue };
        let name = entry.file_name().to_string_lossy().to_string();
        if name.is_empty() || name == ".git" {
            continue;
        }

        let is_hidden = name.starts_with('.');
        let full_path = entry.path();
        let symlink_meta = std::fs::symlink_metadata(&full_path).ok();
        let is_symlink = symlink_meta
            .as_ref()
            .is_some_and(|m| m.file_type().is_symlink());
        let meta = std::fs::metadata(&full_path).ok().or(symlink_meta);
        let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());

        let relative_path = if rel_dir.is_empty() {
            name.clone()
        } else {
            format!("{rel_dir}/{name}")
        };

        if !include_hidden && (is_hidden || gitignore.is_ignored(&relative_path, &name, is_dir)) {
            continue;
        }

        let size_bytes = if is_dir {
            0
        } else {
            meta.as_ref().map(|m| m.len()).unwrap_or(0)
        };

        let modified_at = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64);

        let extension = if is_dir {
            None
        } else {
            full_path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
        };

        let kind = if is_symlink {
            "symlink".to_string()
        } else if is_dir {
            "directory".to_string()
        } else {
            "file".to_string()
        };

        entries.push(WorkspaceFileEntry {
            name,
            path: clean_verbatim(&full_path.to_string_lossy()),
            relative_path,
            kind,
            is_dir,
            is_hidden,
            size_bytes,
            extension,
            modified_at,
            git_status: None,
        });
    }

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
            })
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(entries)
}

/// Parses `git status --porcelain=v1` output into a map of `relative_path -> badge ("M" | "A" | "D" | "R" | "?" | "U")`.
pub fn parse_git_status_porcelain(output: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();

    for raw_line in output.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.len() < 4 {
            continue;
        }
        let bytes = line.as_bytes();
        let x = bytes[0] as char;
        let y = bytes[1] as char;
        if bytes[2] != b' ' {
            continue;
        }
        let raw_path = &line[3..];
        let target_path = if let Some((_, dest)) = raw_path.split_once(" -> ") {
            dest
        } else {
            raw_path
        };

        let unquoted = target_path
            .trim()
            .trim_matches('"')
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_string();

        if unquoted.is_empty() {
            continue;
        }

        let code = if x == '?' || y == '?' {
            "?"
        } else if x == '!' || y == '!' {
            continue;
        } else if x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D') {
            "U"
        } else if x == 'D' || y == 'D' {
            "D"
        } else if x == 'R' || y == 'R' {
            "R"
        } else if x == 'A' || y == 'A' {
            "A"
        } else if x == 'M' || y == 'M' || x == 'T' || y == 'T' || x == 'C' || y == 'C' {
            "M"
        } else {
            continue;
        };

        map.insert(unquoted, code.to_string());
    }

    map
}

fn git_command(cwd: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    cmd.args(args)
        .current_dir(cwd)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let augmented = get_augmented_path();
    if !augmented.is_empty() {
        cmd.env("PATH", augmented);
    }

    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    cmd
}

pub async fn query_git_file_statuses(cwd: &Path) -> Result<HashMap<String, String>, String> {
    if !cwd.exists() || !cwd.is_dir() {
        return Err(format!(
            "Workspace directory does not exist: {}",
            cwd.display()
        ));
    }

    let output = match git_command(cwd, &["status", "--porcelain=v1", "-uall"])
        .output()
        .await
    {
        Ok(out) => out,
        Err(_) => return Ok(HashMap::new()),
    };

    if !output.status.success() {
        // Non-git workspace or git error: return empty map so file explorer still works cleanly
        return Ok(HashMap::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_git_status_porcelain(&stdout))
}

async fn query_git_diff_for_file(cwd: &Path, relative_path: &str) -> Option<String> {
    if relative_path.is_empty() {
        return None;
    }

    // Try `git diff HEAD -- <path>` first (covers both staged and unstaged changes vs HEAD)
    if let Ok(out) = git_command(cwd, &["diff", "--no-color", "HEAD", "--", relative_path])
        .output()
        .await
        && out.status.success()
    {
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }

    // Fallback to unstaged `git diff -- <path>` (e.g. unborn HEAD on initial commit)
    if let Ok(out) = git_command(cwd, &["diff", "--no-color", "--", relative_path])
        .output()
        .await
        && out.status.success()
    {
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }

    // Fallback to staged `git diff --cached -- <path>`
    if let Ok(out) = git_command(
        cwd,
        &["diff", "--no-color", "--cached", "--", relative_path],
    )
    .output()
    .await
        && out.status.success()
    {
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }

    None
}

pub fn read_workspace_file_sync(
    cwd: &Path,
    relative_path: &str,
    max_bytes: Option<usize>,
) -> Result<WorkspaceFilePreview, String> {
    let (full_path, rel_path) = resolve_safe_workspace_path(cwd, Some(relative_path))?;
    if rel_path.is_empty() {
        return Err("File path must not be empty".to_string());
    }

    let limit = max_bytes
        .unwrap_or(DEFAULT_PREVIEW_MAX_BYTES)
        .clamp(MIN_PREVIEW_MAX_BYTES, MAX_PREVIEW_MAX_BYTES);

    let name = full_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| rel_path.clone());
    let extension = full_path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .filter(|s| !s.is_empty());

    if !full_path.exists() {
        return Err(format!("File does not exist: {}", rel_path));
    }

    let meta = std::fs::metadata(&full_path)
        .map_err(|e| format!("Failed to read metadata for {}: {e}", rel_path))?;
    if !meta.is_file() {
        return Err(format!("Path is not a regular file: {}", rel_path));
    }

    let size_bytes = meta.len();
    let file = std::fs::File::open(&full_path)
        .map_err(|e| format!("Failed to open file {}: {e}", rel_path))?;

    let mut buf = Vec::with_capacity((size_bytes as usize).min(limit + 1));
    file.take((limit + 1) as u64)
        .read_to_end(&mut buf)
        .map_err(|e| format!("Failed to read file {}: {e}", rel_path))?;

    let truncated = buf.len() > limit;
    if truncated {
        buf.truncate(limit);
    }

    let check_len = buf.len().min(8192);
    let is_binary = buf[..check_len].contains(&0);

    let content = if is_binary {
        None
    } else {
        Some(String::from_utf8_lossy(&buf).to_string())
    };

    Ok(WorkspaceFilePreview {
        path: clean_verbatim(&full_path.to_string_lossy()),
        relative_path: rel_path,
        name,
        extension,
        size_bytes,
        is_binary,
        truncated,
        content,
        diff: None,
        git_status: None,
    })
}

// ============================================================================
// Tauri Commands
// ============================================================================

#[tauri::command]
pub async fn list_workspace_files(
    cwd: String,
    relative_dir: Option<String>,
    include_hidden: Option<bool>,
) -> Result<Vec<WorkspaceFileEntry>, String> {
    let cwd_buf = PathBuf::from(cwd.trim());
    let show_hidden = include_hidden.unwrap_or(false);

    let entries = tokio::task::spawn_blocking({
        let cwd_clone = cwd_buf.clone();
        move || list_workspace_files_sync(&cwd_clone, relative_dir.as_deref(), show_hidden)
    })
    .await
    .map_err(|e| format!("Failed to join list_workspace_files task: {e}"))??;

    Ok(entries)
}

#[tauri::command]
pub async fn get_git_file_statuses(cwd: String) -> Result<HashMap<String, String>, String> {
    let cwd_buf = PathBuf::from(cwd.trim());
    query_git_file_statuses(&cwd_buf).await
}

#[tauri::command]
pub async fn read_workspace_file(
    cwd: String,
    relative_path: String,
    max_bytes: Option<usize>,
) -> Result<WorkspaceFilePreview, String> {
    let cwd_buf = PathBuf::from(cwd.trim());
    let rel_clone = relative_path.clone();
    let cwd_for_read = cwd_buf.clone();

    let preview_res = tokio::task::spawn_blocking(move || {
        read_workspace_file_sync(&cwd_for_read, &rel_clone, max_bytes)
    })
    .await
    .map_err(|e| format!("Failed to join read_workspace_file task: {e}"))?;

    let (_, normalized_rel) = resolve_safe_workspace_path(&cwd_buf, Some(&relative_path))?;
    let diff = query_git_diff_for_file(&cwd_buf, &normalized_rel).await;

    match preview_res {
        Ok(mut preview) => {
            preview.diff = diff;
            Ok(preview)
        }
        Err(err) => {
            // If the file was deleted in git working tree, still return its git diff if available
            if let Some(diff_text) = diff {
                let full_path = cwd_buf.join(&normalized_rel);
                let name = full_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| normalized_rel.clone());
                let extension = full_path
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
                    .filter(|s| !s.is_empty());
                Ok(WorkspaceFilePreview {
                    path: clean_verbatim(&full_path.to_string_lossy()),
                    relative_path: normalized_rel,
                    name,
                    extension,
                    size_bytes: 0,
                    is_binary: false,
                    truncated: false,
                    content: None,
                    diff: Some(diff_text),
                    git_status: Some("D".to_string()),
                })
            } else {
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pipan-fsexp-test-{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn test_resolve_safe_workspace_path_prevents_traversal() {
        let dir = make_temp_dir("traversal");
        std::fs::create_dir_all(dir.join("src")).unwrap();

        let (resolved, rel) = resolve_safe_workspace_path(&dir, Some("src/main.rs")).unwrap();
        assert_eq!(rel, "src/main.rs");
        assert_eq!(resolved, dir.join("src").join("main.rs"));

        assert!(resolve_safe_workspace_path(&dir, Some("../secret.txt")).is_err());
        assert!(resolve_safe_workspace_path(&dir, Some("src/../../outside")).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_git_status_porcelain_handles_modified_added_deleted_untracked_renamed() {
        let sample = [
            " M src/lib.rs",
            "M  src/main.rs",
            "A  src/new_file.rs",
            " D old_file.txt",
            "R  before.rs -> after.rs",
            "?? untracked.md",
            "!! target/debug/app",
            "UU conflict.rs",
        ]
        .join("\n");
        let map = parse_git_status_porcelain(&sample);
        assert_eq!(map.get("src/lib.rs").map(String::as_str), Some("M"));
        assert_eq!(map.get("src/main.rs").map(String::as_str), Some("M"));
        assert_eq!(map.get("src/new_file.rs").map(String::as_str), Some("A"));
        assert_eq!(map.get("old_file.txt").map(String::as_str), Some("D"));
        assert_eq!(map.get("after.rs").map(String::as_str), Some("R"));
        assert_eq!(map.get("untracked.md").map(String::as_str), Some("?"));
        assert_eq!(map.get("conflict.rs").map(String::as_str), Some("U"));
        assert!(!map.contains_key("target/debug/app"));
    }

    #[test]
    fn test_gitignore_matcher_handles_dirs_globs_and_negations() {
        let mut matcher = GitignoreMatcher::default();
        matcher.add_str(
            "
            # Comment line
            node_modules/
            *.log
            !important.log
            build/output.js
            ",
        );

        assert!(matcher.is_ignored("node_modules", "node_modules", true));
        assert!(!matcher.is_ignored("node_modules", "node_modules", false));
        assert!(matcher.is_ignored("debug.log", "debug.log", false));
        assert!(!matcher.is_ignored("important.log", "important.log", false));
        assert!(matcher.is_ignored("build/output.js", "output.js", false));
        assert!(!matcher.is_ignored("src/main.rs", "main.rs", false));
    }

    #[test]
    fn test_list_workspace_files_sorts_dirs_first_and_respects_hidden_and_gitignore() {
        let dir = make_temp_dir("list");
        std::fs::write(dir.join(".gitignore"), "node_modules/\n*.tmp\n").unwrap();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules")).unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join(".env"), "SECRET=1").unwrap();
        std::fs::write(dir.join("scratch.tmp"), "temp").unwrap();
        std::fs::write(dir.join("README.md"), "# Hello").unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]").unwrap();

        let visible = list_workspace_files_sync(&dir, None, false).unwrap();
        let names: Vec<&str> = visible.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["src", "Cargo.toml", "README.md"]);
        assert!(visible[0].is_dir);
        assert!(!visible[1].is_dir);

        let all = list_workspace_files_sync(&dir, None, true).unwrap();
        let all_names: Vec<&str> = all.iter().map(|e| e.name.as_str()).collect();
        assert!(all_names.contains(&"node_modules"));
        assert!(all_names.contains(&".env"));
        assert!(all_names.contains(&"scratch.tmp"));
        // `.git` is always excluded from workspace file tree
        assert!(!all_names.contains(&".git"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_read_workspace_file_text_truncation_and_binary_detection() {
        let dir = make_temp_dir("preview");
        let text_file = dir.join("hello.rs");
        let long_content = "fn main() {}\n".repeat(200);
        std::fs::write(&text_file, &long_content).unwrap();

        let preview = read_workspace_file_sync(&dir, "hello.rs", Some(1024)).unwrap();
        assert!(!preview.is_binary);
        assert!(preview.truncated);
        assert_eq!(preview.content.as_ref().map(String::len), Some(1024));
        assert_eq!(preview.extension.as_deref(), Some("rs"));

        let bin_file = dir.join("binary.bin");
        std::fs::write(&bin_file, b"\x00\x01\x02\x03binary").unwrap();
        let bin_preview = read_workspace_file_sync(&dir, "binary.bin", None).unwrap();
        assert!(bin_preview.is_binary);
        assert!(bin_preview.content.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
