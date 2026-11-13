<script lang="ts">
  // Unified diff with a file list, line and range comments, and threads from earlier rounds.
  // Rows are flattened and virtualized so a 50,000-line diff stays responsive.
  import {
    buildRows,
    defaultCollapsed,
    lineOn,
    selectionText,
    type ComposerTarget,
    type DiffRow,
    type Side
  } from '#lib/channels/diff-rows.ts';
  import { clipSnippet, normalizeRange, type NewComment, type ReviewState } from '#lib/channels/review-model.ts';
  import type { DiffLine, DiffSubject } from '#lib/channels/types.ts';
  import { ChevronDown, ChevronRight } from 'lucide-svelte';
  import CommentComposer from './CommentComposer.svelte';
  import CommentThread from './CommentThread.svelte';
  import type { ReviewUi } from './review-ui.ts';
  import VirtualList from './VirtualList.svelte';

  let {
    subject,
    review,
    ui,
    hint,
    onadd,
    onedit,
    onreopen,
    ondismiss
  }: {
    subject: DiffSubject;
    review: ReviewState;
    ui: ReviewUi;
    hint?: string;
    onadd: (input: NewComment) => void;
    onedit: (id: string, text: string) => void;
    onreopen: (id: string) => void;
    ondismiss: (id: string) => void;
  } = $props();

  const ROW_HEIGHT = 20;
  const FILE_HEIGHT = 34;
  const HUNK_HEIGHT = 22;
  const NOTICE_HEIGHT = 26;

  // The host remounts the view for every request, so reading the props once is intended.
  // svelte-ignore state_referenced_locally
  let collapsed = $state(defaultCollapsed(subject.files, review.comments));
  let activeFile = $state(0);
  let list: ReturnType<typeof VirtualList<DiffRow>> | undefined = $state();

  /** The range being dragged, before the comment box opens. */
  let drag = $state<{ fileIndex: number; side: Side; anchor: number; head: number } | null>(null);

  const built = $derived(buildRows({ files: subject.files, comments: review.comments, collapsed, composer: ui.composer }));
  const rows = $derived(built.rows);

  const selection = $derived.by(() => {
    if (drag) {
      const r = normalizeRange(drag.anchor, drag.head);
      return { fileIndex: drag.fileIndex, side: drag.side, from: r.line, to: r.endLine ?? r.line };
    }
    const c = ui.composer;
    return c ? { fileIndex: c.fileIndex, side: c.side, from: c.line, to: c.endLine ?? c.line } : null;
  });

  const inSelection = (fileIndex: number, line: DiffLine) => {
    const s = selection;
    if (!s || s.fileIndex !== fileIndex) return false;
    const n = lineOn(line, s.side);
    return n !== undefined && n >= s.from && n <= s.to;
  };

  const fixedHeight = (row: DiffRow): number | undefined => {
    switch (row.type) {
      case 'line':
        return ROW_HEIGHT;
      case 'file':
        return FILE_HEIGHT;
      case 'hunk':
        return HUNK_HEIGHT;
      case 'notice':
        return NOTICE_HEIGHT;
      default:
        return undefined;
    }
  };

  const STATUS_BADGE: Record<string, { letter: string; cls: string }> = {
    added: { letter: 'A', cls: 'text-emerald-400' },
    untracked: { letter: 'U', cls: 'text-emerald-400' },
    modified: { letter: 'M', cls: 'text-amber-400' },
    deleted: { letter: 'D', cls: 'text-red-400' },
    renamed: { letter: 'R', cls: 'text-sky-400' }
  };

  const commentTotals = $derived(
    subject.files.map((f) => review.comments.filter((c) => c.path === f.path && c.status === 'open').length)
  );

  function toggleFile(index: number) {
    const next = new Set(collapsed);
    if (next.has(index)) next.delete(index);
    else next.add(index);
    collapsed = next;
  }

  function jumpToFile(index: number) {
    if (collapsed.has(index)) toggleFile(index);
    list?.scrollToIndex(built.fileRows[index]);
  }

  function onvisible(index: number) {
    const row = rows[index];
    if (row && 'fileIndex' in row) activeFile = row.fileIndex;
  }

  // ---- selecting lines ----

  function currentSelection() {
    if (drag) return drag;
    const c = ui.composer;
    return c ? { fileIndex: c.fileIndex, side: c.side, anchor: c.line, head: c.endLine ?? c.line } : null;
  }

  function startSelect(event: PointerEvent, fileIndex: number, line: DiffLine, side: Side) {
    if (event.button !== 0) return;
    const n = lineOn(line, side);
    if (n === undefined) return;
    event.preventDefault();
    const existing = currentSelection();
    if (event.shiftKey && existing && existing.fileIndex === fileIndex && existing.side === side) {
      drag = { fileIndex, side, anchor: existing.anchor, head: n };
    } else {
      drag = { fileIndex, side, anchor: n, head: n };
    }
    ui.composer = null;
  }

  function extendSelect(fileIndex: number, line: DiffLine) {
    if (!drag || drag.fileIndex !== fileIndex) return;
    const n = lineOn(line, drag.side);
    if (n !== undefined) drag.head = n;
  }

  function finishSelect() {
    if (!drag) return;
    const range = normalizeRange(drag.anchor, drag.head);
    ui.composer = { fileIndex: drag.fileIndex, path: subject.files[drag.fileIndex].path, side: drag.side, ...range };
    drag = null;
  }

  /** Keyboard activation: Enter or Space on a line number button. */
  function keyboardSelect(event: MouseEvent, fileIndex: number, line: DiffLine, side: Side) {
    if (event.detail !== 0) return;
    const n = lineOn(line, side);
    if (n === undefined) return;
    ui.composer = { fileIndex, path: subject.files[fileIndex].path, side, line: n };
  }

  function cancelComposer() {
    ui.composer = null;
    ui.draft = '';
  }

  function saveComposer() {
    const target = ui.composer;
    if (!target) return;
    const file = subject.files[target.fileIndex];
    onadd({
      path: target.path,
      side: target.side,
      line: target.line,
      endLine: target.endLine,
      snippet: clipSnippet(selectionText(file, target.side, target.line, target.endLine ?? target.line)),
      text: ui.draft
    });
    ui.composer = null;
    ui.draft = '';
  }

  const describeTarget = (t: ComposerTarget) =>
    `${t.endLine ? `lines ${t.line}-${t.endLine}` : `line ${t.line}`} of ${t.path}${t.side === 'old' ? ' (before)' : ''}`;
</script>

<svelte:window onpointerup={finishSelect} onpointercancel={finishSelect} />

<div class="flex min-h-0 flex-1">
  <nav class="flex w-56 shrink-0 flex-col border-r border-zinc-800 bg-zinc-950/60" aria-label="Changed files">
    <div class="flex items-center justify-between border-b border-zinc-800 px-2 py-1.5 text-[10px] text-zinc-500">
      <span>{subject.files.length} {subject.files.length === 1 ? 'file' : 'files'}</span>
      <span class="flex gap-2">
        <button type="button" class="cursor-pointer hover:text-zinc-300" onclick={() => (collapsed = new Set())}>Expand all</button>
        <button
          type="button"
          class="cursor-pointer hover:text-zinc-300"
          onclick={() => (collapsed = new Set(subject.files.map((_f, i) => i)))}>Collapse all</button
        >
      </span>
    </div>
    <ul class="min-h-0 flex-1 overflow-y-auto py-1">
      {#each subject.files as file, i (file.path)}
        {@const badge = STATUS_BADGE[file.status]}
        <li>
          <button
            type="button"
            onclick={() => jumpToFile(i)}
            aria-current={activeFile === i ? 'true' : undefined}
            title={file.path}
            class="flex w-full cursor-pointer items-center gap-1.5 px-2 py-1 text-left text-[11px] hover:bg-zinc-800/60 {activeFile === i
              ? 'bg-zinc-800/80 text-zinc-100'
              : 'text-zinc-400'}"
          >
            <span class="w-3 shrink-0 font-mono font-semibold {badge.cls}">{badge.letter}</span>
            <span class="min-w-0 flex-1 truncate">{file.path.split('/').at(-1)}</span>
            {#if commentTotals[i] > 0}
              <span class="shrink-0 rounded bg-amber-500/20 px-1 text-[10px] text-amber-300" title="Open comments">{commentTotals[i]}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  </nav>

  <div class="flex min-h-0 min-w-0 flex-1 flex-col">
    {#if subject.label}
      <div class="border-b border-zinc-800 px-3 py-1.5 text-[11px] text-zinc-500">{subject.label}</div>
    {/if}
    <VirtualList
      bind:this={list}
      items={rows}
      itemKey={(row) => row.key}
      {fixedHeight}
      {onvisible}
      label="Diff"
    >
      {#snippet row(item: DiffRow)}
        {#if item.type === 'file'}
          {@const badge = STATUS_BADGE[item.file.status]}
          <button
            type="button"
            onclick={() => toggleFile(item.fileIndex)}
            aria-expanded={!item.collapsed}
            class="sticky left-0 flex h-full w-full cursor-pointer items-center gap-2 border-y border-zinc-800 bg-zinc-900 px-2 text-left text-[11px] hover:bg-zinc-800"
          >
            {#if item.collapsed}<ChevronRight size={13} />{:else}<ChevronDown size={13} />{/if}
            <span class="font-mono font-semibold {badge.cls}">{badge.letter}</span>
            <span class="truncate font-mono text-zinc-100"
              >{item.file.oldPath && item.file.oldPath !== item.file.path ? `${item.file.oldPath} -> ` : ''}{item.file.path}</span
            >
            <span class="ml-auto flex shrink-0 gap-2 pr-1 font-mono">
              {#if item.collapsed}<span class="text-zinc-500">{item.lineCount} lines hidden</span>{/if}
              {#if item.comments > 0}<span class="text-amber-300">{item.comments} {item.comments === 1 ? 'comment' : 'comments'}</span>{/if}
              <span class="text-emerald-400">+{item.additions}</span>
              <span class="text-red-400">-{item.deletions}</span>
            </span>
          </button>
        {:else if item.type === 'hunk'}
          <div class="flex h-full items-center bg-sky-950/30 pl-[7.5rem] font-mono text-[10px] text-sky-300/80 whitespace-pre">{item.header}</div>
        {:else if item.type === 'notice'}
          <div class="flex h-full items-center pl-[7.5rem] text-[11px] text-zinc-500 italic">{item.text}</div>
        {:else if item.type === 'line'}
          {@const line = item.line}
          <div
            class="flex h-full w-full font-mono text-[11px] leading-5 {inSelection(item.fileIndex, line)
              ? 'bg-indigo-500/25'
              : line.kind === 'add'
                ? 'bg-emerald-950/50'
                : line.kind === 'del'
                  ? 'bg-red-950/50'
                  : ''}"
          >
            {#each ['old', 'new'] as const as side (side)}
              {@const n = lineOn(line, side)}
              {#if n !== undefined}
                <button
                  type="button"
                  aria-label="Comment on {side === 'old' ? 'old' : 'new'} line {n}"
                  title="Comment on line {n}. Drag or shift-click for a range."
                  onpointerdown={(e) => startSelect(e, item.fileIndex, line, side)}
                  onpointerenter={() => extendSelect(item.fileIndex, line)}
                  onclick={(e) => keyboardSelect(e, item.fileIndex, line, side)}
                  class="w-12 shrink-0 cursor-pointer pr-2 text-right text-zinc-500 select-none hover:bg-indigo-500/30 hover:text-zinc-100"
                  >{n}</button
                >
              {:else}
                <span class="w-12 shrink-0"></span>
              {/if}
            {/each}
            <span
              class="w-4 shrink-0 text-center select-none {line.kind === 'add' ? 'text-emerald-400' : line.kind === 'del' ? 'text-red-400' : 'text-zinc-700'}"
              >{line.kind === 'add' ? '+' : line.kind === 'del' ? '-' : ' '}</span
            >
            <span class="pr-4 whitespace-pre text-zinc-200">{line.text}</span>
          </div>
        {:else if item.type === 'threads'}
          <div class="bg-zinc-950/40 py-0.5">
            {#each item.comments as comment (comment.id)}
              <CommentThread {comment} {ui} {onedit} {onreopen} {ondismiss} />
            {/each}
          </div>
        {:else if item.type === 'composer'}
          <div class="bg-zinc-950/40 py-0.5">
            <CommentComposer {ui} {hint} describe={describeTarget(item.target)} onsave={saveComposer} oncancel={cancelComposer} />
          </div>
        {:else if item.type === 'orphans'}
          <div class="border-b border-zinc-800 bg-zinc-950/40 p-2">
            <p class="mb-1 text-[11px] text-zinc-500">Comments on lines that are not in these changes</p>
            {#each item.comments as comment (comment.id)}
              <p class="ml-3 mt-1 font-mono text-[10px] text-zinc-500">{comment.path}</p>
              <CommentThread {comment} {ui} {onedit} {onreopen} {ondismiss} inset="ml-3" />
            {/each}
          </div>
        {/if}
      {/snippet}
    </VirtualList>
  </div>
</div>
