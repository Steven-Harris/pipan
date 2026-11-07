<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    FolderOpen,
    Terminal,
    Plus,
    PowerOff,
    Cpu,
    Sparkles,
    Loader2,
    PanelLeft,
    Workflow,
    Columns2,
    Rows2,
    Square,
    Pencil,
    Check,
    X
  } from 'lucide-svelte';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import { workspaceStore } from '#lib/stores/workspace.svelte.ts';
  import { PANELS } from '#lib/workspace/panels.ts';

  let isPickingFolder = $state(false);
  let isEditingName = $state(false);
  let draftName = $state('');
  let nameInputEl = $state<HTMLInputElement | null>(null);

  function startEditingName() {
    if (!chatStore.activeSessionId) return;
    draftName = chatStore.activeSessionName || workspaceLabel || 'Untitled Session';
    isEditingName = true;
    queueMicrotask(() => {
      nameInputEl?.focus();
      nameInputEl?.select();
    });
  }

  async function commitRename() {
    if (!isEditingName) return;
    const trimmed = draftName.trim();
    isEditingName = false;
    if (trimmed && chatStore.activeSessionId && trimmed !== chatStore.activeSessionName) {
      await chatStore.renameSession(chatStore.activeSessionId, trimmed);
    }
  }

  function cancelRename() {
    isEditingName = false;
  }

  function handleRenameKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      void commitRename();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      cancelRename();
    }
  }

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
      dotClass: 'bg-zinc-400',
      badgeClass: 'border-zinc-700 bg-zinc-800/80 text-zinc-300'
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
    const cleaned = cwd.replace(/^\\\\\?\\/, '');
    const sep = cleaned.includes('\\') ? '\\' : '/';
    const parts = cleaned.split(/[\\/]+/).filter(Boolean);
    if (parts.length <= 2) return cleaned;
    return `…${sep}${parts.slice(-2).join(sep)}`;
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
  <!-- Left: Sidebar Toggle, Brand, Connection Status, Workspace, and Model Badge -->
  <div class="flex min-w-0 flex-wrap items-center gap-2.5">
    <button
      type="button"
      onclick={() => chatStore.toggleSidebar()}
      class="inline-flex h-7 w-7 items-center justify-center rounded-lg border border-zinc-800 bg-zinc-900 text-zinc-300 transition hover:border-zinc-700 hover:bg-zinc-800 hover:text-zinc-100 cursor-pointer"
      title={chatStore.sidebarOpen ? 'Hide sidebar' : 'Show sidebar'}
      aria-label={chatStore.sidebarOpen ? 'Hide sidebar' : 'Show sidebar'}
    >
      <PanelLeft class="h-3.5 w-3.5" />
    </button>

    <div class="flex items-center gap-2 pr-1">
      <div class="flex h-7 w-7 items-center justify-center rounded-lg bg-indigo-600/20 border border-indigo-500/30 text-indigo-300">
        <Workflow class="h-3.5 w-3.5" />
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

    <!-- Active Session Name (Inline Rename via double-click or Edit button) -->
    {#if chatStore.activeSessionId}
      <div class="inline-flex items-center gap-1 rounded-md border border-zinc-800 bg-zinc-900/90 px-2 py-0.5 text-xs text-zinc-100">
        {#if isEditingName}
          <input
            bind:this={nameInputEl}
            bind:value={draftName}
            onkeydown={handleRenameKeydown}
            onblur={() => void commitRename()}
            aria-label="Session name"
            class="w-40 rounded bg-zinc-950 px-1.5 py-0.5 text-xs text-zinc-100 outline-none ring-1 ring-indigo-500/60"
          />
          <button
            type="button"
            onmousedown={(e) => {
              e.preventDefault();
              void commitRename();
            }}
            class="inline-flex h-4 w-4 items-center justify-center rounded text-emerald-400 hover:bg-zinc-800 cursor-pointer"
            title="Save session name"
            aria-label="Save session name"
          >
            <Check class="h-3 w-3" />
          </button>
          <button
            type="button"
            onmousedown={(e) => {
              e.preventDefault();
              cancelRename();
            }}
            class="inline-flex h-4 w-4 items-center justify-center rounded text-zinc-400 hover:bg-zinc-800 cursor-pointer"
            title="Cancel rename"
            aria-label="Cancel rename"
          >
            <X class="h-3 w-3" />
          </button>
        {:else}
          <span
            ondblclick={startEditingName}
            role="button"
            tabindex="0"
            onkeydown={(e) => {
              if (e.key === 'Enter' || e.key === 'F2') startEditingName();
            }}
            class="max-w-[180px] truncate font-medium text-zinc-100 cursor-pointer"
            title="Double-click to rename session"
          >
            {chatStore.activeSessionName || workspaceLabel || 'Untitled Session'}
          </span>
          <button
            type="button"
            onclick={startEditingName}
            class="inline-flex h-4 w-4 items-center justify-center rounded text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 cursor-pointer"
            title="Rename session"
            aria-label="Rename session"
          >
            <Pencil class="h-2.5 w-2.5" />
          </button>
        {/if}
      </div>
    {/if}

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

  <!-- Right: Panel Toggles + Session Actions -->
  <div class="flex shrink-0 items-center gap-2">
    <div class="flex items-center gap-1" role="toolbar" aria-label="Session panels">
      {#each PANELS as def (def.id)}
        {@const Icon = def.icon}
        {@const isOpen = workspaceStore.isOpen(def.id)}
        <button
          type="button"
          onclick={() => workspaceStore.togglePanel(def.id)}
          aria-pressed={isOpen}
          class="inline-flex items-center gap-1.5 rounded-lg border px-2 py-1.5 text-xs font-medium transition cursor-pointer {isOpen
            ? 'border-indigo-500/40 bg-indigo-500/15 text-indigo-100'
            : 'border-zinc-700/80 bg-zinc-800/60 text-zinc-300 hover:border-zinc-600 hover:bg-zinc-700/70'}"
          title={isOpen ? `Close ${def.label}` : `Open ${def.label}`}
        >
          <Icon class="h-3.5 w-3.5 {isOpen ? 'text-indigo-300' : 'text-zinc-400'}" />
          <span class="hidden xl:inline">{def.label}</span>
        </button>
      {/each}

      {#if workspaceStore.hasPanels}
        {@const toLabel =
          workspaceStore.orientation === 'vertical'
            ? 'Switch to top/bottom split'
            : 'Switch to side-by-side split'}
        <button
          type="button"
          onclick={() => workspaceStore.toggleOrientation()}
          class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-700/80 bg-zinc-800/60 px-2 py-1.5 text-xs font-medium text-zinc-300 transition hover:border-zinc-600 hover:bg-zinc-700/70 cursor-pointer"
          title={toLabel}
          aria-label={toLabel}
        >
          {#if workspaceStore.orientation === 'vertical'}
            <Columns2 class="h-3.5 w-3.5 text-indigo-400" />
            <span class="hidden lg:inline">Side by side</span>
          {:else}
            <Rows2 class="h-3.5 w-3.5 text-indigo-400" />
            <span class="hidden lg:inline">Top / bottom</span>
          {/if}
        </button>
      {/if}
    </div>

    <span class="h-5 w-px bg-zinc-800" aria-hidden="true"></span>

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

    {#if chatStore.sessionStatus.state === 'running' || chatStore.sessionStatus.state === 'starting'}
      {#if chatStore.isAgentRunning}
        <button
          type="button"
          onclick={() => chatStore.abort()}
          class="inline-flex items-center gap-1.5 rounded-lg border border-amber-500/40 bg-amber-500/15 px-2.5 py-1.5 text-xs font-semibold text-amber-200 transition hover:border-amber-500/60 hover:bg-amber-500/25 cursor-pointer"
          title="Stop active generation (clear_queue + abort)"
        >
          <Square class="h-3 w-3 fill-current" />
          <span>Stop</span>
        </button>
      {/if}

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
        title="Stop Pi RPC child process (stop_pi_session)"
      >
        <PowerOff class="h-3.5 w-3.5" />
        <span class="hidden md:inline">Stop Process</span>
      </button>
    {/if}
  </div>
</header>
