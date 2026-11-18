import type { NodeStatus, Tone } from './types';

/** Literal class strings so Tailwind sees them. */
export interface ToneClasses {
  border: string;
  badge: string;
  text: string;
}

export const TONE_CLASSES: Record<Tone, ToneClasses> = {
  neutral: { border: 'border-zinc-700', badge: 'bg-zinc-700/60 text-zinc-200', text: 'text-zinc-300' },
  cyan: { border: 'border-cyan-600/70', badge: 'bg-cyan-500/20 text-cyan-200', text: 'text-cyan-300' },
  amber: { border: 'border-amber-600/70', badge: 'bg-amber-500/20 text-amber-200', text: 'text-amber-300' },
  violet: { border: 'border-violet-600/70', badge: 'bg-violet-500/20 text-violet-200', text: 'text-violet-300' },
  green: { border: 'border-emerald-600/70', badge: 'bg-emerald-500/20 text-emerald-200', text: 'text-emerald-300' },
  red: { border: 'border-red-600/70', badge: 'bg-red-500/20 text-red-200', text: 'text-red-300' },
  blue: { border: 'border-sky-600/70', badge: 'bg-sky-500/20 text-sky-200', text: 'text-sky-300' }
};

/** Tones a kind can hash to. Red is left out because it reads as failure. */
const KIND_TONES: readonly Tone[] = ['cyan', 'violet', 'blue', 'green', 'amber'];

function hash(text: string): number {
  let h = 2166136261;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

/** The tone for a node: the producer's `tone`, else a stable one per kind, else neutral. */
export function resolveTone(kind: string | undefined, tone: Tone | undefined): Tone {
  if (tone) return tone;
  if (!kind) return 'neutral';
  return KIND_TONES[hash(kind) % KIND_TONES.length];
}

/** A one-character mark for a kind; empty when there is no kind. */
export function kindGlyph(kind: string | undefined): string {
  const first = kind?.trim().charAt(0);
  return first ? first.toUpperCase() : '';
}

export interface StatusStyle {
  label: string;
  /** Classes for the status dot. */
  dot: string;
  /** Extra classes for the whole card. */
  card: string;
}

export const STATUS_STYLES: Record<NodeStatus, StatusStyle> = {
  pending: { label: 'Pending', dot: 'border border-zinc-500 bg-transparent', card: 'opacity-70' },
  active: { label: 'Active', dot: 'bg-cyan-400 animate-pulse', card: 'ring-1 ring-cyan-500/60' },
  waiting: { label: 'Waiting', dot: 'bg-amber-400 animate-pulse', card: 'ring-1 ring-amber-500/50' },
  done: { label: 'Done', dot: 'bg-emerald-400', card: '' },
  failed: { label: 'Failed', dot: 'bg-red-500', card: 'ring-1 ring-red-500/60' },
  skipped: { label: 'Skipped', dot: 'bg-zinc-600', card: 'opacity-50' }
};
