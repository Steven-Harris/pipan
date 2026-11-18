import type { Node } from '@xyflow/svelte';
import type { GraphNode } from './types';

export type ChannelNodeData = { node: GraphNode };
export type ChannelFlowNode = Node<ChannelNodeData, 'channel'>;
