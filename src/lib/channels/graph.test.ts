import { describe, expect, it } from 'vitest';
import { NODE_HEIGHT, NODE_WIDTH, edgeIds, layoutChannelGraph, structureKey } from './graph-layout';
import { STATUS_STYLES, TONE_CLASSES, kindGlyph, resolveTone } from './graph-style';
import type { GraphState } from './types';

const graph = (overrides: Partial<GraphState> = {}): GraphState => ({
  id: 'g',
  direction: 'TB',
  nodes: [
    { id: 'a', label: 'A', status: 'done' },
    { id: 'b', label: 'B', status: 'active' },
    { id: 'c', label: 'C', status: 'pending' }
  ],
  edges: [
    { from: 'a', to: 'b' },
    { from: 'b', to: 'c' }
  ],
  ...overrides
});

describe('layoutChannelGraph', () => {
  it('ranks along the direction without overlap', () => {
    const tb = layoutChannelGraph(graph(), 'TB');
    expect(tb.b.y).toBeGreaterThanOrEqual(tb.a.y + NODE_HEIGHT);
    expect(tb.c.y).toBeGreaterThanOrEqual(tb.b.y + NODE_HEIGHT);
    const lr = layoutChannelGraph(graph(), 'LR');
    expect(lr.b.x).toBeGreaterThanOrEqual(lr.a.x + NODE_WIDTH);
  });

  it('keeps pinned positions and lays out the rest', () => {
    const free = layoutChannelGraph(graph(), 'TB');
    const pinned = layoutChannelGraph(graph(), 'TB', { b: { x: 900, y: 800, pinned: true } });
    expect(pinned.b).toEqual({ x: 900, y: 800 });
    expect(pinned.a).toEqual(free.a);
  });

  it('releases a node explicitly unpinned and ignores non-finite coordinates', () => {
    const free = layoutChannelGraph(graph(), 'TB');
    const result = layoutChannelGraph(graph(), 'TB', {
      a: { x: 5, y: 5, pinned: false },
      b: { x: Number.NaN, y: 1 }
    });
    expect(result.a).toEqual(free.a);
    expect(result.b).toEqual(free.b);
  });

  it('tolerates edges to unknown nodes, empty graphs, and cycles', () => {
    expect(layoutChannelGraph(graph({ edges: [{ from: 'a', to: 'zzz' }] }), 'TB')).toHaveProperty('c');
    expect(layoutChannelGraph(graph({ nodes: [], edges: [] }), 'TB')).toEqual({});
    const cyclic = graph({ edges: [{ from: 'a', to: 'b' }, { from: 'b', to: 'a' }] });
    expect(Object.keys(layoutChannelGraph(cyclic, 'TB'))).toHaveLength(3);
  });
});

describe('structureKey', () => {
  it('ignores status, counts and details', () => {
    const base = graph();
    const changed = graph({
      nodes: base.nodes.map((n) => ({ ...n, status: 'done' as const, visits: 3 })),
      edges: base.edges.map((e) => ({ ...e, taken: 2 })),
      active: ['c'],
      note: 'x'
    });
    expect(structureKey(changed, 'TB')).toBe(structureKey(base, 'TB'));
  });

  it('changes with nodes, edges, direction and graph id', () => {
    const base = structureKey(graph(), 'TB');
    expect(structureKey(graph({ nodes: [...graph().nodes, { id: 'd', label: 'D', status: 'pending' }] }), 'TB')).not.toBe(base);
    expect(structureKey(graph({ edges: [{ from: 'a', to: 'c' }] }), 'TB')).not.toBe(base);
    expect(structureKey(graph(), 'LR')).not.toBe(base);
    expect(structureKey(graph({ id: 'other' }), 'TB')).not.toBe(base);
  });
});

describe('edgeIds', () => {
  it('uses provided ids and makes duplicates unique', () => {
    const ids = edgeIds(
      graph({ edges: [{ id: 'e1', from: 'a', to: 'b' }, { from: 'a', to: 'b' }, { from: 'a', to: 'b' }] })
    );
    expect(ids[0]).toBe('e1');
    expect(new Set(ids).size).toBe(3);
  });
});

describe('graph style', () => {
  it('uses the producer tone, then a stable per-kind tone, then neutral', () => {
    expect(resolveTone('script', 'red')).toBe('red');
    expect(resolveTone(undefined, undefined)).toBe('neutral');
    expect(resolveTone('script', undefined)).toBe(resolveTone('script', undefined));
    expect(resolveTone('script', undefined)).not.toBe('neutral');
    const tones = new Set(['agent', 'script', 'approval', 'branch', 'loop', 'x1', 'x2'].map((k) => resolveTone(k, undefined)));
    expect(tones.size).toBeGreaterThan(1);
    expect(tones.has('red')).toBe(false);
  });

  it('has classes for every tone and status', () => {
    for (const tone of Object.keys(TONE_CLASSES)) expect(TONE_CLASSES[tone as keyof typeof TONE_CLASSES].border).toBeTruthy();
    for (const status of ['pending', 'active', 'waiting', 'done', 'failed', 'skipped'] as const) {
      expect(STATUS_STYLES[status].label).toBeTruthy();
    }
  });

  it('derives a glyph from the kind', () => {
    expect(kindGlyph('agent')).toBe('A');
    expect(kindGlyph('  x')).toBe('X');
    expect(kindGlyph('   ')).toBe('');
    expect(kindGlyph(undefined)).toBe('');
  });
});
