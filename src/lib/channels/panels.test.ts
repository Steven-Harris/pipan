import { describe, expect, it } from 'vitest';
import { graphPanelId, parseGraphPanelId } from './panels';

describe('graph panel ids', () => {
  it('round-trips, including colons in the topic', () => {
    expect(parseGraphPanelId(graphPanelId('acme.demo', 'graph'))).toEqual({ ch: 'acme.demo', topic: 'graph' });
    expect(parseGraphPanelId(graphPanelId('acme.demo', 'a:b'))).toEqual({ ch: 'acme.demo', topic: 'a:b' });
  });
  it('rejects other ids', () => {
    expect(parseGraphPanelId('nodes')).toBeNull();
    expect(parseGraphPanelId('channel:graph:')).toBeNull();
    expect(parseGraphPanelId('channel:graph:acme')).toBeNull();
    expect(parseGraphPanelId('channel:graph:acme:')).toBeNull();
    expect(parseGraphPanelId('channel:graph::x')).toBeNull();
  });
});
