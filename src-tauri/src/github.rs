//! GitHub CLI (`gh`) integration.
//!
//! All GitHub access goes through direct `gh` execution — no MCP servers and no
//! REST polling. Read-only commands return parsed JSON to the frontend; live
//! updates arrive through [`crate::webhook`].

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::pi_rpc::get_augmented_path;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Name of the official extension providing `gh webhook forward`.
pub const WEBHOOK_EXTENSION: &str = "cli/gh-webhook";

/// Builds a `gh` command with a non-interactive, hidden-window environment.
pub(crate) fn gh_command(args: &[String], cwd: Option<&Path>) -> Command {
    let mut cmd = Command::new("gh");
    cmd.args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let path = get_augmented_path();
    if !path.is_empty() {
        cmd.env("PATH", path);
    }
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

fn to_args(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

fn resolve_cwd(cwd: Option<&str>) -> Result<Option<PathBuf>, String> {
    match cwd {
        None | Some("") => Ok(None),
        Some(dir) => {
            let p = PathBuf::from(dir);
            if p.is_dir() {
                Ok(Some(p))
            } else {
                Err(format!("Not a directory: {}", dir))
            }
        }
    }
}

/// Runs `gh <args>` and returns trimmed stdout.
///
/// `gh pr checks` exits with code 8 while checks are pending; that is not an
/// error as long as it produced output.
pub async fn run_gh(args: &[String], cwd: Option<&Path>) -> Result<String, String> {
    let output = gh_command(args, cwd).output().await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "GitHub CLI (`gh`) was not found on PATH. Install it from https://cli.github.com"
                .to_string()
        } else {
            format!("Failed to run `gh {}`: {}", args.join(" "), e)
        }
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let pending_checks = output.status.code() == Some(8) && !stdout.is_empty();
    if !output.status.success() && !pending_checks {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(format!("`gh {}` failed: {}", args.join(" "), detail));
    }
    Ok(stdout)
}

/// Runs `gh <args>` with `stdin_data` piped into standard input and returns trimmed stdout.
pub async fn run_gh_with_input(
    args: &[String],
    stdin_data: &str,
    cwd: Option<&Path>,
) -> Result<String, String> {
    let mut cmd = gh_command(args, cwd);
    cmd.stdin(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "GitHub CLI (`gh`) was not found on PATH. Install it from https://cli.github.com"
                .to_string()
        } else {
            format!("Failed to spawn `gh {}`: {}", args.join(" "), e)
        }
    })?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(stdin_data.as_bytes())
            .await
            .map_err(|e| format!("Failed to write to `gh` stdin: {}", e))?;
        let _ = stdin.shutdown().await;
    }
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("Failed to wait for `gh {}`: {}", args.join(" "), e))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(format!("`gh {}` failed: {}", args.join(" "), detail));
    }
    Ok(stdout)
}

/// Runs `gh <args>` and parses stdout as JSON.
pub async fn run_gh_json(args: &[String], cwd: Option<&Path>) -> Result<Value, String> {
    let out = run_gh(args, cwd).await?;
    if out.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&out)
        .map_err(|e| format!("`gh {}` did not return valid JSON: {}", args.join(" "), e))
}

/// Whether a `gh` invocation is on the read-only allowlist for [`gh_json`].
///
/// The generic command deliberately excludes mutating verbs and `gh api`.
pub fn is_allowed_gh_invocation(args: &[String]) -> bool {
    const ALLOWED: &[(&str, &[&str])] = &[
        ("pr", &["view", "list", "checks", "status"]),
        ("issue", &["view", "list", "status"]),
        ("run", &["view", "list"]),
        ("workflow", &["view", "list"]),
        ("repo", &["view", "list"]),
        ("release", &["view", "list"]),
    ];
    let mut positional = args.iter().filter(|a| !a.starts_with('-'));
    let (Some(group), Some(action)) = (positional.next(), positional.next()) else {
        return false;
    };
    // Leading flags (e.g. `-R owner/repo pr view`) are not supported: the group
    // must be the first argument so the allowlist cannot be sidestepped.
    if args.first().map(String::as_str) != Some(group.as_str()) {
        return false;
    }
    // `gh run view --log` etc. are not JSON; `--web` would open a browser.
    if args.iter().any(|a| a == "--web" || a == "-w") {
        return false;
    }
    ALLOWED
        .iter()
        .any(|(g, actions)| g == group && actions.contains(&action.as_str()))
}

/// Validates an `owner/name` repository slug.
pub fn is_valid_repo_slug(repo: &str) -> bool {
    let mut parts = repo.split('/');
    let (Some(owner), Some(name), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let ok = |s: &str| {
        !s.is_empty()
            && s != "."
            && s != ".."
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    ok(owner) && ok(name)
}

/// Whether the `gh webhook` extension is installed.
pub async fn webhook_extension_installed() -> bool {
    match run_gh(&to_args(&["extension", "list"]), None).await {
        Ok(out) => out.lines().any(|l| l.contains("gh-webhook")),
        Err(_) => false,
    }
}

/// Installs the `gh webhook` extension.
pub async fn install_webhook_extension() -> Result<(), String> {
    run_gh(&to_args(&["extension", "install", WEBHOOK_EXTENSION]), None)
        .await
        .map(|_| ())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub authenticated: bool,
    pub account: Option<String>,
    pub webhook_extension: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn parse_version(out: &str) -> Option<String> {
    out.lines()
        .next()
        .and_then(|l| l.strip_prefix("gh version "))
        .map(|v| v.split_whitespace().next().unwrap_or(v).to_string())
}

#[tauri::command]
pub async fn gh_status() -> Result<GhStatus, String> {
    let version = match run_gh(&to_args(&["--version"]), None).await {
        Ok(out) => parse_version(&out),
        Err(e) => {
            return Ok(GhStatus {
                installed: false,
                version: None,
                authenticated: false,
                account: None,
                webhook_extension: false,
                error: Some(e),
            });
        }
    };

    let (authenticated, account, error) =
        match run_gh(&to_args(&["api", "user", "--jq", ".login"]), None).await {
            Ok(login) if !login.is_empty() => (true, Some(login), None),
            Ok(_) => (false, None, None),
            Err(e) => (false, None, Some(e)),
        };

    Ok(GhStatus {
        installed: true,
        version,
        authenticated,
        account,
        webhook_extension: webhook_extension_installed().await,
        error,
    })
}

/// Generic read-only `gh` JSON command, e.g. `["pr", "view", "12", "--json", "title"]`.
#[tauri::command]
pub async fn gh_json(args: Vec<String>, cwd: Option<String>) -> Result<Value, String> {
    if !is_allowed_gh_invocation(&args) {
        return Err(format!(
            "`gh {}` is not an allowed read-only command",
            args.join(" ")
        ));
    }
    let cwd = resolve_cwd(cwd.as_deref())?;
    run_gh_json(&args, cwd.as_deref()).await
}

const PR_VIEW_FIELDS: &str = "number,title,state,url,headRefName,baseRefName,author,isDraft,\
mergeable,reviewDecision,statusCheckRollup,updatedAt,body";
const PR_LIST_FIELDS: &str = "number,title,state,url,headRefName,baseRefName,author,isDraft,\
reviewDecision,statusCheckRollup,labels,assignees,reviewRequests,updatedAt,additions,deletions,changedFiles";
const PR_DETAIL_FIELDS: &str = "number,title,state,url,headRefName,baseRefName,headRefOid,author,\
isDraft,reviewDecision,statusCheckRollup,labels,assignees,reviewRequests,updatedAt,createdAt,\
mergedAt,closedAt,additions,deletions,changedFiles,body,comments,reviews,files,commits,\
mergeable,mergeStateStatus";
const PR_CHECK_FIELDS: &str = "name,state,bucket,workflow,link,event,startedAt,completedAt";
const ISSUE_LIST_FIELDS: &str = "number,title,state,url,author,labels,updatedAt";
const RUN_LIST_FIELDS: &str = "databaseId,displayTitle,status,conclusion,workflowName,\
headBranch,event,url,createdAt";

fn fields_arg(fields: Option<Vec<String>>, default: &str) -> Result<String, String> {
    match fields {
        Some(f) if !f.is_empty() => {
            if f.iter()
                .all(|x| !x.is_empty() && x.chars().all(|c| c.is_ascii_alphanumeric()))
            {
                Ok(f.join(","))
            } else {
                Err("Invalid JSON field name".to_string())
            }
        }
        _ => Ok(default.to_string()),
    }
}

fn selector_arg(sel: &str) -> Result<(), String> {
    // PR/issue number, URL, or branch name; never a flag.
    if sel.is_empty() || sel.starts_with('-') {
        Err(format!("Invalid selector: {}", sel))
    } else {
        Ok(())
    }
}

fn repo_args(repo: Option<String>, args: &mut Vec<String>) -> Result<(), String> {
    if let Some(r) = repo.filter(|r| !r.is_empty()) {
        if !is_valid_repo_slug(&r) {
            return Err(format!("Invalid repository: {}", r));
        }
        args.push("--repo".into());
        args.push(r);
    }
    Ok(())
}

/// `gh pr view [selector] --json …`. Without a selector, uses the PR for the
/// current branch of `cwd`.
#[tauri::command]
pub async fn gh_pr_view(
    cwd: Option<String>,
    repo: Option<String>,
    selector: Option<String>,
    fields: Option<Vec<String>>,
) -> Result<Value, String> {
    let mut args = to_args(&["pr", "view"]);
    if let Some(sel) = selector.filter(|s| !s.is_empty()) {
        selector_arg(&sel)?;
        args.push(sel);
    }
    repo_args(repo, &mut args)?;
    args.push("--json".into());
    args.push(fields_arg(fields, PR_VIEW_FIELDS)?);
    run_gh_json(&args, resolve_cwd(cwd.as_deref())?.as_deref()).await
}

/// `gh pr list --json …` (single repo) or cross-repo GraphQL search when `repo` is omitted.
#[tauri::command]
pub async fn gh_pr_list(
    cwd: Option<String>,
    repo: Option<String>,
    state: Option<String>,
    search: Option<String>,
    limit: Option<u32>,
    fields: Option<Vec<String>>,
) -> Result<Value, String> {
    let resolved_cwd = resolve_cwd(cwd.as_deref())?;
    let trimmed_repo = repo.as_deref().map(str::trim).filter(|r| !r.is_empty());

    if trimmed_repo.is_none() && fields.is_none() {
        match search_prs_across_repos(
            state.as_deref(),
            search.as_deref(),
            limit.unwrap_or(50),
            resolved_cwd.as_deref(),
        )
        .await
        {
            Ok(list) => return Ok(list),
            Err(err) => {
                if resolved_cwd.is_none() {
                    return Err(err);
                }
            }
        }
    }

    let args = build_pr_list_args(repo.clone(), state, search, limit, fields)?;
    let mut val = run_gh_json(&args, resolved_cwd.as_deref()).await?;
    if let (Some(r), Some(arr)) = (trimmed_repo, val.as_array_mut()) {
        for item in arr {
            if let Some(obj) = item.as_object_mut() {
                obj.entry("repo".to_string())
                    .or_insert_with(|| Value::String(r.to_string()));
            }
        }
    }
    Ok(val)
}

const PR_GQL_FRAGMENT: &str = r#"
fragment PrFields on PullRequest {
  number
  title
  state
  url
  isDraft
  headRefName
  baseRefName
  reviewDecision
  updatedAt
  additions
  deletions
  changedFiles
  repository {
    nameWithOwner
  }
  author {
    login
  }
  labels(first: 10) {
    nodes {
      name
      color
    }
  }
  assignees(first: 10) {
    nodes {
      login
    }
  }
  reviewRequests(first: 10) {
    nodes {
      requestedReviewer {
        ... on User {
          login
        }
        ... on Team {
          name
          slug
        }
      }
    }
  }
  commits(last: 1) {
    nodes {
      commit {
        statusCheckRollup {
          state
        }
      }
    }
  }
}
"#;

pub fn has_explicit_search_scope(query: &str) -> bool {
    let lower = query.to_ascii_lowercase();
    [
        "repo:",
        "user:",
        "org:",
        "author:",
        "assignee:",
        "review-requested:",
        "team-review-requested:",
        "involves:",
        "mentions:",
    ]
    .iter()
    .any(|prefix| lower.contains(prefix))
}

pub fn normalize_pr_search_base(state: Option<&str>, search: Option<&str>) -> String {
    let mut parts: Vec<String> = Vec::new();
    let raw = search.map(str::trim).unwrap_or("");
    let lower = raw.to_ascii_lowercase();

    if !lower.contains("is:pr") && !lower.contains("type:pr") {
        parts.push("is:pr".to_string());
    }

    let has_state = lower.contains("is:open")
        || lower.contains("is:closed")
        || lower.contains("is:merged")
        || lower.contains("state:open")
        || lower.contains("state:closed")
        || lower.contains("state:merged");

    if !has_state {
        match state.map(str::trim) {
            Some("closed") => parts.push("is:closed".to_string()),
            Some("merged") => parts.push("is:merged".to_string()),
            Some("all") => {}
            _ => parts.push("is:open".to_string()),
        }
    }

    if !raw.is_empty() {
        parts.push(raw.to_string());
    }

    parts.join(" ")
}

fn is_plain_state_only_query(query: &str) -> bool {
    query.split_whitespace().all(|tok| {
        let t = tok.to_ascii_lowercase();
        matches!(
            t.as_str(),
            "is:pr"
                | "type:pr"
                | "is:open"
                | "is:closed"
                | "is:merged"
                | "state:open"
                | "state:closed"
                | "state:merged"
                | "sort:updated-desc"
        )
    })
}

fn normalize_gql_pr_node(node: &Value) -> Option<Value> {
    let obj = node.as_object()?;
    let number = obj.get("number")?.as_u64()?;
    let title = obj.get("title")?.as_str().unwrap_or("").to_string();
    let state = obj.get("state")?.as_str().unwrap_or("OPEN").to_string();
    let url = obj.get("url")?.as_str().unwrap_or("").to_string();
    let repo = node
        .pointer("/repository/nameWithOwner")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let author_login = node
        .pointer("/author/login")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();

    let labels: Vec<Value> = node
        .pointer("/labels/nodes")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|l| {
                    let name = l.get("name")?.as_str()?;
                    Some(json!({
                        "name": name,
                        "color": l.get("color").and_then(Value::as_str),
                    }))
                })
                .collect()
        })
        .unwrap_or_default();

    let assignees: Vec<Value> = node
        .pointer("/assignees/nodes")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    let login = a.get("login")?.as_str()?;
                    Some(json!({ "login": login }))
                })
                .collect()
        })
        .unwrap_or_default();

    let review_requests: Vec<Value> = node
        .pointer("/reviewRequests/nodes")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|r| {
                    let rev = r.get("requestedReviewer")?;
                    let login = rev.get("login").and_then(Value::as_str);
                    let slug = rev.get("slug").and_then(Value::as_str);
                    let name = rev.get("name").and_then(Value::as_str);
                    if login.is_none() && slug.is_none() && name.is_none() {
                        return None;
                    }
                    Some(json!({
                        "login": login.or(slug),
                        "name": name,
                        "slug": slug,
                    }))
                })
                .collect()
        })
        .unwrap_or_default();

    let rollup_state = node
        .pointer("/commits/nodes/0/commit/statusCheckRollup/state")
        .and_then(Value::as_str);
    let status_check_rollup = match rollup_state {
        Some(st) if !st.is_empty() => json!([{ "state": st }]),
        _ => json!([]),
    };

    let review_decision = obj
        .get("reviewDecision")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| Value::String(s.to_string()))
        .unwrap_or(Value::Null);

    Some(json!({
        "number": number,
        "title": title,
        "state": state,
        "url": url,
        "repo": if repo.is_empty() { Value::Null } else { Value::String(repo) },
        "headRefName": obj.get("headRefName").and_then(Value::as_str).unwrap_or(""),
        "baseRefName": obj.get("baseRefName").and_then(Value::as_str).unwrap_or(""),
        "author": {
            "login": author_login,
        },
        "isDraft": obj.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
        "reviewDecision": review_decision,
        "statusCheckRollup": status_check_rollup,
        "labels": labels,
        "assignees": assignees,
        "reviewRequests": review_requests,
        "updatedAt": obj.get("updatedAt").and_then(Value::as_str).unwrap_or(""),
        "additions": obj.get("additions").and_then(Value::as_u64).unwrap_or(0),
        "deletions": obj.get("deletions").and_then(Value::as_u64).unwrap_or(0),
        "changedFiles": obj.get("changedFiles").and_then(Value::as_u64).unwrap_or(0),
    }))
}

async fn search_prs_across_repos(
    state: Option<&str>,
    search: Option<&str>,
    limit: u32,
    cwd: Option<&Path>,
) -> Result<Value, String> {
    let clamped = limit.clamp(1, 100);
    let base = normalize_pr_search_base(state, search);

    let mut raw_nodes: Vec<Value> = Vec::new();

    if has_explicit_search_scope(&base) {
        let query_doc = format!(
            "{PR_GQL_FRAGMENT}\nquery($q: String!, $limit: Int!) {{\n  single: search(query: $q, type: ISSUE, first: $limit) {{\n    nodes {{ ...PrFields }}\n  }}\n}}"
        );
        let args = to_args(&[
            "api",
            "graphql",
            "-f",
            &format!("query={query_doc}"),
            "-f",
            &format!("q={base}"),
            "-F",
            &format!("limit={clamped}"),
        ]);
        let res = run_gh_json(&args, cwd).await?;
        if let Some(arr) = res.pointer("/data/single/nodes").and_then(Value::as_array) {
            raw_nodes.extend(arr.iter().cloned());
        }
    } else {
        let q_owned = format!("{base} user:@me");
        let q_review = format!("{base} review-requested:@me");
        let q_involved = format!("{base} involves:@me");
        let include_collab = is_plain_state_only_query(&base);
        let lower_base = base.to_ascii_lowercase();
        let gql_states = if lower_base.contains("is:merged") || lower_base.contains("state:merged")
        {
            "[MERGED]"
        } else if lower_base.contains("is:closed") || lower_base.contains("state:closed") {
            "[CLOSED, MERGED]"
        } else {
            "[OPEN]"
        };

        let collab_block = if include_collab {
            format!(
                "collab: viewer {{\n    repositories(first: 15, affiliations: [COLLABORATOR, ORGANIZATION_MEMBER], isArchived: false, orderBy: {{field: PUSHED_AT, direction: DESC}}) {{\n      nodes {{\n        viewerPermission\n        pullRequests(states: {gql_states}, first: 10, orderBy: {{field: UPDATED_AT, direction: DESC}}) {{\n          nodes {{ ...PrFields }}\n        }}\n      }}\n    }}\n  }}"
            )
        } else {
            String::new()
        };

        let query_doc = format!(
            "{PR_GQL_FRAGMENT}\nquery($qOwned: String!, $qReview: String!, $qInvolved: String!, $limit: Int!) {{\n  owned: search(query: $qOwned, type: ISSUE, first: $limit) {{\n    nodes {{ ...PrFields }}\n  }}\n  reviewRequested: search(query: $qReview, type: ISSUE, first: $limit) {{\n    nodes {{ ...PrFields }}\n  }}\n  involved: search(query: $qInvolved, type: ISSUE, first: $limit) {{\n    nodes {{ ...PrFields }}\n  }}\n  {collab_block}\n}}"
        );

        let args = to_args(&[
            "api",
            "graphql",
            "-f",
            &format!("query={query_doc}"),
            "-f",
            &format!("qOwned={q_owned}"),
            "-f",
            &format!("qReview={q_review}"),
            "-f",
            &format!("qInvolved={q_involved}"),
            "-F",
            &format!("limit={clamped}"),
        ]);
        let res = run_gh_json(&args, cwd).await?;
        for ptr in [
            "/data/owned/nodes",
            "/data/reviewRequested/nodes",
            "/data/involved/nodes",
        ] {
            if let Some(arr) = res.pointer(ptr).and_then(Value::as_array) {
                raw_nodes.extend(arr.iter().cloned());
            }
        }
        if let Some(repos) = res
            .pointer("/data/collab/repositories/nodes")
            .and_then(Value::as_array)
        {
            for repo_node in repos {
                let perm = repo_node
                    .get("viewerPermission")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if matches!(perm, "ADMIN" | "MAINTAIN" | "WRITE")
                    && let Some(prs) = repo_node
                        .pointer("/pullRequests/nodes")
                        .and_then(Value::as_array)
                {
                    raw_nodes.extend(prs.iter().cloned());
                }
            }
        }
    }

    let mut seen = std::collections::HashSet::<String>::new();
    let mut normalized: Vec<Value> = Vec::new();
    for node in &raw_nodes {
        if let Some(item) = normalize_gql_pr_node(node) {
            let key = item
                .get("url")
                .and_then(Value::as_str)
                .filter(|u| !u.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    format!(
                        "{}#{}",
                        item.get("repo").and_then(Value::as_str).unwrap_or(""),
                        item.get("number").and_then(Value::as_u64).unwrap_or(0)
                    )
                });
            if seen.insert(key) {
                normalized.push(item);
            }
        }
    }

    normalized.sort_by(|a, b| {
        let ua = a.get("updatedAt").and_then(Value::as_str).unwrap_or("");
        let ub = b.get("updatedAt").and_then(Value::as_str).unwrap_or("");
        ub.cmp(ua)
    });
    normalized.truncate(clamped as usize);

    Ok(Value::Array(normalized))
}

pub fn build_pr_list_args(
    repo: Option<String>,
    state: Option<String>,
    search: Option<String>,
    limit: Option<u32>,
    fields: Option<Vec<String>>,
) -> Result<Vec<String>, String> {
    let mut args = to_args(&["pr", "list"]);
    repo_args(repo, &mut args)?;
    push_state_limit(&mut args, state, limit)?;
    if let Some(q) = search
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        args.push("--search".into());
        args.push(q);
    }
    args.push("--json".into());
    args.push(fields_arg(fields, PR_LIST_FIELDS)?);
    Ok(args)
}

/// `gh pr checks [selector] --json …`.
#[tauri::command]
pub async fn gh_pr_checks(
    cwd: Option<String>,
    repo: Option<String>,
    selector: Option<String>,
    fields: Option<Vec<String>>,
) -> Result<Value, String> {
    let mut args = to_args(&["pr", "checks"]);
    if let Some(sel) = selector.filter(|s| !s.is_empty()) {
        selector_arg(&sel)?;
        args.push(sel);
    }
    repo_args(repo, &mut args)?;
    args.push("--json".into());
    args.push(fields_arg(fields, PR_CHECK_FIELDS)?);
    run_gh_json(&args, resolve_cwd(cwd.as_deref())?.as_deref()).await
}

/// `gh issue list --json …`.
#[tauri::command]
pub async fn gh_issue_list(
    cwd: Option<String>,
    repo: Option<String>,
    state: Option<String>,
    limit: Option<u32>,
    fields: Option<Vec<String>>,
) -> Result<Value, String> {
    let mut args = to_args(&["issue", "list"]);
    repo_args(repo, &mut args)?;
    push_state_limit(&mut args, state, limit)?;
    args.push("--json".into());
    args.push(fields_arg(fields, ISSUE_LIST_FIELDS)?);
    run_gh_json(&args, resolve_cwd(cwd.as_deref())?.as_deref()).await
}

/// `gh run list --json …` (GitHub Actions workflow runs).
#[tauri::command]
pub async fn gh_run_list(
    cwd: Option<String>,
    repo: Option<String>,
    branch: Option<String>,
    limit: Option<u32>,
    fields: Option<Vec<String>>,
) -> Result<Value, String> {
    let mut args = to_args(&["run", "list"]);
    repo_args(repo, &mut args)?;
    if let Some(b) = branch.filter(|b| !b.is_empty()) {
        selector_arg(&b)?;
        args.push("--branch".into());
        args.push(b);
    }
    if let Some(n) = limit {
        args.push("--limit".into());
        args.push(n.clamp(1, 100).to_string());
    }
    args.push("--json".into());
    args.push(fields_arg(fields, RUN_LIST_FIELDS)?);
    run_gh_json(&args, resolve_cwd(cwd.as_deref())?.as_deref()).await
}

fn push_state_limit(
    args: &mut Vec<String>,
    state: Option<String>,
    limit: Option<u32>,
) -> Result<(), String> {
    if let Some(s) = state.filter(|s| !s.is_empty()) {
        if !matches!(s.as_str(), "open" | "closed" | "merged" | "all") {
            return Err(format!("Invalid state: {}", s));
        }
        args.push("--state".into());
        args.push(s);
    }
    if let Some(n) = limit {
        args.push("--limit".into());
        args.push(n.clamp(1, 100).to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Phase 5: Auth, PR Detail, Diff, Review Comments, Comment/Review/Merge
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GhAuthCodeEvent {
    pub code: String,
    pub hostname: String,
    pub verification_uri: String,
}

pub fn is_valid_hostname(host: &str) -> bool {
    let h = host.trim();
    !h.is_empty()
        && !h.starts_with('-')
        && !h.starts_with('.')
        && !h.ends_with('.')
        && h.contains('.')
        && h.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'))
}

fn resolve_hostname(hostname: Option<String>) -> Result<String, String> {
    let host = hostname
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "github.com".to_string());
    if !is_valid_hostname(&host) {
        return Err(format!("Invalid hostname: {}", host));
    }
    Ok(host)
}

fn validate_scopes(scopes: Option<Vec<String>>) -> Result<Option<String>, String> {
    let Some(list) = scopes else {
        return Ok(None);
    };
    let cleaned: Vec<String> = list
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if cleaned.is_empty() {
        return Ok(None);
    }
    for s in &cleaned {
        if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-'))
        {
            return Err(format!("Invalid OAuth scope: {}", s));
        }
    }
    Ok(Some(cleaned.join(",")))
}

fn is_device_code_token(token: &str) -> bool {
    let bytes = token.as_bytes();
    bytes.len() == 9
        && bytes[4] == b'-'
        && bytes[..4]
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && bytes[5..]
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// Extracts the 8-character hyphenated device code (`XXXX-XXXX`) from `gh auth login --web` output.
pub fn parse_device_code(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Some((_, rest)) = line.split_once("one-time code:") {
            for word in rest.split_whitespace() {
                let trimmed = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
                if is_device_code_token(trimmed) {
                    return Some(trimmed.to_string());
                }
            }
        }
        for word in line.split_whitespace() {
            let trimmed = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
            if is_device_code_token(trimmed) {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn spawn_auth_stream_reader<R>(
    reader: R,
    app: AppHandle,
    hostname: String,
    emitted: Arc<AtomicBool>,
    collected: Arc<std::sync::Mutex<Vec<String>>>,
) -> tokio::task::JoinHandle<()>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let trimmed = line.trim().to_string();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(mut buf) = collected.lock() {
                buf.push(trimmed.clone());
            }
            if !emitted.load(Ordering::Relaxed)
                && let Some(code) = parse_device_code(&trimmed)
                && !emitted.swap(true, Ordering::Relaxed)
            {
                let verification_uri = if hostname == "github.com" {
                    "https://github.com/login/device".to_string()
                } else {
                    format!("https://{}/login/device", hostname)
                };
                let _ = app.emit(
                    "github:auth-code",
                    GhAuthCodeEvent {
                        code,
                        hostname: hostname.clone(),
                        verification_uri,
                    },
                );
            }
        }
    })
}

/// Spawns `gh auth login --web -h <hostname> -p https`, emits `github:auth-code`
/// with the one-time device code, and awaits authentication completion.
#[tauri::command]
pub async fn gh_auth_login_web(
    app: AppHandle,
    hostname: Option<String>,
    scopes: Option<Vec<String>>,
) -> Result<GhStatus, String> {
    let host = resolve_hostname(hostname)?;
    let mut args = to_args(&["auth", "login", "--web", "-h", &host, "-p", "https"]);
    if let Some(scope_csv) = validate_scopes(scopes)? {
        args.push("--scopes".into());
        args.push(scope_csv);
    }

    let mut cmd = Command::new("gh");
    cmd.args(&args)
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let path = get_augmented_path();
    if !path.is_empty() {
        cmd.env("PATH", path);
    }
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "GitHub CLI (`gh`) was not found on PATH. Install it from https://cli.github.com"
                .to_string()
        } else {
            format!("Failed to spawn `gh auth login --web`: {}", e)
        }
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"\n").await;
        let _ = stdin.shutdown().await;
    }

    let emitted = Arc::new(AtomicBool::new(false));
    let collected = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let mut tasks = Vec::new();

    if let Some(stdout) = child.stdout.take() {
        tasks.push(spawn_auth_stream_reader(
            stdout,
            app.clone(),
            host.clone(),
            emitted.clone(),
            collected.clone(),
        ));
    }
    if let Some(stderr) = child.stderr.take() {
        tasks.push(spawn_auth_stream_reader(
            stderr,
            app.clone(),
            host.clone(),
            emitted,
            collected.clone(),
        ));
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed waiting for `gh auth login --web`: {}", e))?;
    for t in tasks {
        let _ = t.await;
    }

    if !status.success() {
        let detail = collected
            .lock()
            .map(|lines| lines.join("\n"))
            .unwrap_or_default();
        return Err(format!("`gh auth login --web` failed: {}", detail));
    }

    gh_status().await
}

/// Pipes `token` to `gh auth login --with-token` via stdin.
#[tauri::command]
pub async fn gh_auth_login_token(
    token: String,
    hostname: Option<String>,
) -> Result<GhStatus, String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err("Personal access token must not be empty".to_string());
    }
    let host = resolve_hostname(hostname)?;
    let args = to_args(&["auth", "login", "--with-token", "-h", &host]);
    run_gh_with_input(&args, &format!("{trimmed}\n"), None).await?;
    gh_status().await
}

/// Runs `gh auth logout`.
#[tauri::command]
pub async fn gh_auth_logout(hostname: Option<String>) -> Result<GhStatus, String> {
    let host = resolve_hostname(hostname)?;
    let args = to_args(&["auth", "logout", "-h", &host]);
    run_gh(&args, None).await?;
    gh_status().await
}

const REPO_MERGE_FIELDS: &str = "nameWithOwner,mergeCommitAllowed,squashMergeAllowed,rebaseMergeAllowed,deleteBranchOnMerge,viewerPermission";

pub fn build_repo_merge_settings_args(repo: Option<String>) -> Result<Vec<String>, String> {
    let mut args = to_args(&["repo", "view"]);
    if let Some(r) = repo.filter(|r| !r.is_empty()) {
        if !is_valid_repo_slug(&r) {
            return Err(format!("Invalid repository: {}", r));
        }
        args.push(r);
    }
    args.push("--json".into());
    args.push(REPO_MERGE_FIELDS.into());
    Ok(args)
}

/// Fetches full PR metadata including `body,comments,reviews,files,commits,statusCheckRollup,mergeable,mergeStateStatus`
/// plus repository merge settings (`mergeCommitAllowed,squashMergeAllowed,rebaseMergeAllowed,deleteBranchOnMerge,viewerPermission`).
#[tauri::command]
pub async fn gh_pr_detail(
    selector: String,
    repo: Option<String>,
    cwd: Option<String>,
) -> Result<Value, String> {
    let resolved_cwd = resolve_cwd(cwd.as_deref())?;
    let pr_args = build_pr_detail_args(&selector, repo.clone())?;
    let repo_args = build_repo_merge_settings_args(repo.clone())?;

    let (pr_res, repo_res) = tokio::join!(
        run_gh_json(&pr_args, resolved_cwd.as_deref()),
        run_gh_json(&repo_args, resolved_cwd.as_deref())
    );

    let mut detail = pr_res?;
    if let Some(detail_obj) = detail.as_object_mut() {
        if let Some(r) = repo.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
            detail_obj
                .entry("repo".to_string())
                .or_insert_with(|| Value::String(r.to_string()));
        }
        if let Ok(Value::Object(repo_obj)) = repo_res {
            if let Some(Value::String(slug)) = repo_obj.get("nameWithOwner") {
                detail_obj.insert("repo".to_string(), Value::String(slug.clone()));
            }
            for key in [
                "mergeCommitAllowed",
                "squashMergeAllowed",
                "rebaseMergeAllowed",
                "deleteBranchOnMerge",
                "viewerPermission",
            ] {
                if let Some(v) = repo_obj.get(key) {
                    detail_obj.insert(key.to_string(), v.clone());
                }
            }
        }
    }
    Ok(detail)
}

pub fn build_pr_detail_args(selector: &str, repo: Option<String>) -> Result<Vec<String>, String> {
    let sel = selector.trim();
    selector_arg(sel)?;
    let mut args = to_args(&["pr", "view", sel]);
    repo_args(repo, &mut args)?;
    args.push("--json".into());
    args.push(PR_DETAIL_FIELDS.into());
    Ok(args)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrDiffLine {
    /// `"context" | "add" | "del" | "meta"`
    pub kind: String,
    pub content: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrDiffHunk {
    pub header: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<PrDiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrDiffFile {
    pub path: String,
    pub old_path: String,
    pub new_path: String,
    /// `"added" | "modified" | "deleted" | "renamed"`
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
    pub is_binary: bool,
    pub hunks: Vec<PrDiffHunk>,
}

fn strip_ab_prefix(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('"');
    if trimmed == "/dev/null" {
        return String::new();
    }
    trimmed
        .strip_prefix("a/")
        .or_else(|| trimmed.strip_prefix("b/"))
        .unwrap_or(trimmed)
        .to_string()
}

fn parse_range(spec: &str) -> Option<(u32, u32)> {
    let s = spec.strip_prefix('-').or_else(|| spec.strip_prefix('+'))?;
    if let Some((start, count)) = s.split_once(',') {
        Some((start.parse().ok()?, count.parse().ok()?))
    } else {
        Some((s.parse().ok()?, 1))
    }
}

pub fn parse_hunk_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    let rest = line.strip_prefix("@@ ")?;
    let (ranges, _) = rest.split_once(" @@")?;
    let mut parts = ranges.split_whitespace();
    let (old_start, old_lines) = parse_range(parts.next()?)?;
    let (new_start, new_lines) = parse_range(parts.next()?)?;
    Some((old_start, old_lines, new_start, new_lines))
}

/// Parses unified git diff output from `gh pr diff` into structured per-file hunks and lines.
pub fn parse_unified_diff(raw: &str) -> Vec<PrDiffFile> {
    let mut files: Vec<PrDiffFile> = Vec::new();
    let mut current_file: Option<PrDiffFile> = None;
    let mut current_hunk: Option<PrDiffHunk> = None;
    let mut curr_old: u32 = 0;
    let mut curr_new: u32 = 0;

    let flush_hunk = |file: &mut Option<PrDiffFile>, hunk: &mut Option<PrDiffHunk>| {
        if let (Some(f), Some(h)) = (file.as_mut(), hunk.take()) {
            f.hunks.push(h);
        }
    };

    let flush_file = |files: &mut Vec<PrDiffFile>,
                      file: &mut Option<PrDiffFile>,
                      hunk: &mut Option<PrDiffHunk>| {
        flush_hunk(file, hunk);
        if let Some(mut f) = file.take() {
            if f.path.is_empty() {
                f.path = if !f.new_path.is_empty() {
                    f.new_path.clone()
                } else {
                    f.old_path.clone()
                };
            }
            files.push(f);
        }
    };

    for raw_line in raw.lines() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);

        if let Some(rest) = line.strip_prefix("diff --git ") {
            flush_file(&mut files, &mut current_file, &mut current_hunk);
            let (old_path, new_path) = if let Some((a_part, b_part)) = rest.split_once(" b/") {
                (
                    strip_ab_prefix(a_part),
                    strip_ab_prefix(&format!("b/{b_part}")),
                )
            } else {
                let mut parts = rest.split_whitespace();
                let a = parts.next().map(strip_ab_prefix).unwrap_or_default();
                let b = parts.next().map(strip_ab_prefix).unwrap_or_default();
                (a, b)
            };
            let path = if !new_path.is_empty() {
                new_path.clone()
            } else {
                old_path.clone()
            };
            current_file = Some(PrDiffFile {
                path,
                old_path,
                new_path,
                status: "modified".to_string(),
                additions: 0,
                deletions: 0,
                is_binary: false,
                hunks: Vec::new(),
            });
            continue;
        }

        if current_hunk.is_none() {
            if let Some(f) = current_file.as_mut() {
                if line.starts_with("new file mode ") {
                    f.status = "added".to_string();
                    continue;
                }
                if line.starts_with("deleted file mode ") {
                    f.status = "deleted".to_string();
                    continue;
                }
                if let Some(from) = line.strip_prefix("rename from ") {
                    f.status = "renamed".to_string();
                    f.old_path = from.trim().to_string();
                    continue;
                }
                if let Some(to) = line.strip_prefix("rename to ") {
                    f.status = "renamed".to_string();
                    f.new_path = to.trim().to_string();
                    f.path = f.new_path.clone();
                    continue;
                }
                if line.starts_with("Binary files ") || line == "GIT binary patch" {
                    f.is_binary = true;
                    continue;
                }
                if let Some(old_hdr) = line.strip_prefix("--- ") {
                    let parsed = strip_ab_prefix(old_hdr);
                    if parsed.is_empty() {
                        f.status = "added".to_string();
                    } else {
                        f.old_path = parsed;
                    }
                    continue;
                }
                if let Some(new_hdr) = line.strip_prefix("+++ ") {
                    let parsed = strip_ab_prefix(new_hdr);
                    if parsed.is_empty() {
                        f.status = "deleted".to_string();
                        f.path = f.old_path.clone();
                    } else {
                        f.new_path = parsed.clone();
                        f.path = parsed;
                    }
                    continue;
                }
            } else if let Some(old_hdr) = line.strip_prefix("--- ") {
                let old_path = strip_ab_prefix(old_hdr);
                current_file = Some(PrDiffFile {
                    path: old_path.clone(),
                    old_path,
                    new_path: String::new(),
                    status: "modified".to_string(),
                    additions: 0,
                    deletions: 0,
                    is_binary: false,
                    hunks: Vec::new(),
                });
                continue;
            }
        }

        if line.starts_with("@@ ")
            && let Some((old_start, old_lines, new_start, new_lines)) = parse_hunk_header(line)
        {
            flush_hunk(&mut current_file, &mut current_hunk);
            curr_old = old_start;
            curr_new = new_start;
            current_hunk = Some(PrDiffHunk {
                header: line.to_string(),
                old_start,
                old_lines,
                new_start,
                new_lines,
                lines: Vec::new(),
            });
            continue;
        }

        if let (Some(f), Some(hunk)) = (current_file.as_mut(), current_hunk.as_mut()) {
            if let Some(content) = line.strip_prefix('+') {
                let line_no = curr_new;
                curr_new += 1;
                f.additions += 1;
                hunk.lines.push(PrDiffLine {
                    kind: "add".to_string(),
                    content: content.to_string(),
                    old_line: None,
                    new_line: Some(line_no),
                });
            } else if let Some(content) = line.strip_prefix('-') {
                let line_no = curr_old;
                curr_old += 1;
                f.deletions += 1;
                hunk.lines.push(PrDiffLine {
                    kind: "del".to_string(),
                    content: content.to_string(),
                    old_line: Some(line_no),
                    new_line: None,
                });
            } else if let Some(content) = line.strip_prefix(' ') {
                let o = curr_old;
                let n = curr_new;
                curr_old += 1;
                curr_new += 1;
                hunk.lines.push(PrDiffLine {
                    kind: "context".to_string(),
                    content: content.to_string(),
                    old_line: Some(o),
                    new_line: Some(n),
                });
            } else if line.starts_with('\\') {
                hunk.lines.push(PrDiffLine {
                    kind: "meta".to_string(),
                    content: line.to_string(),
                    old_line: None,
                    new_line: None,
                });
            } else if line.is_empty() {
                let o = curr_old;
                let n = curr_new;
                curr_old += 1;
                curr_new += 1;
                hunk.lines.push(PrDiffLine {
                    kind: "context".to_string(),
                    content: String::new(),
                    old_line: Some(o),
                    new_line: Some(n),
                });
            }
        }
    }

    flush_file(&mut files, &mut current_file, &mut current_hunk);
    files
}

/// Runs `gh pr diff <selector>` and parses unified diff per file.
#[tauri::command]
pub async fn gh_pr_diff(
    selector: String,
    repo: Option<String>,
    cwd: Option<String>,
) -> Result<Vec<PrDiffFile>, String> {
    let sel = selector.trim();
    selector_arg(sel)?;
    let mut args = to_args(&["pr", "diff", sel]);
    repo_args(repo, &mut args)?;
    let raw = run_gh(&args, resolve_cwd(cwd.as_deref())?.as_deref()).await?;
    Ok(parse_unified_diff(&raw))
}

pub fn review_comments_endpoint(repo: Option<&str>, pr_number: u64) -> Result<String, String> {
    if pr_number == 0 {
        return Err("Invalid PR number: 0".to_string());
    }
    match repo.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => {
            if !is_valid_repo_slug(r) {
                return Err(format!("Invalid repository: {}", r));
            }
            Ok(format!("repos/{r}/pulls/{pr_number}/comments"))
        }
        None => Ok(format!(
            "repos/{{owner}}/{{repo}}/pulls/{pr_number}/comments"
        )),
    }
}

pub fn reviews_endpoint(repo: Option<&str>, pr_number: u64) -> Result<String, String> {
    if pr_number == 0 {
        return Err("Invalid PR number: 0".to_string());
    }
    match repo.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => {
            if !is_valid_repo_slug(r) {
                return Err(format!("Invalid repository: {}", r));
            }
            Ok(format!("repos/{r}/pulls/{pr_number}/reviews"))
        }
        None => Ok(format!(
            "repos/{{owner}}/{{repo}}/pulls/{pr_number}/reviews"
        )),
    }
}

/// Runs `gh api repos/{owner}/{repo}/pulls/{number}/comments` to list inline file review threads.
#[tauri::command]
pub async fn gh_pr_review_comments(
    repo: Option<String>,
    pr_number: u64,
    cwd: Option<String>,
) -> Result<Value, String> {
    let endpoint = review_comments_endpoint(repo.as_deref(), pr_number)?;
    let args = to_args(&["api", &endpoint]);
    run_gh_json(&args, resolve_cwd(cwd.as_deref())?.as_deref()).await
}

pub fn build_pr_comment_args(
    selector: &str,
    body: &str,
    repo: Option<String>,
) -> Result<Vec<String>, String> {
    let sel = selector.trim();
    selector_arg(sel)?;
    let trimmed_body = body.trim();
    if trimmed_body.is_empty() {
        return Err("Comment body must not be empty".to_string());
    }
    let mut args = to_args(&["pr", "comment", sel, "--body", trimmed_body]);
    repo_args(repo, &mut args)?;
    Ok(args)
}

/// Runs `gh pr comment <selector> --body <body>`.
#[tauri::command]
pub async fn gh_pr_comment(
    selector: String,
    body: String,
    repo: Option<String>,
    cwd: Option<String>,
) -> Result<String, String> {
    let args = build_pr_comment_args(&selector, &body, repo)?;
    run_gh(&args, resolve_cwd(cwd.as_deref())?.as_deref()).await
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrReviewDraftComment {
    pub path: String,
    pub line: u32,
    #[serde(default)]
    pub side: Option<String>,
    pub body: String,
}

pub fn build_pr_review_args(
    selector: &str,
    event: &str,
    body: Option<&str>,
    repo: Option<String>,
) -> Result<Vec<String>, String> {
    let sel = selector.trim();
    selector_arg(sel)?;
    let ev = event.trim().to_ascii_uppercase();
    let trimmed_body = body.map(str::trim).filter(|b| !b.is_empty());

    let flag = match ev.as_str() {
        "APPROVE" => "--approve",
        "REQUEST_CHANGES" => {
            if trimmed_body.is_none() {
                return Err("Requesting changes requires a review comment body".to_string());
            }
            "--request-changes"
        }
        "COMMENT" => {
            if trimmed_body.is_none() {
                return Err("Submitting a comment review requires a body".to_string());
            }
            "--comment"
        }
        other => return Err(format!("Invalid review event: {}", other)),
    };

    let mut args = to_args(&["pr", "review", sel, flag]);
    if let Some(b) = trimmed_body {
        args.push("--body".into());
        args.push(b.to_string());
    }
    repo_args(repo, &mut args)?;
    Ok(args)
}

pub fn build_pr_review_api_payload(
    event: &str,
    body: Option<&str>,
    comments: &[PrReviewDraftComment],
) -> Result<Value, String> {
    let ev = event.trim().to_ascii_uppercase();
    if !matches!(ev.as_str(), "APPROVE" | "REQUEST_CHANGES" | "COMMENT") {
        return Err(format!("Invalid review event: {}", event));
    }
    if comments.is_empty() {
        return Err("Inline comments list must not be empty for API payload".to_string());
    }
    let mut serialized_comments = Vec::with_capacity(comments.len());
    for c in comments {
        let path = c.path.trim();
        let comment_body = c.body.trim();
        if path.is_empty() {
            return Err("Inline review comment path must not be empty".to_string());
        }
        if c.line == 0 {
            return Err("Inline review comment line must be >= 1".to_string());
        }
        if comment_body.is_empty() {
            return Err("Inline review comment body must not be empty".to_string());
        }
        let side = c
            .side
            .as_deref()
            .map(|s| s.trim().to_ascii_uppercase())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "RIGHT".to_string());
        if !matches!(side.as_str(), "LEFT" | "RIGHT") {
            return Err(format!("Invalid diff side: {}", side));
        }
        serialized_comments.push(json!({
            "path": path,
            "line": c.line,
            "side": side,
            "body": comment_body,
        }));
    }

    Ok(json!({
        "event": ev,
        "body": body.map(str::trim).unwrap_or(""),
        "comments": serialized_comments,
    }))
}

/// Submits a PR review (`APPROVE`, `REQUEST_CHANGES`, `COMMENT`), either via
/// `gh pr review` or via `gh api --method POST repos/{owner}/{repo}/pulls/{number}/reviews --input -`
/// when inline file `comments` are included.
#[tauri::command]
pub async fn gh_pr_submit_review(
    selector: String,
    event: String,
    body: Option<String>,
    comments: Option<Vec<PrReviewDraftComment>>,
    repo: Option<String>,
    cwd: Option<String>,
) -> Result<Value, String> {
    let resolved_cwd = resolve_cwd(cwd.as_deref())?;
    let inline = comments.unwrap_or_default();

    if inline.is_empty() {
        let args = build_pr_review_args(&selector, &event, body.as_deref(), repo)?;
        let out = run_gh(&args, resolved_cwd.as_deref()).await?;
        return Ok(Value::String(out));
    }

    let sel = selector.trim();
    selector_arg(sel)?;
    let pr_number: u64 = if let Ok(n) = sel.parse::<u64>() {
        n
    } else {
        let mut view_args = to_args(&["pr", "view", sel]);
        repo_args(repo.clone(), &mut view_args)?;
        view_args.push("--json".into());
        view_args.push("number".into());
        let val = run_gh_json(&view_args, resolved_cwd.as_deref()).await?;
        val.get("number")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("Could not resolve PR number for selector `{}`", sel))?
    };

    let endpoint = reviews_endpoint(repo.as_deref(), pr_number)?;
    let payload = build_pr_review_api_payload(&event, body.as_deref(), &inline)?;
    let payload_str =
        serde_json::to_string(&payload).map_err(|e| format!("Serialize review payload: {e}"))?;
    let args = to_args(&["api", "--method", "POST", &endpoint, "--input", "-"]);
    let out = run_gh_with_input(&args, &payload_str, resolved_cwd.as_deref()).await?;
    if out.is_empty() {
        Ok(Value::Null)
    } else {
        Ok(serde_json::from_str(&out).unwrap_or(Value::String(out)))
    }
}

pub fn build_pr_merge_args(
    selector: &str,
    method: &str,
    delete_branch: bool,
    subject: Option<&str>,
    body: Option<&str>,
    admin: bool,
    repo: Option<String>,
) -> Result<Vec<String>, String> {
    let sel = selector.trim();
    selector_arg(sel)?;
    let method_flag = match method.trim().to_ascii_lowercase().as_str() {
        "merge" => "--merge",
        "squash" => "--squash",
        "rebase" => "--rebase",
        other => return Err(format!("Invalid merge method: {}", other)),
    };
    let mut args = to_args(&["pr", "merge", sel, method_flag]);
    if delete_branch {
        args.push("--delete-branch".into());
    }
    if admin {
        args.push("--admin".into());
    }
    if let Some(s) = subject.map(str::trim).filter(|s| !s.is_empty()) {
        args.push("--subject".into());
        args.push(s.to_string());
    }
    if let Some(b) = body.map(str::trim).filter(|b| !b.is_empty()) {
        args.push("--body".into());
        args.push(b.to_string());
    }
    repo_args(repo, &mut args)?;
    Ok(args)
}

/// Runs `gh pr merge <selector> --<merge|squash|rebase>` (plus `--delete-branch` and `--admin` when selected).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn gh_pr_merge(
    selector: String,
    method: String,
    delete_branch: Option<bool>,
    subject: Option<String>,
    body: Option<String>,
    admin: Option<bool>,
    repo: Option<String>,
    cwd: Option<String>,
) -> Result<String, String> {
    let args = build_pr_merge_args(
        &selector,
        &method,
        delete_branch.unwrap_or(false),
        subject.as_deref(),
        body.as_deref(),
        admin.unwrap_or(false),
        repo,
    )?;
    run_gh(&args, resolve_cwd(cwd.as_deref())?.as_deref()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(args: &[&str]) -> Vec<String> {
        to_args(args)
    }

    #[test]
    fn allowlist_accepts_read_only_commands() {
        assert!(is_allowed_gh_invocation(&v(&[
            "pr", "view", "1", "--json", "title"
        ])));
        assert!(is_allowed_gh_invocation(&v(&[
            "run", "list", "--json", "status"
        ])));
        assert!(is_allowed_gh_invocation(&v(&[
            "pr", "checks", "--json", "name"
        ])));
    }

    #[test]
    fn allowlist_rejects_mutations_and_bypasses() {
        assert!(!is_allowed_gh_invocation(&v(&["pr", "merge", "1"])));
        assert!(!is_allowed_gh_invocation(&v(&[
            "api",
            "-X",
            "DELETE",
            "/repos/a/b"
        ])));
        assert!(!is_allowed_gh_invocation(&v(&["issue", "close", "3"])));
        assert!(!is_allowed_gh_invocation(&v(&[
            "-R", "a/b", "pr", "merge", "1"
        ])));
        assert!(!is_allowed_gh_invocation(&v(&["pr", "view", "--web"])));
        assert!(!is_allowed_gh_invocation(&v(&["pr"])));
        assert!(!is_allowed_gh_invocation(&[]));
    }

    #[test]
    fn repo_slug_validation() {
        assert!(is_valid_repo_slug("Steven-Harris/pipan"));
        assert!(is_valid_repo_slug("a/b.c_d"));
        assert!(!is_valid_repo_slug("pipan"));
        assert!(!is_valid_repo_slug("a/b/c"));
        assert!(!is_valid_repo_slug("a/--flag b"));
        assert!(!is_valid_repo_slug("../x"));
        assert!(!is_valid_repo_slug("/x"));
    }

    #[test]
    fn field_and_selector_validation() {
        assert_eq!(fields_arg(None, "a,b").unwrap(), "a,b");
        assert_eq!(
            fields_arg(Some(vec!["title".into(), "url".into()]), "a").unwrap(),
            "title,url"
        );
        assert!(fields_arg(Some(vec!["a;b".into()]), "x").is_err());
        assert!(selector_arg("--web").is_err());
        assert!(selector_arg("123").is_ok());
    }

    #[test]
    fn parses_version() {
        assert_eq!(
            parse_version("gh version 2.103.3 (2026-10-26)\nhttps://x"),
            Some("2.103.3".to_string())
        );
        assert_eq!(parse_version("garbage"), None);
    }

    #[test]
    fn parses_device_code_from_gh_output() {
        assert_eq!(
            parse_device_code("! First copy your one-time code: AB12-CD34\nPress Enter"),
            Some("AB12-CD34".to_string())
        );
        assert_eq!(
            parse_device_code("Use code 9F8E-7D6C to authenticate."),
            Some("9F8E-7D6C".to_string())
        );
        assert_eq!(parse_device_code("no code here 123-456"), None);
    }

    #[test]
    fn validates_hostnames_and_scopes() {
        assert!(is_valid_hostname("github.com"));
        assert!(is_valid_hostname("ghe.example.internal"));
        assert!(!is_valid_hostname("-bad.com"));
        assert!(!is_valid_hostname("localhost"));
        assert!(!is_valid_hostname("github.com/evil"));

        assert_eq!(
            validate_scopes(Some(vec!["repo".into(), "read:org".into()])).unwrap(),
            Some("repo,read:org".to_string())
        );
        assert!(validate_scopes(Some(vec!["repo;--bad".into()])).is_err());
    }

    #[test]
    fn builds_pr_list_and_detail_args() {
        let args = build_pr_list_args(
            Some("Steven-Harris/pipan".into()),
            Some("open".into()),
            Some("is:open assignee:@me".into()),
            Some(25),
            None,
        )
        .unwrap();
        assert_eq!(
            args,
            v(&[
                "pr",
                "list",
                "--repo",
                "Steven-Harris/pipan",
                "--state",
                "open",
                "--limit",
                "25",
                "--search",
                "is:open assignee:@me",
                "--json",
                PR_LIST_FIELDS,
            ])
        );

        let detail_args = build_pr_detail_args("42", Some("Steven-Harris/pipan".into())).unwrap();
        assert_eq!(
            detail_args,
            v(&[
                "pr",
                "view",
                "42",
                "--repo",
                "Steven-Harris/pipan",
                "--json",
                PR_DETAIL_FIELDS,
            ])
        );
        assert!(build_pr_detail_args("--bad", None).is_err());
    }

    #[test]
    fn parses_unified_diff_into_files_hunks_and_line_numbers() {
        let sample = "\
diff --git a/src/main.rs b/src/main.rs
index 1111111..2222222 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -10,4 +10,5 @@ fn main() {
     let a = 1;
-    let b = 2;
+    let b = 20;
+    let c = 30;
     println!(\"{a}\");
diff --git a/src/new.rs b/src/new.rs
new file mode 100644
--- /dev/null
+++ b/src/new.rs
@@ -0,0 +1,2 @@
+pub fn hello() {}
+pub fn world() {}
";
        let files = parse_unified_diff(sample);
        assert_eq!(files.len(), 2);

        let f0 = &files[0];
        assert_eq!(f0.path, "src/main.rs");
        assert_eq!(f0.status, "modified");
        assert_eq!(f0.additions, 2);
        assert_eq!(f0.deletions, 1);
        assert_eq!(f0.hunks.len(), 1);

        let h0 = &f0.hunks[0];
        assert_eq!(h0.old_start, 10);
        assert_eq!(h0.new_start, 10);
        assert_eq!(h0.lines.len(), 5);
        assert_eq!(h0.lines[0].kind, "context");
        assert_eq!(h0.lines[0].old_line, Some(10));
        assert_eq!(h0.lines[0].new_line, Some(10));
        assert_eq!(h0.lines[1].kind, "del");
        assert_eq!(h0.lines[1].old_line, Some(11));
        assert_eq!(h0.lines[1].new_line, None);
        assert_eq!(h0.lines[2].kind, "add");
        assert_eq!(h0.lines[2].old_line, None);
        assert_eq!(h0.lines[2].new_line, Some(11));
        assert_eq!(h0.lines[3].kind, "add");
        assert_eq!(h0.lines[3].new_line, Some(12));
        assert_eq!(h0.lines[4].kind, "context");
        assert_eq!(h0.lines[4].old_line, Some(12));
        assert_eq!(h0.lines[4].new_line, Some(13));

        let f1 = &files[1];
        assert_eq!(f1.path, "src/new.rs");
        assert_eq!(f1.status, "added");
        assert_eq!(f1.additions, 2);
        assert_eq!(f1.deletions, 0);
    }

    #[test]
    fn builds_pr_comment_review_and_merge_commands() {
        let comment =
            build_pr_comment_args("15", "Looks great!", Some("Steven-Harris/pipan".into()))
                .unwrap();
        assert_eq!(
            comment,
            v(&[
                "pr",
                "comment",
                "15",
                "--body",
                "Looks great!",
                "--repo",
                "Steven-Harris/pipan"
            ])
        );
        assert!(build_pr_comment_args("15", "   ", None).is_err());

        let approve = build_pr_review_args("15", "APPROVE", None, None).unwrap();
        assert_eq!(approve, v(&["pr", "review", "15", "--approve"]));

        let req_changes =
            build_pr_review_args("15", "REQUEST_CHANGES", Some("Please fix tests"), None).unwrap();
        assert_eq!(
            req_changes,
            v(&[
                "pr",
                "review",
                "15",
                "--request-changes",
                "--body",
                "Please fix tests"
            ])
        );
        assert!(build_pr_review_args("15", "REQUEST_CHANGES", None, None).is_err());

        let payload = build_pr_review_api_payload(
            "COMMENT",
            Some("Inline feedback"),
            &[PrReviewDraftComment {
                path: "src/lib.rs".into(),
                line: 42,
                side: Some("RIGHT".into()),
                body: "Nit: rename variable".into(),
            }],
        )
        .unwrap();
        assert_eq!(payload["event"], "COMMENT");
        assert_eq!(payload["body"], "Inline feedback");
        assert_eq!(payload["comments"][0]["path"], "src/lib.rs");
        assert_eq!(payload["comments"][0]["line"], 42);
        assert_eq!(payload["comments"][0]["side"], "RIGHT");

        let merge = build_pr_merge_args(
            "15",
            "squash",
            true,
            Some("feat: ship it"),
            Some("Detailed body"),
            false,
            Some("Steven-Harris/pipan".into()),
        )
        .unwrap();
        assert_eq!(
            merge,
            v(&[
                "pr",
                "merge",
                "15",
                "--squash",
                "--delete-branch",
                "--subject",
                "feat: ship it",
                "--body",
                "Detailed body",
                "--repo",
                "Steven-Harris/pipan"
            ])
        );

        let admin_merge = build_pr_merge_args(
            "15",
            "squash",
            false,
            None,
            None,
            true,
            Some("Steven-Harris/pipan".into()),
        )
        .unwrap();
        assert_eq!(
            admin_merge,
            v(&[
                "pr",
                "merge",
                "15",
                "--squash",
                "--admin",
                "--repo",
                "Steven-Harris/pipan"
            ])
        );

        assert!(build_pr_merge_args("15", "fast-forward", false, None, None, false, None).is_err());
    }

    #[test]
    fn normalizes_cross_repo_search_and_repo_merge_settings() {
        assert_eq!(
            normalize_pr_search_base(Some("open"), Some("is:open")),
            "is:pr is:open"
        );
        assert_eq!(
            normalize_pr_search_base(Some("all"), Some("is:closed author:@me")),
            "is:pr is:closed author:@me"
        );
        assert!(!has_explicit_search_scope("is:pr is:open"));
        assert!(has_explicit_search_scope(
            "is:pr is:open review-requested:@me"
        ));
        assert!(has_explicit_search_scope("is:pr is:open user:@me"));
        assert!(has_explicit_search_scope(
            "is:pr is:open repo:Steven-Harris/githelm"
        ));

        let repo_args =
            build_repo_merge_settings_args(Some("Steven-Harris/githelm".into())).unwrap();
        assert_eq!(
            repo_args,
            v(&[
                "repo",
                "view",
                "Steven-Harris/githelm",
                "--json",
                REPO_MERGE_FIELDS
            ])
        );

        let sample_node = json!({
            "number": 233,
            "title": "Bump @sveltejs/kit from 2.70.2 to 3.0.0",
            "state": "OPEN",
            "url": "https://github.com/Steven-Harris/githelm/pull/233",
            "isDraft": false,
            "headRefName": "dependabot/npm_and_yarn/sveltejs/kit-3.0.0",
            "baseRefName": "main",
            "reviewDecision": "",
            "updatedAt": "2026-10-29T13:06:18Z",
            "additions": 347,
            "deletions": 187,
            "changedFiles": 2,
            "repository": { "nameWithOwner": "Steven-Harris/githelm" },
            "author": { "login": "dependabot" },
            "labels": { "nodes": [{ "name": "dependencies", "color": "0366d6" }] },
            "assignees": { "nodes": [] },
            "reviewRequests": { "nodes": [] },
            "commits": { "nodes": [{ "commit": { "statusCheckRollup": { "state": "FAILURE" } } }] }
        });
        let normalized = normalize_gql_pr_node(&sample_node).unwrap();
        assert_eq!(normalized["number"], 233);
        assert_eq!(normalized["repo"], "Steven-Harris/githelm");
        assert_eq!(normalized["author"]["login"], "dependabot");
        assert_eq!(normalized["statusCheckRollup"][0]["state"], "FAILURE");
    }
}
