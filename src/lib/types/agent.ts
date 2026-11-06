/**
 * Types for the Phase 2 worktree agent runtime (src-tauri/src/agent.rs).
 *
 * Commands (via `invoke`): spawn_agent_session, send_agent_command,
 * stop_agent_session, list_agent_sessions.
 *
 * Events (via `listen`): `agent:status`, `agent:event`, `agent:stderr`.
 */

/** Serialized with Rust's default (snake_case) field names. */
export interface WorktreeInfo {
  repo_root: string;
  path: string;
  branch: string;
  base_ref: string;
}

export type AgentSessionState = 'running' | 'exited' | 'error' | 'stopped';

export interface AgentSessionInfo {
  sessionId: string;
  state: AgentSessionState | string;
  worktree: WorktreeInfo;
  pid: number | null;
  error?: string;
}

export type AgentEventKind = 'thought' | 'tool_call' | 'task_complete' | 'event';

export interface AgentToolContent {
  content?: Array<{ type?: string; text?: string; [key: string]: unknown }>;
  details?: unknown;
}

export interface AgentToolCallData {
  phase: 'start' | 'update' | 'end';
  toolCallId?: string | null;
  toolName?: string | null;
  args?: Record<string, unknown> | null;
  partialResult?: AgentToolContent | null;
  result?: AgentToolContent | null;
  isError?: boolean | null;
}

export interface AgentThoughtData {
  phase: 'delta' | 'end';
  contentIndex?: number | null;
  text?: string;
}

export interface AgentEventPayload {
  sessionId: string;
  kind: AgentEventKind | string;
  timestamp: number;
  data: Record<string, unknown>;
}

/**
 * Phase 5 Approval Request emitted by Rust over `agent:approval_request`.
 */
export interface AgentApprovalRequestPayload {
  requestId: string;
  sessionId: string;
  toolName: string;
  toolCallId?: string | null;
  kind: 'delete' | 'git-push' | 'git-destructive' | string;
  reason: string;
  action: string;
  timestamp: number;
}

/**
 * Phase 5 Approval Resolved event emitted by Rust over `agent:approval_resolved`.
 */
export interface AgentApprovalResolvedPayload {
  requestId: string;
  sessionId: string;
  resolution: 'approved' | 'modified' | 'rejected' | string;
  replacement?: string | null;
  timestamp: number;
}
