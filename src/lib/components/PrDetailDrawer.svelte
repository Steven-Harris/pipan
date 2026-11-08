<script lang="ts">
	import { openUrl } from '@tauri-apps/plugin-opener';
	import {
		ArrowLeft,
		CheckCircle2,
		XCircle,
		Clock,
		ExternalLink,
		GitBranch,
		GitMerge,
		GitPullRequest,
		Loader2,
		MessageSquare,
		MessageSquarePlus,
		RefreshCw,
		Send,
		FileCode2,
		Columns2,
		Rows2,
		Trash2,
		ShieldCheck,
		AlertTriangle,
		Radio,
		Lock,
		Check,
		AlertCircle
	} from 'lucide-svelte';
	import { githubStore } from '#lib/stores/github.svelte.ts';
	import type {
		GhCheck,
		GhPrReviewComment,
		PrDiffHunk,
		PrDiffLine,
		PrMergeMethod,
		PrReviewDraftComment,
		PrReviewEvent
	} from '#lib/types/github.ts';
	import PrDiffFileTree from './github/PrDiffFileTree.svelte';

	let commentText = $state('');
	let mergeMethod = $state<PrMergeMethod>('squash');
	let deleteBranch = $state(true);
	let mergeSubject = $state('');
	let mergeBody = $state('');
	let showMergeOptions = $state(false);

	// Review submission popover state
	let showReviewPopover = $state(false);
	let reviewEvent = $state<PrReviewEvent>('COMMENT');
	let reviewBody = $state('');

	// Inline line comment composer state
	let activeInlineTarget = $state<{
		path: string;
		line: number;
		side: 'LEFT' | 'RIGHT';
	} | null>(null);
	let inlineDraftBody = $state('');

	// File filter in Files & Review panel
	let selectedFileFilter = $state<string | null>(null);
	let showDraftTray = $state(false);
	let leftTab = $state<'checks' | 'overview'>('checks');

	const pr = $derived(githubStore.selectedPrDetail);

	// Sync mergeMethod with repository defaults when pr detail loads
	$effect(() => {
		const allowed = githubStore.allowedMergeMethods;
		if (allowed.length > 0 && !allowed.includes(mergeMethod)) {
			mergeMethod = allowed[0];
		}
		if (typeof pr?.deleteBranchOnMerge === 'boolean') {
			deleteBranch = pr.deleteBranchOnMerge;
		}
	});

	interface TimelineEntry {
		id: string;
		kind: 'comment' | 'review';
		author: string;
		body: string;
		timestamp: string;
		reviewState?: string;
		url?: string;
	}

	const timeline = $derived.by((): TimelineEntry[] => {
		if (!pr) return [];
		const items: TimelineEntry[] = [];
		for (const [idx, c] of (pr.comments ?? []).entries()) {
			items.push({
				id: c.id ?? `comment-${idx}`,
				kind: 'comment',
				author: c.author?.login ?? 'unknown',
				body: c.body ?? '',
				timestamp: c.createdAt ?? '',
				url: c.url
			});
		}
		for (const [idx, r] of (pr.reviews ?? []).entries()) {
			if (!r.body && r.state === 'COMMENTED') continue;
			items.push({
				id: r.id ?? `review-${idx}`,
				kind: 'review',
				author: r.author?.login ?? 'unknown',
				body: r.body ?? '',
				timestamp: r.submittedAt ?? '',
				reviewState: r.state
			});
		}
		return items.sort((a, b) => (a.timestamp || '').localeCompare(b.timestamp || ''));
	});

	const checksSummary = $derived.by(() => {
		const counts = { pass: 0, fail: 0, pending: 0, skipping: 0 };
		for (const c of githubStore.selectedPrChecks) {
			if (c.bucket === 'pass') counts.pass += 1;
			else if (c.bucket === 'fail' || c.bucket === 'cancel') counts.fail += 1;
			else if (c.bucket === 'pending') counts.pending += 1;
			else counts.skipping += 1;
		}
		return counts;
	});

	const visibleDiffFiles = $derived.by(() => {
		if (!selectedFileFilter) return githubStore.selectedPrDiff;
		return githubStore.selectedPrDiff.filter((f) => f.path === selectedFileFilter);
	});

	interface SplitRow {
		left: PrDiffLine | null;
		right: PrDiffLine | null;
		meta: PrDiffLine | null;
	}

	function buildSplitRows(hunk: PrDiffHunk): SplitRow[] {
		const rows: SplitRow[] = [];
		const lines = hunk.lines;
		let i = 0;
		while (i < lines.length) {
			const line = lines[i];
			if (line.kind === 'context') {
				rows.push({ left: line, right: line, meta: null });
				i += 1;
			} else if (line.kind === 'meta') {
				rows.push({ left: null, right: null, meta: line });
				i += 1;
			} else if (line.kind === 'del' || line.kind === 'add') {
				const dels: PrDiffLine[] = [];
				const adds: PrDiffLine[] = [];
				while (i < lines.length && lines[i].kind === 'del') {
					dels.push(lines[i]);
					i += 1;
				}
				while (i < lines.length && lines[i].kind === 'add') {
					adds.push(lines[i]);
					i += 1;
				}
				const maxLen = Math.max(dels.length, adds.length);
				for (let k = 0; k < maxLen; k++) {
					rows.push({
						left: dels[k] ?? null,
						right: adds[k] ?? null,
						meta: null
					});
				}
			} else {
				i += 1;
			}
		}
		return rows;
	}

	function existingCommentsForLine(
		path: string,
		line: number | null,
		side: 'LEFT' | 'RIGHT'
	): GhPrReviewComment[] {
		if (line === null) return [];
		return githubStore.selectedPrReviewComments.filter((c) => {
			const cSide = c.side ?? 'RIGHT';
			const cLine = c.line ?? c.original_line ?? null;
			return c.path === path && cLine === line && cSide === side;
		});
	}

	function draftCommentsForLine(
		path: string,
		line: number | null,
		side: 'LEFT' | 'RIGHT'
	): Array<{ draft: PrReviewDraftComment; index: number }> {
		if (line === null) return [];
		const result: Array<{ draft: PrReviewDraftComment; index: number }> = [];
		githubStore.draftReviewComments.forEach((draft, index) => {
			if (draft.path === path && draft.line === line && draft.side === side) {
				result.push({ draft, index });
			}
		});
		return result;
	}

	function openInlineComposer(path: string, line: number | null, side: 'LEFT' | 'RIGHT') {
		if (line === null || line <= 0) return;
		if (
			activeInlineTarget?.path === path &&
			activeInlineTarget?.line === line &&
			activeInlineTarget?.side === side
		) {
			activeInlineTarget = null;
			return;
		}
		activeInlineTarget = { path, line, side };
		inlineDraftBody = '';
	}

	function submitInlineDraft() {
		if (!activeInlineTarget || !inlineDraftBody.trim()) return;
		githubStore.addDraftReviewComment({
			path: activeInlineTarget.path,
			line: activeInlineTarget.line,
			side: activeInlineTarget.side,
			body: inlineDraftBody
		});
		inlineDraftBody = '';
		activeInlineTarget = null;
	}

	async function handlePostComment() {
		const ok = await githubStore.postComment(commentText);
		if (ok) {
			commentText = '';
		}
	}

	async function handleSubmitReview() {
		const ok = await githubStore.submitReview(reviewEvent, reviewBody);
		if (ok) {
			reviewBody = '';
			showReviewPopover = false;
		}
	}

	async function handleMerge() {
		const ok = await githubStore.mergePr({
			method: mergeMethod,
			deleteBranch,
			subject: mergeSubject,
			body: mergeBody
		});
		if (ok) {
			mergeSubject = '';
			mergeBody = '';
			showMergeOptions = false;
		}
	}

	function formatDuration(startedAt?: string, completedAt?: string): string {
		if (!startedAt) return '—';
		const start = Date.parse(startedAt);
		const end = completedAt ? Date.parse(completedAt) : Date.now();
		if (!Number.isFinite(start) || !Number.isFinite(end) || end < start) return '—';
		const totalSec = Math.round((end - start) / 1000);
		if (totalSec < 60) return `${totalSec}s`;
		const mins = Math.floor(totalSec / 60);
		const secs = totalSec % 60;
		return `${mins}m ${secs}s`;
	}

	function formatTimestamp(iso?: string): string {
		if (!iso) return '';
		const d = new Date(iso);
		if (Number.isNaN(d.getTime())) return iso;
		return d.toLocaleString(undefined, {
			month: 'short',
			day: 'numeric',
			hour: '2-digit',
			minute: '2-digit'
		});
	}

	async function openExternal(url?: string) {
		if (!url) return;
		try {
			await openUrl(url);
		} catch {
			window.open(url, '_blank', 'noopener,noreferrer');
		}
	}

	const isSelfAuthored = $derived(githubStore.isSelectedPrAuthoredByViewer);
	const allowedMethods = $derived(githubStore.allowedMergeMethods);
	const isForwarderLive = $derived(githubStore.forwarder?.state === 'running');
</script>

<div class="flex h-full min-h-0 flex-col bg-zinc-950 text-zinc-100">
	<!-- Top Navigation Header -->
	<header class="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-zinc-800/80 bg-zinc-900/80 px-4 py-2.5 backdrop-blur-md">
		<div class="flex min-w-0 items-center gap-2.5">
			<button
				type="button"
				onclick={() => void githubStore.selectPr(null)}
				class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-zinc-800 bg-zinc-900 text-zinc-300 hover:border-zinc-700 hover:bg-zinc-800 hover:text-zinc-100 cursor-pointer"
				title="Close PR view (Return to Chat workspace)"
				aria-label="Close PR view"
			>
				<ArrowLeft class="h-4 w-4" />
			</button>

			<div class="min-w-0">
				<div class="flex flex-wrap items-center gap-1.5">
					{#if pr?.repo || githubStore.selectedPrRepo}
						<span class="rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-[11px] font-medium text-zinc-300">
							{pr?.repo ?? githubStore.selectedPrRepo}
						</span>
					{/if}
					<span class="font-mono text-xs text-zinc-400">#{githubStore.selectedPrNumber}</span>

					{#if pr}
						{#if pr.state === 'MERGED'}
							<span class="inline-flex items-center gap-1 rounded-full border border-purple-500/30 bg-purple-500/15 px-2 py-0.5 text-[10px] font-semibold text-purple-300">
								<GitMerge class="h-2.5 w-2.5" />
								Merged
							</span>
						{:else if pr.state === 'CLOSED'}
							<span class="inline-flex items-center gap-1 rounded-full border border-rose-500/30 bg-rose-500/15 px-2 py-0.5 text-[10px] font-semibold text-rose-300">
								Closed
							</span>
						{:else if pr.isDraft}
							<span class="inline-flex items-center gap-1 rounded-full border border-zinc-700 bg-zinc-800 px-2 py-0.5 text-[10px] font-semibold text-zinc-300">
								Draft
							</span>
						{:else}
							<span class="inline-flex items-center gap-1 rounded-full border border-emerald-500/30 bg-emerald-500/15 px-2 py-0.5 text-[10px] font-semibold text-emerald-300">
								<GitPullRequest class="h-2.5 w-2.5" />
								Open
							</span>
						{/if}

						{#if pr.reviewDecision === 'APPROVED'}
							<span class="inline-flex items-center gap-1 rounded-full border border-emerald-500/30 bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium text-emerald-300">
								<ShieldCheck class="h-2.5 w-2.5" />
								Approved
							</span>
						{:else if pr.reviewDecision === 'CHANGES_REQUESTED'}
							<span class="inline-flex items-center gap-1 rounded-full border border-amber-500/30 bg-amber-500/10 px-2 py-0.5 text-[10px] font-medium text-amber-300">
								<AlertTriangle class="h-2.5 w-2.5" />
								Changes Requested
							</span>
						{/if}
					{/if}
				</div>

				<h2 class="mt-0.5 max-w-2xl truncate text-sm font-semibold text-zinc-100">
					{pr?.title ?? 'Loading pull request…'}
				</h2>
			</div>
		</div>

		<!-- Right Action Controls (WebSocket status, GitHub link, Refresh) -->
		<div class="flex shrink-0 items-center gap-2">
			<!-- Live WebSocket Forwarder Status -->
			<button
				type="button"
				onclick={() => void githubStore.toggleWebhookForwarder()}
				disabled={githubStore.forwarderBusy}
				class="inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium transition cursor-pointer {isForwarderLive
					? 'border-emerald-500/40 bg-emerald-500/15 text-emerald-300'
					: githubStore.forwarder?.state === 'starting'
						? 'border-amber-500/40 bg-amber-500/15 text-amber-300'
						: 'border-zinc-700 bg-zinc-800 text-zinc-400 hover:text-zinc-200'}"
				title={isForwarderLive
					? `Live WebSocket tunnel connected (${githubStore.forwarder?.repo}). Live CI/check updates active.`
					: 'Click to start live GitHub WebSocket forwarder tunnel'}
			>
				<Radio class="h-3 w-3 {isForwarderLive ? 'animate-pulse text-emerald-400' : ''}" />
				<span>
					{isForwarderLive
						? 'WebSocket Live'
						: githubStore.forwarder?.state === 'starting'
							? 'Connecting…'
							: 'Connect WebSocket'}
				</span>
			</button>

			{#if pr?.url}
				<button
					type="button"
					onclick={() => void openExternal(pr.url)}
					class="inline-flex items-center gap-1 rounded-md border border-zinc-800 bg-zinc-900 px-2.5 py-1 text-xs text-zinc-300 hover:border-zinc-700 hover:bg-zinc-800 cursor-pointer"
					title="Open on GitHub in browser"
				>
					<ExternalLink class="h-3.5 w-3.5" />
					<span class="hidden sm:inline">GitHub</span>
				</button>
			{/if}

			<button
				type="button"
				onclick={() => void githubStore.refreshSelectedPr()}
				disabled={githubStore.loadingDetail}
				class="inline-flex h-7 w-7 items-center justify-center rounded-md border border-zinc-800 bg-zinc-900 text-zinc-300 hover:border-zinc-700 hover:bg-zinc-800 disabled:opacity-50 cursor-pointer"
				title="Refresh pull request metadata, checks, and diff"
				aria-label="Refresh PR"
			>
				<RefreshCw class="h-3.5 w-3.5 {githubStore.loadingDetail ? 'animate-spin text-indigo-400' : ''}" />
			</button>
		</div>
	</header>

	{#if githubStore.actionFeedback}
		<div
			class="flex items-center justify-between border-b px-4 py-1.5 text-xs {githubStore.actionFeedback.type === 'success'
				? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-200'
				: 'border-rose-500/30 bg-rose-500/10 text-rose-200'}"
		>
			<div class="flex items-center gap-1.5">
				{#if githubStore.actionFeedback.type === 'success'}
					<Check class="h-3.5 w-3.5 text-emerald-400" />
				{:else}
					<AlertCircle class="h-3.5 w-3.5 text-rose-400" />
				{/if}
				<span>{githubStore.actionFeedback.message}</span>
			</div>
			<button
				type="button"
				onclick={() => (githubStore.actionFeedback = null)}
				class="text-[11px] underline opacity-80 hover:opacity-100 cursor-pointer"
			>
				Dismiss
			</button>
		</div>
	{/if}

	{#if githubStore.detailError}
		<div class="m-3 rounded-lg border border-rose-500/30 bg-rose-500/10 p-3 text-xs text-rose-200">
			{githubStore.detailError}
		</div>
	{/if}

	<!-- 2-Column Split: Left = Description + Checks + Merge; Right = Files Tree + Diff Viewer -->
	<div class="grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-12 divide-y lg:divide-y-0 lg:divide-x divide-zinc-800/80 overflow-hidden">
		<!-- Left Panel (5 cols on lg, 4 on xl): PR Info, Checks, Merge & Review, Conversation -->
		<div class="flex min-h-0 flex-col overflow-y-auto lg:col-span-5 xl:col-span-4 bg-zinc-950/60 p-4 space-y-4">
			<!-- Branch Info Bar -->
			{#if pr}
				<div class="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-zinc-800 bg-zinc-900/50 p-2.5 text-xs">
					<div class="flex items-center gap-1.5">
						<span class="text-zinc-400">Author:</span>
						<strong class="text-zinc-200">@{pr.author?.login ?? 'unknown'}</strong>
						{#if isSelfAuthored}
							<span class="rounded bg-indigo-500/20 border border-indigo-500/40 px-1.5 py-0.2 text-[10px] font-semibold text-indigo-300">
								You
							</span>
						{/if}
					</div>
					<div class="inline-flex items-center gap-1 rounded bg-zinc-950 px-2 py-0.5 font-mono text-[11px] text-indigo-300 border border-zinc-800">
						<GitBranch class="h-3 w-3" />
						<span class="truncate max-w-[120px]">{pr.headRefName}</span>
						<span class="text-zinc-500">→</span>
						<span>{pr.baseRefName}</span>
					</div>
				</div>
			{/if}

			<!-- Tab Switcher for Left Column: Checks & Merge / Conversation & Description -->
			<div class="flex rounded-lg border border-zinc-800 bg-zinc-900/60 p-0.5 text-xs">
				<button
					type="button"
					onclick={() => (leftTab = 'checks')}
					class="flex-1 rounded-md py-1.5 font-medium transition cursor-pointer {leftTab === 'checks'
						? 'bg-zinc-800 text-zinc-100 shadow-sm'
						: 'text-zinc-400 hover:text-zinc-200'}"
				>
					Checks & Merge
					{#if githubStore.selectedPrChecks.length > 0}
						<span class="ml-1 rounded-full bg-zinc-700 px-1.5 py-0.2 text-[10px] font-mono">
							{githubStore.selectedPrChecks.length}
						</span>
					{/if}
				</button>
				<button
					type="button"
					onclick={() => (leftTab = 'overview')}
					class="flex-1 rounded-md py-1.5 font-medium transition cursor-pointer {leftTab === 'overview'
						? 'bg-zinc-800 text-zinc-100 shadow-sm'
						: 'text-zinc-400 hover:text-zinc-200'}"
				>
					Conversation ({timeline.length})
				</button>
			</div>

			{#if leftTab === 'checks'}
				<!-- Status Checks Section with Live WebSocket indicator -->
				<div class="rounded-xl border border-zinc-800 bg-zinc-900/60 p-3.5 space-y-3">
					<div class="flex items-center justify-between">
						<div class="flex items-center gap-2">
							<CheckCircle2 class="h-4 w-4 text-emerald-400" />
							<span class="text-xs font-semibold text-zinc-100">Status Checks & CI</span>
						</div>
						<div class="flex items-center gap-1.5">
							<span
								class="inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10px] font-medium {isForwarderLive
									? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-300'
									: 'border-zinc-700 bg-zinc-800 text-zinc-400'}"
								title={isForwarderLive
									? 'Receiving check_run and check_suite events via live WebSocket tunnel'
									: 'WebSocket tunnel not active'}
							>
								<Radio class="h-2.5 w-2.5 {isForwarderLive ? 'animate-pulse text-emerald-400' : ''}" />
								<span>{isForwarderLive ? 'Live WebSocket' : 'Standby'}</span>
							</span>
							<button
								type="button"
								onclick={() =>
									githubStore.selectedPrNumber !== null &&
									void githubStore.loadPrChecks(githubStore.selectedPrNumber)}
								class="rounded p-1 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 cursor-pointer"
								title="Refresh CI checks"
							>
								<RefreshCw class="h-3 w-3 {githubStore.loadingChecks ? 'animate-spin text-indigo-400' : ''}" />
							</button>
						</div>
					</div>

					<!-- Checks Summary Pill Counts -->
					<div class="flex flex-wrap items-center gap-2 text-xs">
						<span class="inline-flex items-center gap-1 rounded-md border border-emerald-500/30 bg-emerald-500/10 px-2 py-0.5 text-emerald-300">
							<CheckCircle2 class="h-3 w-3" />
							{checksSummary.pass} passed
						</span>
						{#if checksSummary.fail > 0}
							<span class="inline-flex items-center gap-1 rounded-md border border-rose-500/30 bg-rose-500/10 px-2 py-0.5 text-rose-300">
								<XCircle class="h-3 w-3" />
								{checksSummary.fail} failed
							</span>
						{/if}
						{#if checksSummary.pending > 0}
							<span class="inline-flex items-center gap-1 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-0.5 text-amber-300">
								<Clock class="h-3 w-3" />
								{checksSummary.pending} pending
							</span>
						{/if}
					</div>

					<!-- Individual Check Rows -->
					{#if githubStore.loadingChecks && githubStore.selectedPrChecks.length === 0}
						<div class="flex items-center justify-center py-6 text-xs text-zinc-400 gap-2">
							<Loader2 class="h-4 w-4 animate-spin text-indigo-400" />
							<span>Loading CI status checks…</span>
						</div>
					{:else if githubStore.selectedPrChecks.length === 0}
						<div class="rounded-lg border border-zinc-800/60 bg-zinc-950/40 p-4 text-center text-xs text-zinc-500">
							No CI checks reported for this pull request.
						</div>
					{:else}
						<div class="divide-y divide-zinc-800/70 rounded-lg border border-zinc-800 bg-zinc-950/60 max-h-56 overflow-y-auto">
							{#each githubStore.selectedPrChecks as check, idx (`${check.workflow}-${check.name}-${idx}`)}
								<div class="flex items-center justify-between gap-2 p-2 text-xs">
									<div class="flex min-w-0 items-center gap-2">
										{#if check.bucket === 'pass'}
											<CheckCircle2 class="h-3.5 w-3.5 shrink-0 text-emerald-400" />
										{:else if check.bucket === 'fail' || check.bucket === 'cancel'}
											<XCircle class="h-3.5 w-3.5 shrink-0 text-rose-400" />
										{:else}
											<Clock class="h-3.5 w-3.5 shrink-0 text-amber-400" />
										{/if}
										<div class="min-w-0">
											<div class="truncate font-medium text-zinc-200">{check.name}</div>
											<div class="truncate text-[10px] text-zinc-500">
												{check.workflow || 'Check'}
											</div>
										</div>
									</div>

									<div class="flex shrink-0 items-center gap-1.5">
										<span class="font-mono text-[10px] text-zinc-400">
											{formatDuration(check.startedAt, check.completedAt)}
										</span>
										{#if check.link}
											<button
												type="button"
												onclick={() => void openExternal(check.link)}
												class="rounded p-0.5 text-zinc-400 hover:text-zinc-200 cursor-pointer"
												title="Open check logs"
											>
												<ExternalLink class="h-3 w-3" />
											</button>
										{/if}
									</div>
								</div>
							{/each}
						</div>
					{/if}
				</div>

				<!-- Merge Section with Repo Permissions & Allowed Merge Method Enforcement -->
				{#if pr && pr.state === 'OPEN'}
					<div class="rounded-xl border border-zinc-800 bg-zinc-900/60 p-3.5 space-y-3">
						<div class="flex items-center justify-between">
							<div class="flex items-center gap-2">
								<GitMerge class="h-4 w-4 text-emerald-400" />
								<span class="text-xs font-semibold text-zinc-100">Merge Strategy & Rules</span>
							</div>
							<span class="rounded bg-zinc-800 px-2 py-0.5 font-mono text-[10px] text-zinc-300">
								{pr.mergeable ?? 'MERGEABLE'}
							</span>
						</div>

						<!-- Allowed Merge Types Grid -->
						<div class="rounded-lg border border-zinc-800 bg-zinc-950/60 p-2.5 space-y-2 text-xs">
							<div class="text-[11px] font-medium text-zinc-400">Allowed repository merge types:</div>
							<div class="grid grid-cols-3 gap-1.5 text-center">
								<div class="rounded border p-1.5 font-mono text-[11px] {allowedMethods.includes('squash')
									? 'border-emerald-500/40 bg-emerald-950/20 text-emerald-300'
									: 'border-zinc-800 bg-zinc-900 text-zinc-600 line-through'}">
									Squash
								</div>
								<div class="rounded border p-1.5 font-mono text-[11px] {allowedMethods.includes('merge')
									? 'border-emerald-500/40 bg-emerald-950/20 text-emerald-300'
									: 'border-zinc-800 bg-zinc-900 text-zinc-600 line-through'}">
									Merge
								</div>
								<div class="rounded border p-1.5 font-mono text-[11px] {allowedMethods.includes('rebase')
									? 'border-emerald-500/40 bg-emerald-950/20 text-emerald-300'
									: 'border-zinc-800 bg-zinc-900 text-zinc-600 line-through'}">
									Rebase
								</div>
							</div>
						</div>

						<!-- Merge Controls Form -->
						<div class="space-y-2 pt-1">
							<div class="flex flex-wrap items-center gap-2">
								<select
									bind:value={mergeMethod}
									aria-label="Select merge method"
									class="flex-1 rounded-md border border-zinc-700 bg-zinc-950 px-2 py-1.5 text-xs text-zinc-100 outline-none focus:border-indigo-500"
								>
									{#each allowedMethods as method (method)}
										<option value={method}>
											{method === 'squash'
												? 'Squash and merge'
												: method === 'merge'
													? 'Create merge commit'
													: 'Rebase and merge'}
										</option>
									{/each}
								</select>

								<label class="inline-flex items-center gap-1.5 text-xs text-zinc-300 cursor-pointer">
									<input
										type="checkbox"
										bind:checked={deleteBranch}
										class="rounded border-zinc-700 bg-zinc-950 text-indigo-500"
									/>
									<span>Delete branch</span>
								</label>
							</div>

							<div class="flex items-center justify-between gap-2">
								<button
									type="button"
									onclick={() => (showMergeOptions = !showMergeOptions)}
									class="text-[11px] text-zinc-400 hover:text-zinc-200 underline cursor-pointer"
								>
									{showMergeOptions ? 'Hide custom commit' : 'Customize commit message'}
								</button>

								<button
									type="button"
									onclick={() => void handleMerge()}
									disabled={githubStore.mergingPr || pr.isDraft || pr.mergeable === 'CONFLICTING'}
									class="inline-flex items-center gap-1.5 rounded-lg border border-emerald-500/50 bg-emerald-600 px-3.5 py-1.5 text-xs font-semibold text-white hover:bg-emerald-500 disabled:opacity-50 cursor-pointer"
								>
									{#if githubStore.mergingPr}
										<Loader2 class="h-3.5 w-3.5 animate-spin" />
									{:else}
										<GitMerge class="h-3.5 w-3.5" />
									{/if}
									<span>Merge #{pr.number}</span>
								</button>
							</div>

							{#if showMergeOptions}
								<div class="space-y-2 pt-2 border-t border-zinc-800">
									<input
										type="text"
										bind:value={mergeSubject}
										placeholder="Optional commit subject override…"
										class="w-full rounded border border-zinc-800 bg-zinc-950 px-2.5 py-1 text-xs text-zinc-100 outline-none focus:border-indigo-500"
									/>
									<textarea
										bind:value={mergeBody}
										rows="2"
										placeholder="Optional extended commit description…"
										class="w-full rounded border border-zinc-800 bg-zinc-950 px-2.5 py-1 text-xs text-zinc-100 outline-none focus:border-indigo-500"
									></textarea>
								</div>
							{/if}
						</div>
					</div>
				{/if}
			{/if}

			{#if leftTab === 'overview'}
				<!-- Description Card -->
				<div class="rounded-xl border border-zinc-800 bg-zinc-900/60 p-3.5 space-y-2">
					<div class="flex items-center justify-between text-[11px] text-zinc-400">
						<span class="font-semibold text-zinc-300">Description</span>
						{#if pr?.createdAt}
							<span>Opened {formatTimestamp(pr.createdAt)}</span>
						{/if}
					</div>
					{#if pr?.body && pr.body.trim()}
						<div class="whitespace-pre-wrap break-words text-xs text-zinc-200 leading-relaxed max-h-60 overflow-y-auto">
							{pr.body}
						</div>
					{:else}
						<p class="italic text-xs text-zinc-500">No description provided.</p>
					{/if}
				</div>

				<!-- Conversation Timeline -->
				<div class="space-y-2.5">
					<h3 class="text-xs font-semibold uppercase tracking-wider text-zinc-400">
						Timeline ({timeline.length})
					</h3>
					{#if timeline.length === 0}
						<p class="rounded-lg border border-zinc-800/60 bg-zinc-900/30 p-3 text-xs text-zinc-500">
							No comments or reviews yet.
						</p>
					{:else}
						{#each timeline as item (item.id)}
							<div class="rounded-lg border border-zinc-800 bg-zinc-900/50 p-3 space-y-1">
								<div class="flex items-center justify-between gap-2 text-[11px]">
									<div class="flex items-center gap-1.5">
										<span class="font-semibold text-zinc-200">@{item.author}</span>
										{#if item.kind === 'review'}
											<span class="rounded px-1.5 py-0.2 text-[10px] font-medium {item.reviewState === 'APPROVED'
												? 'bg-emerald-500/15 text-emerald-300'
												: item.reviewState === 'CHANGES_REQUESTED'
													? 'bg-amber-500/15 text-amber-300'
													: 'bg-zinc-800 text-zinc-300'}">
												{item.reviewState}
											</span>
										{/if}
									</div>
									<span class="text-zinc-500">{formatTimestamp(item.timestamp)}</span>
								</div>
								{#if item.body}
									<div class="whitespace-pre-wrap break-words text-xs text-zinc-300">
										{item.body}
									</div>
								{/if}
							</div>
						{/each}
					{/if}
				</div>

				<!-- Post Comment Box -->
				<div class="rounded-xl border border-zinc-800 bg-zinc-900/60 p-3 space-y-2">
					<textarea
						bind:value={commentText}
						rows="3"
						placeholder="Leave a comment on this pull request…"
						class="w-full rounded-md border border-zinc-800 bg-zinc-950 p-2 text-xs text-zinc-100 outline-none focus:border-indigo-500"
					></textarea>
					<div class="flex items-center justify-end">
						<button
							type="button"
							onclick={() => void handlePostComment()}
							disabled={githubStore.submittingComment || !commentText.trim()}
							class="inline-flex items-center gap-1.5 rounded-md border border-indigo-500/40 bg-indigo-600/20 px-3 py-1.5 text-xs font-medium text-indigo-200 hover:bg-indigo-600/30 disabled:opacity-50 cursor-pointer"
						>
							{#if githubStore.submittingComment}
								<Loader2 class="h-3.5 w-3.5 animate-spin" />
							{:else}
								<Send class="h-3.5 w-3.5" />
							{/if}
							<span>Comment</span>
						</button>
					</div>
				</div>
			{/if}
		</div>

		<!-- Right Panel (7 cols on lg, 8 on xl): Changed Files Tree + Unified/Split Diff Viewer -->
		<div class="flex min-h-0 flex-col overflow-hidden lg:col-span-7 xl:col-span-8 bg-zinc-950">
			<!-- Top Files & Review Action Bar -->
			<div class="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-zinc-800/80 bg-zinc-900/60 px-4 py-2">
				<div class="flex items-center gap-2">
					<div class="inline-flex rounded-md border border-zinc-700 bg-zinc-950 p-0.5 text-xs">
						<button
							type="button"
							onclick={() => (githubStore.diffViewMode = 'unified')}
							class="inline-flex items-center gap-1 rounded px-2.5 py-1 cursor-pointer {githubStore.diffViewMode ===
							'unified'
								? 'bg-indigo-600/30 text-indigo-200 font-medium'
								: 'text-zinc-400 hover:text-zinc-200'}"
						>
							<Rows2 class="h-3.5 w-3.5" />
							<span>Unified</span>
						</button>
						<button
							type="button"
							onclick={() => (githubStore.diffViewMode = 'split')}
							class="inline-flex items-center gap-1 rounded px-2.5 py-1 cursor-pointer {githubStore.diffViewMode ===
							'split'
								? 'bg-indigo-600/30 text-indigo-200 font-medium'
								: 'text-zinc-400 hover:text-zinc-200'}"
						>
							<Columns2 class="h-3.5 w-3.5" />
							<span>Split</span>
						</button>
					</div>

					{#if selectedFileFilter}
						<span class="rounded bg-zinc-800 px-2 py-0.5 font-mono text-xs text-zinc-300">
							{selectedFileFilter}
						</span>
					{/if}
				</div>

				<!-- Review Submission Popover Trigger & Draft Comments Tray -->
				<div class="relative flex items-center gap-2">
					{#if githubStore.draftReviewComments.length > 0}
						<button
							type="button"
							onclick={() => (showDraftTray = !showDraftTray)}
							class="inline-flex items-center gap-1 rounded-md border border-amber-500/40 bg-amber-500/15 px-2.5 py-1 text-xs font-medium text-amber-200 hover:bg-amber-500/25 cursor-pointer"
						>
							<span>{githubStore.draftReviewComments.length} pending comments</span>
						</button>
					{/if}

					<button
						type="button"
						onclick={() => (showReviewPopover = !showReviewPopover)}
						class="inline-flex items-center gap-1.5 rounded-lg border border-indigo-500/40 bg-indigo-600 px-3 py-1 text-xs font-semibold text-white hover:bg-indigo-500 cursor-pointer shadow-sm"
					>
						<ShieldCheck class="h-3.5 w-3.5" />
						<span>Review changes</span>
					</button>

					<!-- Submit Review Popover -->
					{#if showReviewPopover}
						<div class="absolute right-0 top-full z-30 mt-2 w-80 rounded-xl border border-zinc-700 bg-zinc-900 p-3.5 shadow-2xl space-y-3">
							<div class="flex items-center justify-between">
								<span class="text-xs font-semibold text-zinc-100">Submit Review</span>
								<span class="text-[11px] text-zinc-400">
									{githubStore.draftReviewComments.length} inline comment(s)
								</span>
							</div>

							<textarea
								bind:value={reviewBody}
								rows="3"
								placeholder="Leave a review summary comment…"
								class="w-full rounded-md border border-zinc-700 bg-zinc-950 p-2 text-xs text-zinc-100 outline-none focus:border-indigo-500"
							></textarea>

							<div class="space-y-2 text-xs">
								<label class="flex items-center gap-2 cursor-pointer">
									<input type="radio" bind:group={reviewEvent} value="COMMENT" />
									<span class="text-zinc-200">Comment (General feedback)</span>
								</label>

								<label class="flex items-center gap-2 {isSelfAuthored ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer'}">
									<input
										type="radio"
										bind:group={reviewEvent}
										value="APPROVE"
										disabled={isSelfAuthored}
									/>
									<span class="text-emerald-300 font-medium">Approve changes</span>
									{#if isSelfAuthored}
										<span class="inline-flex items-center gap-0.5 text-[10px] text-amber-300 font-mono">
											<Lock class="h-2.5 w-2.5" /> Cannot approve own PR
										</span>
									{/if}
								</label>

								<label class="flex items-center gap-2 {isSelfAuthored ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer'}">
									<input
										type="radio"
										bind:group={reviewEvent}
										value="REQUEST_CHANGES"
										disabled={isSelfAuthored}
									/>
									<span class="text-amber-300 font-medium">Request changes</span>
									{#if isSelfAuthored}
										<span class="inline-flex items-center gap-0.5 text-[10px] text-amber-300 font-mono">
											<Lock class="h-2.5 w-2.5" /> Cannot request changes on own PR
										</span>
									{/if}
								</label>
							</div>

							<div class="flex items-center justify-end gap-2 pt-1 border-t border-zinc-800">
								<button
									type="button"
									onclick={() => (showReviewPopover = false)}
									class="rounded-md border border-zinc-700 px-2.5 py-1 text-xs text-zinc-300 hover:bg-zinc-800 cursor-pointer"
								>
									Cancel
								</button>
								<button
									type="button"
									onclick={() => void handleSubmitReview()}
									disabled={githubStore.submittingReview || (isSelfAuthored && reviewEvent !== 'COMMENT')}
									class="inline-flex items-center gap-1 rounded-md bg-indigo-600 px-3 py-1 text-xs font-semibold text-white hover:bg-indigo-500 disabled:opacity-50 cursor-pointer"
								>
									{#if githubStore.submittingReview}
										<Loader2 class="h-3.5 w-3.5 animate-spin" />
									{/if}
									<span>Submit</span>
								</button>
							</div>
						</div>
					{/if}
				</div>
			</div>

			<!-- Pending Inline Review Draft Tray -->
			{#if showDraftTray && githubStore.draftReviewComments.length > 0}
				<div class="shrink-0 border-b border-amber-500/30 bg-amber-950/20 px-3 py-2 space-y-1.5 max-h-36 overflow-y-auto">
					<div class="flex items-center justify-between text-[11px] text-amber-200">
						<span class="font-semibold">Pending Inline Review Comments</span>
						<button
							type="button"
							onclick={() => githubStore.clearDraftReviewComments()}
							class="text-rose-300 hover:underline cursor-pointer"
						>
							Clear all
						</button>
					</div>
					{#each githubStore.draftReviewComments as draft, idx (draft.id ?? idx)}
						<div class="flex items-start justify-between gap-2 rounded border border-zinc-800 bg-zinc-900/80 px-2.5 py-1 text-xs">
							<div class="min-w-0">
								<div class="font-mono text-[10px] text-amber-300">
									{draft.path}:{draft.line} ({draft.side})
								</div>
								<div class="text-zinc-200 break-words">{draft.body}</div>
							</div>
							<button
								type="button"
								onclick={() => githubStore.removeDraftReviewComment(idx)}
								class="text-zinc-400 hover:text-rose-400 cursor-pointer"
								title="Remove draft comment"
							>
								<Trash2 class="h-3 w-3" />
							</button>
						</div>
					{/each}
				</div>
			{/if}

			<!-- Inner Split: Left = Interactive File Tree; Right = Diff Lines with Inline Commenting -->
			<div class="grid min-h-0 flex-1 grid-cols-1 md:grid-cols-12 divide-y md:divide-y-0 md:divide-x divide-zinc-800/80 overflow-hidden">
				<!-- File Tree Sidebar (3 cols on md/lg) -->
				<div class="md:col-span-4 lg:col-span-3 min-h-0 overflow-hidden">
					<PrDiffFileTree
						files={githubStore.selectedPrDiff}
						selectedPath={selectedFileFilter}
						onSelectFile={(path: string | null) => (selectedFileFilter = path)}
					/>
				</div>

				<!-- Diff Code View (9 cols on md/lg) -->
				<div class="md:col-span-8 lg:col-span-9 min-h-0 overflow-y-auto p-4 space-y-4">
					{#if githubStore.loadingDiff && githubStore.selectedPrDiff.length === 0}
						<div class="flex items-center justify-center py-16 text-xs text-zinc-400 gap-2">
							<Loader2 class="h-5 w-5 animate-spin text-indigo-400" />
							<span>Loading pull request diffs…</span>
						</div>
					{:else if visibleDiffFiles.length === 0}
						<div class="rounded-xl border border-zinc-800/60 bg-zinc-900/30 p-8 text-center text-xs text-zinc-500">
							No changed files match this filter.
						</div>
					{:else}
						{#each visibleDiffFiles as file (file.path)}
							<div class="overflow-hidden rounded-xl border border-zinc-800 bg-zinc-950">
								<!-- File Card Header -->
								<div class="flex items-center justify-between gap-2 border-b border-zinc-800 bg-zinc-900/80 px-3 py-2 text-xs">
									<div class="flex min-w-0 items-center gap-2">
										<FileCode2 class="h-3.5 w-3.5 shrink-0 text-indigo-400" />
										<span class="truncate font-mono font-medium text-zinc-100">{file.path}</span>
										<span class="rounded border border-zinc-700 bg-zinc-800 px-1.5 py-0.2 text-[10px] uppercase text-zinc-300">
											{file.status}
										</span>
									</div>
									<div class="shrink-0 font-mono text-[11px]">
										<span class="text-emerald-400">+{file.additions}</span>
										<span class="text-rose-400 ml-1.5">-{file.deletions}</span>
									</div>
								</div>

								{#if file.isBinary}
									<div class="p-4 text-xs italic text-zinc-500">Binary file modified.</div>
								{:else if githubStore.diffViewMode === 'unified'}
									<!-- Unified Diff View -->
									<div class="overflow-x-auto font-mono text-[11px]">
										{#each file.hunks as hunk, hIdx (`${file.path}-hunk-${hIdx}`)}
											<div class="bg-indigo-950/30 px-3 py-1 text-[10px] text-indigo-300 border-y border-zinc-800/60 select-none">
												{hunk.header}
											</div>
											{#each hunk.lines as line, lIdx (`${file.path}-${hIdx}-${lIdx}`)}
												{@const targetSide = line.kind === 'del' ? 'LEFT' : 'RIGHT'}
												{@const targetLine = line.kind === 'del' ? line.oldLine : line.newLine}
												{@const existing = existingCommentsForLine(file.path, targetLine, targetSide)}
												{@const drafts = draftCommentsForLine(file.path, targetLine, targetSide)}
												{@const isComposing =
													activeInlineTarget?.path === file.path &&
													activeInlineTarget?.line === targetLine &&
													activeInlineTarget?.side === targetSide}

												<div
													role="button"
													tabindex="0"
													onclick={() => openInlineComposer(file.path, targetLine, targetSide)}
													onkeydown={(e) => {
														if (e.key === 'Enter') openInlineComposer(file.path, targetLine, targetSide);
													}}
													class="group flex items-start px-2 py-0.5 cursor-pointer select-text {line.kind ===
													'add'
														? 'bg-emerald-950/30 text-emerald-100 hover:bg-emerald-900/40'
														: line.kind === 'del'
															? 'bg-rose-950/30 text-rose-100 hover:bg-rose-900/40'
															: 'text-zinc-300 hover:bg-zinc-900'}"
												>
													<span class="w-10 shrink-0 select-none text-right pr-2 text-zinc-600">
														{line.oldLine ?? ''}
													</span>
													<span class="w-10 shrink-0 select-none text-right pr-2 text-zinc-600">
														{line.newLine ?? ''}
													</span>
													<span class="w-5 shrink-0 select-none text-center text-indigo-400 opacity-0 group-hover:opacity-100">
														<MessageSquarePlus class="h-3 w-3 inline" />
													</span>
													<span class="w-4 shrink-0 select-none">
														{line.kind === 'add' ? '+' : line.kind === 'del' ? '-' : ' '}
													</span>
													<span class="whitespace-pre break-all">{line.content}</span>
												</div>

												{#each existing as c (c.id)}
													<div class="border-y border-zinc-800 bg-zinc-900/90 px-4 py-2 font-sans text-xs">
														<div class="flex items-center justify-between text-[11px] text-zinc-400">
															<span class="font-semibold text-zinc-200">@{c.user?.login ?? 'reviewer'}</span>
															<span>{formatTimestamp(c.created_at)}</span>
														</div>
														<div class="mt-1 whitespace-pre-wrap text-zinc-200">{c.body}</div>
													</div>
												{/each}

												{#each drafts as item (item.draft.id ?? item.index)}
													<div class="border-y border-amber-500/30 bg-amber-950/25 px-4 py-2 font-sans text-xs">
														<div class="flex items-center justify-between text-[11px] text-amber-300">
															<span class="font-semibold">Pending review comment (line {item.draft.line})</span>
															<button
																type="button"
																onclick={() => githubStore.removeDraftReviewComment(item.index)}
																class="text-rose-300 hover:underline cursor-pointer"
															>
																Remove
															</button>
														</div>
														<div class="mt-1 whitespace-pre-wrap text-zinc-100">{item.draft.body}</div>
													</div>
												{/each}

												{#if isComposing}
													<div class="border-y border-indigo-500/40 bg-zinc-900 p-3 font-sans space-y-2">
														<div class="text-[11px] text-indigo-300">
															Add inline comment on <span class="font-mono">{file.path}:{targetLine}</span> ({targetSide})
														</div>
														<textarea
															bind:value={inlineDraftBody}
															rows="2"
															placeholder="Write inline review feedback…"
															class="w-full rounded border border-zinc-700 bg-zinc-950 p-2 text-xs text-zinc-100 outline-none focus:border-indigo-500"
														></textarea>
														<div class="flex justify-end gap-2">
															<button
																type="button"
																onclick={() => (activeInlineTarget = null)}
																class="rounded border border-zinc-700 px-2.5 py-1 text-xs text-zinc-300 hover:bg-zinc-800 cursor-pointer"
															>
																Cancel
															</button>
															<button
																type="button"
																onclick={submitInlineDraft}
																disabled={!inlineDraftBody.trim()}
																class="rounded bg-indigo-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-indigo-500 disabled:opacity-50 cursor-pointer"
															>
																Add to Review
															</button>
														</div>
													</div>
												{/if}
											{/each}
										{/each}
									</div>
								{:else}
									<!-- Split Diff View -->
									<div class="overflow-x-auto font-mono text-[11px]">
										{#each file.hunks as hunk, hIdx (`${file.path}-split-hunk-${hIdx}`)}
											<div class="bg-indigo-950/30 px-3 py-1 text-[10px] text-indigo-300 border-y border-zinc-800/60 select-none">
												{hunk.header}
											</div>
											{#each buildSplitRows(hunk) as row, rIdx (`${file.path}-${hIdx}-row-${rIdx}`)}
												<div class="grid grid-cols-2 divide-x divide-zinc-800/80">
													<!-- Left (Old) -->
													<div
														role="button"
														tabindex="0"
														onclick={() =>
															row.left?.oldLine &&
															openInlineComposer(file.path, row.left.oldLine, 'LEFT')}
														onkeydown={(e) => {
															if (e.key === 'Enter' && row.left?.oldLine) {
																openInlineComposer(file.path, row.left.oldLine, 'LEFT');
															}
														}}
														class="flex items-start px-2 py-0.5 cursor-pointer {row.left?.kind === 'del'
															? 'bg-rose-950/30 text-rose-100 hover:bg-rose-900/40'
															: 'text-zinc-300 hover:bg-zinc-900'}"
													>
														<span class="w-8 shrink-0 select-none text-right pr-2 text-zinc-600">
															{row.left?.oldLine ?? ''}
														</span>
														<span class="w-3 shrink-0 select-none">
															{row.left?.kind === 'del' ? '-' : ' '}
														</span>
														<span class="whitespace-pre break-all">{row.left?.content ?? ''}</span>
													</div>

													<!-- Right (New) -->
													<div
														role="button"
														tabindex="0"
														onclick={() =>
															row.right?.newLine &&
															openInlineComposer(file.path, row.right.newLine, 'RIGHT')}
														onkeydown={(e) => {
															if (e.key === 'Enter' && row.right?.newLine) {
																openInlineComposer(file.path, row.right.newLine, 'RIGHT');
															}
														}}
														class="flex items-start px-2 py-0.5 cursor-pointer {row.right?.kind === 'add'
															? 'bg-emerald-950/30 text-emerald-100 hover:bg-emerald-900/40'
															: 'text-zinc-300 hover:bg-zinc-900'}"
													>
														<span class="w-8 shrink-0 select-none text-right pr-2 text-zinc-600">
															{row.right?.newLine ?? ''}
														</span>
														<span class="w-3 shrink-0 select-none">
															{row.right?.kind === 'add' ? '+' : ' '}
														</span>
														<span class="whitespace-pre break-all">{row.right?.content ?? ''}</span>
													</div>
												</div>
											{/each}
										{/each}
									</div>
								{/if}
							</div>
						{/each}
					{/if}
				</div>
			</div>
		</div>
	</div>
</div>
