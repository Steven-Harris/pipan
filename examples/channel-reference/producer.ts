/**
 * Reference producer for the PiPan extension channel.
 *
 * A standalone Pi extension that uses only `pi.events`, so it has no dependency on PiPan. When
 * PiPan's shim is loaded it publishes graphs, asks questions and writes log entries; without a
 * shim every call is a no-op and the producer falls back to plain Pi dialogs. All data is
 * synthetic. Wire format: docs/plans/extension-channel-spec.md.
 *
 * Commands: /demo-hello, /demo-graph [clear], /demo-state-big <bytes>, /demo-form [pick|checklist|edit|answer|body],
 * /demo-review-diff [large], /demo-review-doc [multi], /demo-unknown, /demo-log [entry|message], /demo-say, /demo-notify [info|warning|error], /demo-dialog [select|confirm|input|editor].
 * Each command reports its outcome through a notification that starts with "demo-result:".
 */

const CH = "acme.demo";

interface Ctx {
	ui: {
		notify(message: string, type?: "info" | "warning" | "error"): void;
		select(title: string, options: string[]): Promise<string | undefined>;
		confirm(title: string, message: string): Promise<boolean>;
		input(title: string, placeholder?: string): Promise<string | undefined>;
		editor(title: string, prefill?: string): Promise<string | undefined>;
	};
}

interface Pi {
	on(event: string, handler: (event: any, ctx: Ctx) => unknown): void;
	events: {
		on(channel: string, handler: (data: any) => unknown): unknown;
		emit(channel: string, data?: unknown): void;
	};
	sendMessage(message: { customType: string; content: string; display: boolean; details?: unknown }): void;
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

	const request = (kind: string, payload: unknown) =>
		new Promise<unknown>((resolve) => pi.events.emit("pipan:request", { ch: CH, kind, payload, reply: resolve }));

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
		if (data.name === "graph.action" && data.payload?.actionId === "approve") advanceFromApproval();
		// Also write a log entry, so the intent is visible in PiPan without reading notifications.
		pi.events.emit("pipan:log", { ch: CH, name: "intent-received", as: "entry", payload: { name: data.name, payload: data.payload } });
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

	let graphIndex = 0;

	/** Moves past the approval step, however the user approved. A decision made twice is ignored. */
	const advanceFromApproval = () => {
		if (STEPS[graphIndex]?.kind !== "approval") return;
		graphIndex += 1;
		state("graph", "graph", graphAt(graphIndex));
		run();
	};

	let approvalAsked = false;

	/**
	 * A decision the run waits on is a request, so the user gets a dialog. The graph action on the
	 * waiting node stays available as a second way to approve. Rejecting or closing the dialog
	 * leaves the node waiting.
	 */
	const askApproval = async () => {
		if (approvalAsked) return;
		approvalAsked = true;
		const answer = (await request("form", {
			title: "Approve the release?",
			body: "The pipeline is waiting at **Approve**.",
			actions: [
				{ id: "approve", label: "Approve", style: "primary" },
				{ id: "reject", label: "Not yet" },
			],
		})) as { action?: string } | undefined;
		approvalAsked = false;
		if (answer?.action === "approve") advanceFromApproval();
	};

	/** Advances once a second. At the approval step it asks and waits for the user. */
	const run = () => {
		if (timer) clearInterval(timer);
		timer = setInterval(() => {
			if (STEPS[graphIndex]?.kind === "approval") return; // waiting for the user
			graphIndex += 1;
			state("graph", "graph", graphAt(graphIndex));
			if (STEPS[graphIndex]?.kind === "approval") void askApproval();
			if (graphIndex >= STEPS.length && timer) {
				clearInterval(timer);
				timer = undefined;
			}
		}, 1000);
	};

	pi.registerCommand("demo-graph", {
		description: "Publish a graph that advances on a timer and waits at the approval step (arg: clear)",
		handler: (args, ctx) => {
			if (timer) clearInterval(timer);
			timer = undefined;
			if (args.trim() === "clear") {
				state("graph", "graph", null);
				return report(ctx, { graph: "cleared" });
			}
			if (!supports("graph")) return report(ctx, { graph: "unsupported" });
			graphIndex = 0;
			state("graph", "graph", graphAt(graphIndex));
			run();
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

	const FORMS: Record<string, unknown> = {
		pick: {
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
		},
		checklist: {
			title: "Steps to run",
			fields: [
				{
					id: "steps",
					type: "multiselect",
					label: "Steps",
					min: 1,
					max: 2,
					default: ["lint"],
					options: [
						{ value: "lint", label: "Lint" },
						{ value: "test", label: "Test", description: "Runs the synthetic suite" },
						{ value: "ship", label: "Ship" },
					],
				},
			],
			actions: [{ id: "run", label: "Run", style: "primary" }],
		},
		edit: {
			title: "Edit the message",
			fields: [
				{
					id: "message",
					type: "text",
					label: "Message",
					multiline: true,
					required: true,
					requiredFor: ["save"],
					default: "first line\nsecond line",
				},
			],
			actions: [
				{ id: "save", label: "Save", style: "primary" },
				{ id: "discard", label: "Discard", style: "danger" },
			],
		},
		answer: {
			title: "Which environment?",
			body: "Pick a fixed choice, or type a different answer and submit it.",
			fields: [{ id: "text", type: "text", label: "Other answer", placeholder: "Type an answer", required: true, requiredFor: ["submit"] }],
			actions: [
				{ id: "staging", label: "Staging" },
				{ id: "production", label: "Production" },
				{ id: "submit", label: "Submit answer", style: "primary" },
			],
		},
		body: {
			title: "Proceed?",
			body: "Nothing to fill in. The chosen action is the answer.",
			actions: [
				{ id: "yes", label: "Yes", style: "primary" },
				{ id: "no", label: "No" },
			],
		},
	};

	pi.registerCommand("demo-form", {
		description: "Ask a form question (arg: pick, checklist, edit, answer or body); falls back to a plain select without a host",
		handler: async (args, ctx) => {
			if (!supports("form")) {
				const picked = await ctx.ui.select("Deploy target?", ["staging", "production"]);
				return report(ctx, { fallback: true, picked: picked ?? null });
			}
			const shape = FORMS[args.trim() || "pick"] ?? FORMS.pick;
			const answer = await request("form", shape);
			report(ctx, { answer: answer ?? null });
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

	const DESIGN =
		"# Design\n\nFirst paragraph.\n\n- item one\n- item two\n\nClosing paragraph.\n";

	pi.registerCommand("demo-review-doc", {
		description: "Ask for a document review (arg: multi for two documents with an earlier comment)",
		handler: async (args, ctx) => {
			if (!supports("review")) return report(ctx, { review: "unsupported" });
			const multi = args.trim() === "multi";
			const documents = multi
				? [
						{ path: "notes/design.md", text: DESIGN },
						{ path: "notes/todo.txt", text: "write tests\nship it\n", format: "text" },
					]
				: [{ path: "notes/design.md", text: DESIGN }];
			const answer = await request("review", {
				title: "Review synthetic design note",
				subject: { type: "document", documents },
				comments: multi
					? [{ id: "d1", path: "notes/todo.txt", side: "new", line: 2, text: "Say what ships.", status: "open" }]
					: [],
				choices: reviewChoices,
			});
			report(ctx, { answer: answer ?? null });
		},
	});

	pi.registerCommand("demo-unknown", {
		description: "Send a request kind this host does not know",
		handler: async (_args, ctx) => {
			const answer = await request("mystery", { hello: "world" });
			report(ctx, { answer: answer ?? null });
		},
	});

	// Messages and notifications --------------------------------------------------------------

	pi.registerCommand("demo-say", {
		description: "Add a message to the session: one shown in the chat and one only the model sees",
		handler: (_args, ctx) => {
			pi.sendMessage({
				customType: "acme.demo-note",
				content: "A **synthetic** note from the demo producer.\n\n- shown in the chat\n- kept in the session",
				display: true,
				details: { n: 1 },
			});
			pi.sendMessage({ customType: "acme.demo-hidden", content: "A note for the model only.", display: false });
			report(ctx, { said: 2 });
		},
	});

	pi.registerCommand("demo-notify", {
		description: "Send a notification (arg: info, warning or error)",
		handler: (args, ctx) => {
			const level = args.trim() === "warning" || args.trim() === "error" ? (args.trim() as "warning" | "error") : "info";
			ctx.ui.notify(`Synthetic ${level} notification`, level);
		},
	});

	// Plain Pi dialogs ------------------------------------------------------------------------

	// These are ordinary `ctx.ui` calls, not channel requests. PiPan shows them as dialogs too.
	pi.registerCommand("demo-dialog", {
		description: "Ask with a plain Pi dialog (arg: select, confirm, input or editor)",
		handler: async (args, ctx) => {
			const method = args.trim() || "select";
			let answer: unknown;
			if (method === "confirm") answer = await ctx.ui.confirm("Proceed?", "This is a synthetic confirmation.");
			else if (method === "input") answer = await ctx.ui.input("Enter a value", "type here");
			else if (method === "editor") answer = await ctx.ui.editor("Edit the text", "first line\nsecond line");
			else answer = await ctx.ui.select("Allow the synthetic action?", ["Allow", "Block"]);
			report(ctx, { method, answer: answer ?? null });
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
