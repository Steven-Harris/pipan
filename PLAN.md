# Pipan v0.2 Implementation Plan — Modular Workspace, Persistent Sessions, Deep GitHub Integration & Extension System

## Summary

This plan replaces the initial Phase 1–5 prototype plan with the complete architecture for the next version of **Pipan**. It transitions the app into a modular, VS Code-style extensible workspace that defaults to a clean Chat-first session view with on-demand split panels (Nodes DAG, Terminal, File Explorer, Pull Requests), reliable node layout with per-session coordinate persistence, persistent Pi sessions with full lifecycle management (search, group, filter, sort, rename, archive, delete, stop), deep `gh` CLI PR review/merge workflows, skill & model selectors, drag-and-drop prompt attachments, a historical Cost & Usage analytics page, an interactive dependency Doctor with auto-remediation, in-app Pi/GitHub login & configuration, and a bottom status bar.

---

## 1. Workspace Layout, Chat-First View & Split Panel System (Items #1, #3, #16)

### Behavior & UI Changes
- **Chat-First Default View**: Opening or starting a session renders a clean, full-height chat interface (`MessageList` + `ChatInput`) without forcing the DAG canvas open.
- **Session Auxiliary Panels**: From the session header toolbar (or command palette/status bar), users can toggle open one or more auxiliary panels alongside the chat:
  - **Node Graph (`FlowCanvas`)**
  - **Terminal (`TerminalPanel`)**
  - **Files (`FilePanel`)**
  - **Pull Requests (`PrPanel`)**
- **Vertical & Horizontal Split Orientation**:
  - Users can select how the auxiliary panel area splits with the Chat window:
    - **Vertical Split (Default)**: Side-by-side layout (Chat on the left, active panel(s) on the right, separated by a draggable vertical splitter).
    - **Horizontal Split**: Top-and-bottom stacked layout (active panel(s) on top, Chat on the bottom, separated by a draggable horizontal splitter).
  - Inside the Node Graph panel itself, Dagre auto-layout also defaults to **Vertical (`TB`)** and can be toggled to **Horizontal (`LR`)**.
  - Split orientation, panel open/closed state, active panel tab, and split ratio persist in user settings.

---

## 2. Node Graph Rendering, Viewport Fitting & Per-Session Persistence (Item #2)

### Root Cause Fixes
- **Decouple Streaming Deltas from Dagre Re-Layout**:
  - Previously, `$effect` in `FlowCanvas.svelte` re-ran `dagre.layout()` on every streaming character delta, overwriting dragged positions and causing sporadic node jumping.
  - Separate **structural graph topology changes** (node added/removed, edge added/removed, layout direction changed) from **data updates** (streaming text, thinking deltas, tool status updates). Data updates mutate `node.data` in place while preserving `node.position`.
- **Compact, Predictable Placement & Viewport Containment**:
  - Update `layout.ts` with tighter, consistent spacing (`nodesep: 28`, `ranksep: 44`, `marginx: 20`, `marginy: 20`) and accurate node bounding sizes.
  - When a new node is added, calculate layout only for unpinned nodes (or place new child nodes directly adjacent to their parent node along the active rank axis) so existing user-positioned nodes never jump.
  - Trigger `fitView({ padding: 0.16, maxZoom: 1.0, duration: 200 })` (or viewport pan-into-view) when structural nodes are added so nodes always render within the current visible canvas unless the canvas is smaller than a single node.
- **Per-Session Node Position Persistence**:
  - Track `onnodeclick` / `onnodedragstop` in `FlowCanvas.svelte` and save `{ [nodeId]: { x, y, pinned?: boolean } }` into the active session's metadata (`sessionMeta.nodePositions`).
  - When navigating away (to Cost, Settings, PRs, or another session) and returning to a session, restore the saved `nodePositions` map before rendering. Provide a `"Reset Layout"` button in the canvas toolbar to clear pinned coordinates and re-run Dagre.

---

## 3. Persistent Sessions: Stop, Search, Group, Filter, Sort, Rename, Archive & Delete (Items #6, #7, #8, #9)

### Backend Changes (`src-tauri/src/pi_rpc.rs`, `src-tauri/src/storage.rs`)
- **Real Pi Session Persistence**:
  - Update `build_pi_command_with_args` so persistent sessions run without `--no-session`, passing `--session <session_jsonl_path>` (stored under `<app_local_data_dir>/sessions/<session_id>.jsonl` or `~/.pi/agent/sessions/`) and `--name <label>` when configured. Keep an explicit ephemeral option only when requested.
  - Support switching/resuming sessions via Pi RPC `switch_session` (`{"type": "switch_session", "sessionPath": "..."}`) and `get_messages` / `get_state` hydration when reopening a saved session.
- **Pipan Local Metadata Store (`src-tauri/src/storage.rs`)**:
  - Persist `<app_local_data_dir>/pipan-metadata.json` containing:
    - `sessions: Record<sessionId, SessionRecord>` with `id`, `name`, `sessionFilePath`, `cwd`, `mode`, `group`, `tags`, `archived`, `pinned`, `createdAt`, `updatedAt`, `messageCount`, `toolCallCount`, `totalTokens`, `totalCostUsd`, `modelId`, `provider`, and `nodePositions`.
    - Tauri commands: `list_saved_sessions`, `save_session_metadata`, `rename_session`, `archive_session`, `delete_session`.

### Frontend Changes (`src/lib/stores/chat.svelte.ts`, `src/lib/components/Sidebar.svelte`, `SessionHeader.svelte`)
- **Stop Running Sessions (#6)**:
  - Add a Stop button directly on every running session item in `Sidebar.svelte`, in `SessionHeader.svelte`, and in `ChatInput.svelte`.
  - Clicking Stop on an active generation sends `clear_queue` + `abort`; clicking Stop Process terminates the child sidecar (`stop_pi_session` or `stop_agent_session`).
- **Search, Group, Filter, and Sort (#7)**:
  - **Search**: Real-time filter input matching session name, folder path, group, tags, or branch.
  - **Filter**: Quick filter pills/dropdown for `Active` (non-archived default), `Running`, `Archived`, `Directory`, `Headless`, `Worktree`.
  - **Group By**: `None`, `Workspace Folder`, `Custom Group`, `Date Bucket (Today / This Week / Older)`.
  - **Sort By**: `Last Updated` (default), `Created Date`, `Name (A–Z)`, `Total Cost`, `Message Count`.
- **Rename (#9)**:
  - Inline double-click / edit button on session cards and in `SessionHeader.svelte`. Updates `SessionRecord.name` in `pipan-metadata.json` and sends `{"type": "set_session_name", "name": newName}` over Pi RPC if connected.
- **Archive & Delete (#8)**:
  - Archive toggle moves sessions into/out of the Archived filter view.
  - Delete button removes the session from metadata, deletes its `.jsonl` session file on disk, stops any running process for that session, and prunes any associated git worktree.

---

## 4. Model Selector, Skills Picker & Drag-and-Drop File Attachments (Items #5, #10, #11)

### Model & Thinking Selector (#10)
- **Pi RPC Integration (`src/lib/stores/chat.svelte.ts`)**:
  - On session connect (after `get_state`), invoke `{"type": "get_available_models"}` and `{"type": "get_available_thinking_levels"}` over Pi RPC and store `availableModels: PiFullModelInfo[]` and `availableThinkingLevels: string[]`.
  - Add Rust fallback command `list_configured_models` that reads `~/.pi/agent/models.json` and `~/.pi/agent/settings.json` so model selection works before a session starts and passes `--model <provider/id>` on launch.
- **UI (`ModelSelector.svelte`)**:
  - Searchable popover in `SessionHeader.svelte` and `ChatInput.svelte` grouped by provider, showing context window size, image support badge, reasoning badge, and $/1M token pricing.
  - Selecting a model sends `{"type": "set_model", "provider": ..., "modelId": ...}` and refreshes available thinking levels; selecting a thinking level sends `{"type": "set_thinking_level", "level": ...}`.

### Installed Skills Picker (#5)
- **Skill Discovery (`src-tauri/src/skills.rs` + Pi RPC `get_commands`)**:
  - Query Pi RPC `{"type": "get_commands"}` when connected to list all discovered skills (`source === "skill"`), prompt templates (`source === "prompt"`), and extension commands (`source === "extension"`).
  - Also provide a Rust command `list_installed_skills(cwd: Option<String>)` that scans `~/.pi/agent/skills/**/SKILL.md`, `~/.agents/skills/**/SKILL.md`, and `<cwd>/.pi/skills/**/SKILL.md` (parsing YAML frontmatter `name` and `description`) so skills are always browsable.
- **UI (`SkillPickerPopover.svelte` in `ChatInput.svelte`)**:
  - Add a `Skills` button in the composer toolbar and `/` slash-command autocomplete trigger when typing `/`.
  - Selecting a skill adds a removable `/skill:<name>` pill (or inserts `/skill:<name> ` into the prompt) so Pi expands the skill on submission.

### Drag-and-Drop Files into Prompt (#11)
- **File Drop & Attachment Pipeline (`ChatInput.svelte` + `src-tauri/src/files.rs`)**:
  - Listen to both HTML5 drag-and-drop events and Tauri's window drag-drop event (`onDragDropEvent`) plus an `"Attach Files"` button using `@tauri-apps/plugin-dialog`.
  - **Image Files (`png`, `jpg`, `jpeg`, `gif`, `webp`, `bmp`)**: Read via a Rust command `read_file_attachment(path)` (or `FileReader` for browser drops) into `{ type: "image", data: base64, mimeType }` and pass in the `images` array of Pi RPC `prompt` / `steer` / `follow_up`. Show image thumbnails above the input.
  - **Code / Text / Other Files**: Render removable file chips above the input showing filename and relative path; when submitting the prompt, prepend `@<relative_or_abs_path>` references (or inline fenced content if dropped from outside the workspace) so Pi's file tools have immediate context.

---

## 5. Deep GitHub Integration & PR Panel (Item #4)

### Backend Additions (`src-tauri/src/github.rs`)
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

### Frontend PR Panel & Saved Search Views (`src/lib/components/github/PrPanel.svelte`, `src/lib/stores/github.svelte.ts`)
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

---

## 6. Session Terminal & File Explorer Panels (Item #16)

### Integrated Workspace Terminal (`src-tauri/src/terminal.rs`, `src/lib/components/panels/TerminalPanel.svelte`)
- **Backend Shell Session Manager (`terminal.rs`)**:
  - Manages persistent interactive shell processes per session (`powershell.exe` / `cmd.exe` on Windows, `$SHELL` / `zsh` / `bash` on macOS/Linux) rooted in the active session's `cwd` or worktree.
  - Tauri commands: `start_terminal_session(session_id, cwd)`, `send_terminal_input(session_id, data)`, `resize_terminal`, `stop_terminal_session`.
  - Streams output lines/chunks over `terminal:output` and exit status over `terminal:exit`.
- **Frontend Terminal Panel (`TerminalPanel.svelte`)**:
  - Renders streaming ANSI/text output, command input with up/down command history, working directory indicator, Clear and Kill/Interrupt (`Ctrl+C`) buttons, and a **"Send to Pi (`bash` RPC)"** toggle that executes the command via Pi's `bash` RPC command so its output is automatically added to the LLM conversation context.

### Workspace File Explorer Panel (`src-tauri/src/files.rs`, `src/lib/components/panels/FilePanel.svelte`)
- **Backend File & Git Status Commands (`files.rs`)**:
  - `list_workspace_files(cwd, relative_dir)`: Lists directory entries (respecting `.gitignore` or toggling hidden files) with file size and type.
  - `get_git_file_statuses(cwd)`: Runs `git status --porcelain=v1` to annotate modified (`M`), added (`A`), untracked (`?`), and deleted (`D`) files.
  - `read_workspace_file(cwd, relative_path, max_bytes)`: Reads file content or `git diff` for preview.
- **Frontend File Panel (`FilePanel.svelte`)**:
  - Expandable directory tree with git status badges, quick file filter, read-only code/diff viewer, and a one-click **"+ Attach to Prompt"** button (or drag into `ChatInput`).

---

## 7. Cost & Usage Analytics Page & Bottom Status Bar (Items #12, #13)

### Usage Indexer & Cost Aggregation (`src-tauri/src/usage.rs`, `src/lib/stores/usage.svelte.ts`)
- **Backend Usage Aggregator (`usage.rs`)**:
  - Command `get_usage_analytics()`: Scans both `~/.pi/agent/sessions/**/*.jsonl` (parsing `message` entries with `usage.cost.total`, `usage.input`, `usage.output`, `usage.cacheRead`, `usage.cacheWrite`, `timestamp`, `model`, `provider`) AND Pipan's local `pipan-metadata.json` ledger (so even headless sessions are recorded).
  - Deduplicates by session/message ID and returns time-series buckets and totals for:
    - **Day** (hourly breakdown for today + last 30 days daily)
    - **Week** (daily breakdown for current week + last 12 weeks)
    - **Month** (daily/weekly breakdown for current month + last 12 months)
    - **Year** (monthly breakdown by year)
  - Also aggregates live active-session stats via Pi RPC `get_session_stats`.

### Cost Page UI (`src/lib/components/views/CostView.svelte`)
- Accessible from the left navigation rail or by clicking the cost indicator in the bottom status bar.
- Period selector tabs: **Day | Week | Month | Year**.
- Summary KPI cards: Total Spend ($), Total Tokens (Input / Output / Cache Read / Cache Write), Active & Total Sessions, Average Cost per Session.
- Interactive bar chart of spend over the selected time granularity + breakdown tables by **Model / Provider** and **Workspace / Session**.

### Bottom Status Bar (`src/lib/components/StatusBar.svelte`) (Item #13)
- Persistent 28px bottom bar across the entire app showing:
  - **Left**: Pi connection status pill, active workspace/worktree branch, active background subagent count, GitHub account & webhook status.
  - **Right**: Active session token/context window % (`contextUsage.percent`), current session cost & total cumulative cost (clicks to open **Cost Page**), total session count (`active / total`), active Model & Thinking level (clicks to open **Model Selector**), Extensions count (clicks to open **Extensions**), and **Doctor** health status indicator.

---

## 8. In-App Login/Configuration & Dependency Doctor (Items #14, #15)

### Login & Configuration Modal/View (`src-tauri/src/config.rs`, `src/lib/components/views/SettingsView.svelte`) (Item #14)
- **Pi Authentication & Settings**:
  - Reads and writes `<agent-dir>/auth.json` (`~/.pi/agent/auth.json`) and `<agent-dir>/settings.json` safely in Rust:
    - Configure API keys for Anthropic, OpenAI, Google Gemini, OpenRouter, xAI, Mistral, Groq, or custom providers (stored with masked previews in UI).
    - Launch interactive `pi` provider OAuth/login helper in an embedded terminal flow if needed.
    - Edit default provider/model, default thinking level, `autoCompaction`, `autoRetry`, `steeringMode`, `followUpMode`, and custom model endpoints (`~/.pi/agent/models.json`).
- **GitHub Authentication & Settings**:
  - Shows current `gh auth status` (account login, scopes, protocol).
  - **Login with Browser (`gh auth login --web`)**: Displays the 8-character one-time device code with a "Copy & Open GitHub" button and polls/waits for the spawned `gh` process to finish authenticating.
  - **Login with Token**: Input field for Personal Access Token (`gh auth login --with-token`).
  - **Logout** button and default repository/webhook event preferences.

### System Doctor & Auto-Fixer (`src-tauri/src/doctor.rs`, `src/lib/components/modals/DoctorModal.svelte`) (Item #15)
- **Diagnostic Checks (`run_doctor`)**:
  1. `git`: Checks `git --version` and user `user.name` / `user.email` config.
  2. `node_npm`: Checks `node --version` and `npm`/`pnpm` availability.
  3. `pi_cli`: Checks `pi --version` on the augmented PATH.
  4. `pi_auth`: Checks whether at least one provider API key or credential is configured in `~/.pi/agent/auth.json` or environment variables.
  5. `gh_cli`: Checks `gh --version` on the augmented PATH.
  6. `gh_auth`: Checks `gh auth status` authentication state.
  7. `gh_webhook`: Checks `gh extension list` for `cli/gh-webhook`.
- **Auto-Fix Actions (`fix_doctor_issue(check_id)`)**:
  - Every failed or warning check displays both step-by-step manual instructions AND a **"Fix for Me"** button:
    - Missing `pi_cli`: Runs `npm install -g @earendil-works/pi-coding-agent` (or `pnpm add -g`) and refreshes augmented PATH cache.
    - Missing `gh_cli` / `git`: Runs platform package manager (`winget install --id GitHub.cli -e` / `brew install gh` / `scoop install gh`) with live output streaming.
    - Missing `gh_webhook`: Runs `gh extension install cli/gh-webhook`.
    - Missing `gh_auth` or `pi_auth`: Opens the corresponding guided login flow directly in the modal.

---

## 9. VS Code-Style Extension / Plugin System (Item #17)

### Architecture & Manifest Contract (`src/lib/extensions/registry.svelte.ts`, `src-tauri/src/extensions.rs`)
- **Extension Manifest (`PipanExtensionManifest`)**:
  ```ts
  export interface PipanExtensionManifest {
    id: string;
    name: string;
    version: string;
    description: string;
    author: string;
    builtin?: boolean;
    enabledByDefault?: boolean;
    contributes: {
      panels?: Array<{ id: string; title: string; icon: string; defaultPosition?: 'split' | 'bottom' }>;
      sidebarTabs?: Array<{ id: string; title: string; icon: string }>;
      mainViews?: Array<{ id: string; title: string; icon: string }>;
      statusBarItems?: Array<{ id: string; alignment: 'left' | 'right'; priority?: number }>;
      slashCommands?: Array<{ name: string; description: string }>;
    };
    entry?: string; // Relative JS module path for external plugins
  }
  ```
- **Repackaged Built-in Extensions**:
  - Repackage Pipan's modular capabilities as 5 first-party extensions managed through the exact same registry and toggleable/installable in the **Extensions Marketplace / Manager** tab:
    1. `pipan.node-graph` — **Orchestration Node Graph (DAG)** (contributes the Nodes split panel & graph inspector)
    2. `pipan.github-prs` — **GitHub Pull Requests & Reviews** (contributes the PR sidebar tab, PR session panel, and webhook status bar item)
    3. `pipan.terminal` — **Integrated Workspace Terminal** (contributes the Terminal session panel)
    4. `pipan.files` — **Workspace File Explorer** (contributes the Files session panel and prompt file tree picker)
    5. `pipan.cost-analytics` — **Cost & Usage Analytics** (contributes the Cost main view and status bar spend widget)
- **External / Custom User Plugins**:
  - Stored in `<app_local_data_dir>/extensions/<ext-id>/` or `~/.pipan/extensions/<ext-id>/`.
  - Backend commands (`src-tauri/src/extensions.rs`):
    - `list_external_extensions()`: Scans local extension directories and returns manifests + entry script contents.
    - `install_extension_from_folder(source_path)`: Copies/links a local plugin directory into `~/.pipan/extensions/`.
    - `install_extension_from_git(repo_url)`: Clones/downloads an extension from a GitHub repo URL into `~/.pipan/extensions/`.
    - `uninstall_extension(extension_id)`: Removes an installed extension.
  - External plugins register custom panels, sidebar views, status bar items, or slash commands via a sandboxed `window.pipan.registerExtension(...)` host API.

---

## 10. Verification & Test Scenarios

1. **Type & Build Verification**:
   - `pnpm check` (`svelte-kit sync && svelte-check`) passes with zero TypeScript/Svelte errors.
   - `cargo check` and `cargo test` in `src-tauri/` compile cleanly and pass all unit tests (GitHub CLI args validation, session storage serialization, usage JSONL parser, doctor checks, extension manifest validation).
2. **Chat-First & Split Panel Verification**:
   - Launch app -> verify clean Chat window renders by default with no Node canvas forced open.
   - Open Node View panel -> verify it opens in a **vertical (side-by-side)** split by default and can be toggled to a **horizontal (top/bottom)** split.
   - Open Terminal, File, and PR panels from the session window and verify tab switching and splitter resizing.
3. **Node Graph Stability & Persistence**:
   - Stream a multi-step prompt with thinking and tool calls -> verify nodes appear compactly inside the current viewport without overlapping or jumping during text streaming.
   - Drag nodes to custom positions, switch to another session or the Cost page, and switch back -> verify exact node positions are restored.
4. **Session Lifecycle**:
   - Create multiple sessions, rename a session, assign groups, search/filter/sort in the sidebar, stop a running session mid-flight, archive a session, and delete a session.
5. **Model, Skills & Drag-and-Drop**:
   - Switch models and thinking levels from the selector; pick an installed skill via the Skills popover; drag and drop both an image and a source file into the prompt and verify they attach and send properly.
6. **GitHub PR Workflow**:
   - Verify default PR views (`Assigned to Me`, `Open PRs`), create a custom persistent `gh` search view, inspect PR checks/diffs, post an inline review comment, approve/request changes, and test merge options.
7. **Cost Page, Status Bar, Auth Config, Doctor & Extensions**:
   - Open Cost page and toggle Day / Week / Month / Year views; verify bottom status bar metrics; run Doctor and test auto-fix actions; open Extensions manager and toggle/install/disable built-in and custom extensions.

---

## Explicit Assumptions & Defaults Chosen
- **Split Default**: Opening the Node View (or other session panels) defaults to a **vertical side-by-side split** (Chat left, Panel right), with a one-click toggle for horizontal top/bottom split.
- **Session Storage**: Sessions use real Pi `.jsonl` session files paired with `<app_local_data_dir>/pipan-metadata.json` so transcripts, costs, and node coordinates survive restarts.
- **Built-in Extensions Enabled by Default**: All 5 repackaged built-in extensions (`pipan.node-graph`, `pipan.github-prs`, `pipan.terminal`, `pipan.files`, `pipan.cost-analytics`) ship enabled by default so all features work out of the box, while users can disable/uninstall them or install custom plugins from the Extensions manager.
