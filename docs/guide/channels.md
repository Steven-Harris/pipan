# Channels

Channels let a Pi extension drive native PiPan UI: a live graph panel, a diff or document review with line comments, and forms. A Pi extension publishes data or asks a question. PiPan renders it and sends the answer back. The extension needs no frontend code.

This page is the protocol reference. [Writing a channel producer](./channels-authoring) walks through the reference producer, and [Extensions](./extensions) covers the plugin side for custom UI.

Conventions: "MUST", "SHOULD" and "MAY" carry their usual meaning. Unknown object keys are ignored. Unknown enum values fall back to the documented default. Text fields are plain text unless named `markdown`, and markdown is sanitized by the host. The canonical TypeScript types are in `src/lib/channels/types.ts`. A [JSON Schema](/channels.schema.json) generated from them describes the payloads (below).

## 1. Roles

- **Producer**: any Pi extension that publishes state, issues requests, or writes log entries.
- **Shim**: the PiPan-owned Pi extension. It is the only code that touches the RPC encoding.
- **Host**: the PiPan app (Rust router plus Svelte store and UI).
- **Channel**: a producer's namespace, written `ch`. It is a dotted lowercase id such as `acme.deploy`. It matches a PiPan plugin id when the producer also ships a plugin. It is self-declared and unauthenticated.

PiPan loads the shim into the primary Pi session of every PiPan-started process. Producer payloads are untrusted data: markdown is rendered, raw HTML is shown as text, images are not loaded, and links keep only `http`, `https` and `mailto` targets. The rendered HTML is then sanitized with DOMPurify against a strict allowlist (no scripts, styles, forms, frames, `data-` or event attributes), so the rendered output contains only text formatting, lists, tables, code and links.

## 2. Producer API (Pi event bus)

Producers use `pi.events` only. Every call is a silent no-op when no shim is loaded.

| Event | Payload | Direction |
|---|---|---|
| `pipan:hello` | `{ reply(info: HostInfo): void }` | producer to shim |
| `pipan:state` | `{ ch, topic, primitive, payload \| null }` | producer to shim |
| `pipan:request` | `{ ch, kind, payload, reply(answer \| undefined): void }` | producer to shim |
| `pipan:log` | `{ ch, name, as: "entry" \| "message", payload, content?, options? }` | producer to shim |
| `pipan:intent` | `{ ch, name, payload }` | shim to producer |
| `pipan:resync` | `{ ch? }` | shim to producers (informational) |

```ts
interface HostInfo {
  protocol: 1;                    // highest protocol version the host speaks
  host: "pipan";
  primitives: Record<string, number>; // primitive name -> highest supported version, e.g. { graph: 1, review: 1, form: 1 }
}
```

- A producer calls `pipan:hello` after `session_start`. No reply means no host. A reply missing the primitive the producer needs means the producer MUST use its normal UI.
- `pipan:state` with `payload: null` clears the topic.
- `pipan:request` resolves `reply` with the answer payload, or `undefined` when cancelled or timed out.
- `pipan:log` with `as: "entry"` calls `pi.appendEntry`. With `as: "message"` it calls `pi.sendMessage({ customType, content, display, details: payload }, options)`; `content` is required for messages and `options` passes `deliverAs` and `triggerTurn` through.
- The shim handles `pipan:resync` by re-emitting its cached state. Producers receive it so they can refresh volatile state, but are not required to react.

## 3. Wire encoding

All envelopes carry a protocol version and the channel:

```ts
interface Envelope<T> { v: 1; ch: string; seq?: number; payload: T }
```

| Carrier | Encoding |
|---|---|
| State | `setWidget(widgetKey, lines)` with `widgetKey = "pipan:s:<ch>:<topic>"`. The JSON of `Envelope<{ primitive, data }>` is split into pieces of at most 65,536 characters, one piece per `widgetLines` entry; the host concatenates. Clearing sends `widgetLines` omitted. |
| Request | An `editor` dialog. `title = "pipan:r:<ch>:<kind>"`. `prefill` is the JSON of `Envelope<{ requestId, payload }>`. The reply `value` is the JSON of `Envelope<{ requestId, payload }>`. A `cancelled: true` reply means no answer. |
| Log entry | `appendEntry("pipan:l:<ch>:<name>", payload)`. |
| Log message | `sendMessage({ customType: "pipan:l:<ch>:<name>", ... })`. |
| Intent | `prompt` with `/pipan-intent <json>` where json is `{ v: 1, ch, name, payload }`. The host checks `get_commands` for `pipan-intent` before sending. |
| Shim announce | `setWidget("pipan:shim", [json of HostInfo])` on every `session_start` (idempotent; the event can fire more than once per session). The host reads this to learn that a shim is present. |

`seq` is a counter per (ch, topic) kept by the shim. The host drops a state whose `seq` is lower than the last it applied.

Limits (enforced by the host; a violation drops the message and shows a producer error in the UI): 8 MiB per state or request, 500 nodes per graph, 2000 edges, 200 files and 50,000 diff lines per review, 1000 comments.

## 4. Primitive: `graph`

A directed graph of steps, shown as a live panel. Topic convention: `graph`, but a channel MAY publish several graphs under different topics.

```ts
interface GraphState {
  id: string;                    // stable id of this graph/run; layout persistence key
  title?: string;
  subtitle?: string;
  direction?: "TB" | "LR";       // default "TB"
  nodes: GraphNode[];
  edges: GraphEdge[];
  active?: string[];             // ids of the nodes currently executing or waiting
  actions?: GraphAction[];       // graph-level buttons
  note?: string;                 // one-line status text
}

interface GraphNode {
  id: string;
  label: string;
  kind?: string;                 // open string, e.g. "script", "agent", "approval". Hint for glyph and color
  tone?: Tone;                   // overrides the host's kind-based color
  status: "pending" | "active" | "waiting" | "done" | "failed" | "skipped";
  visits?: number;               // times entered, for loops
  group?: string;                // id of a containing node, for nesting
  detail?: NodeDetail;
  actions?: GraphAction[];       // node-level buttons
}

type Tone = "neutral" | "cyan" | "amber" | "violet" | "green" | "red" | "blue";

interface NodeDetail {
  summary?: string;
  markdown?: string;
  fields?: { label: string; value: string }[];
  startedAt?: number;            // epoch ms
  endedAt?: number;
  durationMs?: number;
  costUsd?: number;
  tokens?: number;
}

interface GraphEdge {
  id?: string;
  from: string;
  to: string;
  label?: string;
  taken?: number;                // times traversed; 0 or absent means not yet
}

interface GraphAction {
  id: string;                    // sent back as intent name "graph.action" with { graphId, nodeId?, actionId }
  label: string;
  enabled?: boolean;             // default true
  confirm?: string;              // when present, the host asks for confirmation with this text first
  style?: "default" | "primary" | "danger";
}
```

Rules:
- `kind` has no fixed vocabulary. The host assigns a stable color and glyph per distinct `kind` string within a graph and uses `tone` when given. Unknown kinds render as neutral.
- A graph is replaced wholesale on every publish (no deltas in v1). Producers SHOULD publish on each status change, and MAY coalesce at up to 10 per second.
- Node positions are computed by the host (Dagre). The host persists manual positions per `(session, channel, topic, graph.id, node.id)`, so graphs from different channels or topics never share positions. Two graphs from one channel and topic that share a `graph.id` do.
- Actions are sent as intents with `name = "graph.action"`; the host never executes anything itself.
- Node count over the limit: the host shows the first 500 in order and a notice.
- Action ids MUST be unique within one list (graph-level actions, or one node's actions). The host keeps the first action with an id, drops later ones, and shows a notice.

## 5. Primitive: `review`

A request to review a diff or documents with line comments, ending in a choice. Request `kind`: `review`.

```ts
interface ReviewRequest {
  title: string;
  body?: string;                 // markdown, shown above the subject
  subject: DiffSubject | DocumentSubject;
  comments: ReviewComment[];     // threads from earlier rounds; empty on a first review
  choices: ReviewChoice[];       // at least one
  commentHint?: string;          // placeholder text for the comment box
}

interface DiffSubject {
  type: "diff";
  files: DiffFile[];
  label?: string;                // e.g. "Changes against main"
}

interface DiffFile {
  path: string;
  oldPath?: string;              // present for renames
  status: "added" | "modified" | "deleted" | "renamed" | "untracked";
  binary?: boolean;
  truncated?: boolean;
  hunks: DiffHunk[];
}

interface DiffHunk {
  header: string;                // "@@ -10,6 +10,8 @@"
  lines: DiffLine[];
}

interface DiffLine {
  kind: "add" | "del" | "ctx";
  text: string;
  oldLine?: number;
  newLine?: number;
}

interface DocumentSubject {
  type: "document";
  documents: { path: string; text: string; format?: "markdown" | "text" }[]; // "markdown" by default
}

interface ReviewComment {
  id: string;
  path: string;
  side: "old" | "new";           // documents always use "new"
  line: number;                  // first line, 1-based
  endLine?: number;              // for a range
  snippet?: string;              // the commented text at write time
  text: string;
  status: "open" | "addressed" | "declined" | "dismissed";
  outdated?: boolean;
  replies?: { author: "user" | "agent"; text: string }[];
}

interface ReviewChoice {
  id: string;
  label: string;
  description?: string;
  style?: "default" | "primary" | "danger";
  requiresComment?: boolean;     // the host disables this choice until at least one comment is open or touched
}

interface ReviewAnswer {
  choice: string;                // a ReviewChoice id
  comments: ReviewComment[];     // the full list after the user's edits
  touched: string[];             // ids of comments the user added, edited, reopened, or dismissed this round
  note?: string;                 // optional free text with the answer
}
```

Rules:
- The host owns the interaction (selecting lines, writing, editing, reopening and dismissing comments). The producer owns anchoring and meaning: moving comments when the subject changes, deciding `outdated`, and turning `touched` comments into a prompt.
- New comments get a temporary id `tmp-<n>`. The producer assigns permanent ids when it stores them.
- Both subjects share the comment model. For documents the host renders markdown with source line numbers, and a comment anchors to the source line range of the block it was written on.
- A user answer MUST include a `choice`. Closing the dialog without a choice sends a cancel (no answer).
- A review with no files or documents is a producer error. The producer SHOULD skip the request instead.
- File `path`s MUST be unique within a diff review, document `path`s within a document review, and comment `id`s within a review. A request that repeats one is rejected as a producer error, as is a repeated form field, form action, form option value or review choice id.

## 6. Primitive: `form`

A request for structured input. Request `kind`: `form`. Covers a simple pick-one-of-N question, a multi-select checklist, free text, and an edit-with-prefill, as variations on one schema.

```ts
interface FormRequest {
  title: string;
  body?: string;                 // markdown, shown above the fields (scrollable)
  fields?: FormField[];          // none means a body with actions only
  actions: FormAction[];         // at least one
}

type FormField =
  | { id: string; type: "select"; label: string; options: FormOption[]; default?: string; required?: boolean; requiredFor?: string[] }
  | { id: string; type: "multiselect"; label: string; options: FormOption[]; default?: string[]; min?: number; max?: number; requiredFor?: string[] }
  | { id: string; type: "text"; label: string; multiline?: boolean; placeholder?: string; default?: string; required?: boolean; requiredFor?: string[] };

interface FormOption { value: string; label: string; description?: string }

interface FormAction {
  id: string;
  label: string;
  description?: string;
  style?: "default" | "primary" | "danger";
  skipValidation?: boolean;      // e.g. a "Skip" action that ignores required fields
}

interface FormAnswer {
  action: string;                // a FormAction id
  values: Record<string, string | string[]>;
}
```

Rules:
- A question with fixed choices and no fields is a body plus actions. The chosen `action` is the answer.
- `text` with `multiline` and a `default` is the edit-with-prefill case.
- Required fields and `min` and `max` are enforced by the host before an action without `skipValidation` is accepted. The producer MUST still validate.
- `requiredFor` limits `required` (and a multiselect's `min`) to the listed action ids. Without it the requirement applies to every action that validates. With it, pressing any other action skips that requirement and sends the field's current value, which may be empty. `max` and the listed options always apply. Ids that match no action are ignored. A host that does not know `requiredFor` ignores it and applies `required` to every action, so the producer validates what it receives.
- A typed answer next to fixed choices is one form: a text field with `required: true` and `requiredFor: ["submit"]`, a "Submit answer" action, and one action per fixed choice.

## 7. Intents defined by the host

| `ch` | `name` | Payload | Meaning |
|---|---|---|---|
| `_shim` | `resync` | `{}` | Shim re-emits cached state for all channels, and announces itself. Sent when the host attaches to a session or switches to one. |
| any | `graph.action` | `{ graphId, nodeId?, actionId }` | The user pressed a graph action. |
| any | custom | any | Intents sent by a plugin through `window.pipan.channel(ch).intent(...)`. |

## 8. Host behavior

- State is stored per `(sessionId, ch, topic)`, last value wins, and survives panel close.
- The first state seen for a graph topic adds an entry for it to the panel picker and opens its panel once per session and topic. A panel the user closes stays closed for that session, and the host opens at most 4 panels by itself per session; further topics stay in the picker. When a node enters `waiting` and has actions, the panel selects it, unless the user picked another node in the last few seconds, and a panel that is not in view shows a dot on its tab until it is looked at. A `request` opens its surface immediately and is queued behind any pending request for the same session.
- Channel state belongs to the Pi session, not to PiPan. PiPan runs one Pi process, and switching to another session, a new chat, or another folder ends the old session's extensions: the host drops that session's state and cancels its requests, and a producer's in-memory work stops. A producer that must survive this saves its run (for example with `pi.appendEntry`) and republishes at `session_start`.
- A request has no expiry. It waits until the user answers or cancels it, the session switches, or the process exits. Pi's abort does not cancel a pending request. The host cancels every pending request for a session when the session switches, stops, or the process exits. It sends `cancelled: true` for each outstanding Pi request id.
- After a session switch or attach, the host sends `_shim.resync` and reloads the log with `get_entries`, using the last-seen entry id as `since`.
- Payloads that fail validation are dropped. The host shows "Extension `<ch>` sent an invalid `<primitive>`" with details in the stderr log pane. For a request it answers `cancelled`.
- The UI shows the channel id as "requested by `<ch>`" on every request surface.

### Pi's own dialogs

An extension that calls `ctx.ui.select`, `confirm`, `input` or `editor` needs no shim and no channel. Pi sends each call as an `extension_ui_request`, and the host shows it in the same modal and queue as channel requests, headed "Requested by an extension" because Pi does not say which extension asked.

| Pi call | Shown as | Answered with |
|---|---|---|
| `select(title, options)` | one button per option for four or fewer, otherwise a required select field | `value`: the chosen option text |
| `confirm(title, message)` | the message with Yes and No | `confirmed: true` or `false` |
| `input(title, placeholder)` | a text field | `value`: the text, which may be empty |
| `editor(title, prefill)` | a multiline text field holding the prefill | `value`: the text |

- Titles, messages and option labels are plain text. Markdown in a message is shown as typed.
- Cancel, Esc, a session change and a process exit answer `cancelled: true`, which Pi turns into `undefined` for `select`, `input` and `editor` and `false` for `confirm`.
- A `select` with no options is cancelled at once. At most 500 options are shown, with a note saying how many were left out.
- If the call has a `timeout`, Pi resolves the dialog on its own when it expires and the host closes the modal at the same moment, without sending a response.
- These dialogs count against the same limits as channel requests: 20 pending, and 60 per minute for a session.
- Plan-gate approvals for subagents are not shown here. PiPan answers them separately.

## 9. Versioning

- The envelope `v` and each primitive's version are independent. This document defines envelope `v: 1` and `graph`, `review` and `form` at version 1.
- `HostInfo.primitives` lets a producer pick the highest version both sides understand. A producer sends a primitive's payload shape for the version it negotiated; the shape is not carried in the envelope in v1 because the only version is 1.
- Adding an optional field is not a version change. Removing or retyping a field, or changing a field's meaning, bumps the primitive's version.

### JSON Schema

[`channels.schema.json`](/channels.schema.json) is generated from the TypeScript types. Validate a payload against one of its definitions: `HostInfo` (the `hello` reply), `GraphState` (a `graph` state), `FormRequest` and `FormAnswer`, `ReviewRequest` and `ReviewAnswer`. With Ajv, add the schema and compile `{ $ref: "channels#/definitions/GraphState" }`.

The schema describes shape only. It allows unknown keys, because the host ignores them. Enum values are listed as the types define them, while a host falls back to the documented default for an unknown value. The host also enforces what a schema cannot say: the limits in section 8, unique ids and paths within a list, and the rules for line numbers in a review. Run `pnpm channels:schema` after changing `types.ts`; a test fails when the checked-in file is out of date.

## 10. Not in version 1

- `review`: user replies on a thread. Producers may store only one reply per comment, so the host does not offer replies. `touched` includes comments the user dismissed.
- `review`: markdown in comment text. Comments render as plain text.
- `graph`: nesting by `group`. Nodes in a group are drawn flat and labelled with their group.
- Delta updates for large graphs (graphs are replaced wholesale at up to 10 Hz).
- Lazy file bodies for large diffs.
- `form` fields for `confirm` and `number`.
- A `progress` primitive for long-running producers that do not need a graph.
- Authentication for the channel id. Pi does not say which extension issued a call, so a channel id is self-declared and the UI shows it as "requested by `<ch>`".
