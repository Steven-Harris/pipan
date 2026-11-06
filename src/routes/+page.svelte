<script lang="ts">
  import { onMount } from 'svelte';
  import { AlertCircle, Terminal, X, ChevronDown, ChevronUp } from 'lucide-svelte';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import SessionHeader from '#lib/components/SessionHeader.svelte';
  import MessageList from '#lib/components/MessageList.svelte';
  import ChatInput from '#lib/components/ChatInput.svelte';

  let showStderrDrawer = $state(false);

  onMount(() => {
    chatStore.init();
  });
</script>

<div class="flex h-screen flex-col overflow-hidden bg-zinc-950 text-zinc-100">
  <SessionHeader />

  <!-- Error Banner -->
  {#if chatStore.lastError}
    <div class="flex items-center justify-between gap-3 border-b border-rose-500/30 bg-rose-950/50 px-4 py-2 text-xs text-rose-200">
      <div class="flex min-w-0 items-center gap-2">
        <AlertCircle class="h-4 w-4 shrink-0 text-rose-400" />
        <span class="truncate">{chatStore.lastError}</span>
      </div>
      <button
        type="button"
        onclick={() => chatStore.dismissError()}
        class="rounded p-1 text-rose-300 hover:bg-rose-900/50 cursor-pointer"
        title="Dismiss error"
      >
        <X class="h-3.5 w-3.5" />
      </button>
    </div>
  {/if}

  <!-- Collapsible Stderr Diagnostic Bar -->
  {#if chatStore.stderrLines.length > 0}
    <div class="border-b border-zinc-800 bg-zinc-900/90 text-xs">
      <div class="flex items-center justify-between px-4 py-1.5 text-zinc-400">
        <button
          type="button"
          onclick={() => (showStderrDrawer = !showStderrDrawer)}
          class="flex items-center gap-2 hover:text-zinc-200 cursor-pointer"
        >
          <Terminal class="h-3.5 w-3.5 text-amber-400" />
          <span>Pi Diagnostics ({chatStore.stderrLines.length})</span>
          {#if showStderrDrawer}
            <ChevronUp class="h-3.5 w-3.5" />
          {:else}
            <ChevronDown class="h-3.5 w-3.5" />
          {/if}
        </button>
        <button
          type="button"
          onclick={() => chatStore.clearStderr()}
          class="text-[11px] text-zinc-500 hover:text-zinc-300 cursor-pointer"
        >
          Clear
        </button>
      </div>
      {#if showStderrDrawer}
        <div class="max-h-36 overflow-y-auto border-t border-zinc-800/80 bg-zinc-950 px-4 py-2 font-mono text-[11px] text-zinc-400 space-y-1">
          {#each chatStore.stderrLines as item, idx (idx)}
            <div class="whitespace-pre-wrap break-words">{item.line}</div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <MessageList />

  <ChatInput />
</div>
