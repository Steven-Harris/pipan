/**
 * Types for the GitHub CLI + webhook + PR review/merge integration
 * (src-tauri/src/github.rs, src-tauri/src/webhook.rs, src-tauri/src/db.rs).
 *
 * Commands (via `invoke`):
 *  - gh_status, gh_auth_login_web, gh_auth_login_token, gh_auth_logout
 *  - gh_json, gh_pr_view, gh_pr_list, gh_pr_detail, gh_pr_diff, gh_pr_review_comments
 *  - gh_pr_comment, gh_pr_submit_review, gh_pr_merge, gh_pr_checks, gh_issue_list, gh_run_list
 *  - list_github_views, save_github_view, delete_github_view
 *  - start_webhook_forwarder, stop_webhook_forwarder, get_webhook_forwarder
 *
 * Events (via `listen`): `github:webhook`, `github:forwarder`, `github:forwarder-log`, `github:auth-code`.
 */

export interface GhStatus {
	installed: boolean;
	version: string | null;
	authenticated: boolean;
	account: string | null;
	webhookExtension: boolean;
	error?: string;
}

export interface GhAuthCodeEvent {
	code: string;
	hostname: string;
	verificationUri: string;
}

export interface GithubView {
	id: string;
	name: string;
	query: string;
	sortOrder: number;
	builtIn?: boolean;
}

export type ForwarderState = 'starting' | 'running' | 'exited' | 'error' | 'stopped';

export interface ForwarderInfo {
	state: ForwarderState;
	repo: string;
	events: string[];
	/** Loopback port of the local webhook receiver. */
	port: number;
	pid: number | null;
	error?: string;
}

export interface ForwarderLog {
	stream: 'stdout' | 'stderr';
	line: string;
	timestamp: number;
}

export type WebhookCategory =
	| 'comment'
	| 'review'
	| 'ci'
	| 'pull_request'
	| 'issue'
	| 'ping'
	| 'other';

export interface WebhookSummary {
	category: WebhookCategory;
	action: string | null;
	number: number | null;
	title: string | null;
	url: string | null;
	sender: string | null;
	/** Review state, check conclusion/status, or commit status. */
	state: string | null;
}

export interface WebhookEvent {
	deliveryId: string | null;
	/** `X-GitHub-Event` header, e.g. `issue_comment`, `pull_request_review`, `check_run`. */
	event: string;
	repo: string | null;
	timestamp: number;
	summary: WebhookSummary;
	/** Full GitHub payload. */
	payload: Record<string, unknown>;
}

export interface GhAuthor {
	id?: string;
	login: string;
	name?: string;
	is_bot?: boolean;
}

export interface GhLabel {
	id?: string;
	name: string;
	color?: string;
	description?: string;
}

export interface GhReviewRequest {
	login?: string;
	name?: string;
	slug?: string;
}

export interface GhStatusCheckItem {
	__typename?: string;
	name?: string;
	context?: string;
	state?: string;
	status?: string;
	conclusion?: string | null;
	workflowName?: string;
	detailsUrl?: string;
	targetUrl?: string;
	startedAt?: string;
	completedAt?: string;
}

export interface GhPrComment {
	id?: string;
	author: GhAuthor;
	body: string;
	createdAt: string;
	url?: string;
}

export interface GhPrReview {
	id?: string;
	author: GhAuthor;
	state: 'APPROVED' | 'CHANGES_REQUESTED' | 'COMMENTED' | 'DISMISSED' | 'PENDING' | string;
	body: string;
	submittedAt?: string;
}

export interface GhPrFileSummary {
	path: string;
	additions: number;
	deletions: number;
}

export interface GhPrCommit {
	oid: string;
	messageHeadline: string;
	messageBody?: string;
	committedDate?: string;
	authors?: Array<{ name?: string; email?: string; login?: string }>;
}

export interface GhPullRequest {
	number: number;
	title: string;
	state: 'OPEN' | 'CLOSED' | 'MERGED';
	url: string;
	headRefName: string;
	baseRefName: string;
	headRefOid?: string;
	author: GhAuthor;
	isDraft: boolean;
	reviewDecision: string | null;
	statusCheckRollup?: GhStatusCheckItem[];
	labels?: GhLabel[];
	assignees?: GhAuthor[];
	reviewRequests?: GhReviewRequest[];
	updatedAt: string;
	createdAt?: string;
	mergedAt?: string | null;
	closedAt?: string | null;
	additions?: number;
	deletions?: number;
	changedFiles?: number;
	mergeable?: 'MERGEABLE' | 'CONFLICTING' | 'UNKNOWN' | string;
	mergeStateStatus?: string;
	body?: string;
}

export interface GhPullRequestDetail extends GhPullRequest {
	comments?: GhPrComment[];
	reviews?: GhPrReview[];
	files?: GhPrFileSummary[];
	commits?: GhPrCommit[];
}

export interface GhPrReviewComment {
	id: number;
	path: string;
	line: number | null;
	original_line?: number | null;
	side?: 'LEFT' | 'RIGHT';
	body: string;
	user?: {
		login: string;
		avatar_url?: string;
	};
	created_at: string;
	updated_at?: string;
	html_url?: string;
	in_reply_to_id?: number;
}

export type PrDiffLineKind = 'context' | 'add' | 'del' | 'meta';

export interface PrDiffLine {
	kind: PrDiffLineKind;
	content: string;
	oldLine: number | null;
	newLine: number | null;
}

export interface PrDiffHunk {
	header: string;
	oldStart: number;
	oldLines: number;
	newStart: number;
	newLines: number;
	lines: PrDiffLine[];
}

export interface PrDiffFile {
	path: string;
	oldPath: string;
	newPath: string;
	status: 'added' | 'modified' | 'deleted' | 'renamed' | string;
	additions: number;
	deletions: number;
	isBinary: boolean;
	hunks: PrDiffHunk[];
}

export interface PrReviewDraftComment {
	id?: string;
	path: string;
	line: number;
	side: 'LEFT' | 'RIGHT';
	body: string;
}

export type PrReviewEvent = 'APPROVE' | 'REQUEST_CHANGES' | 'COMMENT';

export type PrMergeMethod = 'merge' | 'squash' | 'rebase';

export interface GhCheck {
	name: string;
	state: string;
	bucket: 'pass' | 'fail' | 'pending' | 'skipping' | 'cancel';
	workflow: string;
	link: string;
	event: string;
	startedAt: string;
	completedAt: string;
}

export interface GhIssue {
	number: number;
	title: string;
	state: 'OPEN' | 'CLOSED';
	url: string;
	author: GhAuthor;
	labels: { name: string; color?: string }[];
	updatedAt: string;
}

export interface GhWorkflowRun {
	databaseId: number;
	displayTitle: string;
	status: string;
	conclusion: string;
	workflowName: string;
	headBranch: string;
	event: string;
	url: string;
	createdAt: string;
}
