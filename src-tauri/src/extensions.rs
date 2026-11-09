//! VS Code-style Extension & Plugin System backend (`src-tauri/src/extensions.rs`).
//!
//! Manages external/custom user plugins stored in `~/.pipan/extensions/<ext-id>/`
//! and `<app_local_data_dir>/extensions/<ext-id>/`.
//!
//! Commands:
//! - `list_external_extensions()`: Scans local extension directories and returns manifests + entry script contents.
//! - `install_extension_from_folder(source_path)`: Copies a local plugin directory into `~/.pipan/extensions/<ext-id>/`.
//! - `install_extension_from_git(repo_url)`: Clones an extension from a Git repository URL into `~/.pipan/extensions/<ext-id>/`.
//! - `uninstall_extension(extension_id)`: Removes an installed external extension directory.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};

use crate::models::resolve_user_home_dir;

/// Reserved first-party built-in extension IDs (including canonical IDs and short aliases).
pub const RESERVED_BUILTIN_IDS: &[&str] = &[
    "pipan.node-graph",
    "pipan.nodes",
    "pipan.github-prs",
    "pipan.github",
    "pipan.terminal",
    "pipan.files",
    "pipan.cost-analytics",
    "pipan.analytics",
];

/// Manifest filenames recognized inside an extension folder, checked in priority order.
const MANIFEST_FILENAMES: &[&str] = &["pipan-extension.json", "extension.json", "manifest.json"];

/// Metadata file written when an extension is installed from a Git repository.
const SOURCE_META_FILENAME: &str = ".pipan-source.json";

/// Maximum size (in bytes) of an external plugin's JS entry file (1 MiB).
const MAX_ENTRY_SCRIPT_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionPanelContribution {
    pub id: String,
    pub title: String,
    pub icon: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_position: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionViewContribution {
    pub id: String,
    pub title: String,
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionStatusBarContribution {
    pub id: String,
    pub alignment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionSlashCommandContribution {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionContributes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panels: Option<Vec<ExtensionPanelContribution>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_tabs: Option<Vec<ExtensionViewContribution>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_views: Option<Vec<ExtensionViewContribution>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_bar_items: Option<Vec<ExtensionStatusBarContribution>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slash_commands: Option<Vec<ExtensionSlashCommandContribution>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PiPanExtensionManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_by_default: Option<bool>,
    #[serde(default)]
    pub contributes: ExtensionContributes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExternalExtensionBundle {
    pub manifest: PiPanExtensionManifest,
    pub install_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtensionSourceMetadata {
    source_url: Option<String>,
}

/// Validates an extension identifier so it is safe to use as a directory name.
pub fn validate_extension_id(raw_id: &str) -> Result<String, String> {
    let trimmed = raw_id.trim();
    if trimmed.is_empty() {
        return Err("Extension id must not be empty".to_string());
    }
    if trimmed.len() < 2 || trimmed.len() > 128 {
        return Err("Extension id must be between 2 and 128 characters".to_string());
    }
    if trimmed.starts_with('.') || trimmed.ends_with('.') || trimmed.contains("..") {
        return Err(format!(
            "Extension id `{trimmed}` must not start/end with '.' or contain '..'"
        ));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        return Err(format!(
            "Extension id `{trimmed}` may only contain ASCII letters, digits, '.', '-', or '_'"
        ));
    }
    Ok(trimmed.to_string())
}

/// Validates a relative entry script path so it cannot escape the extension directory.
pub fn validate_relative_entry_path(raw_entry: &str) -> Result<String, String> {
    let trimmed = raw_entry.trim();
    if trimmed.is_empty() {
        return Err("Extension entry path must not be empty".to_string());
    }
    if trimmed.contains('\\') {
        return Err("Extension entry path must use forward slashes ('/')".to_string());
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Err("Extension entry path must be relative to the extension folder".to_string());
    }
    let mut normal_count = 0usize;
    for comp in path.components() {
        match comp {
            Component::Normal(_) => normal_count += 1,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "Extension entry path `{trimmed}` must not traverse outside the extension folder"
                ));
            }
        }
    }
    if normal_count == 0 {
        return Err("Extension entry path must point to a file".to_string());
    }
    Ok(trimmed.to_string())
}

/// Validates and normalizes an extension manifest.
pub fn validate_manifest(manifest: &mut PiPanExtensionManifest) -> Result<(), String> {
    manifest.id = validate_extension_id(&manifest.id)?;
    manifest.name = manifest.name.trim().to_string();
    if manifest.name.is_empty() {
        return Err(format!(
            "Extension `{}` manifest name must not be empty",
            manifest.id
        ));
    }
    manifest.version = manifest.version.trim().to_string();
    if manifest.version.is_empty() {
        return Err(format!(
            "Extension `{}` manifest version must not be empty",
            manifest.id
        ));
    }
    manifest.description = manifest.description.trim().to_string();
    manifest.author = manifest.author.trim().to_string();
    if manifest.author.is_empty() {
        manifest.author = "Unknown".to_string();
    }

    if let Some(entry) = manifest.entry.take() {
        let trimmed = entry.trim();
        if !trimmed.is_empty() {
            manifest.entry = Some(validate_relative_entry_path(trimmed)?);
        }
    }

    if let Some(panels) = &mut manifest.contributes.panels {
        for panel in panels {
            panel.id = panel.id.trim().to_string();
            panel.title = panel.title.trim().to_string();
            panel.icon = panel.icon.trim().to_string();
            if panel.id.is_empty() || panel.title.is_empty() {
                return Err(format!(
                    "Extension `{}` panel contribution requires non-empty id and title",
                    manifest.id
                ));
            }
            if panel.icon.is_empty() {
                panel.icon = "puzzle".to_string();
            }
            if let Some(pos) = &panel.default_position
                && pos != "split"
                && pos != "bottom"
            {
                return Err(format!(
                    "Extension `{}` panel `{}` has invalid defaultPosition `{pos}` (expected 'split' or 'bottom')",
                    manifest.id, panel.id
                ));
            }
        }
    }

    if let Some(tabs) = &mut manifest.contributes.sidebar_tabs {
        for tab in tabs {
            tab.id = tab.id.trim().to_string();
            tab.title = tab.title.trim().to_string();
            tab.icon = tab.icon.trim().to_string();
            if tab.id.is_empty() || tab.title.is_empty() {
                return Err(format!(
                    "Extension `{}` sidebarTab contribution requires non-empty id and title",
                    manifest.id
                ));
            }
            if tab.icon.is_empty() {
                tab.icon = "puzzle".to_string();
            }
        }
    }

    if let Some(views) = &mut manifest.contributes.main_views {
        for view in views {
            view.id = view.id.trim().to_string();
            view.title = view.title.trim().to_string();
            view.icon = view.icon.trim().to_string();
            if view.id.is_empty() || view.title.is_empty() {
                return Err(format!(
                    "Extension `{}` mainView contribution requires non-empty id and title",
                    manifest.id
                ));
            }
            if view.icon.is_empty() {
                view.icon = "puzzle".to_string();
            }
        }
    }

    if let Some(items) = &mut manifest.contributes.status_bar_items {
        for item in items {
            item.id = item.id.trim().to_string();
            item.alignment = item.alignment.trim().to_string();
            if item.id.is_empty() {
                return Err(format!(
                    "Extension `{}` statusBarItem contribution requires non-empty id",
                    manifest.id
                ));
            }
            if item.alignment != "left" && item.alignment != "right" {
                return Err(format!(
                    "Extension `{}` statusBarItem `{}` has invalid alignment `{}` (expected 'left' or 'right')",
                    manifest.id, item.id, item.alignment
                ));
            }
        }
    }

    if let Some(commands) = &mut manifest.contributes.slash_commands {
        for cmd in commands {
            cmd.name = cmd.name.trim().trim_start_matches('/').to_string();
            cmd.description = cmd.description.trim().to_string();
            if cmd.name.is_empty() {
                return Err(format!(
                    "Extension `{}` slashCommand contribution requires non-empty name",
                    manifest.id
                ));
            }
        }
    }

    Ok(())
}

/// Parses and validates a `PiPanExtensionManifest` from a JSON string.
pub fn parse_and_validate_manifest(raw_json: &str) -> Result<PiPanExtensionManifest, String> {
    let stripped = raw_json.trim_start_matches('\u{feff}');
    let mut manifest: PiPanExtensionManifest = serde_json::from_str(stripped)
        .map_err(|e| format!("Invalid extension manifest JSON: {e}"))?;
    validate_manifest(&mut manifest)?;
    Ok(manifest)
}

/// Validates and normalizes a Git repository URL for `git clone`.
pub fn validate_git_repo_url(raw_url: &str) -> Result<String, String> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return Err("Git repository URL must not be empty".to_string());
    }
    if trimmed.starts_with('-') {
        return Err("Git repository URL must not start with '-'".to_string());
    }
    if trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(
            "Git repository URL must not contain whitespace or control characters".to_string(),
        );
    }

    // Support `github.com/owner/repo` or `owner/repo` shorthand
    if let Some(rest) = trimmed.strip_prefix("github.com/")
        && rest.split('/').filter(|s| !s.is_empty()).count() >= 2
    {
        return Ok(format!("https://github.com/{rest}"));
    }

    let allowed_prefixes = ["https://", "http://", "git@", "ssh://", "file://"];
    if allowed_prefixes.iter().any(|p| trimmed.starts_with(p)) {
        return Ok(trimmed.to_string());
    }

    // Check `owner/repo` shorthand (two path segments, safe chars)
    let segments: Vec<&str> = trimmed.split('/').collect();
    if segments.len() == 2
        && segments.iter().all(|seg| {
            !seg.is_empty()
                && !seg.starts_with('.')
                && seg
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        })
    {
        return Ok(format!("https://github.com/{trimmed}.git"));
    }

    Err(format!(
        "Unsupported Git repository URL `{trimmed}` (expected https://, ssh://, git@, or owner/repo)"
    ))
}

/// Locates the manifest file inside `extension_dir`.
fn find_manifest_file(extension_dir: &Path) -> Option<PathBuf> {
    for name in MANIFEST_FILENAMES {
        let candidate = extension_dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Loads a single external extension bundle from its directory.
pub fn load_extension_bundle_from_dir(
    extension_dir: &Path,
) -> Result<ExternalExtensionBundle, String> {
    if !extension_dir.is_dir() {
        return Err(format!(
            "Extension path `{}` is not a directory",
            extension_dir.display()
        ));
    }

    let manifest_path = find_manifest_file(extension_dir).ok_or_else(|| {
        format!(
            "No extension manifest found in `{}` (expected pipan-extension.json, extension.json, or manifest.json)",
            extension_dir.display()
        )
    })?;

    let raw_manifest = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Failed to read `{}`: {e}", manifest_path.display()))?;
    let mut manifest = parse_and_validate_manifest(&raw_manifest)?;
    // External extensions loaded from disk are never marked as first-party builtin.
    manifest.builtin = Some(false);

    let entry_code = if let Some(rel_entry) = &manifest.entry {
        let entry_path = extension_dir.join(rel_entry);
        if !entry_path.is_file() {
            return Err(format!(
                "Extension `{}` entry script `{}` does not exist",
                manifest.id, rel_entry
            ));
        }
        let metadata = std::fs::metadata(&entry_path).map_err(|e| {
            format!(
                "Failed to inspect entry script `{}`: {e}",
                entry_path.display()
            )
        })?;
        if metadata.len() > MAX_ENTRY_SCRIPT_BYTES {
            return Err(format!(
                "Extension `{}` entry script exceeds maximum size of 1 MiB",
                manifest.id
            ));
        }
        let code = std::fs::read_to_string(&entry_path).map_err(|e| {
            format!(
                "Failed to read entry script `{}`: {e}",
                entry_path.display()
            )
        })?;
        Some(code)
    } else {
        None
    };

    let source_url = std::fs::read_to_string(extension_dir.join(SOURCE_META_FILENAME))
        .ok()
        .and_then(|s| serde_json::from_str::<ExtensionSourceMetadata>(&s).ok())
        .and_then(|meta| meta.source_url);

    Ok(ExternalExtensionBundle {
        manifest,
        install_path: extension_dir.to_string_lossy().to_string(),
        entry_code,
        source_url,
    })
}

/// Scans multiple extension root directories (`~/.pipan/extensions` and `<app_local_data_dir>/extensions`),
/// returning deduplicated valid external extension bundles sorted by `manifest.id`.
pub fn scan_extensions_in_dirs(roots: &[PathBuf]) -> Vec<ExternalExtensionBundle> {
    let mut bundles: Vec<ExternalExtensionBundle> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    for root in roots {
        if !root.exists() || !root.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        let mut subdirs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| !n.starts_with('.'))
                        .unwrap_or(false)
            })
            .collect();
        subdirs.sort();

        for subdir in subdirs {
            if let Ok(bundle) = load_extension_bundle_from_dir(&subdir) {
                let key = bundle.manifest.id.to_lowercase();
                if seen_ids.insert(key) {
                    bundles.push(bundle);
                }
            }
        }
    }

    bundles.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
    bundles
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst)
        .map_err(|e| format!("Failed to create directory `{}`: {e}", dst.display()))?;

    let entries = std::fs::read_dir(src)
        .map_err(|e| format!("Failed to read directory `{}`: {e}", src.display()))?;

    for entry_res in entries {
        let entry = entry_res.map_err(|e| format!("Directory entry error: {e}"))?;
        let path = entry.path();
        let Some(name_os) = path.file_name() else {
            continue;
        };
        let name = name_os.to_string_lossy();
        if name == ".git" || name == "node_modules" || name == "target" {
            continue;
        }
        let target_path = dst.join(name_os);
        let file_type = entry
            .file_type()
            .map_err(|e| format!("Failed to inspect `{}`: {e}", path.display()))?;
        if file_type.is_dir() {
            copy_dir_recursive(&path, &target_path)?;
        } else if file_type.is_file() {
            std::fs::copy(&path, &target_path)
                .map_err(|e| format!("Failed to copy `{}`: {e}", path.display()))?;
        }
    }

    Ok(())
}

/// Installs an extension from a local directory (or manifest file path) into `target_root/<ext-id>`.
pub fn install_extension_from_folder_into(
    source_path: &Path,
    target_root: &Path,
    source_url: Option<String>,
) -> Result<ExternalExtensionBundle, String> {
    if !source_path.exists() {
        return Err(format!(
            "Extension source path `{}` does not exist",
            source_path.display()
        ));
    }

    let source_dir = if source_path.is_file() {
        source_path
            .parent()
            .ok_or_else(|| "Invalid manifest file parent directory".to_string())?
    } else {
        source_path
    };

    // Validate the source bundle before copying
    let preview = load_extension_bundle_from_dir(source_dir)?;
    let ext_id = &preview.manifest.id;

    if RESERVED_BUILTIN_IDS
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(ext_id))
    {
        return Err(format!(
            "Extension id `{ext_id}` is reserved for a built-in PiPan extension"
        ));
    }

    std::fs::create_dir_all(target_root).map_err(|e| {
        format!(
            "Failed to create extensions directory `{}`: {e}",
            target_root.display()
        )
    })?;

    let dest_dir = target_root.join(ext_id);

    // If source_dir and dest_dir are not the same canonical path, replace dest_dir with a fresh copy
    let same_dir = std::fs::canonicalize(source_dir)
        .ok()
        .zip(std::fs::canonicalize(&dest_dir).ok())
        .map(|(a, b)| a == b)
        .unwrap_or(false);

    if !same_dir {
        if dest_dir.exists() {
            std::fs::remove_dir_all(&dest_dir).map_err(|e| {
                format!(
                    "Failed to remove previous installation at `{}`: {e}",
                    dest_dir.display()
                )
            })?;
        }
        copy_dir_recursive(source_dir, &dest_dir)?;
    }

    if let Some(url) = &source_url {
        let meta = ExtensionSourceMetadata {
            source_url: Some(url.clone()),
        };
        if let Ok(serialized) = serde_json::to_string_pretty(&meta) {
            let _ = std::fs::write(dest_dir.join(SOURCE_META_FILENAME), serialized);
        }
    }

    load_extension_bundle_from_dir(&dest_dir)
}

/// Removes an installed external extension by ID from all provided extension directories.
pub fn uninstall_extension_from_dirs(
    extension_id: &str,
    roots: &[PathBuf],
) -> Result<bool, String> {
    let valid_id = validate_extension_id(extension_id)?;
    let mut removed_any = false;

    for root in roots {
        let candidate = root.join(&valid_id);
        if candidate.exists() && candidate.is_dir() {
            std::fs::remove_dir_all(&candidate).map_err(|e| {
                format!(
                    "Failed to remove extension directory `{}`: {e}",
                    candidate.display()
                )
            })?;
            removed_any = true;
        }
    }

    Ok(removed_any)
}

fn resolve_extension_roots(app: &AppHandle) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(home) = resolve_user_home_dir() {
        roots.push(home.join(".pipan").join("extensions"));
    }
    if let Ok(local_data) = app.path().app_local_data_dir() {
        let app_ext = local_data.join("extensions");
        if !roots.contains(&app_ext) {
            roots.push(app_ext);
        }
    }
    roots
}

fn resolve_primary_install_root(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(home) = resolve_user_home_dir() {
        return Ok(home.join(".pipan").join("extensions"));
    }
    app.path()
        .app_local_data_dir()
        .map(|d| d.join("extensions"))
        .map_err(|e| format!("Could not resolve extensions install directory: {e}"))
}

fn configure_git_command(cmd: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = cmd;
    }
}

// ---------------------------------------------------------------------------
// Tauri Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_external_extensions(
    app: AppHandle,
) -> Result<Vec<ExternalExtensionBundle>, String> {
    let roots = resolve_extension_roots(&app);
    tokio::task::spawn_blocking(move || Ok(scan_extensions_in_dirs(&roots)))
        .await
        .map_err(|e| format!("Failed to join list_external_extensions task: {e}"))?
}

#[tauri::command]
pub async fn install_extension_from_folder(
    app: AppHandle,
    source_path: String,
) -> Result<ExternalExtensionBundle, String> {
    let target_root = resolve_primary_install_root(&app)?;
    tokio::task::spawn_blocking(move || {
        let trimmed = source_path.trim();
        if trimmed.is_empty() {
            return Err("Extension folder path must not be empty".to_string());
        }
        install_extension_from_folder_into(Path::new(trimmed), &target_root, None)
    })
    .await
    .map_err(|e| format!("Failed to join install_extension_from_folder task: {e}"))?
}

#[tauri::command]
pub async fn install_extension_from_git(
    app: AppHandle,
    repo_url: String,
) -> Result<ExternalExtensionBundle, String> {
    let target_root = resolve_primary_install_root(&app)?;
    tokio::task::spawn_blocking(move || {
        let normalized_url = validate_git_repo_url(&repo_url)?;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let temp_clone_dir = std::env::temp_dir().join(format!(
            "pipan-ext-git-clone-{}-{nanos}",
            std::process::id()
        ));

        if temp_clone_dir.exists() {
            let _ = std::fs::remove_dir_all(&temp_clone_dir);
        }

        let mut cmd = Command::new("git");
        configure_git_command(&mut cmd);
        cmd.args([
            "clone",
            "--depth",
            "1",
            "--",
            &normalized_url,
            &temp_clone_dir.to_string_lossy(),
        ]);

        let output = cmd.output().map_err(|e| {
            let _ = std::fs::remove_dir_all(&temp_clone_dir);
            format!("Failed to execute `git clone`: {e}")
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let _ = std::fs::remove_dir_all(&temp_clone_dir);
            return Err(format!(
                "git clone failed: {}",
                if stderr.is_empty() {
                    "unknown git error"
                } else {
                    &stderr
                }
            ));
        }

        let install_res =
            install_extension_from_folder_into(&temp_clone_dir, &target_root, Some(normalized_url));
        let _ = std::fs::remove_dir_all(&temp_clone_dir);
        install_res
    })
    .await
    .map_err(|e| format!("Failed to join install_extension_from_git task: {e}"))?
}

#[tauri::command]
pub async fn uninstall_extension(app: AppHandle, extension_id: String) -> Result<bool, String> {
    let roots = resolve_extension_roots(&app);
    tokio::task::spawn_blocking(move || uninstall_extension_from_dirs(&extension_id, &roots))
        .await
        .map_err(|e| format!("Failed to join uninstall_extension task: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pipan-extensions-test-{prefix}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create temp test dir");
        dir
    }

    #[test]
    fn test_validate_extension_id_accepts_valid_and_rejects_traversal() {
        assert_eq!(
            validate_extension_id("acme.custom-panel_v1").unwrap(),
            "acme.custom-panel_v1"
        );
        assert!(validate_extension_id("").is_err());
        assert!(validate_extension_id("a").is_err());
        assert!(validate_extension_id("../escape").is_err());
        assert!(validate_extension_id("acme/plugin").is_err());
        assert!(validate_extension_id("acme\\plugin").is_err());
        assert!(validate_extension_id(".hidden").is_err());
    }

    #[test]
    fn test_parse_and_validate_manifest_valid_and_invalid_cases() {
        let valid_json = r#"{
            "id": "acme.deploy-monitor",
            "name": "Deploy Monitor",
            "version": "1.2.0",
            "description": "Monitors deployment pipelines",
            "author": "Acme Corp",
            "entry": "dist/index.js",
            "contributes": {
                "panels": [
                    { "id": "deploys", "title": "Deploys", "icon": "rocket", "defaultPosition": "split" }
                ],
                "statusBarItems": [
                    { "id": "deploy-status", "alignment": "right", "priority": 20 }
                ],
                "slashCommands": [
                    { "name": "/deploy", "description": "Trigger deployment status check" }
                ]
            }
        }"#;

        let manifest = parse_and_validate_manifest(valid_json).expect("valid manifest");
        assert_eq!(manifest.id, "acme.deploy-monitor");
        assert_eq!(manifest.entry.as_deref(), Some("dist/index.js"));
        assert_eq!(
            manifest.contributes.slash_commands.unwrap()[0].name,
            "deploy"
        );

        // Invalid entry traversal
        let bad_entry = r#"{
            "id": "acme.bad",
            "name": "Bad",
            "version": "1.0.0",
            "contributes": {},
            "entry": "../secret.js"
        }"#;
        assert!(parse_and_validate_manifest(bad_entry).is_err());

        // Invalid statusBarItem alignment
        let bad_align = r#"{
            "id": "acme.bad-align",
            "name": "Bad Align",
            "version": "1.0.0",
            "contributes": {
                "statusBarItems": [{ "id": "x", "alignment": "center" }]
            }
        }"#;
        assert!(parse_and_validate_manifest(bad_align).is_err());
    }

    #[test]
    fn test_validate_git_repo_url() {
        assert_eq!(
            validate_git_repo_url("https://github.com/acme/pipan-ext.git").unwrap(),
            "https://github.com/acme/pipan-ext.git"
        );
        assert_eq!(
            validate_git_repo_url("acme/pipan-ext").unwrap(),
            "https://github.com/acme/pipan-ext.git"
        );
        assert!(validate_git_repo_url("--upload-pack=evil").is_err());
        assert!(validate_git_repo_url("").is_err());
    }

    #[test]
    fn test_install_scan_and_uninstall_extension_lifecycle() {
        let root = make_temp_dir("lifecycle");
        let source_dir = root.join("src-plugin");
        let install_root = root.join("installed");
        std::fs::create_dir_all(&source_dir).unwrap();

        let manifest_json = r#"{
            "id": "community.hello-world",
            "name": "Hello World Plugin",
            "version": "0.1.0",
            "description": "Sample external plugin",
            "author": "Community",
            "entry": "index.js",
            "contributes": {
                "panels": [{ "id": "hello-panel", "title": "Hello", "icon": "sparkles" }]
            }
        }"#;
        std::fs::write(source_dir.join("pipan-extension.json"), manifest_json).unwrap();
        std::fs::write(
            source_dir.join("index.js"),
            "window.pipan?.registerExtension({ id: 'community.hello-world' });",
        )
        .unwrap();

        let installed = install_extension_from_folder_into(
            &source_dir,
            &install_root,
            Some("https://github.com/community/hello-world".to_string()),
        )
        .expect("install from folder");

        assert_eq!(installed.manifest.id, "community.hello-world");
        assert_eq!(installed.manifest.builtin, Some(false));
        assert!(
            installed
                .entry_code
                .as_deref()
                .unwrap()
                .contains("registerExtension")
        );
        assert_eq!(
            installed.source_url.as_deref(),
            Some("https://github.com/community/hello-world")
        );

        let scanned = scan_extensions_in_dirs(std::slice::from_ref(&install_root));
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].manifest.id, "community.hello-world");

        let removed = uninstall_extension_from_dirs(
            "community.hello-world",
            std::slice::from_ref(&install_root),
        )
        .expect("uninstall extension");
        assert!(removed);

        let scanned_after = scan_extensions_in_dirs(std::slice::from_ref(&install_root));
        assert!(scanned_after.is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_install_rejects_reserved_builtin_ids() {
        let root = make_temp_dir("reserved");
        let source_dir = root.join("src-builtin-spoof");
        let install_root = root.join("installed");
        std::fs::create_dir_all(&source_dir).unwrap();

        let manifest_json = r#"{
            "id": "pipan.node-graph",
            "name": "Spoof Node Graph",
            "version": "9.9.9",
            "contributes": {}
        }"#;
        std::fs::write(source_dir.join("extension.json"), manifest_json).unwrap();

        let err = install_extension_from_folder_into(&source_dir, &install_root, None)
            .expect_err("reserved builtin id must be rejected");
        assert!(err.contains("reserved"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
