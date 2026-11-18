// Pi's generic dialogs (`ctx.ui.select`, `confirm`, `input`, `editor`) arrive over RPC as
// `extension_ui_request` records. This module turns one into a `form` request the host can show,
// and turns the form's answer back into the `extension_ui_response` Pi expects. It does not say
// which extension asked, because Pi does not either.

import type { FormAnswer, FormRequest } from './types';

export const DIALOG_METHODS = ['select', 'confirm', 'input', 'editor'] as const;
export type DialogMethod = (typeof DIALOG_METHODS)[number];

/** Title prefix of the plan-gate approvals that PiPan answers for subagent processes. */
export const APPROVAL_TITLE_PREFIX = 'PIPAN_APPROVAL::';

/** Options shown for one `select`. Longer lists are cut and say so. */
export const MAX_DIALOG_OPTIONS = 500;
/** Longest title, message or prefill kept. */
export const MAX_DIALOG_TEXT = 100_000;
/** Up to this many `select` options become one button each. */
export const ACTION_OPTION_LIMIT = 4;
/** Longest delay honored for Pi's `timeout`. */
const MAX_TIMEOUT_MS = 2 ** 31 - 1;

export interface PiDialog {
  /** The id of the `extension_ui_request`, needed to answer it. */
  id: string;
  method: DialogMethod;
  title: string;
  message: string;
  options: string[];
  /** Options that were cut off. */
  omitted: number;
  placeholder: string;
  prefill: string;
  /** Milliseconds after which Pi resolves the dialog itself. */
  timeoutMs: number | undefined;
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const text = (value: unknown): string => (typeof value === 'string' ? value.slice(0, MAX_DIALOG_TEXT) : '');

export const isDialogMethod = (value: unknown): value is DialogMethod =>
  typeof value === 'string' && (DIALOG_METHODS as readonly string[]).includes(value);

/**
 * Reads a dialog request. Returns null for anything that is not one the host should show: another
 * method, a missing id, or a plan-gate approval.
 */
export function parseDialog(event: unknown): PiDialog | null {
  if (!isRecord(event) || event.type !== 'extension_ui_request') return null;
  if (!isDialogMethod(event.method) || typeof event.id !== 'string' || event.id === '') return null;
  const title = text(event.title);
  if (title.startsWith(APPROVAL_TITLE_PREFIX)) return null;
  const all = Array.isArray(event.options) ? event.options.filter((o): o is string => typeof o === 'string') : [];
  const timeout = typeof event.timeout === 'number' && Number.isFinite(event.timeout) && event.timeout > 0 ? event.timeout : undefined;
  return {
    id: event.id,
    method: event.method,
    title,
    message: text(event.message),
    options: all.slice(0, MAX_DIALOG_OPTIONS).map((o) => o.slice(0, 1000)),
    omitted: Math.max(0, all.length - MAX_DIALOG_OPTIONS),
    placeholder: text(event.placeholder),
    prefill: text(event.prefill),
    timeoutMs: timeout === undefined ? undefined : Math.min(timeout, MAX_TIMEOUT_MS)
  };
}

/**
 * Plain text as markdown that shows the same text: markup characters are escaped and a single
 * line break stays a line break.
 */
export function plainToMarkdown(source: string): string {
  return source
    .replace(/\r\n|\r/g, '\n')
    .replace(/[\\`*_{}[\]()#+\-.!|>~<&]/g, (c) => `\\${c}`)
    .replace(/\n/g, '  \n');
}

const FALLBACK_TITLE: Record<DialogMethod, string> = {
  select: 'Choose an option',
  confirm: 'Confirm',
  input: 'Enter a value',
  editor: 'Edit text'
};

/** The form that stands in for a dialog. Select option values are list positions. */
export function dialogToForm(dialog: PiDialog): FormRequest {
  const title = dialog.title.trim() === '' ? FALLBACK_TITLE[dialog.method] : dialog.title;
  switch (dialog.method) {
    case 'confirm':
      return {
        title,
        body: dialog.message === '' ? undefined : plainToMarkdown(dialog.message),
        actions: [
          { id: 'yes', label: 'Yes', style: 'primary' },
          { id: 'no', label: 'No' }
        ]
      };
    case 'select': {
      const note = dialog.omitted > 0 ? `${dialog.omitted} more options are not shown.` : undefined;
      const body = [dialog.message === '' ? undefined : plainToMarkdown(dialog.message), note].filter(Boolean).join('\n\n');
      if (dialog.options.length <= ACTION_OPTION_LIMIT) {
        return {
          title,
          body: body || undefined,
          actions: dialog.options.map((label, i) => ({ id: `option:${i}`, label }))
        };
      }
      return {
        title,
        body: body || undefined,
        fields: [
          {
            id: 'choice',
            type: 'select',
            label: 'Option',
            required: true,
            options: dialog.options.map((label, i) => ({ value: String(i), label }))
          }
        ],
        actions: [{ id: 'choose', label: 'Choose', style: 'primary' }]
      };
    }
    case 'input':
      return {
        title,
        body: dialog.message === '' ? undefined : plainToMarkdown(dialog.message),
        fields: [{ id: 'value', type: 'text', label: 'Value', placeholder: dialog.placeholder || undefined, default: dialog.prefill }],
        actions: [{ id: 'ok', label: 'OK', style: 'primary' }]
      };
    case 'editor':
      return {
        title,
        fields: [{ id: 'value', type: 'text', label: 'Text', multiline: true, default: dialog.prefill }],
        actions: [{ id: 'ok', label: 'OK', style: 'primary' }]
      };
  }
}

/** The `extension_ui_response` for a form answer, or a cancellation when the answer makes no sense. */
export function dialogResponse(dialog: PiDialog, answer: FormAnswer): Record<string, unknown> {
  const base = { type: 'extension_ui_response', id: dialog.id };
  const cancelled = { ...base, cancelled: true };
  switch (dialog.method) {
    case 'confirm':
      return answer.action === 'yes' ? { ...base, confirmed: true } : answer.action === 'no' ? { ...base, confirmed: false } : cancelled;
    case 'select': {
      const chosen = answer.action.startsWith('option:') ? answer.action.slice('option:'.length) : answer.values.choice;
      const index = typeof chosen === 'string' && /^\d+$/.test(chosen) ? Number(chosen) : -1;
      return index >= 0 && index < dialog.options.length ? { ...base, value: dialog.options[index] } : cancelled;
    }
    case 'input':
    case 'editor': {
      const value = answer.values.value;
      return typeof value === 'string' ? { ...base, value } : cancelled;
    }
  }
}
