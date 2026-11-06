<script lang="ts">
  import { Sparkles, ChevronDown, ChevronRight } from 'lucide-svelte';
  import type { ThinkingContentBlock } from '$lib/types/pi';

  let { block }: { block: ThinkingContentBlock } = $props();

  let isOpen = $state(false);

  const previewText = $derived.by(() => {
    const clean = block.thinking.replace(/\s+/g, ' ').trim();
    if (!clean) return block.isComplete ? 'Thought process' : 'Thinking…';
    return clean.length > 90 ? `${clean.slice(0, 90)}…` : clean;
  });
</script>

<div class="my-1.5 rounded-lg border border-zinc-800/80 bg-zinc-900/50 text-xs">
  <button
    type="button"
    onclick={() => (isOpen = !isOpen)}
    class="flex w-full items-center justify-between gap-2 px-3 py-2 text-left text-zinc-400 transition hover:text-zinc-200 cursor-pointer"
  >
    <div class="flex min-w-0 items-center gap-2">
      <Sparkles
        class="h-3.5 w-3.5 shrink-0 {block.isComplete
          ? 'text-indigo-400/70'
          : 'text-indigo-400 animate-pulse'}"
      />
      <span class="font-medium text-zinc-300 shrink-0">
        {block.isComplete ? 'Reasoning' : 'Thinking'}
      </span>
      {#if !isOpen}
        <span class="truncate text-zinc-500 italic">{previewText}</span>
      {/if}
    </div>

    <div class="flex shrink-0 items-center gap-1.5">
      {#if !block.isComplete}
        <span class="h-1.5 w-1.5 rounded-full bg-indigo-400 animate-ping"></span>
      {/if}
      {#if isOpen}
        <ChevronDown class="h-3.5 w-3.5 text-zinc-500" />
      {:else}
        <ChevronRight class="h-3.5 w-3.5 text-zinc-500" />
      {/if}
    </div>
  </button>

  {#if isOpen}
    <div class="border-t border-zinc-800/70 px-3 py-2.5 font-mono text-[11px] leading-relaxed text-zinc-400 whitespace-pre-wrap break-words max-h-80 overflow-y-auto">
      {block.thinking || '…'}
    </div>
  {/if}
</div>
