/**
 * PiPan "Plan Mode" gate (Phase 5).
 *
 * Loaded into every worktree Pi session with `pi --extension <this file>`. It intercepts
 * destructive `bash` tool calls (file deletion, `git push`, hard resets, ...) and pauses the
 * call by awaiting an RPC dialog. PiPan's Rust core recognises the dialog by its title
 * prefix, renders it as an `ApprovalNode` in the Svelte Flow dashboard and answers with an
 * `extension_ui_response` once the user clicks Approve / Modify / Reject.
 *
 * Wire format (kept in sync with `src-tauri/src/approval.rs`):
 *   request : extension_ui_request { method: "select", title: "PIPAN_APPROVAL::<json>",
 *             options: ["approve", "modify", "reject"] }
 *   response: value = "approve" | "reject" | "modify:<replacement command>"
 *
 * This file intentionally has no imports (types are structural) so it loads from anywhere.
 */

const TITLE_PREFIX = "PIPAN_APPROVAL::";

/** Matches the start of a shell command: line start, a separator, or a wrapper like sudo. */
const CMD = String.raw`(?:^|[;&|(\n\`]\s*|\b(?:sudo|xargs|exec|command|nohup|time)\s+(?:-\S+\s+)*)`;

interface Rule {
	kind: "delete" | "git-push" | "git-destructive";
	label: string;
	pattern: RegExp;
}

const RULES: Rule[] = [
	{
		kind: "delete",
		label: "Deletes files",
		pattern: new RegExp(String.raw`${CMD}(?:rm|rmdir|unlink|shred|del|erase|rd|remove-item|ri)\b`, "i"),
	},
	{ kind: "delete", label: "Deletes files", pattern: /\bgit\s+(?:-\S+\s+)*rm\b/i },
	{ kind: "delete", label: "Deletes files", pattern: /\bfind\b[^;&|\n]*\s-delete\b/i },
	{ kind: "git-push", label: "Pushes to a remote", pattern: /\bgit\s+(?:-\S+\s+)*push\b/i },
	{
		kind: "git-destructive",
		label: "Discards git history or changes",
		pattern:
			/\bgit\s+(?:-\S+\s+)*(?:reset\s+(?:\S+\s+)*--hard|clean\s+(?:\S+\s+)*-\S*[fdx]|branch\s+(?:\S+\s+)*-D|tag\s+(?:\S+\s+)*-d|stash\s+(?:drop|clear)|checkout\s+(?:\S+\s+)*--\s+\.|restore\s+(?:\S+\s+)*\.)/i,
	},
];

export function classify(command: string): Rule | undefined {
	return RULES.find((r) => r.pattern.test(command));
}

interface ToolCallEventLike {
	toolName: string;
	toolCallId?: string;
	input: Record<string, unknown>;
}

interface ContextLike {
	hasUI?: boolean;
	signal?: AbortSignal;
	ui: {
		select(
			title: string,
			options: string[],
			opts?: { signal?: AbortSignal; timeout?: number },
		): Promise<string | undefined>;
	};
}

interface ExtensionAPILike {
	on(
		event: "tool_call",
		handler: (
			event: ToolCallEventLike,
			ctx: ContextLike,
		) => Promise<{ block: true; reason: string } | undefined> | { block: true; reason: string } | undefined,
	): void;
}

export default function (pi: ExtensionAPILike) {
	if (process.env.PIPAN_PLAN_MODE === "off") return;

	pi.on("tool_call", async (event, ctx) => {
		if (event.toolName !== "bash") return undefined;
		const command = typeof event.input?.command === "string" ? event.input.command : "";
		const rule = command ? classify(command) : undefined;
		if (!rule) return undefined;

		if (!ctx.hasUI) {
			return { block: true, reason: `Blocked by PiPan Plan Mode (${rule.label.toLowerCase()}); no approval UI available.` };
		}

		const payload = {
			toolName: event.toolName,
			toolCallId: event.toolCallId ?? null,
			kind: rule.kind,
			reason: rule.label,
			action: command,
		};

		// No timeout: the call stays paused until the user decides. Aborting the run resolves
		// the dialog with `undefined`, which is treated as a rejection below.
		const choice = await ctx.ui.select(
			`${TITLE_PREFIX}${JSON.stringify(payload)}`,
			["approve", "modify", "reject"],
			{ signal: ctx.signal },
		);

		if (choice === "approve") return undefined;

		if (typeof choice === "string" && choice.startsWith("modify:")) {
			const replacement = choice.slice("modify:".length);
			if (replacement.trim()) {
				// In-place mutation: the (user-edited) command is what actually runs.
				event.input.command = replacement;
				return undefined;
			}
		}

		return { block: true, reason: "Rejected by the user in PiPan Plan Mode. Do not retry this action; ask for guidance or choose a safer approach." };
	});
}
