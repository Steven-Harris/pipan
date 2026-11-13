import { describe, expect, it } from 'vitest';
import {
  ACTION_OPTION_LIMIT,
  MAX_DIALOG_OPTIONS,
  dialogResponse,
  dialogToForm,
  parseDialog,
  plainToMarkdown
} from './dialog-bridge';
import { validateForm } from './validate';

const req = (extra: Record<string, unknown>) => ({ type: 'extension_ui_request', id: 'u1', method: 'select', title: 'T', ...extra });
const parse = (extra: Record<string, unknown>) => {
  const dialog = parseDialog(req(extra));
  if (!dialog) throw new Error('expected a dialog');
  return dialog;
};

describe('parseDialog', () => {
  it('reads the four dialog methods', () => {
    for (const method of ['select', 'confirm', 'input', 'editor']) expect(parseDialog(req({ method }))?.method).toBe(method);
  });

  it('ignores other methods, other types, a missing id and plan-gate approvals', () => {
    expect(parseDialog(req({ method: 'notify' }))).toBeNull();
    expect(parseDialog(req({ method: 'setWidget' }))).toBeNull();
    expect(parseDialog({ ...req({}), type: 'response' })).toBeNull();
    expect(parseDialog(req({ id: undefined }))).toBeNull();
    expect(parseDialog(req({ id: '' }))).toBeNull();
    expect(parseDialog(req({ title: 'PIPAN_APPROVAL::{}' }))).toBeNull();
    expect(parseDialog(null)).toBeNull();
  });

  it('keeps only string options and cuts a long list', () => {
    const many = Array.from({ length: MAX_DIALOG_OPTIONS + 3 }, (_u, i) => `o${i}`);
    const dialog = parse({ options: ['a', 1, null, 'b', ...many] });
    expect(dialog.options).toHaveLength(MAX_DIALOG_OPTIONS);
    expect(dialog.omitted).toBe(5);
  });

  it('accepts only a positive finite timeout and clamps it', () => {
    expect(parse({ timeout: 5000 }).timeoutMs).toBe(5000);
    for (const timeout of [0, -1, Number.NaN, Number.POSITIVE_INFINITY, '5']) expect(parse({ timeout }).timeoutMs).toBeUndefined();
    expect(parse({ timeout: 2 ** 40 }).timeoutMs).toBe(2 ** 31 - 1);
  });
});

describe('plainToMarkdown', () => {
  it('escapes markup and keeps line breaks', () => {
    expect(plainToMarkdown('a_b *c*\nd')).toBe('a\\_b \\*c\\*  \nd');
  });
});

describe('dialogToForm', () => {
  it('maps confirm to a body with Yes and No', () => {
    const form = dialogToForm(parse({ method: 'confirm', title: 'Clear?', message: 'All gone.' }));
    expect(form.title).toBe('Clear?');
    expect(form.body).toContain('All gone');
    expect(form.actions.map((a) => a.id)).toEqual(['yes', 'no']);
    expect(form.fields).toBeUndefined();
  });

  it('maps a short select to one action per option', () => {
    const form = dialogToForm(parse({ options: ['Allow', 'Block'] }));
    expect(form.actions.map((a) => a.label)).toEqual(['Allow', 'Block']);
    expect(form.fields).toBeUndefined();
  });

  it('maps a longer select to a select field, even with repeated labels', () => {
    const options = Array.from({ length: ACTION_OPTION_LIMIT + 2 }, () => 'same');
    const form = dialogToForm(parse({ options }));
    expect(form.fields?.[0]).toMatchObject({ type: 'select', required: true });
    expect(form.actions.map((a) => a.id)).toEqual(['choose']);
  });

  it('says when options were cut', () => {
    const options = Array.from({ length: MAX_DIALOG_OPTIONS + 2 }, (_u, i) => `o${i}`);
    expect(dialogToForm(parse({ options })).body).toContain('2 more options');
  });

  it('maps input and editor to a text field with the prefill as default', () => {
    const input = dialogToForm(parse({ method: 'input', placeholder: 'type', prefill: 'p' }));
    expect(input.fields?.[0]).toMatchObject({ type: 'text', placeholder: 'type', default: 'p' });
    const editor = dialogToForm(parse({ method: 'editor', prefill: 'a\nb' }));
    expect(editor.fields?.[0]).toMatchObject({ type: 'text', multiline: true, default: 'a\nb' });
  });

  it('falls back to a title when the extension gave none', () => {
    expect(dialogToForm(parse({ method: 'confirm', title: '' })).title).toBe('Confirm');
  });

  it('builds forms the validator accepts', () => {
    const cases = [
      parse({ method: 'confirm', message: 'm' }),
      parse({ options: ['a', 'b'] }),
      parse({ options: ['a', 'a', 'a', 'a', 'a', 'a'] }),
      parse({ method: 'input' }),
      parse({ method: 'editor', prefill: 'x' })
    ];
    for (const dialog of cases) expect(validateForm(dialogToForm(dialog)).ok).toBe(true);
  });
});

describe('dialogResponse', () => {
  const answer = (action: string, values: Record<string, string | string[]> = {}) => ({ action, values });

  it('answers confirm with a boolean', () => {
    const d = parse({ method: 'confirm' });
    expect(dialogResponse(d, answer('yes'))).toEqual({ type: 'extension_ui_response', id: 'u1', confirmed: true });
    expect(dialogResponse(d, answer('no'))).toEqual({ type: 'extension_ui_response', id: 'u1', confirmed: false });
  });

  it('answers select with the option text, from a button or from the field', () => {
    const d = parse({ options: ['Allow', 'Block'] });
    expect(dialogResponse(d, answer('option:1'))).toMatchObject({ value: 'Block' });
    const long = parse({ options: ['a', 'b', 'c', 'd', 'e', 'f'] });
    expect(dialogResponse(long, answer('choose', { choice: '4' }))).toMatchObject({ value: 'e' });
  });

  it('answers input and editor with the text, including an empty one', () => {
    expect(dialogResponse(parse({ method: 'input' }), answer('ok', { value: '' }))).toMatchObject({ value: '' });
    expect(dialogResponse(parse({ method: 'editor' }), answer('ok', { value: 'a\nb' }))).toMatchObject({ value: 'a\nb' });
  });

  it('cancels when the answer does not fit the dialog', () => {
    const cancelled = { type: 'extension_ui_response', id: 'u1', cancelled: true };
    expect(dialogResponse(parse({ method: 'confirm' }), answer('maybe'))).toEqual(cancelled);
    expect(dialogResponse(parse({ options: ['a'] }), answer('option:7'))).toEqual(cancelled);
    expect(dialogResponse(parse({ options: ['a', 'b', 'c', 'd', 'e'] }), answer('choose', { choice: 'x' }))).toEqual(cancelled);
    expect(dialogResponse(parse({ method: 'input' }), answer('ok', {}))).toEqual(cancelled);
  });
});
