import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { chatStore } from '#lib/stores/chat.svelte.ts';
import type {
	ForwarderInfo,
	GhAuthCodeEvent,
	GhCheck,
	GhPrReviewComment,
	GhPullRequest,
	GhPullRequestDetail,
	GhStatus,
	GithubView,
	PrDiffFile,
	PrMergeMethod,
	PrReviewDraftComment,
	PrReviewEvent,
	WebhookEvent
} from '#lib/types/github.ts';

export const BUILTIN_GITHUB_VIEWS: readonly GithubView[] = [
	{
		id: 'builtin:assigned',
		name: 'Assigned to Me',
		query: 'is:open assignee:@me',
		sortOrder: -4,
		builtIn: true
	},
	{
		id: 'builtin:review-requested',
		name: 'Review Requested',
		query: 'is:open review-requested:@me',
		sortOrder: -3,
		builtIn: true
	},
	{
		id: 'builtin:created',
		name: 'Created by Me',
		query: 'is:open author:@me',
		sortOrder: -2,
		builtIn: true
	},
	{
		id: 'builtin:all-open',
		name: 'All Open PRs',
		query: 'is:open',
		sortOrder: -1,
		builtIn: true
	}
];

function makeViewId(): string {
	if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
		return `view-${crypto.randomUUID()}`;
	}
	return `view-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

class GithubStore {
	status = $state<GhStatus | null>(null);
	authCode = $state<GhAuthCodeEvent | null>(null);
	authBusy = $state<boolean>(false);
	authError = $state<string | null>(null);

	repoOverride = $state<string>('');
	detectedRepo = $state<string | null>(null);

	customViews = $state<GithubView[]>([]);
	activeViewId = $state<string>('builtin:all-open');
	searchQuery = $state<string>('is:open');

	pullRequests = $state<GhPullRequest[]>([]);
	loadingList = $state<boolean>(false);
	listError = $state<string | null>(null);

	selectedPrNumber = $state<number | null>(null);
	selectedPrDetail = $state<GhPullRequestDetail | null>(null);
	selectedPrChecks = $state<GhCheck[]>([]);
	selectedPrDiff = $state<PrDiffFile[]>([]);
	selectedPrReviewComments = $state<GhPrReviewComment[]>([]);

	loadingDetail = $state<boolean>(false);
	loadingChecks = $state<boolean>(false);
	loadingDiff = $state<boolean>(false);
	detailError = $state<string | null>(null);

	activeDetailTab = $state<'overview' | 'checks' | 'files'>('overview');
	diffViewMode = $state<'unified' | 'split'>('unified');

	draftReviewComments = $state<PrReviewDraftComment[]>([]);
	submittingComment = $state<boolean>(false);
	submittingReview = $state<boolean>(false);
	mergingPr = $state<boolean>(false);
	actionFeedback = $state<{ type: 'success' | 'error'; message: string } | null>(null);

	forwarder = $state<ForwarderInfo | null>(null);
	forwarderBusy = $state<boolean>(false);
	recentWebhooks = $state<WebhookEvent[]>([]);

	readonly allViews = $derived.by((): GithubView[] => [
		...BUILTIN_GITHUB_VIEWS,
		...this.customViews
	]);

	readonly activeView = $derived.by(
		(): GithubView =>
			this.allViews.find((v) => v.id === this.activeViewId) ?? BUILTIN_GITHUB_VIEWS[3]
	);

	readonly effectiveRepo = $derived.by((): string | null => {
		const override = this.repoOverride.trim();
		if (override) return override;
		return this.detectedRepo;
	});

	readonly effectiveCwd = $derived.by((): string | null => {
		if (chatStore.sessionStatus.mode === 'directory' && chatStore.sessionStatus.cwd) {
			return chatStore.sessionStatus.cwd;
		}
		return null;
	});

	private initialized = false;
	private unlisteners: UnlistenFn[] = [];
	private lastDetectedCwd: string | null | undefined = undefined;

	async init(): Promise<void> {
		if (!this.initialized) {
			this.initialized = true;

			try {
				const unlistenWebhook = await listen<WebhookEvent>('github:webhook', (event) => {
					this.handleWebhookEvent(event.payload);
				});
				const unlistenForwarder = await listen<ForwarderInfo>('github:forwarder', (event) => {
					this.forwarder = event.payload;
				});
				const unlistenAuthCode = await listen<GhAuthCodeEvent>('github:auth-code', (event) => {
					this.authCode = event.payload;
				});
				this.unlisteners.push(unlistenWebhook, unlistenForwarder, unlistenAuthCode);
			} catch {
				// Running outside Tauri runtime
			}
		}

		await Promise.all([
			this.refreshStatus(),
			this.loadCustomViews(),
			this.detectWorkspaceRepo(),
			this.refreshForwarderStatus()
		]);

		if (this.status?.installed && this.status?.authenticated) {
			await this.refreshPrList();
		}
	}

	destroy(): void {
		for (const unlisten of this.unlisteners) {
			unlisten();
		}
		this.unlisteners = [];
		this.initialized = false;
	}

	private handleWebhookEvent(ev: WebhookEvent): void {
		this.recentWebhooks = [ev, ...this.recentWebhooks.slice(0, 29)];
		void this.refreshPrList();

		if (this.selectedPrNumber !== null) {
			const matchesPr =
				ev.summary.number === null ||
				ev.summary.number === this.selectedPrNumber ||
				ev.summary.category === 'ci';
			if (matchesPr) {
				void this.refreshSelectedPr();
			}
		}
	}

	async refreshStatus(): Promise<GhStatus | null> {
		try {
			const st = await invoke<GhStatus>('gh_status');
			this.status = st;
			return st;
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			this.status = {
				installed: false,
				version: null,
				authenticated: false,
				account: null,
				webhookExtension: false,
				error: msg
			};
			return this.status;
		}
	}

	async detectWorkspaceRepo(force = false): Promise<string | null> {
		const cwd = this.effectiveCwd;
		if (!force && cwd === this.lastDetectedCwd) {
			return this.detectedRepo;
		}
		this.lastDetectedCwd = cwd;
		try {
			const res = await invoke<{ nameWithOwner?: string } | null>('gh_json', {
				args: ['repo', 'view', '--json', 'nameWithOwner'],
				cwd: cwd ?? null
			});
			const slug = res?.nameWithOwner?.trim() || null;
			this.detectedRepo = slug;
			return slug;
		} catch {
			this.detectedRepo = null;
			return null;
		}
	}

	async loginWithWeb(hostname?: string, scopes?: string[]): Promise<boolean> {
		this.authBusy = true;
		this.authError = null;
		this.authCode = null;
		try {
			const st = await invoke<GhStatus>('gh_auth_login_web', {
				hostname: hostname?.trim() || null,
				scopes: scopes ?? null
			});
			this.status = st;
			this.authCode = null;
			if (st.authenticated) {
				await this.detectWorkspaceRepo(true);
				await this.refreshPrList();
			}
			return st.authenticated;
		} catch (err) {
			this.authError = err instanceof Error ? err.message : String(err);
			return false;
		} finally {
			this.authBusy = false;
		}
	}

	async loginWithToken(token: string, hostname?: string): Promise<boolean> {
		if (!token.trim()) {
			this.authError = 'Personal access token is required.';
			return false;
		}
		this.authBusy = true;
		this.authError = null;
		try {
			const st = await invoke<GhStatus>('gh_auth_login_token', {
				token: token.trim(),
				hostname: hostname?.trim() || null
			});
			this.status = st;
			if (st.authenticated) {
				await this.detectWorkspaceRepo(true);
				await this.refreshPrList();
			}
			return st.authenticated;
		} catch (err) {
			this.authError = err instanceof Error ? err.message : String(err);
			return false;
		} finally {
			this.authBusy = false;
		}
	}

	async logout(hostname?: string): Promise<void> {
		this.authBusy = true;
		this.authError = null;
		try {
			const st = await invoke<GhStatus>('gh_auth_logout', {
				hostname: hostname?.trim() || null
			});
			this.status = st;
			this.pullRequests = [];
			this.selectedPrNumber = null;
			this.selectedPrDetail = null;
		} catch (err) {
			this.authError = err instanceof Error ? err.message : String(err);
		} finally {
			this.authBusy = false;
		}
	}

	async loadCustomViews(): Promise<void> {
		try {
			const views = await invoke<GithubView[]>('list_github_views');
			if (Array.isArray(views)) {
				this.customViews = views.map((v) => ({ ...v, builtIn: false }));
			}
		} catch {
			// Fallback when running in browser dev mode
		}
	}

	async saveCustomView(input: {
		id?: string;
		name: string;
		query: string;
		sortOrder?: number;
	}): Promise<GithubView | null> {
		const id = input.id?.trim() || makeViewId();
		const name = input.name.trim();
		const query = input.query.trim();
		const sortOrder = input.sortOrder ?? this.customViews.length + 1;
		if (!name || !query) return null;

		try {
			const saved = await invoke<GithubView>('save_github_view', {
				view: { id, name, query, sortOrder }
			});
			await this.loadCustomViews();
			this.activeViewId = saved.id;
			this.searchQuery = saved.query;
			await this.refreshPrList();
			return saved;
		} catch (err) {
			this.listError = err instanceof Error ? err.message : String(err);
			return null;
		}
	}

	async deleteCustomView(id: string): Promise<boolean> {
		try {
			const ok = await invoke<boolean>('delete_github_view', { id });
			await this.loadCustomViews();
			if (this.activeViewId === id) {
				this.selectView('builtin:all-open');
			}
			return ok;
		} catch (err) {
			this.listError = err instanceof Error ? err.message : String(err);
			return false;
		}
	}

	selectView(viewId: string): void {
		const found = this.allViews.find((v) => v.id === viewId);
		if (!found) return;
		this.activeViewId = found.id;
		this.searchQuery = found.query;
		void this.refreshPrList();
	}

	setRepoOverride(repo: string): void {
		this.repoOverride = repo.trim();
		void this.refreshPrList();
		if (this.selectedPrNumber !== null) {
			void this.refreshSelectedPr();
		}
	}

	async refreshPrList(explicitQuery?: string): Promise<void> {
		if (explicitQuery !== undefined) {
			this.searchQuery = explicitQuery;
		}
		this.loadingList = true;
		this.listError = null;

		try {
			await this.detectWorkspaceRepo();
			const query = this.searchQuery.trim();
			const repo = this.effectiveRepo;
			const cwd = this.effectiveCwd;

			// When the search query already specifies `is:closed`, `is:merged`, or `state:...`,
			// pass state: 'all' so `gh pr list` does not restrict to open-only by default.
			const hasExplicitState = /\b(is|state):(open|closed|merged)\b/i.test(query);
			const state = hasExplicitState ? 'all' : 'open';

			const list = await invoke<GhPullRequest[] | null>('gh_pr_list', {
				cwd: cwd ?? null,
				repo: repo ?? null,
				state,
				search: query || null,
				limit: 50,
				fields: null
			});
			this.pullRequests = Array.isArray(list) ? list : [];
		} catch (err) {
			this.listError = err instanceof Error ? err.message : String(err);
		} finally {
			this.loadingList = false;
		}
	}

	async selectPr(prNumber: number | null): Promise<void> {
		this.selectedPrNumber = prNumber;
		this.detailError = null;
		this.actionFeedback = null;
		this.draftReviewComments = [];
		if (prNumber === null) {
			this.selectedPrDetail = null;
			this.selectedPrChecks = [];
			this.selectedPrDiff = [];
			this.selectedPrReviewComments = [];
			return;
		}
		await this.refreshSelectedPr();
	}

	async refreshSelectedPr(): Promise<void> {
		const prNumber = this.selectedPrNumber;
		if (prNumber === null) return;

		await Promise.all([
			this.loadPrDetail(prNumber),
			this.loadPrChecks(prNumber),
			this.loadPrDiff(prNumber),
			this.loadPrReviewComments(prNumber)
		]);
	}

	async loadPrDetail(prNumber: number): Promise<void> {
		this.loadingDetail = true;
		this.detailError = null;
		try {
			const detail = await invoke<GhPullRequestDetail | null>('gh_pr_detail', {
				selector: String(prNumber),
				repo: this.effectiveRepo ?? null,
				cwd: this.effectiveCwd ?? null
			});
			if (this.selectedPrNumber === prNumber && detail) {
				this.selectedPrDetail = detail;
			}
		} catch (err) {
			if (this.selectedPrNumber === prNumber) {
				this.detailError = err instanceof Error ? err.message : String(err);
			}
		} finally {
			if (this.selectedPrNumber === prNumber) {
				this.loadingDetail = false;
			}
		}
	}

	async loadPrChecks(prNumber: number): Promise<void> {
		this.loadingChecks = true;
		try {
			const checks = await invoke<GhCheck[] | null>('gh_pr_checks', {
				cwd: this.effectiveCwd ?? null,
				repo: this.effectiveRepo ?? null,
				selector: String(prNumber),
				fields: null
			});
			if (this.selectedPrNumber === prNumber) {
				this.selectedPrChecks = Array.isArray(checks) ? checks : [];
			}
		} catch {
			if (this.selectedPrNumber === prNumber) {
				this.selectedPrChecks = [];
			}
		} finally {
			if (this.selectedPrNumber === prNumber) {
				this.loadingChecks = false;
			}
		}
	}

	async loadPrDiff(prNumber: number): Promise<void> {
		this.loadingDiff = true;
		try {
			const files = await invoke<PrDiffFile[] | null>('gh_pr_diff', {
				selector: String(prNumber),
				repo: this.effectiveRepo ?? null,
				cwd: this.effectiveCwd ?? null
			});
			if (this.selectedPrNumber === prNumber) {
				this.selectedPrDiff = Array.isArray(files) ? files : [];
			}
		} catch {
			if (this.selectedPrNumber === prNumber) {
				this.selectedPrDiff = [];
			}
		} finally {
			if (this.selectedPrNumber === prNumber) {
				this.loadingDiff = false;
			}
		}
	}

	async loadPrReviewComments(prNumber: number): Promise<void> {
		try {
			const comments = await invoke<GhPrReviewComment[] | null>('gh_pr_review_comments', {
				repo: this.effectiveRepo ?? null,
				prNumber,
				cwd: this.effectiveCwd ?? null
			});
			if (this.selectedPrNumber === prNumber) {
				this.selectedPrReviewComments = Array.isArray(comments) ? comments : [];
			}
		} catch {
			if (this.selectedPrNumber === prNumber) {
				this.selectedPrReviewComments = [];
			}
		}
	}

	addDraftReviewComment(comment: {
		path: string;
		line: number;
		side: 'LEFT' | 'RIGHT';
		body: string;
	}): void {
		const trimmed = comment.body.trim();
		if (!trimmed || !comment.path || comment.line <= 0) return;
		this.draftReviewComments = [
			...this.draftReviewComments,
			{
				id: `draft-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
				path: comment.path,
				line: comment.line,
				side: comment.side,
				body: trimmed
			}
		];
	}

	removeDraftReviewComment(index: number): void {
		this.draftReviewComments = this.draftReviewComments.filter((_, i) => i !== index);
	}

	clearDraftReviewComments(): void {
		this.draftReviewComments = [];
	}

	async postComment(body: string): Promise<boolean> {
		const prNumber = this.selectedPrNumber;
		const trimmed = body.trim();
		if (prNumber === null || !trimmed) return false;

		this.submittingComment = true;
		this.actionFeedback = null;
		try {
			await invoke<string>('gh_pr_comment', {
				selector: String(prNumber),
				body: trimmed,
				repo: this.effectiveRepo ?? null,
				cwd: this.effectiveCwd ?? null
			});
			this.actionFeedback = { type: 'success', message: 'Comment posted to PR #' + prNumber };
			await this.loadPrDetail(prNumber);
			return true;
		} catch (err) {
			this.actionFeedback = {
				type: 'error',
				message: err instanceof Error ? err.message : String(err)
			};
			return false;
		} finally {
			this.submittingComment = false;
		}
	}

	async submitReview(event: PrReviewEvent, body: string): Promise<boolean> {
		const prNumber = this.selectedPrNumber;
		if (prNumber === null) return false;

		this.submittingReview = true;
		this.actionFeedback = null;
		try {
			const commentsPayload = this.draftReviewComments.map((c) => ({
				path: c.path,
				line: c.line,
				side: c.side,
				body: c.body
			}));
			await invoke('gh_pr_submit_review', {
				selector: String(prNumber),
				event,
				body: body.trim() || null,
				comments: commentsPayload.length > 0 ? commentsPayload : null,
				repo: this.effectiveRepo ?? null,
				cwd: this.effectiveCwd ?? null
			});
			this.draftReviewComments = [];
			this.actionFeedback = {
				type: 'success',
				message: `Review (${event.replace('_', ' ').toLowerCase()}) submitted on PR #${prNumber}`
			};
			await Promise.all([
				this.loadPrDetail(prNumber),
				this.loadPrReviewComments(prNumber),
				this.refreshPrList()
			]);
			return true;
		} catch (err) {
			this.actionFeedback = {
				type: 'error',
				message: err instanceof Error ? err.message : String(err)
			};
			return false;
		} finally {
			this.submittingReview = false;
		}
	}

	async mergePr(options: {
		method: PrMergeMethod;
		deleteBranch: boolean;
		subject?: string;
		body?: string;
	}): Promise<boolean> {
		const prNumber = this.selectedPrNumber;
		if (prNumber === null) return false;

		this.mergingPr = true;
		this.actionFeedback = null;
		try {
			await invoke<string>('gh_pr_merge', {
				selector: String(prNumber),
				method: options.method,
				deleteBranch: options.deleteBranch,
				subject: options.subject?.trim() || null,
				body: options.body?.trim() || null,
				repo: this.effectiveRepo ?? null,
				cwd: this.effectiveCwd ?? null
			});
			this.actionFeedback = {
				type: 'success',
				message: `Merged PR #${prNumber} via ${options.method}`
			};
			await Promise.all([this.loadPrDetail(prNumber), this.refreshPrList()]);
			return true;
		} catch (err) {
			this.actionFeedback = {
				type: 'error',
				message: err instanceof Error ? err.message : String(err)
			};
			return false;
		} finally {
			this.mergingPr = false;
		}
	}

	async refreshForwarderStatus(): Promise<void> {
		try {
			const info = await invoke<ForwarderInfo | null>('get_webhook_forwarder');
			this.forwarder = info;
		} catch {
			// Non-fatal
		}
	}

	async toggleWebhookForwarder(): Promise<void> {
		if (this.forwarderBusy) return;
		this.forwarderBusy = true;
		try {
			if (
				this.forwarder &&
				(this.forwarder.state === 'running' || this.forwarder.state === 'starting')
			) {
				await invoke('stop_webhook_forwarder');
				this.forwarder = null;
			} else {
				const repo = this.effectiveRepo;
				if (!repo) {
					this.listError = 'Set or detect an owner/repo slug before starting live webhooks.';
					return;
				}
				const info = await invoke<ForwarderInfo>('start_webhook_forwarder', {
					repo,
					events: null,
					installExtension: true
				});
				this.forwarder = info;
			}
		} catch (err) {
			this.listError = err instanceof Error ? err.message : String(err);
		} finally {
			this.forwarderBusy = false;
		}
	}
}

export const githubStore = new GithubStore();
