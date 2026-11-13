<script lang="ts">
  // Document review: markdown (or plain text) shown as blocks with line numbers. A comment
  // anchors to the source lines of the block it was written on.
  import {
    buildDocRows,
    inRange,
    linesText,
    parseDocument,
    selectBlock,
    type DocComposer,
    type ParsedDocument
  } from '#lib/channels/doc-blocks.ts';
  import { clipSnippet, type NewComment, type ReviewState } from '#lib/channels/review-model.ts';
  import type { DocumentSubject } from '#lib/channels/types.ts';
  import CommentComposer from './CommentComposer.svelte';
  import CommentThread from './CommentThread.svelte';
  import Markdown from '../Markdown.svelte';
  import type { ReviewUi } from './review-ui.ts';

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
    subject: DocumentSubject;
    review: ReviewState;
    ui: ReviewUi;
    hint?: string;
    onadd: (input: NewComment) => void;
    onedit: (id: string, text: string) => void;
    onreopen: (id: string) => void;
    ondismiss: (id: string) => void;
  } = $props();

  let active = $state(0);

  // Parsed once per request. The host remounts the view for every request.
  // svelte-ignore state_referenced_locally
  const parsed: ParsedDocument[] = subject.documents.map((d) => parseDocument(d));

  const doc = $derived(subject.documents[active]);
  const current = $derived(parsed[active]);

  /** Comments from every round that belong to a document that is not in this request. */
  const unknownPaths = $derived(review.comments.filter((c) => !subject.documents.some((d) => d.path === c.path)));
  const forDoc = $derived(review.comments.filter((c) => c.path === doc.path));
  const built = $derived(buildDocRows(current, forDoc, ui.composer?.fileIndex === active ? ui.composer : null));
  const openCounts = $derived(
    subject.documents.map((d) => review.comments.filter((c) => c.path === d.path && c.status === 'open').length)
  );

  const selection = $derived<DocComposer | null>(ui.composer?.fileIndex === active ? ui.composer : null);

  function select(index: number, event: MouseEvent) {
    const block = current.blocks[index];
    const next = selectBlock(block, selection, event.shiftKey);
    ui.composer = { fileIndex: active, path: doc.path, side: 'new', ...next };
  }

  function cancelComposer() {
    ui.composer = null;
    ui.draft = '';
  }

  function saveComposer() {
    const target = ui.composer;
    if (!target) return;
    onadd({
      path: target.path,
      side: 'new',
      line: target.line,
      endLine: target.endLine,
      snippet: clipSnippet(linesText(current, target.line, target.endLine ?? target.line)),
      text: ui.draft
    });
    ui.composer = null;
    ui.draft = '';
  }

  const label = (line: number, endLine?: number) => (endLine && endLine > line ? `${line}-${endLine}` : `${line}`);
</script>

<div class="flex min-h-0 flex-1 flex-col">
  {#if subject.documents.length > 1}
    <div role="tablist" aria-label="Documents" class="flex shrink-0 gap-1 overflow-x-auto border-b border-zinc-800 px-2 pt-1.5">
      {#each subject.documents as d, i (d.path)}
        <button
          type="button"
          role="tab"
          aria-selected={i === active}
          onclick={() => (active = i)}
          title={d.path}
          class="flex cursor-pointer items-center gap-1.5 rounded-t-md border border-b-0 px-3 py-1 text-[11px] {i === active
            ? 'border-zinc-700 bg-zinc-900 text-zinc-100'
            : 'border-transparent text-zinc-500 hover:text-zinc-300'}"
        >
          {d.path.split('/').at(-1)}
          {#if openCounts[i] > 0}
            <span class="rounded bg-amber-500/20 px-1 text-[10px] text-amber-300" title="Open comments">{openCounts[i]}</span>
          {/if}
        </button>
      {/each}
    </div>
  {/if}

  <div class="min-h-0 flex-1 overflow-y-auto" role="tabpanel" aria-label={doc.path}>
    <div class="sticky top-0 z-10 border-b border-zinc-800 bg-zinc-900/95 px-3 py-1.5 font-mono text-[11px] text-zinc-400">
      {doc.path}
      <span class="ml-2 font-sans text-zinc-600">Click a line number to comment. Shift-click to extend.</span>
    </div>

    {#if unknownPaths.length > 0 || built.orphans.length > 0}
      <div class="border-b border-zinc-800 bg-zinc-950/40 p-2">
        <p class="mb-1 text-[11px] text-zinc-500">Comments on lines that are not in these documents</p>
        {#each [...unknownPaths, ...built.orphans] as comment (comment.id)}
          <p class="ml-3 mt-1 font-mono text-[10px] text-zinc-500">{comment.path}</p>
          <CommentThread {comment} {ui} {onedit} {onreopen} {ondismiss} inset="ml-3" />
        {/each}
      </div>
    {/if}

    {#if current.blocks.length === 0}
      <p class="px-4 py-6 text-xs text-zinc-500">This document is empty.</p>
    {/if}

    {#each built.rows as row (row.key)}
      {#if row.type === 'block'}
        {@const selected = inRange(row.block, selection)}
        <div class="flex text-xs [content-visibility:auto] [contain-intrinsic-size:auto_2rem] {selected ? 'bg-indigo-500/20' : ''}">
          <button
            type="button"
            aria-label="Comment on lines {label(row.block.startLine, row.block.endLine)}"
            title="Comment on lines {label(row.block.startLine, row.block.endLine)}. Shift-click to extend the selection."
            onclick={(e) => select(row.index, e)}
            class="w-14 shrink-0 cursor-pointer self-stretch px-2 pt-1 text-right font-mono text-[11px] text-zinc-500 select-none hover:bg-indigo-500/30 hover:text-zinc-100"
            >{label(row.block.startLine, row.block.endLine)}</button
          >
          <div class="min-w-0 flex-1 px-3 py-1 text-zinc-300">
            {#if row.block.html !== undefined}
              <Markdown html={row.block.html} />
            {:else}
              <pre class="font-mono text-[11px] whitespace-pre-wrap">{row.block.text}</pre>
            {/if}
          </div>
        </div>
      {:else if row.type === 'threads'}
        <div class="bg-zinc-950/40 py-0.5">
          {#each row.comments as comment (comment.id)}
            <CommentThread {comment} {ui} {onedit} {onreopen} {ondismiss} inset="ml-14" />
          {/each}
        </div>
      {:else}
        <div class="bg-zinc-950/40 py-0.5">
          <CommentComposer
            {ui}
            {hint}
            describe="{row.target.endLine ? 'lines' : 'line'} {label(row.target.line, row.target.endLine)} of {doc.path}"
            onsave={saveComposer}
            oncancel={cancelComposer}
            inset="ml-14"
          />
        </div>
      {/if}
    {/each}
  </div>
</div>
