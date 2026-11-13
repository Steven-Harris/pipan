// Manually dragged node positions of extension channel graphs, per (session, channel, topic, graph id).
//
// The panel is unmounted when its tab is hidden, so positions live here and are saved to
// pipan.db through the Rust core. Without a backend (plain browser dev server) they stay in
// memory.

import { invoke } from '@tauri-apps/api/core';
import type { PositionMap, SavedPosition } from '#lib/flow/layout.ts';

const EMPTY: PositionMap = Object.freeze({}) as PositionMap;

/** Identifies one graph. The channel and topic keep graphs that share a graph id apart. */
export interface GraphRef {
  sessionId: string;
  ch: string;
  topic: string;
  graphId: string;
}

const keyOf = (ref: GraphRef) => [ref.sessionId, ref.ch, ref.topic, ref.graphId].join('\u0000');
const argsOf = (ref: GraphRef) => ({ sessionId: ref.sessionId, ch: ref.ch, topic: ref.topic, graphId: ref.graphId });

class ChannelGraphLayoutStore {
  private positions = $state.raw<Record<string, PositionMap>>({});
  private hydrated = $state.raw<Record<string, true>>({});
  private inflight = new Map<string, Promise<void>>();

  isReady(ref: GraphRef): boolean {
    return this.hydrated[keyOf(ref)] === true;
  }

  /** Loads saved positions once. Safe to call repeatedly. */
  hydrate(ref: GraphRef): Promise<void> {
    const key = keyOf(ref);
    if (this.hydrated[key]) return Promise.resolve();
    const existing = this.inflight.get(key);
    if (existing) return existing;

    const task = (async () => {
      try {
        const saved = await invoke<PositionMap>('load_channel_node_positions', argsOf(ref));
        // Anything the user did while loading wins over the stored copy.
        this.positions = { ...this.positions, [key]: { ...saved, ...this.positions[key] } };
      } catch {
        // No backend: positions are session-scoped memory only.
      } finally {
        this.hydrated = { ...this.hydrated, [key]: true };
        this.inflight.delete(key);
      }
    })();
    this.inflight.set(key, task);
    return task;
  }

  positionsFor(ref: GraphRef): PositionMap {
    return this.positions[keyOf(ref)] ?? EMPTY;
  }

  /** Records dragged nodes as pinned. */
  pin(ref: GraphRef, moved: Record<string, { x: number; y: number }>): void {
    const updates: Record<string, SavedPosition> = {};
    for (const [id, p] of Object.entries(moved)) updates[id] = { x: p.x, y: p.y, pinned: true };
    if (Object.keys(updates).length === 0) return;
    const key = keyOf(ref);
    this.positions = { ...this.positions, [key]: { ...this.positions[key], ...updates } };
    invoke('save_channel_node_positions', { ...argsOf(ref), positions: updates }).catch((err) =>
      console.error('Failed to save channel graph positions:', err)
    );
  }

  /** Releases every dragged node of the graph. */
  reset(ref: GraphRef): void {
    this.positions = { ...this.positions, [keyOf(ref)]: EMPTY };
    invoke('clear_channel_node_positions', argsOf(ref)).catch((err) =>
      console.error('Failed to clear channel graph positions:', err)
    );
  }

  pinnedCount(ref: GraphRef): number {
    return Object.values(this.positionsFor(ref)).filter((p) => p.pinned !== false).length;
  }
}

export const channelGraphLayout = new ChannelGraphLayoutStore();
