// Keeps docs/public/channels.schema.json honest: it must match the types, accept everything the
// reference producer emits, and reject malformed payloads.
import { readFileSync } from 'node:fs';
import Ajv from 'ajv';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import producer from '../examples/channel-reference/producer.ts';
import { validateForm, validateGraph, validateReview } from '../src/lib/channels/validate.ts';
import { SCHEMA_PATH, generateSchema } from './channels-schema.mjs';

const schemaText = readFileSync(SCHEMA_PATH, 'utf8');
const schema = JSON.parse(schemaText);

const ajv = new Ajv({ strict: false, allErrors: true });
ajv.addSchema(schema, 'channels');
const check = (type: string) => {
  const validate = ajv.getSchema(`channels#/definitions/${type}`);
  if (!validate) throw new Error(`no definition ${type}`);
  return (value: unknown) => ({ ok: validate(value) as boolean, errors: ajv.errorsText(validate.errors) });
};

describe('channels.schema.json', () => {
  it('matches the types it was generated from', () => {
    expect(generateSchema()).toBe(schemaText);
  });

  it('defines every payload type', () => {
    for (const name of ['HostInfo', 'GraphState', 'FormRequest', 'FormAnswer', 'ReviewRequest', 'ReviewAnswer']) {
      expect(schema.definitions[name], name).toBeDefined();
    }
  });
});

// ---- What the reference producer emits -----------------------------------------------------

type Handler = (args: string, ctx: unknown) => unknown;
interface Captured {
  states: { topic: string; primitive: string; payload: unknown }[];
  requests: { kind: string; payload: unknown; reply: (answer: unknown) => void }[];
}

/** Loads the producer into a fake Pi and returns a way to run its commands. */
function loadProducer() {
  const commands = new Map<string, Handler>();
  const listeners = new Map<string, ((data: any) => unknown)[]>();
  const handlers = new Map<string, ((event: unknown, ctx: unknown) => unknown)[]>();
  const captured: Captured = { states: [], requests: [] };
  const ctx = { ui: { notify() {}, select: async () => undefined, confirm: async () => false, input: async () => undefined, editor: async () => undefined } };

  const events = {
    on(channel: string, handler: (data: any) => unknown) {
      listeners.set(channel, [...(listeners.get(channel) ?? []), handler]);
    },
    emit(channel: string, data?: any) {
      if (channel === 'pipan:hello') data.reply({ protocol: 1, host: 'pipan', primitives: { graph: 1, review: 1, form: 1 } });
      else if (channel === 'pipan:state') captured.states.push({ topic: data.topic, primitive: data.primitive, payload: data.payload });
      else if (channel === 'pipan:request') captured.requests.push({ kind: data.kind, payload: data.payload, reply: data.reply });
      for (const handler of listeners.get(channel) ?? []) handler(data);
    }
  };
  const pi = {
    on: (event: string, handler: (event: unknown, ctx: unknown) => unknown) => handlers.set(event, [...(handlers.get(event) ?? []), handler]),
    events,
    sendMessage() {},
    registerCommand: (name: string, options: { handler: Handler }) => commands.set(name, options.handler)
  };
  producer(pi as never);
  for (const handler of handlers.get('session_start') ?? []) handler({}, ctx);

  return {
    captured,
    async run(command: string, args = '') {
      const before = captured.requests.length;
      const pending = commands.get(command)?.(args, ctx);
      // A request waits for an answer. Cancel it so the command finishes.
      for (const request of captured.requests.slice(before)) request.reply(undefined);
      await pending;
    },
    shutdown() {
      for (const handler of handlers.get('session_shutdown') ?? []) handler({}, ctx);
    }
  };
}

describe('the reference producer against the schema', () => {
  let loaded: ReturnType<typeof loadProducer>;
  beforeEach(() => {
    vi.useFakeTimers();
    loaded = loadProducer();
  });
  afterEach(() => {
    loaded.shutdown();
    vi.useRealTimers();
  });

  it('publishes graphs the schema and the host accept', async () => {
    await loaded.run('demo-graph');
    vi.advanceTimersByTime(5000);
    const graphs = loaded.captured.states.filter((s) => s.primitive === 'graph');
    expect(graphs.length).toBeGreaterThanOrEqual(4);
    const graph = check('GraphState');
    for (const state of graphs) {
      expect(graph(state.payload), JSON.stringify(state.payload)).toMatchObject({ ok: true });
      expect(validateGraph(state.payload).ok).toBe(true);
    }
  });

  it('asks form questions the schema and the host accept', async () => {
    const form = check('FormRequest');
    for (const shape of ['pick', 'checklist', 'edit', 'answer', 'body']) {
      await loaded.run('demo-form', shape);
    }
    const forms = loaded.captured.requests.filter((r) => r.kind === 'form');
    expect(forms).toHaveLength(5);
    for (const request of forms) {
      expect(form(request.payload), JSON.stringify(request.payload)).toMatchObject({ ok: true });
      expect(validateForm(request.payload).ok).toBe(true);
    }
  });

  it('asks the approval question as a form', async () => {
    await loaded.run('demo-graph');
    vi.advanceTimersByTime(4000);
    const approval = loaded.captured.requests.find((r) => (r.payload as { title?: string }).title === 'Approve the release?');
    expect(approval).toBeDefined();
    expect(check('FormRequest')(approval?.payload)).toMatchObject({ ok: true });
  });

  it('asks review questions the schema and the host accept', async () => {
    const review = check('ReviewRequest');
    await loaded.run('demo-review-diff');
    await loaded.run('demo-review-doc');
    await loaded.run('demo-review-doc', 'multi');
    const reviews = loaded.captured.requests.filter((r) => r.kind === 'review');
    expect(reviews).toHaveLength(3);
    for (const request of reviews) {
      expect(review(request.payload), JSON.stringify(request.payload).slice(0, 300)).toMatchObject({ ok: true });
      expect(validateReview(request.payload).ok).toBe(true);
    }
  });
});

describe('answers and handshake', () => {
  it('accepts well-formed answers and host info', () => {
    expect(check('FormAnswer')({ action: 'go', values: { a: 'x', b: ['y', 'z'] } })).toMatchObject({ ok: true });
    expect(
      check('ReviewAnswer')({
        choice: 'approve',
        touched: ['c1'],
        comments: [{ id: 'c1', path: 'a.ts', line: 3, side: 'new', text: 'note', author: 'user', status: 'open' }]
      })
    ).toMatchObject({ ok: true });
    expect(check('HostInfo')({ protocol: 1, host: 'pipan', primitives: { graph: 1 } })).toMatchObject({ ok: true });
  });
});

describe('malformed payloads', () => {
  const node = { id: 'a', label: 'A', status: 'active' };

  it('rejects graphs with missing or mistyped fields', () => {
    const graph = check('GraphState');
    expect(graph({ id: 'g', nodes: [{ label: 'A', status: 'active' }], edges: [] }).ok).toBe(false);
    expect(graph({ id: 'g', nodes: [{ ...node, status: 'exploded' }], edges: [] }).ok).toBe(false);
    expect(graph({ nodes: [node], edges: [] }).ok).toBe(false);
    expect(graph({ id: 'g', nodes: node, edges: [] }).ok).toBe(false);
    expect(graph({ id: 'g', nodes: [node], edges: [] }).ok).toBe(true);
  });

  it('rejects forms without actions or with an unknown field type', () => {
    const form = check('FormRequest');
    expect(form({ title: 'T' }).ok).toBe(false);
    expect(form({ title: 'T', actions: [{ label: 'Go' }] }).ok).toBe(false);
    expect(form({ title: 'T', fields: [{ id: 'x', type: 'slider', label: 'X' }], actions: [{ id: 'go', label: 'Go' }] }).ok).toBe(false);
    expect(form({ title: 'T', actions: [{ id: 'go', label: 'Go' }] }).ok).toBe(true);
  });

  it('rejects reviews without choices or a subject', () => {
    const review = check('ReviewRequest');
    expect(review({ title: 'R', subject: { type: 'document', documents: [] }, comments: [] }).ok).toBe(false);
    expect(review({ title: 'R', comments: [], choices: [{ id: 'a', label: 'A' }] }).ok).toBe(false);
  });

  it('rejects answers without the required keys', () => {
    expect(check('FormAnswer')({ values: {} }).ok).toBe(false);
    expect(check('FormAnswer')({ action: 'go', values: { a: 1 } }).ok).toBe(false);
    expect(check('ReviewAnswer')({ choice: 'approve' }).ok).toBe(false);
  });
});
