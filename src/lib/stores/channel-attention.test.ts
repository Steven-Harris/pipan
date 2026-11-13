import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => []) }));

import { graphPanelId } from '#lib/channels/panels.ts';
import { MAX_AUTO_OPENED_PANELS } from '#lib/channels/attention.ts';
import { channelAttention } from './channel-attention.svelte.ts';
import { channels } from './channels.svelte.ts';
import { workspaceStore } from './workspace.svelte.ts';

const S = 's1';
const CH = 'acme.demo';
let seq = 0;

const widget = (widgetKey: string, lines: string[]) => ({
  type: 'extension_ui_request',
  id: 'w',
  method: 'setWidget',
  widgetKey,
  widgetLines: lines
});
const publish = (topic: string, nodes: unknown[], session = S) =>
  channels.handleEvent(
    session,
    widget(`pipan:s:${CH}:${topic}`, [
      JSON.stringify({ v: 1, ch: CH, seq: ++seq, payload: { primitive: 'graph', data: { id: topic, nodes, edges: [] } } })
    ])
  );
const node = (id: string, status: string, actions = true) => ({
  id,
  label: id,
  status,
  actions: actions ? [{ id: 'go', label: 'Go' }] : undefined
});
const announce = (session = S) =>
  channels.handleEvent(session, widget('pipan:shim', [JSON.stringify({ protocol: 1, host: 'pipan', primitives: { graph: 1 } })]));

describe('channelAttention', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    channels.processExited();
    channelAttention.resetForTests();
    for (const id of [...workspaceStore.openPanels]) workspaceStore.closePanel(id);
    workspaceStore.closePanelArea();
    channelAttention.init();
    announce();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('opens a graph panel on its first publish and activates it', () => {
    publish('graph', [node('a', 'active')]);
    expect(workspaceStore.isOpen(graphPanelId(CH, 'graph'))).toBe(true);
    expect(workspaceStore.activePanel).toBe(graphPanelId(CH, 'graph'));
    expect(workspaceStore.panelOpen).toBe(true);
  });

  it('does not reopen a panel the user closed', () => {
    publish('graph', [node('a', 'active')]);
    workspaceStore.closePanel(graphPanelId(CH, 'graph'));
    vi.advanceTimersByTime(200);
    publish('graph', [node('a', 'done')]);
    expect(workspaceStore.isOpen(graphPanelId(CH, 'graph'))).toBe(false);
  });

  it('does not reopen it after the session was left and revisited', () => {
    publish('graph', [node('a', 'active')]);
    workspaceStore.closePanel(graphPanelId(CH, 'graph'));
    channels.processExited();
    announce();
    publish('graph', [node('a', 'active')]);
    expect(workspaceStore.isOpen(graphPanelId(CH, 'graph'))).toBe(false);
  });

  it('opens at most the capped number of panels per session', () => {
    for (let i = 0; i < MAX_AUTO_OPENED_PANELS + 2; i++) publish(`t${i}`, [node('a', 'active')]);
    expect(workspaceStore.openPanels).toHaveLength(MAX_AUTO_OPENED_PANELS);
    expect(workspaceStore.isOpen(graphPanelId(CH, `t${MAX_AUTO_OPENED_PANELS}`))).toBe(false);
  });

  it('counts the cap per session', () => {
    for (let i = 0; i < MAX_AUTO_OPENED_PANELS; i++) publish(`t${i}`, [node('a', 'active')]);
    for (const id of [...workspaceStore.openPanels]) workspaceStore.closePanel(id);
    channels.processExited();
    announce('s2');
    publish('other', [node('a', 'active')], 's2');
    expect(workspaceStore.isOpen(graphPanelId(CH, 'other'))).toBe(true);
  });

  it('does not count a panel that was already open, and keeps the active tab', () => {
    workspaceStore.openPanel(graphPanelId(CH, 'graph'));
    workspaceStore.openPanel('terminal');
    publish('graph', [node('a', 'active')]);
    expect(workspaceStore.activePanel).toBe('terminal');
  });

  it('asks the panel to select a node that starts waiting', () => {
    const id = graphPanelId(CH, 'graph');
    publish('graph', [node('a', 'active'), node('b', 'pending')]);
    expect(channelAttention.focus[id]).toBeUndefined();
    vi.advanceTimersByTime(200);
    publish('graph', [node('a', 'done'), node('b', 'waiting')]);
    expect(channelAttention.focus[id]).toBe('b');
    channelAttention.consumeFocus(id);
    expect(channelAttention.focus[id]).toBeUndefined();
  });

  it('marks a hidden panel and clears the mark when it becomes visible', () => {
    const id = graphPanelId(CH, 'graph');
    publish('graph', [node('a', 'active')]);
    workspaceStore.openPanel('terminal');
    vi.advanceTimersByTime(200);
    publish('graph', [node('a', 'waiting')]);
    expect(channelAttention.unseen[id]).toBe(true);
    workspaceStore.setActivePanel(id);
    flushSync();
    expect(channelAttention.unseen[id]).toBeUndefined();
  });

  it('drops the mark when the run moves past the waiting node', () => {
    const id = graphPanelId(CH, 'graph');
    publish('graph', [node('a', 'active')]);
    workspaceStore.openPanel('terminal');
    vi.advanceTimersByTime(200);
    publish('graph', [node('a', 'waiting')]);
    expect(channelAttention.needsAttention(id)).toBe(true);
    vi.advanceTimersByTime(200);
    publish('graph', [node('a', 'done')]);
    expect(channelAttention.unseen[id]).toBeUndefined();
    expect(channelAttention.needsAttention(id)).toBe(false);
  });

  it('shows no mark once the graph is cleared', () => {
    const id = graphPanelId(CH, 'graph');
    publish('graph', [node('a', 'active')]);
    workspaceStore.openPanel('terminal');
    vi.advanceTimersByTime(200);
    publish('graph', [node('a', 'waiting')]);
    expect(channelAttention.needsAttention(id)).toBe(true);
    channels.handleEvent(S, widget(`pipan:s:${CH}:graph`, undefined as never));
    expect(channelAttention.needsAttention(id)).toBe(false);
  });

  it('does not mark a panel the user is looking at', () => {
    const id = graphPanelId(CH, 'graph');
    publish('graph', [node('a', 'active')]);
    vi.advanceTimersByTime(200);
    publish('graph', [node('a', 'waiting')]);
    expect(channelAttention.unseen[id]).toBeUndefined();
  });

  it('does nothing for a node that waits without actions', () => {
    const id = graphPanelId(CH, 'graph');
    publish('graph', [node('a', 'active')]);
    vi.advanceTimersByTime(200);
    publish('graph', [node('a', 'waiting', false)]);
    expect(channelAttention.focus[id]).toBeUndefined();
  });

  it('clears marks when the process exits', () => {
    const id = graphPanelId(CH, 'graph');
    publish('graph', [node('a', 'waiting')]);
    workspaceStore.openPanel('terminal');
    vi.advanceTimersByTime(200);
    publish('graph', [node('a', 'active'), node('b', 'waiting')]);
    expect(channelAttention.focus[id]).toBe('b');
    channels.processExited();
    expect(channelAttention.focus[id]).toBeUndefined();
    expect(channelAttention.unseen[id]).toBeUndefined();
  });
});
