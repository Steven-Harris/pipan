# Writing a channel producer

A producer is a Pi extension that uses `pi.events` to publish state, ask questions and write log entries. PiPan renders them. This page builds up the reference producer in `examples/channel-reference/producer.ts`. The wire format and schemas are in the [Channels reference](./channels).

You need Pi 1.5 or later. A producer works without PiPan: every call is a no-op when no shim answers, so ship a normal fallback.

## 1. Find out whether PiPan is there

The shim answers `pipan:hello` with the primitives and versions the host can render.

```ts
const hello = () => {
  let reply: { protocol: number; primitives: Record<string, number> } | undefined;
  pi.events.emit("pipan:hello", { reply: (info) => (reply = info) });
  return reply;
};
const supports = (primitive: string) => (hello()?.primitives[primitive] ?? 0) >= 1;

pi.on("session_start", () => void hello());
```

No reply means no host. A reply without the primitive you need means you must use your normal UI. Call `hello` after `session_start`, and again before each use, because it is cheap.

## 2. Publish a graph

`pipan:state` publishes the latest value of a topic. The host replaces the previous value, keeps it per session, and shows a panel in the panel picker.

```ts
const CH = "acme.demo"; // your channel id: dotted, lowercase

const state = (topic: string, primitive: string, payload: unknown) =>
  pi.events.emit("pipan:state", { ch: CH, topic, primitive, payload });

state("graph", "graph", {
  id: "run-1",
  title: "Deploy",
  direction: "LR",
  nodes: [
    { id: "build", label: "Build", status: "done" },
    { id: "ship", label: "Ship", status: "active", actions: [{ id: "pause", label: "Pause" }] },
  ],
  edges: [{ from: "build", to: "ship", taken: 1 }],
  active: ["ship"],
});

state("graph", "graph", null); // clear
```

Publish on each status change. The host applies at most 10 updates per second. `graph.id`, with the channel and topic, is the key under which users' dragged node positions are saved, so keep it stable for one logical run.

## 3. Ask a question

`pipan:request` shows a modal and resolves `reply` with the answer, or with `undefined` when the user cancels, the request times out or the session ends. Always handle `undefined`.

```ts
const request = (kind: string, payload: unknown, timeoutMs?: number) =>
  new Promise<unknown>((resolve) =>
    pi.events.emit("pipan:request", { ch: CH, kind, payload, timeoutMs, reply: resolve }),
  );

const answer = await request("form", {
  title: "Deploy target",
  body: "Choose where the build goes.",
  fields: [
    { id: "target", type: "select", label: "Target", required: true,
      options: [{ value: "staging", label: "Staging" }, { value: "production", label: "Production" }] },
  ],
  actions: [
    { id: "go", label: "Deploy", style: "primary" },
    { id: "skip", label: "Skip", skipValidation: true },
  ],
});
// { action: "go", values: { target: "staging" } }
```

The `form` primitive covers a pick-one question, a checklist, free text and an edit with a prefill. To accept either a fixed choice or a typed answer, add a text field with `required: true, requiredFor: ["submit"]` and an action `submit`: the choice actions then send an empty text without an error. The reference producer's `/demo-form` takes `pick`, `checklist`, `edit`, `answer` or `body` to show each shape.

Validate the answer yourself. The host enforces required fields and `min`/`max`, but only the producer knows what is acceptable.

Requests time out after 30 minutes by default. Pass `timeoutMs` to change that.

## 4. Review a diff or a document

`review` uses the same call with `kind: "review"`. The subject is a diff or a list of documents, and `comments` carries threads from earlier rounds.

```ts
const answer = await request("review", {
  title: "Review the changes",
  subject: { type: "diff", label: "Changes against main", files },
  comments: earlierComments,
  choices: [
    { id: "approve", label: "Approve", style: "primary" },
    { id: "changes", label: "Request changes", requiresComment: true },
  ],
});
// { choice, comments, touched, note? }
```

The user comments on lines or ranges, and can edit, reopen and dismiss earlier comments. The answer returns the full comment list. `touched` lists the ids the user added, edited, reopened or dismissed in this round. New comments have temporary ids (`tmp-1`, `tmp-2`); assign permanent ones when you store them.

Anchoring is yours. When the subject changes between rounds, move comments, set `outdated` on those that no longer fit, and decide what to send to the model.

For documents, use `subject: { type: "document", documents: [{ path, text }] }`. Markdown documents are split into blocks, one per paragraph, heading or list item, and a comment anchors to the source lines of a block. Set `format: "text"` for one block per line.

## 5. Receive intents

When the user presses a graph action or a plugin sends an intent, the shim emits `pipan:intent` on the event bus.

```ts
pi.events.on("pipan:intent", (data) => {
  if (data.ch !== CH) return;
  if (data.name === "graph.action") {
    // data.payload: { graphId, nodeId?, actionId }
  }
});
```

Intents arrive outside any command, so keep a reference to the latest context if you need to notify the user.

## 6. Write to the log

`pipan:log` stores a durable entry in the session file. With `as: "entry"` it uses `pi.appendEntry` and the model never sees it. With `as: "message"` it uses `pi.sendMessage`, so the model can see the text in `content`.

```ts
pi.events.emit("pipan:log", { ch: CH, name: "note", as: "message", payload: { n: 1 }, content: "A note for the model." });
```

The host reloads the log when it attaches to a session, so log entries survive a restart of PiPan. A plugin can draw messages with `messageRenderers` (below).

## 7. Try it

The harness drives the reference producer through a real Pi process. It needs `pi` on the path and runs offline.

```bash
pnpm test:channel
```

To use it in the app, copy the producer to `~/.pi/agent/extensions/` (or `.pi/extensions/` in a project), start a session in PiPan, and run `/demo-graph`, `/demo-form pick`, `/demo-review-diff` or `/demo-review-doc multi`. Each command reports its result in a notification that starts with `demo-result:`.

## 8. Custom UI from a plugin

When the built-in primitives are not enough, ship a [PiPan plugin](./extensions) whose id is your channel id. The plugin reaches the channel through `window.pipan.channel(id)`.

```js
const channel = window.pipan.channel("acme.demo");

// The latest value for the viewed session, now and on every change. null means nothing is published.
const stop = channel.subscribe("graph", (snapshot) => { /* snapshot.data */ });

// Take over one request kind from this channel.
channel.onRequest("pick-file", (container, request) => {
  container.textContent = JSON.stringify(request.payload);
  // request.answer({ path: "a.txt" })  or  request.cancel()
});

// Send an intent to the producer.
await channel.intent("refresh", { reason: "button" });
```

A plugin registration can also draw log messages in the chat transcript, keyed by the log `name`:

```js
window.pipan.registerExtension({
  id: "acme.demo",
  messageRenderers: {
    note: (message, container) => { container.textContent = String(message.content); },
  },
});
```

A manifest can fix the placement and title of a graph panel, and list it in the panel picker before the producer has published anything:

```json
{ "contributes": { "channels": [{ "topic": "graph", "primitive": "graph", "title": "Deploy", "icon": "network" }] } }
```

`examples/channel-plugin/` is a complete plugin for the reference producer.

## Limits

The host drops a message that breaks a limit and shows a producer error. Limits are 8 MiB per state or request, 500 nodes per graph (extra nodes are cut with a notice), 2000 edges, 200 files and 50,000 diff lines per review, and 1000 comments. Pending requests are bounded per session and rate limited.
