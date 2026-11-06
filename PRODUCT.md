# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Software engineers and AI-assisted developers (inferred from `PLAN.md` and codebase) who run and supervise `pi` coding agent sessions against local repositories and GitHub pull requests, needing real-time telemetry, live steering, and strict workspace isolation without mutating their working tree directly.

## Product Purpose

`pipan` is a desktop GUI wrapper and orchestration workspace for the `pi` AI coding agent built with Tauri and Svelte 5. It turns the `pi` CLI into an inspectable, steerable desktop environment—currently providing live streaming chat, reasoning inspection, tool-call execution cards, mid-flight steering/follow-up queues, and stderr diagnostics, with planned expansion (`PLAN.md`) into a living DAG orchestration canvas and GitHub PR/webhook integration.

Success means developers can launch isolated or directory-bound `pi` sessions, see every thought, tool invocation, and subagent branch in real time, and intervene (steer, queue follow-ups, abort, or approve gated actions) without losing context or risking unapproved host-repo mutations.

## Positioning

Unlike terminal-only CLI harnesses or opaque chat wrappers, `pipan` pairs direct `pi --mode rpc` sidecar process control with isolated workspace execution (headless temp sandboxes today; per-session Git worktrees planned) and direct `gh` CLI / webhook event streams—avoiding MCP server overhead and REST polling rate limits while exposing structured agent telemetry and mid-turn steering controls.

## Operating Context

- Runs as a native desktop window via Tauri v2 (`800x600` default window, local webview at `http://localhost:1420` in dev) alongside the developer's IDE, terminal, and Git repositories.
- Operates in two session modes today:
  - **Headless (Isolated Temp):** Spawns `pi` inside `<temp_dir>/pipan-headless` for safe, scratchpad exploration.
  - **Directory Workspace:** Spawns `pi` bound to a user-selected local project folder via native OS file dialog.
- Expects `pi` CLI installed on the host `PATH` (invoked with `--mode rpc --no-session`) and, for planned GitHub workflows, `gh` CLI and `git` on the host machine.

## Capabilities and Constraints

### Confirmed Implemented Capabilities
- **Rust Sidecar & RPC Manager (`src-tauri/src/pi_rpc.rs`):** Spawns and manages a single active `pi --mode rpc --no-session` child process with generation-safe lifecycle cleanup, newline-delimited JSONL stdout parsing (`pi:event`), live stderr streaming (`pi:stderr`), and connection state broadcasts (`pi:status`).
- **Reactive Telemetry Store (`src/lib/stores/chat.svelte.ts`):** Reconstructs streaming text deltas, collapsible reasoning/thinking blocks, live tool call argument/execution progress and durations, token usage, compaction status, and active `steer` / `followUp` message queues.
- **Session & Steering UI (`src/lib/components/`):**
  - `SessionHeader.svelte`: Connection status badge (`Disconnected`, `Starting…`, `Ready`, `Thinking / Streaming`, `Error`), workspace indicator, active model + thinking-level pill, and session lifecycle actions (`Start Headless`, `Open Folder…`, `New Chat`, `Disconnect`).
  - `MessageList.svelte` & `MessageItem.svelte`: Auto-scrolling transcript with scroll-lock override ("Jump to bottom"), disconnected onboarding state, connected quick-starter prompts, `ThinkingBlock` accordions, and `ToolCallCard` inspectors.
  - `ChatInput.svelte`: Context-aware composer that switches from `Send` when idle to live `Steer`, `Follow-up`, and `Stop` (`clear_queue` + `abort`) controls while the agent is running, with visual queue pills above the input.
  - Collapsible `Pi Diagnostics` stderr drawer and dismissible error banner in `src/routes/+page.svelte`.

### Architectural Constraints (`PLAN.md` & Codebase)
- **Strict JSON RPC Communication:** Communicate with `pi` exclusively as a sidecar process over JSON RPC (`stdin`/`stdout`), never by scraping a pseudo-terminal.
- **No MCP Servers for GitHub:** All planned GitHub integration must execute directly via `gh` CLI and local webhook forwarding (`gh webhook forward`), never via MCP servers or REST polling loops.
- **Workspace Isolation:** Planned multi-session architecture requires every agent session to run inside an isolated `git worktree` so the host repository is never mutated directly without review.
- **Tech Stack:** Tauri v2 (Rust, `tokio`, `serde_json`), Svelte 5 (Runes, SvelteKit static adapter), Tailwind CSS v4, TypeScript, and `lucide-svelte` (with `@xyflow/svelte` and `dagre` planned in `PLAN.md` for Phase 4).

### Open / Undecided Product Decisions
- **Primary Surface Hierarchy (Inferred / Pending Confirmation):** How the existing full-screen chat/steering transcript (`src/routes/+page.svelte`) will coexist with the planned Phase 4 Svelte Flow DAG canvas (`PLAN.md` specifies a sidebar for history/PRs, main canvas for the Svelte Flow dashboard, and bottom drawer for chat/steering input).
- **Destructive Action Interception Mechanism:** Exact RPC protocol hook or extension mechanism for pausing `pi` on destructive operations (`ApprovalNode` in Phase 5 of `PLAN.md`).

## Brand Commitments

- **Product Name & Identifier:** `pipan` (`com.steven-harris.pipan`).

## Evidence on Hand

- `PLAN.md`: Five-phase architectural roadmap covering Tauri scaffolding, Pi RPC & Git worktrees, `gh` CLI & webhook tunnel, Svelte Flow living dashboard (`OrchestratorNode`, `WorkerNode`, `ActionNode`, `ApprovalNode`), and interactive steering/review.
- `src-tauri/src/pi_rpc.rs` & `src-tauri/src/lib.rs`: Working Rust sidecar process manager and Tauri command handlers (`start_pi_session`, `stop_pi_session`, `send_pi_command`, `get_pi_status`).
- `src/lib/stores/chat.svelte.ts` & `src/lib/types/pi.ts`: Complete client-side RPC protocol types and reactive state machine.
- `src/routes/+page.svelte` & `src/lib/components/*.svelte`: Working dark-mode (`zinc-950` / `indigo` / `emerald` / `amber` / `rose`) chat and telemetry inspector UI.
- **Absences:** No external marketing claims, customer testimonials, benchmark numbers, or custom vector logos beyond default Tauri app icons in `src-tauri/icons/` exist; future work must not fabricate them.

## Product Principles

1. **Transparent Agent Telemetry:** Every reasoning trace, tool call, argument payload, execution duration, and stderr diagnostic must be inspectable in real time without cluttering the primary flow when collapsed.
2. **Continuous Human Steering:** The user is never locked out while the agent works; mid-turn `Steer`, queued `Follow-up`, and immediate `Stop` (`abort`) controls remain first-class affordances during streaming.
3. **Isolation by Default:** Agent execution should default to safe sandboxes (headless temp directories or isolated Git worktrees) so exploration and multi-agent tasks cannot corrupt the user's primary checkout.
4. **Direct Local Primitives over Heavy Middleware:** Rely on direct CLI sidecars (`pi --mode rpc`, `git worktree`, `gh` CLI, local webhook tunnels) with zero polling and no unnecessary intermediary servers.
