import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

import { invoke } from '@tauri-apps/api/core';
import type { PiRpcEvent } from '#lib/types/pi.ts';
import { chatStore } from './chat.svelte.ts';

// The store's event handler is private; tests feed it the events Pi would send.
const feed = (event: PiRpcEvent) => (chatStore as unknown as { handlePiEvent(e: PiRpcEvent): void }).handlePiEvent(event);

async function send(text: string): Promise<string> {
  vi.mocked(invoke).mockClear();
  await chatStore.sendPrompt(text);
  const call = vi.mocked(invoke).mock.calls.find(([name]) => name === 'send_pi_command');
  return (call?.[1] as { command: { id: string } }).command.id;
}

describe('optimistic user messages', () => {
  beforeEach(() => {
    chatStore.messages = [];
    chatStore.isAgentRunning = false;
    chatStore.sessionStatus = { ...chatStore.sessionStatus, state: 'running' };
  });

  it('stays optimistic until Pi starts the user message', async () => {
    const id = await send('hello');
    feed({ type: 'response', id, command: 'prompt', success: true, data: { disposition: 'started' } });
    expect(chatStore.messages[0].isOptimistic).toBe(true);
    feed({ type: 'message_start', message: { role: 'user', content: 'hello' } });
    expect(chatStore.messages[0].isOptimistic).toBe(false);
  });

  it('a prompt Pi handled itself is no longer sending', async () => {
    const id = await send('/demo-graph');
    feed({ type: 'response', id, command: 'prompt', success: true, data: { disposition: 'handled' } });
    expect(chatStore.messages).toHaveLength(1);
    expect(chatStore.messages[0].isOptimistic).toBe(false);
  });

  it('a handled prompt does not take over the next real user message', async () => {
    const first = await send('/demo-graph');
    feed({ type: 'response', id: first, command: 'prompt', success: true, data: { disposition: 'handled' } });
    const second = await send('what is 2+2');
    feed({ type: 'response', id: second, command: 'prompt', success: true, data: { disposition: 'started' } });
    feed({ type: 'message_start', message: { role: 'user', content: 'what is 2+2' } });
    expect(chatStore.messages.map((m) => m.blocks[0].type === 'text' && m.blocks[0].text)).toEqual(['/demo-graph', 'what is 2+2']);
    expect(chatStore.messages.every((m) => !m.isOptimistic)).toBe(true);
  });

  it('removes the message when Pi rejects the prompt', async () => {
    const id = await send('hello');
    feed({ type: 'response', id, command: 'prompt', success: false, error: 'No model' });
    expect(chatStore.messages).toHaveLength(0);
    expect(chatStore.lastError).toBe('No model');
  });

  it('ignores responses to other commands and unknown ids', async () => {
    await send('hello');
    feed({ type: 'response', id: 'other', command: 'prompt', success: true, data: { disposition: 'handled' } });
    feed({ type: 'response', id: 'x', command: 'get_state', success: true, data: {} });
    expect(chatStore.messages[0].isOptimistic).toBe(true);
  });
});
