<script lang="ts">
  // The new-comment box for a line or range. The draft lives in `ui` so it survives scrolling.
  import { focusOnMount, type ReviewUi } from './review-ui.ts';

  let {
    ui,
    hint,
    describe,
    onsave,
    oncancel,
    inset = 'ml-24'
  }: {
    ui: ReviewUi;
    hint?: string;
    /** Text such as "line 12" that says what the comment is about. */
    describe: string;
    onsave: () => void;
    oncancel: () => void;
    inset?: string;
  } = $props();

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      oncancel();
    } else if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      if (ui.draft.trim()) onsave();
    }
  }
</script>

<div class="my-1 {inset} mr-3 max-w-3xl rounded-md border border-indigo-500/60 bg-zinc-900 p-2 font-sans text-xs whitespace-normal">
  <label class="mb-1 block text-[10px] text-zinc-500" for="review-composer">Comment on {describe}</label>
  <textarea
    id="review-composer"
    bind:value={ui.draft}
    onkeydown={keydown}
    rows="3"
    placeholder={hint ?? 'Write a comment'}
    use:focusOnMount
    class="w-full resize-y rounded border border-zinc-700 bg-zinc-950 p-1.5 text-zinc-100"
  ></textarea>
  <div class="mt-1 flex items-center justify-end gap-1.5">
    <span class="mr-auto text-[10px] text-zinc-600">Ctrl+Enter to add</span>
    <button type="button" onclick={oncancel} class="cursor-pointer rounded border border-zinc-700 px-2 py-0.5 text-zinc-300 hover:bg-zinc-800"
      >Cancel</button
    >
    <button
      type="button"
      onclick={onsave}
      disabled={!ui.draft.trim()}
      class="cursor-pointer rounded border border-indigo-500 bg-indigo-600 px-2 py-0.5 text-white hover:bg-indigo-500 disabled:cursor-not-allowed disabled:opacity-50"
      >Add comment</button
    >
  </div>
</div>
