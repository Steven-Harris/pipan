/**
 * Flattens a diff plus its comments into one list of rows, so a virtual list can render any
 * window of a 50,000-line diff.
 */
import { anchorKey, commentAnchor } from './review-model';
import type { DiffFile, DiffLine, ReviewComment } from './types';

/** Files with more lines than this start collapsed, unless they carry a comment. */
export const COLLAPSE_LINES = 400;

export type Side = 'old' | 'new';

export interface ComposerTarget {
  fileIndex: number;
  path: string;
  side: Side;
  line: number;
  endLine?: number;
}

export type DiffRow =
  | { key: string; type: 'orphans'; comments: ReviewComment[] }
  | {
      key: string;
      type: 'file';
      fileIndex: number;
      file: DiffFile;
      collapsed: boolean;
      lineCount: number;
      additions: number;
      deletions: number;
      comments: number;
    }
  | { key: string; type: 'notice'; fileIndex: number; text: string }
  | { key: string; type: 'hunk'; fileIndex: number; header: string }
  | { key: string; type: 'line'; fileIndex: number; line: DiffLine }
  | { key: string; type: 'threads'; fileIndex: number; comments: ReviewComment[] }
  | { key: string; type: 'composer'; fileIndex: number; target: ComposerTarget };

export interface FileStats {
  lineCount: number;
  additions: number;
  deletions: number;
}

export function fileStats(file: DiffFile): FileStats {
  let additions = 0;
  let deletions = 0;
  let lineCount = 0;
  for (const hunk of file.hunks) {
    for (const line of hunk.lines) {
      lineCount++;
      if (line.kind === 'add') additions++;
      else if (line.kind === 'del') deletions++;
    }
  }
  return { lineCount, additions, deletions };
}

/** The line number a row has on one side, if any. */
export const lineOn = (line: DiffLine, side: Side): number | undefined => (side === 'old' ? line.oldLine : line.newLine);

/** Indexes of files that start collapsed. */
export function defaultCollapsed(files: readonly DiffFile[], comments: readonly ReviewComment[]): Set<number> {
  const withComments = new Set(comments.map((c) => c.path));
  const collapsed = new Set<number>();
  files.forEach((file, i) => {
    if (fileStats(file).lineCount > COLLAPSE_LINES && !withComments.has(file.path)) collapsed.add(i);
  });
  return collapsed;
}

export interface BuildInput {
  files: readonly DiffFile[];
  comments: readonly ReviewComment[];
  collapsed: ReadonlySet<number>;
  composer: ComposerTarget | null;
}

export interface BuiltRows {
  rows: DiffRow[];
  /** Row index of each file's header, by file index. */
  fileRows: number[];
  /** Comments whose anchor is not on a visible line of the diff. */
  orphans: ReviewComment[];
}

/** Keys of the lines a file shows, so comments pointing elsewhere can be listed separately. */
function visibleAnchors(file: DiffFile): Set<string> {
  const keys = new Set<string>();
  for (const hunk of file.hunks) {
    for (const line of hunk.lines) {
      if (line.oldLine !== undefined) keys.add(anchorKey('old', line.oldLine));
      if (line.newLine !== undefined) keys.add(anchorKey('new', line.newLine));
    }
  }
  return keys;
}

export function buildRows({ files, comments, collapsed, composer }: BuildInput): BuiltRows {
  const rows: DiffRow[] = [];
  const fileRows: number[] = [];
  const fileIndexByPath = new Map<string, number>();
  files.forEach((f, i) => fileIndexByPath.set(f.path, i));

  // Group comments by file and anchor. A comment whose file or line is not shown is an orphan.
  const anchors = files.map(visibleAnchors);
  const byAnchor = files.map(() => new Map<string, ReviewComment[]>());
  const perFile = files.map(() => 0);
  const orphans: ReviewComment[] = [];
  for (const c of comments) {
    const fi = fileIndexByPath.get(c.path);
    const key = commentAnchor(c);
    if (fi === undefined || !anchors[fi].has(key)) {
      orphans.push(c);
      continue;
    }
    perFile[fi]++;
    const list = byAnchor[fi].get(key);
    if (list) list.push(c);
    else byAnchor[fi].set(key, [c]);
  }

  if (orphans.length > 0) rows.push({ key: 'orphans', type: 'orphans', comments: orphans });

  files.forEach((file, fileIndex) => {
    const stats = fileStats(file);
    const isCollapsed = collapsed.has(fileIndex);
    fileRows.push(rows.length);
    rows.push({
      key: `f${fileIndex}`,
      type: 'file',
      fileIndex,
      file,
      collapsed: isCollapsed,
      ...stats,
      comments: perFile[fileIndex]
    });
    if (isCollapsed) return;

    if (file.binary) rows.push({ key: `f${fileIndex}:n`, type: 'notice', fileIndex, text: 'Binary file not shown.' });
    else if (file.hunks.length === 0)
      rows.push({ key: `f${fileIndex}:n`, type: 'notice', fileIndex, text: 'No content changes.' });

    const composerHere = composer && composer.fileIndex === fileIndex ? anchorKey(composer.side, composer.endLine ?? composer.line) : null;

    file.hunks.forEach((hunk, hi) => {
      rows.push({ key: `f${fileIndex}:h${hi}`, type: 'hunk', fileIndex, header: hunk.header });
      hunk.lines.forEach((line, li) => {
        rows.push({ key: `f${fileIndex}:h${hi}:l${li}`, type: 'line', fileIndex, line });
        const here: ReviewComment[] = [];
        const keys: string[] = [];
        if (line.oldLine !== undefined) keys.push(anchorKey('old', line.oldLine));
        if (line.newLine !== undefined) keys.push(anchorKey('new', line.newLine));
        for (const key of keys) {
          const list = byAnchor[fileIndex].get(key);
          if (list) here.push(...list);
        }
        if (here.length > 0) rows.push({ key: `f${fileIndex}:h${hi}:l${li}:t`, type: 'threads', fileIndex, comments: here });
        if (composer && composerHere && keys.includes(composerHere)) {
          rows.push({ key: `f${fileIndex}:composer`, type: 'composer', fileIndex, target: composer });
        }
      });
    });

    if (file.truncated) {
      rows.push({ key: `f${fileIndex}:trunc`, type: 'notice', fileIndex, text: 'The producer truncated this file.' });
    }
  });

  return { rows, fileRows, orphans };
}

/** The text of the lines a selection covers, for the comment snippet. */
export function selectionText(file: DiffFile, side: Side, from: number, to: number): string[] {
  const out: string[] = [];
  for (const hunk of file.hunks) {
    for (const line of hunk.lines) {
      const n = lineOn(line, side);
      if (n !== undefined && n >= from && n <= to) out.push(line.text);
    }
  }
  return out;
}
