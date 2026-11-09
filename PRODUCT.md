# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop app (Tauri v2 + Svelte 5 + Rust)

## Users

Software engineers and AI-assisted developers who run and supervise `pi` coding agent sessions against local repositories and GitHub pull requests. They need real-time telemetry, live steering, and workspace isolation without mutating their primary working tree directly.

## Product Purpose

PiPan is a native desktop application running via Tauri v2 to run, inspect, and steer the `pi` AI coding agent. PiPan turns the `pi` CLI into an interactive desktop workspace. It provides live streaming chat, model reasoning inspection, tool-call cards, a living DAG node graph, GitHub pull request workflows, an embedded terminal, a native file explorer, cost analytics, and an in-app dependency Doctor.

Success means developers can launch isolated or directory-bound `pi` sessions, watch every thought, tool invocation, and subagent step in real time, and intervene (steer, queue follow-ups, abort, or approve gated actions) without losing context or risking unapproved host repository mutations.

## Positioning

PiPan is a native desktop application running via Tauri v2. Unlike terminal-only CLI harnesses or opaque chat wrappers, PiPan pairs direct `pi --mode rpc` sidecar process control with isolated workspace execution (headless temp directories or Git worktrees) and direct `gh` CLI and local webhook event streams. This approach eliminates MCP server overhead and REST polling limits while exposing structured telemetry and human steering controls.

## Operating Context

- Runs as a native desktop application via Tauri v2 alongside the developer's code editor, terminal, and Git repositories.
- Operates in two session modes:
  - **Headless Sandbox:** Spawns `pi` inside an isolated temporary directory for scratchpad tasks.
  - **Directory Workspace:** Spawns `pi` inside a user-selected project folder or Git worktree.
- Requires the `pi` CLI on the host `PATH` (invoked with `--mode rpc --no-session`).
- Uses `git` and the `gh` CLI on the host machine for repository tracking, pull request workflows, and live webhook events.

## Capabilities and Constraints

### Confirmed Implemented Capabilities
- **Rust Sidecar & RPC Manager (`src-tauri/src/pi_rpc.rs`):** Spawns and supervises active `pi --mode rpc --no-session` child processes. Parses JSONL events from `stdout`, streams live `stderr` diagnostics, and broadcasts connection states.
- **Split-Panel Workspace Layout (`src/lib/components/WorkspaceLayout.svelte`):** Provides a flexible desktop layout with dedicated panels for Chat, DAG canvas, File Explorer, Terminal, and GitHub Pull Requests.
- **Living DAG Node Graph (`src/lib/components/FlowCanvas.svelte`):** Visualizes agent sessions, turns, tool calls, and approval steps as an interactive node graph using `@xyflow/svelte`. Persists node layout and coordinates across sessions.
- **Persistent SQLite Session Management (`src-tauri/src/db.rs`):** Stores sessions, messages, turns, tool calls, and graph node coordinates in a local SQLite database (`pipan.db`). Supports full-text search, grouping, filtering, archiving, and deletion.
- **Deep GitHub Integration (`src-tauri/src/github.rs`, `src-tauri/src/webhook.rs`):** Executes `gh` CLI commands directly for pull requests, checks, issues, and workflow runs. Forwards repository webhooks locally without MCP servers or polling loops.
- **Embedded Session Terminal and Native File Tree Explorer (`src-tauri/src/terminal.rs`, `src-tauri/src/fs_explorer.rs`):** Offers an in-app pseudo-terminal for direct shell execution and a native directory tree explorer with quick file viewing.
- **Historical Cost & Usage Analytics Dashboard (`src-tauri/src/usage.rs`):** Indexes transcript JSONL files into SQLite to compute token counts and estimated costs across days, weeks, months, and years. Surfaces active telemetry in the bottom status bar.
- **In-App Dependency Doctor (`src-tauri/src/doctor.rs`):** Audits required host tools (`pi`, `gh`, `git`, Node.js, and toolchains). Highlights missing tools or invalid authentication and provides auto-remediation scripts.
- **VS Code-Style Extension and Plugin Host System (`src-tauri/src/extensions.rs`):** Loads workspace extensions from local manifests. Supports custom panels, views, status bar items, and slash commands.
- **Reactive Telemetry & Steering UI (`src/lib/stores/chat.svelte.ts`, `src/lib/components/ChatInput.svelte`):** Displays streaming tokens, collapsible reasoning blocks, and expandable tool execution cards. Provides live `Steer`, queued `Follow-up`, and immediate `Stop` (`abort`) controls.
- **Action Approval System (`src-tauri/src/approval.rs`):** Intercepts sensitive actions and surfaces approval requests for user review and resolution before execution.

### Architectural Constraints
- **Strict JSON-RPC Communication:** PiPan communicates with `pi` exclusively as a sidecar process over JSON-RPC (`stdin`/`stdout`), never by scraping terminal output.
- **Direct CLI Primitives over Middleware:** All GitHub operations run directly through `gh` CLI commands and local webhook forwarding (`gh webhook forward`). PiPan avoids MCP servers and REST polling loops.
- **Workspace Isolation:** Multi-session workflows isolate runs inside Git worktrees or temporary directories to protect the host checkout.
- **Tech Stack:** Tauri v2 (Rust, `tokio`, `rusqlite`, `serde_json`), Svelte 5 (Runes, SvelteKit static adapter), Tailwind CSS v4, TypeScript, `@xyflow/svelte`, and `lucide-svelte`.

### Open / Undecided Product Decisions
- Core v0.2 architectural decisions are settled:
  - Surface hierarchy: Settled via the split-panel workspace layout hosting Chat, DAG Canvas, File Explorer, Terminal, and GitHub PR views.
  - Action gating: Settled via native approval records and interactive approval dialogs.
  - Persistence: Settled via SQLite (`pipan.db`) in WAL mode.
  - Extension contracts: Settled via declarative manifests and contribution points.

## Brand Commitments

- **Product Name & Identifier:** `PiPan` / `pipan` (`com.steven-harris.pipan`).

## Evidence on Hand

- `src-tauri/`: Rust backend modules for sidecar RPC, SQLite storage, terminal PTY, file explorer, GitHub operations, webhook listener, cost analytics, extension system, dependency doctor, and approval flow.
- `src/lib/`: Svelte 5 components, reactive runes stores, DAG canvas (`@xyflow/svelte`), split panels, and modal dialogs.
- `plan/`: Architectural roadmap files covering workspace layout, node graph, persistence, GitHub integration, terminal, file explorer, cost tracking, doctor, and extensions.
- `src-tauri/Cargo.toml` & `package.json`: Configured dependencies and verified build scripts for Tauri v2, Svelte 5, SQLite, and Tailwind CSS v4.

## Product Principles

1. **Transparent Agent Telemetry:** Every reasoning trace, tool call, argument payload, execution duration, and stderr diagnostic must be inspectable in real time without cluttering the primary flow when collapsed.
2. **Continuous Human Steering:** The user is never locked out while the agent works; mid-turn `Steer`, queued `Follow-up`, and immediate `Stop` (`abort`) controls remain first-class affordances during streaming.
3. **Isolation by Default:** Agent execution should default to safe sandboxes (headless temp directories or isolated Git worktrees) so exploration and multi-agent tasks cannot corrupt the user's primary checkout.
4. **Direct Local Primitives over Heavy Middleware:** Rely on direct CLI sidecars (`pi --mode rpc`, `git worktree`, `gh` CLI, local webhook tunnels) with zero polling and no unnecessary intermediary servers.
