import type { ChatMessage } from '#lib/types/pi.ts';
import type { LogItem } from '#lib/stores/channels.svelte.ts';

export type TranscriptItem =
  | { type: 'chat'; key: string; message: ChatMessage }
  | { type: 'channel'; key: string; item: LogItem };

/**
 * Puts channel log messages that a plugin can render into the chat transcript, by time. A log
 * item with no usable timestamp goes last. Chat messages keep their order.
 */
export function mergeTranscript(
  messages: readonly ChatMessage[],
  log: readonly LogItem[],
  canRender: (ch: string, name: string) => boolean
): TranscriptItem[] {
  const channelItems = log
    .filter((item) => item.kind === 'message' && canRender(item.ch, item.name))
    .map((item) => ({ item, time: item.timestamp ? Date.parse(item.timestamp) : Number.NaN }));

  const out: TranscriptItem[] = [];
  const timed = channelItems.filter((c) => !Number.isNaN(c.time)).sort((a, b) => a.time - b.time);
  const untimed = channelItems.filter((c) => Number.isNaN(c.time));

  let next = 0;
  for (const message of messages) {
    while (next < timed.length && timed[next].time <= message.timestamp) {
      out.push({ type: 'channel', key: `c:${timed[next].item.entryId}`, item: timed[next].item });
      next++;
    }
    out.push({ type: 'chat', key: message.id, message });
  }
  for (; next < timed.length; next++) {
    out.push({ type: 'channel', key: `c:${timed[next].item.entryId}`, item: timed[next].item });
  }
  for (const c of untimed) out.push({ type: 'channel', key: `c:${c.item.entryId}`, item: c.item });
  return out;
}
