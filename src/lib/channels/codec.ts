/**
 * Pure encode and decode of the extension channel wire format
 * (docs/plans/extension-channel-spec.md section 3). No store, no I/O.
 */

import { MAX_PAYLOAD_BYTES } from './limits';
import {
  PROTOCOL_VERSION,
  type Envelope,
  type HostInfo,
  type IntentPayload,
  type RequestPayload,
  type StatePayload
} from './types';

export type Decoded<T> = { ok: true; value: T } | { ok: false; error: string };

const ok = <T>(value: T): Decoded<T> => ({ ok: true, value });
const fail = <T = never>(error: string): Decoded<T> => ({ ok: false, error });

export const SHIM_WIDGET_KEY = 'pipan:shim';
export const STATE_KEY_PREFIX = 'pipan:s:';
export const REQUEST_TITLE_PREFIX = 'pipan:r:';
export const LOG_TYPE_PREFIX = 'pipan:l:';
export const INTENT_COMMAND = 'pipan-intent';

/** Dotted lowercase channel id such as `acme.deploy`. */
const CHANNEL_ID = /^[a-z][a-z0-9_-]*(\.[a-z0-9][a-z0-9_-]*)*$/;

export const isChannelId = (value: unknown): value is string =>
  typeof value === 'string' && value.length <= 128 && CHANNEL_ID.test(value);

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

/** UTF-8 size of a string. Every UTF-16 unit takes at least one byte, so a long string is oversized without a scan. */
export function utf8ByteLength(text: string): number {
  return text.length > MAX_PAYLOAD_BYTES ? text.length : countUtf8(text);
}

function countUtf8(text: string): number {
  let bytes = 0;
  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i);
    if (code < 0x80) bytes += 1;
    else if (code < 0x800) bytes += 2;
    else if (code >= 0xd800 && code <= 0xdbff && i + 1 < text.length) {
      bytes += 4;
      i += 1;
    } else bytes += 3;
  }
  return bytes;
}

function parseJson(text: string): Decoded<unknown> {
  try {
    return ok(JSON.parse(text));
  } catch {
    return fail('payload is not valid JSON');
  }
}

function decodeEnvelope(text: string): Decoded<Envelope<unknown>> {
  if (utf8ByteLength(text) > MAX_PAYLOAD_BYTES) {
    return fail(`payload exceeds ${MAX_PAYLOAD_BYTES} bytes`);
  }
  const parsed = parseJson(text);
  if (!parsed.ok) return parsed;
  const raw = parsed.value;
  if (!isRecord(raw)) return fail('envelope is not an object');
  if (raw.v !== PROTOCOL_VERSION) return fail(`unsupported protocol version ${String(raw.v)}`);
  if (!isChannelId(raw.ch)) return fail('envelope has an invalid channel id');
  const seq = raw.seq;
  if (seq !== undefined && (typeof seq !== 'number' || !Number.isFinite(seq))) {
    return fail('envelope seq is not a number');
  }
  return ok({ v: 1, ch: raw.ch, seq: seq as number | undefined, payload: raw.payload });
}

/** Splits `<ch>:<rest>` where the channel id contains no colon. */
function splitChannel(rest: string): { ch: string; tail: string } | null {
  const index = rest.indexOf(':');
  if (index <= 0) return null;
  const ch = rest.slice(0, index);
  return isChannelId(ch) ? { ch, tail: rest.slice(index + 1) } : null;
}

// ---- Shim announce -------------------------------------------------------------------------

export function decodeShimAnnounce(lines: readonly string[] | undefined): Decoded<HostInfo> {
  if (!lines || lines.length === 0) return fail('announce has no lines');
  const parsed = parseJson(lines.join(''));
  if (!parsed.ok) return parsed;
  const raw = parsed.value;
  if (!isRecord(raw) || raw.host !== 'pipan') return fail('announce is not a PiPan host info');
  if (typeof raw.protocol !== 'number' || raw.protocol < 1) return fail('announce has no protocol');
  const primitives: Record<string, number> = {};
  if (isRecord(raw.primitives)) {
    for (const [name, version] of Object.entries(raw.primitives)) {
      if (typeof version === 'number' && Number.isFinite(version)) primitives[name] = version;
    }
  }
  return ok({ protocol: 1, host: 'pipan', primitives });
}

// ---- State ---------------------------------------------------------------------------------

export interface DecodedState {
  ch: string;
  topic: string;
  seq: number | undefined;
  primitive: string;
  data: unknown;
}

export function parseStateKey(key: string): { ch: string; topic: string } | null {
  if (!key.startsWith(STATE_KEY_PREFIX)) return null;
  const split = splitChannel(key.slice(STATE_KEY_PREFIX.length));
  return split && split.tail.length > 0 ? { ch: split.ch, topic: split.tail } : null;
}

/** Reassembles the widget lines of a state widget and checks them against the key. */
export function decodeState(key: string, lines: readonly string[]): Decoded<DecodedState> {
  const parsedKey = parseStateKey(key);
  if (!parsedKey) return fail(`invalid state key ${key}`);
  const envelope = decodeEnvelope(lines.join(''));
  if (!envelope.ok) return envelope;
  if (envelope.value.ch !== parsedKey.ch) return fail('envelope channel does not match the key');
  const payload = envelope.value.payload;
  if (!isRecord(payload) || typeof payload.primitive !== 'string') {
    return fail('state payload has no primitive');
  }
  return ok({
    ch: parsedKey.ch,
    topic: parsedKey.topic,
    seq: envelope.value.seq,
    primitive: payload.primitive,
    data: (payload as unknown as StatePayload).data
  });
}

/** Tracks the highest `seq` applied per (channel, topic) so stale snapshots are dropped. */
export class SeqTracker {
  private seen = new Map<string, number>();

  /** True when the snapshot is at least as new as the last applied one. Missing seq is accepted. */
  accept(ch: string, topic: string, seq: number | undefined): boolean {
    if (seq === undefined) return true;
    const key = `${ch}\u0000${topic}`;
    const last = this.seen.get(key);
    if (last !== undefined && seq < last) return false;
    this.seen.set(key, seq);
    return true;
  }

  reset(): void {
    this.seen.clear();
  }
}

// ---- Requests ------------------------------------------------------------------------------

export interface DecodedRequest {
  ch: string;
  kind: string;
  requestId: string;
  payload: unknown;
}

export function decodeRequest(title: string, prefill: string | undefined): Decoded<DecodedRequest> {
  if (!title.startsWith(REQUEST_TITLE_PREFIX)) return fail('not a channel request');
  const split = splitChannel(title.slice(REQUEST_TITLE_PREFIX.length));
  if (!split || split.tail.length === 0) return fail(`invalid request title ${title}`);
  const envelope = decodeEnvelope(prefill ?? '');
  if (!envelope.ok) return envelope;
  if (envelope.value.ch !== split.ch) return fail('envelope channel does not match the title');
  const payload = envelope.value.payload;
  if (!isRecord(payload) || typeof payload.requestId !== 'string' || payload.requestId === '') {
    return fail('request payload has no requestId');
  }
  return ok({
    ch: split.ch,
    kind: split.tail,
    requestId: payload.requestId,
    payload: (payload as unknown as RequestPayload).payload
  });
}

/** The `value` of an `extension_ui_response` that answers a request. */
export function encodeRequestAnswer(ch: string, requestId: string, answer: unknown): string {
  const envelope: Envelope<RequestPayload> = {
    v: PROTOCOL_VERSION,
    ch,
    payload: { requestId, payload: answer }
  };
  return JSON.stringify(envelope);
}

// ---- Intents -------------------------------------------------------------------------------

/** The prompt message that delivers an intent to the shim. */
export function encodeIntent(ch: string, name: string, payload: unknown): string {
  const intent: IntentPayload = { v: PROTOCOL_VERSION, ch, name, payload };
  return `/${INTENT_COMMAND} ${JSON.stringify(intent)}`;
}

export const encodeResync = (): string => encodeIntent('_shim', 'resync', {});

// ---- Log -----------------------------------------------------------------------------------

export function parseLogType(customType: string): { ch: string; name: string } | null {
  if (!customType.startsWith(LOG_TYPE_PREFIX)) return null;
  const split = splitChannel(customType.slice(LOG_TYPE_PREFIX.length));
  return split && split.tail.length > 0 ? { ch: split.ch, name: split.tail } : null;
}
