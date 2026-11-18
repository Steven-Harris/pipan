/**
 * Splits a review document into commentable blocks and lays them out with their comment threads.
 * Markdown documents give one block per paragraph, heading, list item and so on; plain text gives
 * one block per line.
 */
import { renderMarkdownBlocks } from './markdown';
import { anchorKey, commentAnchor } from './review-model';
import type { ReviewComment, ReviewDocument } from './types';

export interface DocBlock {
  startLine: number;
  endLine: number;
  /** Rendered markdown. Absent for plain text, which uses `text`. */
  html?: string;
  text?: string;
}

export interface ParsedDocument {
  blocks: DocBlock[];
  /** Source lines, for snippets. */
  lines: string[];
}

export function parseDocument(doc: Pick<ReviewDocument, 'text' | 'format'>): ParsedDocument {
  const normalized = doc.text.replace(/\r\n|\r/g, '\n');
  const lines = normalized.split('\n');
  // A trailing newline ends the last line; it does not start another.
  if (lines.length > 1 && lines[lines.length - 1] === '') lines.pop();

  if (doc.format === 'text') {
    return { lines, blocks: lines.map((text, i) => ({ startLine: i + 1, endLine: i + 1, text })) };
  }
  return { lines, blocks: renderMarkdownBlocks(normalized).map((b) => ({ ...b })) };
}

/**
 * The block a comment appears after: the one holding its last line, or the closest block above
 * it when that line is blank. -1 when the line is outside the document.
 */
export function blockForLine(blocks: readonly DocBlock[], lineCount: number, line: number): number {
  if (blocks.length === 0 || line < 1 || line > lineCount) return -1;
  let found = 0;
  for (let i = 0; i < blocks.length; i++) {
    if (blocks[i].startLine <= line) found = i;
    else break;
  }
  return found;
}

export interface DocComposer {
  line: number;
  endLine?: number;
}

export type DocRow =
  | { key: string; type: 'block'; index: number; block: DocBlock }
  | { key: string; type: 'threads'; comments: ReviewComment[] }
  | { key: string; type: 'composer'; target: DocComposer };

export interface BuiltDocRows {
  rows: DocRow[];
  /** Comments whose line is not in this document. */
  orphans: ReviewComment[];
}

export function buildDocRows(
  parsed: ParsedDocument,
  comments: readonly ReviewComment[],
  composer: DocComposer | null
): BuiltDocRows {
  const { blocks, lines } = parsed;
  const byBlock = new Map<number, ReviewComment[]>();
  const orphans: ReviewComment[] = [];
  for (const c of comments) {
    const index = blockForLine(blocks, lines.length, c.endLine ?? c.line);
    if (index < 0) {
      orphans.push(c);
      continue;
    }
    const list = byBlock.get(index);
    if (list) list.push(c);
    else byBlock.set(index, [c]);
  }
  const composerBlock = composer ? blockForLine(blocks, lines.length, composer.endLine ?? composer.line) : -1;

  const rows: DocRow[] = [];
  blocks.forEach((block, index) => {
    rows.push({ key: `b${index}`, type: 'block', index, block });
    const threads = byBlock.get(index);
    if (threads) rows.push({ key: `b${index}:t`, type: 'threads', comments: threads });
    if (composer && composerBlock === index) rows.push({ key: 'composer', type: 'composer', target: composer });
  });
  return { rows, orphans };
}

/**
 * The line range after a click on a block. With `extend` and an existing selection the range
 * grows to cover both; otherwise it is just the block.
 */
export function selectBlock(block: DocBlock, existing: DocComposer | null, extend: boolean): DocComposer {
  let from = block.startLine;
  let to = block.endLine;
  if (extend && existing) {
    from = Math.min(from, existing.line);
    to = Math.max(to, existing.endLine ?? existing.line);
  }
  return to > from ? { line: from, endLine: to } : { line: from };
}

/** Source text of a line range, for the comment snippet. */
export const linesText = (parsed: ParsedDocument, from: number, to: number): string[] => parsed.lines.slice(from - 1, to);

export const inRange = (block: DocBlock, range: DocComposer | null): boolean => {
  if (!range) return false;
  const to = range.endLine ?? range.line;
  return block.endLine >= range.line && block.startLine <= to;
};

export { anchorKey, commentAnchor };
