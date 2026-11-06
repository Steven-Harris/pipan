/**
 * Maps Pi telemetry (Tauri IPC events) onto the orchestration graph.
 *
 * `TelemetryGraph` is framework-free: it owns the node/link model for worktree agent
 * sessions (Phase 2 `agent:*` events) and GitHub webhook activity (Phase 3
 * `github:webhook`). The Svelte store in `stores/flow.svelte.ts` feeds it events and
 * publishes immutable snapshots; `FlowCanvas.svelte` merges snapshots with the
 * chat-derived graph and runs dagre over the union.
 *
 * Mapping:
 *   agent:status (new session)   -> append WorkerNode  (`agent_spawned`)
 *   agent:event tool_call/start  -> append ActionNode chained under the worker
 *   agent:event tool_call/update -> stream partial output into that ActionNode
 *   agent:event tool_call/end    -> ActionNode status completed|error + duration (`tool_result`)
 *   agent:event thought          -> latest reasoning shown on the WorkerNode
 *   agent:event task_complete    -> WorkerNode idle, dangling ActionNodes closed
 *   github:webhook               -> ActionNode (origin: github) under the orchestrator
 */
import type { Edge, MarkerType } from '@xyflow/svelte';
import type {
  AgentEventPayload,
  AgentSessionInfo,
  AgentToolCallData,
  AgentToolContent
} from '#lib/types/agent.ts';
import type { WebhookEvent } from '#lib/types/github.ts';
import type { FlowApprovalRequest, FlowNodeMeta, FlowNodeStatus } from '#lib/types/pi.ts';
import type { FlowNode } from './layout.ts';

/** Id of the orchestrator node rendered by FlowCanvas; workers hang off it. */
export const ORCHESTRATOR_NODE_ID = 'agent-orchestrator';

const MAX_WEBHOOK_NODES = 12;
const DETAIL_LIMIT = 240;

export interface FlowLink {
  id: string;
  source: string;
  target: string;
}

/**
 * How urgently a change should be published:
 * `structural` (nodes/status changed) publishes immediately, `minor` (streaming text)
 * may be throttled by the caller, `none` needs no publish.
 */
export type GraphChange = 'none' | 'minor' | 'structural';

export const workerNodeId = (sessionId: string) => `worker-${sessionId}`;
export const actionNodeId = (sessionId: string, callId: string) =>
  `action-${sessionId}-${callId}`;
const approvalNodeId = (sessionId: string, key: string) => `approval-${sessionId}-${key}`;

export function summarizeToolArgs(args?: Record<string, unknown> | null, argsJson?: string): string {
  if (args && typeof args === 'object') {
    for (const k of ['command', 'path', 'pattern', 'query', 'url', 'file']) {
      const v = args[k];
      if (typeof v === 'string' && v.trim()) return v.trim();
    }
  }
  if (argsJson) return argsJson.replace(/\s+/g, ' ').trim();
  return 'Executing tool call';
}

function contentText(content?: AgentToolContent | null): string {
  if (!content || !Array.isArray(content.content)) return '';
  return content.content
    .map((part) => {
      if (typeof part?.text === 'string') return part.text;
      if (part?.type === 'image') return '[Image output]';
      return '';
    })
    .filter(Boolean)
    .join('\n');
}

const clip = (text: string, limit = DETAIL_LIMIT) =>
  text.length > limit ? `…${text.slice(-limit)}` : text;

const head = (text: string, limit = DETAIL_LIMIT) =>
  text.length > limit ? `${text.slice(0, limit)}…` : text;

const shortId = (id: string) => (id.length > 8 ? id.slice(0, 8) : id);

const STATUS_COLOR: Record<FlowNodeStatus, string> = {
  error: '#f43f5e',
  running: '#6366f1',
  waiting: '#f59e0b',
  completed: '#10b981',
  idle: '#52525b',
  disconnected: '#3f3f46'
};

/** Builds a styled edge whose look follows the status of the node it points at. */
export function buildEdge(link: FlowLink, targetStatus: FlowNodeStatus): Edge {
  const color = STATUS_COLOR[targetStatus];
  return {
    id: link.id,
    source: link.source,
    target: link.target,
    animated: targetStatus === 'running',
    style: `stroke: ${color}; stroke-width: 1.75px;`,
    markerEnd: { type: 'arrowclosed' as MarkerType, color }
  };
}

function webhookStatus(ev: WebhookEvent): FlowNodeStatus {
  const state = (ev.summary.state ?? '').toLowerCase();
  if (/(fail|error|timed_out|cancel|action_required|changes_requested)/.test(state)) {
    return 'error';
  }
  return 'completed';
}

export class TelemetryGraph {
  private nodes = new Map<string, FlowNode>();
  private links: FlowLink[] = [];
  /** Last node in each worker's tool-call chain; the next tool call hangs off it. */
  private tail = new Map<string, string>();
  /** Reasoning text accumulated for the current thinking block of each session. */
  private thoughts = new Map<string, string>();
  private toolStartedAt = new Map<string, number>();
  private webhookNodeIds: string[] = [];

  snapshot(): { nodes: FlowNode[]; links: FlowLink[] } {
    return { nodes: [...this.nodes.values()], links: [...this.links] };
  }

  reset(): void {
    this.nodes.clear();
    this.links = [];
    this.tail.clear();
    this.thoughts.clear();
    this.toolStartedAt.clear();
    this.webhookNodeIds = [];
  }

  // -------------------------------------------------------------------------
  // agent:status
  // -------------------------------------------------------------------------

  applyStatus(info: AgentSessionInfo): GraphChange {
    const id = workerNodeId(info.sessionId);
    const existing = this.nodes.get(id);

    if (!existing) {
      this.addNode(
        {
          id,
          type: 'worker',
          position: { x: 0, y: 0 },
          data: {
            category: 'worker',
            title: `Worker ${shortId(info.sessionId)}`,
            subtitle: info.worktree.branch,
            badge: 'spawned',
            status: 'idle',
            detail: info.worktree.path,
            sessionId: info.sessionId,
            branch: info.worktree.branch,
            toolCount: 0,
            timestamp: Date.now()
          }
        },
        ORCHESTRATOR_NODE_ID
      );
      this.tail.set(info.sessionId, id);
    }

    if (info.state === 'running') {
      if (existing) {
        this.patch(id, { subtitle: info.worktree.branch, branch: info.worktree.branch });
      }
    } else {
      const status: FlowNodeStatus =
        info.state === 'exited' ? 'completed' : info.state === 'error' ? 'error' : 'disconnected';
      this.closeRunningActions(info.sessionId, 'error', 'interrupted');
      this.patch(id, {
        status,
        badge: info.state,
        detail: info.error ?? (info.state === 'exited' ? 'Pi process exited.' : 'Session ended.')
      });
    }
    return 'structural';
  }

  // -------------------------------------------------------------------------
  // agent:event
  // -------------------------------------------------------------------------

  applyEvent(p: AgentEventPayload): GraphChange {
    this.ensureWorker(p.sessionId);

    switch (p.kind) {
      case 'thought':
        return this.applyThought(p);
      case 'tool_call':
        return this.applyToolCall(p);
      case 'task_complete': {
        const aborted = p.data?.aborted === true;
        this.thoughts.delete(p.sessionId);
        this.closeRunningActions(
          p.sessionId,
          aborted ? 'error' : 'completed',
          aborted ? 'aborted' : undefined
        );
        this.patch(workerNodeId(p.sessionId), {
          status: 'idle',
          badge: aborted ? 'aborted' : 'task complete',
          detail: aborted ? 'Task aborted.' : 'Task complete. Awaiting next prompt.'
        });
        return 'structural';
      }
      case 'event': {
        const type = p.data?.type;
        if (type === 'agent_start' || type === 'turn_start') {
          this.patch(workerNodeId(p.sessionId), { status: 'running', badge: 'working' });
          return 'structural';
        }
        return 'none';
      }
      default:
        return 'none';
    }
  }

  private applyThought(p: AgentEventPayload): GraphChange {
    const data = p.data as { phase?: string; text?: string };
    const text = typeof data.text === 'string' ? data.text : '';
    let buffer = this.thoughts.get(p.sessionId) ?? '';
    if (data.phase === 'end') {
      // `thinking_end` carries the full block; prefer it over the streamed deltas.
      buffer = text || buffer;
      this.thoughts.delete(p.sessionId);
    } else {
      buffer += text;
      this.thoughts.set(p.sessionId, buffer);
    }
    this.patch(workerNodeId(p.sessionId), {
      status: 'running',
      badge: 'thinking',
      detail: buffer.trim() ? clip(buffer.trim()) : 'Reasoning…'
    });
    return 'minor';
  }

  private applyToolCall(p: AgentEventPayload): GraphChange {
    const data = p.data as unknown as AgentToolCallData;
    const callId = data.toolCallId ?? `t${p.timestamp}`;
    const nodeId = actionNodeId(p.sessionId, callId);
    const workerId = workerNodeId(p.sessionId);
    const toolName = data.toolName ?? 'tool';
    const argSummary = summarizeToolArgs(data.args);

    if (data.phase === 'start' || !this.nodes.has(nodeId)) {
      const isNew = !this.nodes.has(nodeId);
      if (isNew) {
        this.addNode(
          {
            id: nodeId,
            type: 'action',
            position: { x: 0, y: 0 },
            data: {
              category: 'action',
              title: toolName,
              subtitle: argSummary,
              badge: data.phase === 'end' ? 'finished' : 'running',
              status: 'running',
              detail: argSummary,
              sessionId: p.sessionId,
              toolCallId: callId,
              origin: 'pi',
              timestamp: p.timestamp
            }
          },
          this.tail.get(p.sessionId) ?? workerId
        );
        this.tail.set(p.sessionId, nodeId);
        this.toolStartedAt.set(nodeId, p.timestamp);
        const worker = this.nodes.get(workerId);
        this.patch(workerId, {
          status: 'running',
          badge: `tool: ${toolName}`,
          toolCount: (worker?.data.toolCount ?? 0) + 1
        });
      }
      if (data.phase === 'start') return 'structural';
    }

    if (data.phase === 'update') {
      const partial = contentText(data.partialResult);
      if (partial) this.patch(nodeId, { detail: clip(partial) });
      return 'minor';
    }

    // phase === 'end' (`tool_result`)
    const started = this.toolStartedAt.get(nodeId);
    this.toolStartedAt.delete(nodeId);
    const failed = data.isError === true;
    const result = contentText(data.result);
    this.patch(nodeId, {
      status: failed ? 'error' : 'completed',
      badge: failed ? 'error' : 'completed',
      detail: result ? head(result) : this.nodes.get(nodeId)?.data.detail,
      durationMs: started !== undefined ? Math.max(0, p.timestamp - started) : undefined
    });
    return 'structural';
  }

  // -------------------------------------------------------------------------
  // github:webhook
  // -------------------------------------------------------------------------

  applyWebhook(ev: WebhookEvent): GraphChange {
    const id = `action-gh-${ev.deliveryId ?? ev.timestamp}`;
    if (this.nodes.has(id)) return 'none';

    const s = ev.summary;
    const who = s.sender ? `@${s.sender}` : null;
    const ref = s.number !== null ? `#${s.number}` : null;
    const detail = [who, ref, s.title].filter(Boolean).join(' · ');

    this.addNode(
      {
        id,
        type: 'action',
        position: { x: 0, y: 0 },
        data: {
          category: 'action',
          title: ev.event,
          subtitle: ev.repo ?? undefined,
          badge: s.action ?? s.state ?? s.category,
          status: webhookStatus(ev),
          detail: detail || `GitHub ${ev.event} event`,
          origin: 'github',
          timestamp: ev.timestamp
        }
      },
      ORCHESTRATOR_NODE_ID
    );

    this.webhookNodeIds.push(id);
    while (this.webhookNodeIds.length > MAX_WEBHOOK_NODES) {
      const oldest = this.webhookNodeIds.shift()!;
      this.nodes.delete(oldest);
      this.links = this.links.filter((l) => l.source !== oldest && l.target !== oldest);
    }
    return 'structural';
  }

  // -------------------------------------------------------------------------
  // Approval gates (rendered now; the Rust interceptor that creates them is Phase 5)
  // -------------------------------------------------------------------------

  requestApproval(
    sessionId: string,
    request: Omit<FlowApprovalRequest, 'resolution'>,
    key: string = String(Date.now())
  ): string {
    this.ensureWorker(sessionId);
    const id = approvalNodeId(sessionId, key);
    if (!this.nodes.has(id)) {
      this.addNode(
        {
          id,
          type: 'approval',
          position: { x: 0, y: 0 },
          data: {
            category: 'approval',
            title: 'Approval required',
            subtitle: shortId(sessionId),
            badge: 'blocked',
            status: 'waiting',
            detail: request.reason ?? request.action,
            sessionId,
            approval: { ...request, resolution: 'pending' },
            timestamp: Date.now()
          }
        },
        this.tail.get(sessionId) ?? workerNodeId(sessionId)
      );
      this.tail.set(sessionId, id);
    }
    return id;
  }

  resolveApproval(nodeId: string, resolution: 'approved' | 'modified'): GraphChange {
    const node = this.nodes.get(nodeId);
    if (!node?.data.approval || node.data.approval.resolution !== 'pending') return 'none';
    this.patch(nodeId, {
      status: 'completed',
      badge: resolution,
      approval: { ...node.data.approval, resolution }
    });
    return 'structural';
  }

  // -------------------------------------------------------------------------
  // helpers
  // -------------------------------------------------------------------------

  private ensureWorker(sessionId: string): void {
    const id = workerNodeId(sessionId);
    if (this.nodes.has(id)) return;
    // Events can arrive before (or without) the matching `agent:status` snapshot.
    this.addNode(
      {
        id,
        type: 'worker',
        position: { x: 0, y: 0 },
        data: {
          category: 'worker',
          title: `Worker ${shortId(sessionId)}`,
          badge: 'running',
          status: 'running',
          sessionId,
          toolCount: 0,
          timestamp: Date.now()
        }
      },
      ORCHESTRATOR_NODE_ID
    );
    this.tail.set(sessionId, id);
  }

  private addNode(node: FlowNode, parentId: string): void {
    this.nodes.set(node.id, node);
    this.links.push({ id: `e-${parentId}-${node.id}`, source: parentId, target: node.id });
  }

  private patch(id: string, patch: Partial<FlowNodeMeta>): void {
    const node = this.nodes.get(id);
    if (!node) return;
    this.nodes.set(id, { ...node, data: { ...node.data, ...patch } });
  }

  private closeRunningActions(
    sessionId: string,
    status: FlowNodeStatus,
    badge: string | undefined
  ): void {
    for (const [id, node] of this.nodes) {
      if (
        node.data.sessionId === sessionId &&
        node.data.category === 'action' &&
        node.data.status === 'running'
      ) {
        this.toolStartedAt.delete(id);
        this.patch(id, { status, badge: badge ?? status });
      }
    }
  }
}
