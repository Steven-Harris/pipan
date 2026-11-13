#!/usr/bin/env node
/**
 * Channel harness: runs a real `pi --mode rpc --offline --no-session` with the PiPan shim and the
 * reference producer, drives the producer's commands and asserts on the emitted RPC stream.
 * Requires `pi` on PATH. Run with `pnpm test:channel`.
 */
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const shim = join(root, "src-tauri/resources/pipan-channel-shim.ts");
const producer = join(root, "examples/channel-reference/producer.ts");
const CH = "acme.demo";

const version = spawnSync("pi", ["--version"], { encoding: "utf8" });
if (version.error || version.status !== 0) {
	console.error("channel-harness: `pi` was not found on PATH");
	process.exit(2);
}
console.log(`pi version: ${(version.stdout || version.stderr).trim()}`);

const cwd = mkdtempSync(join(tmpdir(), "pipan-channel-"));
const pi = spawn("pi", ["--mode", "rpc", "--offline", "--no-session", "--no-extensions", "-e", shim, "-e", producer], {
	cwd,
	stdio: ["pipe", "pipe", "pipe"],
});
let stderr = "";
pi.stderr.on("data", (chunk) => (stderr += chunk));

/** Every parsed stdout line, in order. */
const events = [];
const waiters = new Set();
createInterface({ input: pi.stdout }).on("line", (line) => {
	let event;
	try {
		event = JSON.parse(line);
	} catch {
		return;
	}
	events.push(event);
	// Channel requests are editor dialogs titled pipan:r:...; the scenario's responder answers them.
	// Any other dialog goes to the dialog responder.
	if (event.type === "extension_ui_request" && ["select", "confirm", "input", "editor"].includes(event.method)) {
		if (event.method === "editor" && event.title?.startsWith("pipan:r:")) responder(event);
		else dialogResponder(event);
	}
	for (const check of [...waiters]) check();
});

const send = (command) => pi.stdin.write(`${JSON.stringify(command)}\n`);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function waitFor(predicate, what, timeoutMs = 8000) {
	return new Promise((resolveWait, reject) => {
		const timer = setTimeout(() => {
			waiters.delete(check);
			reject(new Error(`timed out waiting for ${what}`));
		}, timeoutMs);
		function check() {
			const found = predicate();
			if (!found) return;
			clearTimeout(timer);
			waiters.delete(check);
			resolveWait(found);
		}
		waiters.add(check);
		check();
	});
}

let nextId = 0;
/** Sends a prompt and resolves with its response once Pi has handled it. */
async function prompt(message) {
	const id = `p${++nextId}`;
	send({ type: "prompt", id, message });
	return waitFor(() => events.find((e) => e.type === "response" && e.id === id), `response to ${message}`);
}

const widgets = (key) => events.filter((e) => e.type === "extension_ui_request" && e.method === "setWidget" && e.widgetKey === key);
const results = () =>
	events
		.filter((e) => e.type === "extension_ui_request" && e.method === "notify" && e.message?.startsWith("demo-result:"))
		.map((e) => JSON.parse(e.message.slice("demo-result:".length)));

/** Runs a command and resolves with the result it reports once `count` more results exist. */
async function runCommand(message, expectResults = 1) {
	const before = results().length;
	const response = await prompt(message);
	const got = await waitFor(() => results().length >= before + expectResults && results().slice(before), `result of ${message}`);
	return { response, results: got };
}

const decodeWidget = (event) => JSON.parse(event.widgetLines.join(""));
const envelope = (ch, payload) => ({ v: 1, ch, payload });

let responder = () => {};
let dialogResponder = () => {};
const answerWith = (payloadFor) => (event) => {
	const request = JSON.parse(event.prefill);
	const value = payloadFor(request.payload, event);
	send(
		value === undefined
			? { type: "extension_ui_response", id: event.id, cancelled: true }
			: {
					type: "extension_ui_response",
					id: event.id,
					value: JSON.stringify(envelope(request.ch, { requestId: request.payload.requestId, payload: value })),
				},
	);
};

function assert(condition, message) {
	if (!condition) throw new Error(`assertion failed: ${message}`);
}
const eq = (actual, expected, message) =>
	assert(JSON.stringify(actual) === JSON.stringify(expected), `${message}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);

const tests = [];
const test = (name, fn) => tests.push({ name, fn });

test("shim announces itself at startup", async () => {
	const announce = await waitFor(() => widgets("pipan:shim")[0], "pipan:shim announce");
	const info = JSON.parse(announce.widgetLines[0]);
	eq(info.protocol, 1, "protocol");
	eq(info.host, "pipan", "host");
	assert(info.primitives.graph >= 1 && info.primitives.review >= 1 && info.primitives.form >= 1, "primitives advertised");
});

test("hello: producer receives host info", async () => {
	const { results: [r] } = await runCommand("/demo-hello");
	eq(r.host.protocol, 1, "protocol in reply");
	assert(r.host.primitives.form >= 1, "form primitive in reply");
});

test("state: replace and clear", async () => {
	const key = `pipan:s:${CH}:graph`;
	await runCommand("/demo-graph");
	const first = await waitFor(() => widgets(key)[0], "first graph state");
	const firstEnv = decodeWidget(first);
	eq(firstEnv.v, 1, "envelope version");
	eq(firstEnv.ch, CH, "envelope channel");
	eq(firstEnv.payload.primitive, "graph", "primitive");
	eq(firstEnv.payload.data.nodes[0].status, "active", "first node active");
	// The reference graph advances on a timer, so a later snapshot replaces the first.
	const second = await waitFor(() => widgets(key).find((w) => w.widgetLines && decodeWidget(w).seq > firstEnv.seq), "replacement state");
	assert(decodeWidget(second).payload.data.nodes[0].status === "done", "replacement shows progress");
	await runCommand("/demo-graph clear");
	const cleared = await waitFor(() => widgets(key).find((w) => !w.widgetLines), "cleared widget");
	assert(cleared.widgetLines === undefined, "clear omits widgetLines");
});

test("state: chunked 3 MB payload reassembles", async () => {
	const key = `pipan:s:${CH}:big`;
	await runCommand("/demo-state-big 3000000");
	const event = await waitFor(() => widgets(key).find((w) => w.widgetLines && w.widgetLines.length > 1), "chunked state");
	assert(event.widgetLines.length >= 46, `expected at least 46 chunks, got ${event.widgetLines.length}`);
	assert(event.widgetLines.every((line) => line.length <= 64 * 1024), "every chunk within 64 KiB");
	eq(decodeWidget(event).payload.data.blob.length, 3_000_000, "reassembled blob length");
});

test("request: answered", async () => {
	responder = answerWith((payload) => {
		eq(payload.payload.title, "Deploy target", "request title");
		return { action: "go", values: { target: "staging" } };
	});
	const { results: [r] } = await runCommand("/demo-form");
	eq(r.answer, { action: "go", values: { target: "staging" } }, "answer");
	const request = events.findLast((e) => e.method === "editor");
	eq(request.title, `pipan:r:${CH}:form`, "editor title");
	assert(typeof JSON.parse(request.prefill).payload.requestId === "string", "requestId present");
});

test("request: requiredFor reaches the host and a choice answer carries an empty text", async () => {
	let seen;
	responder = answerWith((payload) => {
		seen = payload.payload;
		return { action: "staging", values: { text: "" } };
	});
	const { results: [r] } = await runCommand("/demo-form answer");
	eq(seen.fields[0].requiredFor, ["submit"], "requiredFor is passed through unchanged");
	eq(r.answer, { action: "staging", values: { text: "" } }, "answer");
});

test("request: cancelled", async () => {
	responder = answerWith(() => undefined);
	const { results: [r] } = await runCommand("/demo-form");
	eq(r.answer, null, "cancelled request resolves with no answer");
});

test("request: a reply with a wrong requestId is treated as cancelled", async () => {
	responder = (event) => {
		const request = JSON.parse(event.prefill);
		send({
			type: "extension_ui_response",
			id: event.id,
			value: JSON.stringify(envelope(request.ch, { requestId: "someone-else", payload: { action: "go" } })),
		});
	};
	const { results: [r] } = await runCommand("/demo-form");
	eq(r.answer, null, "mismatched requestId");
});

test("request: shim timeout resolves the producer and signals expiry", async () => {
	let timedOut;
	responder = (event) => {
		timedOut = event; // never answered; the shim must give up on its own
	};
	const { results: [r] } = await runCommand("/demo-timeout 300");
	eq(r.answer, null, "timed out request resolves with no answer");
	const requestId = JSON.parse(timedOut.prefill).payload.requestId;
	const expiry = await waitFor(() => widgets(`pipan:x:${requestId}`).find((w) => w.widgetLines), "expiry signal");
	eq(JSON.parse(expiry.widgetLines[0]).payload, { requestId, reason: "timeout" }, "expiry payload");
	// A late cancel from the host (which cannot know about the timeout) must be harmless.
	send({ type: "extension_ui_response", id: timedOut.id, cancelled: true });
	responder = answerWith(() => ({ action: "go", values: {} }));
	const { results: [again] } = await runCommand("/demo-form");
	eq(again.answer, { action: "go", values: {} }, "shim still serves requests after a timeout");
});

test("resync: re-emits cached state and announces", async () => {
	responder = () => {};
	const key = `pipan:s:${CH}:big`;
	await runCommand("/demo-state-big 100");
	const original = await waitFor(() => widgets(key).findLast((w) => w.widgetLines?.length === 1), "small state");
	const stateBefore = widgets(key).length;
	const announceBefore = widgets("pipan:shim").length;
	const response = await prompt(`/pipan-intent ${JSON.stringify({ v: 1, ch: "_shim", name: "resync", payload: {} })}`);
	eq(response.data?.disposition, "handled", "resync is a handled command");
	await waitFor(() => widgets(key).length > stateBefore, "re-emitted state");
	await waitFor(() => widgets("pipan:shim").length > announceBefore, "re-announce");
	const replayed = widgets(key).at(-1);
	eq(replayed.widgetLines, original.widgetLines, "replayed lines match the cached state");
	// The graph topic was cleared earlier, so it must not come back.
	const graphKey = `pipan:s:${CH}:graph`;
	assert(widgets(graphKey).at(-1).widgetLines === undefined, "cleared topic is not resurrected");
});

test("dialogs: plain Pi dialogs reach the host as extension_ui_request records and take Pi's response shape", async () => {
	const seen = [];
	dialogResponder = (event) => {
		seen.push(event);
		const reply =
			event.method === "select" ? { value: event.options[1] }
			: event.method === "confirm" ? { confirmed: true }
			: event.method === "input" ? { value: "typed" }
			: { value: "edited" };
		send({ type: "extension_ui_response", id: event.id, ...reply });
	};
	const answers = {};
	for (const method of ["select", "confirm", "input", "editor"]) {
		const { results: [r] } = await runCommand(`/demo-dialog ${method}`);
		answers[method] = r.answer;
	}
	eq(answers, { select: "Block", confirm: true, input: "typed", editor: "edited" }, "values reach the extension");
	eq(seen.map((e) => e.method), ["select", "confirm", "input", "editor"], "one request per dialog");
	eq(seen[0].options, ["Allow", "Block"], "options are sent");
	dialogResponder = (event) => send({ type: "extension_ui_response", id: event.id, cancelled: true });
	const { results: [cancelled] } = await runCommand("/demo-dialog confirm");
	eq(cancelled.answer, false, "a cancelled confirm is false");
	const { results: [none] } = await runCommand("/demo-dialog select");
	eq(none.answer, null, "a cancelled select is undefined");
	dialogResponder = () => {};
});

test("graph approval: the waiting node comes with a form request, and approving advances the run", async () => {
	const key = `pipan:s:${CH}:graph`;
	let asked;
	responder = answerWith((payload) => {
		asked = payload;
		return { action: "approve", values: {} };
	});
	const from = widgets(key).length; // earlier tests left states of the same graph
	await runCommand("/demo-graph");
	const done = await waitFor(
		() => widgets(key).slice(from).find((w) => w.widgetLines && decodeWidget(w).payload.data.nodes.find((n) => n.id === "approve")?.status === "done"),
		"approval node done",
		15000,
	);
	eq(asked.payload.title, "Approve the release?", "approval is asked as a form request");
	const waiting = widgets(key).slice(from).find((w) => w.widgetLines && decodeWidget(w).payload.data.nodes.find((n) => n.id === "approve")?.status === "waiting");
	assert(waiting, "the approval node was published as waiting");
	eq(decodeWidget(waiting).payload.data.nodes.find((n) => n.id === "approve").actions[0].id, "approve", "graph action stays available");
	assert(decodeWidget(done).payload.data.nodes.find((n) => n.id === "ship").status === "active", "run moved on to ship");
	await runCommand("/demo-graph clear");
});

test("intent: reaches the producer", async () => {
	const before = results().length;
	const response = await prompt(
		`/pipan-intent ${JSON.stringify({ v: 1, ch: CH, name: "graph.action", payload: { graphId: "demo-run-1", actionId: "approve" } })}`,
	);
	eq(response.data?.disposition, "handled", "intent is a handled command");
	const [r] = await waitFor(() => results().length > before && results().slice(before), "intent result");
	eq(r.intent, { name: "graph.action", payload: { graphId: "demo-run-1", actionId: "approve" } }, "intent delivered");
	const transcript = (await rpc({ type: "get_messages" })).data.messages;
	assert(!JSON.stringify(transcript).includes("pipan-intent"), "intent left no trace in the transcript");
});

test("intent: other channels and malformed input are not delivered to this producer", async () => {
	const before = results().length;
	await prompt(`/pipan-intent ${JSON.stringify({ v: 1, ch: "other.channel", name: "x", payload: {} })}`);
	await prompt("/pipan-intent {not json");
	await prompt(`/pipan-intent ${JSON.stringify({ v: 2, ch: CH, name: "x", payload: {} })}`);
	await sleep(300);
	eq(results().length, before, "no result for foreign or invalid intents");
});

test("messages: a displayed and a hidden custom message reach the stream and the history", async () => {
	const before = events.length;
	await runCommand("/demo-say");
	const ended = events.slice(before).filter((e) => e.type === "message_end" && e.message?.role === "custom");
	eq(ended.map((e) => [e.message.customType, e.message.display]), [["acme.demo-note", true], ["acme.demo-hidden", false]], "custom messages with their display flag");
	const history = (await rpc({ type: "get_messages" })).data.messages.filter((m) => m.role === "custom" && m.customType.startsWith("acme.demo-"));
	eq(history.map((m) => m.display), [true, false], "history keeps the display flag");
});

test("notify: level reaches the stream", async () => {
	const before = events.length;
	await prompt("/demo-notify warning");
	const found = await waitFor(() => events.slice(before).find((e) => e.method === "notify" && e.message === "Synthetic warning notification"), "notification");
	eq(found.notifyType, "warning", "level");
});

test("log: entry and message reach the stream", async () => {
	await runCommand("/demo-log entry");
	const entry = await waitFor(
		() => events.find((e) => e.type === "entry_appended" && JSON.stringify(e).includes(`pipan:l:${CH}:note`)),
		"entry_appended",
	);
	assert(JSON.stringify(entry).includes("synthetic note"), "entry data intact");
	await runCommand("/demo-log message");
	await waitFor(
		() => events.find((e) => e.type === "message_end" && e.message?.customType === `pipan:l:${CH}:note`),
		"custom message",
	);
});

test("double session_start: handlers are not duplicated", async () => {
	const key = `pipan:s:${CH}:big`;
	const announceBefore = widgets("pipan:shim").length;
	const session = await rpc({ type: "new_session" });
	assert(session.success, "new_session succeeded");
	await waitFor(() => widgets("pipan:shim").length > announceBefore, "announce after new_session");
	await sleep(500); // let any second session_start land
	console.log(`       announces after new_session: ${widgets("pipan:shim").length - announceBefore}`);
	const stateBefore = widgets(key).length;
	const resultsBefore = results().length;
	// One hello reply and exactly one widget per publish, however many session_start events fired.
	await runCommand("/demo-hello");
	await runCommand("/demo-state-big 50");
	await sleep(300);
	eq(results().length - resultsBefore, 2, "one result per command");
	const fresh = widgets(key).slice(stateBefore).filter((w) => w.widgetLines);
	eq(fresh.length, 1, "exactly one widget emission for one publish");
	eq(decodeWidget(fresh[0]).payload.data.blob.length, 50, "fresh state content");
});

/** Sends a non-prompt command and resolves with its response. */
async function rpc(command) {
	const id = `c${++nextId}`;
	send({ ...command, id });
	return waitFor(() => events.find((e) => e.type === "response" && e.id === id), `response to ${command.type}`);
}

let failed = 0;
try {
	await sleep(1500); // let Pi finish startup
	for (const { name, fn } of tests) {
		try {
			await fn();
			console.log(`ok   - ${name}`);
		} catch (error) {
			failed += 1;
			console.log(`FAIL - ${name}\n       ${error.message}`);
		}
	}
} finally {
	pi.stdin.end();
	await sleep(500);
	pi.kill();
	rmSync(cwd, { recursive: true, force: true });
}
if (failed) {
	console.log(`\n${failed} of ${tests.length} failed`);
	if (stderr.trim()) console.log(`pi stderr:\n${stderr.slice(0, 2000)}`);
	process.exit(1);
}
console.log(`\n${tests.length} passed`);
