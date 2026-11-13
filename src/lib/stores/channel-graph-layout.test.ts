import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const { channelGraphLayout } = await import('./channel-graph-layout.svelte.ts');

const ref = (ch: string, topic: string, graphId = 'run') => ({ sessionId: 's1', ch, topic, graphId });

describe('channelGraphLayout', () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockResolvedValue(undefined);
  });

  it('keeps graphs that share a graph id apart by channel and topic', () => {
    channelGraphLayout.pin(ref('acme.one', 'graph'), { n: { x: 1, y: 1 } });
    channelGraphLayout.pin(ref('acme.one', 'graph.1'), { n: { x: 2, y: 2 } });
    channelGraphLayout.pin(ref('acme.two', 'graph'), { n: { x: 3, y: 3 } });

    expect(channelGraphLayout.positionsFor(ref('acme.one', 'graph')).n).toMatchObject({ x: 1, y: 1 });
    expect(channelGraphLayout.positionsFor(ref('acme.one', 'graph.1')).n).toMatchObject({ x: 2, y: 2 });
    expect(channelGraphLayout.positionsFor(ref('acme.two', 'graph')).n).toMatchObject({ x: 3, y: 3 });
    expect(channelGraphLayout.pinnedCount(ref('acme.one', 'graph'))).toBe(1);

    channelGraphLayout.reset(ref('acme.one', 'graph'));
    expect(channelGraphLayout.pinnedCount(ref('acme.one', 'graph'))).toBe(0);
    expect(channelGraphLayout.pinnedCount(ref('acme.one', 'graph.1'))).toBe(1);
  });

  it('sends the channel and topic to the backend', async () => {
    const r = ref('acme.load', 'graph');
    invoke.mockResolvedValueOnce({ a: { x: 5, y: 6, pinned: true } });
    await channelGraphLayout.hydrate(r);
    expect(invoke).toHaveBeenCalledWith('load_channel_node_positions', {
      sessionId: 's1',
      ch: 'acme.load',
      topic: 'graph',
      graphId: 'run'
    });
    expect(channelGraphLayout.isReady(r)).toBe(true);
    expect(channelGraphLayout.positionsFor(r).a).toMatchObject({ x: 5, y: 6 });

    channelGraphLayout.pin(r, { a: { x: 7, y: 8 } });
    expect(invoke).toHaveBeenLastCalledWith('save_channel_node_positions', {
      sessionId: 's1',
      ch: 'acme.load',
      topic: 'graph',
      graphId: 'run',
      positions: { a: { x: 7, y: 8, pinned: true } }
    });
    channelGraphLayout.reset(r);
    expect(invoke).toHaveBeenLastCalledWith('clear_channel_node_positions', {
      sessionId: 's1',
      ch: 'acme.load',
      topic: 'graph',
      graphId: 'run'
    });
  });
});
