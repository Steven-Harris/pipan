import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

import type { PiRawMessage, PiRpcEvent } from '#lib/types/pi.ts';
import { chatStore } from './chat.svelte.ts';

type Internals = { handlePiEvent(e: PiRpcEvent): void; hydrateFromRawMessages(m: PiRawMessage[]): void };
const internals = () => chatStore as unknown as Internals;
const feed = (event: PiRpcEvent) => internals().handlePiEvent(event);

const custom = (extra: Record<string, unknown> = {}): PiRawMessage => ({
  role: 'custom',
  customType: 'acme.note',
  content: 'Visible **text**',
  display: true,
  timestamp: 1000,
  ...extra
});

const textOf = (i: number) => {
  const block = chatStore.messages[i].blocks[0];
  return block.type === 'text' ? block.text : '';
};

describe('custom messages in the chat', () => {
  beforeEach(() => {
    chatStore.messages = [];
  });

  it('draws a displayed custom message once, when it ends', () => {
    feed({ type: 'message_start', message: custom() });
    expect(chatStore.messages).toHaveLength(0);
    feed({ type: 'message_end', message: custom() });
    expect(chatStore.messages).toHaveLength(1);
    expect(chatStore.messages[0]).toMatchObject({ role: 'custom', customType: 'acme.note', timestamp: 1000 });
    expect(textOf(0)).toBe('Visible **text**');
  });

  it('does not draw a message that is for the model only', () => {
    feed({ type: 'message_end', message: custom({ display: false }) });
    expect(chatStore.messages).toHaveLength(0);
  });

  it('does not draw channel log messages', () => {
    feed({ type: 'message_end', message: custom({ customType: 'pipan:l:acme.demo:note' }) });
    expect(chatStore.messages).toHaveLength(0);
  });

  it('draws them again, in order, when the history is rebuilt', () => {
    internals().hydrateFromRawMessages([
      { role: 'user', content: 'hi', timestamp: 1 },
      custom({ timestamp: 2 }),
      custom({ display: false, content: 'hidden', timestamp: 3 }),
      { role: 'assistant', content: [{ type: 'text', text: 'reply' }], timestamp: 4 }
    ]);
    expect(chatStore.messages.map((m) => m.role)).toEqual(['user', 'custom', 'assistant']);
    expect(chatStore.messages[1].customType).toBe('acme.note');
  });

  it('keeps streaming into the same assistant message after an extension message arrives mid-turn', () => {
    feed({ type: 'message_start', message: { role: 'assistant', content: [], timestamp: 1 } });
    feed({ type: 'message_end', message: custom({ timestamp: 2 }) });
    feed({ type: 'message_end', message: { role: 'assistant', content: [], stopReason: 'stop', timestamp: 1 } });
    const assistant = chatStore.messages.find((m) => m.role === 'assistant');
    expect(assistant?.isStreaming).toBe(false);
    expect(chatStore.messages.filter((m) => m.role === 'assistant')).toHaveLength(1);
  });
});
