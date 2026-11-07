# 5. Deep GitHub Integration & PR Panel (Item #4)

## Backend Additions (`src-tauri/src/github.rs`)
- Expand `github.rs` with structured Tauri commands (all executing `gh` directly with no REST polling):
  - **Auth**:
    - `gh_auth_login_web(hostname: Option<String>, scopes: Option<Vec<String>>)`: Spawns `gh auth login --web -h github.com -p https`, parses the one-time device code from stderr/stdout, emits `github:auth-code` to the frontend so the user can copy it, and awaits completion.
    - `gh_auth_login_token(token: String, hostname: Option<String>)`: Pipes token to `gh auth login --with-token` via stdin.
    - `gh_auth_logout(hostname: Option<String>)`: Runs `gh auth logout`.
  - **PR Search & Views**:
    - Update `gh_pr_list` to accept an arbitrary `search: Option<String>` query (`--search <query>`), `state: Option<String>`, and `repo: Option<String>` (`-R owner/repo`), returning `number,title,state,url,headRefName,baseRefName,author,isDraft,reviewDecision,statusCheckRollup,labels,assignees,reviewRequests,updatedAt,additions,deletions,changedFiles`.
  - **PR Inspection, Diff & Comments**:
    - `gh_pr_detail(selector, repo, cwd)`: Fetches full PR metadata including `body,comments,reviews,files,commits,statusCheckRollup,mergeable,mergeStateStatus`.
    - `gh_pr_diff(selector, repo, cwd)`: Runs `gh pr diff <selector>` and parses unified diff per file so users can view file changes and leave inline comments.
    - `gh_pr_review_comments(repo, pr_number, cwd)`: Runs `gh api repos/{owner}/{repo}/pulls/{number}/comments` to list inline file review threads.
  - **PR Mutations (Comment, Review, Approve, Request Changes, Merge)**:
    - `gh_pr_comment(selector, body, repo, cwd)`: Runs `gh pr comment <selector> --body <body>`.
    - `gh_pr_submit_review(selector, event, body, comments, repo, cwd)`:
      - Supports `"APPROVE"` (`gh pr review --approve`), `"REQUEST_CHANGES"` (`gh pr review --request-changes`), and `"COMMENT"` (`gh pr review --comment`).
      - When inline file `comments` (`Array<{ path, line, side, body }>`) are included, submits via `gh api --method POST repos/{owner}/{repo}/pulls/{number}/reviews --input -`.
    - `gh_pr_merge(selector, method, delete_branch, subject, body, repo, cwd)`: Runs `gh pr merge <selector> --<merge|squash|rebase>` (plus `--delete-branch` when selected).

## Frontend PR Panel & Saved Search Views (`src/lib/components/github/PrPanel.svelte`, `src/lib/stores/github.svelte.ts`)
- **Persistent Saved Views**:
  - Built-in default views:
    1. `Assigned to Me` (`is:open assignee:@me`)
    2. `Review Requested` (`is:open review-requested:@me`)
    3. `Created by Me` (`is:open author:@me`)
    4. `All Open PRs` (`is:open`)
  - Custom Views Builder: Users can create, edit, and delete custom views with any `gh` search syntax (e.g., `is:open label:bug draft:false`), persisted in `pipan-metadata.json` (`githubViews`).
- **PR Detail & Review Workspace**:
  - Three sub-tabs inside a selected PR:
    - **Overview & Conversation**: PR description, labels, branch badges, timeline of comments & reviews, comment composer, and **Merge PR** action bar (Merge Commit / Squash / Rebase + Delete branch).
    - **Checks & CI**: Live status of all `gh pr checks` jobs with workflow name, duration, status badge, and external log link (updated automatically when `github:webhook` events arrive).
    - **Files & Review**: Unified/split diff viewer for each changed file, click-any-line to add an inline review comment, pending review draft tray, and **Submit Review** popover (`Comment`, `Approve`, `Request Changes`).
