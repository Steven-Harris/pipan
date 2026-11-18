/**
 * Comment model shared by diff and document reviews. Pure functions over an immutable state so
 * the rules (temporary ids, `touched`, choice gating) are testable without a UI.
 */
import type { ReviewAnswer, ReviewChoice, ReviewComment } from './types';

export interface ReviewState {
  /** The full list: comments from earlier rounds plus this round's additions. */
  comments: ReviewComment[];
  /** Ids the user added, edited, reopened or dismissed this round, in order of first touch. */
  touched: string[];
  /** Counter behind the next `tmp-<n>` id. */
  nextTmp: number;
}

export interface NewComment {
  path: string;
  side: 'old' | 'new';
  line: number;
  endLine?: number;
  snippet?: string;
  text: string;
}

const TMP_PREFIX = 'tmp-';
export const isTemporaryId = (id: string): boolean => id.startsWith(TMP_PREFIX);

export function createReviewState(comments: readonly ReviewComment[]): ReviewState {
  // Skip numbers already used by incoming ids so a producer echoing `tmp-` ids cannot collide.
  let next = 1;
  for (const c of comments) {
    if (isTemporaryId(c.id)) {
      const n = Number(c.id.slice(TMP_PREFIX.length));
      if (Number.isInteger(n) && n >= next) next = n + 1;
    }
  }
  return { comments: comments.map((c) => ({ ...c })), touched: [], nextTmp: next };
}

const withTouched = (touched: string[], id: string): string[] => (touched.includes(id) ? touched : [...touched, id]);

/** Adds a comment. Returns the state unchanged (and no id) when the text is blank. */
export function addComment(state: ReviewState, input: NewComment): { state: ReviewState; id?: string } {
  const text = input.text.trim();
  if (text === '') return { state };
  const id = `${TMP_PREFIX}${state.nextTmp}`;
  const comment: ReviewComment = {
    id,
    path: input.path,
    side: input.side,
    line: input.line,
    endLine: input.endLine !== undefined && input.endLine > input.line ? input.endLine : undefined,
    snippet: input.snippet,
    text,
    status: 'open'
  };
  return {
    id,
    state: {
      comments: [...state.comments, comment],
      touched: withTouched(state.touched, id),
      nextTmp: state.nextTmp + 1
    }
  };
}

function update(state: ReviewState, id: string, change: (c: ReviewComment) => ReviewComment | null): ReviewState {
  const index = state.comments.findIndex((c) => c.id === id);
  if (index < 0) return state;
  const next = change(state.comments[index]);
  if (next === state.comments[index]) return state;
  if (next === null) {
    return {
      ...state,
      comments: state.comments.filter((c) => c.id !== id),
      touched: state.touched.filter((t) => t !== id)
    };
  }
  const comments = state.comments.slice();
  comments[index] = next;
  return { ...state, comments, touched: withTouched(state.touched, id) };
}

/** Changes the text of an open comment. Blank or unchanged text is ignored. */
export function editComment(state: ReviewState, id: string, text: string): ReviewState {
  const trimmed = text.trim();
  return update(state, id, (c) => (trimmed === '' || c.status !== 'open' || trimmed === c.text ? c : { ...c, text: trimmed }));
}

/** Reopens a comment that is addressed, declined or dismissed. */
export function reopenComment(state: ReviewState, id: string): ReviewState {
  return update(state, id, (c) => (c.status === 'open' ? c : { ...c, status: 'open' }));
}

/**
 * Dismisses a comment. A comment added this round was never sent to the producer, so it is
 * removed instead of kept as dismissed.
 */
export function dismissComment(state: ReviewState, id: string): ReviewState {
  return update(state, id, (c) => (isTemporaryId(c.id) ? null : c.status === 'dismissed' ? c : { ...c, status: 'dismissed' }));
}

/** A choice that requires a comment is available once any comment is open or touched. */
export function hasActionableComment(state: ReviewState): boolean {
  return state.touched.length > 0 || state.comments.some((c) => c.status === 'open');
}

export function choiceEnabled(choice: ReviewChoice, state: ReviewState): boolean {
  return !choice.requiresComment || hasActionableComment(state);
}

export function buildReviewAnswer(state: ReviewState, choiceId: string, note?: string): ReviewAnswer {
  const trimmed = note?.trim();
  return {
    choice: choiceId,
    comments: state.comments,
    touched: state.touched,
    ...(trimmed ? { note: trimmed } : {})
  };
}

/** Number of changes the user would lose by closing without answering. */
export const unsentCount = (state: ReviewState): number => state.touched.length;

/** Key that ties a comment to the display line it appears after. */
export const anchorKey = (side: 'old' | 'new', line: number): string => `${side}:${line}`;

/** The display line of a comment: the last line of its range. */
export const commentAnchor = (c: ReviewComment): string => anchorKey(c.side, c.endLine ?? c.line);

/** First and last line of a selection, whichever way it was dragged. */
export function normalizeRange(a: number, b: number): { line: number; endLine?: number } {
  const line = Math.min(a, b);
  const end = Math.max(a, b);
  return end > line ? { line, endLine: end } : { line };
}

/** Largest snippet stored with a comment. */
export const MAX_SNIPPET_CHARS = 500;

export function clipSnippet(lines: readonly string[]): string | undefined {
  const joined = lines.join('\n');
  if (joined === '') return undefined;
  return joined.length > MAX_SNIPPET_CHARS ? `${joined.slice(0, MAX_SNIPPET_CHARS)}...` : joined;
}
