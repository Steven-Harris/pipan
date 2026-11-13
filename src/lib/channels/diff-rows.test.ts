import { describe, expect, it } from 'vitest';
import { buildRows, COLLAPSE_LINES, defaultCollapsed, fileStats, lineOn, selectionText } from './diff-rows';
import type { DiffFile, DiffLine, ReviewComment } from './types';

const ctx = (n: number): DiffLine => ({ kind: 'ctx', text: `c${n}`, oldLine: n, newLine: n });
const file = (path: string, lines: DiffLine[]): DiffFile => ({
  path,
  status: 'modified',
  hunks: [{ header: '@@', lines }]
});
const comment = (over: Partial<ReviewComment>): ReviewComment => ({
  id: 'c1',
  path: 'a.ts',
  side: 'new',
  line: 1,
  text: 't',
  status: 'open',
  ...over
});
const a = file('a.ts', [
  ctx(1),
  { kind: 'del', text: 'gone', oldLine: 2 },
  { kind: 'add', text: 'fresh', newLine: 2 },
  ctx(3)
]);

describe('fileStats / lineOn', () => {
  it('counts lines, additions and deletions', () => {
    expect(fileStats(a)).toEqual({ lineCount: 4, additions: 1, deletions: 1 });
  });
  it('reads a side', () => {
    expect(lineOn(a.hunks[0].lines[1], 'new')).toBeUndefined();
    expect(lineOn(a.hunks[0].lines[1], 'old')).toBe(2);
  });
});

describe('buildRows', () => {
  const base = { files: [a], comments: [], collapsed: new Set<number>(), composer: null };
  it('emits a header, hunk header and one row per line', () => {
    const { rows, fileRows } = buildRows(base);
    expect(rows.map((r) => r.type)).toEqual(['file', 'hunk', 'line', 'line', 'line', 'line']);
    expect(fileRows).toEqual([0]);
  });
  it('omits lines of a collapsed file but keeps its header', () => {
    const { rows } = buildRows({ ...base, collapsed: new Set([0]) });
    expect(rows.map((r) => r.type)).toEqual(['file']);
  });
  it('puts threads after the line the comment ends on, on the matching side', () => {
    const { rows } = buildRows({
      ...base,
      comments: [comment({ id: 'n', side: 'new', line: 2 }), comment({ id: 'o', side: 'old', line: 2 })]
    });
    const types = rows.map((r) => r.type);
    expect(types).toEqual(['file', 'hunk', 'line', 'line', 'threads', 'line', 'threads', 'line']);
    const threads = rows.filter((r) => r.type === 'threads');
    expect(threads.map((t) => t.type === 'threads' && t.comments[0].id)).toEqual(['o', 'n']);
  });
  it('anchors a range at its last line', () => {
    const { rows } = buildRows({ ...base, comments: [comment({ line: 1, endLine: 3 })] });
    const idx = rows.findIndex((r) => r.type === 'threads');
    expect((rows[idx - 1] as { line: DiffLine }).line.newLine).toBe(3);
  });
  it('lists comments with an unknown file or line as orphans', () => {
    const { rows, orphans } = buildRows({
      ...base,
      comments: [comment({ id: 'x', path: 'other.ts' }), comment({ id: 'y', line: 99 }), comment({ id: 'z', line: 3 })]
    });
    expect(orphans.map((c) => c.id)).toEqual(['x', 'y']);
    expect(rows[0].type).toBe('orphans');
    expect(rows[1]).toMatchObject({ type: 'file', comments: 1 });
  });
  it('inserts the composer after its end line', () => {
    const { rows } = buildRows({ ...base, composer: { fileIndex: 0, path: 'a.ts', side: 'new', line: 2, endLine: 3 } });
    const idx = rows.findIndex((r) => r.type === 'composer');
    expect((rows[idx - 1] as { line: DiffLine }).line.newLine).toBe(3);
  });
  it('shows notices for binary, empty and truncated files', () => {
    const { rows } = buildRows({
      ...base,
      files: [{ path: 'b.png', status: 'added', binary: true, hunks: [] }, { path: 'e', status: 'renamed', hunks: [] }, { ...a, truncated: true }]
    });
    const texts = rows.filter((r) => r.type === 'notice').map((r) => (r.type === 'notice' ? r.text : ''));
    expect(texts).toEqual(['Binary file not shown.', 'No content changes.', 'The producer truncated this file.']);
  });
  it('builds 50,000 rows fast', () => {
    const lines = Array.from({ length: 1000 }, (_, i) => ctx(i + 1));
    const files = Array.from({ length: 50 }, (_, i) => file(`f${i}.ts`, lines));
    const t = performance.now();
    const { rows } = buildRows({ files, comments: [], collapsed: new Set(), composer: null });
    expect(rows.length).toBe(50 * 1002);
    expect(performance.now() - t).toBeLessThan(1000);
  });
});

describe('defaultCollapsed', () => {
  const big = file('big.ts', Array.from({ length: COLLAPSE_LINES + 1 }, (_, i) => ctx(i + 1)));
  it('collapses large files without comments', () => {
    expect([...defaultCollapsed([a, big], [])]).toEqual([1]);
  });
  it('keeps a large file open when it has a comment', () => {
    expect([...defaultCollapsed([a, big], [comment({ path: 'big.ts' })])]).toEqual([]);
  });
});

describe('selectionText', () => {
  it('collects the text of the lines on one side', () => {
    expect(selectionText(a, 'new', 1, 3)).toEqual(['c1', 'fresh', 'c3']);
    expect(selectionText(a, 'old', 2, 2)).toEqual(['gone']);
  });
});
