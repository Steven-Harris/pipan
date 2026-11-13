/** Ids of the panels the channels extension adds for channel topics. */

const PREFIX = 'channel:graph:';

export const CHANNELS_EXTENSION_ID = 'pipan.channels';

export function graphPanelId(ch: string, topic: string): string {
  return `${PREFIX}${ch}:${topic}`;
}

/** Splits a graph panel id. Channel ids contain no colon, so the first colon ends the channel. */
export function parseGraphPanelId(id: string): { ch: string; topic: string } | null {
  if (!id.startsWith(PREFIX)) return null;
  const rest = id.slice(PREFIX.length);
  const split = rest.indexOf(':');
  if (split <= 0 || split === rest.length - 1) return null;
  return { ch: rest.slice(0, split), topic: rest.slice(split + 1) };
}
