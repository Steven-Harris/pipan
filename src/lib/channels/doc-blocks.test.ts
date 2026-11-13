import { describe, expect, it } from 'vitest';
import { blockForLine, buildDocRows, inRange, linesText, parseDocument, selectBlock } from './doc-blocks';
import type { ReviewComment } from './types';

const md = '# Design\n\nFirst paragraph\nspans two lines.\n\n- item one\n- item two\n  continued\n\n```ts\nconst a = 1;\n\nconst b = 2;\n```\n\nClosing.\n';
const doc = parseDocument({ text: md });
const spans = doc.blocks.map((b) => [b.startLine, b.endLine]);

const comment = (over: Partial<ReviewComment>): ReviewComment => ({
  id: 'c',
  path: 'd.md',
  side: 'new',
  line: 1,
  text: 't',
  status: 'open',
  ...over
});

describe('parseDocument (markdown)', () => {
  it('gives each block its source lines', () => {
    expect(spans).toEqual([
      [1, 1], // heading
      [3, 4], // paragraph
      [6, 6], // item one
      [7, 8], // item two with continuation
      [10, 14], // fenced code with a blank line inside
      [16, 16] // closing
    ]);
  });
  it('renders html for each block', () => {
    expect(doc.blocks[0].html).toContain('<h1');
    expect(doc.blocks[2].html).toContain('<li>');
    expect(doc.blocks[4].html).toContain('<pre>');
  });
  it('splits the source into lines without a phantom last line', () => {
    expect(doc.lines).toHaveLength(16);
    expect(doc.lines[15]).toBe('Closing.');
  });
  it('keeps ordered list numbering per item', () => {
    const d = parseDocument({ text: '1. a\n2. b\n3. c\n' });
    expect(d.blocks.map((b) => [b.startLine, b.endLine])).toEqual([[1, 1], [2, 2], [3, 3]]);
    expect(d.blocks[1].html).toContain('start="2"');
  });
  it('handles loose lists and nested items', () => {
    const d = parseDocument({ text: '- a\n\n- b\n  - nested\n\n- c\n' });
    expect(d.blocks.map((b) => b.startLine)).toEqual([1, 3, 6]);
    expect(d.blocks[1].endLine).toBe(4);
  });
  it('normalizes line endings so numbers match an editor', () => {
    const d = parseDocument({ text: 'one\r\n\r\ntwo\r\n' });
    expect(d.blocks.map((b) => [b.startLine, b.endLine])).toEqual([[1, 1], [3, 3]]);
  });
  it('escapes raw html in a block', () => {
    const d = parseDocument({ text: '<script>alert(1)</script>\n\nok\n' });
    expect(d.blocks.every((b) => !b.html!.includes('<script'))).toBe(true);
  });
  it('handles empty text', () => {
    expect(parseDocument({ text: '' }).blocks).toEqual([]);
  });
  it('resolves reference links across blocks', () => {
    const d = parseDocument({ text: 'See [docs][d].\n\n[d]: https://example.com\n' });
    expect(d.blocks[0].html).toContain('href="https://example.com/"');
  });
});

describe('parseDocument (text)', () => {
  it('makes one block per line', () => {
    const d = parseDocument({ text: 'a\n\nb\n', format: 'text' });
    expect(d.blocks.map((b) => [b.startLine, b.text])).toEqual([[1, 'a'], [2, ''], [3, 'b']]);
  });
});

describe('blockForLine', () => {
  it('finds the block holding the line', () => {
    expect(blockForLine(doc.blocks, doc.lines.length, 4)).toBe(1);
    expect(blockForLine(doc.blocks, doc.lines.length, 12)).toBe(4);
  });
  it('uses the block above for a blank line', () => {
    expect(blockForLine(doc.blocks, doc.lines.length, 5)).toBe(1);
  });
  it('returns -1 outside the document', () => {
    expect(blockForLine(doc.blocks, doc.lines.length, 0)).toBe(-1);
    expect(blockForLine(doc.blocks, doc.lines.length, 99)).toBe(-1);
    expect(blockForLine([], 0, 1)).toBe(-1);
  });
});

describe('buildDocRows', () => {
  it('puts threads after the block of the last line and lists orphans', () => {
    const { rows, orphans } = buildDocRows(
      doc,
      [comment({ id: 'a', line: 3, endLine: 4 }), comment({ id: 'b', line: 99 })],
      null
    );
    expect(orphans.map((c) => c.id)).toEqual(['b']);
    expect(rows.map((r) => r.type).slice(0, 4)).toEqual(['block', 'block', 'threads', 'block']);
  });
  it('places the composer after the block holding its end line', () => {
    const { rows } = buildDocRows(doc, [], { line: 6, endLine: 7 });
    const i = rows.findIndex((r) => r.type === 'composer');
    expect((rows[i - 1] as { index: number }).index).toBe(3);
  });
});

describe('selection', () => {
  it('selects a block and extends over blocks', () => {
    expect(selectBlock(doc.blocks[1], null, false)).toEqual({ line: 3, endLine: 4 });
    expect(selectBlock(doc.blocks[0], null, false)).toEqual({ line: 1 });
    expect(selectBlock(doc.blocks[3], { line: 3, endLine: 4 }, true)).toEqual({ line: 3, endLine: 8 });
    expect(selectBlock(doc.blocks[0], { line: 3, endLine: 4 }, true)).toEqual({ line: 1, endLine: 4 });
    expect(selectBlock(doc.blocks[3], { line: 3 }, false)).toEqual({ line: 7, endLine: 8 });
  });
  it('tests whether a block is inside a range', () => {
    expect(inRange(doc.blocks[1], { line: 4 })).toBe(true);
    expect(inRange(doc.blocks[2], { line: 3, endLine: 4 })).toBe(false);
    expect(inRange(doc.blocks[2], null)).toBe(false);
  });
  it('returns the snippet lines', () => {
    expect(linesText(doc, 3, 4)).toEqual(['First paragraph', 'spans two lines.']);
  });
});
