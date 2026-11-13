// Manually dragged node positions of extension channel graphs, per (session, graph id).
//
// The panel is unmounted when its tab is hidden, so positions live here and are saved to
// pipan.db through the Rust core. Without a backend (plain browser dev server) they stay in
// memory.

import { invoke } from '@tauri-apps/api/core';
import type { PositionMap, SavedPosition } from '#lib/flow/layout.ts';

const EMPTY: PositionMap = Object.freeze({}) as PositionMap;
const keyOf = (sessionId: string, graphId: string) => `${sessionId}\u0000${graphId}`;

class ChannelGraphLayoutStore {
  private positions = $state.raw<Record<string, PositionMap>>({});
  private hydrated = $state.raw<Record<string, true>>({});
  private inflight = new Map<string, Promise<void>>();

  isReady(sessionId: string, graphId: string): boolean {
    return this.hydrated[keyOf(sessionId, graphId)] === true;
  }

  /** Loads saved positions once. Safe to call repeatedly. */
  hydrate(sessionId: string, graphId: string): Promise<void> {
    const key = keyOf(sessionId, graphId);
    if (this.hydrated[key]) return Promise.resolve();
    const existing = this.inflight.get(key);
    if (existing) return existing;

    const task = (async () => {
      try {
        const saved = await invoke<PositionMap>('load_channel_node_positions', { sessionId, graphId });
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

  positionsFor(sessionId: string, graphId: string): PositionMap {
    return this.positions[keyOf(sessionId, graphId)] ?? EMPTY;
  }

  /** Records dragged nodes as pinned. */
  pin(sessionId: string, graphId: string, moved: Record<string, { x: number; y: number }>): void {
    const updates: Record<string, SavedPosition> = {};
    for (const [id, p] of Object.entries(moved)) updates[id] = { x: p.x, y: p.y, pinned: true };
    if (Object.keys(updates).length === 0) return;
    const key = keyOf(sessionId, graphId);
    this.positions = { ...this.positions, [key]: { ...this.positions[key], ...updates } };
    invoke('save_channel_node_positions', { sessionId, graphId, positions: updates }).catch((err) =>
      console.error('Failed to save channel graph positions:', err)
    );
  }

  /** Releases every dragged node of the graph. */
  reset(sessionId: string, graphId: string): void {
    this.positions = { ...this.positions, [keyOf(sessionId, graphId)]: EMPTY };
    invoke('clear_channel_node_positions', { sessionId, graphId }).catch((err) =>
      console.error('Failed to clear channel graph positions:', err)
    );
  }

  pinnedCount(sessionId: string, graphId: string): number {
    return Object.values(this.positionsFor(sessionId, graphId)).filter((p) => p.pinned !== false).length;
  }
}

export const channelGraphLayout = new ChannelGraphLayoutStore();
