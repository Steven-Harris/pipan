/**
 * Shared IPC & Pi RPC Contract for Phase 0 Direct Pi RPC Chat Window
 */

export type PiConnectionState = 'disconnected' | 'starting' | 'running' | 'error';
export type PiSessionMode = 'headless' | 'directory';
export type SavedSessionMode = 'headless' | 'directory' | 'worktree';

export interface PiSessionStatus {
  state: PiConnectionState;
  mode: PiSessionMode;
  cwd: string | null;
  pid: number | null;
  error?: string | null;
  sessionId?: string | null;
  sessionFilePath?: string | null;
  name?: string | null;
}

export interface PiStderrPayload {
  line: string;
  timestamp: number;
}

export interface PiModelInfo {
  id?: string;
  name?: string;
  provider?: string;
  thinkingLevel?: string;
}

export interface TokenUsage {
  input?: number;
  output?: number;
  cacheRead?: number;
  cacheWrite?: number;
  totalTokens?: number;
  cost?: {
    input?: number;
    output?: number;
    cacheRead?: number;
    cacheWrite?: number;
    total?: number;
  };
}

// ============================================================================
// Outgoing Pi RPC Commands (Frontend -> Rust -> Pi stdin JSONL)
// ============================================================================

export type StreamingBehavior = 'steer' | 'followUp';

export type PiRpcCommand =
  | {
      id?: string;
      type: 'prompt';
      message: string;
      images?: Array<{ type: 'image'; data: string; mimeType: string }>;
      streamingBehavior?: StreamingBehavior;
    }
  | {
      id?: string;
      type: 'steer';
      message: string;
      images?: Array<{ type: 'image'; data: string; mimeType: string }>;
    }
  | {
      id?: string;
      type: 'follow_up';
      message: string;
      images?: Array<{ type: 'image'; data: string; mimeType: string }>;
    }
  | {
      id?: string;
      type: 'abort';
    }
  | {
      id?: string;
      type: 'clear_queue';
    }
  | {
      id?: string;
      type: 'new_session';
      parentSession?: string;
    }
  | {
      id?: string;
      type: 'switch_session';
      sessionPath: string;
      sessionId?: string;
    }
  | {
      id?: string;
      type: 'set_session_name';
      name: string;
    }
  | {
      id?: string;
      type: 'get_state';
    }
  | {
      id?: string;
      type: 'get_messages';
    }
  | {
      id?: string;
      type: string;
      [key: string]: unknown;
    };

// ============================================================================
// Incoming Pi RPC Events (Pi stdout JSONL -> Rust -> Frontend `pi:event`)
// ============================================================================

export interface PiRpcResponseEvent {
  type: 'response';
  id?: string;
  command: string;
  success: boolean;
  data?: Record<string, unknown>;
  error?: string;
}

export interface PiAssistantMessageUpdateSubEvent {
  type:
    | 'start'
    | 'text_start'
    | 'text_delta'
    | 'text_end'
    | 'thinking_start'
    | 'thinking_delta'
    | 'thinking_end'
    | 'toolcall_start'
    | 'toolcall_delta'
    | 'toolcall_end'
    | 'done'
    | 'error';
  contentIndex?: number;
  delta?: string;
  content?: string;
  toolCall?: {
    id?: string;
    name?: string;
    arguments?: Record<string, unknown>;
  };
  partial?: Record<string, unknown>;
  reason?: string;
  error?: Record<string, unknown>;
}

export interface PiRawMessage {
  id?: string;
  role?: 'user' | 'assistant' | 'toolResult' | string;
  timestamp?: number;
  content?: string | Array<Record<string, unknown>>;
  stopReason?: string;
  errorMessage?: string;
  usage?: TokenUsage;
  model?: string;
  provider?: string;
  [key: string]: unknown;
}

export interface PiRpcEvent {
  type:
    | 'response'
    | 'agent_start'
    | 'agent_end'
    | 'agent_settled'
    | 'turn_start'
    | 'turn_end'
    | 'message_start'
    | 'message_update'
    | 'message_end'
    | 'tool_execution_start'
    | 'tool_execution_update'
    | 'tool_execution_end'
    | 'queue_update'
    | 'compaction_start'
    | 'compaction_end'
    | 'auto_retry_start'
    | 'auto_retry_end'
    | 'extension_error'
    | string;
  // response fields
  id?: string;
  command?: string;
  success?: boolean;
  data?: Record<string, unknown>;
  error?: string;
  // message_start / message_update / message_end
  message?: PiRawMessage;
  assistantMessageEvent?: PiAssistantMessageUpdateSubEvent;
  // tool_execution_*
  toolCallId?: string;
  toolName?: string;
  args?: Record<string, unknown>;
  partialResult?: {
    content?: Array<{ type?: string; text?: string; [key: string]: unknown }>;
    details?: unknown;
  };
  result?: {
    content?: Array<{ type?: string; text?: string; [key: string]: unknown }>;
    details?: unknown;
  };
  isError?: boolean;
  // queue_update
  steering?: string[];
  followUp?: string[];
  // agent_settled / turn_end
  aborted?: boolean;
  [key: string]: unknown;
}

// ============================================================================
// UI View Models
// ============================================================================

export interface TextContentBlock {
  type: 'text';
  text: string;
  isComplete: boolean;
}

export interface ThinkingContentBlock {
  type: 'thinking';
  thinking: string;
  isComplete: boolean;
}

export type ToolCallStatus = 'streaming_args' | 'running' | 'completed' | 'error';

export interface ToolCallContentBlock {
  type: 'tool_call';
  id: string;
  name: string;
  argsJson: string;
  args?: Record<string, unknown>;
  status: ToolCallStatus;
  partialOutput?: string;
  resultOutput?: string;
  isError?: boolean;
  durationMs?: number;
  startedAt?: number;
}

export type ContentBlock = TextContentBlock | ThinkingContentBlock | ToolCallContentBlock;

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  timestamp: number;
  blocks: ContentBlock[];
  isStreaming?: boolean;
  stopReason?: string;
  errorMessage?: string;
  usage?: TokenUsage;
  isOptimistic?: boolean;
}

export interface SessionHistoryEntry {
  id: string;
  mode: PiSessionMode;
  cwd: string | null;
  label: string;
  startedAt: number;
  pid: number | null;
  state: PiConnectionState;
  messageCount: number;
  toolCallCount: number;
}

// ============================================================================
// Persistent Sessions (Phase 3 SQLite Store)
// ============================================================================

export type SessionFilter =
  | 'active'
  | 'running'
  | 'archived'
  | 'directory'
  | 'headless'
  | 'worktree';

export type SessionGroupBy = 'none' | 'folder' | 'group' | 'date';

export type SessionSortBy =
  | 'updated_at'
  | 'created_at'
  | 'name'
  | 'total_cost_usd'
  | 'message_count';

export interface SavedSession {
  id: string;
  name: string;
  title: string;
  status: string;
  mode: SavedSessionMode | string;
  cwd: string | null;
  sessionFilePath: string | null;
  piSessionFile?: string | null;
  groupName: string | null;
  branch: string | null;
  worktreePath: string | null;
  archived: boolean;
  pinned: boolean;
  createdAt: number;
  updatedAt: number;
  messageCount: number;
  toolCallCount: number;
  totalTokens: number;
  totalCostUsd: number;
  modelId: string | null;
  model?: string | null;
  provider: string | null;
  graphDirection: 'TB' | 'LR' | null;
  tags: string[];
}

export interface SaveSessionMetadataInput {
  id: string;
  name?: string | null;
  status?: string | null;
  mode?: string | null;
  cwd?: string | null;
  sessionFilePath?: string | null;
  groupName?: string | null;
  branch?: string | null;
  worktreePath?: string | null;
  archived?: boolean;
  pinned?: boolean;
  createdAt?: number;
  updatedAt?: number;
  messageCount?: number;
  toolCallCount?: number;
  totalTokens?: number;
  totalCostUsd?: number;
  modelId?: string | null;
  provider?: string | null;
  graphDirection?: 'TB' | 'LR' | null;
  tags?: string[];
}

export interface SessionListQuery {
  search?: string;
  filter?: SessionFilter | string;
  sortBy?: SessionSortBy | string;
  sortOrder?: 'asc' | 'desc';
}

export type FlowNodeCategory =
  | 'supervisor'
  | 'orchestrator'
  | 'user_prompt'
  | 'thinking'
  | 'tool_call'
  | 'assistant_reply'
  | 'worker'
  | 'action'
  | 'approval';

export type FlowNodeStatus =
  | 'idle'
  | 'running'
  | 'completed'
  | 'error'
  | 'disconnected'
  | 'waiting';

/** Svelte Flow `type` keys registered in FlowCanvas. */
export type FlowNodeType = 'pipan' | 'orchestrator' | 'worker' | 'action' | 'approval';

export type ApprovalResolution = 'pending' | 'approved' | 'modified' | 'rejected';

export interface FlowApprovalRequest {
  /** The blocked action, e.g. a shell command or file path. */
  action: string;
  reason?: string;
  resolution: ApprovalResolution;
  /** Correlating request id from Rust (Phase 5). */
  requestId?: string;
  /** Modified replacement command if resolution was 'modified'. */
  replacement?: string;
}

export interface FlowNodeMeta {
  [key: string]: unknown;
  category: FlowNodeCategory;
  title: string;
  subtitle?: string;
  badge?: string;
  status: FlowNodeStatus;
  detail?: string;
  timestamp?: number;
  durationMs?: number;
  tokens?: number;
  /** Worktree agent session this node belongs to (Phase 2 `agent:*` events). */
  sessionId?: string;
  /** Git branch of the worker's isolated worktree. */
  branch?: string;
  /** Number of tool calls a worker has issued. */
  toolCount?: number;
  /** Origin of an action node. */
  origin?: 'pi' | 'github';
  /** Present on approval nodes. */
  approval?: FlowApprovalRequest;
}

