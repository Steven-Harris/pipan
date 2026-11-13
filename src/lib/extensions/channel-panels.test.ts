import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => []) }));

import { graphPanelId } from '#lib/channels/panels.ts';
import { channels } from '#lib/stores/channels.svelte.ts';
import { getPanelDefinition } from '#lib/workspace/panels.ts';
import { extensionRegistry } from './registry.svelte.ts';

const S = 's1';
const widget = (widgetKey: string, lines: string[]) => ({
  type: 'extension_ui_request',
  id: 'w',
  method: 'setWidget',
  widgetKey,
  widgetLines: lines
});
const publish = (topic: string, primitive: string, data: unknown) =>
  channels.handleEvent(
    S,
    widget(`pipan:s:acme.demo:${topic}`, [JSON.stringify({ v: 1, ch: 'acme.demo', seq: 1, payload: { primitive, data } })])
  );

describe('channel graph panels', () => {
  beforeEach(() => {
    channels.processExited();
    channels.handleEvent(
      S,
      widget('pipan:shim', [JSON.stringify({ protocol: 1, host: 'pipan', primitives: { graph: 1 } })])
    );
  });

  it('offers one panel per published graph topic and none for other primitives', () => {
    publish('graph', 'graph', { id: 'g', title: 'Demo run', nodes: [{ id: 'a', label: 'A', status: 'done' }], edges: [] });
    publish('other', 'custom', { x: 1 });
    const ids = extensionRegistry.contributedPanels.filter((p) => p.extensionId === 'pipan.channels').map((p) => p.id);
    expect(ids).toEqual([graphPanelId('acme.demo', 'graph')]);
    expect(extensionRegistry.contributedPanelsById[ids[0]].label).toBe('Demo run');
    expect(extensionRegistry.isPanelEnabled(ids[0])).toBe(true);
    expect(getPanelDefinition(ids[0]).label).toBe('Demo run');
  });

  it('removes the picker entry when the state is cleared', () => {
    publish('graph', 'graph', { id: 'g', nodes: [{ id: 'a', label: 'A', status: 'done' }], edges: [] });
    channels.handleEvent(S, { type: 'extension_ui_request', id: 'w', method: 'setWidget', widgetKey: 'pipan:s:acme.demo:graph' });
    expect(extensionRegistry.contributedPanels.some((p) => p.extensionId === 'pipan.channels')).toBe(false);
  });

  it('falls back to the topic as the label before a title is published', () => {
    expect(getPanelDefinition(graphPanelId('acme.demo', 'graph')).label).toBe('graph');
  });
});
