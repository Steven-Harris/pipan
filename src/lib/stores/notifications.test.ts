import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { MAX_HISTORY, MAX_MESSAGE_CHARS, MAX_TOASTS, TOAST_MS, notifications } from './notifications.svelte';

const notify = (message: unknown, notifyType?: unknown) => ({
  type: 'extension_ui_request',
  id: 'n',
  method: 'notify',
  message,
  notifyType
});

describe('notifications', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    notifications.forgetSession('s1');
    notifications.forgetSession('s2');
    for (const t of [...notifications.toasts]) notifications.dismiss(t.id);
  });
  afterEach(() => vi.useRealTimers());

  it('handles notify events and leaves other events alone', () => {
    expect(notifications.handleEvent('s1', notify('hi'))).toBe(true);
    expect(notifications.handleEvent('s1', { type: 'extension_ui_request', method: 'setWidget' })).toBe(false);
    expect(notifications.handleEvent('s1', { type: 'message_end' })).toBe(false);
    expect(notifications.handleEvent('s1', null)).toBe(false);
    expect(notifications.toasts.map((t) => t.message)).toEqual(['hi']);
  });

  it('maps the level and defaults to info', () => {
    notifications.handleEvent('s1', notify('a'));
    notifications.handleEvent('s1', notify('b', 'warning'));
    notifications.handleEvent('s1', notify('c', 'error'));
    notifications.handleEvent('s1', notify('d', 'bogus'));
    expect(notifications.toasts.map((t) => t.level)).toEqual(['info', 'warning', 'error', 'info']);
  });

  it('ignores an empty message but still counts it as handled', () => {
    expect(notifications.handleEvent('s1', notify('   '))).toBe(true);
    expect(notifications.handleEvent('s1', notify(undefined))).toBe(true);
    expect(notifications.toasts).toHaveLength(0);
  });

  it('removes info and warning toasts after a while, and errors only when dismissed', () => {
    notifications.handleEvent('s1', notify('i'));
    notifications.handleEvent('s1', notify('w', 'warning'));
    notifications.handleEvent('s1', notify('e', 'error'));
    vi.advanceTimersByTime(TOAST_MS + 10);
    expect(notifications.toasts.map((t) => t.message)).toEqual(['e']);
    notifications.dismiss(notifications.toasts[0].id);
    expect(notifications.toasts).toHaveLength(0);
  });

  it('shows at most the capped number of toasts, dropping the oldest', () => {
    for (let i = 0; i < MAX_TOASTS + 2; i++) notifications.handleEvent('s1', notify(`m${i}`, 'error'));
    expect(notifications.toasts).toHaveLength(MAX_TOASTS);
    expect(notifications.toasts[0].message).toBe('m2');
  });

  it('keeps a capped history per session, including dismissed toasts', () => {
    notifications.handleEvent('s1', notify('one'));
    notifications.handleEvent('s2', notify('other'));
    vi.advanceTimersByTime(TOAST_MS + 10);
    expect(notifications.toasts).toHaveLength(0);
    expect(notifications.historyFor('s1').map((n) => n.message)).toEqual(['one']);
    for (let i = 0; i < MAX_HISTORY + 5; i++) notifications.handleEvent('s1', notify(`n${i}`));
    expect(notifications.historyFor('s1')).toHaveLength(MAX_HISTORY);
    expect(notifications.historyFor('s2')).toHaveLength(1);
  });

  it('truncates a very long message', () => {
    notifications.handleEvent('s1', notify('x'.repeat(MAX_MESSAGE_CHARS + 100), 'error'));
    expect(notifications.toasts[0].message).toHaveLength(MAX_MESSAGE_CHARS + 3);
  });

  it('forgets a deleted session', () => {
    notifications.handleEvent('s1', notify('gone', 'error'));
    notifications.forgetSession('s1');
    expect(notifications.toasts).toHaveLength(0);
    expect(notifications.historyFor('s1')).toEqual([]);
  });
});
