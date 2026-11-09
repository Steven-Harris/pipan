# Pipan

A native desktop application to run, inspect, and steer Pi AI coding agents.

[![CI](https://github.com/Steven-Harris/pipan/actions/workflows/ci.yml/badge.svg)](https://github.com/Steven-Harris/pipan/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](package.json)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-blue)](https://v2.tauri.app/)

---

## What is Pipan?

Pipan is a desktop application to run, inspect, and steer `pi` coding agents. It wraps the `pi` CLI in a visual workspace built with Tauri v2, Svelte 5, and Rust.

Instead of running `pi` in a plain terminal, Pipan communicates with `pi` using JSON-RPC over `stdin` and `stdout`. This lets you follow model reasoning steps, monitor tool calls, track token costs, steer tasks in real time, and view your workflow as an interactive node graph.

---

## Key Features

- **Rust Sidecar & RPC Manager:** Controls `pi` as a background child process (`pi --mode rpc --no-session`). Parses JSONL events, streams `stderr` diagnostics, and handles session lifecycles.
- **Split-Panel Workspace:** Organize your workspace with panels for Chat, DAG Canvas, File Explorer, Terminal, and GitHub Pull Requests.
- **Living DAG Canvas:** Visualizes agent turns, tool calls, and approval steps as a node graph using `@xyflow/svelte`. Saves node coordinates for each session.
- **Persistent SQLite Storage:** Stores sessions, messages, turns, tool calls, and graph states in `pipan.db`. Includes search, grouping, filtering, archiving, and deletion.
- **Mid-Flight Steering:** Send follow-up instructions, steer running tasks, or abort turns immediately with dedicated controls.
- **Deep GitHub Integration:** Uses the `gh` CLI and local webhook forwarding to track pull requests, reviews, and CI checks without MCP servers.
- **Embedded Terminal & File Explorer:** Open a native pseudo-terminal or browse project files directly inside the workspace.
- **Cost & Usage Analytics:** Scans session transcripts to track token counts and estimated costs across days, weeks, months, and years. Displays telemetry in the bottom status bar.
- **In-App Doctor:** Scans your system for `pi`, `gh`, `git`, Node.js, and compiler toolchains. Provides fix commands for missing tools or auth errors.
- **Extension System:** Load custom workspace extensions that contribute panels, custom views, status bar items, and slash commands.

---

## Architecture at a Glance

```text
┌────────────────────────────────────────────────────────┐
│             Frontend (Svelte 5 + Runes)                │
│   Chat • DAG Graph • Panels • Cost Dashboard • Doctor  │
└──────────────────────────┬─────────────────────────────┘
                           │ Tauri IPC (Commands & Events)
┌──────────────────────────▼─────────────────────────────┐
│                 Tauri v2 Host (Rust)                   │
│  SQLite (pipan.db) • Terminal PTY • Extension Loader   │
└────────────┬─────────────────────────────┬─────────────┘
             │ JSON-RPC (stdin/stdout)     │ Direct CLI execution
┌────────────▼─────────────┐ ┌─────────────▼─────────────┐
│   pi CLI Sidecar Process │ │    git & gh CLI Tools     │
└──────────────────────────┘ └───────────────────────────┘
```

- **Desktop Shell:** Tauri v2 with Rust.
- **Frontend:** Svelte 5 (Runes), SvelteKit static adapter, Tailwind CSS v4, and `@xyflow/svelte`.
- **Sidecar Manager:** Spawns `pi --mode rpc --no-session` via `tokio` async pipes and parses JSONL streams.
- **Persistence:** Local SQLite database (`pipan.db`) running in WAL mode.
- **Integrations:** Direct host execution of `git` and `gh` CLI commands, plus a local HTTP webhook listener.

---

## Prerequisites

Install these tools before running Pipan:

1. **[pi CLI](https://github.com/badlogic/pi-mono):** Install and verify it is available on your `PATH`:
   ```bash
   pi --version
   ```
2. **Git:** Required for workspace isolation and repository tracking:
   ```bash
   git --version
   ```
3. **[GitHub CLI (`gh`)](https://cli.github.com/):** Required for pull requests and webhook forwarding:
   ```bash
   gh auth status
   ```
4. **Node.js & pnpm:** Node.js 20+ and pnpm 10+:
   ```bash
   node --version
   pnpm --version
   ```
5. **Rust toolchain:** Cargo and `rustc` (2024 edition supported):
   ```bash
   cargo --version
   ```
   Follow the [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/) for your operating system.

---

## Getting Started

1. Clone the repository:
   ```bash
   git clone https://github.com/Steven-Harris/pipan.git
   cd pipan
   ```

2. Install dependencies:
   ```bash
   pnpm install
   ```

3. Run in development mode:
   ```bash
   pnpm tauri dev
   ```

This command starts the Vite dev server and opens the native Tauri desktop window with hot module reloading.

---

## Development Commands

- **Type-check frontend:**
  ```bash
  pnpm check
  ```

- **Run Rust tests:**
  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml
  ```

- **Check Rust code:**
  ```bash
  cargo check --manifest-path src-tauri/Cargo.toml
  ```

- **Build production bundle:**
  ```bash
  pnpm tauri build
  ```

---

## License

[MIT](package.json)
