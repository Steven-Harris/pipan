import { describe, expect, it } from 'vitest';
import { newlyWaiting, waitingWithActions } from './attention';
import type { GraphNode, GraphState } from './types';

const node = (id: string, status: GraphNode['status'], withActions = true): GraphNode => ({
  id,
  label: id,
  status,
  actions: withActions ? [{ id: 'go', label: 'Go' }] : undefined
});
const graph = (...nodes: GraphNode[]): GraphState => ({ id: 'g', direction: 'TB', nodes, edges: [] });

describe('waitingWithActions', () => {
  it('lists waiting nodes that have actions', () => {
    const g = graph(node('a', 'waiting'), node('b', 'waiting', false), node('c', 'active'), node('d', 'done'));
    expect(waitingWithActions(g)).toEqual(['a']);
  });
});

describe('newlyWaiting', () => {
  it('reports everything that waits when there is no previous graph', () => {
    expect(newlyWaiting(undefined, graph(node('a', 'waiting'), node('b', 'active')))).toEqual(['a']);
  });
  it('reports a node that changed to waiting', () => {
    expect(newlyWaiting(graph(node('a', 'active')), graph(node('a', 'waiting')))).toEqual(['a']);
  });
  it('does not report a node that was already waiting', () => {
    expect(newlyWaiting(graph(node('a', 'waiting')), graph(node('a', 'waiting')))).toEqual([]);
  });
  it('reports a waiting node that gained actions', () => {
    expect(newlyWaiting(graph(node('a', 'waiting', false)), graph(node('a', 'waiting')))).toEqual(['a']);
  });
  it('reports a node again after it stopped waiting and waited again', () => {
    const first = graph(node('a', 'waiting'));
    const between = graph(node('a', 'active'));
    expect(newlyWaiting(first, between)).toEqual([]);
    expect(newlyWaiting(between, first)).toEqual(['a']);
  });
});
