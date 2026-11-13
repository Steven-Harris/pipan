// Drives the channels store with the event stream of a real `pi --mode rpc --offline` process
// running the shim and the reference producer. Skipped when `pi` is not on PATH.
import { spawn, spawnSync, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { addComment, buildReviewAnswer, createReviewState, dismissComment } from '../src/lib/channels/review-model.ts';
import type { ReviewRequest } from '../src/lib/channels/types.ts';
import { buildAnswer, initialValues, validateValues } from '../src/lib/channels/form-state.ts';
import type { FormRequest } from '../src/lib/channels/types.ts';
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

  /** Runs a demo command and waits for its pending request and then its reported result. */
  const askAndWait = async (command: string) => {
    const before = results().length;
    await prompt(command);
    await until(() => (channels.pending[SID] ?? []).length === 1, `pending request for ${command}`);
    return { request: channels.pending[SID][0], finished: async () => {
      await until(() => results().length > before, `result of ${command}`);
      return results().at(-1);
    } };
  };

  it('form: pick one, with validation before the action', async () => {
    const { request, finished } = await askAndWait('/demo-form pick');
    const form = request.payload as FormRequest;
    const values = initialValues(form);
    expect(validateValues(form, values)).toHaveProperty('target');
    values.target = 'production';
    expect(validateValues(form, values)).toEqual({});
    await channels.answer(SID, request.requestId, buildAnswer(form, 'go', values));
    expect((await finished()).answer).toEqual({ action: 'go', values: { target: 'production' } });
  });

  it('form: an action that skips validation answers with empty values', async () => {
    const { request, finished } = await askAndWait('/demo-form pick');
    const form = request.payload as FormRequest;
    await channels.answer(SID, request.requestId, buildAnswer(form, 'skip', initialValues(form)));
    expect((await finished()).answer).toEqual({ action: 'skip', values: { target: '' } });
  });

  it('form: checklist honours defaults, min and max', async () => {
    const { request, finished } = await askAndWait('/demo-form checklist');
    const form = request.payload as FormRequest;
    const values = initialValues(form);
    expect(values.steps).toEqual(['lint']);
    expect(validateValues(form, { steps: [] })).toHaveProperty('steps');
    expect(validateValues(form, { steps: ['lint', 'test', 'ship'] })).toHaveProperty('steps');
    values.steps = ['ship', 'test'];
    await channels.answer(SID, request.requestId, buildAnswer(form, 'run', values));
    expect((await finished()).answer).toEqual({ action: 'run', values: { steps: ['test', 'ship'] } });
  });

  it('form: multiline edit starts from the prefill and returns the edited text', async () => {
    const { request, finished } = await askAndWait('/demo-form edit');
    const form = request.payload as FormRequest;
    const values = initialValues(form);
    expect(values.message).toBe('first line\nsecond line');
    values.message = `${values.message}\nthird line`;
    await channels.answer(SID, request.requestId, buildAnswer(form, 'save', values));
    expect((await finished()).answer).toEqual({
      action: 'save',
      values: { message: 'first line\nsecond line\nthird line' }
    });
  });

  it('form: body and actions only', async () => {
    const { request, finished } = await askAndWait('/demo-form body');
    const form = request.payload as FormRequest;
    expect(form.fields).toBeUndefined();
    await channels.answer(SID, request.requestId, buildAnswer(form, 'no', initialValues(form)));
    expect((await finished()).answer).toEqual({ action: 'no', values: {} });
  });

  it('dismissing a request cancels it', async () => {
    const { request, finished } = await askAndWait('/demo-form pick');
    await channels.cancel(SID, request.requestId);
    expect((await finished()).answer).toBeNull();
    expect(channels.pending[SID] ?? []).toHaveLength(0);
  });

  it('review: comments round-trip with ids and touched, and earlier threads arrive', async () => {
    const { request, finished } = await askAndWait('/demo-review-diff');
    const review = request.payload as ReviewRequest;
    expect(review.subject.type).toBe('diff');
    expect(review.comments.map((c) => c.id)).toEqual(['c1']);
    let model = createReviewState(review.comments);
    model = addComment(model, { path: 'src/module1.ts', side: 'new', line: 4, endLine: 6, text: 'Extract this.' }).state;
    await channels.answer(SID, request.requestId, buildReviewAnswer(model, 'changes', 'see comments'));
    const answer = (await finished()).answer;
    expect(answer.choice).toBe('changes');
    expect(answer.touched).toEqual(['tmp-1']);
    expect(answer.note).toBe('see comments');
    expect(answer.comments.map((c: { id: string }) => c.id)).toEqual(['c1', 'tmp-1']);
    expect(answer.comments[1]).toMatchObject({ path: 'src/module1.ts', side: 'new', line: 4, endLine: 6, status: 'open' });
  });

  it('review: dismissing an earlier comment is reported as touched', async () => {
    const { request, finished } = await askAndWait('/demo-review-diff');
    const model = dismissComment(createReviewState((request.payload as ReviewRequest).comments), 'c1');
    await channels.answer(SID, request.requestId, buildReviewAnswer(model, 'approve'));
    const answer = (await finished()).answer;
    expect(answer.touched).toEqual(['c1']);
    expect(answer.comments[0].status).toBe('dismissed');
  });

  it('review: the 50,000-line diff arrives and can be answered', async () => {
    const { request, finished } = await askAndWait('/demo-review-diff large');
    const review = request.payload as ReviewRequest;
    expect(review.subject.type === 'diff' && review.subject.files).toHaveLength(50);
    await channels.answer(SID, request.requestId, buildReviewAnswer(createReviewState(review.comments), 'approve'));
    expect((await finished()).answer.choice).toBe('approve');
  }, 60_000);

  it('a request that is not shown by this version is cancelled so the producer continues', async () => {
    const { request, finished } = await askAndWait('/demo-review-doc');
    expect(request.kind).toBe('review');
    await channels.cancel(SID, request.requestId);
    expect((await finished()).answer).toBeNull();
  });

  it('a shim timeout closes the pending request', async () => {
    const before = results().length;
    await prompt('/demo-timeout 400');
    await until(() => (channels.pending[SID] ?? []).length === 1, 'pending request');
    await until(() => (channels.pending[SID] ?? []).length === 0, 'timeout closes the request');
    await until(() => results().length > before, 'producer told of the timeout');
    expect(results().at(-1).answer).toBeNull();
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
