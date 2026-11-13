import { describe, expect, it } from 'vitest';
import {
  SeqTracker,
  decodeExpiry,
  decodeRequest,
  decodeShimAnnounce,
  decodeState,
  encodeIntent,
  encodeRequestAnswer,
  encodeResync,
  isChannelId,
  parseLogType,
  parseStateKey,
  utf8ByteLength
} from './codec';
import { MAX_PAYLOAD_BYTES } from './limits';

const chunk = (text: string, size: number) => {
  const out: string[] = [];
  for (let i = 0; i < text.length; i += size) out.push(text.slice(i, i + size));
  return out;
};

const stateLines = (ch: string, seq: number, data: unknown, primitive = 'graph', size = 64 * 1024) =>
  chunk(JSON.stringify({ v: 1, ch, seq, payload: { primitive, data } }), size);

describe('isChannelId', () => {
  it('accepts dotted lowercase ids', () => {
    expect(isChannelId('acme.deploy')).toBe(true);
    expect(isChannelId('acme')).toBe(true);
    expect(isChannelId('a1.b-2_c')).toBe(true);
  });
  it('rejects ids with colons, capitals, spaces or nothing', () => {
    for (const id of ['', 'Acme', 'a:b', 'a b', '.a', 'a..b', 5, undefined]) {
      expect(isChannelId(id)).toBe(false);
    }
  });
});

describe('parseStateKey and parseLogType', () => {
  it('splits channel and topic, keeping colons in the topic', () => {
    expect(parseStateKey('pipan:s:acme.demo:graph')).toEqual({ ch: 'acme.demo', topic: 'graph' });
    expect(parseStateKey('pipan:s:acme.demo:a:b')).toEqual({ ch: 'acme.demo', topic: 'a:b' });
  });
  it('rejects malformed keys', () => {
    expect(parseStateKey('pipan:s:acme.demo')).toBeNull();
    expect(parseStateKey('pipan:s:acme.demo:')).toBeNull();
    expect(parseStateKey('pipan:s::graph')).toBeNull();
    expect(parseStateKey('other:s:acme:graph')).toBeNull();
  });
  it('parses log types', () => {
    expect(parseLogType('pipan:l:acme.demo:note')).toEqual({ ch: 'acme.demo', name: 'note' });
    expect(parseLogType('spike-entry')).toBeNull();
    expect(parseLogType('pipan:l:acme.demo')).toBeNull();
  });
});

describe('decodeShimAnnounce', () => {
  const info = { protocol: 1, host: 'pipan', primitives: { graph: 1, form: 1 } };
  it('decodes host info', () => {
    expect(decodeShimAnnounce([JSON.stringify(info)])).toEqual({
      ok: true,
      value: { protocol: 1, host: 'pipan', primitives: { graph: 1, form: 1 } }
    });
  });
  it('drops non-numeric primitive versions', () => {
    const result = decodeShimAnnounce([JSON.stringify({ ...info, primitives: { graph: 'x', form: 2 } })]);
    expect(result.ok && result.value.primitives).toEqual({ form: 2 });
  });
  it('rejects other hosts, bad JSON and empty lines', () => {
    expect(decodeShimAnnounce([JSON.stringify({ ...info, host: 'other' })]).ok).toBe(false);
    expect(decodeShimAnnounce(['{nope']).ok).toBe(false);
    expect(decodeShimAnnounce([]).ok).toBe(false);
    expect(decodeShimAnnounce(undefined).ok).toBe(false);
  });
});

describe('decodeState', () => {
  const key = 'pipan:s:acme.demo:graph';

  it('decodes a single-piece envelope', () => {
    const result = decodeState(key, stateLines('acme.demo', 3, { id: 'g' }));
    expect(result).toEqual({
      ok: true,
      value: { ch: 'acme.demo', topic: 'graph', seq: 3, primitive: 'graph', data: { id: 'g' } }
    });
  });

  it('reassembles chunked widget lines', () => {
    const data = { blob: 'x'.repeat(200_000) };
    const lines = stateLines('acme.demo', 1, data);
    expect(lines.length).toBeGreaterThan(3);
    const result = decodeState(key, lines);
    expect(result.ok && (result.value.data as typeof data).blob.length).toBe(200_000);
  });

  it('rejects a channel that disagrees with the key', () => {
    expect(decodeState(key, stateLines('acme.other', 1, {})).ok).toBe(false);
  });

  it('rejects unsupported protocol versions, missing primitive, bad JSON and bad seq', () => {
    const env = (value: object) => [JSON.stringify(value)];
    expect(decodeState(key, env({ v: 2, ch: 'acme.demo', payload: { primitive: 'graph' } })).ok).toBe(false);
    expect(decodeState(key, env({ v: 1, ch: 'acme.demo', payload: {} })).ok).toBe(false);
    expect(decodeState(key, ['{broken']).ok).toBe(false);
    expect(decodeState(key, env({ v: 1, ch: 'acme.demo', seq: 'x', payload: { primitive: 'graph' } })).ok).toBe(false);
    expect(decodeState('pipan:s:bad', ['{}']).ok).toBe(false);
  });

  it('enforces the payload size limit', () => {
    const lines = stateLines('acme.demo', 1, { blob: 'x'.repeat(MAX_PAYLOAD_BYTES + 10) });
    const result = decodeState(key, lines);
    expect(result.ok).toBe(false);
  });
});

describe('utf8ByteLength', () => {
  it('counts multi-byte characters', () => {
    expect(utf8ByteLength('abc')).toBe(3);
    expect(utf8ByteLength('\u00e9')).toBe(2);
    expect(utf8ByteLength('\u20ac')).toBe(3);
    expect(utf8ByteLength('\u{1F600}')).toBe(4);
  });
});

describe('SeqTracker', () => {
  it('drops lower seq and accepts equal or higher', () => {
    const tracker = new SeqTracker();
    expect(tracker.accept('a.b', 't', 2)).toBe(true);
    expect(tracker.accept('a.b', 't', 1)).toBe(false);
    expect(tracker.accept('a.b', 't', 2)).toBe(true);
    expect(tracker.accept('a.b', 't', 5)).toBe(true);
  });
  it('tracks topics and channels independently and accepts a missing seq', () => {
    const tracker = new SeqTracker();
    tracker.accept('a.b', 't', 9);
    expect(tracker.accept('a.b', 'u', 1)).toBe(true);
    expect(tracker.accept('a.c', 't', 1)).toBe(true);
    expect(tracker.accept('a.b', 't', undefined)).toBe(true);
  });
  it('forgets everything on reset', () => {
    const tracker = new SeqTracker();
    tracker.accept('a.b', 't', 9);
    tracker.reset();
    expect(tracker.accept('a.b', 't', 1)).toBe(true);
  });
});

describe('requests', () => {
  const prefill = (ch: string, payload: unknown) => JSON.stringify({ v: 1, ch, payload });

  it('decodes a request', () => {
    const result = decodeRequest(
      'pipan:r:acme.demo:form',
      prefill('acme.demo', { requestId: 'r1', payload: { title: 'T' } })
    );
    expect(result).toEqual({
      ok: true,
      value: { ch: 'acme.demo', kind: 'form', requestId: 'r1', payload: { title: 'T' } }
    });
  });

  it('rejects bad titles, mismatched channels, missing ids and missing prefill', () => {
    expect(decodeRequest('nope', '{}').ok).toBe(false);
    expect(decodeRequest('pipan:r:acme.demo', '{}').ok).toBe(false);
    expect(decodeRequest('pipan:r:acme.demo:form', prefill('acme.other', { requestId: 'r' })).ok).toBe(false);
    expect(decodeRequest('pipan:r:acme.demo:form', prefill('acme.demo', { payload: {} })).ok).toBe(false);
    expect(decodeRequest('pipan:r:acme.demo:form', undefined).ok).toBe(false);
  });

  it('encodes an answer the shim accepts', () => {
    const value = encodeRequestAnswer('acme.demo', 'r1', { action: 'go' });
    expect(JSON.parse(value)).toEqual({ v: 1, ch: 'acme.demo', payload: { requestId: 'r1', payload: { action: 'go' } } });
  });

  it('decodes expiry signals', () => {
    const lines = [JSON.stringify({ v: 1, ch: 'acme.demo', payload: { requestId: 'r1', reason: 'timeout' } })];
    expect(decodeExpiry('pipan:x:r1', lines)).toEqual({ ok: true, value: { requestId: 'r1', reason: 'timeout' } });
    expect(decodeExpiry('pipan:x:r1', undefined).ok).toBe(false);
    expect(decodeExpiry('other', lines).ok).toBe(false);
  });
});

describe('intents', () => {
  it('encodes the hidden command', () => {
    const message = encodeIntent('acme.demo', 'graph.action', { actionId: 'a' });
    expect(message.startsWith('/pipan-intent ')).toBe(true);
    expect(JSON.parse(message.slice('/pipan-intent '.length))).toEqual({
      v: 1,
      ch: 'acme.demo',
      name: 'graph.action',
      payload: { actionId: 'a' }
    });
  });
  it('encodes resync for the _shim channel', () => {
    expect(JSON.parse(encodeResync().slice('/pipan-intent '.length))).toEqual({
      v: 1,
      ch: '_shim',
      name: 'resync',
      payload: {}
    });
  });
});
