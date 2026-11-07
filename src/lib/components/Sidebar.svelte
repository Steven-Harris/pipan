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
    Wrench,
    Search,
    Square,
    PowerOff,
    Pencil,
    Archive,
    ArchiveRestore,
    Trash2,
    Pin,
    Tag,
    Check,
    X,
    Coins
  } from 'lucide-svelte';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import { workspaceStore } from '#lib/stores/workspace.svelte.ts';
  import type {
    SavedSession,
    SessionFilter,
    SessionGroupBy,
    SessionSortBy
  } from '#lib/types/pi.ts';

  let activeTab = $state<'sessions' | 'prs'>('sessions');
  let isPickingFolder = $state(false);

  // Inline rename state per session card
  let editingSessionId = $state<string | null>(null);
  let editingNameDraft = $state('');

  // Inline custom group assignment state per session card
  let editingGroupSessionId = $state<string | null>(null);
  let editingGroupDraft = $state('');

  const FILTER_PILLS: Array<{ id: SessionFilter; label: string }> = [
    { id: 'active', label: 'Active' },
    { id: 'running', label: 'Running' },
    { id: 'archived', label: 'Archived' },
    { id: 'directory', label: 'Directory' },
    { id: 'headless', label: 'Headless' },
    { id: 'worktree', label: 'Worktree' }
  ];

  const GROUP_OPTIONS: Array<{ id: SessionGroupBy; label: string }> = [
    { id: 'none', label: 'None' },
    { id: 'folder', label: 'Workspace Folder' },
    { id: 'group', label: 'Custom Group' },
    { id: 'date', label: 'Date Bucket' }
  ];

  const SORT_OPTIONS: Array<{ id: SessionSortBy; label: string }> = [
    { id: 'updated_at', label: 'Last Updated' },
    { id: 'created_at', label: 'Created Date' },
    { id: 'name', label: 'Name (A–Z)' },
    { id: 'total_cost_usd', label: 'Total Cost' },
    { id: 'message_count', label: 'Message Count' }
  ];

  function toMillis(ts: number): number {
    return ts < 10_000_000_000 ? ts * 1000 : ts;
  }

  function formatRelativeTime(ts: number): string {
    const ms = toMillis(ts);
    const date = new Date(ms);
    const now = new Date();
    const sameDay =
      date.getFullYear() === now.getFullYear() &&
      date.getMonth() === now.getMonth() &&
      date.getDate() === now.getDate();
    if (sameDay) {
      return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    }
    return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
  }

  function formatFolderBucket(session: SavedSession): string {
    if (session.mode === 'headless' || !session.cwd) {
      return 'Headless Sandbox';
    }
    const cleaned = session.cwd.replace(/^\\\\\?\\/, '');
    const sep = cleaned.includes('\\') ? '\\' : '/';
    const parts = cleaned.split(/[\\/]+/).filter(Boolean);
    if (parts.length <= 2) return cleaned;
    return `…${sep}${parts.slice(-2).join(sep)}`;
  }

  function computeDateBucket(ts: number): 'Today' | 'This Week' | 'Older' {
    const ms = toMillis(ts);
    const now = new Date();
    const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
    if (ms >= startOfToday) {
      return 'Today';
    }
    const sevenDaysAgo = startOfToday - 6 * 24 * 60 * 60 * 1000;
    if (ms >= sevenDaysAgo) {
      return 'This Week';
    }
    return 'Older';
  }

  const groupedSessions = $derived.by(
    (): Array<{ key: string; label: string; items: SavedSession[] }> => {
      const list = chatStore.savedSessions;
      const groupBy = chatStore.sessionGroupBy;

      if (groupBy === 'none') {
        return [{ key: '__all__', label: '', items: list }];
      }

      const buckets = new Map<string, SavedSession[]>();
      for (const session of list) {
        let bucketLabel: string;
        if (groupBy === 'folder') {
          bucketLabel = formatFolderBucket(session);
        } else if (groupBy === 'group') {
          bucketLabel = session.groupName?.trim() || 'Ungrouped';
        } else {
          bucketLabel = computeDateBucket(session.updatedAt || session.createdAt);
        }
        const existing = buckets.get(bucketLabel);
        if (existing) {
          existing.push(session);
        } else {
          buckets.set(bucketLabel, [session]);
        }
      }

      if (groupBy === 'date') {
        const order: Array<'Today' | 'This Week' | 'Older'> = ['Today', 'This Week', 'Older'];
        return order
          .filter((b) => buckets.has(b))
          .map((b) => ({ key: b, label: b, items: buckets.get(b) ?? [] }));
      }

      return Array.from(buckets.entries()).map(([label, items]) => ({
        key: label,
        label,
        items
      }));
    }
  );

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

  function beginRename(session: SavedSession) {
    editingGroupSessionId = null;
    editingSessionId = session.id;
    editingNameDraft = session.name;
  }

  async function commitRename(sessionId: string) {
    if (editingSessionId !== sessionId) return;
    const trimmed = editingNameDraft.trim();
    editingSessionId = null;
    if (trimmed) {
      await chatStore.renameSession(sessionId, trimmed);
    }
  }

  function cancelRename() {
    editingSessionId = null;
  }

  function beginEditGroup(session: SavedSession) {
    editingSessionId = null;
    editingGroupSessionId = session.id;
    editingGroupDraft = session.groupName ?? '';
  }

  async function commitGroup(sessionId: string) {
    if (editingGroupSessionId !== sessionId) return;
    const trimmed = editingGroupDraft.trim();
    editingGroupSessionId = null;
    await chatStore.setSessionGroup(sessionId, trimmed || null);
  }

  function cancelGroup() {
    editingGroupSessionId = null;
  }

  function isSessionRunning(session: SavedSession): boolean {
    const isCurrent = session.id === chatStore.activeSessionId;
    if (isCurrent) {
      return (
        chatStore.sessionStatus.state === 'running' ||
        chatStore.sessionStatus.state === 'starting'
      );
    }
    return session.status === 'running' || session.status === 'starting';
  }

  function formatCost(costUsd: number): string {
    if (!costUsd || costUsd <= 0) return '$0.00';
    if (costUsd < 0.01) return `$${costUsd.toFixed(4)}`;
    return `$${costUsd.toFixed(2)}`;
  }
</script>

{#if chatStore.sidebarOpen}
  <aside
    aria-label="Workspace navigation"
    class="flex w-72 shrink-0 flex-col border-r border-zinc-800/80 bg-zinc-900/60 select-none"
  >
    <!-- Top Tab Switcher -->
    <div class="grid grid-cols-3 gap-1 border-b border-zinc-800/80 p-2">
      <button
        type="button"
        onclick={() => {
          activeTab = 'sessions';
          workspaceStore.openChatView();
        }}
        class="inline-flex items-center justify-center gap-1 rounded-lg px-2 py-1.5 text-xs font-medium transition cursor-pointer {activeTab ===
          'sessions' && workspaceStore.currentView === 'chat'
          ? 'bg-zinc-800 text-zinc-100 shadow-sm'
          : 'text-zinc-300 hover:bg-zinc-800/50 hover:text-zinc-100'}"
      >
        <History class="h-3.5 w-3.5 text-indigo-400" />
        <span class="truncate">Sessions</span>
        {#if chatStore.savedSessions.length > 0}
          <span class="hidden sm:inline-block rounded-full bg-zinc-700/80 px-1 py-0.2 font-mono text-[9px] text-zinc-200 tabular-nums">
            {chatStore.savedSessions.length}
          </span>
        {/if}
      </button>

      <button
        type="button"
        onclick={() => {
          activeTab = 'prs';
          workspaceStore.openChatView();
        }}
        class="inline-flex items-center justify-center gap-1 rounded-lg px-2 py-1.5 text-xs font-medium transition cursor-pointer {activeTab ===
          'prs' && workspaceStore.currentView === 'chat'
          ? 'bg-zinc-800 text-zinc-100 shadow-sm'
          : 'text-zinc-300 hover:bg-zinc-800/50 hover:text-zinc-100'}"
      >
        <GitPullRequest class="h-3.5 w-3.5 text-sky-400" />
        <span class="truncate">PRs</span>
      </button>

      <button
        type="button"
        onclick={() => workspaceStore.toggleCostView()}
        class="inline-flex items-center justify-center gap-1 rounded-lg px-2 py-1.5 text-xs font-medium transition cursor-pointer {workspaceStore.currentView ===
        'cost'
          ? 'bg-amber-500/20 text-amber-200 border border-amber-500/40 shadow-sm'
          : 'text-zinc-300 hover:bg-zinc-800/50 hover:text-zinc-100'}"
        title="Open Cost & Usage Analytics"
      >
        <Coins class="h-3.5 w-3.5 text-amber-400" />
        <span class="truncate">Cost</span>
      </button>
    </div>

    {#if activeTab === 'sessions'}
      <!-- Quick Session Launchers -->
      <div class="border-b border-zinc-800/80 p-2.5 space-y-2">
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

        <!-- Real-time Search Input -->
        <div class="relative">
          <Search class="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-zinc-400" />
          <input
            type="search"
            value={chatStore.searchQuery}
            oninput={(e) => void chatStore.setSearchQuery(e.currentTarget.value)}
            placeholder="Search name, folder, group, tag, branch…"
            aria-label="Search sessions"
            class="w-full rounded-lg border border-zinc-800 bg-zinc-950/80 pl-8 pr-2.5 py-1.5 text-xs text-zinc-100 placeholder-zinc-500 focus:border-indigo-500/50 focus:outline-none"
          />
        </div>

        <!-- Quick Filter Pills -->
        <div class="flex flex-wrap gap-1" role="group" aria-label="Filter sessions">
          {#each FILTER_PILLS as pill (pill.id)}
            {@const active = chatStore.sessionFilter === pill.id}
            <button
              type="button"
              onclick={() => void chatStore.setSessionFilter(pill.id)}
              aria-pressed={active}
              class="rounded-md border px-2 py-0.5 text-[10px] font-medium transition cursor-pointer {active
                ? 'border-indigo-500/50 bg-indigo-500/20 text-indigo-200'
                : 'border-zinc-800 bg-zinc-900/70 text-zinc-400 hover:border-zinc-700 hover:text-zinc-200'}"
            >
              {pill.label}
            </button>
          {/each}
        </div>

        <!-- Group By & Sort By Controls -->
        <div class="grid grid-cols-2 gap-1.5 text-[11px]">
          <label class="flex flex-col gap-0.5 text-zinc-400">
            <span class="text-[10px] uppercase tracking-wider text-zinc-500">Group By</span>
            <select
              value={chatStore.sessionGroupBy}
              onchange={(e) =>
                chatStore.setSessionGroupBy(e.currentTarget.value as SessionGroupBy)}
              aria-label="Group sessions by"
              class="rounded-md border border-zinc-800 bg-zinc-950 px-1.5 py-1 text-[11px] text-zinc-200 focus:border-indigo-500/50 focus:outline-none"
            >
              {#each GROUP_OPTIONS as opt (opt.id)}
                <option value={opt.id}>{opt.label}</option>
              {/each}
            </select>
          </label>

          <label class="flex flex-col gap-0.5 text-zinc-400">
            <span class="text-[10px] uppercase tracking-wider text-zinc-500">Sort By</span>
            <select
              value={chatStore.sessionSortBy}
              onchange={(e) =>
                void chatStore.setSessionSortBy(e.currentTarget.value as SessionSortBy)}
              aria-label="Sort sessions by"
              class="rounded-md border border-zinc-800 bg-zinc-950 px-1.5 py-1 text-[11px] text-zinc-200 focus:border-indigo-500/50 focus:outline-none"
            >
              {#each SORT_OPTIONS as opt (opt.id)}
                <option value={opt.id}>{opt.label}</option>
              {/each}
            </select>
          </label>
        </div>
      </div>

      <!-- Persistent Sessions List -->
      <div class="flex-1 overflow-y-auto p-2.5 space-y-3">
        {#if chatStore.savedSessions.length === 0}
          <div class="px-2 py-6 text-center text-xs text-zinc-300 space-y-1.5">
            <p class="font-medium text-zinc-200">No matching sessions</p>
            <p class="text-[11px] leading-relaxed text-zinc-400">
              {#if chatStore.searchQuery || chatStore.sessionFilter !== 'active'}
                Try clearing the search query or switching the filter pill above.
              {:else}
                Start a Headless or Folder session to persist transcripts and metadata in pipan.db.
              {/if}
            </p>
          </div>
        {:else}
          {#each groupedSessions as group (group.key)}
            <div class="space-y-1.5">
              {#if group.label}
                <div class="flex items-center justify-between px-1 text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
                  <span class="truncate">{group.label}</span>
                  <span class="font-mono text-zinc-500">{group.items.length}</span>
                </div>
              {/if}

              <ul class="space-y-1.5">
                {#each group.items as session (session.id)}
                  {@const isCurrent = session.id === chatStore.activeSessionId}
                  {@const running = isSessionRunning(session)}
                  {@const isGenerating = isCurrent && chatStore.isAgentRunning}
                  <li
                    class="rounded-xl border px-2.5 py-2 text-xs transition {isCurrent
                      ? 'border-indigo-500/40 bg-indigo-950/20 text-zinc-100'
                      : 'border-zinc-800/80 bg-zinc-900/50 text-zinc-200 hover:border-zinc-700'}"
                  >
                    <!-- Row 1: Icon, Session Name (Inline Rename), Pin & Timestamp -->
                    <div class="flex items-center justify-between gap-1.5">
                      <div class="flex min-w-0 flex-1 items-center gap-1.5">
                        {#if session.mode === 'headless'}
                          <Terminal class="h-3.5 w-3.5 shrink-0 text-amber-400" />
                        {:else if session.mode === 'worktree'}
                          <GitBranch class="h-3.5 w-3.5 shrink-0 text-emerald-400" />
                        {:else}
                          <FolderOpen class="h-3.5 w-3.5 shrink-0 text-sky-400" />
                        {/if}

                        {#if editingSessionId === session.id}
                          <div class="flex min-w-0 flex-1 items-center gap-1">
                            <input
                              type="text"
                              bind:value={editingNameDraft}
                              onkeydown={(e) => {
                                if (e.key === 'Enter') {
                                  e.preventDefault();
                                  void commitRename(session.id);
                                } else if (e.key === 'Escape') {
                                  e.preventDefault();
                                  cancelRename();
                                }
                              }}
                              onblur={() => void commitRename(session.id)}
                              aria-label="Edit session name"
                              class="min-w-0 flex-1 rounded bg-zinc-950 px-1.5 py-0.5 text-[11px] text-zinc-100 outline-none ring-1 ring-indigo-500/60"
                            />
                            <button
                              type="button"
                              onmousedown={(e) => {
                                e.preventDefault();
                                void commitRename(session.id);
                              }}
                              class="text-emerald-400 hover:text-emerald-300 cursor-pointer"
                              title="Save name"
                            >
                              <Check class="h-3 w-3" />
                            </button>
                            <button
                              type="button"
                              onmousedown={(e) => {
                                e.preventDefault();
                                cancelRename();
                              }}
                              class="text-zinc-400 hover:text-zinc-200 cursor-pointer"
                              title="Cancel"
                            >
                              <X class="h-3 w-3" />
                            </button>
                          </div>
                        {:else}
                          <button
                            type="button"
                            onclick={() => void chatStore.openSavedSession(session)}
                            ondblclick={(e) => {
                              e.stopPropagation();
                              beginRename(session);
                            }}
                            class="min-w-0 flex-1 truncate text-left font-medium text-zinc-100 hover:text-indigo-200 cursor-pointer"
                            title="{session.name} — click to open, double-click to rename"
                          >
                            {session.name}
                          </button>
                        {/if}
                      </div>

                      <div class="flex shrink-0 items-center gap-1">
                        {#if session.pinned}
                          <Pin class="h-3 w-3 text-amber-400" />
                        {/if}
                        <span class="font-mono text-[10px] text-zinc-400 tabular-nums">
                          {formatRelativeTime(session.updatedAt || session.createdAt)}
                        </span>
                      </div>
                    </div>

                    <!-- Row 2: Folder / Branch / Group / Tags metadata -->
                    <div class="mt-1 flex flex-wrap items-center gap-1 text-[10px] text-zinc-400">
                      <span class="truncate font-mono max-w-[160px]" title={session.cwd ?? 'Headless Sandbox'}>
                        {formatFolderBucket(session)}
                      </span>
                      {#if session.branch}
                        <span class="rounded bg-emerald-500/15 px-1.5 py-0.2 font-mono text-emerald-300">
                          {session.branch}
                        </span>
                      {/if}
                      {#if session.groupName}
                        <span class="rounded bg-indigo-500/15 px-1.5 py-0.2 text-indigo-300">
                          {session.groupName}
                        </span>
                      {/if}
                      {#each session.tags as tag (tag)}
                        <span class="rounded bg-zinc-800 px-1.5 py-0.2 text-zinc-300">
                          #{tag}
                        </span>
                      {/each}
                    </div>

                    <!-- Inline Custom Group Editor (when active) -->
                    {#if editingGroupSessionId === session.id}
                      <div class="mt-1.5 flex items-center gap-1">
                        <input
                          type="text"
                          bind:value={editingGroupDraft}
                          placeholder="Custom group name…"
                          onkeydown={(e) => {
                            if (e.key === 'Enter') {
                              e.preventDefault();
                              void commitGroup(session.id);
                            } else if (e.key === 'Escape') {
                              e.preventDefault();
                              cancelGroup();
                            }
                          }}
                          onblur={() => void commitGroup(session.id)}
                          aria-label="Custom group name"
                          class="min-w-0 flex-1 rounded bg-zinc-950 px-1.5 py-0.5 text-[11px] text-zinc-100 outline-none ring-1 ring-indigo-500/60"
                        />
                        <button
                          type="button"
                          onmousedown={(e) => {
                            e.preventDefault();
                            void commitGroup(session.id);
                          }}
                          class="text-emerald-400 hover:text-emerald-300 cursor-pointer"
                          title="Save group"
                        >
                          <Check class="h-3 w-3" />
                        </button>
                      </div>
                    {/if}

                    <!-- Row 3: Telemetry + Lifecycle Actions (Stop, Resume, Rename, Group, Pin, Archive, Delete) -->
                    <div class="mt-2 flex items-center justify-between gap-1.5 text-[11px] text-zinc-300 tabular-nums">
                      <div class="flex items-center gap-2">
                        <span class="inline-flex items-center gap-0.5" title="Messages">
                          <MessageSquare class="h-3 w-3 text-zinc-400" />
                          {session.messageCount}
                        </span>
                        <span class="inline-flex items-center gap-0.5" title="Tool calls">
                          <Wrench class="h-3 w-3 text-zinc-400" />
                          {session.toolCallCount}
                        </span>
                        {#if session.totalCostUsd > 0}
                          <span class="inline-flex items-center gap-0.5 font-mono text-[10px] text-zinc-400" title="Total session cost">
                            <Coins class="h-2.5 w-2.5 text-amber-400" />
                            {formatCost(session.totalCostUsd)}
                          </span>
                        {/if}
                      </div>

                      <div class="flex items-center gap-1">
                        {#if running}
                          {#if isGenerating}
                            <button
                              type="button"
                              onclick={() => void chatStore.abort()}
                              class="inline-flex items-center gap-1 rounded border border-amber-500/40 bg-amber-500/15 px-1.5 py-0.5 text-[10px] font-medium text-amber-200 hover:bg-amber-500/25 cursor-pointer"
                              title="Stop active generation (clear_queue + abort)"
                            >
                              <Square class="h-2.5 w-2.5 fill-current" />
                              <span>Stop</span>
                            </button>
                          {/if}
                          <button
                            type="button"
                            onclick={() => void chatStore.stopSessionById(session.id, session.mode)}
                            class="inline-flex items-center gap-1 rounded border border-rose-500/40 bg-rose-500/15 px-1.5 py-0.5 text-[10px] font-medium text-rose-200 hover:bg-rose-500/25 cursor-pointer"
                            title="Stop running session process"
                          >
                            <PowerOff class="h-2.5 w-2.5" />
                            <span>Stop</span>
                          </button>
                        {:else}
                          <button
                            type="button"
                            onclick={() => void chatStore.openSavedSession(session)}
                            class="inline-flex items-center gap-1 rounded border border-zinc-700 bg-zinc-800 px-1.5 py-0.5 text-[10px] font-medium text-zinc-200 hover:bg-zinc-700 cursor-pointer"
                            title="Resume or switch to this saved session"
                          >
                            <RotateCcw class="h-2.5 w-2.5" />
                            <span>Resume</span>
                          </button>
                        {/if}

                        <button
                          type="button"
                          onclick={() => beginRename(session)}
                          class="inline-flex h-5 w-5 items-center justify-center rounded text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 cursor-pointer"
                          title="Rename session"
                          aria-label="Rename session"
                        >
                          <Pencil class="h-3 w-3" />
                        </button>

                        <button
                          type="button"
                          onclick={() => beginEditGroup(session)}
                          class="inline-flex h-5 w-5 items-center justify-center rounded text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 cursor-pointer"
                          title="Assign custom group"
                          aria-label="Assign custom group"
                        >
                          <Tag class="h-3 w-3" />
                        </button>

                        <button
                          type="button"
                          onclick={() => void chatStore.togglePinSession(session.id, !session.pinned)}
                          class="inline-flex h-5 w-5 items-center justify-center rounded {session.pinned
                            ? 'text-amber-400'
                            : 'text-zinc-400 hover:text-zinc-200'} hover:bg-zinc-800 cursor-pointer"
                          title={session.pinned ? 'Unpin session' : 'Pin session'}
                          aria-label={session.pinned ? 'Unpin session' : 'Pin session'}
                        >
                          <Pin class="h-3 w-3" />
                        </button>

                        <button
                          type="button"
                          onclick={() => void chatStore.archiveSession(session.id, !session.archived)}
                          class="inline-flex h-5 w-5 items-center justify-center rounded text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 cursor-pointer"
                          title={session.archived ? 'Unarchive session' : 'Archive session'}
                          aria-label={session.archived ? 'Unarchive session' : 'Archive session'}
                        >
                          {#if session.archived}
                            <ArchiveRestore class="h-3 w-3 text-sky-400" />
                          {:else}
                            <Archive class="h-3 w-3" />
                          {/if}
                        </button>

                        <button
                          type="button"
                          onclick={() => void chatStore.deleteSession(session.id)}
                          class="inline-flex h-5 w-5 items-center justify-center rounded text-zinc-400 hover:bg-rose-500/20 hover:text-rose-300 cursor-pointer"
                          title="Delete session and remove .jsonl transcript"
                          aria-label="Delete session"
                        >
                          <Trash2 class="h-3 w-3" />
                        </button>
                      </div>
                    </div>
                  </li>
                {/each}
              </ul>
            </div>
          {/each}
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
