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

### Behavior of Pi that the design relies on

These were verified against Pi 1.5.0.

- `pi.events` is shared between extensions loaded with `--extension` and extensions Pi discovers itself, so a producer needs no dependency on PiPan.
- `setWidget` carries large values in 64 KiB `widgetLines` entries without truncation. An `editor` request carries large `prefill` and reply values.
- Widgets are not replayed to a late client, so the shim caches the latest state per channel and topic and re-emits it on a `_shim.resync` intent.
- `appendEntry` entries and custom messages reach the RPC stream with `data` and `details` intact. `get_messages` omits `appendEntry` entries, and `get_entries` returns both and accepts a `since` entry id. Pi writes the session file only after an assistant message exists, so an earlier cursor can answer `Entry not found` after a restart. The host then reloads everything.
- Extension commands run through `prompt` return `disposition: "handled"`, run even mid-stream, and add nothing to the transcript. This carries intents. A slash command that no extension registered returns `"started"` and goes to the model, so the host checks `get_commands` for `pipan-intent` first.
- A pending dialog is not cancelled when the session switches or Pi aborts, so the host sends `cancelled: true` for every outstanding request id.
- `session_start` can fire more than once per session, so shim startup work is idempotent.
- Over RPC, `ctx.ui.custom()` returns `undefined`, which is why extensions need a channel to show rich UI.

### Trust and limits

The channel id is self-declared because Pi does not say which extension issued a widget or dialog. The UI shows it as "requested by `<ch>`". All payloads are untrusted data: markdown is sanitized, nothing is evaluated, and size limits apply (see the [Channels reference](./channels)). At most 20 requests are pending per session and 60 are accepted per minute.

## SQLite Persistence {#sqlite-persistence}

PiPan stores application metadata in a local SQLite file (`pipan.db`).

### Key Design Points

- **Safe Concurrency**: PiPan uses Write-Ahead Logging (`PRAGMA journal_mode = WAL`) and foreign key validation (`PRAGMA foreign_keys = ON`).
- **Transcript Separation**: `pi` stores its own raw conversation logs in `.jsonl` files. SQLite stores metadata, search indexes, and layout positions.
- **Fast Search**: Full queries run inside SQLite using SQL indexes. Search filters never slow down the frontend.

### What SQLite Stores

- **Sessions**: Session ID, custom name, project folder path, model ID, token usage, cost, and timestamps.
- **Node Positions**: Custom (X, Y) coordinates for the DAG canvas, saved per session and layout direction.
- **Channel Node Positions**: Custom coordinates for graph panels that extensions publish, saved per session, channel, topic, graph and node.
- **GitHub Views**: Saved pull request filter queries.
- **Tags & Metadata**: Tags and group assignments for organizing large lists of sessions.
