# Architecture

Learn how PiPan works under the hood.

## Overview {#overview}

PiPan combines a Rust desktop runtime with a modern web interface. It runs `pi` as an external sidecar process.

The system has three main layers:

1. **Rust Backend (Tauri v2)**: Manages OS tasks, sidecars, file access, and local storage.
2. **Frontend UI (Svelte 5 & Tailwind CSS)**: Renders live chat, thinking blocks, and graph canvases.
3. **Pi Sidecar (`pi --mode rpc`)**: Executes reasoning, generates tool calls, and modifies code.

```
+------------------------------------------------------+
|                     PiPan UI                         |
|     (Svelte 5 Runes + SvelteKit + Tailwind CSS)     |
+--------------------------+---------------------------+
                           | IPC Commands & Events
+--------------------------v---------------------------+
|                    Tauri Rust Core                   |
|  - Process Manager (Tokio)    - SQLite Storage       |
|  - GitHub CLI Driver          - Worktree Manager     |
+--------------------------+---------------------------+
                           | JSON-RPC (stdin / stdout)
+--------------------------v---------------------------+
|                   Pi Agent Sidecar                   |
|              (pi --mode rpc --no-session)            |
+------------------------------------------------------+
```

## Tauri & Rust Core {#tauri-and-rust-core}

Tauri v2 gives PiPan native speed and small memory use.

The Rust backend handles critical tasks:

- **Child Process Supervision**: Launches and monitors the `pi` sidecar with Tokio. It cleans up child processes when sessions stop.
- **Typed IPC Layer**: Sends events to the webview using Tauri's typed event system (`pi:event`, `pi:stderr`, `pi:status`).
- **File System Operations**: Reads files, inspects git status, and creates temporary sandboxes.
- **Async Execution**: Runs heavy database operations and CLI commands on background worker threads. The UI thread never freezes.

## Pi RPC Sidecar {#pi-rpc-sidecar}

PiPan does not run `pi` inside a virtual terminal. It starts `pi` with `--mode rpc`.

### Communication Protocol

- **Input (`stdin`)**: PiPan sends JSON commands as single lines.
- **Output (`stdout`)**: `pi` writes JSON responses and streaming events as newline-delimited JSON (JSONL).
- **Diagnostics (`stderr`)**: Raw logs stream into a collapsible diagnostics drawer.

### Event Stream (`pi:event`)

The sidecar emits structured events during every turn:

- `text_delta`: Streaming text chunks for the assistant response.
- `thinking_delta`: Reasoning traces shown inside collapsible thinking blocks.
- `tool_call`: Name, arguments, and execution state of requested tools.
- `tool_result`: Output or error message from each tool run.

### Mid-Turn Steering

You can guide `pi` while it generates tokens:

- **Steer (`steer`)**: Injects immediate course corrections into the active turn.
- **Follow-up (`followUp`)**: Adds instructions to a queue. The agent processes them on the next turn.
- **Abort (`abort`)**: Terminates generation and clears pending queues instantly.

## Extension Channel {#extension-channel}

Pi extensions can drive native PiPan UI through channels. PiPan loads a small shim extension into the primary Pi session (`--extension <shim>`). The shim turns calls on the shared `pi.events` bus into messages that Pi's RPC mode already carries: widgets for state, an `editor` dialog for requests, custom entries for the log. A prompt that starts with `/pipan-intent` carries the reply from the host.

The frontend store (`src/lib/stores/channels.svelte.ts`) decodes and validates those messages, keeps state per session, queues requests, and renders them with built-in primitives: a graph panel, a diff or document review, and a form. Nothing activates unless the shim announces itself. See [Channels](./channels).

## SQLite Persistence {#sqlite-persistence}

PiPan stores application metadata in a local SQLite file (`pipan.db`).

### Key Design Points

- **Safe Concurrency**: PiPan uses Write-Ahead Logging (`PRAGMA journal_mode = WAL`) and foreign key validation (`PRAGMA foreign_keys = ON`).
- **Transcript Separation**: `pi` stores its own raw conversation logs in `.jsonl` files. SQLite stores metadata, search indexes, and layout positions.
- **Fast Search**: Full queries run inside SQLite using SQL indexes. Search filters never slow down the frontend.

### What SQLite Stores

- **Sessions**: Session ID, custom name, project folder path, model ID, token usage, cost, and timestamps.
- **Node Positions**: Custom (X, Y) coordinates for the DAG canvas, saved per session and layout direction.
- **Channel Node Positions**: Custom coordinates for graph panels that extensions publish, saved per session, graph and node.
- **GitHub Views**: Saved pull request filter queries.
- **Tags & Metadata**: Tags and group assignments for organizing large lists of sessions.
