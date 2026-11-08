<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import {
    SquareTerminal,
    FolderOpen,
    Trash2,
    OctagonX,
    RotateCcw,
    Send,
    Sparkles,
    CornerDownLeft,
    Loader2,
    PowerOff
  } from 'lucide-svelte';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import { terminalStore } from '#lib/stores/terminal.svelte.ts';

  let commandInput = $state('');
  let historyCursor = $state<number>(-1);
  let savedDraftBeforeHistory = $state<string>('');
  let outputViewportEl = $state<HTMLDivElement | null>(null);
  let inputEl = $state<HTMLInputElement | null>(null);

  const activeSessionId = $derived(chatStore.activeSessionId ?? 'default');
  const activeCwd = $derived(chatStore.effectiveWorkspaceCwd);
  const sessionView = $derived(terminalStore.getView(activeSessionId));

  const displayCwd = $derived.by(() => {
    const raw = sessionView.info?.cwd ?? activeCwd;
    if (!raw) return 'Headless Sandbox';
    return raw.replace(/^\\\\\?\\/, '');
  });

  // Automatically start or sync the shell session when activeSessionId or activeCwd changes
  $effect(() => {
    const sid = activeSessionId;
    const cwd = activeCwd;
    untrack(() => {
      void terminalStore.ensureSession(sid, cwd);
    });
  });

  // Auto-scroll terminal viewport when new output items arrive
  $effect(() => {
    const count = sessionView.items.length;
    if (count > 0 && outputViewportEl) {
      void tick().then(() => {
        if (outputViewportEl) {
          outputViewportEl.scrollTop = outputViewportEl.scrollHeight;
        }
      });
    }
  });

  onMount(() => {
    void terminalStore.init();

    let resizeObserver: ResizeObserver | undefined;
    if (typeof ResizeObserver !== 'undefined' && outputViewportEl) {
      resizeObserver = new ResizeObserver((entries) => {
        const rect = entries[0]?.contentRect;
        if (!rect || rect.width <= 0 || rect.height <= 0) return;
        const approxCols = Math.max(20, Math.floor(rect.width / 7.5));
        const approxRows = Math.max(5, Math.floor(rect.height / 18));
        void terminalStore.resize(activeSessionId, approxCols, approxRows);
      });
      resizeObserver.observe(outputViewportEl);
    }

    return () => {
      resizeObserver?.disconnect();
    };
  });

  async function handleRunCommand() {
    const trimmed = commandInput.trim();
    if (!trimmed) return;
    commandInput = '';
    historyCursor = -1;
    savedDraftBeforeHistory = '';
    await terminalStore.executeCommand(activeSessionId, trimmed, activeCwd);
    inputEl?.focus();
  }

  function handleInputKeydown(e: KeyboardEvent) {
    if (e.isComposing || e.keyCode === 229) return;

    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void handleRunCommand();
      return;
    }

    if (e.key === 'c' && e.ctrlKey && !commandInput) {
      e.preventDefault();
      void terminalStore.sendInterrupt(activeSessionId);
      return;
    }

    if (e.key === 'l' && e.ctrlKey) {
      e.preventDefault();
      terminalStore.clearOutput(activeSessionId);
      return;
    }

    const history = sessionView.history;
    if (e.key === 'ArrowUp') {
      if (history.length === 0) return;
      e.preventDefault();
      if (historyCursor === -1) {
        savedDraftBeforeHistory = commandInput;
        historyCursor = history.length - 1;
      } else if (historyCursor > 0) {
        historyCursor -= 1;
      }
      commandInput = history[historyCursor] ?? '';
      return;
    }

    if (e.key === 'ArrowDown') {
      if (historyCursor === -1) return;
      e.preventDefault();
      if (historyCursor < history.length - 1) {
        historyCursor += 1;
        commandInput = history[historyCursor] ?? '';
      } else {
        historyCursor = -1;
        commandInput = savedDraftBeforeHistory;
      }
    }
  }
</script>

<div class="flex h-full min-h-0 flex-col bg-zinc-950 text-zinc-100">
  <!-- Terminal Header Toolbar -->
  <div class="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-zinc-800/80 bg-zinc-900/60 px-3 py-2">
    <div class="flex min-w-0 items-center gap-2">
      <SquareTerminal class="h-4 w-4 shrink-0 text-indigo-400" />
      <span
        class="inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[11px] font-medium {sessionView
          .info?.running
          ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-300'
          : sessionView.isStarting
            ? 'border-amber-500/30 bg-amber-500/10 text-amber-300'
            : 'border-zinc-700 bg-zinc-800 text-zinc-400'}"
      >
        <span
          class="h-1.5 w-1.5 rounded-full {sessionView.info?.running
            ? 'bg-emerald-400'
            : sessionView.isStarting
              ? 'bg-amber-400 animate-pulse'
              : 'bg-zinc-500'}"
        ></span>
        <span>
          {sessionView.info?.shell ?? 'shell'}
          {#if sessionView.info?.pid}
            <span class="font-mono text-[10px] opacity-75">({sessionView.info.pid})</span>
          {/if}
        </span>
      </span>

      <!-- Working Directory Indicator -->
      <div
        class="inline-flex min-w-0 max-w-xs items-center gap-1.5 rounded border border-zinc-800 bg-zinc-900/90 px-2 py-0.5 text-xs text-zinc-300"
        title={displayCwd}
      >
        <FolderOpen class="h-3 w-3 shrink-0 text-sky-400" />
        <span class="truncate font-mono text-[11px]">{displayCwd}</span>
      </div>
    </div>

    <!-- Action Controls: Send to Pi (bash RPC) Toggle, Interrupt (Ctrl+C), Clear, Restart/Stop -->
    <div class="flex flex-wrap items-center gap-1.5">
      <button
        type="button"
        role="switch"
        aria-checked={sessionView.sendToPiBash}
        onclick={() => terminalStore.toggleSendToPiBash(activeSessionId)}
        class="inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1 text-xs font-medium transition cursor-pointer {sessionView.sendToPiBash
          ? 'border-indigo-500/50 bg-indigo-500/20 text-indigo-100'
          : 'border-zinc-800 bg-zinc-900 text-zinc-400 hover:border-zinc-700 hover:text-zinc-200'}"
        title="Execute commands via Pi's `bash` RPC command so output is automatically added to the LLM conversation context"
      >
        <Sparkles class="h-3 w-3 {sessionView.sendToPiBash ? 'text-indigo-300' : 'text-zinc-500'}" />
        <span>Send to Pi (`bash` RPC)</span>
      </button>

      <button
        type="button"
        onclick={() => void terminalStore.sendInterrupt(activeSessionId)}
        disabled={!sessionView.info?.running && !sessionView.isRunningPiBash}
        class="inline-flex items-center gap-1 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-1 text-xs font-medium text-amber-200 transition hover:bg-amber-500/20 disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer"
        title="Interrupt running command (Ctrl+C)"
        aria-label="Interrupt running command (Ctrl+C)"
      >
        <OctagonX class="h-3 w-3" />
        <span>Ctrl+C</span>
      </button>

      <button
        type="button"
        onclick={() => terminalStore.clearOutput(activeSessionId)}
        class="inline-flex items-center gap-1 rounded-md border border-zinc-800 bg-zinc-900 px-2 py-1 text-xs text-zinc-300 transition hover:border-zinc-700 hover:bg-zinc-800 cursor-pointer"
        title="Clear terminal output (Ctrl+L)"
        aria-label="Clear terminal output"
      >
        <Trash2 class="h-3 w-3" />
        <span class="hidden sm:inline">Clear</span>
      </button>

      <button
        type="button"
        onclick={() => void terminalStore.startSession(activeSessionId, activeCwd)}
        disabled={sessionView.isStarting}
        class="inline-flex items-center gap-1 rounded-md border border-zinc-800 bg-zinc-900 px-2 py-1 text-xs text-zinc-300 transition hover:border-zinc-700 hover:bg-zinc-800 disabled:opacity-40 cursor-pointer"
        title="Restart shell session in current workspace"
        aria-label="Restart shell session"
      >
        <RotateCcw class="h-3 w-3 {sessionView.isStarting ? 'animate-spin' : ''}" />
      </button>

      {#if sessionView.info?.running}
        <button
          type="button"
          onclick={() => void terminalStore.stopSession(activeSessionId)}
          class="inline-flex items-center gap-1 rounded-md border border-rose-500/30 bg-rose-500/10 px-2 py-1 text-xs text-rose-300 transition hover:bg-rose-500/20 cursor-pointer"
          title="Kill shell session process"
          aria-label="Kill shell session process"
        >
          <PowerOff class="h-3 w-3" />
        </button>
      {/if}
    </div>
  </div>

  {#if sessionView.error}
    <div class="mx-3 mt-2 rounded-lg border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-xs text-rose-200">
      {sessionView.error}
    </div>
  {/if}

  <!-- Streaming ANSI/Text Output Viewport -->
  <div
    bind:this={outputViewportEl}
    role="log"
    aria-live="polite"
    aria-label="Terminal output"
    class="min-h-0 flex-1 overflow-y-auto px-3 py-2.5 font-mono text-xs leading-relaxed select-text"
  >
    {#if sessionView.items.length === 0}
      <div class="flex h-full flex-col items-center justify-center text-center text-zinc-500">
        <SquareTerminal class="mb-2 h-6 w-6 text-zinc-700" />
        <p class="text-xs text-zinc-400">
          Interactive shell ready in <span class="text-zinc-200">{displayCwd}</span>
        </p>
        <p class="mt-1 text-[11px] text-zinc-500">
          Use <kbd class="rounded border border-zinc-800 bg-zinc-900 px-1 py-0.5 text-[10px]">↑</kbd> /
          <kbd class="rounded border border-zinc-800 bg-zinc-900 px-1 py-0.5 text-[10px]">↓</kbd> for command history · Toggle
          <strong class="text-indigo-300">Send to Pi (`bash` RPC)</strong> to share output with the LLM.
        </p>
      </div>
    {:else}
      {#each sessionView.items as item (item.id)}
        <span
          class="whitespace-pre-wrap break-words {item.stream === 'command'
            ? 'font-semibold text-indigo-300'
            : item.stream === 'stderr'
              ? 'text-rose-300'
              : item.stream === 'system'
                ? 'text-amber-300/90 italic'
                : item.stream === 'pi-bash'
                  ? 'text-emerald-200'
                  : 'text-zinc-200'}"
        >
          {#each item.spans as span, idx (idx)}
            <span
              class="{span.fgClass ?? ''} {span.bgClass ?? ''} {span.bold
                ? 'font-bold'
                : ''} {span.dim ? 'opacity-60' : ''} {span.italic
                ? 'italic'
                : ''} {span.underline ? 'underline' : ''}">{span.text}</span
            >
          {/each}
        </span>
      {/each}
    {/if}
  </div>

  <!-- Command Input Bar -->
  <div class="shrink-0 border-t border-zinc-800/80 bg-zinc-900/70 p-2">
    <div
      class="flex items-center gap-2 rounded-lg border px-2.5 py-1.5 transition {sessionView.sendToPiBash
        ? 'border-indigo-500/50 bg-indigo-950/20 focus-within:border-indigo-400'
        : 'border-zinc-800 bg-zinc-950 focus-within:border-indigo-500/50'}"
    >
      <span
        class="select-none font-mono text-xs font-bold {sessionView.sendToPiBash
          ? 'text-indigo-400'
          : 'text-emerald-400'}"
      >
        {sessionView.sendToPiBash ? 'pi$' : '$'}
      </span>

      <input
        bind:this={inputEl}
        type="text"
        bind:value={commandInput}
        onkeydown={handleInputKeydown}
        placeholder={sessionView.sendToPiBash
          ? 'Run command via Pi `bash` RPC (adds output to LLM context)…'
          : 'Enter shell command (↑/↓ for history, Ctrl+C to interrupt)…'}
        aria-label="Terminal command input"
        class="min-w-0 flex-1 bg-transparent font-mono text-xs text-zinc-100 placeholder-zinc-500 outline-none"
      />

      {#if sessionView.isRunningPiBash}
        <Loader2 class="h-3.5 w-3.5 animate-spin text-indigo-400" />
      {/if}

      <button
        type="button"
        onclick={() => void handleRunCommand()}
        disabled={!commandInput.trim()}
        class="inline-flex items-center gap-1 rounded-md px-2.5 py-1 text-xs font-medium text-white transition disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer {sessionView.sendToPiBash
          ? 'bg-indigo-600 hover:bg-indigo-500'
          : 'bg-emerald-600 hover:bg-emerald-500'}"
      >
        {#if sessionView.sendToPiBash}
          <Send class="h-3 w-3" />
          <span>Run in Pi</span>
        {:else}
          <CornerDownLeft class="h-3 w-3" />
          <span>Run</span>
        {/if}
      </button>
    </div>
  </div>
</div>
