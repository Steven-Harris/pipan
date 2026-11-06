import dagre from 'dagre';
import { Position, type Edge, type Node } from '@xyflow/svelte';
import type { FlowNodeMeta, FlowNodeType } from '#lib/types/pi.ts';

export type FlowNode = Node<FlowNodeMeta, FlowNodeType>;
export type LayoutDirection = 'TB' | 'LR';

/** Approximate rendered sizes; dagre only needs them to space ranks. */
const NODE_SIZES: Record<FlowNodeType, { width: number; height: number }> = {
  pipan: { width: 244, height: 112 },
  orchestrator: { width: 260, height: 112 },
  worker: { width: 276, height: 132 },
  action: { width: 244, height: 96 },
  approval: { width: 276, height: 156 }
};

function sizeOf(node: FlowNode): { width: number; height: number } {
  return NODE_SIZES[(node.type as FlowNodeType) ?? 'pipan'] ?? NODE_SIZES.pipan;
}

/**
 * Lays the graph out as a directed acyclic graph with dagre. Pure: returns new node
 * objects with `position` and handle sides set; edges are returned untouched.
 */
export function layoutGraph(
  rawNodes: FlowNode[],
  rawEdges: Edge[],
  direction: LayoutDirection
): { nodes: FlowNode[]; edges: Edge[] } {
  const g = new dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({
    rankdir: direction,
    nodesep: direction === 'TB' ? 48 : 36,
    ranksep: direction === 'TB' ? 68 : 84,
    marginx: 32,
    marginy: 32
  });

  for (const node of rawNodes) {
    g.setNode(node.id, sizeOf(node));
  }
  for (const edge of rawEdges) {
    // Dagre would implicitly create unknown endpoints; skip dangling edges instead.
    if (g.hasNode(edge.source) && g.hasNode(edge.target)) {
      g.setEdge(edge.source, edge.target);
    }
  }

  dagre.layout(g);

  const horizontal = direction === 'LR';
  const targetPosition = horizontal ? Position.Left : Position.Top;
  const sourcePosition = horizontal ? Position.Right : Position.Bottom;

  const nodes = rawNodes.map((node) => {
    const pos = g.node(node.id);
    const { width, height } = sizeOf(node);
    return {
      ...node,
      targetPosition,
      sourcePosition,
      position: {
        x: (pos?.x ?? 0) - width / 2,
        y: (pos?.y ?? 0) - height / 2
      }
    };
  });

  return { nodes, edges: rawEdges.filter((e) => g.hasNode(e.source) && g.hasNode(e.target)) };
}
