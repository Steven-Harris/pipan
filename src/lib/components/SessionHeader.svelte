<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    FolderOpen,
    Terminal,
    Plus,
    PowerOff,
    Cpu,
    Sparkles,
    Loader2
  } from 'lucide-svelte';
  import { chatStore } from '$lib/stores/chat.svelte';

  let isPickingFolder = $state(false);

  const statusBadge = $derived.by(() => {
    const state = chatStore.sessionStatus.state;
    if (state === 'starting') {
      return {
        label: 'Starting...',
        dotClass: 'bg-amber-400 animate-pulse',
        badgeClass: 'border-amber-500/30 bg-amber-500/10 text-amber-300'
      };
    }
    if (state === 'error') {
      return {
        label: 'Error',
        dotClass: 'bg-rose-500',
        badgeClass: 'border-rose-500/30 bg-rose-500/10 text-rose-300'
      };
    }
    if (state === 'running') {
      if (chatStore.isAgentRunning) {
        return {
          label: 'Thinking / Streaming',
          dotClass: 'bg-indigo-400 animate-ping',
          badgeClass: 'border-indigo-500/30 bg-indigo-500/15 text-indigo-200'
        };
      }
      return {
        label: 'Ready',
        dotClass: 'bg-emerald-400',
        badgeClass: 'border-emerald-500/30 bg-emerald-500/10 text-emerald-300'
      };
    }
    return {
      label: 'Disconnected',
      dotClass: 'bg-zinc-500',
      badgeClass: 'border-zinc-700 bg-zinc-800/80 text-zinc-400'
    };
  });

  const workspaceLabel = $derived.by(() => {
    if (chatStore.sessionStatus.state === 'disconnected') {
      return null;
    }
    if (chatStore.sessionStatus.mode === 'headless') {
      return 'Headless (Isolated Temp)';
    }
    const cwd = chatStore.sessionStatus.cwd;
    if (!cwd) return 'Directory';
    const normalized = cwd.replace(/\\/g, '/');
    const parts = normalized.split('/').filter(Boolean);
    if (parts.length <= 2) return cwd;
    return `…/${parts.slice(-2).join('/')}`;
  });

  async function handleStartHeadless() {
    await chatStore.startHeadless();
  }

  async function handlePickFolder() {
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

  async function handleNewChat() {
    await chatStore.newSession();
  }

  async function handleDisconnect() {
    await chatStore.stopSession();
  }
</script>

<header class="flex items-center justify-between gap-3 border-b border-zinc-800/80 bg-zinc-900/70 px-4 py-2.5 backdrop-blur-md">
  <!-- Left: Brand, Connection Status, Workspace, and Model Badge -->
  <div class="flex min-w-0 flex-wrap items-center gap-2.5">
    <div class="flex items-center gap-2 pr-1">
      <div class="flex h-7 w-7 items-center justify-center rounded-lg bg-indigo-600/20 border border-indigo-500/30 text-indigo-400 font-semibold text-xs">
        π
      </div>
      <span class="font-semibold tracking-tight text-zinc-100 text-sm">Pipan</span>
    </div>

    <!-- Status Badge -->
    <div
      class="inline-flex items-center gap-2 rounded-full border px-2.5 py-0.5 text-xs font-medium {statusBadge.badgeClass}"
    >
      <span class="h-2 w-2 rounded-full {statusBadge.dotClass}"></span>
      <span>{statusBadge.label}</span>
    </div>

    <!-- Workspace Badge -->
    {#if workspaceLabel}
      <div
        class="inline-flex max-w-xs items-center gap-1.5 rounded-md border border-zinc-800 bg-zinc-900 px-2.5 py-1 text-xs text-zinc-300"
        title={chatStore.sessionStatus.cwd ?? 'Headless Temp Workspace'}
      >
        {#if chatStore.sessionStatus.mode === 'headless'}
          <Terminal class="h-3.5 w-3.5 shrink-0 text-amber-400" />
        {:else}
          <FolderOpen class="h-3.5 w-3.5 shrink-0 text-sky-400" />
        {/if}
        <span class="truncate font-mono">{workspaceLabel}</span>
      </div>
    {/if}

    <!-- Model & Thinking Level Badge -->
    {#if chatStore.sessionStatus.state === 'running' && chatStore.modelInfo?.id}
      <div
        class="hidden sm:inline-flex items-center gap-1.5 rounded-md border border-zinc-800/90 bg-zinc-900/60 px-2.5 py-1 text-xs text-zinc-400"
        title={chatStore.modelInfo.provider
          ? `${chatStore.modelInfo.provider}/${chatStore.modelInfo.id}`
          : chatStore.modelInfo.id}
      >
        <Cpu class="h-3.5 w-3.5 text-indigo-400" />
        <span class="truncate max-w-[180px] text-zinc-300">{chatStore.modelInfo.id}</span>
        {#if chatStore.modelInfo.thinkingLevel && chatStore.modelInfo.thinkingLevel !== 'off'}
          <span class="inline-flex items-center gap-0.5 rounded bg-indigo-500/15 px-1.5 py-0.2 text-[10px] font-medium text-indigo-300">
            <Sparkles class="h-2.5 w-2.5" />
            {chatStore.modelInfo.thinkingLevel}
          </span>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Right: Session Actions -->
  <div class="flex shrink-0 items-center gap-2">
    <button
      type="button"
      onclick={handleStartHeadless}
      disabled={chatStore.sessionStatus.state === 'starting'}
      class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-700/80 bg-zinc-800/90 px-2.5 py-1.5 text-xs font-medium text-zinc-200 transition hover:border-zinc-600 hover:bg-zinc-700/80 disabled:opacity-50 cursor-pointer"
      title="Start an isolated headless Pi RPC session in a temporary directory"
    >
      {#if chatStore.sessionStatus.state === 'starting' && chatStore.sessionStatus.mode === 'headless'}
        <Loader2 class="h-3.5 w-3.5 animate-spin text-amber-400" />
      {:else}
        <Terminal class="h-3.5 w-3.5 text-amber-400" />
      {/if}
      <span>Start Headless</span>
    </button>

    <button
      type="button"
      onclick={handlePickFolder}
      disabled={chatStore.sessionStatus.state === 'starting' || isPickingFolder}
      class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-700/80 bg-zinc-800/90 px-2.5 py-1.5 text-xs font-medium text-zinc-200 transition hover:border-zinc-600 hover:bg-zinc-700/80 disabled:opacity-50 cursor-pointer"
      title="Select a project folder and launch Pi RPC inside it"
    >
      <FolderOpen class="h-3.5 w-3.5 text-sky-400" />
      <span>Open Folder…</span>
    </button>

    {#if chatStore.sessionStatus.state === 'running'}
      <button
        type="button"
        onclick={handleNewChat}
        class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-700/80 bg-zinc-800/60 px-2.5 py-1.5 text-xs font-medium text-zinc-300 transition hover:border-zinc-600 hover:bg-zinc-700/70 cursor-pointer"
        title="Reset conversation (new_session)"
      >
        <Plus class="h-3.5 w-3.5 text-emerald-400" />
        <span class="hidden md:inline">New Chat</span>
      </button>

      <button
        type="button"
        onclick={handleDisconnect}
        class="inline-flex items-center gap-1.5 rounded-lg border border-rose-500/30 bg-rose-500/10 px-2.5 py-1.5 text-xs font-medium text-rose-300 transition hover:border-rose-500/50 hover:bg-rose-500/20 cursor-pointer"
        title="Stop Pi RPC process"
      >
        <PowerOff class="h-3.5 w-3.5" />
        <span class="hidden md:inline">Disconnect</span>
      </button>
    {/if}
  </div>
</header>
