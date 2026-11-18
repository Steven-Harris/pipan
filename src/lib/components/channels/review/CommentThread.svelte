<script lang="ts">
  // One comment with its status, replies and the actions the user can take on it.
  import { isTemporaryId } from '#lib/channels/review-model.ts';
  import type { ReviewComment } from '#lib/channels/types.ts';
  import { focusOnMount, type ReviewUi } from './review-ui.ts';

  let {
    comment,
    ui,
    onedit,
    onreopen,
    ondismiss,
    inset = 'ml-24'
  }: {
    comment: ReviewComment;
    ui: ReviewUi;
    onedit: (id: string, text: string) => void;
    onreopen: (id: string) => void;
    ondismiss: (id: string) => void;
    /** Left margin class that lines the thread up with the code. */
    inset?: string;
  } = $props();

  const editing = $derived(ui.editing?.id === comment.id);
  const isNew = $derived(isTemporaryId(comment.id));
  const range = $derived(comment.endLine ? `lines ${comment.line}-${comment.endLine}` : `line ${comment.line}`);

  const STATUS_CLASS: Record<ReviewComment['status'], string> = {
    open: 'bg-amber-500/15 text-amber-300',
    addressed: 'bg-emerald-500/15 text-emerald-300',
    declined: 'bg-zinc-700/50 text-zinc-300',
    dismissed: 'bg-zinc-800 text-zinc-500'
  };

  function save() {
    if (!ui.editing) return;
    onedit(comment.id, ui.editing.text);
    ui.editing = null;
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      ui.editing = null;
    } else if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      save();
    }
  }
</script>

<div
  class="my-1 {inset} mr-3 max-w-3xl rounded-md border border-zinc-700 bg-zinc-900 p-2 font-sans text-xs whitespace-normal {comment.status ===
  'dismissed'
    ? 'opacity-60'
    : ''}"
>
  <div class="mb-1 flex flex-wrap items-center gap-2 text-[10px]">
    <span class="rounded px-1.5 py-0.5 font-medium {STATUS_CLASS[comment.status]}">{comment.status}</span>
    {#if isNew}<span class="rounded bg-indigo-500/20 px-1.5 py-0.5 text-indigo-300">new</span>{/if}
    {#if comment.outdated}
      <span class="rounded bg-orange-500/20 px-1.5 py-0.5 text-orange-300" title="The code changed since this was written"
        >outdated</span
      >
    {/if}
    <span class="text-zinc-500">{range}{comment.side === 'old' ? ' (before)' : ''}</span>
  </div>

  {#if comment.outdated && comment.snippet}
    <pre class="mb-1 overflow-x-auto rounded bg-zinc-950 p-1.5 font-mono text-[10px] text-zinc-400">{comment.snippet}</pre>
  {/if}

  {#if editing && ui.editing}
    <textarea
      bind:value={ui.editing.text}
      onkeydown={keydown}
      rows="3"
      aria-label="Edit comment"
      use:focusOnMount
      class="w-full resize-y rounded border border-zinc-700 bg-zinc-950 p-1.5 text-zinc-100"
    ></textarea>
    <div class="mt-1 flex justify-end gap-1.5">
      <button type="button" onclick={() => (ui.editing = null)} class="cursor-pointer rounded border border-zinc-700 px-2 py-0.5 text-zinc-300 hover:bg-zinc-800"
        >Cancel</button
      >
      <button
        type="button"
        onclick={save}
        disabled={!ui.editing.text.trim()}
        class="cursor-pointer rounded border border-indigo-500 bg-indigo-600 px-2 py-0.5 text-white hover:bg-indigo-500 disabled:cursor-not-allowed disabled:opacity-50"
        >Save</button
      >
    </div>
  {:else}
    <p class="whitespace-pre-wrap break-words text-zinc-200">{comment.text}</p>
    {#each comment.replies ?? [] as reply, i (i)}
      <div class="mt-1.5 border-l-2 pl-2 {reply.author === 'agent' ? 'border-sky-500/60' : 'border-zinc-600'}">
        <span class="text-[10px] text-zinc-500">{reply.author === 'agent' ? 'Agent' : 'You'}</span>
        <p class="whitespace-pre-wrap break-words text-zinc-300">{reply.text}</p>
      </div>
    {/each}
    <div class="mt-1.5 flex gap-1.5 text-[11px]">
      {#if comment.status === 'open'}
        <button
          type="button"
          onclick={() => (ui.editing = { id: comment.id, text: comment.text })}
          class="cursor-pointer rounded border border-zinc-700 px-2 py-0.5 text-zinc-300 hover:bg-zinc-800">Edit</button
        >
      {/if}
      {#if comment.status !== 'open'}
        <button
          type="button"
          onclick={() => onreopen(comment.id)}
          class="cursor-pointer rounded border border-zinc-700 px-2 py-0.5 text-zinc-300 hover:bg-zinc-800">Reopen</button
        >
      {/if}
      {#if comment.status !== 'dismissed'}
        <button
          type="button"
          onclick={() => ondismiss(comment.id)}
          class="cursor-pointer rounded border border-zinc-700 px-2 py-0.5 text-zinc-400 hover:bg-zinc-800"
          >{isNew ? 'Delete' : 'Dismiss'}</button
        >
      {/if}
    </div>
  {/if}
</div>
