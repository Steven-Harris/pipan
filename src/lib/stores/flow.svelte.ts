import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { Edge } from '@xyflow/svelte';
import type { AgentEventPayload, AgentSessionInfo } from '#lib/types/agent.ts';
import type { WebhookEvent } from '#lib/types/github.ts';
import type { FlowNode } from '#lib/flow/layout.ts';
import {
  TelemetryGraph,
  buildEdge,
  type FlowLink,
  type GraphChange
} from '#lib/flow/telemetry.ts';

/** Streaming updates (thought deltas, partial tool output) are coalesced to this interval. */
const MINOR_PUBLISH_MS = 120;

/**
 * Reactive view of the Pi telemetry graph. Subscribes to the Tauri IPC events emitted by
 * the Rust core and republishes immutable snapshots that FlowCanvas folds into the DAG.
 */
class FlowStore {
  /** Worker / action / approval nodes (unpositioned; FlowCanvas runs dagre). */
  nodes = $state.raw<FlowNode[]>([]);
  links = $state.raw<FlowLink[]>([]);

  private graph = new TelemetryGraph();
  private initialized = false;
  private unlisteners: UnlistenFn[] = [];
  private publishTimer: ReturnType<typeof setTimeout> | null = null;

  /** Edges styled from the current status of each link's target node. */
  get edges(): Edge[] {
    const status = new Map(this.nodes.map((n) => [n.id, n.data.status] as const));
    return this.links.map((l) => buildEdge(l, status.get(l.target) ?? 'idle'));
  }

  async init(): Promise<void> {
    if (this.initialized) return;
    this.initialized = true;

    this.unlisteners.push(
      await listen<AgentSessionInfo>('agent:status', (e) => {
        this.schedule(this.graph.applyStatus(e.payload));
      }),
      await listen<AgentEventPayload>('agent:event', (e) => {
        this.schedule(this.graph.applyEvent(e.payload));
      }),
      await listen<WebhookEvent>('github:webhook', (e) => {
        this.schedule(this.graph.applyWebhook(e.payload));
      })
    );

    // Hydrate sessions that were spawned before this listener attached (e.g. after reload).
    try {
      const sessions = await invoke<AgentSessionInfo[]>('list_agent_sessions');
      for (const info of sessions) this.graph.applyStatus(info);
      this.publish();
    } catch {
      // Backend unavailable (plain browser dev server); the graph simply starts empty.
    }
  }

  destroy(): void {
    for (const unlisten of this.unlisteners) unlisten();
    this.unlisteners = [];
    if (this.publishTimer) clearTimeout(this.publishTimer);
    this.publishTimer = null;
    this.initialized = false;
  }

  clear(): void {
    this.graph.reset();
    this.publish();
  }

  /**
   * Resolves an approval gate in the graph. Phase 5 extends this to inject the decision
   * back into the paused Pi RPC stream.
   */
  resolveApproval(nodeId: string, resolution: 'approved' | 'modified'): void {
    this.schedule(this.graph.resolveApproval(nodeId, resolution));
  }

  private schedule(change: GraphChange): void {
    if (change === 'none') return;
    if (change === 'structural') {
      this.publish();
      return;
    }
    if (this.publishTimer) return;
    this.publishTimer = setTimeout(() => this.publish(), MINOR_PUBLISH_MS);
  }

  private publish(): void {
    if (this.publishTimer) {
      clearTimeout(this.publishTimer);
      this.publishTimer = null;
    }
    const snap = this.graph.snapshot();
    this.nodes = snap.nodes;
    this.links = snap.links;
  }
}

export const flowStore = new FlowStore();
