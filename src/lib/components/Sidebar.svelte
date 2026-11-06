<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    History,
    GitPullRequest,
    Terminal,
    FolderOpen,
    RotateCcw,
    Radio,
    GitBranch,
    MessageSquare,
    Wrench
  } from 'lucide-svelte';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import type { SessionHistoryEntry } from '#lib/types/pi.ts';

  let activeTab = $state<'sessions' | 'prs'>('sessions');
  let isPickingFolder = $state(false);

  async function handleSelectFolder() {
    if (isPickingFolder) return;
    isPickingFolder = true;
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
    } finally {
      isPickingFolder = false;
    }
  }

  async function handleRelaunchEntry(entry: SessionHistoryEntry) {
    if (entry.mode === 'directory' && entry.cwd) {
      await chatStore.startInDirectory(entry.cwd);
    } else {
      await chatStore.startHeadless();
    }
  }

  function formatTime(ts: number): string {
    return new Date(ts).toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit'
    });
  }
</script>

{#if chatStore.sidebarOpen}
  <aside
    aria-label="Workspace navigation"
    class="flex w-64 shrink-0 flex-col border-r border-zinc-800/80 bg-zinc-900/60 select-none"
  >
    <!-- Top Tab Switcher -->
    <div class="grid grid-cols-2 gap-1 border-b border-zinc-800/80 p-2">
      <button
        type="button"
        onclick={() => (activeTab = 'sessions')}
        class="inline-flex items-center justify-center gap-1.5 rounded-lg px-2.5 py-1.5 text-xs font-medium transition cursor-pointer {activeTab ===
        'sessions'
          ? 'bg-zinc-800 text-zinc-100 shadow-sm'
          : 'text-zinc-300 hover:bg-zinc-800/50 hover:text-zinc-100'}"
      >
        <History class="h-3.5 w-3.5 text-indigo-400" />
        <span>Sessions</span>
        {#if chatStore.sessionHistory.length > 0}
          <span class="rounded-full bg-zinc-700/80 px-1.5 py-0.2 font-mono text-[10px] text-zinc-200 tabular-nums">
            {chatStore.sessionHistory.length}
          </span>
        {/if}
      </button>

      <button
        type="button"
        onclick={() => (activeTab = 'prs')}
        class="inline-flex items-center justify-center gap-1.5 rounded-lg px-2.5 py-1.5 text-xs font-medium transition cursor-pointer {activeTab ===
        'prs'
          ? 'bg-zinc-800 text-zinc-100 shadow-sm'
          : 'text-zinc-300 hover:bg-zinc-800/50 hover:text-zinc-100'}"
      >
        <GitPullRequest class="h-3.5 w-3.5 text-sky-400" />
        <span>Pull Requests</span>
      </button>
    </div>

    {#if activeTab === 'sessions'}
      <!-- Quick Session Launchers -->
      <div class="border-b border-zinc-800/80 p-3 space-y-2">
        <div class="grid grid-cols-2 gap-1.5">
          <button
            type="button"
            onclick={() => chatStore.startHeadless()}
            disabled={chatStore.sessionStatus.state === 'starting'}
            class="inline-flex items-center justify-center gap-1.5 rounded-lg border border-zinc-700/80 bg-zinc-800/90 px-2.5 py-1.5 text-xs font-medium text-zinc-100 transition hover:border-zinc-600 hover:bg-zinc-700/80 disabled:opacity-50 cursor-pointer"
            title="Launch an isolated headless temp workspace"
          >
            <Terminal class="h-3.5 w-3.5 text-amber-400 shrink-0" />
            <span class="truncate">Headless</span>
          </button>

          <button
            type="button"
            onclick={handleSelectFolder}
            disabled={chatStore.sessionStatus.state === 'starting' || isPickingFolder}
            class="inline-flex items-center justify-center gap-1.5 rounded-lg border border-zinc-700/80 bg-zinc-800/90 px-2.5 py-1.5 text-xs font-medium text-zinc-100 transition hover:border-zinc-600 hover:bg-zinc-700/80 disabled:opacity-50 cursor-pointer"
            title="Bind Pi RPC to a local directory"
          >
            <FolderOpen class="h-3.5 w-3.5 text-sky-400 shrink-0" />
            <span class="truncate">Folder…</span>
          </button>
        </div>
      </div>

      <!-- Session History List -->
      <div class="flex-1 overflow-y-auto p-2.5">
        {#if chatStore.sessionHistory.length === 0}
          <div class="px-2 py-6 text-center text-xs text-zinc-300 space-y-1.5">
            <p class="font-medium text-zinc-200">No session history yet</p>
            <p class="text-[11px] leading-relaxed text-zinc-300">
              Start a Headless or Folder session to record telemetry and workspace runs here.
            </p>
          </div>
        {:else}
          <ul class="space-y-1.5">
            {#each chatStore.sessionHistory as entry (entry.id)}
              {@const isCurrent = entry.id === chatStore.activeSessionId}
              <li
                class="rounded-xl border px-3 py-2 text-xs transition {isCurrent
                  ? 'border-indigo-500/40 bg-indigo-950/20 text-zinc-100'
                  : 'border-zinc-800/80 bg-zinc-900/50 text-zinc-200 hover:border-zinc-700'}"
              >
                <div class="flex items-center justify-between gap-2">
                  <div class="flex min-w-0 items-center gap-1.5">
                    {#if entry.mode === 'headless'}
                      <Terminal class="h-3.5 w-3.5 shrink-0 text-amber-400" />
                    {:else}
                      <FolderOpen class="h-3.5 w-3.5 shrink-0 text-sky-400" />
                    {/if}
                    <span class="truncate font-mono text-[11px] font-medium text-zinc-100" title={entry.cwd ?? 'Headless Temp Sandbox'}>
                      {entry.label}
                    </span>
                  </div>

                  <span class="shrink-0 font-mono text-[10px] text-zinc-300 tabular-nums">
                    {formatTime(entry.startedAt)}
                  </span>
                </div>

                <div class="mt-2 flex items-center justify-between gap-2 text-[11px] text-zinc-300 tabular-nums">
                  <div class="flex items-center gap-2.5">
                    <span class="inline-flex items-center gap-1" title="Messages">
                      <MessageSquare class="h-3 w-3 text-zinc-400" />
                      {entry.messageCount}
                    </span>
                    <span class="inline-flex items-center gap-1" title="Tool calls">
                      <Wrench class="h-3 w-3 text-zinc-400" />
                      {entry.toolCallCount}
                    </span>
                    {#if entry.pid}
                      <span class="font-mono text-[10px] text-zinc-300">
                        PID {entry.pid}
                      </span>
                    {/if}
                  </div>

                  {#if isCurrent && chatStore.sessionStatus.state === 'running'}
                    <span class="inline-flex items-center gap-1 rounded-full bg-emerald-500/15 px-2 py-0.5 text-[10px] font-medium text-emerald-300">
                      <span class="h-1.5 w-1.5 rounded-full bg-emerald-400"></span>
                      Active
                    </span>
                  {:else}
                    <button
                      type="button"
                      onclick={() => handleRelaunchEntry(entry)}
                      class="inline-flex items-center gap-1 rounded-md border border-zinc-700 bg-zinc-800 px-2 py-0.5 text-[10px] font-medium text-zinc-200 hover:bg-zinc-700 cursor-pointer"
                      title="Relaunch this workspace session"
                    >
                      <RotateCcw class="h-2.5 w-2.5" />
                      <span>Resume</span>
                    </button>
                  {/if}
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {:else}
      <!-- Pull Requests & Webhook Tunnel Status -->
      <div class="flex-1 overflow-y-auto p-3 space-y-3 text-xs">
        <div class="rounded-xl border border-zinc-800 bg-zinc-900/80 p-3 space-y-2">
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-1.5 font-semibold text-zinc-100">
              <GitBranch class="h-3.5 w-3.5 text-sky-400" />
              <span>Workspace Repository</span>
            </div>
            <span class="rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-[10px] text-zinc-200">
              {chatStore.sessionStatus.mode === 'directory' && chatStore.sessionStatus.cwd
                ? 'Bound'
                : 'Sandbox'}
            </span>
          </div>

          <p class="text-[11px] leading-relaxed text-zinc-300 break-words font-mono">
            {chatStore.sessionStatus.mode === 'directory' && chatStore.sessionStatus.cwd
              ? chatStore.sessionStatus.cwd
              : 'Headless temp directory active. Open a Git repository folder to inspect pull requests.'}
          </p>
        </div>

        <div class="rounded-xl border border-zinc-800 bg-zinc-900/80 p-3 space-y-2">
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-1.5 font-semibold text-zinc-100">
              <Radio class="h-3.5 w-3.5 text-amber-400" />
              <span>Webhook Tunnel</span>
            </div>
            <span class="rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-[10px] text-zinc-300">
              Standby
            </span>
          </div>

          <p class="text-[11px] leading-relaxed text-zinc-300">
            Pull request reviews, issue comments, and CI status checks stream via <code class="rounded bg-zinc-800 px-1 py-0.5 font-mono text-zinc-200">gh webhook forward</code> without REST polling.
          </p>

          {#if chatStore.sessionStatus.mode !== 'directory'}
            <button
              type="button"
              onclick={handleSelectFolder}
              class="mt-1 inline-flex w-full items-center justify-center gap-1.5 rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-1.5 text-xs font-medium text-zinc-100 transition hover:bg-zinc-700 cursor-pointer"
            >
              <FolderOpen class="h-3.5 w-3.5 text-sky-400" />
              <span>Open Repository Folder…</span>
            </button>
          {/if}
        </div>
      </div>
    {/if}
  </aside>
{/if}
