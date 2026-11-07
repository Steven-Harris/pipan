<script lang="ts">
  import { Send, Compass, Clock, Square, PowerOff } from 'lucide-svelte';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import type { StreamingBehavior } from '#lib/types/pi.ts';

  let draft = $state('');
  let textareaEl = $state<HTMLTextAreaElement | null>(null);

  const isConnected = $derived(chatStore.sessionStatus.state === 'running');
  const canSend = $derived(isConnected && draft.trim().length > 0);

  function autoResize() {
    if (!textareaEl) return;
    textareaEl.style.height = 'auto';
    textareaEl.style.height = `${Math.min(textareaEl.scrollHeight, 180)}px`;
  }

  async function submitMessage(behavior?: StreamingBehavior) {
    const text = draft.trim();
    if (!text || !isConnected) return;
    draft = '';
    if (textareaEl) {
      textareaEl.style.height = 'auto';
    }
    await chatStore.sendPrompt(text, behavior);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.isComposing || e.keyCode === 229) return;
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      submitMessage(chatStore.isAgentRunning ? 'steer' : undefined);
    }
  }
</script>

<div class="border-t border-zinc-800/80 bg-zinc-900/70 px-4 py-3 backdrop-blur-md">
  <div class="mx-auto max-w-4xl space-y-2">
    <!-- Active Steering / Follow-Up Queues -->
    {#if chatStore.steeringQueue.length > 0 || chatStore.followUpQueue.length > 0}
      <div class="flex flex-wrap items-center gap-1.5 text-xs">
        {#each chatStore.steeringQueue as item, i (`steer-${i}`)}
          <span class="inline-flex items-center gap-1 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-0.5 text-amber-300">
            <Compass class="h-3 w-3" />
            <span class="font-medium">Steer:</span>
            <span class="max-w-xs truncate">{item}</span>
          </span>
        {/each}
        {#each chatStore.followUpQueue as item, i (`follow-${i}`)}
          <span class="inline-flex items-center gap-1 rounded-md border border-sky-500/30 bg-sky-500/10 px-2 py-0.5 text-sky-300">
            <Clock class="h-3 w-3" />
            <span class="font-medium">Follow-up:</span>
            <span class="max-w-xs truncate">{item}</span>
          </span>
        {/each}
      </div>
    {/if}

    <div class="flex items-end gap-2 rounded-xl border border-zinc-800 bg-zinc-950/90 p-2 focus-within:border-indigo-500/50 transition">
      <textarea
        bind:this={textareaEl}
        bind:value={draft}
        oninput={autoResize}
        onkeydown={handleKeydown}
        disabled={!isConnected}
        rows="1"
        placeholder={!isConnected
          ? 'Start a Headless or Folder session above to begin chatting…'
          : chatStore.isAgentRunning
            ? 'Type to steer Pi mid-run (Enter to Steer, or click Queue Follow-up)…'
            : 'Message Pi… (Enter to send, Shift+Enter for newline)'}
        class="max-h-44 min-h-[36px] flex-1 resize-none bg-transparent px-2 py-1.5 text-sm text-zinc-100 placeholder-zinc-400 focus:outline-none disabled:cursor-not-allowed disabled:opacity-50"
      ></textarea>

      <div class="flex shrink-0 items-center gap-1.5">
        {#if !chatStore.isAgentRunning}
          <button
            type="button"
            onclick={() => submitMessage()}
            disabled={!canSend}
            class="inline-flex items-center gap-1.5 rounded-lg bg-indigo-600 px-3 py-2 text-xs font-semibold text-white shadow transition hover:bg-indigo-500 disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer"
          >
            <Send class="h-3.5 w-3.5" />
            <span>Send</span>
          </button>
        {:else}
          <!-- Streaming state controls: Steer, Follow-up, Stop / Abort -->
          <button
            type="button"
            onclick={() => submitMessage('steer')}
            disabled={!canSend}
            class="inline-flex items-center gap-1 rounded-lg border border-amber-500/40 bg-amber-500/15 px-2.5 py-2 text-xs font-medium text-amber-200 transition hover:bg-amber-500/25 disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer"
            title="Interrupt after current tool call and steer Pi immediately"
          >
            <Compass class="h-3.5 w-3.5" />
            <span>Steer</span>
          </button>

          <button
            type="button"
            onclick={() => submitMessage('followUp')}
            disabled={!canSend}
            class="inline-flex items-center gap-1 rounded-lg border border-sky-500/40 bg-sky-500/15 px-2.5 py-2 text-xs font-medium text-sky-200 transition hover:bg-sky-500/25 disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer"
            title="Queue message to run after Pi finishes the current turn"
          >
            <Clock class="h-3.5 w-3.5" />
            <span class="hidden sm:inline">Follow-up</span>
          </button>

          <button
            type="button"
            onclick={() => chatStore.abort()}
            class="inline-flex items-center gap-1 rounded-lg border border-rose-500/40 bg-rose-600/20 px-2.5 py-2 text-xs font-semibold text-rose-200 transition hover:bg-rose-600/35 cursor-pointer"
            title="Clear queues and abort current generation (clear_queue + abort)"
          >
            <Square class="h-3.5 w-3.5 fill-current" />
            <span>Stop</span>
          </button>

          <button
            type="button"
            onclick={() => chatStore.stopSession()}
            class="inline-flex items-center gap-1 rounded-lg border border-zinc-700 bg-zinc-800/80 px-2 py-2 text-xs font-medium text-zinc-300 transition hover:border-rose-500/40 hover:bg-rose-500/15 hover:text-rose-200 cursor-pointer"
            title="Terminate Pi RPC child process (stop_pi_session)"
            aria-label="Stop Process"
          >
            <PowerOff class="h-3.5 w-3.5" />
            <span class="hidden md:inline">Stop Process</span>
          </button>
        {/if}
      </div>
    </div>
  </div>
</div>
