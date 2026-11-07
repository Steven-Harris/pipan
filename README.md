# Pipan

**Pipan** is a native desktop GUI and orchestration workspace for the [`pi`](https://github.com/badlogic/pi-mono) AI coding agent, built with **Tauri v2** and **Svelte 5**.

Instead of wrapping a pseudo-terminal, Pipan spawns `pi` as a structured JSON-RPC sidecar process (`pi --mode rpc --no-session`), turning the CLI agent into an inspectable, steerable desktop environment with real-time reasoning traces, tool-call telemetry, workspace isolation, and mid-flight human intervention.

---

## Quick Start

1. **Install the `pi` CLI** and make sure it is available on your system `PATH`:
   ```bash
   pi --version
   ```
2. **Download Pipan:** Go to the [**Releases**](https://github.com/Steven-Harris/pipan/releases) tab and download the latest installer or bundle for your platform:
   - **Windows (x64):** `.msi` or `.exe` (NSIS) installer
   - **macOS (Apple Silicon & Intel):** `.dmg` disk image
   - **Linux (x64):** `.deb`, `.rpm`, or `.AppImage`
3. **Launch & Connect:** Open **Pipan** and choose a session mode in the header:
   - Click **Start Headless** to launch an isolated scratchpad session (`<temp_dir>/pipan-headless`), or
   - Click **Open Folder…** to bind `pi` to a local project directory.

---

## Features

### Implemented
- **JSON-RPC Sidecar Supervisor (`src-tauri/src/pi_rpc.rs`)**
  - Spawns and manages a `pi --mode rpc --no-session` child process over async `stdin`/`stdout` pipes using `tokio`.
  - Generation-safe process lifecycle cleanup, newline-delimited JSONL event parsing (`pi:event`), live `stderr` streaming (`pi:stderr`), and connection state broadcasts (`pi:status`).
- **Flexible Session Modes**
  - **Headless Sandbox:** Spawns `pi` inside an isolated temporary directory (`<temp_dir>/pipan-headless`) for safe scratchpad exploration.
  - **Directory Workspace:** Binds `pi` to any local repository or project folder via the native OS folder picker.
- **Transparent Agent Telemetry**
  - Live streaming assistant responses with auto-scroll and scroll-lock override.
  - Collapsible **Thinking Blocks** (`ThinkingBlock.svelte`) to inspect reasoning traces in real time.
  - Expandable **Tool Call Cards** (`ToolCallCard.svelte`) displaying tool arguments, execution state, elapsed duration, and results.
  - Session header telemetry (`SessionHeader.svelte`) showing connection state, workspace path, active model, thinking level, token usage, and context compaction status.
  - Collapsible **Pi Diagnostics** drawer for raw `stderr` output and runtime error banners.
- **Mid-Flight Human Steering (`ChatInput.svelte`)**
  - Context-aware composer that switches from **Send** when idle to live **Steer**, **Follow-up**, and **Stop** controls while `pi` is working.
  - Visual queue pills for pending `steer` and `followUp` messages, plus immediate turn cancellation (`clear_queue` + `abort`).

### Roadmap (`plan/`)
- **Per-Session Git Worktree Isolation:** Automatically create isolated `git worktree` sandboxes per agent session so the host working tree is never mutated directly without review.
- **Direct GitHub CLI & Webhook Tunnel:** Execute synchronous `gh` CLI queries and stream real-time repository events via `gh webhook forward` into a local Rust HTTP listener—zero MCP server overhead and no REST polling rate limits.
- **Living Orchestration Graph:** Visualize multi-agent workflows, tool calls, and subagents as a live directed acyclic graph (DAG) using `@xyflow/svelte` and `dagre`.
- **Interactive Action Gating:** Intercept destructive operations in Rust and surface interactive **Approve / Modify** nodes before execution continues.

---

## Tech Stack

- **Desktop Shell & Backend:** [Tauri v2](https://v2.tauri.app/), Rust (2024 edition), `tokio`, `serde`, `serde_json`
- **Frontend:** [Svelte 5](https://svelte.dev/) (Runes), SvelteKit (`@sveltejs/adapter-static`), TypeScript, Vite
- **Styling & Icons:** Tailwind CSS v4 (`@tailwindcss/vite`), `lucide-svelte`
- **Agent Runtime:** `pi` CLI (`--mode rpc --no-session`)

---

## Development (Build from Source)

### Prerequisites

1. **pi CLI** installed and accessible on your system `PATH`:
   ```bash
   pi --version
   ```
2. **Node.js** and **pnpm** (`pnpm@10.x` recommended)
3. **Rust toolchain** (`rustup`, `cargo`) and [Tauri v2 system dependencies](https://v2.tauri.app/start/prerequisites/) for your OS

### 1. Install dependencies

```bash
pnpm install
```

### 2. Run in development mode

Launch the Vite dev server (`http://localhost:1420`) and the native Tauri desktop window with hot reload:

```bash
pnpm tauri dev
```

### 3. Type-check & validate

Run SvelteKit sync and `svelte-check`:

```bash
pnpm check
```

Check the Rust backend:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

### 4. Build for production

Compile the static frontend and bundle the native desktop installer for your platform:

```bash
pnpm tauri build
```

---

## Project Structure

```text
pipan/
├── src/
│   ├── lib/
│   │   ├── components/
│   │   │   ├── ChatInput.svelte      # Prompt composer + live Steer / Follow-up / Stop controls
│   │   │   ├── MessageItem.svelte    # User & assistant message renderer
│   │   │   ├── MessageList.svelte    # Auto-scrolling transcript & starter prompts
│   │   │   ├── SessionHeader.svelte  # Status badge, model pill, workspace & session actions
│   │   │   ├── ThinkingBlock.svelte  # Collapsible reasoning trace accordion
│   │   │   └── ToolCallCard.svelte   # Live tool execution inspector
│   │   ├── stores/
│   │   │   └── chat.svelte.ts        # Reactive Svelte 5 runes store & Tauri IPC bridge
│   │   └── types/
│   │       └── pi.ts                 # TypeScript definitions for Pi RPC commands & events
│   └── routes/
│       └── +page.svelte              # Main application layout & diagnostics drawer
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs                    # Tauri builder & IPC command registration
│   │   ├── github.rs                 # gh CLI commands returning JSON (PRs, checks, issues, runs)
│   │   ├── main.rs                   # Binary entrypoint
│   │   ├── pi_rpc.rs                 # Async Pi sidecar manager (stdin/stdout JSONL + stderr)
│   │   └── webhook.rs                # gh webhook forward tunnel + local axum receiver
│   ├── Cargo.toml
│   └── tauri.conf.json
├── plan/                             # Multi-phase architecture roadmap
├── PRODUCT.md                        # Product specification & capabilities overview
└── package.json
```

---

## License

[MIT](package.json)
