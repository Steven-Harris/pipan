# Extension channel implementation plan

## Summary

- **Goal:** PiPan gets a generic, extension-agnostic channel so any Pi extension can drive native UI: a live `graph` panel, a `review` UI (diff and document, with line comments), and `form` dialogs.
- **Why:** over `pi --mode rpc`, `ctx.ui.custom()` returns `undefined`, so extensions that depend on custom terminal UI degrade to plain select and input dialogs.
- **Mechanism:** PiPan ships a small Pi extension (the shim). Other extensions talk to it over `pi.events`, and it encodes traffic onto existing RPC carriers: `setWidget` for state, `editor` dialogs for requests and answers, `appendEntry` and `sendMessage` for the log, and a hidden `/pipan-intent` command for user actions.
- **Constraint:** PiPan is public and MIT-licensed. No code, docs, tests, fixtures or commits may name or copy any internal or third-party extension. Fixtures are synthetic.
- **Spike results (Pi 1.5.0):** `pi.events` is shared across extensions; 3 MB of state and 500 KB requests work; custom entries and messages reach RPC clients and reload with `get_entries`. Two gaps shaped the design: pending dialogs are not cancelled on session switch, and widgets are not replayed to a late client.
- **Decided:** layout positions go in a new `channel_node_positions` table (migration v5); the shim loads for the primary session only; routing lives in the frontend; work happens on one local branch with a commit per milestone.
- **Open:** vitest, the markdown parser, pending requests in an unviewed session, and subagents being out of scope for v1.

Companion to [extension-channel.md](./extension-channel.md) (rationale, spike results) and [extension-channel-spec.md](./extension-channel-spec.md) (wire format and schemas). This document says what to build, in what order, in which files, and how each step is verified.

## 1. Findings from reading the code that shape the plan

- **Routing can live in the frontend.** `pi_rpc.rs` already forwards every stdout JSON line to the webview as `pi:event`, including `extension_ui_request`, `entry_appended` and custom messages. Nothing in Rust has to decode channel traffic. Replies already have a path: `send_pi_command` writes any JSON line to Pi's stdin. Rust only needs to load the shim and to tell the frontend when the process dies (already `pi:status`).
- **The shim loads the way the plan gate does.** `approval.rs` embeds `resources/pipan-plan-gate.ts` with `include_str!`, writes it with `ensure_plan_gate_extension`, and `agent.rs` passes `--extension <path>`. The shim follows that pattern for the primary session.
- **There is one primary Pi process.** `PiRpcManager` runs a single process and `start_session` shuts down the existing one. Subagent processes (`agent.rs`) have their own event handling. v1 covers the primary session only.
- **Panels are string ids.** `PanelId` is `BuiltinPanelId | (string & {})`, and `getPanelDefinition` falls back to contributed panels from `extensionRegistry`. Dynamic channel panels can register through the registry.
- **No frontend test runner exists.** `package.json` has `check` only, no vitest. Rust has unit tests in 18 modules. The codec, validators and stores are pure logic that must be tested, so a runner is part of milestone 1.
- **`FlowCanvas.svelte` (673 lines) and `flow.svelte.ts` render PiPan's own agent DAG.** The `graph` primitive reuses `@xyflow/svelte`, Dagre and the persistence in `graph-layout.svelte.ts`, but its data model is different. Generalizing `FlowCanvas` is riskier than a new component that shares layout helpers.

## 2. Principles

- Every milestone is shippable on its own: a user who never loads a producer sees no change.
- The reference producer comes first. It is the fixture that every later milestone is built and verified against, and the example third parties copy.
- The frontend works from the spec's types only. Validation rejects anything else.
- Each milestone ends with the Rule 4 checks: `pnpm check`, `cargo test`, `cargo clippy`, `cargo fmt`, plus the new tests.

## 3. Dependency order

```
M0 shim + reference producer + harness
        |
M1 host plumbing (codec, stores, cancel, resync)
     /          \
M2 graph        M3 request modal + form
                    |
                M4 review: diff
                    |
                M5 review: document
     \              /
      M6 plugin API + docs
```

M2 and M3 are independent after M1 and can be built in either order. M4 and M5 need M3's modal shell.

> Request expiry was removed after this plan was written: a request now waits until the user answers or cancels it, the session changes, or Pi exits. See [extension-channel-followups.md](./extension-channel-followups.md), F6.

## 4. Milestones

### M0. Shim, reference producer, harness

Goal: Pi emits correct channel traffic and a script can verify it without the app.

Tasks:
1. `src-tauri/resources/pipan-channel-shim.ts`: implements sections 2 and 3 of the spec. Event handlers for `pipan:hello`, `state`, `request`, `log`; the `pipan-intent` command; chunking; `seq` counters; a state cache with `_shim.resync`; the `pipan:shim` announce widget on every `session_start`; a request timeout; idempotent startup.
2. `examples/channel-reference/producer.ts`: a standalone Pi extension using only `pi.events`. Commands: `/demo-graph` (publishes a graph and advances it on a timer), `/demo-form`, `/demo-review-diff`, `/demo-review-doc`, `/demo-log`. Synthetic data only, including a large diff fixture.
3. `scripts/channel-harness.mjs`: spawns `pi --mode rpc --offline --no-session` with the shim and the producer, drives each command, and asserts on the emitted stream. This is the spike code, cleaned up. Package script `pnpm test:channel` (needs `pi` on PATH, so it is not part of `pnpm check`).

Acceptance: the harness passes for hello, state replace and clear, chunked 3 MB state, request answered and cancelled, resync re-emitting cached state, intent reaching the producer, and double `session_start`.

Risks: Pi API drift. Mitigation: the harness records the Pi version.

### M1. Host plumbing

Goal: the webview decodes channel traffic into stores, and pending requests never leak.

Tasks:
1. Rust: `ensure_channel_shim(app)` in a new `src-tauri/src/channel.rs` (mirrors `ensure_plan_gate_extension`); `start_session` passes `--extension <path>` for the primary session. Unit tests for the argument list and file write.
2. Add `vitest` as a dev dependency and a `test` script. Keep `pnpm check` unchanged.
3. `src/lib/channels/types.ts`: the spec's types, exported. `limits.ts`: the spec's limits.
4. `src/lib/channels/codec.ts`: decode and encode envelopes; widget-chunk reassembly; `seq` filtering. Pure, tested.
5. `src/lib/channels/validate.ts`: validators per primitive (hand-written guards; no runtime schema library unless one is already a dependency). Pure, tested, including limit violations and unknown fields.
6. `src/lib/stores/channels.svelte.ts`: per `(sessionId, ch, topic)` state map; pending request queue per session; `answer(requestId, payload)` and `cancelAll(sessionId)`; producer error list.
7. Wire into `chat.svelte.ts` `init()`: route `extension_ui_request` (`setWidget` keys starting `pipan:s:` and `pipan:shim`, `editor` titles starting `pipan:r:`) and `entry_appended` or custom-message events with `pipan:l:` types to `channels`. Replies call `send_pi_command` with `extension_ui_response`. Non-channel requests pass through unchanged.
8. Lifecycle: on `pi:status` of `disconnected` or `error`, and before `switch_session` or `new_session`, call `cancelAll` (sends `cancelled: true` for each pending id). On attach and after a switch, send the `_shim.resync` intent and call `get_entries` with the saved `since` cursor.
9. Capability gate: nothing happens until the `pipan:shim` announce is seen.

Acceptance: with the reference producer loaded, a dev-only debug view of the store shows live state, request queue and log entries; switching sessions leaves no pending request; vitest covers codec and validators.

Risks: `chat.svelte.ts` is 1,866 lines. Keep the channel hook to one call site and put all logic in the channels store. Confirm during this milestone that the `get_entries` `since` cursor survives a session file reload.

### M2. `graph` primitive

Goal: a live graph panel for any producer.

Tasks:
1. `src/lib/components/channels/GraphPanel.svelte`: `@xyflow/svelte` canvas fed from the store; node card by `kind` and `tone` (stable per-kind palette from a fixed token set); status marks; visit and edge-taken counts; click selects a node and opens a detail drawer (summary, fields, markdown through the sanitizer, timing and cost).
2. `src/lib/channels/graph-layout.ts`: Dagre layout reusing the helpers behind `graph-layout.svelte.ts`; positions persisted per `(session, graph.id, node.id)` in `channel_node_positions` (decision 1). The v5 migration and its repository functions in `db.rs` and `storage.rs` ship with this task, with Rust tests for the migration and the upsert.
3. Panel registration: a built-in extension `pipan.channels` that registers a panel on demand for each channel topic whose primitive is `graph`. First state seen adds a picker entry; it is not opened automatically.
4. Actions: buttons render from `actions`; press sends the `graph.action` intent, with a confirm dialog when `confirm` is set.
5. Over-limit handling: first 500 nodes plus a notice.

Acceptance: the reference graph appears in the picker, animates as status changes, keeps manual positions across a session switch and back, and action buttons reach the producer.

Risks: node count and update rate. Mitigation: coalesce to 10 per second in the store; do layout only when the node or edge set changes, not on status changes.

### M3. Request modal shell and `form` primitive

Goal: any producer can ask a question and get an answer.

Tasks:
1. `src/lib/components/channels/RequestHost.svelte`: renders the head of the session's pending queue in a modal (provenance line "requested by `<ch>`"), shows queue depth, handles dismiss as cancel.
2. `FormModal.svelte`: body (sanitized markdown, scrollable), fields (select, multiselect, text), actions, validation before an action without `skipValidation`.
3. Unknown request kinds: shown as "unsupported request" with a cancel button, so a producer never hangs the UI.

Acceptance: harness-driven form flows (pick one, checklist, multiline edit with prefill, body plus actions only) answer correctly; dismiss cancels.

### M4. `review`: diff

Goal: line-commented diff review with choices.

Tasks:
1. `DiffReview.svelte`: file list with status and counts, unified view, line gutter selection (single line and ranges), inline comment composer, thread display with replies and status, outdated markers, and collapse of large files.
2. A comment model module `src/lib/channels/review-model.ts`: add, edit, reopen, dismiss; assigns `tmp-<n>` ids; computes `touched`; pure and tested.
3. Choice bar from `choices` with `requiresComment` enforcement; answer carries `choice`, full `comments`, `touched`, optional `note`.
4. Virtualized rendering for large diffs (50,000 lines at the limit).

Acceptance: the reference diff, including the large fixture, scrolls smoothly; comments round-trip to the producer with correct ids and `touched`; earlier-round comments appear as threads.

Risks: performance on very large diffs. Mitigation: virtualized rows from the start; a split view is a later option, not in v1.

### M5. `review`: document

Goal: line-commented document review with the same model.

Tasks:
1. `DocReview.svelte`: render markdown to blocks with source line ranges (use a markdown parser already in the project; add one only if none exists), attach the gutter and composer to a block's source range.
2. Reuse the comment model, thread display and choice bar from M4.

Acceptance: the reference document review answers with correct line ranges; multi-document subjects switch cleanly.

### M6. Plugin API and docs

Goal: third parties can use channels from a PiPan plugin and from a bare Pi extension.

Tasks:
1. Extend `window.pipan` and `PiPanRuntimeExtensionRegistration` with `channel(extId)` (`subscribe`, `onRequest`, `intent`) and `messageRenderers`, as sketched in the plan.
2. Manifest: optional `contributes.channels` for plugins that want a fixed panel placement, title and icon.
3. Docs: promote the spec to `docs/guide/` (a "Channels" page), add an authoring guide that walks through the reference producer, and update `docs/guide/extensions.md`.
4. Add the channel events and commands to the tables in `AGENTS.md` and the architecture guide.

Acceptance: a minimal external plugin subscribes to a channel and renders custom UI; the docs build; the reference producer README matches the guide.

## 5. Branching and commits

All work happens locally on one branch, `feat/extension-channel`, created from `main`. No PRs are opened until the whole feature works locally.

- The plan documents in `docs/plans/` are committed first, as the branch's opening commit.
- Each milestone gets one or more commits, made when the milestone's acceptance criteria and Rule 4 checks pass: `pnpm check`, `cargo test`, `cargo clippy`, `cargo fmt --check`, plus the milestone's own tests. A milestone is not committed half-done, so every commit builds and passes.
- Commit messages use the repository's conventional prefixes (`feat:`, `test:`, `docs:`, `refactor:`), name the milestone, and carry no emoji. Example: `feat(channels): M1 host plumbing and channel store`.
- M2 and M3 are independent after M1. Build them in sequence on the same branch, in either order.
- No feature flag is needed because nothing activates without the shim announce. The debug store view in M1 stays behind a dev-only check.
- Before any PR, review the whole branch diff, decide how to split or squash it, and open the PRs with `.github/PULL_REQUEST_TEMPLATE.md`.

## 6. Decisions needed before or during the work

1. **Graph layout persistence (decided).** A new `channel_node_positions` table in migration v5, keyed by `(session_id, graph_id, node_id)` with `x`, `y`, `pinned`, and a cascade delete on `session_id`. The existing `node_positions` table is untouched.
2. **Test runner.** Add vitest (M1), as a dev dependency only.
3. **Markdown parser.** Check what the frontend already ships before choosing one for M3 and M5.
4. **Pending requests in a session the user is not viewing.** Resolved: Pi runs one session at a time and a session switch cancels every pending request, so only the viewed session can have one. No badge is needed.
5. **Subagent processes** are out of scope for v1. Revisit when a producer needs them.

## 6a. Follow-ups from the manual run

Found while running the reference producer in `pnpm tauri dev`. Items 1 to 3 are done in the follow-ups plan (F3), see [extension-channel-followups.md](./extension-channel-followups.md).

1. **Graph panels open only from the picker.** After a producer publishes a graph, nothing visible happens until the user opens the panel from the "+" menu. Proposal: open a graph panel once per session and topic, the first time it is published. If the user closes it, it stays closed for that session. Changes spec section 8, which says panels are not opened automatically.
2. **A waiting node with actions is easy to miss.** Proposal: when a node enters `waiting` and has actions, select it so its drawer opens, and show an attention dot on the panel tab while the panel is not in view.
3. **The reference producer models approval as a graph action.** A decision the producer must wait for belongs in `pipan:request`, which opens the modal by itself and returns the answer directly. Proposal: `/demo-graph` awaits a `form` request at the Approve step and keeps the graph action as a second way to approve. The authoring guide should say that graph actions are for out-of-band controls such as pause or retry.
4. **Nothing opens a modal from a graph node.** Keep it that way: a node that needs a decision is a request.
5. **`notify` messages are not shown by PiPan.** The producer reports demo results through them, so they are invisible in the app. The producer now also writes an `intent-received` log entry, visible in the dev debug view. Decide whether PiPan should surface `notify` events, which is outside the channel work.
6. **Project trust.** Pi skips `.pi/extensions` in an untrusted folder, and RPC mode cannot prompt, so a project-local producer silently does not load. The authoring guide should tell authors to use `~/.pi/agent/extensions/` or save trust with `/trust`.

## 7. Verification matrix

| Layer | Tool |
|---|---|
| Shim and Pi behavior | `pnpm test:channel` (real `pi`, offline) |
| Codec, validators, review model, stores | vitest |
| Rust shim writer and argument list | `cargo test` |
| Components | `pnpm check`, plus manual runs of the reference producer in `pnpm tauri dev` |
| Final | `pnpm check`, `cargo test`, `cargo clippy`, `cargo fmt --check` |
