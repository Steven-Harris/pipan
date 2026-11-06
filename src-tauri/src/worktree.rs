//! Git worktree management for isolated Pi sessions.
//!
//! Every agent session runs inside its own `git worktree` on a dedicated
//! `pipan/<id>` branch, so the agent can never mutate the host checkout.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command;

use crate::pi_rpc::get_augmented_path;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub const BRANCH_PREFIX: &str = "pipan/";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorktreeInfo {
    /// Top-level directory of the host repository.
    pub repo_root: String,
    /// Absolute path of the isolated worktree (Pi's working directory).
    pub path: String,
    /// Branch created for this worktree.
    pub branch: String,
    /// Commit-ish the worktree branch was created from.
    pub base_ref: String,
}

/// Generates a short, unique, filesystem- and ref-safe session id.
pub fn new_session_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}", millis, n)
}

/// Directory name (under the worktree root) grouping all worktrees of one repo.
fn repo_dir_name(repo_root: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let name: String = repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".to_string())
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    repo_root.hash(&mut hasher);
    format!("{}-{:08x}", name, hasher.finish() as u32)
}

async fn run_git(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let path = get_augmented_path();
    if !path.is_empty() {
        cmd.env("PATH", path);
    }
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run `git {}`: {}", args.join(" "), e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("`git {}` failed: {}", args.join(" "), stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Resolves the top-level directory of the repository containing `path`.
pub async fn resolve_repo_root(path: &Path) -> Result<PathBuf, String> {
    if !path.is_dir() {
        return Err(format!("Not a directory: {}", path.display()));
    }
    let top = run_git(path, &["rev-parse", "--show-toplevel"])
        .await
        .map_err(|_| format!("Not a git repository: {}", path.display()))?;
    Ok(PathBuf::from(top))
}

/// Creates an isolated worktree for `session_id` under `worktrees_root`.
///
/// The worktree is checked out on a new `pipan/<session_id>` branch started at
/// `base_ref` (default `HEAD`). Uncommitted changes in the host checkout are not
/// carried over.
pub async fn create_worktree(
    repo_path: &Path,
    worktrees_root: &Path,
    session_id: &str,
    base_ref: Option<&str>,
) -> Result<WorktreeInfo, String> {
    let repo_root = resolve_repo_root(repo_path).await?;

    let base = base_ref
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("HEAD");
    if base.starts_with('-') {
        return Err(format!("Invalid base ref: {}", base));
    }
    run_git(
        &repo_root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{}^{{commit}}", base),
        ],
    )
    .await
    .map_err(|_| {
        format!(
            "Base ref `{}` does not resolve to a commit (does the repository have any commits?)",
            base
        )
    })?;

    let parent = worktrees_root.join(repo_dir_name(&repo_root));
    std::fs::create_dir_all(&parent).map_err(|e| {
        format!(
            "Failed to create worktree directory {}: {}",
            parent.display(),
            e
        )
    })?;
    let path = parent.join(session_id);
    let branch = format!("{}{}", BRANCH_PREFIX, session_id);

    let path_str = path.to_string_lossy().to_string();
    run_git(
        &repo_root,
        &["worktree", "add", "-b", &branch, &path_str, base],
    )
    .await?;

    Ok(WorktreeInfo {
        repo_root: repo_root.to_string_lossy().to_string(),
        path: path_str,
        branch,
        base_ref: base.to_string(),
    })
}

/// Removes a worktree previously created by [`create_worktree`].
///
/// Refuses to touch anything outside `worktrees_root`. The `pipan/<id>` branch
/// is kept so the agent's commits are not lost.
pub async fn remove_worktree(worktrees_root: &Path, worktree_path: &Path) -> Result<(), String> {
    let root = std::fs::canonicalize(worktrees_root)
        .map_err(|e| format!("Worktree root unavailable: {}", e))?;
    let target = std::fs::canonicalize(worktree_path)
        .map_err(|e| format!("Worktree not found ({}): {}", worktree_path.display(), e))?;
    if target == root || !target.starts_with(&root) {
        return Err(format!(
            "Refusing to remove a path outside the pipan worktree root: {}",
            worktree_path.display()
        ));
    }
    let target_str = target.to_string_lossy().to_string();
    // Locate the owning repository through the worktree, then run `worktree remove` from the
    // repository root. Running git with the worktree as its cwd would lock the directory on
    // Windows and make the removal fail with "Permission denied".
    let common_dir = run_git(
        &target,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .await
    .map_err(|e| format!("Not a git worktree: {}", e))?;
    let common_dir = PathBuf::from(common_dir);
    let repo_dir = common_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| common_dir.clone());
    run_git(&repo_dir, &["worktree", "remove", "--force", &target_str]).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("pipan-wt-{}-{}", tag, new_session_id()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    async fn init_repo(dir: &Path, commit: bool) {
        run_git(dir, &["init", "-q"]).await.unwrap();
        run_git(dir, &["config", "user.email", "t@example.com"])
            .await
            .unwrap();
        run_git(dir, &["config", "user.name", "t"]).await.unwrap();
        run_git(dir, &["config", "commit.gpgsign", "false"])
            .await
            .unwrap();
        if commit {
            std::fs::write(dir.join("a.txt"), "hello").unwrap();
            run_git(dir, &["add", "."]).await.unwrap();
            run_git(dir, &["commit", "-q", "-m", "init"]).await.unwrap();
        }
    }

    #[test]
    fn session_ids_are_unique_and_ref_safe() {
        let a = new_session_id();
        let b = new_session_id();
        assert_ne!(a, b);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    }

    #[tokio::test]
    async fn creates_and_removes_isolated_worktree() {
        let repo = unique_dir("repo");
        let root = unique_dir("root");
        init_repo(&repo, true).await;

        let id = new_session_id();
        let info = create_worktree(&repo, &root, &id, None).await.unwrap();
        assert_eq!(info.branch, format!("pipan/{}", id));
        let wt = PathBuf::from(&info.path);
        assert!(wt.join("a.txt").is_file());
        assert!(wt.starts_with(&root));

        // Edits in the worktree never touch the host checkout.
        std::fs::write(wt.join("a.txt"), "changed").unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.join("a.txt")).unwrap(),
            "hello"
        );

        remove_worktree(&root, &wt).await.unwrap();
        assert!(!wt.exists());
        // Branch survives removal.
        let branches = run_git(&repo, &["branch", "--list", &info.branch])
            .await
            .unwrap();
        assert!(branches.contains(&info.branch));

        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn rejects_non_repo_empty_repo_and_bad_refs() {
        let plain = unique_dir("plain");
        let root = unique_dir("root2");
        assert!(create_worktree(&plain, &root, "x", None).await.is_err());

        let empty = unique_dir("empty");
        init_repo(&empty, false).await;
        assert!(create_worktree(&empty, &root, "y", None).await.is_err());

        let repo = unique_dir("repo2");
        init_repo(&repo, true).await;
        assert!(
            create_worktree(&repo, &root, "z", Some("--evil"))
                .await
                .is_err()
        );
        assert!(
            create_worktree(&repo, &root, "w", Some("no-such-ref"))
                .await
                .is_err()
        );

        for d in [&plain, &empty, &repo, &root] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[tokio::test]
    async fn remove_refuses_paths_outside_root() {
        let root = unique_dir("root3");
        let other = unique_dir("other");
        assert!(remove_worktree(&root, &other).await.is_err());
        assert!(remove_worktree(&root, &root).await.is_err());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&other);
    }
}
