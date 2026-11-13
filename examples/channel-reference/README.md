# Channel reference producer

A synthetic Pi extension that exercises every part of the PiPan extension channel. It is the example in [Writing a channel producer](../../docs/guide/channels-authoring.md). The protocol is in [Channels](../../docs/guide/channels.md).

Copy `producer.ts` to `~/.pi/agent/extensions/` (or `.pi/extensions/` in a project), then start a session in PiPan. PiPan loads its shim into the session, so the commands below open native UI.

| Command | What it does |
|---|---|
| `/demo-hello` | Handshake with the shim |
| `/demo-graph [clear]` | Publish a graph that advances on a timer and asks for approval at the Approve step (a form request, plus a graph action on the waiting node), or clear it |
| `/demo-state-big <bytes>` | Publish a state of about N bytes, for chunking |
| `/demo-form [pick\|checklist\|edit\|answer\|body]` | Ask a form question |
| `/demo-review-diff [large]` | Ask for a diff review; `large` is 50 files of 1,000 lines |
| `/demo-review-doc [multi]` | Ask for a document review; `multi` adds a second document and an earlier comment |
| `/demo-unknown` | Send a request kind the host does not know |
| `/demo-timeout <ms>` | Ask a question nobody answers, to see it time out |
| `/demo-log [entry\|message]` | Write a log entry or message |
| `/demo-say` | Add one message that is shown in the chat and one that only the model sees |
| `/demo-notify [info\|warning\|error]` | Send a notification, shown as a toast |

Each command reports its outcome in a notification that starts with `demo-result:`.

The channel id is `acme.demo`. The companion plugin in [`../channel-plugin`](../channel-plugin) uses the same id.

`pnpm test:channel` runs the producer against a real `pi --mode rpc` process with no PiPan UI.
