// Drives the channels store with the event stream of a real `pi --mode rpc --offline` process
// running the shim and the reference producer. Skipped when `pi` is not on PATH.
import { spawn, spawnSync, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { channels, type ChannelPiEvent } from '../src/lib/stores/channels.svelte.ts';

const hasPi = !spawnSync('pi', ['--version']).error;
const root = resolve(import.meta.dirname, '..');
const SID = 'real-session';

describe.skipIf(!hasPi)('channels store against real Pi', () => {
  let pi: ChildProcessWithoutNullStreams;
  let cwd: string;
  const events: ChannelPiEvent[] = [];
  let nextId = 0;

  const send = (command: Record<string, unknown>) => {
    pi.stdin.write(`${JSON.stringify(command)}\n`);
    return Promise.resolve();
  };
  const until = async (check: () => boolean, what: string, timeoutMs = 8000) => {
    const start = Date.now();
    while (!check()) {
      if (Date.now() - start > timeoutMs) throw new Error(`timed out waiting for ${what}`);
      await new Promise((r) => setTimeout(r, 25));
    }
  };
  const prompt = (message: string) => send({ type: 'prompt', id: `t${++nextId}`, message });
  const results = () =>
    events
      .filter((e) => e.type === 'extension_ui_request' && e.method === 'notify' && String(e.message).startsWith('demo-result:'))
      .map((e) => JSON.parse(String(e.message).slice('demo-result:'.length)));

  beforeAll(async () => {
    cwd = mkdtempSync(join(tmpdir(), 'pipan-channel-store-'));
    pi = spawn(
      'pi',
      [
        '--mode', 'rpc', '--offline', '--no-session', '--no-extensions',
        '-e', join(root, 'src-tauri/resources/pipan-channel-shim.ts'),
        '-e', join(root, 'examples/channel-reference/producer.ts')
      ],
      { cwd }
    );
    createInterface({ input: pi.stdout }).on('line', (line) => {
      let event: ChannelPiEvent;
      try {
        event = JSON.parse(line);
      } catch {
        return;
      }
      events.push(event);
      channels.handleEvent(SID, event);
    });
    channels.processExited();
    channels.configure({ send });
    await until(() => channels.isActive(SID), 'shim announce', 15000);
  }, 20000);

  afterAll(() => {
    pi?.kill();
    channels.processExited();
    channels.configure(null);
    rmSync(cwd, { recursive: true, force: true });
  });

  it('decodes live state from the reference graph', async () => {
    await prompt('/demo-graph');
    await until(() => channels.statesFor(SID).some((s) => s.primitive === 'graph'), 'graph state');
    const state = channels.statesFor(SID).find((s) => s.topic === 'graph')!;
    expect(state.data).toMatchObject({ id: 'demo-run-1', nodes: expect.any(Array) });
    await prompt('/demo-graph clear');
    await until(() => !channels.statesFor(SID).some((s) => s.topic === 'graph'), 'graph cleared');
  });

  it('re-learns state after a late attach through resync', async () => {
    await prompt('/demo-state-big 1000');
    await until(() => channels.statesFor(SID).some((s) => s.topic === 'big'), 'big state');
    channels.processExited(); // simulates a webview reload: the store knows nothing
    expect(channels.isActive(SID)).toBe(false);
    await channels.attach();
    await until(() => channels.statesFor(SID).some((s) => s.topic === 'big'), 'state replayed after resync');
    expect(channels.isActive(SID)).toBe(true);
  });

  it('queues a request, then a session switch cancels it and leaves nothing pending', async () => {
    const before = results().length;
    await prompt('/demo-form');
    await until(() => (channels.pending[SID] ?? []).length === 1, 'pending form request');
    expect(channels.pending[SID][0]).toMatchObject({ kind: 'form', ch: 'acme.demo' });

    await channels.beforeSessionChange();
    expect(channels.pending[SID] ?? []).toHaveLength(0);
    await send({ type: 'new_session', id: `t${++nextId}` });

    await until(() => results().length > before, 'producer told the request was cancelled');
    expect(results().at(-1).answer).toBeNull();
    await until(() => channels.isActive(SID), 'shim announce after new_session');
    expect(channels.pending[SID] ?? []).toHaveLength(0);
  });

  it('answers a request through the store', async () => {
    const before = results().length;
    await prompt('/demo-form');
    await until(() => (channels.pending[SID] ?? []).length === 1, 'pending form request');
    const request = channels.pending[SID][0];
    await channels.answer(SID, request.requestId, { action: 'go', values: { target: 'staging' } });
    await until(() => results().length > before, 'producer answer');
    expect(results().at(-1).answer).toEqual({ action: 'go', values: { target: 'staging' } });
  });

  it('loads log entries written by the producer', async () => {
    await prompt('/demo-log entry');
    await prompt('/demo-log message');
    await until(() => (channels.log[SID] ?? []).length >= 2, 'log items');
    const kinds = channels.log[SID].map((i) => i.kind);
    expect(kinds).toEqual(expect.arrayContaining(['entry', 'message']));
    expect(channels.log[SID][0]).toMatchObject({ ch: 'acme.demo', name: 'note', data: { text: 'synthetic note' } });
  });
});
