/**
 * Types for the Phase 3 GitHub CLI + webhook bridge (src-tauri/src/github.rs,
 * src-tauri/src/webhook.rs).
 *
 * Commands (via `invoke`):
 *  - gh_status, gh_json, gh_pr_view, gh_pr_list, gh_pr_checks, gh_issue_list, gh_run_list
 *  - start_webhook_forwarder, stop_webhook_forwarder, get_webhook_forwarder
 *
 * Events (via `listen`): `github:webhook`, `github:forwarder`, `github:forwarder-log`.
 */

export interface GhStatus {
	installed: boolean;
	version: string | null;
	authenticated: boolean;
	account: string | null;
	webhookExtension: boolean;
	error?: string;
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

export interface GhPullRequest {
	number: number;
	title: string;
	state: 'OPEN' | 'CLOSED' | 'MERGED';
	url: string;
	headRefName: string;
	baseRefName: string;
	author: GhAuthor;
	isDraft: boolean;
	reviewDecision: string;
	updatedAt: string;
	mergeable?: string;
	statusCheckRollup?: unknown[];
	body?: string;
}

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
