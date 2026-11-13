// Pure rules for drawing the user to a graph that needs them.

import type { GraphState } from './types';

/** Panels PiPan opens by itself per session. Further graphs stay available in the "+" picker. */
export const MAX_AUTO_OPENED_PANELS = 4;

/** Ids of nodes that are waiting and have something the user can press. */
export function waitingWithActions(graph: GraphState): string[] {
  return graph.nodes.filter((n) => n.status === 'waiting' && (n.actions?.length ?? 0) > 0).map((n) => n.id);
}

/**
 * Nodes that entered the waiting-with-actions state in `next`: waiting now, and not waiting (or
 * without actions) in `previous`. Everything that waits counts when there is no previous graph.
 */
export function newlyWaiting(previous: GraphState | undefined, next: GraphState): string[] {
  const before = new Set(previous ? waitingWithActions(previous) : []);
  return waitingWithActions(next).filter((id) => !before.has(id));
}
