<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { Terminal, FolderOpen, ArrowDown, Sparkles, MessageSquareCode } from 'lucide-svelte';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import MessageItem from './MessageItem.svelte';

  let viewport = $state<HTMLDivElement | null>(null);
  let userScrolledUp = $state(false);

  function handleScroll() {
    if (!viewport) return;
    const distanceFromBottom =
      viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight;
    userScrolledUp = distanceFromBottom > 90;
  }

  function scrollToBottom(smooth = false) {
    if (!viewport) return;
    viewport.scrollTo({
      top: viewport.scrollHeight,
      behavior: smooth ? 'smooth' : 'auto'
    });
    userScrolledUp = false;
  }

  // Auto-scroll when messages or streaming blocks update, unless user scrolled up
  $effect(() => {
    const _len = chatStore.messages.length;
    const lastMsg = chatStore.messages[_len - 1];
    const _blocksLen = lastMsg?.blocks.length ?? 0;
    const lastBlock = lastMsg?.blocks[_blocksLen - 1];
    const _deltaTracker =
      lastBlock?.type === 'text'
        ? lastBlock.text.length
        : lastBlock?.type === 'thinking'
          ? lastBlock.thinking.length
          : lastBlock?.type === 'tool_call'
            ? (lastBlock.partialOutput?.length ?? 0) + lastBlock.argsJson.length
            : 0;

    if (!userScrolledUp && viewport) {
      queueMicrotask(() => {
        if (viewport && !userScrolledUp) {
          viewport.scrollTop = viewport.scrollHeight;
        }
      });
    }
  });

  async function handleSelectFolder() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Workspace Folder for Pi Session'
      });
      if (typeof selected === 'string' && selected.trim()) {
        await chatStore.startInDirectory(selected);
      }
    } catch (err) {
      chatStore.lastError = err instanceof Error ? err.message : String(err);
    }
  }
</script>

<div class="relative flex-1 overflow-hidden">
  <div
    bind:this={viewport}
    onscroll={handleScroll}
    class="h-full overflow-y-auto px-4 py-4 sm:px-6"
  >
    {#if chatStore.sessionStatus.state === 'disconnected' && chatStore.messages.length === 0}
      <!-- Disconnected Onboarding State -->
      <div class="flex h-full items-center justify-center">
        <div class="max-w-md w-full rounded-2xl border border-zinc-800/90 bg-zinc-900/50 p-6 text-center shadow-xl">
          <div class="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-2xl border border-indigo-500/30 bg-indigo-500/15 text-indigo-400">
            <MessageSquareCode class="h-6 w-6" />
          </div>
          <h2 class="text-base font-semibold text-zinc-100">
            Direct Pi RPC Chat Window
          </h2>
          <p class="mt-1.5 text-xs leading-relaxed text-zinc-400">
            Launch a local <code class="rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-zinc-300">pi --mode rpc --no-session</code> process either isolated in a temporary workspace or bound to a project directory.
          </p>

          <div class="mt-6 flex flex-col gap-2.5 sm:flex-row sm:justify-center">
            <button
              type="button"
              onclick={() => chatStore.startHeadless()}
              class="inline-flex items-center justify-center gap-2 rounded-xl bg-indigo-600 px-4 py-2.5 text-xs font-semibold text-white shadow hover:bg-indigo-500 transition cursor-pointer"
            >
              <Terminal class="h-4 w-4" />
              <span>Start Headless Chat</span>
            </button>

            <button
              type="button"
              onclick={handleSelectFolder}
              class="inline-flex items-center justify-center gap-2 rounded-xl border border-zinc-700 bg-zinc-800/90 px-4 py-2.5 text-xs font-semibold text-zinc-200 hover:bg-zinc-700/80 transition cursor-pointer"
            >
              <FolderOpen class="h-4 w-4 text-sky-400" />
              <span>Select Workspace Folder…</span>
            </button>
          </div>
        </div>
      </div>
    {:else if chatStore.messages.length === 0}
      <!-- Connected Empty Transcript State -->
      <div class="flex h-full items-center justify-center">
        <div class="max-w-lg w-full text-center space-y-4">
          <div class="inline-flex items-center gap-2 rounded-full border border-emerald-500/30 bg-emerald-500/10 px-3 py-1 text-xs text-emerald-300">
            <Sparkles class="h-3.5 w-3.5" />
            <span>
              Pi session active ({chatStore.sessionStatus.mode === 'headless'
                ? 'Headless mode'
                : chatStore.sessionStatus.cwd})
            </span>
          </div>
          <p class="text-xs text-zinc-400">
            Send a prompt below or try a quick starter:
          </p>
          <div class="flex flex-wrap justify-center gap-2">
            <button
              type="button"
              onclick={() => chatStore.sendPrompt('Say hello in one sentence and tell me your current working directory.')}
              class="rounded-lg border border-zinc-800 bg-zinc-900/80 px-3 py-1.5 text-xs text-zinc-300 hover:border-zinc-700 hover:bg-zinc-800 transition cursor-pointer"
            >
              Say hello &amp; check cwd
            </button>
            <button
              type="button"
              onclick={() => chatStore.sendPrompt('List the files in the current directory.')}
              class="rounded-lg border border-zinc-800 bg-zinc-900/80 px-3 py-1.5 text-xs text-zinc-300 hover:border-zinc-700 hover:bg-zinc-800 transition cursor-pointer"
            >
              List files in workspace
            </button>
          </div>
        </div>
      </div>
    {:else}
      <!-- Active Conversation Transcript -->
      <div class="mx-auto max-w-4xl space-y-1 pb-4">
        {#each chatStore.messages as message (message.id)}
          <MessageItem {message} />
        {/each}
      </div>
    {/if}
  </div>

  {#if userScrolledUp}
    <button
      type="button"
      onclick={() => scrollToBottom(true)}
      class="absolute bottom-4 right-6 inline-flex items-center gap-1.5 rounded-full border border-zinc-700 bg-zinc-900/95 px-3 py-1.5 text-xs font-medium text-zinc-200 shadow-lg hover:bg-zinc-800 transition cursor-pointer"
    >
      <ArrowDown class="h-3.5 w-3.5 text-indigo-400" />
      <span>Jump to bottom</span>
    </button>
  {/if}
</div>
