// Messages that extensions add to the session with `pi.sendMessage`. Pi stores them as
// `role: "custom"`, and `display` says whether a client should draw them.

import { parseLogType } from '#lib/channels/codec.ts';
import type { PiRawMessage } from '#lib/types/pi.ts';

/** Longest text drawn for one message. */
export const MAX_CUSTOM_MESSAGE_CHARS = 20_000;

export interface CustomMessageView {
  customType: string;
  text: string;
}

function textOf(content: unknown): string {
  if (typeof content === 'string') return content;
  if (!Array.isArray(content)) return '';
  return content
    .map((item) =>
      item && typeof item === 'object' && 'text' in item && typeof item.text === 'string' ? item.text : ''
    )
    .filter(Boolean)
    .join('\n');
}

/**
 * What to draw for a raw message, or null when it is not a custom message that should be shown:
 * `display` must be true (messages meant for the model only stay hidden), and channel log
 * messages are left to the plugins that render them.
 */
export function customMessageView(raw: PiRawMessage): CustomMessageView | null {
  if (raw.role !== 'custom' || raw.display !== true) return null;
  const customType = typeof raw.customType === 'string' ? raw.customType : '';
  if (customType === '' || parseLogType(customType)) return null;
  let text = textOf(raw.content).trim();
  if (text === '') return null;
  if (text.length > MAX_CUSTOM_MESSAGE_CHARS) text = `${text.slice(0, MAX_CUSTOM_MESSAGE_CHARS)}\n\n(truncated)`;
  return { customType, text };
}
