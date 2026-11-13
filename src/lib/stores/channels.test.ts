import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { MAX_PENDING_REQUESTS, MAX_REQUESTS_PER_WINDOW } from '#lib/channels/limits.ts';
import { channels } from './channels.svelte.ts';

const S = 's1';
const CH = 'acme.demo';
let sent: Record<string, unknown>[] = [];

const widget = (widgetKey: string, lines?: string[]) => ({
  type: 'extension_ui_request',
  id: `w-${Math.random()}`,
  method: 'setWidget',
  widgetKey,
  widgetLines: lines
});

const announce = (sessionId = S) =>
  channels.handleEvent(
    sessionId,
    widget('pipan:shim', [JSON.stringify({ protocol: 1, host: 'pipan', primitives: { graph: 1, form: 1, review: 1 } })])
  );

const stateEvent = (topic: string, seq: number, primitive: string, data: unknown, ch = CH) =>
  widget(`pipan:s:${ch}:${topic}`, [JSON.stringify({ v: 1, ch, seq, payload: { primitive, data } })]);

const graph = (id = 'g') => ({ id, nodes: [{ id: 'a', label: 'A', status: 'active' }], edges: [] });
const form = { title: 'T', actions: [{ id: 'go', label: 'Go' }] };

let piId = 0;
const requestEvent = (kind: string, requestId: string, payload: unknown, ch = CH) => ({
  type: 'extension_ui_request',
  id: `pi-${++piId}`,
  method: 'editor',
  title: `pipan:r:${ch}:${kind}`,
  prefill: JSON.stringify({ v: 1, ch, payload: { requestId, payload } })
});

const pendingFor = (sessionId = S) => channels.pending[sessionId] ?? [];
const cancelledIds = () => sent.filter((c) => c.cancelled === true).map((c) => c.id);

beforeEach(() => {
  vi.useFakeTimers();
  sent = [];
  piId = 0;
  channels.processExited();
  channels.forgetSession(S);
  channels.dismissErrors();
  channels.configure({
    send: async (command) => {
      sent.push(command);
    }
  });
});

afterEach(() => {
  vi.useRealTimers();
});

describe('capability gate', () => {
  it('ignores state until the shim announces', () => {
    channels.handleEvent(S, stateEvent('graph', 1, 'graph', graph()));
    expect(channels.statesFor(S)).toHaveLength(0);
    expect(channels.isActive(S)).toBe(false);
    announce();
    expect(channels.isActive(S)).toBe(true);
    channels.handleEvent(S, stateEvent('graph', 1, 'graph', graph()));
    expect(channels.statesFor(S)).toHaveLength(1);
  });

  it('cancels requests that arrive before the announce', () => {
    channels.handleEvent(S, requestEvent('form', 'r1', form));
    expect(pendingFor()).toHaveLength(0);
    expect(cancelledIds()).toEqual(['pi-1']);
  });

  it('passes non-channel traffic through', () => {
    expect(channels.handleEvent(S, widget('someone-else', ['x']))).toBe(false);
    expect(channels.handleEvent(S, { type: 'extension_ui_request', method: 'editor', id: 'e', title: 'Edit' })).toBe(false);
    expect(channels.handleEvent(S, { type: 'agent_start' })).toBe(false);
    expect(channels.handleEvent(S, { type: 'response', id: 'abc', command: 'get_state' })).toBe(false);
    expect(sent).toEqual([]);
  });

  it('consumes channel widgets and requests', () => {
    expect(announce()).toBe(true);
    expect(channels.handleEvent(S, stateEvent('graph', 1, 'graph', graph()))).toBe(true);
    expect(channels.handleEvent(S, requestEvent('form', 'r1', form))).toBe(true);
  });
});

describe('state', () => {
  beforeEach(() => void announce());

  it('stores the latest validated state per topic and clears on empty widget', () => {
    channels.handleEvent(S, stateEvent('graph', 1, 'graph', graph('one')));
    vi.advanceTimersByTime(100);
    channels.handleEvent(S, stateEvent('graph', 2, 'graph', graph('two')));
    channels.handleEvent(S, stateEvent('other', 1, 'custom', { any: 1 }));
    const states = channels.statesFor(S);
    expect(states).toHaveLength(2);
    expect(states.find((s) => s.topic === 'graph')?.data).toMatchObject({ id: 'two' });

    channels.handleEvent(S, widget(`pipan:s:${CH}:graph`, undefined));
    expect(channels.statesFor(S).map((s) => s.topic)).toEqual(['other']);
  });

  it('keeps states of different sessions apart', () => {
    announce('s2');
    channels.handleEvent(S, stateEvent('graph', 1, 'graph', graph('one')));
    channels.handleEvent('s2', stateEvent('graph', 1, 'graph', graph('two')));
    expect(channels.statesFor(S)[0].data).toMatchObject({ id: 'one' });
    expect(channels.statesFor('s2')[0].data).toMatchObject({ id: 'two' });
    channels.forgetSession('s2');
  });

  it('drops stale snapshots by seq', () => {
    channels.handleEvent(S, stateEvent('graph', 5, 'graph', graph('new')));
    channels.handleEvent(S, stateEvent('graph', 4, 'graph', graph('old')));
    expect(channels.statesFor(S)[0].data).toMatchObject({ id: 'new' });
  });

  it('reassembles chunked widget lines', () => {
    const text = JSON.stringify({ v: 1, ch: CH, seq: 1, payload: { primitive: 'custom', data: { blob: 'x'.repeat(300_000) } } });
    const lines = [];
    for (let i = 0; i < text.length; i += 64 * 1024) lines.push(text.slice(i, i + 64 * 1024));
    channels.handleEvent(S, widget(`pipan:s:${CH}:big`, lines));
    expect((channels.statesFor(S)[0].data as { blob: string }).blob).toHaveLength(300_000);
  });

  it('records an error and keeps the previous state when a payload is invalid', () => {
    channels.handleEvent(S, stateEvent('graph', 1, 'graph', graph('ok')));
    channels.handleEvent(S, stateEvent('graph', 2, 'graph', { id: 'bad' }));
    expect(channels.statesFor(S)[0].data).toMatchObject({ id: 'ok' });
    expect(channels.errors.at(-1)?.message).toMatch(/Extension acme\.demo sent an invalid graph/);
  });

  it('records an error for undecodable widgets', () => {
    channels.handleEvent(S, widget(`pipan:s:${CH}:graph`, ['{broken']));
    expect(channels.statesFor(S)).toHaveLength(0);
    expect(channels.errors).toHaveLength(1);
  });

  it('starts from scratch on a repeated announce, including seq', () => {
    channels.handleEvent(S, stateEvent('graph', 9, 'graph', graph('old')));
    announce();
    expect(channels.statesFor(S)).toHaveLength(0);
    // A restarted shim counts from 1 again and must not be mistaken for stale.
    channels.handleEvent(S, stateEvent('graph', 1, 'graph', graph('fresh')));
    expect(channels.statesFor(S)[0].data).toMatchObject({ id: 'fresh' });
  });

  it('coalesces bursts of graph updates to one per interval, newest wins', () => {
    channels.handleEvent(S, stateEvent('graph', 1, 'graph', graph('first')));
    expect(channels.statesFor(S)[0].data).toMatchObject({ id: 'first' });
    for (let i = 2; i <= 6; i++) channels.handleEvent(S, stateEvent('graph', i, 'graph', graph(`g${i}`)));
    expect(channels.statesFor(S)[0].data).toMatchObject({ id: 'first' });
    vi.advanceTimersByTime(100);
    expect(channels.statesFor(S)[0].data).toMatchObject({ id: 'g6' });
    // After the interval an update applies immediately again.
    vi.advanceTimersByTime(100);
    channels.handleEvent(S, stateEvent('graph', 7, 'graph', graph('g7')));
    expect(channels.statesFor(S)[0].data).toMatchObject({ id: 'g7' });
  });

  it('does not coalesce non-graph primitives', () => {
    channels.handleEvent(S, stateEvent('t', 1, 'custom', { n: 1 }));
    channels.handleEvent(S, stateEvent('t', 2, 'custom', { n: 2 }));
    expect(channels.statesFor(S)[0].data).toEqual({ n: 2 });
  });

  it('a clear cancels a held graph update', () => {
    channels.handleEvent(S, stateEvent('graph', 1, 'graph', graph('first')));
    channels.handleEvent(S, stateEvent('graph', 2, 'graph', graph('held')));
    channels.handleEvent(S, widget(`pipan:s:${CH}:graph`, undefined));
    vi.advanceTimersByTime(500);
    expect(channels.statesFor(S)).toHaveLength(0);
  });

  it('a session change cancels held graph updates', async () => {
    channels.handleEvent(S, stateEvent('graph', 1, 'graph', graph('first')));
    channels.handleEvent(S, stateEvent('graph', 2, 'graph', graph('held')));
    await channels.beforeSessionChange();
    vi.advanceTimersByTime(500);
    expect(channels.statesFor(S)).toHaveLength(0);
  });

  it('surfaces truncation notices', () => {
    const nodes = Array.from({ length: 510 }, (_u, i) => ({ id: `n${i}`, label: 'n', status: 'done' }));
    channels.handleEvent(S, stateEvent('graph', 1, 'graph', { id: 'g', nodes, edges: [] }));
    expect(channels.statesFor(S)[0].notices).toHaveLength(1);
  });
});

describe('requests', () => {
  beforeEach(() => void announce());

  it('queues validated requests in order', () => {
    channels.handleEvent(S, requestEvent('form', 'r1', form));
    channels.handleEvent(S, requestEvent('form', 'r2', form));
    expect(pendingFor().map((r) => r.requestId)).toEqual(['r1', 'r2']);
    expect(pendingFor()[0]).toMatchObject({ ch: CH, kind: 'form', piRequestId: 'pi-1' });
  });

  it('answers through an extension_ui_response the shim can decode', async () => {
    channels.handleEvent(S, requestEvent('form', 'r1', form));
    await channels.answer(S, 'r1', { action: 'go', values: {} });
    expect(pendingFor()).toHaveLength(0);
    const response = sent.at(-1)!;
    expect(response).toMatchObject({ type: 'extension_ui_response', id: 'pi-1' });
    expect(JSON.parse(response.value as string)).toEqual({
      v: 1,
      ch: CH,
      payload: { requestId: 'r1', payload: { action: 'go', values: {} } }
    });
  });

  it('cancels one request', async () => {
    channels.handleEvent(S, requestEvent('form', 'r1', form));
    channels.handleEvent(S, requestEvent('form', 'r2', form));
    await channels.cancel(S, 'r1');
    expect(cancelledIds()).toEqual(['pi-1']);
    expect(pendingFor().map((r) => r.requestId)).toEqual(['r2']);
  });

  it('ignores answers for unknown requests', async () => {
    await channels.answer(S, 'nope', {});
    expect(sent).toEqual([]);
  });

  it('cancels invalid payloads and unparseable requests with an error', () => {
    channels.handleEvent(S, requestEvent('form', 'r1', { title: 'no actions' }));
    channels.handleEvent(S, { ...requestEvent('form', 'r2', form), prefill: 'garbage' });
    expect(pendingFor()).toHaveLength(0);
    expect(cancelledIds()).toEqual(['pi-1', 'pi-2']);
    expect(channels.errors).toHaveLength(2);
  });

  it('queues unknown kinds so the UI can show them as unsupported', () => {
    channels.handleEvent(S, requestEvent('mystery', 'r1', { any: 1 }));
    expect(pendingFor()[0]).toMatchObject({ kind: 'mystery', payload: { any: 1 } });
  });

  it('bounds the pending queue', () => {
    for (let i = 0; i < MAX_PENDING_REQUESTS + 2; i++) channels.handleEvent(S, requestEvent('form', `r${i}`, form));
    expect(pendingFor()).toHaveLength(MAX_PENDING_REQUESTS);
    expect(cancelledIds()).toHaveLength(2);
  });

  it('rate limits requests per window and recovers after it', async () => {
    for (let i = 0; i < MAX_REQUESTS_PER_WINDOW; i++) {
      channels.handleEvent(S, requestEvent('form', `r${i}`, form));
      await channels.cancel(S, `r${i}`);
    }
    sent = [];
    channels.handleEvent(S, requestEvent('form', 'over', form));
    expect(pendingFor()).toHaveLength(0);
    expect(cancelledIds()).toHaveLength(1);

    vi.advanceTimersByTime(61_000);
    channels.handleEvent(S, requestEvent('form', 'again', form));
    expect(pendingFor()).toHaveLength(1);
  });

  it('closes a request the shim reports as expired and cancels the Pi dialog', () => {
    channels.handleEvent(S, requestEvent('form', 'r1', form));
    const expiry = [JSON.stringify({ v: 1, ch: CH, payload: { requestId: 'r1', reason: 'timeout' } })];
    channels.handleEvent(S, widget('pipan:x:r1', expiry));
    expect(pendingFor()).toHaveLength(0);
    expect(cancelledIds()).toEqual(['pi-1']);
    // The shim clears the widget right after; that must be harmless.
    channels.handleEvent(S, widget('pipan:x:r1', undefined));
    expect(cancelledIds()).toEqual(['pi-1']);
  });
});

describe('lifecycle', () => {
  beforeEach(() => void announce());

  it('cancelAll answers every pending request', async () => {
    channels.handleEvent(S, requestEvent('form', 'r1', form));
    channels.handleEvent(S, requestEvent('form', 'r2', form));
    await channels.cancelAll();
    expect(pendingFor()).toHaveLength(0);
    expect(cancelledIds().sort()).toEqual(['pi-1', 'pi-2']);
  });

  it('beforeSessionChange cancels requests and drops state until the next announce', async () => {
    channels.handleEvent(S, stateEvent('graph', 1, 'graph', graph()));
    channels.handleEvent(S, requestEvent('form', 'r1', form));
    await channels.beforeSessionChange();
    expect(cancelledIds()).toEqual(['pi-1']);
    expect(channels.statesFor(S)).toHaveLength(0);
    expect(channels.isActive(S)).toBe(false);
    announce();
    expect(channels.isActive(S)).toBe(true);
  });

  it('processExited clears everything without sending', () => {
    channels.handleEvent(S, stateEvent('graph', 1, 'graph', graph()));
    channels.handleEvent(S, requestEvent('form', 'r1', form));
    sent = [];
    channels.processExited();
    expect(channels.statesFor(S)).toHaveLength(0);
    expect(pendingFor()).toHaveLength(0);
    expect(channels.isActive(S)).toBe(false);
    expect(sent).toEqual([]);
  });
});

describe('intents and attach', () => {
  it('does not send intents before the shim announces', async () => {
    expect(await channels.sendIntent(S, CH, 'x', {})).toBe(false);
    expect(sent).toEqual([]);
  });

  it('sends intents as the hidden command once active', async () => {
    announce();
    expect(await channels.sendIntent(S, CH, 'graph.action', { actionId: 'a' })).toBe(true);
    expect(sent[0]).toMatchObject({ type: 'prompt' });
    expect((sent[0].message as string).startsWith('/pipan-intent ')).toBe(true);
  });

  it('attach asks for commands and resyncs only when the shim command exists', async () => {
    await channels.attach();
    const request = sent[0];
    expect(request.type).toBe('get_commands');
    const id = request.id as string;

    expect(channels.handleEvent(S, { type: 'response', id, command: 'get_commands', success: true, data: { commands: [{ name: 'other' }] } })).toBe(true);
    expect(sent).toHaveLength(1);

    await channels.attach();
    const id2 = sent[1].id as string;
    channels.handleEvent(S, { type: 'response', id: id2, command: 'get_commands', success: true, data: { commands: [{ name: 'pipan-intent' }] } });
    expect(sent).toHaveLength(3);
    const resync = sent[2];
    expect(JSON.parse((resync.message as string).slice('/pipan-intent '.length))).toMatchObject({ ch: '_shim', name: 'resync' });
  });
});

describe('log', () => {
  const entry = (id: string, customType: string, data: unknown) => ({ type: 'custom', id, customType, data, timestamp: 't' });
  const message = (id: string, customType: string) => ({
    type: 'custom_message',
    id,
    customType,
    content: 'hello',
    details: { k: 1 }
  });
  const respond = (request: Record<string, unknown>, entries: unknown[], success = true) =>
    channels.handleEvent(S, { type: 'response', id: request.id, command: 'get_entries', success, data: { entries } });

  it('loads on announce, then fetches only entries after the cursor', async () => {
    announce();
    await vi.advanceTimersByTimeAsync(60);
    expect(sent[0]).toMatchObject({ type: 'get_entries' });
    expect(sent[0]).not.toHaveProperty('since');

    respond(sent[0], [
      { type: 'message', id: 'e1' },
      entry('e2', 'pipan:l:acme.demo:note', { n: 1 }),
      entry('e3', 'unrelated', {}),
      message('e4', 'pipan:l:acme.demo:note')
    ]);
    expect(channels.log[S]).toEqual([
      { entryId: 'e2', kind: 'entry', ch: CH, name: 'note', data: { n: 1 }, timestamp: 't' },
      { entryId: 'e4', kind: 'message', ch: CH, name: 'note', data: { k: 1 }, content: 'hello', timestamp: undefined }
    ]);

    channels.handleEvent(S, { type: 'entry_appended', entry: { customType: 'pipan:l:acme.demo:note' } });
    await vi.advanceTimersByTimeAsync(60);
    expect(sent.at(-1)).toMatchObject({ type: 'get_entries', since: 'e4' });
    respond(sent.at(-1)!, [entry('e5', 'pipan:l:acme.demo:note', { n: 2 })]);
    expect(channels.log[S].map((i) => i.entryId)).toEqual(['e2', 'e4', 'e5']);
  });

  it('triggers a sync for custom messages but leaves them for the transcript', async () => {
    announce();
    await vi.advanceTimersByTimeAsync(60);
    respond(sent[0], []);
    const before = sent.length;
    const consumed = channels.handleEvent(S, { type: 'message_end', message: { role: 'custom', customType: 'pipan:l:acme.demo:note' } });
    expect(consumed).toBe(false);
    await vi.advanceTimersByTimeAsync(60);
    expect(sent.length).toBe(before + 1);
  });

  it('ignores unrelated entries and coalesces bursts into one fetch', async () => {
    announce();
    await vi.advanceTimersByTimeAsync(60);
    respond(sent[0], []);
    sent = [];
    channels.handleEvent(S, { type: 'entry_appended', entry: { customType: 'other' } });
    await vi.advanceTimersByTimeAsync(60);
    expect(sent).toHaveLength(0);
    for (let i = 0; i < 5; i++) channels.handleEvent(S, { type: 'entry_appended', entry: { customType: 'pipan:l:acme.demo:n' } });
    await vi.advanceTimersByTimeAsync(60);
    expect(sent).toHaveLength(1);
  });

  it('reruns once when a trigger arrives while a fetch is in flight', async () => {
    announce();
    await vi.advanceTimersByTimeAsync(60);
    const first = sent[0];
    channels.handleEvent(S, { type: 'entry_appended', entry: { customType: 'pipan:l:acme.demo:n' } });
    await vi.advanceTimersByTimeAsync(60);
    expect(sent).toHaveLength(1); // still waiting on the first response
    respond(first, [entry('e1', 'pipan:l:acme.demo:n', {})]);
    await vi.advanceTimersByTimeAsync(0);
    expect(sent).toHaveLength(2);
    expect(sent[1]).toMatchObject({ since: 'e1' });
  });

  it('reloads everything when Pi does not know the cursor', async () => {
    announce();
    await vi.advanceTimersByTimeAsync(60);
    respond(sent[0], [entry('e1', 'pipan:l:acme.demo:n', { v: 1 })]);

    channels.handleEvent(S, { type: 'entry_appended', entry: { customType: 'pipan:l:acme.demo:n' } });
    await vi.advanceTimersByTimeAsync(60);
    respond(sent.at(-1)!, [], false); // since "e1" not found
    await vi.advanceTimersByTimeAsync(0);
    const reload = sent.at(-1)!;
    expect(reload).toMatchObject({ type: 'get_entries' });
    expect(reload).not.toHaveProperty('since');
    respond(reload, [entry('f1', 'pipan:l:acme.demo:n', { v: 2 })]);
    expect(channels.log[S].map((i) => i.entryId)).toEqual(['f1']);
  });

  it('keeps the log and cursor across a process exit', async () => {
    announce();
    await vi.advanceTimersByTimeAsync(60);
    respond(sent[0], [entry('e1', 'pipan:l:acme.demo:n', {})]);
    channels.processExited();
    expect(channels.log[S]).toHaveLength(1);
    announce();
    await vi.advanceTimersByTimeAsync(60);
    expect(sent.at(-1)).toMatchObject({ type: 'get_entries', since: 'e1' });
  });
});
