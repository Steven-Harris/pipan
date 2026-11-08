<script lang="ts">
	import { onMount } from 'svelte';
	import { openUrl } from '@tauri-apps/plugin-opener';
	import {
		GitPullRequest,
		GitMerge,
		GitBranch,
		RefreshCw,
		Search,
		Plus,
		Pencil,
		Trash2,
		CheckCircle2,
		XCircle,
		Clock,
		KeyRound,
		Globe,
		LogOut,
		Radio,
		Copy,
		Check,
		Loader2,
		ShieldCheck,
		AlertTriangle,
		Filter
	} from 'lucide-svelte';
	import { githubStore } from '#lib/stores/github.svelte.ts';
	import type { GhPullRequest, GhStatusCheckItem, GithubView } from '#lib/types/github.ts';

	let repoInput = $state('');
	let editingRepo = $state(false);

	// Custom view builder state
	let showViewEditor = $state(false);
	let editingViewId = $state<string | undefined>(undefined);
	let viewNameInput = $state('');
	let viewQueryInput = $state('');

	// Auth form state
	let tokenInput = $state('');
	let showTokenInput = $state(false);
	let copiedCode = $state(false);

	onMount(() => {
		void githubStore.init();
	});

	function startNewView(prefillQuery?: string) {
		editingViewId = undefined;
		viewNameInput = '';
		viewQueryInput = prefillQuery ?? githubStore.searchQuery;
		showViewEditor = true;
	}

	function startEditView(view: GithubView) {
		if (view.builtIn) return;
		editingViewId = view.id;
		viewNameInput = view.name;
		viewQueryInput = view.query;
		showViewEditor = true;
	}

	async function handleSaveView() {
		if (!viewNameInput.trim() || !viewQueryInput.trim()) return;
		const saved = await githubStore.saveCustomView({
			id: editingViewId,
			name: viewNameInput,
			query: viewQueryInput
		});
		if (saved) {
			showViewEditor = false;
			editingViewId = undefined;
			viewNameInput = '';
		}
	}

	function commitRepoOverride() {
		editingRepo = false;
		githubStore.setRepoOverride(repoInput);
	}

	async function copyAuthCode(code: string, uri: string) {
		try {
			await navigator.clipboard.writeText(code);
			copiedCode = true;
			setTimeout(() => (copiedCode = false), 2500);
		} catch {
			// Ignore clipboard failures
		}
		try {
			await openUrl(uri);
		} catch {
			window.open(uri, '_blank', 'noopener,noreferrer');
		}
	}

	function summarizeRollup(rollup?: GhStatusCheckItem[]): 'pass' | 'fail' | 'pending' | null {
		if (!Array.isArray(rollup) || rollup.length === 0) return null;
		let hasPending = false;
		for (const item of rollup) {
			const conclusion = (item.conclusion ?? item.state ?? '').toUpperCase();
			const status = (item.status ?? '').toUpperCase();
			if (
				conclusion === 'FAILURE' ||
				conclusion === 'ERROR' ||
				conclusion === 'TIMED_OUT' ||
				conclusion === 'ACTION_REQUIRED'
			) {
				return 'fail';
			}
			if (
				status === 'IN_PROGRESS' ||
				status === 'QUEUED' ||
				status === 'PENDING' ||
				conclusion === 'PENDING'
			) {
				hasPending = true;
			}
		}
		return hasPending ? 'pending' : 'pass';
	}

	function formatRelativeTime(iso?: string): string {
		if (!iso) return '';
		const ts = Date.parse(iso);
		if (!Number.isFinite(ts)) return '';
		const diffSec = Math.max(0, Math.floor((Date.now() - ts) / 1000));
		if (diffSec < 60) return 'just now';
		const mins = Math.floor(diffSec / 60);
		if (mins < 60) return `${mins}m ago`;
		const hours = Math.floor(mins / 60);
		if (hours < 24) return `${hours}h ago`;
		const days = Math.floor(hours / 24);
		return `${days}d ago`;
	}

	function prStateBadge(pr: GhPullRequest) {
		if (pr.state === 'MERGED') {
			return {
				label: 'Merged',
				cls: 'border-purple-500/30 bg-purple-500/15 text-purple-300'
			};
		}
		if (pr.state === 'CLOSED') {
			return {
				label: 'Closed',
				cls: 'border-rose-500/30 bg-rose-500/15 text-rose-300'
			};
		}
		if (pr.isDraft) {
			return {
				label: 'Draft',
				cls: 'border-zinc-700 bg-zinc-800 text-zinc-300'
			};
		}
		return {
			label: 'Open',
			cls: 'border-emerald-500/30 bg-emerald-500/15 text-emerald-300'
		};
	}

	const isForwarderLive = $derived(githubStore.forwarder?.state === 'running');
</script>

<div class="flex h-full min-h-0 flex-col overflow-hidden bg-zinc-950 text-zinc-100">
	<!-- Top Bar: Repo & Webhook status & Refresh -->
	<div class="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-zinc-800/80 bg-zinc-900/70 px-3 py-2">
		<div class="flex min-w-0 items-center gap-1.5">
			<GitPullRequest class="h-4 w-4 shrink-0 text-indigo-400" />
			{#if editingRepo}
				<input
					type="text"
					bind:value={repoInput}
					placeholder="owner/repo"
					onkeydown={(e) => {
						if (e.key === 'Enter') commitRepoOverride();
						if (e.key === 'Escape') editingRepo = false;
					}}
					onblur={commitRepoOverride}
					aria-label="Repository slug override"
					class="w-36 rounded border border-indigo-500/60 bg-zinc-950 px-2 py-0.5 font-mono text-xs text-zinc-100 outline-none"
				/>
			{:else}
				<button
					type="button"
					onclick={() => {
						repoInput = githubStore.effectiveRepo ?? '';
						editingRepo = true;
					}}
					class="inline-flex max-w-[150px] items-center gap-1 truncate rounded border border-zinc-800 bg-zinc-900 px-2 py-0.5 font-mono text-[11px] text-zinc-200 hover:border-zinc-700 cursor-pointer"
					title="Click to set owner/repo override (leave blank for all user repos)"
				>
					<span class="truncate">{githubStore.effectiveRepo ?? 'All Repos'}</span>
					<Pencil class="h-2 w-2 shrink-0 text-zinc-500" />
				</button>
			{/if}

			<!-- Webhook Live Forwarder Toggle -->
			<button
				type="button"
				onclick={() => void githubStore.toggleWebhookForwarder()}
				disabled={githubStore.forwarderBusy}
				class="inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10px] font-medium transition cursor-pointer {isForwarderLive
					? 'border-emerald-500/40 bg-emerald-500/15 text-emerald-300'
					: githubStore.forwarder?.state === 'starting'
						? 'border-amber-500/40 bg-amber-500/15 text-amber-300'
						: 'border-zinc-800 bg-zinc-900 text-zinc-400 hover:text-zinc-200'}"
				title="Toggle gh webhook forward live event stream"
			>
				<Radio class="h-2.5 w-2.5 {isForwarderLive ? 'animate-pulse text-emerald-400' : ''}" />
				<span>{isForwarderLive ? 'Live' : 'Off'}</span>
			</button>
		</div>

		<div class="flex items-center gap-1">
			{#if githubStore.status?.authenticated && githubStore.status.account}
				<span class="max-w-[80px] truncate text-[10px] text-zinc-400" title={`@${githubStore.status.account}`}>
					@{githubStore.status.account}
				</span>
				<button
					type="button"
					onclick={() => void githubStore.logout()}
					disabled={githubStore.authBusy}
					class="inline-flex h-6 w-6 items-center justify-center rounded border border-zinc-800 bg-zinc-900 text-zinc-400 hover:border-zinc-700 hover:text-rose-300 cursor-pointer"
					title="Sign out of GitHub CLI"
					aria-label="Sign out of GitHub CLI"
				>
					<LogOut class="h-3 w-3" />
				</button>
			{/if}

			<button
				type="button"
				onclick={() => void githubStore.refreshPrList()}
				disabled={githubStore.loadingList}
				class="inline-flex h-6 w-6 items-center justify-center rounded border border-zinc-800 bg-zinc-900 text-zinc-300 hover:border-zinc-700 hover:bg-zinc-800 disabled:opacity-50 cursor-pointer"
				title="Refresh pull requests"
				aria-label="Refresh pull requests"
			>
				<RefreshCw class="h-3 w-3 {githubStore.loadingList ? 'animate-spin' : ''}" />
			</button>
		</div>
	</div>

	<!-- Authentication Banner if gh is unauthenticated or missing -->
	{#if githubStore.status && (!githubStore.status.installed || !githubStore.status.authenticated)}
		<div class="m-2 rounded-xl border border-amber-500/30 bg-amber-950/20 p-2.5 space-y-2 text-xs">
			{#if !githubStore.status.installed}
				<div class="font-semibold text-amber-200">GitHub CLI (`gh`) not detected</div>
				<p class="text-zinc-300 text-[11px]">
					Install `gh` from <span class="font-mono text-indigo-300">https://cli.github.com</span>.
				</p>
			{:else}
				<div class="flex items-center justify-between text-[11px]">
					<span class="font-semibold text-amber-200">GitHub Login Required</span>
					<span class="text-zinc-400">gh v{githubStore.status.version ?? ''}</span>
				</div>

				{#if githubStore.authCode}
					<div class="rounded-lg border border-indigo-500/40 bg-indigo-950/30 p-2 space-y-1">
						<div class="text-[10px] text-indigo-200">Device code:</div>
						<div class="flex items-center justify-between gap-1.5">
							<span class="rounded bg-zinc-950 px-2 py-0.5 font-mono text-xs font-bold text-indigo-300">
								{githubStore.authCode.code}
							</span>
							<button
								type="button"
								onclick={() =>
									githubStore.authCode &&
									void copyAuthCode(githubStore.authCode.code, githubStore.authCode.verificationUri)}
								class="inline-flex items-center gap-1 rounded bg-indigo-600 px-2 py-0.5 text-[11px] font-medium text-white hover:bg-indigo-500 cursor-pointer"
							>
								{#if copiedCode}
									<Check class="h-3 w-3" />
								{:else}
									<Copy class="h-3 w-3" />
								{/if}
								<span>Copy</span>
							</button>
						</div>
					</div>
				{/if}

				<div class="flex flex-wrap items-center gap-1.5">
					<button
						type="button"
						onclick={() => void githubStore.loginWithWeb()}
						disabled={githubStore.authBusy}
						class="inline-flex items-center gap-1 rounded-md border border-indigo-500/40 bg-indigo-600/20 px-2.5 py-1 text-xs font-medium text-indigo-100 hover:bg-indigo-600/30 disabled:opacity-50 cursor-pointer"
					>
						{#if githubStore.authBusy}
							<Loader2 class="h-3 w-3 animate-spin" />
						{:else}
							<Globe class="h-3 w-3" />
						{/if}
						<span>Browser Login</span>
					</button>

					<button
						type="button"
						onclick={() => (showTokenInput = !showTokenInput)}
						class="inline-flex items-center gap-1 rounded-md border border-zinc-700 bg-zinc-800 px-2 py-1 text-xs text-zinc-300 hover:bg-zinc-700 cursor-pointer"
					>
						<KeyRound class="h-3 w-3" />
						<span>Token</span>
					</button>
				</div>

				{#if showTokenInput}
					<div class="flex items-center gap-1.5 pt-1">
						<input
							type="password"
							bind:value={tokenInput}
							placeholder="ghp_… or github_pat_…"
							class="flex-1 rounded border border-zinc-700 bg-zinc-950 px-2 py-1 text-xs text-zinc-100 outline-none focus:border-indigo-500"
						/>
						<button
							type="button"
							onclick={async () => {
								const ok = await githubStore.loginWithToken(tokenInput);
								if (ok) {
									tokenInput = '';
									showTokenInput = false;
								}
							}}
							disabled={githubStore.authBusy || !tokenInput.trim()}
							class="rounded bg-indigo-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-indigo-500 disabled:opacity-50 cursor-pointer"
						>
							Save
						</button>
					</div>
				{/if}
			{/if}
		</div>
	{/if}

	<!-- Views & Filters Header -->
	<div class="flex shrink-0 flex-col gap-2 border-b border-zinc-800/80 bg-zinc-900/30 px-3 py-2">
		<!-- Saved Views Horizontal Scroll -->
		<div class="flex flex-wrap items-center gap-1">
			{#each githubStore.allViews as view (view.id)}
				{@const active = githubStore.activeViewId === view.id}
				<div
					class="inline-flex items-center rounded-md border text-[11px] transition {active
						? 'border-indigo-500/40 bg-indigo-500/15 text-indigo-100'
						: 'border-zinc-800 bg-zinc-900/80 text-zinc-400 hover:border-zinc-700 hover:text-zinc-200'}"
				>
					<button
						type="button"
						onclick={() => githubStore.selectView(view.id)}
						class="px-2 py-0.5 font-medium cursor-pointer"
						title={view.query}
					>
						{view.name}
					</button>
					{#if !view.builtIn}
						<button
							type="button"
							onclick={() => startEditView(view)}
							class="p-0.5 text-zinc-500 hover:text-zinc-200 cursor-pointer"
							title="Edit view"
							aria-label="Edit {view.name}"
						>
							<Pencil class="h-2 w-2" />
						</button>
						<button
							type="button"
							onclick={() => void githubStore.deleteCustomView(view.id)}
							class="p-0.5 text-zinc-500 hover:text-rose-400 cursor-pointer"
							title="Delete view"
							aria-label="Delete {view.name}"
						>
							<Trash2 class="h-2 w-2" />
						</button>
					{/if}
				</div>
			{/each}

			<button
				type="button"
				onclick={() => startNewView()}
				class="inline-flex items-center gap-0.5 rounded border border-dashed border-zinc-700 px-1.5 py-0.5 text-[11px] text-zinc-400 hover:border-zinc-600 hover:text-zinc-200 cursor-pointer"
				title="Create a custom saved PR search view"
			>
				<Plus class="h-2.5 w-2.5" />
				<span>View</span>
			</button>
		</div>

		<!-- Custom View Builder Form -->
		{#if showViewEditor}
			<div class="rounded-lg border border-indigo-500/30 bg-zinc-900 p-2 space-y-1.5 text-xs">
				<div class="flex items-center justify-between font-medium text-indigo-200 text-[11px]">
					<span>{editingViewId ? 'Edit Custom View' : 'New Custom View'}</span>
				</div>
				<input
					type="text"
					bind:value={viewNameInput}
					placeholder="View name (e.g. Bug Fixes)"
					aria-label="Custom view name"
					class="w-full rounded border border-zinc-700 bg-zinc-950 px-2 py-1 text-xs text-zinc-100 outline-none focus:border-indigo-500"
				/>
				<input
					type="text"
					bind:value={viewQueryInput}
					placeholder="gh search query (e.g. is:open label:bug)"
					aria-label="Custom view query"
					class="w-full rounded border border-zinc-700 bg-zinc-950 px-2 py-1 font-mono text-xs text-zinc-100 outline-none focus:border-indigo-500"
				/>
				<div class="flex items-center justify-end gap-1.5 pt-1">
					<button
						type="button"
						onclick={() => (showViewEditor = false)}
						class="rounded border border-zinc-700 px-2 py-0.5 text-xs text-zinc-300 hover:bg-zinc-800 cursor-pointer"
					>
						Cancel
					</button>
					<button
						type="button"
						onclick={() => void handleSaveView()}
						disabled={!viewNameInput.trim() || !viewQueryInput.trim()}
						class="rounded bg-indigo-600 px-2.5 py-0.5 text-xs font-medium text-white hover:bg-indigo-500 disabled:opacity-50 cursor-pointer"
					>
						Save
					</button>
				</div>
			</div>
		{/if}

		<!-- Repository Filter Selector (when viewing cross-repo results) -->
		{#if githubStore.availableRepos.length > 1}
			<div class="flex items-center gap-1.5 text-xs">
				<Filter class="h-3 w-3 text-zinc-500 shrink-0" />
				<select
					bind:value={githubStore.listRepoFilter}
					aria-label="Filter pull requests by repository"
					class="w-full rounded border border-zinc-800 bg-zinc-950 px-2 py-1 font-mono text-[11px] text-zinc-300 outline-none focus:border-indigo-500"
				>
					<option value="all">All repositories ({githubStore.pullRequests.length})</option>
					{#each githubStore.availableRepos as repo (repo)}
						<option value={repo}>{repo}</option>
					{/each}
				</select>
			</div>
		{/if}

		<!-- Search Input Bar -->
		<div class="flex items-center gap-1">
			<div class="relative flex-1">
				<Search class="pointer-events-none absolute left-2 top-1/2 h-3 w-3 -translate-y-1/2 text-zinc-500" />
				<input
					type="text"
					bind:value={githubStore.searchQuery}
					onkeydown={(e) => {
						if (e.key === 'Enter') void githubStore.refreshPrList();
					}}
					placeholder="Search PRs (gh syntax)…"
					aria-label="Search pull requests"
					class="w-full rounded-md border border-zinc-800 bg-zinc-950 py-1 pl-7 pr-2 font-mono text-xs text-zinc-100 outline-none focus:border-indigo-500"
				/>
			</div>
			<button
				type="button"
				onclick={() => void githubStore.refreshPrList()}
				class="rounded-md border border-zinc-700 bg-zinc-800 px-2 py-1 text-xs font-medium text-zinc-200 hover:bg-zinc-700 cursor-pointer"
			>
				Run
			</button>
		</div>
	</div>

	{#if githubStore.listError}
		<div class="m-2 rounded-lg border border-rose-500/30 bg-rose-500/10 p-2.5 text-xs text-rose-200">
			{githubStore.listError}
		</div>
	{/if}

	<!-- PR List Items -->
	<div class="min-h-0 flex-1 overflow-y-auto divide-y divide-zinc-800/70">
		{#if githubStore.loadingList && githubStore.filteredPullRequests.length === 0}
			<div class="flex items-center justify-center py-10 text-xs text-zinc-400 gap-2">
				<Loader2 class="h-4 w-4 animate-spin text-indigo-400" />
				<span>Loading pull requests…</span>
			</div>
		{:else if githubStore.filteredPullRequests.length === 0}
			<div class="p-6 text-center">
				<GitPullRequest class="mx-auto mb-2 h-5 w-5 text-zinc-600" />
				<div class="text-xs font-medium text-zinc-300">No pull requests match</div>
				<div class="mt-1 font-mono text-[10px] text-zinc-500">
					{githubStore.searchQuery || 'is:open'}
				</div>
			</div>
		{:else}
			{#each githubStore.filteredPullRequests as pr (`${pr.repo ?? ''}-${pr.number}`)}
				{@const badge = prStateBadge(pr)}
				{@const ciState = summarizeRollup(pr.statusCheckRollup)}
				{@const isSelected = githubStore.selectedPrNumber === pr.number && (!pr.repo || pr.repo === githubStore.selectedPrRepo)}

				<button
					type="button"
					onclick={() => void githubStore.selectPr(pr.number, pr.repo)}
					class="flex w-full flex-col gap-1 px-3 py-2.5 text-left transition cursor-pointer {isSelected
						? 'bg-indigo-600/15 border-l-2 border-indigo-400'
						: 'hover:bg-zinc-900/70 border-l-2 border-transparent'}"
				>
					<div class="flex items-start justify-between gap-1.5">
						<div class="min-w-0 flex-1">
							<div class="flex flex-wrap items-center gap-1">
								{#if pr.repo}
									<span class="rounded bg-zinc-800 px-1 py-0.2 font-mono text-[10px] text-zinc-400">
										{pr.repo.split('/')[1] ?? pr.repo}
									</span>
								{/if}
								<span class="font-mono text-xs text-zinc-400">#{pr.number}</span>
								<span class="inline-flex items-center gap-0.5 rounded-full border px-1.5 py-0.2 text-[9px] font-semibold {badge.cls}">
									{#if pr.state === 'MERGED'}
										<GitMerge class="h-2 w-2" />
									{:else}
										<GitPullRequest class="h-2 w-2" />
									{/if}
									{badge.label}
								</span>
							</div>
							<div class="mt-0.5 font-medium text-xs text-zinc-100 leading-snug line-clamp-2">
								{pr.title}
							</div>
						</div>

						<!-- Status indicators -->
						<div class="flex shrink-0 items-center gap-1">
							{#if ciState === 'pass'}
								<span title="Checks passing" class="text-emerald-400">
									<CheckCircle2 class="h-3.5 w-3.5" />
								</span>
							{:else if ciState === 'fail'}
								<span title="Checks failing" class="text-rose-400">
									<XCircle class="h-3.5 w-3.5" />
								</span>
							{:else if ciState === 'pending'}
								<span title="Checks pending" class="text-amber-400">
									<Clock class="h-3.5 w-3.5" />
								</span>
							{/if}

							{#if pr.reviewDecision === 'APPROVED'}
								<span title="Approved" class="text-emerald-400">
									<ShieldCheck class="h-3.5 w-3.5" />
								</span>
							{:else if pr.reviewDecision === 'CHANGES_REQUESTED'}
								<span title="Changes requested" class="text-amber-400">
									<AlertTriangle class="h-3.5 w-3.5" />
								</span>
							{/if}
						</div>
					</div>

					<div class="flex items-center justify-between text-[10px] text-zinc-500 pt-0.5 font-mono">
						<span class="truncate max-w-[140px]">@{pr.author?.login ?? 'unknown'}</span>
						<div class="flex items-center gap-1.5">
							{#if typeof pr.additions === 'number' || typeof pr.deletions === 'number'}
								<span class="text-emerald-400">+{pr.additions ?? 0}</span>
								<span class="text-rose-400">-{pr.deletions ?? 0}</span>
							{/if}
							<span>{formatRelativeTime(pr.updatedAt)}</span>
						</div>
					</div>
				</button>
			{/each}
		{/if}
	</div>
</div>
