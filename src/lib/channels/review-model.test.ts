import { describe, expect, it } from 'vitest';
import {
  addComment,
  buildReviewAnswer,
  choiceEnabled,
  clipSnippet,
  commentAnchor,
  createReviewState,
  dismissComment,
  editComment,
  hasActionableComment,
  normalizeRange,
  reopenComment
} from './review-model';
import type { ReviewComment } from './types';

const earlier = (over: Partial<ReviewComment> = {}): ReviewComment => ({
  id: 'c1',
  path: 'a.ts',
  side: 'new',
  line: 3,
  text: 'Rename this.',
  status: 'open',
  ...over
});
const input = { path: 'a.ts', side: 'new' as const, line: 5, text: ' hello ' };

describe('addComment', () => {
  it('assigns sequential temporary ids and marks them touched', () => {
    let s = createReviewState([]);
    const a = addComment(s, input);
    const b = addComment(a.state, { ...input, line: 6 });
    expect([a.id, b.id]).toEqual(['tmp-1', 'tmp-2']);
    expect(b.state.touched).toEqual(['tmp-1', 'tmp-2']);
    expect(b.state.comments[0]).toMatchObject({ text: 'hello', status: 'open', path: 'a.ts', line: 5 });
    s = b.state;
    expect(s.comments).toHaveLength(2);
  });
  it('ignores blank text', () => {
    const s = createReviewState([]);
    const r = addComment(s, { ...input, text: '   ' });
    expect(r.id).toBeUndefined();
    expect(r.state).toBe(s);
  });
  it('keeps endLine only for real ranges', () => {
    const s = createReviewState([]);
    expect(addComment(s, { ...input, endLine: 5 }).state.comments[0].endLine).toBeUndefined();
    expect(addComment(s, { ...input, endLine: 8 }).state.comments[0].endLine).toBe(8);
  });
  it('skips temporary ids already in use', () => {
    const s = createReviewState([earlier({ id: 'tmp-4' })]);
    expect(addComment(s, input).id).toBe('tmp-5');
  });
});

describe('earlier comments', () => {
  it('start untouched and are not mutated by edits', () => {
    const original = earlier();
    const s = createReviewState([original]);
    expect(s.touched).toEqual([]);
    editComment(s, 'c1', 'changed');
    expect(original.text).toBe('Rename this.');
  });
  it('editing marks touched once and changes only open comments', () => {
    let s = createReviewState([earlier(), earlier({ id: 'c2', status: 'addressed' })]);
    s = editComment(s, 'c1', 'Rename it now.');
    s = editComment(s, 'c1', 'Rename it later.');
    expect(s.touched).toEqual(['c1']);
    expect(s.comments[0].text).toBe('Rename it later.');
    const unchanged = editComment(s, 'c2', 'nope');
    expect(unchanged).toBe(s);
  });
  it('ignores a blank or identical edit', () => {
    const s = createReviewState([earlier()]);
    expect(editComment(s, 'c1', '  ')).toBe(s);
    expect(editComment(s, 'c1', 'Rename this.')).toBe(s);
    expect(editComment(s, 'missing', 'x')).toBe(s);
  });
  it('reopens a closed comment and touches it', () => {
    const s = createReviewState([earlier({ status: 'declined' })]);
    const r = reopenComment(s, 'c1');
    expect(r.comments[0].status).toBe('open');
    expect(r.touched).toEqual(['c1']);
    expect(reopenComment(r, 'c1')).toBe(r);
  });
  it('dismissing keeps the comment as dismissed and touched', () => {
    const s = dismissComment(createReviewState([earlier()]), 'c1');
    expect(s.comments[0].status).toBe('dismissed');
    expect(s.touched).toEqual(['c1']);
  });
  it('dismissing a comment from this round removes it entirely', () => {
    const added = addComment(createReviewState([]), input);
    const s = dismissComment(added.state, added.id!);
    expect(s.comments).toEqual([]);
    expect(s.touched).toEqual([]);
  });
});

describe('choices', () => {
  const needs = { id: 'changes', label: 'Request changes', requiresComment: true };
  const free = { id: 'approve', label: 'Approve' };
  it('gates requiresComment on an open or touched comment', () => {
    const none = createReviewState([]);
    expect(choiceEnabled(needs, none)).toBe(false);
    expect(choiceEnabled(free, none)).toBe(true);
    expect(choiceEnabled(needs, createReviewState([earlier()]))).toBe(true);
    expect(choiceEnabled(needs, createReviewState([earlier({ status: 'addressed' })]))).toBe(false);
  });
  it('a dismissed comment still counts because the user touched it', () => {
    const s = dismissComment(createReviewState([earlier()]), 'c1');
    expect(hasActionableComment(s)).toBe(true);
  });
});

describe('buildReviewAnswer', () => {
  it('carries the choice, the full list and touched ids', () => {
    const s = addComment(createReviewState([earlier()]), input).state;
    const answer = buildReviewAnswer(s, 'changes');
    expect(answer.choice).toBe('changes');
    expect(answer.comments.map((c) => c.id)).toEqual(['c1', 'tmp-1']);
    expect(answer.touched).toEqual(['tmp-1']);
    expect('note' in answer).toBe(false);
  });
  it('includes a trimmed note only when present', () => {
    const s = createReviewState([]);
    expect(buildReviewAnswer(s, 'approve', '  ship it ').note).toBe('ship it');
    expect('note' in buildReviewAnswer(s, 'approve', '   ')).toBe(false);
  });
});

describe('helpers', () => {
  it('anchors a range comment at its last line', () => {
    expect(commentAnchor(earlier({ line: 3, endLine: 6 }))).toBe('new:6');
    expect(commentAnchor(earlier({ side: 'old' }))).toBe('old:3');
  });
  it('normalizes a range in either direction', () => {
    expect(normalizeRange(7, 3)).toEqual({ line: 3, endLine: 7 });
    expect(normalizeRange(4, 4)).toEqual({ line: 4 });
  });
  it('clips long snippets', () => {
    expect(clipSnippet([])).toBeUndefined();
    expect(clipSnippet(['a', 'b'])).toBe('a\nb');
    expect(clipSnippet(['x'.repeat(600)])!.length).toBe(503);
  });
});
