import type { FormAction, FormAnswer, FormField, FormRequest } from './types';

export type FormValues = Record<string, string | string[]>;
/** Error message per field id. */
export type FormErrors = Record<string, string>;

/** Starting values: producer defaults, otherwise empty. */
export function initialValues(form: Pick<FormRequest, 'fields'>): FormValues {
  const values: FormValues = {};
  for (const field of form.fields ?? []) {
    if (field.type === 'multiselect') values[field.id] = [...(field.default ?? [])];
    else values[field.id] = field.default ?? '';
  }
  return values;
}

/**
 * True when the field's `required` (or a multiselect's `min`) applies to the action. A field
 * without `requiredFor` applies to every action. Ids in the list that match no action are
 * harmless, so a stale list cannot break a form.
 */
export const appliesTo = (field: FormField, actionId: string): boolean =>
  field.requiredFor === undefined || field.requiredFor.includes(actionId);

function fieldError(field: FormField, value: string | string[] | undefined, actionId: string): string | null {
  const strict = appliesTo(field, actionId);
  switch (field.type) {
    case 'select': {
      const chosen = typeof value === 'string' ? value : '';
      if (chosen === '') return field.required && strict ? 'Choose an option.' : null;
      return field.options.some((o) => o.value === chosen) ? null : 'Choose one of the listed options.';
    }
    case 'multiselect': {
      const chosen = Array.isArray(value) ? value : [];
      if (chosen.some((v) => !field.options.some((o) => o.value === v))) return 'Choose only listed options.';
      if (strict && field.min !== undefined && chosen.length < field.min) {
        return `Choose at least ${field.min}.`;
      }
      if (field.max !== undefined && chosen.length > field.max) {
        return `Choose at most ${field.max}.`;
      }
      return null;
    }
    case 'text': {
      const text = typeof value === 'string' ? value : '';
      return field.required && strict && text.trim() === '' ? 'This field is required.' : null;
    }
  }
}

/**
 * Errors for every invalid field when the action is pressed. Empty when the values satisfy the
 * form. `max` and the listed options always apply, `required` and `min` only to the actions a
 * field's `requiredFor` names.
 */
export function validateValues(
  form: Pick<FormRequest, 'fields'>,
  values: FormValues,
  actionId: string
): FormErrors {
  const errors: FormErrors = {};
  for (const field of form.fields ?? []) {
    const message = fieldError(field, values[field.id], actionId);
    if (message) errors[field.id] = message;
  }
  return errors;
}

/** True when pressing the action must check the fields first. */
export const needsValidation = (action: FormAction): boolean => action.skipValidation !== true;

/**
 * The answer for an action: every field's value, with multiselect values in option order and
 * values that are not listed options removed.
 */
export function buildAnswer(form: Pick<FormRequest, 'fields'>, actionId: string, values: FormValues): FormAnswer {
  const answer: FormValues = {};
  for (const field of form.fields ?? []) {
    const value = values[field.id];
    if (field.type === 'multiselect') {
      const chosen = new Set(Array.isArray(value) ? value : []);
      answer[field.id] = field.options.map((o) => o.value).filter((v) => chosen.has(v));
    } else if (field.type === 'select') {
      const chosen = typeof value === 'string' ? value : '';
      answer[field.id] = field.options.some((o) => o.value === chosen) ? chosen : '';
    } else {
      answer[field.id] = typeof value === 'string' ? value : '';
    }
  }
  return { action: actionId, values: answer };
}

/** The action Enter submits: the first primary one, else the only one, else none. */
export function defaultAction(form: Pick<FormRequest, 'actions'>): FormAction | undefined {
  return form.actions.find((a) => a.style === 'primary') ?? (form.actions.length === 1 ? form.actions[0] : undefined);
}
