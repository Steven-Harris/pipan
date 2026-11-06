# Project: Pi Harness Desktop GUI (GitHub Copilot Clone)

## 1. Project Overview & Architecture
Build a desktop GUI wrapper for the `pi` AI coding agent using Tauri. The application will clone the deep integration capabilities of GitHub Copilot Desktop while providing a "living dashboard" architecture diagram for agent orchestration.

**Core Constraints & Directives:**
*   **No MCP Servers:** All GitHub integration must happen via direct `gh` CLI execution and Webhooks.
*   **No Polling:** Do not use GitHub REST endpoints for status updates to avoid rate limits. Use `gh webhook forward`.
*   **Isolation:** The agent must never mutate the host repository directly. Every Pi session must run inside an isolated Git worktree.
*   **Communication:** Pi must be executed as a sidecar process using its JSON RPC mode, not as a text terminal.

## 2. Tech Stack
*   **Backend / System Shell:** Tauri (Rust)
*   **Frontend UI:** Svelte 5 (TypeScript) + Vite, Tailwind CSS
*   **Graph Visualization:** `@xyflow/svelte` (Svelte Flow) + `dagre` (Auto-layout)
*   **AI Core:** `pi` CLI (invoked as a spawned child process by Rust)
*   **GitHub Engine:** `gh` CLI

## 3. Execution Phases

### Phase 1: Foundation & Tauri Scaffolding
- [ ] Initialize a new Tauri project using the Svelte/TypeScript/Vite template.
- [ ] Install frontend dependencies: `tailwindcss`, `@xyflow/svelte`, `dagre`, and `lucide-svelte` (for icons).
- [ ] Configure Tailwind and verify the IPC connection between the Svelte frontend and the Rust backend.
- [ ] Set up the basic UI shell (Sidebar for history/PRs, Main canvas for the Svelte Flow dashboard, Bottom drawer for chat/steering input).

### Phase 2: Rust Backend - Pi Core & Git Worktrees
- [ ] Create a Rust command (`spawn_agent_session`) that executes `git worktree add` to create an isolated directory for the new session.
- [ ] Spawn the `pi` CLI as a child process utilizing RPC mode, setting its working directory to the newly created worktree.
- [ ] Implement a Rust async loop using `tokio` to read the stdout JSON RPC stream from Pi.
- [ ] Route Pi's telemetry events (`thought`, `tool_call`, `task_complete`) over Tauri's IPC (`app_handle.emit_all`) to the Svelte frontend.

### Phase 3: Rust Backend - GitHub CLI & Webhook Tunnel
- [ ] Create Rust commands to execute `gh` CLI operations synchronously and return JSON payloads (e.g., `gh pr view --json`).
- [ ] Implement an async background task in Rust that spawns `gh webhook forward` pointing to a local lightweight HTTP server (using `axum` or `warp`).
- [ ] Configure the local HTTP server to receive GitHub webhook payloads (issue comments, PR reviews, CI status) and immediately broadcast them to the Svelte frontend via Tauri IPC.

### Phase 4: Frontend - The Living Dashboard (Svelte Flow)
- [ ] Create custom Svelte Flow node components for the orchestration graph: `OrchestratorNode.svelte`, `WorkerNode.svelte`, `ActionNode.svelte`, and `ApprovalNode.svelte`.
- [ ] Implement Svelte state (using runes or Svelte stores) that listens to Tauri IPC events from the Pi core.
- [ ] Map Pi's incoming telemetry to graph updates: append new nodes to the nodes array on `tool_call` or `agent_spawned`, and dynamically update node status properties on `tool_result`.
- [ ] Integrate `dagre` to automatically recalculate the directed acyclic graph (DAG) layout top-to-bottom every time a new node is added, ensuring the UI remains organized without manual dragging.

### Phase 5: Interactive Steering & Review
- [ ] Implement a "Plan Mode" interceptor: When Pi requests a destructive command (file delete, git push), pause the Pi child process in Rust.
- [ ] Render the blocked action as an `ApprovalNode` in Svelte Flow, displaying "Approve" and "Modify" buttons inside the custom node.
- [ ] Wire the frontend buttons to send a Tauri command back to Rust, which then injects the approval or modification back into the Pi RPC stdin stream.
