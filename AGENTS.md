# AGENTS.md

This document explains the architecture, subsystems, directory layout, commands, and operational rules for PiPan.

---

## 1. System Overview & Tech Stack

PiPan is an inspectable desktop orchestration workspace for the Pi coding agent. It wraps the Pi CLI in a native desktop interface. Developers can supervise agent reasoning, inspect tool calls, steer execution mid-flight, and manage Git repositories safely.

### Tech Stack

- **Desktop Shell**: Tauri v2 (`src-tauri`). Tauri provides native OS windowing, file dialogs, system menus, and safe IPC bindings between the frontend webview and the native host.
- **Frontend Framework**: Svelte 5 with modern Runes (`$state`, `$derived`, `$props`). The frontend runs in a native webview managed by Tauri and SvelteKit static adapter.
- **Styling & UI**: Tailwind CSS v4 and Lucide Svelte icons.
- **Node Graph Visualization**: `@xyflow/svelte` and Dagre. Used to render live multi-agent execution graphs and directed acyclic graphs (DAG).
- **Backend Runtime**: Rust (2024 edition) with Tokio. Tokio handles asynchronous child processes, PTY terminals, file watching, and local HTTP endpoints.
- **Persistent Storage**: SQLite 3 via `rusqlite` (bundled feature). The database file lives at `<app_local_data_dir>/pipan.db`.
- **Agent Sidecar**: Pi CLI (`pi --mode rpc`), spawned as a native background process.

---

## 2. Core Architecture & Subsystems

```
+-------------------------------------------------------------------------+
|                           Svelte 5 Webview                              |
|   (Runes: $state, $derived, $props | Tailwind CSS v4 | @xyflow/svelte)  |
|                                                                         |
|  [ ChatPane ]  [ FlowCanvas ]  [ Terminal ]  [ Files ]  [ PRs ] [ Cost ]|
+-------------------------------------------------------------------------+
        |                                                 ^
        | Tauri invoke(command, args)                     | Tauri emit(event)
        v                                                 |
+-------------------------------------------------------------------------+
|                            Tauri v2 Core                                |
|   (src-tauri/src/lib.rs | Tokio Runtime | Mutex Managed States)         |
|                                                                         |
|  +--------------------+  +--------------------+  +-------------------+  |
|  |    PiRpcManager    |  |  Database (SQLite) |  |   AgentManager    |  |
|  |  (src/pi_rpc.rs)   |  |   (src/db.rs)      |  |   (src/agent.rs)  |  |
|  +--------------------+  +--------------------+  +-------------------+  |
|  |   TerminalManager  |  |   Doctor Engine    |  |  Extension Host   |  |
|  |  (src/terminal.rs) |  |  (src/doctor.rs)   |  | (src/extensions.rs|  |
|  +--------------------+  +--------------------+  +-------------------+  |
+-------------------------------------------------------------------------+
       | stdin (JSONL)        | SQL (WAL)                 | Direct CLI
       v stdout (JSONL)       v                           v
+------------------+   +-------------------+    +----------------------+
|  pi --mode rpc   |   |     pipan.db      |    | gh CLI / git CLI     |
| (Child Process)  |   | (Local SQLite DB) |    | (Host Executables)   |
+------------------+   +-------------------+    +----------------------+
```

### a. Pi JSONL RPC Sidecar

PiPan controls the Pi agent as a background child process via `src-tauri/src/pi_rpc.rs`.

- **Transport**: Newline-delimited JSON RPC over standard input (`stdin`) and standard output (`stdout`).
- **Input Commands**: PiPan writes one JSON object per line ending with `\n` to Pi's `stdin`. Commands include:
  - `prompt`: Submits a user message, optional images, and streaming behavior (`steer` or `followUp`).
  - `steer`: Injects live guidance while the agent is currently thinking or running tools.
  - `follow_up`: Queues a prompt that executes right after the current turn finishes.
  - `abort`: Cancels the active LLM response or tool execution immediately.
  - `clear_queue`: Empties pending steer and follow-up queues.
  - `switch_session`: Changes the active transcript file to another session.
  - `set_session_name`: Updates the title of the active session.
  - `get_state`, `get_messages`, `get_available_models`, `get_session_stats`: Inspects session data and host configuration.
- **Output Event Streams**:
  - `pi:event`: Pi emits JSON lines on `stdout`. PiPan splits output strictly on `\n` characters (never Unicode line terminators), validates JSON, and emits the payload to the frontend. Sub-events include `thinking_delta`, `text_delta`, `toolcall_start`, `toolcall_delta`, `toolcall_end`, and `done`.
  - `pi:stderr`: PiPan drains Pi's `stderr` in a background Tokio loop. It forwards lines to the frontend with millisecond timestamps. Draining `stderr` prevents pipe buffer deadlocks.
  - `pi:status`: Broadcasts process status changes (`disconnected`, `starting`, `running`, `error`), the operating mode, working directory, and operating system process ID (PID).
- **Session Lifecycle**:
  - **Start**: `start_pi_session` spawns `pi --mode rpc`. If persistent, it passes `--session <path>` pointing to a `.jsonl` transcript file and `--name <label>`. If ephemeral, it passes `--no-session`.
  - **Stop Process**: `stop_pi_session` closes child `stdin` so Pi receives an EOF signal. It waits up to 2 seconds for clean exit. If the process does not exit, it terminates the process tree.
  - **Stop Generation**: The UI stop button sends `clear_queue` followed by `abort`. This stops generation while keeping the Pi process alive.

### b. Persistent Storage & SQLite Schema

PiPan stores application state and metadata in a SQLite database located at `<app_local_data_dir>/pipan.db`.

- **Database Connection**: `src-tauri/src/db.rs` opens SQLite in Write-Ahead Logging (`WAL`) mode with foreign key constraints enabled. It lives behind a thread-safe `Mutex<Connection>` in Tauri managed state.
- **Database Migrations**: PiPan tracks database versioning using `PRAGMA user_version`. Each migration runs once inside a transaction at startup:
  - **v1 (Graph Persistence)**:
    - `sessions`: Stores `id` (primary key), `graph_direction` (`'TB'` or `'LR'`), `created_at`, `updated_at`.
    - `node_positions`: Stores `session_id` (foreign key with cascade delete), `direction`, `node_id`, `x`, `y`, `pinned`.
  - **v2 (Persistent Sessions & Search Indexes)**:
    - Extends `sessions` with: `name`, `title`, `status`, `mode`, `cwd`, `session_file_path`, `pi_session_file`, `group_name`, `branch`, `worktree_path`, `model`, `model_id`, `provider`, `pinned`, `archived`, `message_count`, `tool_call_count`, `total_tokens`, `total_cost_usd`.
    - `session_tags`: Stores `session_id` (foreign key with cascade delete) and `tag`.
    - Indexes: Added on `updated_at`, `archived`, `cwd`, and `session_tags(tag)`.
  - **v3 (GitHub Saved Views)**:
    - `github_views`: Stores `id`, `name`, `query`, and `sort_order`.
  - **v4 (Usage & Cost Ledger)**:
    - `usage_ledger`: Stores `session_id`, `message_id`, `timestamp`, `provider`, `model`, `input`, `output`, `cache_read`, `cache_write`, `cost_usd`, `cwd`.
    - `usage_file_checkpoints`: Tracks file scan positions (`path`, `mtime`, `byte_offset`) for incremental parsing of Pi transcript files.
  - **v5 (Extension Channel Layouts)**:
    - `channel_node_positions`: Stores manual node positions of extension channel graphs.
  - **v6 (Channel Layout Keys)**:
    - `channel_node_positions` is rebuilt with `session_id` (foreign key with cascade delete), `ch`, `topic`, `graph_id`, `node_id`, `x`, `y`, `pinned`. Rows saved under v5 had no channel or topic and are dropped.
- **Session Data vs. Transcript Files**:
  - `pipan.db` stores session metadata, graph layouts, PR views, and usage metrics.
  - Pi CLI stores raw message conversations, run states, and tool execution logs directly in its `.jsonl` transcript file (`session_file_path`).
  - PiPan indexes message counts, tool call counts, and token usage into SQLite so search, filtering, grouping, and sorting happen directly in SQL without parsing transcripts on every query.
- **Repository Layer (`src-tauri/src/storage.rs`)**:
  - Provides CRUD operations: `list_saved_sessions`, `save_session_metadata`, `rename_session`, `archive_session`, `delete_session`.
  - Runs database queries on Tokio's blocking threadpool (`spawn_blocking`) to keep the async executor responsive.
  - One-time migration: Automatically imports legacy `pipan-metadata.json` files into SQLite on initial run.

### c. Tauri IPC Commands & Frontend Bindings

The Svelte 5 frontend communicates with the Rust core via typed IPC wrappers.

#### Key Tauri Invoke Commands

| Category | Command | Description |
|---|---|---|
| **Pi RPC** | `start_pi_session` | Starts Pi child process in directory or headless mode |
| | `stop_pi_session` | Closes stdin and terminates the active Pi process |
| | `send_pi_command` | Writes a JSON command line to Pi stdin |
| | `get_pi_status` | Returns active connection state, mode, cwd, and PID |
| **Session Store** | `list_saved_sessions` | Queries sessions with SQL search, filter, and sort |
| | `save_session_metadata` | Upserts session fields, tags, and metrics |
| | `rename_session` | Renames session in SQLite and notifies Pi RPC |
| | `archive_session` | Toggles archived state for a session |
| | `delete_session` | Deletes DB records, deletes `.jsonl` file, and stops child process |
| **Multi-Agent** | `spawn_agent_session` | Launches an agent sub-process in a Git worktree |
| | `send_agent_command` | Sends commands to subagents |
| | `stop_agent_session` | Terminates subagent and optionally prunes worktree |
| | `resolve_approval` | Resolves an approval gate (`approve`, `reject`, `modify`) |
| | `list_pending_approvals` | Lists actions waiting for user confirmation |
| **Terminal** | `start_terminal_session` | Starts a native PTY shell session |
| | `send_terminal_input` | Sends keystrokes/data to the shell stdin |
| | `resize_terminal` | Updates terminal columns and rows |
| | `stop_terminal_session` | Terminates shell process |
| **Files & Git** | `list_workspace_files` | Reads directory tree respecting `.gitignore` |
| | `get_git_file_statuses` | Returns modified, untracked, and staged file status |
| | `read_workspace_file` | Reads file content with size limits and image detection |
| | `read_file_attachment` | Reads file or image attachment for prompt injection |
| **GitHub** | `gh_status` | Checks GitHub CLI authentication state |
| | `gh_auth_login_web` | Spawns browser login flow with one-time code |
| | `gh_auth_login_token` | Authenticates with Personal Access Token |
| | `gh_auth_logout` | Logs out of GitHub account |
| | `gh_pr_list` | Lists open pull requests |
| | `gh_pr_detail` | Gets pull request details and CI status |
| | `gh_pr_diff` | Retrieves pull request diff parsed per file |
| | `gh_pr_submit_review` | Submits approval, comment, or change request |
| | `gh_pr_merge` | Merges pull request (merge, squash, rebase) |
| | `start_webhook_forwarder` | Starts Axum listener and `gh webhook forward` |
| | `stop_webhook_forwarder` | Stops webhook listener |
| **Doctor** | `run_doctor` | Executes all 7 dependency diagnostics |
| | `fix_doctor_issue` | Runs automated remediation for a check |
| **Extensions** | `list_external_extensions` | Scans manifest files from extension directories |
| | `install_extension_from_folder` | Copies local plugin to extensions directory |
| | `install_extension_from_git` | Clones Git repository into extensions directory |
| | `uninstall_extension` | Deletes external extension folder |
| **Channels** | `load_channel_node_positions` | Loads saved node positions of a channel graph |
| | `save_channel_node_positions` | Saves dragged node positions of a channel graph |
| | `clear_channel_node_positions` | Resets a channel graph layout |
| **Usage** | `get_usage_analytics` | Aggregates token counts and costs |
| | `rescan_usage_files` | Scans transcript files for new token usage |
| | `record_usage_entry` | Records single usage entry into SQLite |

#### Key Listen Channels (Rust to Frontend)

- `pi:status`: Broadcasts connection state changes and process metadata.
- `pi:event`: Delivers raw JSON RPC events from Pi stdout. Extension channel traffic (shim announce, state widgets, request dialogs, log entries) arrives here and is handled first by `src/lib/stores/channels.svelte.ts`.
- `pi:stderr`: Streams stderr diagnostic logs.
- `agent:status`: Broadcasts multi-agent session lifecycle changes.
- `agent:approval_request`: Emits gated action requests needing user approval.
- `agent:telemetry`: Sends step and token progress from subagents.
- `terminal:stdout`: Delivers raw output bytes from the shell PTY.
- `terminal:stderr`: Delivers shell error stream output.
- `terminal:exit`: Notifies frontend of shell exit code.
- `github:webhook`: Delivers incoming GitHub webhook event payloads.
- `github:forwarder`: Updates status of the webhook forwarding process.
- `doctor:fix_log`: Streams live log lines from dependency auto-fixers.

### d. Extension & Plugin System

PiPan uses a VS Code-style extension model defined in `src-tauri/src/extensions.rs` and `src/lib/extensions/registry.svelte.ts`.

- **Manifest Schema (`pipan-extension.json`)**:
  ```json
  {
    "id": "vendor.my-extension",
    "name": "My Custom Extension",
    "version": "1.0.0",
    "description": "Adds custom analysis panels and tools.",
    "author": "Developer Name",
    "entry": "dist/index.js",
    "contributes": {
      "panels": [
        {
          "id": "my-panel",
          "title": "Code Inspector",
          "icon": "Code2",
          "defaultPosition": "split"
        }
      ],
      "sidebarTabs": [
        {
          "id": "my-tab",
          "title": "Audits",
          "icon": "ShieldCheck"
        }
      ],
      "mainViews": [
        {
          "id": "my-view",
          "title": "Architecture Map",
          "icon": "Network"
        }
      ],
      "statusBarItems": [
        {
          "id": "my-status",
          "alignment": "right",
          "priority": 10
        }
      ],
      "slashCommands": [
        {
          "name": "analyze",
          "description": "Run custom static analysis"
        }
      ],
      "channels": [
        {
          "topic": "graph",
          "primitive": "graph",
          "title": "Run graph"
        }
      ]
    }
  }
  ```
- **Contribution Points**:
  - `panels`: Split or bottom workspace panels (such as nodes graph, terminal, files).
  - `sidebarTabs`: Primary navigation tabs in the left sidebar.
  - `mainViews`: Dedicated full-window screens (such as Cost Analytics or Extension Manager).
  - `statusBarItems`: Indicators in the bottom status bar (left or right aligned).
  - `slashCommands`: Quick actions available in the chat prompt input.
  - `channels`: Graph topics of the plugin's own channel that PiPan renders with a fixed title and placement.
- **Built-in First-Party Extensions**:
  PiPan repackages its primary subsystems as built-in extensions:
  1. `pipan.node-graph`: Living DAG canvas and node inspection panel.
  2. `pipan.github-prs`: Pull request review panel, sidebar tab, and webhook monitor.
  3. `pipan.terminal`: Integrated PTY shell panel.
  4. `pipan.files`: Workspace file explorer and prompt attachment selector.
  5. `pipan.cost-analytics`: Token and expense analytics view and status widget.
  6. `pipan.channels`: Graph panels published by Pi extensions through channels.
- **Registration & Sandboxing**:
  - External extensions reside in `<app_local_data_dir>/extensions/<id>/` or `~/.pipan/extensions/<id>/`.
  - Built-in IDs (`pipan.*`) are reserved and protected against overwriting.
  - Extension JavaScript bundles are limited to 1 MiB.
  - External scripts execute in a restricted webview environment and register through `window.pipan.registerExtension(...)`.
  - Extensions cannot execute arbitrary shell code directly; all host operations route through validated Tauri commands.

### e. Extension Channel

Pi extensions drive native PiPan UI through channels. The contract is in `docs/guide/channels.md`.

- **Shim**: `src-tauri/resources/pipan-channel-shim.ts` is written to the app data directory and passed to the primary Pi session with `--extension` (`src-tauri/src/channel.rs`, `pi_rpc.rs`). It is the only code that touches the wire encoding.
- **Producer API**: Pi extensions use `pi.events` (`pipan:hello`, `pipan:state`, `pipan:request`, `pipan:log`, and `pipan:intent` back to the producer). Every call is a no-op without a shim.
- **Carriers**: state is a widget `pipan:s:<ch>:<topic>`, a request is an `editor` dialog titled `pipan:r:<ch>:<kind>`, the log is custom entries and messages `pipan:l:<ch>:<name>`, an intent is a prompt `/pipan-intent <json>`, and the shim announces itself with the widget `pipan:shim`.
- **Frontend** (`src/lib/channels/`, `src/lib/stores/channels.svelte.ts`): codec, validators, the channel store, and the primitives: `graph` (panel, Dagre layout), `review` (diff and document, line comments) and `form`. `RequestHost.svelte` shows the oldest pending request of the viewed session in a modal. Pending requests are cancelled when the session switches, stops or exits.
- **Plugin API**: `window.pipan.channel(id)` gives plugins `subscribe`, `onRequest` and `intent`. `registerExtension` accepts `messageRenderers`, and manifests accept `contributes.channels`.
- **Tests**: `pnpm test:channel` drives `examples/channel-reference/producer.ts` through a real `pi --mode rpc` process. `pnpm test` covers the store, validators and review model.

### f. Dependency Doctor

The Dependency Doctor (`src-tauri/src/doctor.rs`, `src/lib/stores/doctor.svelte.ts`) diagnoses host environment prerequisites and repairs missing tools.

- **Diagnostic Checks**:
  1. `git`: Checks `git --version` on PATH and verifies `user.name` and `user.email` git config values.
  2. `node_npm`: Verifies Node.js installation (`node --version`) and package manager (`pnpm` or `npm`).
  3. `pi_cli`: Verifies `pi --version` on the augmented system PATH.
  4. `pi_auth`: Verifies whether valid API keys or OAuth credentials exist in `~/.pi/agent/auth.json` or active environment variables.
  5. `gh_cli`: Verifies GitHub CLI (`gh --version`) on PATH.
  6. `gh_auth`: Verifies active GitHub login state via `gh auth status`.
  7. `gh_webhook`: Verifies whether the `cli/gh-webhook` extension is installed.
- **Auto-Remediation (`fix_doctor_issue`)**:
  - Each check includes both step-by-step manual instructions and automated fixes.
  - `pi_cli`: Installs Pi globally using `npm install -g @earendil-works/pi-coding-agent` (or `pnpm add -g`) and updates the runtime PATH cache.
  - `gh_cli`: Runs platform package managers (`winget install --id GitHub.cli -e` on Windows, `brew install gh` on macOS, or Linux package managers) with real-time log streaming.
  - `git`: Invokes platform package managers (`winget install --id Git.Git -e` or `brew install git`).
  - `gh_webhook`: Executes `gh extension install cli/gh-webhook`.
  - `gh_auth`: Launches the guided browser login flow with one-time code display.
  - `pi_auth`: Opens the in-app Provider Settings view to save API keys securely.

---

## 3. Repository Directory Layout

```
pipan/
├── examples/                      # channel-reference (producer) and channel-plugin
├── docs/                          # VitePress documentation site
│   ├── .vitepress/                # VitePress site configuration and theme
│   ├── guide/                     # Documentation guide pages
│   └── index.md                   # Documentation landing page
├── src/                           # Svelte 5 frontend source code
│   ├── app.css                    # Tailwind CSS v4 directives and theme variables
│   ├── app.html                   # HTML document template
│   ├── routes/                    # SvelteKit routing
│   │   ├── +layout.svelte         # Root application shell and global listener bindings
│   │   ├── +layout.ts             # Static prerender config
│   │   └── +page.svelte           # Main workspace view router
│   ├── lib/
│   │   ├── components/            # Svelte 5 UI components
│   │   │   ├── ChatPane.svelte            # Main chat container
│   │   │   ├── ChatInput.svelte           # Composer with send, steer, follow-up, stop
│   │   │   ├── MessageList.svelte         # Virtual scroll transcript
│   │   │   ├── MessageItem.svelte         # Chat message card
│   │   │   ├── ThinkingBlock.svelte       # Collapsible reasoning inspector
│   │   │   ├── ToolCallCard.svelte        # Tool execution telemetry card
│   │   │   ├── SessionHeader.svelte       # Status indicator and session actions
│   │   │   ├── Sidebar.svelte             # Navigation, session list, search/filter
│   │   │   ├── StatusBar.svelte           # Bottom status bar with model, cost, webhook
│   │   │   ├── WorkspaceLayout.svelte     # Multi-panel grid and split-view host
│   │   │   ├── PanelHost.svelte           # Dynamic panel container
│   │   │   ├── CostAnalyticsPage.svelte   # Cost and token usage dashboard
│   │   │   ├── ExtensionsManagerPage.svelte # Plugin installer and manager
│   │   │   ├── DoctorModal.svelte         # Diagnostic and auto-fix dialog
│   │   │   ├── SettingsAuthModal.svelte   # Pi and GitHub settings dialog
│   │   │   ├── FlowCanvas.svelte          # Svelte Flow DAG canvas
│   │   │   ├── github/                    # GitHub PR list and diff viewers
│   │   │   ├── nodes/                     # Svelte Flow custom graph node cards
│   │   │   └── panels/                    # Split panels (Files, Terminal, PRs)
│   │   ├── stores/                # Reactive state machines (Svelte 5 runes)
│   │   │   ├── chat.svelte.ts             # Pi RPC session and message store
│   │   │   ├── doctor.svelte.ts           # Diagnostic checks and auto-fix store
│   │   │   ├── files.svelte.ts            # Workspace files and Git status store
│   │   │   ├── flow.svelte.ts             # Agent DAG and approval gate store
│   │   │   ├── github.svelte.ts           # PRs, diffs, reviews, and webhook store
│   │   │   ├── graph-layout.svelte.ts     # Dagre layout and coordinate persistence
│   │   │   ├── terminal.svelte.ts         # PTY terminal manager store
│   │   │   ├── usage.svelte.ts            # Cost ledger and token analytics store
│   │   │   └── workspace.svelte.ts        # Split panels, layout, and active views
│   │   ├── extensions/            # Frontend extension registry and loaders
│   │   │   ├── builtin.ts                 # Manifests for built-in extensions
│   │   │   ├── registry.svelte.ts         # Extension registration and lifecycle store
│   │   │   └── types.ts                   # Manifest and contribution type contracts
│   │   ├── channels/              # Extension channel codec, validators, review model, plugin API
│   │   ├── flow/                  # DAG layout algorithms and telemetry helpers
│   │   ├── types/                 # Shared TypeScript interfaces
│   │   │   ├── agent.ts                   # Multi-agent and approval payload types
│   │   │   ├── auth.ts                    # Settings and credential types
│   │   │   ├── doctor.ts                  # Check result and fix result types
│   │   │   ├── github.ts                  # PR, check, diff, and webhook types
│   │   │   ├── pi.ts                      # Pi RPC message and event protocol types
│   │   │   └── usage.ts                   # Cost and token metrics types
│   │   └── workspace/             # Panel configuration and registry
├── src-tauri/                     # Tauri v2 Rust backend
│   ├── Cargo.toml                 # Rust dependencies and release settings
│   ├── tauri.conf.json            # Window configurations, bundle ID, permissions
│   ├── capabilities/              # Tauri v2 security capabilities (`default.json`)
│   ├── icons/                     # Native desktop application icons
│   ├── resources/                 # Embedded scripts and helper binaries
│   └── src/
│       ├── main.rs                # Desktop binary entry point
│       ├── lib.rs                 # Tauri app builder, command handlers, exit hooks
│       ├── pi_rpc.rs              # Pi CLI JSONL sidecar process supervisor
│       ├── storage.rs             # SQLite session repository and query pushdown
│       ├── db.rs                  # SQLite migrations and connection management
│       ├── doctor.rs              # System dependency checks and package manager fixes
│       ├── extensions.rs          # Extension manifest parsing and disk manager
│       ├── channel.rs             # Writes the channel shim and builds its Pi arguments
│       ├── github.rs              # Host gh CLI wrapper (PRs, diffs, auth)
│       ├── webhook.rs             # Local Axum webhook receiver and forwarder
│       ├── terminal.rs            # Native PTY shell session manager
│       ├── files.rs               # Prompt file attachment reader
│       ├── fs_explorer.rs         # Directory tree and Git status inspector
│       ├── models.rs              # Pi model discovery (~/.pi/agent/models.json)
│       ├── skills.rs              # Pi skills discovery
│       ├── auth.rs                # Credential management (~/.pi/agent/auth.json)
│       ├── usage.rs               # Cost ledger and transcript checkpointing
│       ├── agent.rs               # Multi-agent sub-process orchestration
│       ├── approval.rs            # Gated action approvals
│       └── worktree.rs            # Git worktree creation and teardown
├── static/                        # Bundled static web assets
├── docs/                          # Guides, specifications, and architecture docs
├── package.json                   # Frontend scripts and Node dependencies
├── pnpm-lock.yaml                 # Pnpm lockfile
└── tsconfig.json                  # TypeScript compiler options
```

---

## 4. Development & Verification Commands

Run all frontend and backend verification commands before pushing changes.

### Frontend Development

```bash
# Start Vite frontend preview server
pnpm dev

# Launch full Tauri v2 desktop window with hot reloading
pnpm tauri dev

# Type-check TypeScript and Svelte components
pnpm check

# Build static production bundle
pnpm build
```

### Rust Backend Development

```bash
# Run all Rust backend unit and integration tests
cargo test --manifest-path src-tauri/Cargo.toml

# Run Clippy linter with strict warning checks
cargo clippy --manifest-path src-tauri/Cargo.toml

# Check Rust code formatting without altering files
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check

# Apply Rust code formatting
cargo fmt --manifest-path src-tauri/Cargo.toml
```

---

## 5. Agent Rules of Engagement

All agents and contributors working in this repository must follow these operational rules:

### Rule 1: Strict JSON RPC Communication with Pi
- Communicate with the Pi agent exclusively through its JSON RPC protocol over `stdin` and `stdout`.
- Never scrape pseudo-terminal output or parse ANSI terminal escape sequences to deduce agent state or tool call output.
- All messages sent to Pi must be valid single-line JSON objects ending with `\n`.
- All incoming lines from Pi must be parsed independently as JSON lines.
- Always drain `stderr` concurrently to avoid buffer saturation.

### Rule 2: Workspace Isolation
- Never mutate the user's primary checked-out Git branch directly during agent exploration.
- **Headless Sessions**: Must execute inside isolated temporary directories (`<temp_dir>/pipan-headless`).
- **Repository Tasks**: Multi-turn or multi-agent tasks on real repositories must run in isolated Git worktrees (`.git/worktrees/...`).
- Worktrees must be cleanly pruned when the session is deleted or completed.

### Rule 3: No MCP Servers for GitHub
- Do not add Model Context Protocol (MCP) servers or background polling loops to interact with GitHub.
- Execute all GitHub interactions directly through the host `gh` CLI (`gh pr list`, `gh pr diff`, `gh pr merge`).
- Deliver real-time GitHub notifications exclusively by forwarding webhooks via `gh webhook forward` into the local Rust Axum HTTP endpoint.

### Rule 4: Verify Before Finishing
- Run `pnpm check` to ensure all Svelte and TypeScript files pass type checking.
- Run `cargo test` to verify all backend tests pass.
- Run `cargo clippy` and `cargo fmt` to confirm Rust code meets formatting and linting standards.
- Never commit broken builds, failing tests, or unformatted files.
