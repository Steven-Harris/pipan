/**
 * PiPan extension channel shim.
 *
 * Loaded into the primary Pi session with `pi --extension <this file>`. Other Pi extensions
 * talk to it over the shared in-process event bus (`pi.events`); the shim encodes that traffic
 * onto the Pi RPC carriers PiPan can see. Wire format: docs/plans/extension-channel-spec.md,
 * sections 2 and 3.
 *
 *   state   : setWidget("pipan:s:<ch>:<topic>", <envelope JSON in 64 KiB pieces>)
 *   request : editor(title "pipan:r:<ch>:<kind>", prefill <envelope JSON>)
 *   expiry  : setWidget("pipan:x:<requestId>", [<json>]) when a request times out
 *   log     : appendEntry / sendMessage with customType "pipan:l:<ch>:<name>"
 *   intent  : the hidden "/pipan-intent <json>" command
 *   announce: setWidget("pipan:shim", [<HostInfo json>]) on every session_start
 *
 * This file intentionally has no imports (types are structural) so it loads from anywhere.
 */

const PROTOCOL = 1;
const CHUNK_SIZE = 64 * 1024;
const DEFAULT_REQUEST_TIMEOUT_MS = 30 * 60 * 1000;

/** Highest version of each primitive the host renders. Advertised in the handshake. */
const PRIMITIVES: Record<string, number> = { graph: 1, review: 1, form: 1 };

interface HostInfo {
	protocol: 1;
	host: "pipan";
	primitives: Record<string, number>;
}

interface Ui {
	setWidget(key: string, lines: string[] | undefined): void;
	editor(title: string, prefill?: string): Promise<string | undefined>;
}

interface Ctx {
	ui: Ui;
}

interface Pi {
	on(event: string, handler: (event: any, ctx: Ctx) => unknown): void;
	events: {
		on(channel: string, handler: (data: any) => unknown): unknown;
		emit(channel: string, data?: unknown): void;
	};
	registerCommand(
		name: string,
		options: { description?: string; handler: (args: string, ctx: Ctx) => unknown },
	): void;
	appendEntry(customType: string, data?: unknown): void;
	sendMessage(message: Record<string, unknown>, options?: Record<string, unknown>): void;
}

interface CachedState {
	key: string;
	lines: string[];
}

/** Split a string into pieces of at most `size` UTF-16 units without cutting a surrogate pair. */
function chunk(text: string, size: number): string[] {
	const pieces: string[] = [];
	let start = 0;
	while (start < text.length) {
		let end = Math.min(start + size, text.length);
		if (end < text.length) {
			const last = text.charCodeAt(end - 1);
			if (last >= 0xd800 && last <= 0xdbff) end -= 1;
		}
		pieces.push(text.slice(start, end));
		start = end;
	}
	return pieces;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

export default function (pi: Pi) {
	/** Replaced on every session_start; undefined between shutdown and the next start. */
	let ctx: Ctx | undefined;
	/** Latest encoded state per widget key. */
	const cache = new Map<string, CachedState>();
	/** seq counter per "<ch>\u0000<topic>". Survives clears so the host never sees it go backwards. */
	const seqs = new Map<string, number>();
	/** Settlers for in-flight requests, so shutdown can cancel them. */
	const pending = new Set<() => void>();
	let requestCounter = 0;

	const hostInfo = (): HostInfo => ({ protocol: PROTOCOL, host: "pipan", primitives: { ...PRIMITIVES } });

	const announce = () => ctx?.ui.setWidget("pipan:shim", [JSON.stringify(hostInfo())]);

	const emitAllState = () => {
		for (const { key, lines } of cache.values()) ctx?.ui.setWidget(key, lines);
	};

	pi.on("session_start", (_event, c) => {
		// Fires more than once per session on new_session. Handlers are registered once at load,
		// so this only refreshes the context and republishes; it is safe to repeat.
		ctx = c;
		announce();
		emitAllState();
	});

	pi.on("session_shutdown", () => {
		ctx = undefined;
		for (const settle of [...pending]) settle();
	});

	pi.events.on("pipan:hello", (data) => {
		if (isRecord(data) && typeof data.reply === "function") data.reply(hostInfo());
	});

	pi.events.on("pipan:state", (data) => {
		if (!isRecord(data) || typeof data.ch !== "string" || typeof data.topic !== "string") return;
		const { ch, topic } = data;
		const key = `pipan:s:${ch}:${topic}`;
		const counterKey = `${ch}\u0000${topic}`;
		const seq = (seqs.get(counterKey) ?? 0) + 1;
		seqs.set(counterKey, seq);

		if (data.payload === null || data.payload === undefined) {
			cache.delete(key);
			ctx?.ui.setWidget(key, undefined);
			return;
		}
		if (typeof data.primitive !== "string") return;
		const envelope = { v: PROTOCOL, ch, seq, payload: { primitive: data.primitive, data: data.payload } };
		const lines = chunk(JSON.stringify(envelope), CHUNK_SIZE);
		cache.set(key, { key, lines });
		ctx?.ui.setWidget(key, lines);
	});

	pi.events.on("pipan:request", (data) => {
		if (!isRecord(data) || typeof data.reply !== "function") return;
		const reply = data.reply as (answer: unknown) => void;
		const { ch, kind } = data;
		const active = ctx;
		if (!active || typeof ch !== "string" || typeof kind !== "string") {
			reply(undefined);
			return;
		}

		const requestId = `r${++requestCounter}-${Math.random().toString(36).slice(2, 8)}`;
		const timeoutMs =
			typeof data.timeoutMs === "number" && data.timeoutMs > 0 ? data.timeoutMs : DEFAULT_REQUEST_TIMEOUT_MS;
		let settled = false;
		let timer: ReturnType<typeof setTimeout> | undefined;

		const settle = (answer?: unknown) => {
			if (settled) return;
			settled = true;
			if (timer) clearTimeout(timer);
			pending.delete(cancel);
			reply(answer);
		};
		const cancel = () => settle(undefined);
		pending.add(cancel);

		timer = setTimeout(() => {
			if (settled) return;
			// The editor dialog has no timeout option over RPC, so tell the host the request expired.
			const expiry = JSON.stringify({ v: PROTOCOL, ch, payload: { requestId, reason: "timeout" } });
			active.ui.setWidget(`pipan:x:${requestId}`, [expiry]);
			active.ui.setWidget(`pipan:x:${requestId}`, undefined);
			settle(undefined);
		}, timeoutMs);

		const prefill = JSON.stringify({ v: PROTOCOL, ch, payload: { requestId, payload: data.payload } });
		active.ui
			.editor(`pipan:r:${ch}:${kind}`, prefill)
			.then((value) => {
				if (value === undefined) return settle(undefined);
				try {
					const parsed = JSON.parse(value);
					const inner = parsed?.payload;
					if (parsed?.v !== PROTOCOL || !isRecord(inner) || inner.requestId !== requestId) return settle(undefined);
					settle(inner.payload);
				} catch {
					settle(undefined);
				}
			})
			.catch(() => settle(undefined));
	});

	pi.events.on("pipan:log", (data) => {
		if (!isRecord(data) || typeof data.ch !== "string" || typeof data.name !== "string") return;
		const customType = `pipan:l:${data.ch}:${data.name}`;
		if (data.as === "message") {
			if (typeof data.content !== "string") return;
			const options = isRecord(data.options) ? data.options : undefined;
			pi.sendMessage({ customType, content: data.content, display: true, details: data.payload }, options);
		} else {
			pi.appendEntry(customType, data.payload);
		}
	});

	pi.registerCommand("pipan-intent", {
		description: "PiPan channel intent (internal)",
		handler: (args) => {
			let parsed: unknown;
			try {
				parsed = JSON.parse(args);
			} catch {
				return;
			}
			if (!isRecord(parsed) || parsed.v !== PROTOCOL) return;
			if (typeof parsed.ch !== "string" || typeof parsed.name !== "string") return;

			if (parsed.ch === "_shim" && parsed.name === "resync") {
				announce();
				emitAllState();
				pi.events.emit("pipan:resync", {});
				return;
			}
			pi.events.emit("pipan:intent", { ch: parsed.ch, name: parsed.name, payload: parsed.payload });
		},
	});
}
