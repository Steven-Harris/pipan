<script lang="ts">
  // A `review` request: optional body, the subject (diff or documents), and the choice bar.
  import {
    addComment,
    buildReviewAnswer,
    choiceEnabled,
    createReviewState,
    dismissComment,
    editComment,
    reopenComment,
    unsentCount,
    type NewComment
  } from '#lib/channels/review-model.ts';
  import type { ReviewAnswer, ReviewChoice, ReviewRequest } from '#lib/channels/types.ts';
  import Markdown from '../Markdown.svelte';
  import DiffReview from './DiffReview.svelte';
  import DocReview from './DocReview.svelte';
  import { emptyUi } from './review-ui.ts';

  let {
    review,
    onanswer,
    unsent = $bindable(0)
  }: {
    review: ReviewRequest;
    onanswer: (answer: ReviewAnswer) => void;
    /** How many changes would be lost by closing without a choice. */
    unsent?: number;
  } = $props();

  // The host remounts the view for every request, so reading the prop once is intended.
  // svelte-ignore state_referenced_locally
  let model = $state(createReviewState(review.comments));
  const ui = $state(emptyUi());
  let note = $state('');
  let noteOpen = $state(false);

  $effect(() => {
    unsent = unsentCount(model) + (ui.composer && ui.draft.trim() ? 1 : 0);
  });

  const add = (input: NewComment) => (model = addComment(model, input).state);
  const edit = (id: string, text: string) => (model = editComment(model, id, text));
  const reopen = (id: string) => (model = reopenComment(model, id));
  const dismiss = (id: string) => (model = dismissComment(model, id));

  const choiceClass = (style: ReviewChoice['style']) =>
    style === 'primary'
      ? 'border-indigo-500 bg-indigo-600 text-white hover:bg-indigo-500'
      : style === 'danger'
        ? 'border-red-600/70 bg-red-900/40 text-red-100 hover:bg-red-800/60'
        : 'border-zinc-700 bg-zinc-800 text-zinc-200 hover:bg-zinc-700';

  const blocked = $derived(review.choices.filter((c) => !choiceEnabled(c, model)));
</script>

<div class="flex min-h-0 flex-1 flex-col">
  {#if review.body}
    <details class="shrink-0 border-b border-zinc-800 px-4 py-2 text-xs text-zinc-300" open>
      <summary class="cursor-pointer text-[11px] text-zinc-500">Details</summary>
      <div class="mt-1 max-h-40 overflow-y-auto"><Markdown source={review.body} /></div>
    </details>
  {/if}

  {#if review.subject.type === 'diff'}
    <DiffReview
      subject={review.subject}
      review={model}
      {ui}
      hint={review.commentHint}
      onadd={add}
      onedit={edit}
      onreopen={reopen}
      ondismiss={dismiss}
    />
  {:else if review.subject.type === 'document'}
    <DocReview
      subject={review.subject}
      review={model}
      {ui}
      hint={review.commentHint}
      onadd={add}
      onedit={edit}
      onreopen={reopen}
      ondismiss={dismiss}
    />
  {:else}
    <p class="px-4 py-6 text-xs text-zinc-400">This version of PiPan cannot show this kind of review. Cancel to continue.</p>
  {/if}

  <div class="shrink-0 border-t border-zinc-800 px-4 py-3">
    {#if noteOpen}
      <label class="mb-1 block text-[11px] text-zinc-500" for="review-note">Note</label>
      <textarea
        id="review-note"
        bind:value={note}
        rows="2"
        class="mb-2 w-full resize-y rounded border border-zinc-700 bg-zinc-950 p-1.5 text-xs text-zinc-100"
      ></textarea>
    {/if}
    <div class="flex flex-wrap items-center gap-2">
      <button
        type="button"
        onclick={() => (noteOpen = !noteOpen)}
        aria-expanded={noteOpen}
        class="mr-auto cursor-pointer text-[11px] text-zinc-500 hover:text-zinc-300">{noteOpen ? 'Hide note' : 'Add a note'}</button
      >
      {#if blocked.length > 0}
        <span id="review-blocked" class="text-[11px] text-zinc-500">Add a comment to enable {blocked.map((c) => c.label).join(', ')}.</span>
      {/if}
      {#each review.choices as choice (choice.id)}
        {@const enabled = choiceEnabled(choice, model)}
        <button
          type="button"
          disabled={!enabled}
          aria-describedby={enabled ? undefined : 'review-blocked'}
          title={choice.description}
          onclick={() => onanswer(buildReviewAnswer(model, choice.id, note))}
          class="rounded-md border px-3 py-1.5 text-xs font-medium transition {enabled
            ? 'cursor-pointer'
            : 'cursor-not-allowed opacity-50'} {choiceClass(choice.style)}">{choice.label}</button
        >
      {/each}
    </div>
  </div>
</div>
