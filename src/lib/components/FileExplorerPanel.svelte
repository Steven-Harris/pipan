<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    FolderTree,
    Folder,
    FolderOpen,
    FileCode,
    FileText,
    FileImage,
    ChevronRight,
    ChevronDown,
    RefreshCw,
    Search,
    Eye,
    EyeOff,
    Plus,
    Check,
    GitCompare,
    Code2,
    Loader2,
    X
  } from 'lucide-svelte';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import { fileExplorerStore } from '#lib/stores/files.svelte.ts';
  import type { GitFileStatusCode, WorkspaceFileEntry } from '#lib/types/pi.ts';

  let isPickingFolder = $state(false);

  const activeCwd = $derived(chatStore.effectiveWorkspaceCwd);

  // Sync the file explorer whenever the active session's cwd changes
  $effect(() => {
    void fileExplorerStore.syncWorkspace(activeCwd);
  });

  const codeLines = $derived.by(() => {
    const raw = fileExplorerStore.preview?.content;
    if (!raw) return [];
    return raw.replace(/\r\n/g, '\n').split('\n');
  });

  const diffLines = $derived.by(() => {
    const raw = fileExplorerStore.preview?.diff;
    if (!raw) return [];
    return raw.replace(/\r\n/g, '\n').split('\n');
  });

  function formatBytes(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
    if (bytes < 1024) return `${bytes} B`;
    const kb = bytes / 1024;
    if (kb < 1024) return `${kb.toFixed(1)} KB`;
    const mb = kb / 1024;
    return `${mb.toFixed(1)} MB`;
  }

  function gitBadgeMeta(status?: GitFileStatusCode): {
    label: string;
    title: string;
    cls: string;
  } | null {
    if (!status) return null;
    switch (status) {
      case 'M':
        return {
          label: 'M',
          title: 'Modified',
          cls: 'border-amber-500/40 bg-amber-500/15 text-amber-300'
        };
      case 'A':
        return {
          label: 'A',
          title: 'Added',
          cls: 'border-emerald-500/40 bg-emerald-500/15 text-emerald-300'
        };
      case 'D':
        return {
          label: 'D',
          title: 'Deleted',
          cls: 'border-rose-500/40 bg-rose-500/15 text-rose-300'
        };
      case 'R':
        return {
          label: 'R',
          title: 'Renamed',
          cls: 'border-sky-500/40 bg-sky-500/15 text-sky-300'
        };
      case '?':
        return {
          label: 'U',
          title: 'Untracked',
          cls: 'border-emerald-500/30 bg-emerald-500/10 text-emerald-300'
        };
      case 'U':
        return {
          label: '!',
          title: 'Unmerged / Conflict',
          cls: 'border-rose-500/50 bg-rose-500/20 text-rose-200'
        };
      default:
        return {
          label: status,
          title: `Git status: ${status}`,
          cls: 'border-zinc-700 bg-zinc-800 text-zinc-300'
        };
    }
  }

  function isImageExt(ext?: string): boolean {
    if (!ext) return false;
    return ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'ico'].includes(ext.toLowerCase());
  }

  function isDocExt(ext?: string): boolean {
    if (!ext) return false;
    return ['md', 'txt', 'rst', 'adoc', 'log'].includes(ext.toLowerCase());
  }

  function handleEntryDragStart(e: DragEvent, entry: WorkspaceFileEntry) {
    if (entry.isDir || !e.dataTransfer) return;
    e.dataTransfer.effectAllowed = 'copy';
    e.dataTransfer.setData('application/x-pipan-file-path', entry.path);
    e.dataTransfer.setData('text/plain', entry.path);
  }

  async function handlePickWorkspaceFolder() {
    if (isPickingFolder) return;
    isPickingFolder = true;
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Workspace Folder'
      });
      if (typeof selected === 'string' && selected.trim()) {
        await chatStore.startInDirectory(selected);
      }
    } catch (err) {
      fileExplorerStore.error = err instanceof Error ? err.message : String(err);
    } finally {
      isPickingFolder = false;
    }
  }

  function diffLineClass(line: string): string {
    if (line.startsWith('+++') || line.startsWith('---') || line.startsWith('diff --git')) {
      return 'bg-zinc-900/90 text-zinc-400 font-semibold';
    }
    if (line.startsWith('@@')) {
      return 'bg-indigo-950/40 text-indigo-300';
    }
    if (line.startsWith('+')) {
      return 'bg-emerald-950/35 text-emerald-200';
    }
    if (line.startsWith('-')) {
      return 'bg-rose-950/35 text-rose-200';
    }
    return 'text-zinc-300';
  }
</script>

<div class="flex h-full min-h-0 flex-col bg-zinc-950 text-zinc-100">
  <!-- Top Toolbar: Workspace Path, Modified Count, Hidden Toggle, Refresh -->
  <div class="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-zinc-800/80 bg-zinc-900/60 px-3 py-2">
    <div class="flex min-w-0 items-center gap-2">
      <FolderTree class="h-4 w-4 shrink-0 text-indigo-400" />
      <span
        class="max-w-xs truncate font-mono text-xs text-zinc-200"
        title={fileExplorerStore.cwd ?? 'No folder workspace selected'}
      >
        {fileExplorerStore.cwd
          ? fileExplorerStore.cwd.replace(/^\\\\\?\\/, '')
          : 'No folder workspace'}
      </span>

      {#if fileExplorerStore.modifiedCount > 0}
        <span
          class="rounded-full border border-amber-500/30 bg-amber-500/15 px-2 py-0.5 font-mono text-[10px] font-medium text-amber-300"
          title={`${fileExplorerStore.modifiedCount} changed or untracked files in git working tree`}
        >
          {fileExplorerStore.modifiedCount} changed
        </span>
      {/if}
    </div>

    <div class="flex items-center gap-1.5">
      <button
        type="button"
        onclick={() => void fileExplorerStore.toggleHiddenFiles()}
        aria-pressed={fileExplorerStore.includeHidden}
        class="inline-flex items-center gap-1 rounded border px-2 py-1 text-[11px] font-medium transition cursor-pointer {fileExplorerStore.includeHidden
          ? 'border-indigo-500/40 bg-indigo-500/15 text-indigo-200'
          : 'border-zinc-800 bg-zinc-900 text-zinc-400 hover:border-zinc-700 hover:text-zinc-200'}"
        title={fileExplorerStore.includeHidden
          ? 'Showing dotfiles and gitignored files (click to hide)'
          : 'Respecting .gitignore and hiding dotfiles (click to show all)'}
      >
        {#if fileExplorerStore.includeHidden}
          <Eye class="h-3 w-3 text-indigo-300" />
          <span>All Files</span>
        {:else}
          <EyeOff class="h-3 w-3" />
          <span> Filtered</span>
        {/if}
      </button>

      <button
        type="button"
        onclick={() => void fileExplorerStore.refresh()}
        disabled={!fileExplorerStore.cwd || fileExplorerStore.loadingRoot}
        class="inline-flex h-6 w-6 items-center justify-center rounded border border-zinc-800 bg-zinc-900 text-zinc-300 transition hover:border-zinc-700 hover:bg-zinc-800 disabled:opacity-40 cursor-pointer"
        title="Refresh workspace files and git status"
        aria-label="Refresh workspace files"
      >
        <RefreshCw class="h-3 w-3 {fileExplorerStore.loadingRoot ? 'animate-spin' : ''}" />
      </button>
    </div>
  </div>

  <!-- Quick File Filter Bar -->
  {#if fileExplorerStore.cwd}
    <div class="shrink-0 border-b border-zinc-800/80 bg-zinc-900/30 px-3 py-1.5">
      <div class="relative flex items-center">
        <Search class="pointer-events-none absolute left-2.5 h-3.5 w-3.5 text-zinc-500" />
        <input
          type="text"
          bind:value={fileExplorerStore.filterQuery}
          placeholder="Filter files by name or path…"
          aria-label="Filter workspace files"
          class="w-full rounded-md border border-zinc-800 bg-zinc-950 py-1 pl-8 pr-7 font-mono text-xs text-zinc-100 placeholder-zinc-500 outline-none focus:border-indigo-500"
        />
        {#if fileExplorerStore.filterQuery}
          <button
            type="button"
            onclick={() => (fileExplorerStore.filterQuery = '')}
            class="absolute right-2 rounded p-0.5 text-zinc-500 hover:text-zinc-200 cursor-pointer"
            aria-label="Clear filter"
          >
            <X class="h-3 w-3" />
          </button>
        {/if}
      </div>
    </div>
  {/if}

  {#if fileExplorerStore.error}
    <div class="m-3 rounded-lg border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-xs text-rose-200">
      {fileExplorerStore.error}
    </div>
  {/if}

  {#if !fileExplorerStore.cwd}
    <!-- Empty State when no folder workspace is active -->
    <div class="flex flex-1 flex-col items-center justify-center p-6 text-center">
      <FolderTree class="mb-2.5 h-8 w-8 text-zinc-700" />
      <div class="text-xs font-semibold text-zinc-200">No workspace folder open</div>
      <p class="mt-1 max-w-xs text-[11px] text-zinc-400">
        Open a project directory to browse files, inspect git status badges, view diffs, and attach files to your prompt.
      </p>
      <button
        type="button"
        onclick={() => void handlePickWorkspaceFolder()}
        disabled={isPickingFolder}
        class="mt-3 inline-flex items-center gap-1.5 rounded-lg bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white shadow hover:bg-indigo-500 disabled:opacity-50 cursor-pointer"
      >
        <FolderOpen class="h-3.5 w-3.5" />
        <span>Open Folder…</span>
      </button>
    </div>
  {:else}
    <!-- Split Tree + Code/Diff Preview Area -->
    <div class="flex min-h-0 flex-1 flex-col lg:flex-row divide-y lg:divide-y-0 lg:divide-x divide-zinc-800/80">
      <!-- Left / Top: Expandable Directory Tree -->
      <div
        role="tree"
        aria-label="Workspace files"
        class="min-h-[180px] flex-1 overflow-y-auto py-1 lg:max-w-[44%]"
      >
        {#if fileExplorerStore.loadingRoot && fileExplorerStore.visibleTreeNodes.length === 0}
          <div class="flex items-center justify-center gap-2 py-10 text-xs text-zinc-400">
            <Loader2 class="h-4 w-4 animate-spin text-indigo-400" />
            <span>Loading workspace files…</span>
          </div>
        {:else if fileExplorerStore.visibleTreeNodes.length === 0}
          <div class="px-4 py-8 text-center text-xs text-zinc-500">
            No matching files found.
          </div>
        {:else}
          {#each fileExplorerStore.visibleTreeNodes as node (node.entry.relativePath)}
            {@const selected = fileExplorerStore.selectedRelativePath === node.entry.relativePath}
            {@const badge = gitBadgeMeta(node.gitStatus)}
            {@const isAttached = fileExplorerStore.recentlyAttachedPath === node.entry.path}
            <div
              role="treeitem"
              aria-selected={selected}
              aria-expanded={node.entry.isDir ? node.expanded : undefined}
              tabindex="0"
              draggable={!node.entry.isDir}
              ondragstart={(e) => handleEntryDragStart(e, node.entry)}
              onclick={() => void fileExplorerStore.selectEntry(node.entry)}
              onkeydown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  void fileExplorerStore.selectEntry(node.entry);
                }
              }}
              style:padding-left={`${node.depth * 14 + 8}px`}
              class="group flex items-center justify-between gap-1.5 pr-2 py-1 text-xs transition select-none cursor-pointer {selected
                ? 'bg-indigo-500/15 text-indigo-100'
                : 'text-zinc-300 hover:bg-zinc-900/90 hover:text-zinc-100'}"
            >
              <div class="flex min-w-0 flex-1 items-center gap-1.5">
                {#if node.entry.isDir}
                  {#if node.loading}
                    <Loader2 class="h-3.5 w-3.5 shrink-0 animate-spin text-zinc-500" />
                  {:else if node.expanded}
                    <ChevronDown class="h-3.5 w-3.5 shrink-0 text-zinc-400" />
                  {:else}
                    <ChevronRight class="h-3.5 w-3.5 shrink-0 text-zinc-500" />
                  {/if}
                  {#if node.expanded}
                    <FolderOpen class="h-3.5 w-3.5 shrink-0 text-amber-400/90" />
                  {:else}
                    <Folder class="h-3.5 w-3.5 shrink-0 text-amber-400/80" />
                  {/if}
                {:else}
                  <span class="w-3.5 shrink-0"></span>
                  {#if isImageExt(node.entry.extension)}
                    <FileImage class="h-3.5 w-3.5 shrink-0 text-fuchsia-400" />
                  {:else if isDocExt(node.entry.extension)}
                    <FileText class="h-3.5 w-3.5 shrink-0 text-emerald-400" />
                  {:else}
                    <FileCode class="h-3.5 w-3.5 shrink-0 text-sky-400" />
                  {/if}
                {/if}

                <span
                  class="truncate font-mono text-[11px] {node.gitStatus === 'D'
                    ? 'line-through text-rose-300/80'
                    : node.entry.isHidden
                      ? 'text-zinc-400'
                      : ''}"
                  title={node.entry.relativePath}
                >
                  {node.entry.name}
                </span>
              </div>

              <div class="flex shrink-0 items-center gap-1">
                {#if !node.entry.isDir}
                  <button
                    type="button"
                    onclick={(e) => {
                      e.stopPropagation();
                      void fileExplorerStore.attachToPrompt(node.entry.path);
                    }}
                    class="hidden group-hover:inline-flex items-center gap-0.5 rounded border border-indigo-500/40 bg-indigo-500/15 px-1.5 py-0.5 text-[10px] font-medium text-indigo-200 hover:bg-indigo-500/25 cursor-pointer"
                    title="Attach file to chat prompt"
                  >
                    {#if isAttached}
                      <Check class="h-2.5 w-2.5 text-emerald-300" />
                      <span>Added</span>
                    {:else}
                      <Plus class="h-2.5 w-2.5" />
                      <span>Attach</span>
                    {/if}
                  </button>
                {/if}

                {#if badge}
                  <span
                    class="inline-flex h-4 min-w-[16px] items-center justify-center rounded border px-1 font-mono text-[10px] font-semibold {badge.cls}"
                    title={badge.title}
                  >
                    {badge.label}
                  </span>
                {:else if !node.entry.isDir}
                  <span class="font-mono text-[10px] text-zinc-500">
                    {formatBytes(node.entry.sizeBytes)}
                  </span>
                {/if}
              </div>
            </div>
          {/each}
        {/if}
      </div>

      <!-- Right / Bottom: Read-Only Code & Git Diff Viewer -->
      <div class="flex min-h-[220px] min-w-0 flex-1 flex-col bg-zinc-950">
        {#if fileExplorerStore.loadingPreview}
          <div class="flex flex-1 items-center justify-center gap-2 text-xs text-zinc-400">
            <Loader2 class="h-4 w-4 animate-spin text-indigo-400" />
            <span>Loading file preview…</span>
          </div>
        {:else if !fileExplorerStore.preview}
          <div class="flex flex-1 flex-col items-center justify-center p-6 text-center text-zinc-500">
            <FileCode class="mb-2 h-6 w-6 text-zinc-700" />
            <p class="text-xs text-zinc-400">Select a file to preview its code or `git diff`</p>
            <p class="mt-1 text-[11px] text-zinc-500">
              Click <strong class="text-indigo-300">+ Attach to Prompt</strong> or drag any file into the chat composer.
            </p>
          </div>
        {:else}
          {@const preview = fileExplorerStore.preview}
          {@const previewBadge = gitBadgeMeta(preview.gitStatus)}
          {@const isAttached = fileExplorerStore.recentlyAttachedPath === preview.path}

          <!-- File Preview Header -->
          <div class="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-zinc-800/80 bg-zinc-900/50 px-3 py-1.5">
            <div class="flex min-w-0 items-center gap-2">
              <span class="truncate font-mono text-xs font-medium text-zinc-100" title={preview.relativePath}>
                {preview.relativePath}
              </span>
              {#if previewBadge}
                <span
                  class="inline-flex items-center rounded border px-1.5 py-0.2 font-mono text-[10px] font-semibold {previewBadge.cls}"
                  title={previewBadge.title}
                >
                  {previewBadge.label}
                </span>
              {/if}
              <span class="font-mono text-[10px] text-zinc-500">
                {formatBytes(preview.sizeBytes)}
                {#if preview.truncated}
                  · truncated
                {/if}
              </span>
            </div>

            <div class="flex items-center gap-1.5">
              {#if preview.diff}
                <div class="inline-flex rounded-md border border-zinc-800 bg-zinc-900 p-0.5 text-[11px]">
                  <button
                    type="button"
                    onclick={() => (fileExplorerStore.previewTab = 'code')}
                    class="inline-flex items-center gap-1 rounded px-2 py-0.5 font-medium transition cursor-pointer {fileExplorerStore.previewTab === 'code'
                      ? 'bg-zinc-800 text-zinc-100'
                      : 'text-zinc-400 hover:text-zinc-200'}"
                  >
                    <Code2 class="h-3 w-3" />
                    <span>Code</span>
                  </button>
                  <button
                    type="button"
                    onclick={() => (fileExplorerStore.previewTab = 'diff')}
                    class="inline-flex items-center gap-1 rounded px-2 py-0.5 font-medium transition cursor-pointer {fileExplorerStore.previewTab === 'diff'
                      ? 'bg-indigo-600/30 text-indigo-200'
                      : 'text-zinc-400 hover:text-zinc-200'}"
                  >
                    <GitCompare class="h-3 w-3" />
                    <span>Diff</span>
                  </button>
                </div>
              {/if}

              <button
                type="button"
                onclick={() => void fileExplorerStore.attachToPrompt(preview.path)}
                class="inline-flex items-center gap-1 rounded-md border border-indigo-500/40 bg-indigo-500/15 px-2.5 py-1 text-xs font-medium text-indigo-100 transition hover:bg-indigo-500/25 cursor-pointer"
                title="Attach this file to the chat prompt composer"
              >
                {#if isAttached}
                  <Check class="h-3 w-3 text-emerald-300" />
                  <span>Attached to Prompt</span>
                {:else}
                  <Plus class="h-3 w-3 text-indigo-300" />
                  <span>+ Attach to Prompt</span>
                {/if}
              </button>
            </div>
          </div>

          <!-- Viewer Body -->
          <div class="min-h-0 flex-1 overflow-auto font-mono text-xs leading-relaxed select-text">
            {#if fileExplorerStore.previewTab === 'diff' && preview.diff}
              <div class="divide-y divide-zinc-900/50 py-1">
                {#each diffLines as line, idx (idx)}
                  <div class="flex px-3 py-0.5 whitespace-pre {diffLineClass(line)}">
                    <span class="w-9 shrink-0 select-none pr-3 text-right text-[10px] text-zinc-600 tabular-nums">
                      {idx + 1}
                    </span>
                    <span class="min-w-0 flex-1">{line}</span>
                  </div>
                {/each}
              </div>
            {:else if preview.isBinary}
              <div class="flex h-full flex-col items-center justify-center p-6 text-center text-zinc-400">
                <p class="text-xs">Binary file ({formatBytes(preview.sizeBytes)}) cannot be rendered as text.</p>
                <p class="mt-1 text-[11px] text-zinc-500">
                  You can still click <strong>+ Attach to Prompt</strong> above to include it in your message.
                </p>
              </div>
            {:else if preview.content !== undefined}
              <div class="py-1">
                {#each codeLines as line, idx (idx)}
                  <div class="flex px-3 py-0.2 whitespace-pre hover:bg-zinc-900/60">
                    <span class="w-10 shrink-0 select-none pr-3 text-right text-[10px] text-zinc-600 tabular-nums">
                      {idx + 1}
                    </span>
                    <span class="min-w-0 flex-1 text-zinc-200">{line}</span>
                  </div>
                {/each}
              </div>
            {:else}
              <div class="flex h-full items-center justify-center p-6 text-xs text-zinc-500">
                File is empty or has been deleted from the working tree.
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>
