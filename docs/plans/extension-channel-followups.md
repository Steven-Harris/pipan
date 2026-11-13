# Extension channel follow-ups plan

## Summary

- **Goal:** close the gaps found after the extension channel milestones (M0 to M6) were built, so a long-running producer, one that runs for minutes, asks questions, and publishes a live graph, works well in PiPan.
- **Source of the items:** a manual run of the reference producer in `pnpm tauri dev` (recorded in section 6a of [extension-channel-implementation.md](./extension-channel-implementation.md)), and a review of what a producer with a large graph and many asks needs from the host.
- **Scope:** PiPan side only. Nothing here changes the wire protocol except one additive form field (F2), removing request expiry (F6), and one behavior change to the spec (F3).
- **Constraint:** PiPan is public and MIT-licensed. No code, docs, tests, fixtures or commits may name or copy any internal or third-party extension. Fixtures are synthetic.
- **Branch:** `feat/extension-channel`, continuing from M6. One commit per milestone.

Companion to [extension-channel.md](./extension-channel.md) (rationale), [extension-channel-spec.md](./extension-channel-spec.md) (wire format) and [extension-channel-implementation.md](./extension-channel-implementation.md) (M0 to M6, done). The published reference is `docs/guide/channels.md`.

## 1. Findings that shape this plan

Each was checked against the code or the Pi 1.5.0 docs.

- **Saved graph positions are keyed by `(session_id, graph_id, node_id)`** (`channel_node_positions`, `src-tauri/src/db.rs`). Neither the channel id nor the topic is in the key, so two graphs in one session that share a `graph.id` share positions.
- **Several keyed `{#each}` lists take their keys from producer data and are not checked for uniqueness.** Svelte throws at render when keys repeat. The validator checks unique ids for nodes, form fields, form actions and review choices only. The unchecked keys are: diff file `path`, document `path`, comment `id`, form option `value`, graph action `id` (graph-level and node-level).
- **PiPan answers only `extension_ui_request` records that belong to the channel.** `channels.svelte.ts` `handleUiRequest` returns false for everything else, and `chat.svelte.ts` has no handler. So Pi's `select`, `input`, `confirm` and non-channel `editor` dialogs are never answered, and the extension that asked waits forever. `notify` and `setStatus` are also not displayed. Subagent approval requests (`PIPAN_APPROVAL::` titles) are handled in Rust for subagent processes and are not affected.
- **Custom messages are not drawn in the chat.** `chat.svelte.ts` handles `message_start` and the history rebuild only for roles `user`, `assistant` and tool results. A `pi.sendMessage` message with `display: true` is stored by Pi but never shown, live or after a reload.
- **Extension commands are not stored by Pi.** A prompt that an extension command handles has response `disposition: "handled"` and adds nothing to the session file, so the chat cannot rebuild it after a reload, the same as in Pi's own TUI. The live chat shows it (fixed in `25c5043`).
- **There is one primary Pi process.** Switching sessions sends `switch_session` to the same process, and `channels.beforeSessionChange` cancels every pending request first (spec section 8). So a request cannot be pending in a session the user is not viewing. The earlier plan's "badge in the session list" recommendation is moot. The real issue is that clicking another session silently cancels a pending ask.
- **Graph panels open only from the "+" picker** (`registry.svelte.ts` `contributedPanels`, spec section 8). A producer that publishes a graph, or an idle graph with a start button, is invisible until the user opens the panel by hand.
- **A graph with no nodes and some graph-level actions is valid** (checked against `validateGraph` and `layoutChannelGraph`), and `GraphPanel.svelte` draws graph-level actions in its header whatever the node count. This gives a producer a launcher with no host change.
- **Pi 1.5.0 skips `.pi/extensions` in an untrusted project folder**, and RPC mode cannot prompt, so a project-local producer silently does not load. User-level extensions and packages are not affected.

## 2. Principles

- Fix correctness bugs first (F1). They are small and block nothing else.
- Protocol changes are additive and do not bump a primitive version (spec section 9). F2 follows that rule.
- Everything a producer controls is untrusted. Any new host feature that renders producer or extension text goes through the existing sanitizer.
- A pending request or dialog always has a way out: Cancel, Esc, a session change, or the process exiting. PiPan adds no timer of its own. A timeout that Pi sends with a bridged dialog (F5) is honored.
- No new background polling. New host UI reuses the request modal, the queue and the cancel rules already built.

## 3. Dependency order

```
F1 (key fixes) ----------------------------> independent, do first
F2 (requiredFor) --------------------------> independent
F3 (graph attention) ----------------------> independent
F4 (messages in chat) ---------------------> independent
F5 (dialog bridge) ------------------------> after F2 (uses form fields and requiredFor-free forms) and after F4 decisions on notify
F6 (request lifecycle) --------------------> after F5 (the guard covers bridged dialogs too)
F7 (type distribution) --------------------> after F1 and F2 (the schema must be final)
F8 (docs sweep and manual run) -------------> last
```

## 4. Milestones

### F1. Saved-position key, keyed-list validation and markdown sanitizing

Goal: two producers or topics cannot interfere, producer data cannot crash a dialog, and producer markdown is sanitized by a maintained library.

Tasks:
1. Positions key. Change `channel_node_positions` to `PRIMARY KEY (session_id, ch, topic, graph_id, node_id)`. Update `db.rs` (`load`, `save`, `clear` take `ch`, `topic`, `graph_id`), the three Tauri commands in `lib.rs`, `channel-graph-layout.svelte.ts`, and `GraphPanel.svelte`. Add migration v6 that rebuilds the table: create `channel_node_positions_new` with the new key, copy existing rows with `ch` and `topic` set to an empty string (the old rows carry neither), drop the old table, rename. Update `verification.rs` for the new user version and table shape.
2. Uniqueness in `validate.ts`. Add `uniqueBy(items, key, path)` and reject, with a path and a message, a diff file `path`, a document `path`, a comment `id`, a form option `value` within one field, and a graph action `id` within one list (graph-level and per node). For graphs, a duplicate action id is dropped with a notice, like other graph over-limit cases, because rejecting a whole graph for a button is harsh. Reviews and forms reject, as today.
3. Audit every other `{#each ... (key)}` in `src/lib/components/channels` against the list above, and make the debug panel keys safe too.
4. Docs: add "paths and ids must be unique within a request" to `docs/guide/channels.md` sections 4 to 6.

5. Markdown sanitizing. Replace the hand-written sanitizer in `src/lib/channels/markdown.ts` with DOMPurify, run over the HTML that `marked` produces. Add `dompurify` as a dependency. Configure it with a strict allowlist: no scripts, styles, forms, images or iframes, links limited to `http`, `https` and `mailto` with `rel="noopener noreferrer"` and `target="_blank"` forced by a hook, and no `data-` or `on*` attributes. Images become their alt text through a `marked` renderer override, as now. Keep `safeHref`. `renderMarkdownBlocks` sanitizes each block separately, and the result is the only value passed to `{@html}` (`Markdown.svelte` is the single site). Keep every existing case in `markdown.test.ts` as a regression suite, and add known bypass strings (nested `<svg>` and `<math>` content, `javascript:` and `data:` links, mutation-XSS patterns, HTML inside a table cell or a code fence). DOMPurify needs a DOM, and vitest runs in plain Node with no DOM today. Add `happy-dom` as a dev dependency and select it for the markdown tests only, with a `// @vitest-environment happy-dom` comment in that file, so the other tests stay in Node. The sanitizer module creates its DOMPurify instance lazily, so importing `markdown.ts` outside a DOM (the store and integration tests do) does not fail.

Tests: `validate.test.ts` cases for each duplicate; `channel-graph-layout` cases showing two topics with the same graph id keep separate positions; a Rust test with two channels saving to the same graph id; the markdown regression and bypass suite.

Acceptance: a review with two files of the same path is rejected and the request is answered `cancelled` with a producer error; two topics with one graph id keep their own dragged positions.

Risks for task 5: a stricter or looser allowlist changes what producers see, for example an `<img>` that used to render alt text. Mitigation: the regression suite covers the current output, and a difference is reviewed, not accepted automatically.

Risks for task 1: copied rows with an empty `ch` and `topic` never match a real graph, so they are orphans. Mitigation: delete them in the migration instead of copying, since positions only matter for the life of a run; the migration test asserts the table is empty and has the new key.

### F2. `FormField.requiredFor`

Goal: a text field can be required for some actions only, so a form with fixed choices plus an optional typed answer needs one request, not two.

Tasks:
1. Types: `requiredFor?: string[]` on `select`, `multiselect` (for its `min`) and `text` fields in `src/lib/channels/types.ts`.
2. Validator: accept the field; unknown action ids in the list are ignored with no error (a stale id must not break the form).
3. `form-state.ts`: `validateValues(form, values, actionId)` applies a field's `required` (and `min`) when `requiredFor` is absent, and only when the pressed action is in the list when it is present. `max` and option membership always apply.
4. `FormView.svelte`: pass the action to the validation call, show `*` on a field only while it applies to the focused or default action, and keep the error text per field.
5. Docs: spec section 6 and section 10 of the spec (remove from "Not in version 1"), guide `channels.md`, the reference producer (`/demo-form edit` uses it for "Save" vs "Discard"), and the authoring guide.

Tests: `form-state.test.ts` for each combination; a harness or store test that answers the reference form with a skipped field.

Acceptance: with `requiredFor: ["submit"]` on a text field, pressing "Submit" with empty text shows the error, and pressing a choice action sends an answer with an empty value.

Risks: an older host ignores the field and enforces `required` on every action. Mitigation: a producer must still validate (spec rule); the guide says to keep `required` off a field that has `requiredFor`.

### F3. Graph attention

Goal: a graph is seen when it matters, and a waiting node is easy to act on.

Tasks:
1. Open once. When a graph topic is published for a session for the first time, open its panel through `workspaceStore.openPanel`. Keep a per-session set of topics already opened, so a panel the user closed is not reopened for that session. Clear the set on session change and process exit. Declared graph panels (`contributes.channels`) follow the same rule.
2. Select waiting. In `GraphPanel.svelte`, when a node enters `waiting` and has actions, select it so the drawer opens. Do not steal selection if the user selected another node in the last few seconds.
3. Attention dot. In `PanelHost.svelte`, show a dot on a graph panel's tab when a node enters `waiting` with actions and the panel is not the active tab. Clear it when the tab becomes active.
4. Spec section 8 changes from "not opened automatically" to "opened once per session and topic on first publish". Update the spec, the guide, and section 6a of the first plan.
5. Reference producer: model approval as a request, which is the right tool for a decision the producer waits on. `/demo-graph` awaits a `form` request at the Approve step, shows the node as `waiting` while the modal is open, and keeps the graph action as a second way to approve. The authoring guide says graph actions are for out-of-band controls such as pause or retry.

Tests: store-level tests for once-per-session opening (first publish opens, close then republish does not, session change resets); a harness step for the approval form.

Acceptance: running `/demo-graph` opens the Demo pipeline panel by itself, and reaching Approve shows the modal and selects the node.

Risks: an extension could flood the workspace with panels. Mitigation: at most 4 panels are opened automatically per session, counted from the first auto-open and reset on session change. Panels the user opens from the "+" picker do not count.

### F4. Extension messages in the chat

Goal: what an extension says is visible in PiPan, and survives a reload.

Tasks:
1. Custom messages. Draw `role: "custom"` messages with `display: true` in the transcript, live (`message_end`) and in the history rebuild (`hydrateFromRawMessages`). Show the text as a distinct system card with the extension's `customType` as a small label, rendered with the existing sanitized markdown. Messages with `display: false` stay hidden. Channel log messages (`pipan:l:...`) stay out of the chat unless a plugin renderer exists, as now.
2. Plugin override. A plugin's `messageRenderers` entry still wins for its own channel's log messages. The new card is the fallback for any other custom message.
3. Notifications. Show `notify` events as a transient toast (info, warning, error), and also append them to a per-session list kept in the debug view so a missed one can be read. Notifications are not added to the transcript.
4. Types: add `customType`, `display`, `details` to the raw message type, and a `kind: 'custom'` branch to `ChatMessage`.

Tests: hydrate and live cases for display true and false.

Acceptance: a producer's `pi.sendMessage` message appears in the chat when sent and after reloading the session; a `notify` appears as a toast.

Risks: custom messages that are meant for the model only (`display: false`) must never be shown. Mitigation: the rule is the `display` flag, with a test.

### F5. Bridge for Pi's generic dialogs

Goal: an extension that uses `ctx.ui.select`, `input`, `confirm` or `editor` works in PiPan instead of waiting forever.

Tasks:
1. Route. In `channels.svelte.ts` `handleUiRequest` (or a sibling `extension-dialogs.svelte.ts` that the chat store calls next), take `extension_ui_request` records with `method` `select`, `confirm`, `input` and `editor` that are not channel requests and not `PIPAN_APPROVAL::`. Queue them in the same per-session pending list as channel requests so one modal shows one thing at a time.
2. Map each to the `form` renderer without a new primitive: `select` is a select field plus Choose and Cancel actions (or the options as actions when there are four or fewer), `confirm` is a body plus Yes and No actions, `input` is a text field, `editor` is a multiline text field with `prefill` as the default. The answer goes back as `extension_ui_response` with `value`, `confirmed` or `cancelled: true`, per Pi's RPC extension UI record.
3. Provenance. Pi does not say which extension asked, so the modal says "requested by an extension", not a channel id.
4. Timeouts. If the request has `timeout` (milliseconds), Pi resolves it itself on expiry. The host must close the modal at the same time, so start a local timer and drop the request without sending a response (Pi would ignore a late one).
5. Cancel rules are the existing ones: Cancel and Esc answer `cancelled`, a session change or process exit answers `cancelled` for each outstanding request.
6. Limits and rate limiting reuse the channel queue limits (20 pending, 60 per minute).

Tests: store tests for each method and the answer record shape; a timeout test; a test that a `PIPAN_APPROVAL::` editor request is left alone; a harness scenario with a producer that calls each `ctx.ui` dialog without the shim.

Acceptance: a producer using `ctx.ui.select` and `ctx.ui.confirm` gets answers in PiPan; closing the modal with Esc returns `undefined` to the producer.

Risks: dialogs can come in bursts from a misbehaving extension. Mitigation: the existing bounded queue and rate limit. Another risk is hiding a security-relevant confirm among others. Mitigation: bridged dialogs show their title and message in full and cannot be dismissed by clicking outside.

### F6. Pending request lifecycle

Goal: a request waits for the user for as long as it takes, and the user does not lose it by clicking elsewhere.

Tasks:
0. Remove request expiry. The shim no longer times requests out and no longer writes the `pipan:x:<requestId>` widget; `timeoutMs` is dropped from the producer request options. The host removes the expiry handling for that widget in `channels.svelte.ts` and `codec.ts`. A request now ends by an answer, Cancel or Esc, a session change, or the process exiting. Remove `/demo-timeout` from the reference producer, the harness and the integration test, and update spec sections 3 and 8, the guide, the authoring guide and the producer README. Keep a producer-side rule in the guide: a producer that cannot wait forever must race its own timer against the request and decide what to do, since PiPan will not cancel it.

1. Guard. When the user starts a session switch, new chat, or stop while the pending queue is non-empty, show a confirm ("1 request is waiting. Switching cancels it.") with Switch and Stay. The existing cancel behavior stays for the confirmed case.
2. Because the request modal is blocking, the guard only matters for a switch started by code (a keyboard shortcut or an extension intent). Check which entry points can run while a modal is open, and cover those.
3. Update section 6 of the first plan and spec section 8 to say the unviewed-session badge is not needed because only the viewed session can have a pending request.

Tests: a store test that a guarded switch leaves requests pending until confirmed; the harness no longer expects a timeout answer, and a request stays pending past the old 30 minute limit (use a fake clock in the store test).

Acceptance: with a request pending, an attempted switch asks first, and Stay keeps the modal.

Risks: a forgotten ask blocks its producer until the user cancels or switches sessions. Mitigation: the request modal is always dismissible with Cancel and Esc, and the guide tells producers to treat `cancelled` as a normal answer. The guard may reduce to documentation if the modal makes every other entry point unreachable.

### F7. Type distribution for third-party producers

Goal: authors do not copy interfaces by hand from a page.

Tasks:
1. Generate a JSON Schema from `src/lib/channels/types.ts` for the three request kinds and the graph state, with a package script (`pnpm channels:schema`) and a drift check in the tests. Publish the schema file under `docs/public/` so the docs site serves it.
2. Link the schema and the types from the guide. The types also stay in the guide. No npm package is published.

Tests: the schema validates the reference producer's payloads and rejects malformed ones.

Acceptance: the schema file is generated, checked in, and validates every payload the reference producer emits.

Risks: a new dev dependency (a schema generator). Mitigation: dev only.

### F8. Docs sweep and manual run

Goal: everything built is documented, and the unverified paths are exercised in the app.

Tasks:
1. Guide changes for F1 to F7, plus a note in the authoring guide that a project-local producer needs the project to be trusted (`/trust`, or an entry in `~/.pi/agent/trust.json`), and that user-level extensions and packages are not affected.
2. Update `AGENTS.md`: the migration (v6 position key), new commands if any, the dialog bridge, the new chat message kind.
3. Manual run in `pnpm tauri dev` with the reference producer, covering what has not been exercised in the real app: the debug view, graph drag persistence across a restart, every `/demo-*` command, the dialog bridge with a producer that calls `ctx.ui` directly, custom messages after a reload, and a session switch with a pending request.

Acceptance: the checklist in section 7 is complete and recorded.

## 5. Branching and commits

Same rules as the first plan: one local branch, a commit per milestone after the milestone's tests and the Rule 4 checks pass (`pnpm check`, `pnpm test`, `pnpm test:channel`, `cargo test`, `cargo clippy`, `cargo fmt --check`), conventional prefixes, no emoji, and a grep for internal names before each commit. No PRs until the whole branch has been reviewed and the split decided.

## 6. Decisions needed

1. **F1, migration approach.** Decided: add v6 that rebuilds the table. v5 stays as shipped on the branch.
2. **F1, duplicate graph actions.** Decided: drop the later duplicate and add a notice. The first action with an id wins.
3. **F3, auto-open.** Decided: once per session and topic for every graph, with no protocol change.
4. **F3, panel cap.** Decided: at most 4 panels are opened automatically per session. Further topics stay available in the "+" picker.
5. **F4, notifications.** Decided: toast only, plus the per-session list in the debug view. Nothing is added to the transcript. Error toasts stay until dismissed.
6. **F4, handled-command record.** Decided: not recorded. A prompt that an extension command handles is not stored by Pi, and the history after a reload matches Pi's own behavior. Revisit with a `session_commands` table if it becomes a problem.
7. **F5, scope.** Decided: the dialog bridge stays on this branch as its own milestone and commit. The PR split can still make it a separate PR.
8. **F5, provenance wording.** Decided: bridged dialogs say "Requested by an extension". PiPan does not guess which extension asked.
9. **F7, package.** Decided: the JSON Schema on the docs site only. Revisit an npm package of types once the protocol has real users.

Carried over from the first plan:

10. **Request expiry.** Decided: no expiry. The shim no longer times requests out, and a request waits until it is answered, cancelled, or the session ends. Implemented in F6 task 0.
11. **Abort does not cancel pending requests.** Decided: follow the spec (section 8). Stop ends generation only; a request is ended by an answer, Cancel, a session change, or the process exiting. The first plan's section 3.2 is superseded.
12. **Markdown sanitizer.** Decided: DOMPurify over `marked` output, replacing the hand-written sanitizer. Implemented in F1 task 5.
13. **Chunk size unit.** Decided: keep 65,536 UTF-16 code units per chunk. The guide says so. Pi's RPC has no stated line size limit.

## 7. Verification matrix

| Layer | Tool |
|---|---|
| Key and validator changes | vitest, `cargo test` |
| Form validation with `requiredFor` | vitest, the harness |
| Graph auto-open and attention | vitest for the store logic, manual run for the panel and tab dot |
| Custom messages and notifications | vitest for history and live paths, manual run |
| Dialog bridge | vitest, a harness scenario without the shim, manual run |
| Switch guard | vitest, manual run |
| Schema | schema validation test against the reference producer |
| Final | `pnpm check`, `pnpm test`, `pnpm test:channel`, `cargo test`, `cargo clippy`, `cargo fmt --check`, `pnpm docs:build` |

Manual checklist (F8): debug view; drag a node, restart PiPan, confirm the position returns; each `/demo-*` command; a producer calling `ctx.ui.select`, `confirm`, `input` and `editor` directly; a `display: true` message before and after reloading the session; a `notify`; a switch attempt with a request pending; a project-local producer in an untrusted and a trusted folder.
