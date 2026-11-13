import { flushSync } from 'svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { channels } from '#lib/stores/channels.svelte.ts';
import { channelPlugins, type TopicSnapshot } from './plugin-api.svelte.ts';

const widget = (widgetKey: string, lines: string[]) => ({
  type: 'extension_ui_request',
  id: 'w',
  method: 'setWidget',
  widgetKey,
  widgetLines: lines
});
const announce = (session: string) =>
  channels.handleEvent(session, widget('pipan:shim', [JSON.stringify({ protocol: 1, host: 'pipan', primitives: { graph: 1 } })]));
let seq = 0;
const publish = (session: string, topic: string, data: unknown) =>
  channels.handleEvent(
    session,
    widget(`pipan:s:acme.demo:${topic}`, [JSON.stringify({ v: 1, ch: 'acme.demo', seq: ++seq, payload: { primitive: 'custom', data } })])
  );

let session = $state('s1');
const sent: Record<string, unknown>[] = [];

beforeEach(() => {
  channels.processExited();
  channels.configure({ send: async (command) => void sent.push(command) });
  sent.length = 0;
  session = 's1';
  channelPlugins.configure({ activeSession: () => session });
  announce('s1');
  announce('s2');
});

describe('channel(ch).subscribe', () => {
  it('replays the current value, then every change', () => {
    publish('s1', 'status', { n: 1 });
    const seen: (TopicSnapshot | null)[] = [];
    const stop = channelPlugins.channel('acme.demo').subscribe('status', (s) => seen.push(s));
    flushSync();
    expect(seen).toHaveLength(1);
    expect(seen[0]).toMatchObject({ topic: 'status', primitive: 'custom', data: { n: 1 } });

    publish('s1', 'status', { n: 2 });
    flushSync();
    expect(seen.at(-1)?.data).toEqual({ n: 2 });
    stop();
  });

  it('gets null when nothing is published, then the value once it is', () => {
    const seen: (TopicSnapshot | null)[] = [];
    const stop = channelPlugins.channel('acme.demo').subscribe('status', (s) => seen.push(s));
    flushSync();
    expect(seen).toEqual([null]);
    publish('s1', 'status', { n: 1 });
    flushSync();
    expect(seen).toHaveLength(2);
    stop();
  });

  it('follows the viewed session', () => {
    publish('s1', 'status', { from: 's1' });
    publish('s2', 'status', { from: 's2' });
    const seen: (TopicSnapshot | null)[] = [];
    const stop = channelPlugins.channel('acme.demo').subscribe('status', (s) => seen.push(s));
    flushSync();
    session = 's2';
    flushSync();
    expect(seen.map((s) => (s?.data as { from: string }).from)).toEqual(['s1', 's2']);
    stop();
  });

  it('ignores other channels and topics, and stops after unsubscribe', () => {
    const seen: unknown[] = [];
    const stop = channelPlugins.channel('acme.demo').subscribe('status', (s) => seen.push(s));
    flushSync();
    channels.handleEvent(
      's1',
      widget('pipan:s:other.ch:status', [JSON.stringify({ v: 1, ch: 'other.ch', seq: ++seq, payload: { primitive: 'custom', data: 1 } })])
    );
    publish('s1', 'elsewhere', 1);
    flushSync();
    expect(seen).toEqual([null]);
    stop();
    publish('s1', 'status', { n: 9 });
    flushSync();
    expect(seen).toEqual([null]);
  });

  it('survives a callback that throws', () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    const stop = channelPlugins.channel('acme.demo').subscribe('status', () => {
      throw new Error('boom');
    });
    flushSync();
    expect(error).toHaveBeenCalled();
    stop();
    error.mockRestore();
  });
});

describe('channel(ch).onRequest', () => {
  it('registers a handler for that channel and kind only', () => {
    const handler = vi.fn();
    const stop = channelPlugins.channel('acme.demo').onRequest('pick', handler);
    expect(channelPlugins.requestHandler('acme.demo', 'pick')).toBe(handler);
    expect(channelPlugins.requestHandler('acme.demo', 'form')).toBeUndefined();
    expect(channelPlugins.requestHandler('other.ch', 'pick')).toBeUndefined();
    stop();
    expect(channelPlugins.requestHandler('acme.demo', 'pick')).toBeUndefined();
  });

  it('keeps a newer handler when an older one unsubscribes', () => {
    const first = vi.fn();
    const second = vi.fn();
    const stopFirst = channelPlugins.channel('acme.demo').onRequest('pick', first);
    channelPlugins.channel('acme.demo').onRequest('pick', second);
    stopFirst();
    expect(channelPlugins.requestHandler('acme.demo', 'pick')).toBe(second);
  });
});

describe('channel(ch).intent', () => {
  it('sends to the viewed session', async () => {
    expect(await channelPlugins.channel('acme.demo').intent('refresh', { n: 1 })).toBe(true);
    const command = sent.at(-1) as { type: string; message: string };
    expect(command.type).toBe('prompt');
    expect(command.message).toBe('/pipan-intent {"v":1,"ch":"acme.demo","name":"refresh","payload":{"n":1}}');
  });

  it('resolves false when no producer announced for the session', async () => {
    session = 's-none';
    expect(await channelPlugins.channel('acme.demo').intent('refresh')).toBe(false);
  });
});
