import { describe, expect, it } from 'vitest';
import type { LogItem } from '#lib/stores/channels.svelte.ts';
import type { ChatMessage } from '#lib/types/pi.ts';
import { mergeTranscript } from './transcript';

const msg = (id: string, timestamp: number): ChatMessage => ({ id, role: 'user', timestamp, blocks: [] });
const log = (entryId: string, timestamp: string | undefined, over: Partial<LogItem> = {}): LogItem => ({
  entryId,
  kind: 'message',
  ch: 'acme.demo',
  name: 'note',
  data: {},
  timestamp,
  ...over
});
const iso = (ms: number) => new Date(ms).toISOString();
const keys = (items: ReturnType<typeof mergeTranscript>) => items.map((i) => i.key);
const all = () => true;

describe('mergeTranscript', () => {
  it('places log messages between chat messages by time', () => {
    const items = mergeTranscript([msg('a', 1000), msg('b', 3000)], [log('x', iso(2000))], all);
    expect(keys(items)).toEqual(['a', 'c:x', 'b']);
  });
  it('puts later and untimed items at the end', () => {
    const items = mergeTranscript([msg('a', 1000)], [log('late', iso(9000)), log('none', undefined)], all);
    expect(keys(items)).toEqual(['a', 'c:late', 'c:none']);
  });
  it('keeps log items before the first message when earlier', () => {
    expect(keys(mergeTranscript([msg('a', 5000)], [log('x', iso(1000))], all))).toEqual(['c:x', 'a']);
  });
  it('drops entries, and messages nobody can render', () => {
    const items = mergeTranscript(
      [msg('a', 1)],
      [log('e', iso(2), { kind: 'entry' }), log('m', iso(2), { name: 'other' }), log('ok', iso(2))],
      (_ch, name) => name === 'note'
    );
    expect(keys(items)).toEqual(['a', 'c:ok']);
  });
  it('orders several log items by time even when given out of order', () => {
    const items = mergeTranscript([], [log('b', iso(2000)), log('a', iso(1000))], all);
    expect(keys(items)).toEqual(['c:a', 'c:b']);
  });
});
