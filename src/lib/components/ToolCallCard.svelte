<script lang="ts">
  import {
    Wrench,
    Loader2,
    CheckCircle2,
    AlertCircle,
    ChevronDown,
    ChevronRight
  } from 'lucide-svelte';
  import type { ToolCallContentBlock } from '#lib/types/pi.ts';

  let { block }: { block: ToolCallContentBlock } = $props();

  let expanded = $state(false);

  const argSummary = $derived.by(() => {
    const args = block.args;
    if (args && typeof args === 'object') {
      const candidateKeys = ['command', 'path', 'pattern', 'query', 'url', 'file'];
      for (const key of candidateKeys) {
        if (typeof args[key] === 'string' && args[key].trim()) {
          const val = String(args[key]).trim();
          return val.length > 72 ? `${val.slice(0, 72)}…` : val;
        }
      }
      const firstStr = Object.values(args).find((v) => typeof v === 'string' && v.trim());
      if (typeof firstStr === 'string') {
        return firstStr.length > 72 ? `${firstStr.slice(0, 72)}…` : firstStr;
      }
    }
    if (block.argsJson) {
      const oneLine = block.argsJson.replace(/\s+/g, ' ').trim();
      return oneLine.length > 64 ? `${oneLine.slice(0, 64)}…` : oneLine;
    }
    return '';
  });

  const displayOutput = $derived(block.resultOutput ?? block.partialOutput ?? '');

  const durationText = $derived.by(() => {
    if (typeof block.durationMs !== 'number') return null;
    if (block.durationMs < 1000) return `${block.durationMs}ms`;
    return `${(block.durationMs / 1000).toFixed(1)}s`;
  });
</script>

<div
  class="my-1.5 overflow-hidden rounded-lg border text-xs transition {block.status === 'error'
    ? 'border-rose-500/30 bg-rose-950/20'
    : block.status === 'running' || block.status === 'streaming_args'
      ? 'border-indigo-500/30 bg-zinc-900/80'
      : 'border-zinc-800 bg-zinc-900/60'}"
>
  <button
    type="button"
    onclick={() => (expanded = !expanded)}
    class="flex w-full items-center justify-between gap-2 px-3 py-2 text-left transition hover:bg-zinc-800/40 cursor-pointer"
  >
    <div class="flex min-w-0 items-center gap-2">
      {#if block.status === 'running' || block.status === 'streaming_args'}
        <Loader2 class="h-3.5 w-3.5 shrink-0 animate-spin text-indigo-400" />
      {:else if block.status === 'error'}
        <AlertCircle class="h-3.5 w-3.5 shrink-0 text-rose-400" />
      {:else}
        <CheckCircle2 class="h-3.5 w-3.5 shrink-0 text-emerald-400" />
      {/if}

      <span class="inline-flex items-center gap-1 rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-[11px] font-semibold text-zinc-200">
        <Wrench class="h-2.5 w-2.5 text-zinc-400" />
        {block.name}
      </span>

      {#if argSummary}
        <span class="truncate font-mono text-[11px] text-zinc-400">{argSummary}</span>
      {/if}
    </div>

    <div class="flex shrink-0 items-center gap-2">
      {#if durationText}
        <span class="rounded bg-zinc-800/80 px-1.5 py-0.5 font-mono text-[10px] text-zinc-300 tabular-nums">
          {durationText}
        </span>
      {/if}
      {#if expanded}
        <ChevronDown class="h-3.5 w-3.5 text-zinc-400" />
      {:else}
        <ChevronRight class="h-3.5 w-3.5 text-zinc-400" />
      {/if}
    </div>
  </button>

  {#if expanded}
    <div class="border-t border-zinc-800/80 bg-zinc-950/60 p-3 space-y-2.5 font-mono text-[11px]">
      {#if block.argsJson}
        <div>
          <div class="mb-1 text-[11px] font-sans font-medium text-zinc-300">
            Arguments
          </div>
          <pre class="max-h-48 overflow-auto rounded bg-zinc-900 p-2 text-zinc-200 whitespace-pre-wrap break-words">{block.argsJson}</pre>
        </div>
      {/if}

      {#if displayOutput}
        <div>
          <div class="mb-1 flex items-center justify-between text-[11px] font-sans font-medium text-zinc-300">
            <span>{block.status === 'running' ? 'Live Output' : 'Output'}</span>
            {#if block.isError}
              <span class="text-rose-400 font-semibold">Error</span>
            {/if}
          </div>
          <pre
            class="max-h-72 overflow-auto rounded bg-zinc-900 p-2 whitespace-pre-wrap break-words {block.isError
              ? 'text-rose-200'
              : 'text-zinc-200'}">{displayOutput}</pre>
        </div>
      {:else if block.status === 'running' || block.status === 'streaming_args'}
        <div class="text-zinc-300 italic font-sans">Executing tool…</div>
      {/if}
    </div>
  {/if}
</div>
