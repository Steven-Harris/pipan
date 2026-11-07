// Per-session node graph layout: the chosen direction and user-dragged node positions.
//
// FlowCanvas is unmounted whenever its panel tab is hidden or the user navigates away,
// so this state lives here, not in the component. Saved to pipan.db through the Rust
// core; without a backend (plain browser dev server) it stays in memory.

import { invoke } from '@tauri-apps/api/core';
import type { LayoutDirection, PositionMap, SavedPosition } from '#lib/flow/layout.ts';
import type { SplitOrientation } from '#lib/stores/workspace.svelte.ts';

/** Graph state for a canvas with no session yet. Never persisted. */
const NO_SESSION = '__none__';
const EMPTY: PositionMap = Object.freeze({}) as PositionMap;

/** Side-by-side split is wide and short, so ranks run left to right; stacked is the reverse. */
export function defaultDirection(orientation: SplitOrientation): LayoutDirection {
  return orientation === 'vertical' ? 'LR' : 'TB';
}

function key(sessionId: string | null): string {
  return sessionId ?? NO_SESSION;
}

function posKey(sessionId: string | null, direction: LayoutDirection): string {
  return `${key(sessionId)}\u0000${direction}`;
}

class GraphLayoutStore {
  /** Bumped by `resetLayout`; part of the canvas's structural key so it re-runs dagre. */
  epoch = $state(0);

  /** Explicit user direction per session. Absent means "follow the split default". */
  private explicit = $state.raw<Record<string, LayoutDirection>>({});
  private positions = $state.raw<Record<string, PositionMap>>({});
  private hydrated = $state.raw<Record<string, true>>({});
  private inflight = new Map<string, Promise<void>>();

  /** True once saved layout for the session has been read, so the canvas can place nodes once. */
  isReady(sessionId: string | null): boolean {
    return sessionId === null || this.hydrated[key(sessionId)] === true;
  }

  /** Loads the saved direction and both direction layouts. Safe to call repeatedly. */
  hydrate(sessionId: string | null): Promise<void> {
    if (sessionId === null || this.isReady(sessionId)) return Promise.resolve();
    const existing = this.inflight.get(sessionId);
    if (existing) return existing;

    const task = (async () => {
      try {
        const [direction, tb, lr] = await Promise.all([
          invoke<LayoutDirection | null>('get_graph_direction', { sessionId }),
          invoke<PositionMap>('load_node_positions', { sessionId, direction: 'TB' }),
          invoke<PositionMap>('load_node_positions', { sessionId, direction: 'LR' })
        ]);
        // Anything the user did while loading wins over the stored copy.
        const keyTb = posKey(sessionId, 'TB');
        const keyLr = posKey(sessionId, 'LR');
        this.positions = {
          ...this.positions,
          [keyTb]: { ...tb, ...this.positions[keyTb] },
          [keyLr]: { ...lr, ...this.positions[keyLr] }
        };
        if ((direction === 'TB' || direction === 'LR') && !this.explicit[sessionId]) {
          this.explicit = { ...this.explicit, [sessionId]: direction };
        }
      } catch {
        // No backend: layout is session-scoped memory only.
      } finally {
        this.hydrated = { ...this.hydrated, [sessionId]: true };
        this.inflight.delete(sessionId);
      }
    })();
    this.inflight.set(sessionId, task);
    return task;
  }

  /** The direction to render: an explicit choice always wins over the split default. */
  directionFor(sessionId: string | null, orientation: SplitOrientation): LayoutDirection {
    return this.explicit[key(sessionId)] ?? defaultDirection(orientation);
  }

  setDirection(sessionId: string | null, direction: LayoutDirection): void {
    this.explicit = { ...this.explicit, [key(sessionId)]: direction };
    if (sessionId === null) return;
    invoke('set_graph_direction', { sessionId, direction }).catch((err) =>
      console.error('Failed to save graph direction:', err)
    );
  }

  positionsFor(sessionId: string | null, direction: LayoutDirection): PositionMap {
    return this.positions[posKey(sessionId, direction)] ?? EMPTY;
  }

  /** Records dragged nodes as pinned for the direction they were dragged in. */
  pin(
    sessionId: string | null,
    direction: LayoutDirection,
    moved: Record<string, { x: number; y: number }>
  ): void {
    const updates: Record<string, SavedPosition> = {};
    for (const [id, p] of Object.entries(moved)) {
      updates[id] = { x: p.x, y: p.y, pinned: true };
    }
    if (Object.keys(updates).length === 0) return;

    const k = posKey(sessionId, direction);
    this.positions = { ...this.positions, [k]: { ...this.positions[k], ...updates } };
    if (sessionId === null) return;
    invoke('save_node_positions', { sessionId, direction, positions: updates }).catch((err) =>
      console.error('Failed to save node positions:', err)
    );
  }

  /** Drops pinned coordinates for the active direction only and asks for a fresh layout. */
  resetLayout(sessionId: string | null, direction: LayoutDirection): void {
    const k = posKey(sessionId, direction);
    this.positions = { ...this.positions, [k]: EMPTY };
    this.epoch += 1;
    if (sessionId === null) return;
    invoke('clear_node_positions', { sessionId, direction }).catch((err) =>
      console.error('Failed to clear node positions:', err)
    );
  }
}

export const graphLayoutStore = new GraphLayoutStore();
