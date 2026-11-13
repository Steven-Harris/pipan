<script lang="ts">
  import { User, Bot, AlertTriangle, Puzzle } from 'lucide-svelte';
  import Markdown from './channels/Markdown.svelte';
  import type { ChatMessage } from '#lib/types/pi.ts';
  import ThinkingBlock from './ThinkingBlock.svelte';
  import ToolCallCard from './ToolCallCard.svelte';

  let { message }: { message: ChatMessage } = $props();

  const formattedTime = $derived(
    new Date(message.timestamp).toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit'
    })
  );

  // Split text into alternating prose and ```code``` segments for clean code block rendering
  function parseTextSegments(
    raw: string
  ): Array<{ type: 'prose' | 'code'; lang?: string; content: string }> {
    const segments: Array<{ type: 'prose' | 'code'; lang?: string; content: string }> = [];
    const fenceRegex = /```([a-zA-Z0-9_-]*)\r?\n?([\s\S]*?)```/g;
    let lastIdx = 0;
    let match: RegExpExecArray | null;

    while ((match = fenceRegex.exec(raw)) !== null) {
      if (match.index > lastIdx) {
        segments.push({
          type: 'prose',
          content: raw.slice(lastIdx, match.index)
        });
      }
      segments.push({
        type: 'code',
        lang: match[1] || undefined,
        content: match[2]
      });
      lastIdx = fenceRegex.lastIndex;
    }

    if (lastIdx < raw.length) {
      segments.push({
        type: 'prose',
        content: raw.slice(lastIdx)
      });
    }

    return segments.length > 0 ? segments : [{ type: 'prose', content: raw }];
  }
</script>

{#if message.role === 'user'}
  <div class="flex justify-end py-2">
    <div class="max-w-2xl rounded-2xl rounded-tr-sm border border-indigo-500/30 bg-indigo-600/20 px-4 py-2.5 text-sm text-zinc-100 shadow-sm">
      <div class="mb-1 flex items-center justify-end gap-1.5 text-[11px] text-indigo-300/80">
        <span>You</span>
        <span>·</span>
        <span>{formattedTime}</span>
        {#if message.isOptimistic}
          <span class="italic text-indigo-300/60">(sending…)</span>
        {/if}
        <User class="h-3 w-3" />
      </div>
      {#each message.blocks as block, idx (idx)}
        {#if block.type === 'text'}
          <div class="whitespace-pre-wrap break-words leading-relaxed">{block.text}</div>
        {/if}
      {/each}
    </div>
  </div>
{:else if message.role === 'custom'}
  <div class="flex justify-start py-2">
    <div class="w-full max-w-4xl rounded-xl border border-zinc-800/90 bg-zinc-900/30 px-4 py-2.5 text-sm text-zinc-200">
      <div class="mb-1 flex items-center gap-1.5 text-[11px] text-zinc-500">
        <Puzzle class="h-3 w-3" />
        <span>{message.customType}</span>
        <span>·</span>
        <span>{formattedTime}</span>
      </div>
      {#each message.blocks as block, idx (idx)}
        {#if block.type === 'text'}
          <Markdown source={block.text} />
        {/if}
      {/each}
    </div>
  </div>
{:else}
  <div class="flex justify-start py-2">
    <div class="w-full max-w-4xl rounded-2xl rounded-tl-sm border border-zinc-800/90 bg-zinc-900/40 px-4 py-3 text-sm text-zinc-100">
      <div class="mb-2 flex items-center justify-between gap-2 text-[11px] text-zinc-400">
        <div class="flex items-center gap-1.5">
          <div class="flex h-5 w-5 items-center justify-center rounded-md bg-zinc-800 text-indigo-400">
            <Bot class="h-3.5 w-3.5" />
          </div>
          <span class="font-medium text-zinc-300">Pi Assistant</span>
          <span>·</span>
          <span>{formattedTime}</span>
          {#if message.isStreaming}
            <span class="inline-flex items-center gap-1 rounded-full bg-indigo-500/15 px-2 py-0.5 text-[10px] font-medium text-indigo-300">
              <span class="h-1.5 w-1.5 rounded-full bg-indigo-400 animate-pulse"></span>
              streaming
            </span>
          {/if}
        </div>

        {#if message.usage?.totalTokens}
          <span class="font-mono text-[10px] text-zinc-400 tabular-nums">
            {message.usage.totalTokens.toLocaleString()} tok
          </span>
        {/if}
      </div>

      {#if message.blocks.length === 0 && message.isStreaming}
        <div class="flex items-center gap-2 py-2 text-xs text-zinc-400">
          <span class="h-2 w-2 rounded-full bg-indigo-400 animate-ping"></span>
          <span>Pi is thinking…</span>
        </div>
      {/if}

      <div class="space-y-2">
        {#each message.blocks as block, idx (idx)}
          {#if block.type === 'thinking'}
            <ThinkingBlock {block} />
          {:else if block.type === 'tool_call'}
            <ToolCallCard {block} />
          {:else if block.type === 'text'}
            <div class="space-y-2 leading-relaxed text-zinc-200">
              {#each parseTextSegments(block.text) as seg, sIdx (sIdx)}
                {#if seg.type === 'code'}
                  <div class="my-2 overflow-hidden rounded-lg border border-zinc-800 bg-zinc-950">
                    {#if seg.lang}
                      <div class="border-b border-zinc-800/80 bg-zinc-900/70 px-3 py-1 font-mono text-[10px] text-zinc-400">
                        {seg.lang}
                      </div>
                    {/if}
                    <pre class="overflow-x-auto p-3 font-mono text-xs text-zinc-200">{seg.content}</pre>
                  </div>
                {:else}
                  <div class="whitespace-pre-wrap break-words">{seg.content}</div>
                {/if}
              {/each}
            </div>
          {/if}
        {/each}
      </div>

      {#if message.errorMessage}
        <div class="mt-2.5 flex items-start gap-2 rounded-lg border border-rose-500/30 bg-rose-950/30 p-2.5 text-xs text-rose-300">
          <AlertTriangle class="h-4 w-4 shrink-0 text-rose-400 mt-0.5" />
          <span class="whitespace-pre-wrap break-words">{message.errorMessage}</span>
        </div>
      {/if}
    </div>
  </div>
{/if}
