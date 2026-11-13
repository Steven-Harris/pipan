/**
 * Reference producer for the PiPan extension channel.
 *
 * A standalone Pi extension that uses only `pi.events`, so it has no dependency on PiPan. When
 * PiPan's shim is loaded it publishes graphs, asks questions and writes log entries; without a
 * shim every call is a no-op and the producer falls back to plain Pi dialogs. All data is
 * synthetic. Wire format: docs/plans/extension-channel-spec.md.
 *
 * Commands: /demo-hello, /demo-graph [clear], /demo-state-big <bytes>, /demo-form,
 * /demo-review-diff [large], /demo-review-doc, /demo-log [entry|message], /demo-timeout <ms>.
 * Each command reports its outcome through a notification that starts with "demo-result:".
 */

const CH = "acme.demo";

interface Ctx {
	ui: {
		notify(message: string, type?: "info" | "warning" | "error"): void;
		select(title: string, options: string[]): Promise<string | undefined>;
	};
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
}

interface HostInfo {
	protocol: number;
	primitives: Record<string, number>;
}

export default function (pi: Pi) {
	let timer: ReturnType<typeof setInterval> | undefined;

	/** Returns the host info, or undefined when no shim answers. Cheap, so called per command. */
	const hello = (): HostInfo | undefined => {
		let reply: HostInfo | undefined;
		pi.events.emit("pipan:hello", { reply: (info: HostInfo) => (reply = info) });
		return reply;
	};

	const supports = (primitive: string) => (hello()?.primitives[primitive] ?? 0) >= 1;

	const state = (topic: string, primitive: string, payload: unknown) =>
		pi.events.emit("pipan:state", { ch: CH, topic, primitive, payload });

	const request = (kind: string, payload: unknown, timeoutMs?: number) =>
		new Promise<unknown>((resolve) => pi.events.emit("pipan:request", { ch: CH, kind, payload, timeoutMs, reply: resolve }));

	const report = (ctx: Ctx, result: unknown) => ctx.ui.notify(`demo-result:${JSON.stringify(result)}`, "info");

	/** Intents arrive outside a command, so keep the latest session context for reporting them. */
	let sessionCtx: Ctx | undefined;

	pi.on("session_start", (_event, ctx) => {
		sessionCtx = ctx;
		hello();
	});
	pi.on("session_shutdown", () => {
		sessionCtx = undefined;
		if (timer) clearInterval(timer);
		timer = undefined;
	});

	pi.events.on("pipan:intent", (data) => {
		if (data?.ch !== CH || !sessionCtx) return;
		report(sessionCtx, { intent: { name: data.name, payload: data.payload } });
	});

	pi.registerCommand("demo-hello", {
		description: "Handshake with the PiPan shim",
		handler: (_args, ctx) => report(ctx, { host: hello() ?? null }),
	});

	// Graph -----------------------------------------------------------------------------------

	const STEPS = [
		{ id: "plan", label: "Plan", kind: "agent" },
		{ id: "build", label: "Build", kind: "script" },
		{ id: "test", label: "Test", kind: "script" },
		{ id: "approve", label: "Approve", kind: "approval" },
		{ id: "ship", label: "Ship", kind: "script" },
	];

	const graphAt = (index: number) => ({
		id: "demo-run-1",
		title: "Demo pipeline",
		direction: "LR",
		nodes: STEPS.map((step, i) => ({
			...step,
			status: i < index ? "done" : i === index ? (step.kind === "approval" ? "waiting" : "active") : "pending",
			visits: i <= index ? 1 : 0,
			detail: { summary: `Synthetic step ${step.label}` },
			actions: step.kind === "approval" ? [{ id: "approve", label: "Approve", style: "primary" }] : undefined,
		})),
		edges: STEPS.slice(1).map((step, i) => ({ from: STEPS[i].id, to: step.id, taken: i < index ? 1 : 0 })),
		active: index < STEPS.length ? [STEPS[index].id] : [],
		note: index < STEPS.length ? `Running ${STEPS[index].label}` : "Finished",
	});

	pi.registerCommand("demo-graph", {
		description: "Publish a graph and advance it on a timer (arg: clear)",
		handler: (args, ctx) => {
			if (timer) clearInterval(timer);
			timer = undefined;
			if (args.trim() === "clear") {
				state("graph", "graph", null);
				return report(ctx, { graph: "cleared" });
			}
			if (!supports("graph")) return report(ctx, { graph: "unsupported" });
			let index = 0;
			state("graph", "graph", graphAt(index));
			timer = setInterval(() => {
				index += 1;
				state("graph", "graph", graphAt(index));
				if (index >= STEPS.length && timer) {
					clearInterval(timer);
					timer = undefined;
				}
			}, 1000);
			report(ctx, { graph: "started" });
		},
	});

	pi.registerCommand("demo-state-big", {
		description: "Publish a state of roughly N bytes (for chunking tests)",
		handler: (args, ctx) => {
			const bytes = Math.max(1, Number(args) || 1024);
			state("big", "blob", { blob: "x".repeat(bytes) });
			report(ctx, { published: bytes });
		},
	});

	// Requests --------------------------------------------------------------------------------

	pi.registerCommand("demo-form", {
		description: "Ask a form question; falls back to a plain select without a host",
		handler: async (_args, ctx) => {
			if (!supports("form")) {
				const picked = await ctx.ui.select("Deploy target?", ["staging", "production"]);
				return report(ctx, { fallback: true, picked: picked ?? null });
			}
			const answer = await request("form", {
				title: "Deploy target",
				body: "Choose where the **synthetic** build goes.",
				fields: [
					{
						id: "target",
						type: "select",
						label: "Target",
						required: true,
						options: [
							{ value: "staging", label: "Staging" },
							{ value: "production", label: "Production" },
						],
					},
				],
				actions: [
					{ id: "go", label: "Deploy", style: "primary" },
					{ id: "skip", label: "Skip", skipValidation: true },
				],
			});
			report(ctx, { answer: answer ?? null });
		},
	});

	pi.registerCommand("demo-timeout", {
		description: "Ask a form question that nobody answers (arg: timeout in ms)",
		handler: async (args, ctx) => {
			const timeoutMs = Number(args) || 500;
			const answer = await request("form", { title: "Unanswered", actions: [{ id: "ok", label: "OK" }] }, timeoutMs);
			report(ctx, { answer: answer ?? null, timeoutMs });
		},
	});

	// Review ----------------------------------------------------------------------------------

	const diffFile = (index: number, lineCount: number) => {
		const lines = [];
		for (let n = 1; n <= lineCount; n++) {
			const kind = n % 7 === 0 ? "add" : n % 11 === 0 ? "del" : "ctx";
			lines.push({
				kind,
				text: `const value${index}_${n} = compute(${n});`,
				oldLine: kind === "add" ? undefined : n,
				newLine: kind === "del" ? undefined : n,
			});
		}
		return {
			path: `src/module${index}.ts`,
			status: "modified",
			hunks: [{ header: `@@ -1,${lineCount} +1,${lineCount} @@`, lines }],
		};
	};

	const reviewChoices = [
		{ id: "approve", label: "Approve", style: "primary" },
		{ id: "changes", label: "Request changes", requiresComment: true },
	];

	pi.registerCommand("demo-review-diff", {
		description: "Ask for a diff review (arg: large for a 50,000-line fixture)",
		handler: async (args, ctx) => {
			if (!supports("review")) return report(ctx, { review: "unsupported" });
			const large = args.trim() === "large";
			const files = large
				? Array.from({ length: 50 }, (_unused, i) => diffFile(i, 1000))
				: [diffFile(0, 20), diffFile(1, 12)];
			const answer = await request("review", {
				title: "Review synthetic changes",
				subject: { type: "diff", label: "Changes against main", files },
				comments: [
					{ id: "c1", path: "src/module0.ts", side: "new", line: 3, text: "Rename this value.", status: "open" },
				],
				choices: reviewChoices,
			});
			report(ctx, { answer: answer ?? null });
		},
	});

	pi.registerCommand("demo-review-doc", {
		description: "Ask for a document review",
		handler: async (_args, ctx) => {
			if (!supports("review")) return report(ctx, { review: "unsupported" });
			const answer = await request("review", {
				title: "Review synthetic design note",
				subject: {
					type: "document",
					documents: [
						{
							path: "notes/design.md",
							text: "# Design\n\nFirst paragraph.\n\n- item one\n- item two\n\nClosing paragraph.\n",
						},
					],
				},
				comments: [],
				choices: reviewChoices,
			});
			report(ctx, { answer: answer ?? null });
		},
	});

	// Log -------------------------------------------------------------------------------------

	pi.registerCommand("demo-log", {
		description: "Write a log entry (arg: entry or message)",
		handler: (args, ctx) => {
			const as = args.trim() === "message" ? "message" : "entry";
			pi.events.emit("pipan:log", {
				ch: CH,
				name: "note",
				as,
				payload: { text: "synthetic note", n: 1 },
				content: as === "message" ? "A synthetic note for the model." : undefined,
			});
			report(ctx, { logged: as });
		},
	});
}
