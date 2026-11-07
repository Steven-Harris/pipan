import dagre from 'dagre';
import { Position, type Edge, type Node } from '@xyflow/svelte';
import type { FlowNodeMeta, FlowNodeType } from '#lib/types/pi.ts';

export type FlowNode = Node<FlowNodeMeta, FlowNodeType>;
export type LayoutDirection = 'TB' | 'LR';

export interface SavedPosition {
  x: number;
  y: number;
  /** Absent means pinned: only an explicit `false` releases a node back to dagre. */
  pinned?: boolean;
}
export type PositionMap = Record<string, SavedPosition>;

interface Size {
  width: number;
  height: number;
}

/** Fallback sizes (match the fixed widths of the node components) until a node is measured. */
const NODE_SIZES: Record<FlowNodeType, Size> = {
  pipan: { width: 244, height: 112 },
  orchestrator: { width: 260, height: 112 },
  worker: { width: 276, height: 132 },
  action: { width: 244, height: 96 },
  approval: { width: 290, height: 172 }
};

/**
 * Spacing is tuned per direction: `TB` ranks are separated by node height, `LR` ranks by
 * node width (much larger), so LR needs the wider rank gap and the tighter cross gap.
 */
export const LAYOUT_SPACING: Record<LayoutDirection, { nodesep: number; ranksep: number }> = {
  TB: { nodesep: 28, ranksep: 44 },
  LR: { nodesep: 24, ranksep: 72 }
};
const MARGIN = 20;

export interface LayoutOptions {
  /** User-placed coordinates for the active direction; these nodes are never moved. */
  pinned?: PositionMap;
  /**
   * Current positions of nodes already on the canvas in this layout context. An unpinned
   * node under an anchored parent keeps its spot; one absent from this map is new.
   */
  placed?: ReadonlyMap<string, { x: number; y: number }>;
  /** Rendered sizes from the canvas, preferred over the type-based estimates. */
  measured?: ReadonlyMap<string, Size>;
}

export function estimatedSize(node: Pick<FlowNode, 'type'>): Size {
  return NODE_SIZES[(node.type as FlowNodeType) ?? 'pipan'] ?? NODE_SIZES.pipan;
}

function isPinned(p: SavedPosition | undefined): p is SavedPosition {
  return Boolean(p) && p!.pinned !== false && Number.isFinite(p!.x) && Number.isFinite(p!.y);
}

/** Handle sides for a direction; node components render their `<Handle>`s from these. */
export function handlePositions(direction: LayoutDirection): {
  sourcePosition: Position;
  targetPosition: Position;
} {
  return direction === 'LR'
    ? { sourcePosition: Position.Right, targetPosition: Position.Left }
    : { sourcePosition: Position.Bottom, targetPosition: Position.Top };
}

/**
 * Lays the graph out as a directed acyclic graph with dagre. Pure: returns new node
 * objects with `position` and handle sides set; edges are returned untouched.
 *
 * - Pinned nodes keep their saved coordinates and never jump.
 * - A node under an anchored parent (pinned, or itself anchored this way) that was already
 *   on the canvas stays put; a new one goes directly beside the parent along the rank
 *   axis: below it in `TB`, to its right in `LR`. Everything else takes its dagre slot.
 */
export function layoutGraph(
  rawNodes: FlowNode[],
  rawEdges: Edge[],
  direction: LayoutDirection,
  options: LayoutOptions = {}
): { nodes: FlowNode[]; edges: Edge[] } {
  const { pinned = {}, placed = new Map<string, { x: number; y: number }>(), measured } = options;
  const { nodesep, ranksep } = LAYOUT_SPACING[direction];
  const sizeOf = (node: FlowNode): Size => measured?.get(node.id) ?? estimatedSize(node);

  const g = new dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({ rankdir: direction, nodesep, ranksep, marginx: MARGIN, marginy: MARGIN });

  for (const node of rawNodes) {
    // Dagre writes x/y back onto the label object, so every node needs its own copy.
    g.setNode(node.id, { ...sizeOf(node) });
  }
  const edges = rawEdges.filter((e) => g.hasNode(e.source) && g.hasNode(e.target));
  const parentOf = new Map<string, string>();
  const childCount = new Map<string, number>();
  for (const edge of edges) {
    // Dagre would implicitly create unknown endpoints; dangling edges were dropped above.
    g.setEdge(edge.source, edge.target);
    if (!parentOf.has(edge.target)) parentOf.set(edge.target, edge.source);
  }

  dagre.layout(g);

  const horizontal = direction === 'LR';
  const handles = handlePositions(direction);
  /** Top-left position and size of every node whose final position is fixed so far. */
  const anchored = new Map<string, { x: number; y: number } & Size>();

  const nodes = rawNodes.map((node) => {
    const { width, height } = sizeOf(node);
    const saved = pinned[node.id];
    let x: number;
    let y: number;

    const parent = parentOf.get(node.id);
    const anchor = parent ? anchored.get(parent) : undefined;

    if (isPinned(saved)) {
      x = saved.x;
      y = saved.y;
      anchored.set(node.id, { x, y, width, height });
    } else if (anchor) {
      const current = placed.get(node.id);
      if (current) {
        x = current.x;
        y = current.y;
      } else {
        const k = childCount.get(parent!) ?? 0;
        childCount.set(parent!, k + 1);
        if (horizontal) {
          x = anchor.x + anchor.width + ranksep;
          y = anchor.y + (anchor.height - height) / 2 + k * (height + nodesep);
        } else {
          x = anchor.x + (anchor.width - width) / 2 + k * (width + nodesep);
          y = anchor.y + anchor.height + ranksep;
        }
      }
      anchored.set(node.id, { x, y, width, height });
    } else {
      const pos = g.node(node.id);
      x = (pos?.x ?? 0) - width / 2;
      y = (pos?.y ?? 0) - height / 2;
    }

    return { ...node, ...handles, position: { x, y } };
  });

  return { nodes, edges };
}
