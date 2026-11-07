<script lang="ts">
  import { MessageSquare, Terminal, ChevronDown, ChevronUp } from 'lucide-svelte';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import MessageList from './MessageList.svelte';
  import ChatInput from './ChatInput.svelte';

  let showStderrDrawer = $state(false);

  const runningToolCount = $derived.by(() => {
    let count = 0;
    for (const msg of chatStore.messages) {
      for (const b of msg.blocks) {
        if (
          b.type === 'tool_call' &&
          (b.status === 'running' || b.status === 'streaming_args')
        ) {
          count += 1;
        }
      }
    }
    return count;
  });
</script>

<section
  aria-label="Chat"
  class="flex h-full min-h-0 min-w-0 flex-1 flex-col bg-zinc-950"
>
  <div class="flex items-center gap-2.5 border-b border-zinc-800/80 bg-zinc-900/80 px-4 py-1.5 text-xs select-none">
    <div class="flex items-center gap-1.5 font-medium text-zinc-100">
      <MessageSquare class="h-3.5 w-3.5 text-indigo-400" />
      <span>Chat</span>
    </div>

    <span class="rounded-full bg-zinc-800 px-2 py-0.5 font-mono text-[10px] text-zinc-200 tabular-nums">
      {chatStore.messages.length} {chatStore.messages.length === 1 ? 'turn' : 'turns'}
    </span>

    {#if runningToolCount > 0}
      <span class="inline-flex items-center gap-1 rounded-full bg-indigo-500/15 px-2 py-0.5 font-mono text-[10px] text-indigo-200 tabular-nums">
        <span class="h-1.5 w-1.5 rounded-full bg-indigo-400 animate-ping"></span>
        {runningToolCount} {runningToolCount === 1 ? 'tool active' : 'tools active'}
      </span>
    {/if}

    {#if chatStore.stderrLines.length > 0}
      <button
        type="button"
        onclick={() => (showStderrDrawer = !showStderrDrawer)}
        class="inline-flex items-center gap-1.5 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-0.5 text-[11px] text-amber-200 hover:bg-amber-500/20 cursor-pointer tabular-nums"
        aria-expanded={showStderrDrawer}
      >
        <Terminal class="h-3 w-3 text-amber-400" />
        <span>Diagnostics ({chatStore.stderrLines.length})</span>
        {#if showStderrDrawer}
          <ChevronUp class="h-3 w-3" />
        {:else}
          <ChevronDown class="h-3 w-3" />
        {/if}
      </button>
    {/if}
  </div>

  {#if showStderrDrawer && chatStore.stderrLines.length > 0}
    <div class="border-b border-zinc-800 bg-zinc-950 px-4 py-2 text-xs">
      <div class="mb-1.5 flex items-center justify-between text-[11px] text-zinc-300">
        <span class="font-medium">Pi Sidecar Stderr Stream</span>
        <button
          type="button"
          onclick={() => chatStore.clearStderr()}
          class="text-zinc-300 underline underline-offset-2 hover:text-zinc-100 cursor-pointer"
        >
          Clear logs
        </button>
      </div>
      <div class="max-h-28 overflow-y-auto font-mono text-[11px] text-zinc-300 space-y-1">
        {#each chatStore.stderrLines as item, idx (idx)}
          <div class="whitespace-pre-wrap break-words">{item.line}</div>
        {/each}
      </div>
    </div>
  {/if}

  <MessageList />
  <ChatInput />
</section>
