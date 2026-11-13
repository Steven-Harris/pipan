<script lang="ts">
  // Modal for the oldest pending request of a session. Closing it by any route other than an
  // answer (Esc, the Cancel button) cancels the request.
  import { onMount } from 'svelte';
  import type { FormAnswer, FormRequest, ReviewAnswer, ReviewRequest } from '#lib/channels/types.ts';
  import { channels, type PendingRequest } from '#lib/stores/channels.svelte.ts';
  import { channelPlugins } from '#lib/channels/plugin-api.svelte.ts';
  import CustomRequestView from './CustomRequestView.svelte';
  import FormView from './FormView.svelte';
  import ReviewView from './review/ReviewView.svelte';

  let { request, waiting }: { request: PendingRequest; waiting: number } = $props();

  let dialog: HTMLDialogElement;
  // A plugin's renderer for this channel and kind replaces the built-in view.
  const custom = $derived(channelPlugins.requestHandler(request.ch, request.kind));
  const form = $derived(!custom && request.kind === 'form' ? (request.payload as FormRequest) : null);
  const review = $derived(!custom && request.kind === 'review' ? (request.payload as ReviewRequest) : null);
  const payloadTitle = $derived(
    typeof (request.payload as { title?: unknown } | null)?.title === 'string'
      ? (request.payload as { title: string }).title
      : request.kind
  );
  const title = $derived(form?.title ?? review?.title ?? (custom ? payloadTitle : 'Unsupported request'));
  let unsent = $state(0);
  let confirmDiscard = $state(false);
  const titleId = $derived(`request-title-${request.requestId}`);

  onMount(() => {
    dialog.showModal();
    // The view marks the control that should receive focus first.
    dialog.querySelector<HTMLElement>('[data-autofocus]')?.focus();
  });

  async function answer(value: FormAnswer | ReviewAnswer) {
    await channels.answer(request.sessionId, request.requestId, value);
  }

  function cancel() {
    void channels.cancel(request.sessionId, request.requestId);
  }

  /** Cancel asks first when the user has comments that would be lost. */
  function requestCancel() {
    if (unsent > 0) confirmDiscard = true;
    else cancel();
  }

  function oncancel(event: Event) {
    if (unsent > 0 && !confirmDiscard) {
      event.preventDefault();
      confirmDiscard = true;
    }
  }

  // Fires for Esc and the back gesture. An answer removes the request first, so the dialog is
  // gone before this could run for it.
  function onclose() {
    if ((channels.pending[request.sessionId] ?? []).some((r) => r.requestId === request.requestId)) cancel();
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby={titleId}
  {onclose}
  {oncancel}
  class="m-auto flex max-h-[85vh] {review ? 'h-[85vh] w-[80rem]' : 'w-[34rem]'} max-w-[calc(100vw-2rem)] flex-col overflow-hidden rounded-xl border border-zinc-700 bg-zinc-900 p-0 text-zinc-100 shadow-2xl backdrop:bg-black/60 open:flex"
>
  <header class="flex shrink-0 items-start justify-between gap-3 border-b border-zinc-800 px-4 py-3">
    <div class="min-w-0">
      <h2 id={titleId} class="truncate text-sm font-semibold">{title}</h2>
      <p class="text-[11px] text-zinc-500">
        Requested by <code class="rounded bg-zinc-800 px-1 text-zinc-300">{request.ch}</code>
        {#if waiting > 0}<span class="ml-1">- {waiting} more waiting</span>{/if}
      </p>
    </div>
    <button
      type="button"
      onclick={requestCancel}
      class="shrink-0 rounded-md border border-zinc-700 bg-zinc-800 px-2 py-1 text-[11px] text-zinc-200 hover:bg-zinc-700 cursor-pointer"
      >Cancel</button
    >
  </header>

  {#if confirmDiscard}
    <div role="alert" class="flex shrink-0 items-center gap-3 border-b border-red-900/60 bg-red-950/40 px-4 py-2 text-xs text-red-100">
      <span class="mr-auto">Discard {unsent} unsent {unsent === 1 ? 'change' : 'changes'} and cancel the review?</span>
      <button type="button" onclick={() => (confirmDiscard = false)} class="cursor-pointer rounded border border-zinc-600 px-2 py-0.5 hover:bg-zinc-800"
        >Keep reviewing</button
      >
      <button type="button" onclick={cancel} class="cursor-pointer rounded border border-red-600 bg-red-900/60 px-2 py-0.5 hover:bg-red-800">Discard</button>
    </div>
  {/if}

  {#if form}
    <FormView {form} onanswer={answer} />
  {:else if custom}
    <CustomRequestView {request} handler={custom} />
  {:else if review}
    <ReviewView {review} onanswer={answer} bind:unsent />
  {:else}
    <div class="space-y-2 px-4 py-4 text-xs text-zinc-300">
      <p>
        This version of PiPan cannot show a <code class="rounded bg-zinc-800 px-1">{request.kind}</code> request. Cancel
        it so the extension can continue.
      </p>
    </div>
  {/if}
</dialog>

<style>
  dialog:modal {
    overscroll-behavior: contain;
  }
  dialog::backdrop {
    overflow: hidden;
    overscroll-behavior: contain;
  }
</style>
