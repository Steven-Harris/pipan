import { describe, expect, it } from 'vitest';
import { MAX_CUSTOM_MESSAGE_CHARS, customMessageView } from './custom-messages';

const raw = (extra: Record<string, unknown>) => ({ role: 'custom', customType: 'acme.note', content: 'hello', display: true, ...extra });

describe('customMessageView', () => {
  it('returns the label and text of a displayed custom message', () => {
    expect(customMessageView(raw({}))).toEqual({ customType: 'acme.note', text: 'hello' });
  });

  it('joins text blocks and ignores other content blocks', () => {
    const view = customMessageView(
      raw({ content: [{ type: 'text', text: 'one' }, { type: 'image', data: 'x' }, { type: 'text', text: 'two' }] })
    );
    expect(view?.text).toBe('one\ntwo');
  });

  it('hides messages that are not marked for display', () => {
    expect(customMessageView(raw({ display: false }))).toBeNull();
    expect(customMessageView(raw({ display: undefined }))).toBeNull();
    expect(customMessageView(raw({ display: 'yes' }))).toBeNull();
  });

  it('ignores other roles, a missing label and empty text', () => {
    expect(customMessageView({ ...raw({}), role: 'assistant' })).toBeNull();
    expect(customMessageView(raw({ customType: '' }))).toBeNull();
    expect(customMessageView(raw({ customType: undefined }))).toBeNull();
    expect(customMessageView(raw({ content: '   ' }))).toBeNull();
    expect(customMessageView(raw({ content: undefined }))).toBeNull();
  });

  it('leaves channel log messages to the plugins that render them', () => {
    expect(customMessageView(raw({ customType: 'pipan:l:acme.demo:note' }))).toBeNull();
  });

  it('truncates very long text', () => {
    const view = customMessageView(raw({ content: 'x'.repeat(MAX_CUSTOM_MESSAGE_CHARS + 50) }));
    expect(view?.text.endsWith('(truncated)')).toBe(true);
    expect(view?.text.length).toBeLessThan(MAX_CUSTOM_MESSAGE_CHARS + 30);
  });
});
