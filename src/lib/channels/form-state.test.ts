import { describe, expect, it } from 'vitest';
import { appliesTo, buildAnswer, defaultAction, initialValues, needsValidation, validateValues } from './form-state';
import type { FormRequest } from './types';

const opts = [
  { value: 'a', label: 'A' },
  { value: 'b', label: 'B' },
  { value: 'c', label: 'C' }
];
const form: FormRequest = {
  title: 'T',
  fields: [
    { id: 's', type: 'select', label: 'S', options: opts, required: true },
    { id: 'm', type: 'multiselect', label: 'M', options: opts, min: 1, max: 2, default: ['b'] },
    { id: 't', type: 'text', label: 'T', required: true, default: 'hello' }
  ],
  actions: [{ id: 'go', label: 'Go', style: 'primary' }]
};

describe('initialValues', () => {
  it('uses defaults and empties', () => {
    expect(initialValues(form)).toEqual({ s: '', m: ['b'], t: 'hello' });
    expect(initialValues({})).toEqual({});
  });
  it('copies default arrays so edits do not leak into the request', () => {
    const values = initialValues(form);
    (values.m as string[]).push('c');
    expect((form.fields![1] as { default: string[] }).default).toEqual(['b']);
  });
});

describe('validateValues', () => {
  it('accepts valid values', () => {
    expect(validateValues(form, { s: 'a', m: ['b'], t: 'x' }, 'go')).toEqual({});
  });
  it('flags a required select, min, max, and a blank required text', () => {
    expect(validateValues(form, { s: '', m: [], t: '   ' }, 'go')).toEqual({
      s: 'Choose an option.',
      m: 'Choose at least 1.',
      t: 'This field is required.'
    });
    expect(validateValues(form, { s: 'a', m: ['a', 'b', 'c'], t: 'x' }, 'go').m).toBe('Choose at most 2.');
  });
  it('lets an optional select stay empty but rejects unlisted values', () => {
    const optional: FormRequest = {
      ...form,
      fields: [{ id: 's', type: 'select', label: 'S', options: opts }]
    };
    expect(validateValues(optional, { s: '' }, 'go')).toEqual({});
    expect(validateValues(optional, { s: 'zzz' }, 'go').s).toMatch(/listed/);
    expect(validateValues(form, { s: 'a', m: ['nope'], t: 'x' }, 'go').m).toMatch(/listed/);
  });
  it('treats missing values as empty', () => {
    expect(Object.keys(validateValues(form, {}, 'go'))).toEqual(['s', 'm', 't']);
  });
  it('does nothing for a body-only form', () => {
    expect(validateValues({}, {}, 'go')).toEqual({});
  });
});

describe('buildAnswer', () => {
  it('returns the action and a value for every field', () => {
    expect(buildAnswer(form, 'go', { s: 'a', m: ['c', 'b'], t: 'x' })).toEqual({
      action: 'go',
      values: { s: 'a', m: ['b', 'c'], t: 'x' }
    });
  });
  it('drops unlisted values and fills missing ones', () => {
    expect(buildAnswer(form, 'skip', { s: 'zzz', m: ['b', 'zzz'] })).toEqual({
      action: 'skip',
      values: { s: '', m: ['b'], t: '' }
    });
  });
  it('keeps multiline text exactly', () => {
    const edit: FormRequest = { fields: [{ id: 'x', type: 'text', label: 'X', multiline: true }], title: 'E', actions: [] };
    expect(buildAnswer(edit, 'save', { x: 'a\n  b\n' }).values.x).toBe('a\n  b\n');
  });
  it('answers a body-only form with just the action', () => {
    expect(buildAnswer({}, 'yes', {})).toEqual({ action: 'yes', values: {} });
  });
});

describe('actions', () => {
  it('skips validation only when asked', () => {
    expect(needsValidation({ id: 'a', label: 'A' })).toBe(true);
    expect(needsValidation({ id: 'a', label: 'A', skipValidation: true })).toBe(false);
  });
  it('picks the Enter action', () => {
    expect(defaultAction({ actions: [{ id: 'a', label: 'A' }, { id: 'b', label: 'B', style: 'primary' }] })?.id).toBe('b');
    expect(defaultAction({ actions: [{ id: 'a', label: 'A' }] })?.id).toBe('a');
    expect(defaultAction({ actions: [{ id: 'a', label: 'A' }, { id: 'b', label: 'B' }] })).toBeUndefined();
  });
});

describe('requiredFor', () => {
  const answerForm: FormRequest = {
    title: 'Q',
    fields: [
      { id: 'pick', type: 'select', label: 'Pick', options: opts, required: true, requiredFor: ['submit'] },
      { id: 'tags', type: 'multiselect', label: 'Tags', options: opts, min: 1, max: 2, requiredFor: ['submit'] },
      { id: 'text', type: 'text', label: 'Text', required: true, requiredFor: ['submit'] },
      { id: 'always', type: 'text', label: 'Always', required: true }
    ],
    actions: [
      { id: 'submit', label: 'Submit', style: 'primary' },
      { id: 'skip', label: 'Skip' }
    ]
  };
  const empty = { pick: '', tags: [], text: '  ', always: 'x' };

  it('applies required and min only to the listed actions', () => {
    expect(validateValues(answerForm, empty, 'submit')).toEqual({
      pick: 'Choose an option.',
      tags: 'Choose at least 1.',
      text: 'This field is required.'
    });
    expect(validateValues(answerForm, empty, 'skip')).toEqual({});
  });

  it('still applies a field without requiredFor to every action', () => {
    expect(validateValues(answerForm, { ...empty, always: '' }, 'skip')).toEqual({ always: 'This field is required.' });
  });

  it('keeps max and option membership for every action', () => {
    expect(validateValues(answerForm, { ...empty, tags: ['a', 'b', 'c'] }, 'skip').tags).toBe('Choose at most 2.');
    expect(validateValues(answerForm, { ...empty, pick: 'zzz' }, 'skip').pick).toMatch(/listed/);
  });

  it('treats an empty list as applying to no action and ignores unknown action ids', () => {
    const f: FormRequest = {
      title: 'Q',
      fields: [{ id: 't', type: 'text', label: 'T', required: true, requiredFor: [] }],
      actions: [{ id: 'a', label: 'A' }]
    };
    expect(validateValues(f, { t: '' }, 'a')).toEqual({});
    const stale: FormRequest = { ...f, fields: [{ id: 't', type: 'text', label: 'T', required: true, requiredFor: ['gone', 'a'] }] };
    expect(validateValues(stale, { t: '' }, 'a')).toEqual({ t: 'This field is required.' });
  });

  it('sends empty values for fields an action did not require', () => {
    const answer = buildAnswer(answerForm, 'skip', empty);
    expect(answer).toEqual({ action: 'skip', values: { pick: '', tags: [], text: '  ', always: 'x' } });
  });

  it('reports which fields apply to an action', () => {
    const field = answerForm.fields![0];
    expect(appliesTo(field, 'submit')).toBe(true);
    expect(appliesTo(field, 'skip')).toBe(false);
    expect(appliesTo(answerForm.fields![3], 'skip')).toBe(true);
  });
});
