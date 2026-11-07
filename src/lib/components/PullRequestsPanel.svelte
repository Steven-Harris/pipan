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
		AlertTriangle
	} from 'lucide-svelte';
	import { githubStore } from '#lib/stores/github.svelte.ts';
	import type { GhPullRequest, GhStatusCheckItem, GithubView } from '#lib/types/github.ts';
	import PrDetailDrawer from './PrDetailDrawer.svelte';

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
</script>

{#if githubStore.selectedPrNumber !== null}
	<PrDetailDrawer />
{:else}
	<div class="flex h-full min-h-0 flex-col bg-zinc-950 text-zinc-100">
		<!-- Top Header: Repo Slug, Webhook Forwarder, Auth Status, Refresh -->
		<div class="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-zinc-800/80 bg-zinc-900/60 px-3 py-2">
			<div class="flex min-w-0 items-center gap-2">
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
						class="w-44 rounded border border-indigo-500/60 bg-zinc-950 px-2 py-0.5 font-mono text-xs text-zinc-100 outline-none"
					/>
				{:else}
					<button
						type="button"
						onclick={() => {
							repoInput = githubStore.effectiveRepo ?? '';
							editingRepo = true;
						}}
						class="inline-flex items-center gap-1.5 rounded border border-zinc-800 bg-zinc-900 px-2 py-0.5 font-mono text-xs text-zinc-200 hover:border-zinc-700 cursor-pointer"
						title="Click to set owner/repo override"
					>
						<span>{githubStore.effectiveRepo ?? 'Set owner/repo…'}</span>
						<Pencil class="h-2.5 w-2.5 text-zinc-400" />
					</button>
				{/if}

				<!-- Webhook Live Forwarder Toggle -->
				<button
					type="button"
					onclick={() => void githubStore.toggleWebhookForwarder()}
					disabled={githubStore.forwarderBusy}
					class="inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10px] font-medium transition cursor-pointer {githubStore
						.forwarder?.state === 'running'
						? 'border-emerald-500/40 bg-emerald-500/15 text-emerald-300'
						: githubStore.forwarder?.state === 'starting'
							? 'border-amber-500/40 bg-amber-500/15 text-amber-300'
							: 'border-zinc-800 bg-zinc-900 text-zinc-400 hover:text-zinc-200'}"
					title="Toggle `gh webhook forward` live event stream"
				>
					<Radio class="h-2.5 w-2.5" />
					<span>
						{githubStore.forwarder?.state === 'running'
							? 'Live Webhooks'
							: githubStore.forwarder?.state === 'starting'
								? 'Starting Webhooks…'
								: 'Live Off'}
					</span>
				</button>
			</div>

			<div class="flex items-center gap-1.5">
				{#if githubStore.status?.authenticated && githubStore.status.account}
					<span class="text-[11px] text-zinc-400">
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

		<!-- Authentication Banner if gh is missing or unauthenticated -->
		{#if githubStore.status && (!githubStore.status.installed || !githubStore.status.authenticated)}
			<div class="m-3 rounded-xl border border-amber-500/30 bg-amber-950/20 p-3 space-y-2.5 text-xs">
				{#if !githubStore.status.installed}
					<div class="font-semibold text-amber-200">GitHub CLI (`gh`) not detected</div>
					<p class="text-zinc-300">
						Install `gh` from <span class="font-mono text-indigo-300">https://cli.github.com</span> and click Refresh.
					</p>
				{:else}
					<div class="flex items-center justify-between">
						<span class="font-semibold text-amber-200">Authenticate with GitHub CLI</span>
						<span class="text-[11px] text-zinc-400">gh v{githubStore.status.version ?? ''}</span>
					</div>

					{#if githubStore.authCode}
						<div class="rounded-lg border border-indigo-500/40 bg-indigo-950/30 p-2.5 space-y-1.5">
							<div class="text-[11px] text-indigo-200">
								One-time device code for <strong>{githubStore.authCode.hostname}</strong>:
							</div>
							<div class="flex items-center justify-between gap-2">
								<span class="rounded bg-zinc-950 px-2.5 py-1 font-mono text-sm font-bold tracking-widest text-indigo-300">
									{githubStore.authCode.code}
								</span>
								<button
									type="button"
									onclick={() =>
										githubStore.authCode &&
										void copyAuthCode(
											githubStore.authCode.code,
											githubStore.authCode.verificationUri
										)}
									class="inline-flex items-center gap-1 rounded bg-indigo-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-indigo-500 cursor-pointer"
								>
									{#if copiedCode}
										<Check class="h-3.5 w-3.5" />
										<span>Copied & Opened</span>
									{:else}
										<Copy class="h-3.5 w-3.5" />
										<span>Copy & Open GitHub</span>
									{/if}
								</button>
							</div>
						</div>
					{/if}

					<div class="flex flex-wrap items-center gap-2">
						<button
							type="button"
							onclick={() => void githubStore.loginWithWeb()}
							disabled={githubStore.authBusy}
							class="inline-flex items-center gap-1.5 rounded-lg border border-indigo-500/40 bg-indigo-600/20 px-3 py-1.5 text-xs font-medium text-indigo-100 hover:bg-indigo-600/30 disabled:opacity-50 cursor-pointer"
						>
							{#if githubStore.authBusy}
								<Loader2 class="h-3.5 w-3.5 animate-spin" />
							{:else}
								<Globe class="h-3.5 w-3.5" />
							{/if}
							<span>Login with Browser</span>
						</button>

						<button
							type="button"
							onclick={() => (showTokenInput = !showTokenInput)}
							class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-1.5 text-xs font-medium text-zinc-200 hover:bg-zinc-700 cursor-pointer"
						>
							<KeyRound class="h-3.5 w-3.5" />
							<span>Use Token</span>
						</button>
					</div>

					{#if showTokenInput}
						<div class="flex items-center gap-2 pt-1">
							<input
								type="password"
								bind:value={tokenInput}
								placeholder="ghp_… or github_pat_…"
								class="flex-1 rounded-md border border-zinc-700 bg-zinc-950 px-2.5 py-1 text-xs text-zinc-100 outline-none focus:border-indigo-500"
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
								class="rounded-md bg-indigo-600 px-3 py-1 text-xs font-medium text-white hover:bg-indigo-500 disabled:opacity-50 cursor-pointer"
							>
								Save Token
							</button>
						</div>
					{/if}

					{#if githubStore.authError}
						<div class="text-rose-300 text-[11px]">{githubStore.authError}</div>
					{/if}
				{/if}
			</div>
		{/if}

		<!-- Saved Views Bar (Built-in + Persistent Custom Views) -->
		<div class="flex shrink-0 flex-col gap-2 border-b border-zinc-800/80 bg-zinc-900/30 px-3 py-2">
			<div class="flex flex-wrap items-center gap-1">
				{#each githubStore.allViews as view (view.id)}
					{@const active = githubStore.activeViewId === view.id}
					<div
						class="inline-flex items-center rounded-md border text-xs transition {active
							? 'border-indigo-500/40 bg-indigo-500/15 text-indigo-100'
							: 'border-zinc-800 bg-zinc-900/80 text-zinc-400 hover:border-zinc-700 hover:text-zinc-200'}"
					>
						<button
							type="button"
							onclick={() => githubStore.selectView(view.id)}
							class="px-2.5 py-1 font-medium cursor-pointer"
							title={view.query}
						>
							{view.name}
						</button>
						{#if !view.builtIn}
							<button
								type="button"
								onclick={() => startEditView(view)}
								class="p-1 text-zinc-500 hover:text-zinc-200 cursor-pointer"
								title="Edit custom view"
								aria-label="Edit {view.name}"
							>
								<Pencil class="h-2.5 w-2.5" />
							</button>
							<button
								type="button"
								onclick={() => void githubStore.deleteCustomView(view.id)}
								class="pr-1.5 pl-0.5 py-1 text-zinc-500 hover:text-rose-400 cursor-pointer"
								title="Delete custom view"
								aria-label="Delete {view.name}"
							>
								<Trash2 class="h-2.5 w-2.5" />
							</button>
						{/if}
					</div>
				{/each}

				<button
					type="button"
					onclick={() => startNewView()}
					class="inline-flex items-center gap-1 rounded-md border border-dashed border-zinc-700 px-2 py-1 text-xs text-zinc-400 hover:border-zinc-600 hover:text-zinc-200 cursor-pointer"
					title="Create a custom saved PR search view"
				>
					<Plus class="h-3 w-3" />
					<span>New View</span>
				</button>
			</div>

			<!-- Custom View Builder Form -->
			{#if showViewEditor}
				<div class="rounded-lg border border-indigo-500/30 bg-zinc-900 p-2.5 space-y-2">
					<div class="flex items-center justify-between text-xs font-medium text-indigo-200">
						<span>{editingViewId ? 'Edit Custom View' : 'New Custom View'}</span>
						<span class="text-[11px] text-zinc-400">Saved to pipan.db</span>
					</div>
					<div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
						<input
							type="text"
							bind:value={viewNameInput}
							placeholder="View name (e.g. Bug Fixes)"
							aria-label="Custom view name"
							class="rounded border border-zinc-700 bg-zinc-950 px-2.5 py-1 text-xs text-zinc-100 outline-none focus:border-indigo-500"
						/>
						<input
							type="text"
							bind:value={viewQueryInput}
							placeholder="gh search query (e.g. is:open label:bug draft:false)"
							aria-label="Custom view search query"
							class="rounded border border-zinc-700 bg-zinc-950 px-2.5 py-1 font-mono text-xs text-zinc-100 outline-none focus:border-indigo-500"
						/>
					</div>
					<div class="flex items-center justify-end gap-2">
						<button
							type="button"
							onclick={() => (showViewEditor = false)}
							class="rounded border border-zinc-700 px-2.5 py-1 text-xs text-zinc-300 hover:bg-zinc-800 cursor-pointer"
						>
							Cancel
						</button>
						<button
							type="button"
							onclick={() => void handleSaveView()}
							disabled={!viewNameInput.trim() || !viewQueryInput.trim()}
							class="rounded bg-indigo-600 px-3 py-1 text-xs font-medium text-white hover:bg-indigo-500 disabled:opacity-50 cursor-pointer"
						>
							Save View
						</button>
					</div>
				</div>
			{/if}

			<!-- Active Search Syntax Input -->
			<div class="flex items-center gap-1.5">
				<div class="relative flex-1">
					<Search class="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-zinc-500" />
					<input
						type="text"
						bind:value={githubStore.searchQuery}
						onkeydown={(e) => {
							if (e.key === 'Enter') void githubStore.refreshPrList();
						}}
						placeholder="Search PRs with gh syntax (e.g. is:open assignee:@me label:bug)…"
						aria-label="Search pull requests"
						class="w-full rounded-md border border-zinc-800 bg-zinc-950 py-1 pl-8 pr-2.5 font-mono text-xs text-zinc-100 outline-none focus:border-indigo-500"
					/>
				</div>
				<button
					type="button"
					onclick={() => void githubStore.refreshPrList()}
					class="rounded-md border border-zinc-700 bg-zinc-800 px-2.5 py-1 text-xs font-medium text-zinc-200 hover:bg-zinc-700 cursor-pointer"
				>
					Run
				</button>
				{#if githubStore.searchQuery.trim() && githubStore.searchQuery.trim() !== githubStore.activeView.query}
					<button
						type="button"
						onclick={() => startNewView(githubStore.searchQuery)}
						class="rounded-md border border-indigo-500/40 bg-indigo-500/15 px-2.5 py-1 text-xs font-medium text-indigo-200 hover:bg-indigo-500/25 cursor-pointer"
					>
						Save Query
					</button>
				{/if}
			</div>
		</div>

		{#if githubStore.listError}
			<div class="m-3 rounded-lg border border-rose-500/30 bg-rose-500/10 p-3 text-xs text-rose-200">
				{githubStore.listError}
			</div>
		{/if}

		<!-- Pull Requests List -->
		<div class="min-h-0 flex-1 overflow-y-auto divide-y divide-zinc-800/70">
			{#if githubStore.loadingList && githubStore.pullRequests.length === 0}
				<div class="flex items-center justify-center py-12 text-xs text-zinc-400 gap-2">
					<Loader2 class="h-4 w-4 animate-spin text-indigo-400" />
					<span>Querying GitHub pull requests…</span>
				</div>
			{:else if githubStore.pullRequests.length === 0}
				<div class="p-8 text-center">
					<GitPullRequest class="mx-auto mb-2 h-6 w-6 text-zinc-600" />
					<div class="text-xs font-medium text-zinc-300">No pull requests match this view</div>
					<div class="mt-1 font-mono text-[11px] text-zinc-500">
						{githubStore.searchQuery || 'is:open'}
					</div>
				</div>
			{:else}
				{#each githubStore.pullRequests as pr (pr.number)}
					{@const badge = prStateBadge(pr)}
					{@const ciState = summarizeRollup(pr.statusCheckRollup)}
					<button
						type="button"
						onclick={() => void githubStore.selectPr(pr.number)}
						class="flex w-full flex-col gap-1.5 px-3.5 py-2.5 text-left transition hover:bg-zinc-900/70 cursor-pointer"
					>
						<div class="flex items-start justify-between gap-2">
							<div class="min-w-0 flex-1">
								<div class="flex flex-wrap items-center gap-1.5">
									<span class="font-mono text-xs text-zinc-400">#{pr.number}</span>
									<span
										class="inline-flex items-center gap-1 rounded-full border px-2 py-0.2 text-[10px] font-semibold {badge.cls}"
									>
										{#if pr.state === 'MERGED'}
											<GitMerge class="h-2.5 w-2.5" />
										{:else}
											<GitPullRequest class="h-2.5 w-2.5" />
										{/if}
										{badge.label}
									</span>
									<span class="font-medium text-xs text-zinc-100 leading-snug">
										{pr.title}
									</span>
								</div>
							</div>

							<!-- CI & Review Status Icons -->
							<div class="flex shrink-0 items-center gap-1.5">
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
									<span
										class="inline-flex items-center gap-0.5 rounded border border-emerald-500/30 bg-emerald-500/10 px-1.5 py-0.2 text-[10px] text-emerald-300"
									>
										<ShieldCheck class="h-2.5 w-2.5" />
										Approved
									</span>
								{:else if pr.reviewDecision === 'CHANGES_REQUESTED'}
									<span
										class="inline-flex items-center gap-0.5 rounded border border-amber-500/30 bg-amber-500/10 px-1.5 py-0.2 text-[10px] text-amber-300"
									>
										<AlertTriangle class="h-2.5 w-2.5" />
										Changes
									</span>
								{/if}
							</div>
						</div>

						<div class="flex flex-wrap items-center justify-between gap-2 text-[11px] text-zinc-400">
							<div class="flex flex-wrap items-center gap-2">
								<span>@{pr.author?.login ?? 'unknown'}</span>
								<span class="inline-flex items-center gap-1 rounded bg-zinc-900 px-1.5 py-0.2 font-mono text-[10px] text-zinc-300">
									<GitBranch class="h-2.5 w-2.5 text-indigo-400" />
									{pr.headRefName} → {pr.baseRefName}
								</span>

								{#each pr.labels ?? [] as label (label.name)}
									<span class="rounded-full border border-zinc-700/80 bg-zinc-900 px-1.5 py-0.2 text-[10px] text-zinc-300">
										{label.name}
									</span>
								{/each}
							</div>

							<div class="flex items-center gap-2 font-mono text-[10px]">
								{#if typeof pr.additions === 'number' || typeof pr.deletions === 'number'}
									<span class="text-emerald-400">+{pr.additions ?? 0}</span>
									<span class="text-rose-400">-{pr.deletions ?? 0}</span>
								{/if}
								<span class="font-sans text-zinc-500">{formatRelativeTime(pr.updatedAt)}</span>
							</div>
						</div>
					</button>
				{/each}
			{/if}
		</div>
	</div>
{/if}
