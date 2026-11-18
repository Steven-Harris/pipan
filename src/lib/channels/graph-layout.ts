import dagre from 'dagre';
import type { LayoutDirection, PositionMap } from '#lib/flow/layout.ts';
import { LAYOUT_SPACING } from '#lib/flow/layout.ts';
import type { GraphState } from './types';

export const NODE_WIDTH = 220;
export const NODE_HEIGHT = 76;
const MARGIN = 20;

export interface XY {
  x: number;
  y: number;
}

/** Edge ids are unique within a graph even when the producer omits or repeats them. */
export function edgeIds(graph: Pick<GraphState, 'edges'>): string[] {
  const seen = new Set<string>();
  return graph.edges.map((edge, index) => {
    let id = edge.id ?? `${edge.from}->${edge.to}`;
    if (seen.has(id)) id = `${id}#${index}`;
    seen.add(id);
    return id;
  });
}

/**
 * Changes only when nodes or edges are added, removed or re-pointed (or the direction changes),
 * not when statuses, counts or details change. Layout is recomputed on a key change only.
 */
export function structureKey(graph: GraphState, direction: LayoutDirection): string {
  return [
    graph.id,
    direction,
    graph.nodes.map((n) => n.id).join('\u0001'),
    graph.edges.map((e) => `${e.from}>${e.to}`).join('\u0001')
  ].join('\u0000');
}

/**
 * Top-left position per node id. Dagre places everything, then pinned (manually dragged)
 * positions replace the computed ones so they never jump.
 */
export function layoutChannelGraph(
  graph: GraphState,
  direction: LayoutDirection,
  pinned: PositionMap = {}
): Record<string, XY> {
  const { nodesep, ranksep } = LAYOUT_SPACING[direction];
  const g = new dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({ rankdir: direction, nodesep, ranksep, marginx: MARGIN, marginy: MARGIN });
  for (const node of graph.nodes) g.setNode(node.id, { width: NODE_WIDTH, height: NODE_HEIGHT });
  for (const edge of graph.edges) {
    if (g.hasNode(edge.from) && g.hasNode(edge.to)) g.setEdge(edge.from, edge.to);
  }
  dagre.layout(g);

  const positions: Record<string, XY> = {};
  for (const node of graph.nodes) {
    const saved = pinned[node.id];
    if (saved && saved.pinned !== false && Number.isFinite(saved.x) && Number.isFinite(saved.y)) {
      positions[node.id] = { x: saved.x, y: saved.y };
      continue;
    }
    const placed = g.node(node.id);
    positions[node.id] = { x: (placed?.x ?? 0) - NODE_WIDTH / 2, y: (placed?.y ?? 0) - NODE_HEIGHT / 2 };
  }
  return positions;
}
