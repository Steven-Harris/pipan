# Extension channel and UI primitives

Status: draft. Transport facts in section 7 were checked by a spike against Pi 1.5.0; results are in section 8.

## 1. Goal

Let any Pi extension drive rich native UI in PiPan (live graphs, line-commented reviews, forms) without PiPan knowing anything about what the extension does. Pi's RPC mode cannot render custom components (`ctx.ui.custom()` returns `undefined`), so extensions that depend on custom terminal UI degrade to plain `select` and `input` dialogs. PiPan can offer a better surface if there is a generic way for an extension to hand it data and receive answers.

Non-goals: a PiPan-specific agent protocol, replacing Pi's RPC, or any domain concept (workflows, CI, code review) in the PiPan core.

## 2. What Pi's RPC gives us

Source: Pi docs `rpc-extension-ui.md`, `rpc-commands.md`, `json.md`.

| Direction | Mechanism | Properties |
|---|---|---|
| Pi to PiPan, keyed state | `setWidget(key, string[])`, `setStatus(key, text)` | Fire-and-forget. Last value wins. Clearable. Not replayed to a late client. |
| Pi to PiPan, request/response | `select`, `confirm`, `input`, `editor` with an `id` | Extension awaits the answer. Cancel and timeout supported. `editor` carries a free-form `prefill` and `value`. |
| Pi to PiPan, durable log | `sendMessage({ customType })`, `appendEntry` (`entry_appended` event) | Persisted in the session file. |
| PiPan to Pi | `prompt` with `/command args` | Extension commands run immediately, even mid-stream. |
| Discovery | `get_commands` | Lists extension commands. |

There is no first-class extension message type, and all payloads are strings. The channel is an envelope multiplexed over these.

## 3. Design

### 3.1 Shim extension

PiPan ships a small Pi extension (the shim) and loads it with `--extension` when it spawns Pi, as it does for the plan gate. The shim owns the wire encoding. Other extensions talk to it through Pi's shared in-process event bus (`pi.events`), so they have no build-time or runtime dependency on PiPan.

```
Pi extension                      Shim (PiPan-owned)                  PiPan app
pi.events.emit("pipan:state")  -> setWidget("pipan:<ch>:<topic>")   -> channel store
pi.events.emit("pipan:ask")    -> editor request + correlation id    -> UI surface
   <- reply callback in event     <- extension_ui_response            <- user answer
pi.events.on("pipan:intent")   <- hidden slash command (prompt)      <- UI action
```

Handshake: an extension emits `pipan:hello` with a reply callback. The shim replies with `{ protocol, primitives }`. No reply means no PiPan host, and the extension uses its normal UI. A host that lacks a primitive the extension needs causes the extension to fall back.

### 3.2 Channel primitives

Every message is scoped by a channel id (the PiPan plugin id) and a topic.

1. **State**: keyed, last-value snapshots. Cleared with `null`. The host stores the latest value per (session, channel, topic). The shim re-emits all state on a `resync` intent, which PiPan sends when it attaches to or switches a session.
2. **Request**: a question with a reply. The host renders it with the surface registered for its `kind`. Answer or cancel resolves the callback. Session stop, switch, or abort cancels pending requests.
3. **Intent**: user action to extension. Sent as a hidden slash command to the shim, which dispatches `pipan:intent`.
4. **Log**: durable entries keyed by `customType`, rendered by a registered message renderer. Two kinds, chosen by the producer:
   - `entry`: `pi.appendEntry`. Persisted, never sent to the model. The default for UI-only history.
   - `message`: `pi.sendMessage`. Persisted and part of the model's context, so it costs tokens and can affect the next turn. Use it only for content the model must see. Its `deliverAs` and `triggerTurn` options pass through unchanged.

   The host reloads both with `get_entries` and the last-seen entry id as a `since` cursor.

### 3.3 Envelope

```json
{ "v": 1, "ch": "vendor.my-ext", "topic": "graph", "seq": 12, "payload": {} }
```

- `v`: protocol version. The host ignores versions it does not know.
- `seq`: monotonic per (channel, topic) so the host drops stale snapshots.
- Large payloads are split across `widgetLines` entries; the shim reassembles. Reviews with big diffs use lazy loading (send a file list, fetch bodies through an intent).

### 3.4 Frontend plugin API additions

Extends `window.pipan` and `registerExtension` in `src/lib/extensions/`:

```ts
pipan.channel(extId): {
  subscribe(topic, cb): Unsubscribe;          // last value replayed from the host store
  onRequest(kind, handler): Unsubscribe;      // custom renderer for a request kind
  intent(name, payload): Promise<void>;
}
registration.messageRenderers[customType] = (message, container) => cleanup;
```

### 3.5 Tiers

- **Tier 1, declarative primitives.** PiPan renders data against a small set of schemas; the extension needs no frontend code.
  - `graph`: nodes (open `kind` string, label, status), edges (optional label), current node, optional per-node detail.
  - `review`: a diff or a document, line-anchored comments with replies and statuses, a set of choices. A single comment model serves both subjects.
  - `form`: select, multi-select checklist, text, editor with prefill, markdown body with options.
- **Tier 2, custom plugin UI.** Existing DOM-container plugin model, subscribing to channels, for UI the primitives cannot express. The 1 MiB bundle limit applies.

New primitives are added only when a second consumer proves the shape.

## 4. Security and trust

- Pi does not say which extension issued a dialog or widget, so the channel id is self-declared. The UI shows it as "requested by <id>".
- All payloads are untrusted data. Markdown is rendered through a sanitizer. No payload is evaluated or injected as HTML.
- Enforce payload size limits and per-session request rate limits. Pending requests are bounded.
- External plugins keep the existing restriction: no direct shell access; host operations go through validated Tauri commands.

## 5. Open design questions

- Dialog queueing when several requests are pending (serial modal queue or an inbox).
- Whether `review` answers return only the user's edits (intents) or the full comment list. Prefer edits plus ids, with the producer owning anchoring.
- Where tier 1 primitives live in the extension manifest (a `contributes.channels` block that declares which primitive each topic uses).
- Upstreaming a first-class extension message to Pi, which would let the shim shrink.

## 6. Phases

This is an outline. The working plan, with files, tasks and acceptance criteria per milestone, is [extension-channel-implementation.md](./extension-channel-implementation.md).

1. Spike (throwaway extension) to confirm section 7.
2. Spec: schemas for envelope, `graph`, `review`, `form`. Draft in [extension-channel-spec.md](./extension-channel-spec.md).
3. Shim extension plus Rust routing (`pi_rpc.rs` emits `pipan:channel` events keyed by session) plus a channel store (`channels.svelte.ts`).
4. `graph` primitive and panel (reuses `@xyflow/svelte` and Dagre).
5. `form` primitive and generic request modal.
6. `review` primitive: diff then document.
7. Reference producer extension in the repo with synthetic data, used for tests and as an authoring example.
8. Plugin API docs in `docs/guide/extensions.md`.

## 7. Transport facts to verify

1. `pi.events` is shared between extensions loaded through `--extension` and through installed packages.
2. A request issued from a `pi.events` handler can use `ctx.ui` captured in the shim's `session_start`.
3. `entry_appended` and custom messages reach the RPC stream with their `data` intact.
4. `setWidget` and `editor` payload size limits, and whether widget lines are subject to trimming.
5. Behavior of pending dialogs when the Pi process is stopped, and when `switch_session` is sent.
6. Extension commands run during a stream and do not enter the transcript (needed for hidden intents).
7. Whether widgets are re-emitted on session start or reload.

## 8. Spike results

Setup: a throwaway shim extension and a separate producer extension (outside the repo, under `/tmp`), driven over `pi --mode rpc --no-session --offline`. No model calls were needed because everything ran through extension commands.

| # | Question | Result |
|---|---|---|
| 1 | `pi.events` shared across extensions | Yes. Worked between two `-e` extensions, and between an `-e` extension and a project-local auto-discovered extension (`.pi/extensions/`). Installed packages were not tested separately but use the same loader. |
| 2 | Request issued from an event handler using `ctx` captured in `session_start` | Works. `ctx.mode` is `"rpc"` and `ctx.hasUI` is `true`. The handshake reply carried both. |
| 3 | `entry_appended` and custom messages reach the RPC stream | Yes, with `data` and `details` intact: `entry_appended` for `appendEntry`, `message_start` and `message_end` (role `custom`, with `customType`) for `sendMessage`. |
| 3b | Replay | `get_messages` returns custom messages but not `appendEntry` entries. `get_entries` returns both (types `custom` and `custom_message`), with a `since` id cursor, so both kinds are reloadable. |
| 4 | Payload size | `setWidget` delivered 3 MB in one request, in 64 KiB `widgetLines` chunks with no truncation. An `editor` request carried a 500 KB `prefill` and a JSON reply round-tripped in 8 ms. Chunk reassembly is still the shim's job. |
| 5 | Pending dialog on session switch | The pending `editor` request is not cancelled. `new_session` succeeded and the extension's promise stayed unresolved. PiPan must send `{cancelled: true}` for every pending request id when it switches or stops a session, and the shim should time out requests. |
| 6 | Hidden intents | Extension commands run via `prompt` return `disposition: "handled"` and add nothing to the transcript. An intent sent with `/pipan-intent {...}` reached the producer. |
| 7 | Widget replay | Widgets are emitted when set and are not replayed on `get_state`. A late client sees nothing until the extension publishes again, so the shim must cache the latest state per key and re-emit on a `resync` intent. |

Other observations:

- `new_session` caused `session_start` (reason `new`) to fire twice, so the shim's startup work must be idempotent and must not assume one call per session.
- Slash-command prompts return `disposition: "started"` when the command is not registered (the text goes to the model), and `"handled"` when it is. A client can check `get_commands` before sending an intent.
- Not tested: behavior with several concurrent dialogs, and a process killed during a pending request.

## 9. Consequences for the design

- Shim caches state per (channel, topic) and re-emits on `resync`. Required.
- Pending-request cleanup belongs in PiPan (`pi_rpc.rs` or the chat store), keyed by session, plus a shim-side timeout.
- State and requests can carry large payloads, so lazy loading is an optimization, not a prerequisite for the first version.
- Durable history uses `appendEntry` by default, so it stays out of the model's context. `sendMessage` is for content the model should see, and the shim exposes both. The host reloads history with `get_entries` and a `since` cursor.
- State (section 3.2, item 1) is for the current picture of something. Log entries are for history. A producer republishes state on resync and does not rely on replaying the log to rebuild it.
