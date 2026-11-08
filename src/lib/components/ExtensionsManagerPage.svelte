<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    Blocks,
    CheckCircle2,
    Download,
    FolderOpen,
    GitBranch,
    Loader2,
    MessageSquare,
    Power,
    Puzzle,
    RefreshCw,
    RotateCcw,
    Search,
    Sparkles,
    Terminal,
    Trash2,
    AlertCircle,
    ExternalLink
  } from 'lucide-svelte';
  import { extensionRegistry } from '#lib/extensions/registry.svelte.ts';
  import { resolveExtensionIcon } from '#lib/extensions/builtin.ts';
  import { workspaceStore } from '#lib/stores/workspace.svelte.ts';
  import type { RegisteredExtension } from '#lib/extensions/types.ts';

  type FilterTab = 'all' | 'builtin' | 'external' | 'enabled' | 'disabled';

  let searchQuery = $state('');
  let filterTab = $state<FilterTab>('all');
  let folderPathInput = $state('');
  let gitRepoUrlInput = $state('');
  let isPickingFolder = $state(false);
  let statusBanner = $state<{ type: 'success' | 'error'; message: string } | null>(null);

  const filteredExtensions = $derived.by((): RegisteredExtension[] => {
    const q = searchQuery.trim().toLowerCase();
    return extensionRegistry.extensions.filter((ext) => {
      if (filterTab === 'builtin' && !ext.builtin) return false;
      if (filterTab === 'external' && ext.builtin) return false;
      if (filterTab === 'enabled' && !(ext.installed && ext.enabled)) return false;
      if (filterTab === 'disabled' && ext.installed && ext.enabled) return false;

      if (!q) return true;
      const m = ext.manifest;
      const haystacks = [
        m.id,
        m.name,
        m.description,
        m.author,
        ...ext.aliases,
        ...(m.contributes.panels?.map((p) => `${p.id} ${p.title}`) ?? []),
        ...(m.contributes.sidebarTabs?.map((t) => `${t.id} ${t.title}`) ?? []),
        ...(m.contributes.mainViews?.map((v) => `${v.id} ${v.title}`) ?? []),
        ...(m.contributes.slashCommands?.map((c) => `/${c.name} ${c.description}`) ?? [])
      ];
      return haystacks.some((h) => h.toLowerCase().includes(q));
    });
  });

  function primaryIconForExtension(ext: RegisteredExtension) {
    const firstIcon =
      ext.manifest.contributes.panels?.[0]?.icon ??
      ext.manifest.contributes.mainViews?.[0]?.icon ??
      ext.manifest.contributes.sidebarTabs?.[0]?.icon ??
      'puzzle';
    return resolveExtensionIcon(firstIcon);
  }

  async function handleBrowseAndInstallFolder() {
    if (isPickingFolder) return;
    isPickingFolder = true;
    statusBanner = null;
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Pipan Extension Folder (containing pipan-extension.json)'
      });
      if (typeof selected === 'string' && selected.trim()) {
        folderPathInput = selected;
        const installed = await extensionRegistry.installFromFolder(selected);
        if (installed) {
          statusBanner = {
            type: 'success',
            message: `Installed extension "${installed.manifest.name}" (${installed.manifest.id} v${installed.manifest.version}).`
          };
          folderPathInput = '';
        } else if (extensionRegistry.error) {
          statusBanner = { type: 'error', message: extensionRegistry.error };
        }
      }
    } catch (err) {
      statusBanner = {
        type: 'error',
        message: err instanceof Error ? err.message : String(err)
      };
    } finally {
      isPickingFolder = false;
    }
  }

  async function handleInstallFolderByPath() {
    const path = folderPathInput.trim();
    if (!path) return;
    statusBanner = null;
    const installed = await extensionRegistry.installFromFolder(path);
    if (installed) {
      statusBanner = {
        type: 'success',
        message: `Installed extension "${installed.manifest.name}" (${installed.manifest.id} v${installed.manifest.version}).`
      };
      folderPathInput = '';
    } else if (extensionRegistry.error) {
      statusBanner = { type: 'error', message: extensionRegistry.error };
    }
  }

  async function handleInstallFromGit() {
    const url = gitRepoUrlInput.trim();
    if (!url) return;
    statusBanner = null;
    const installed = await extensionRegistry.installFromGit(url);
    if (installed) {
      statusBanner = {
        type: 'success',
        message: `Cloned and installed "${installed.manifest.name}" (${installed.manifest.id} v${installed.manifest.version}).`
      };
      gitRepoUrlInput = '';
    } else if (extensionRegistry.error) {
      statusBanner = { type: 'error', message: extensionRegistry.error };
    }
  }

  async function handleUninstall(ext: RegisteredExtension) {
    statusBanner = null;
    const ok = await extensionRegistry.uninstallExtension(ext.manifest.id);
    if (ok) {
      statusBanner = {
        type: 'success',
        message: ext.builtin
          ? `Uninstalled built-in extension "${ext.manifest.name}". You can reinstall it anytime below.`
          : `Uninstalled external extension "${ext.manifest.name}".`
      };
    } else if (extensionRegistry.error) {
      statusBanner = { type: 'error', message: extensionRegistry.error };
    }
  }
</script>

<section
  aria-label="Extensions Marketplace and Manager"
  class="flex h-full min-h-0 flex-1 flex-col overflow-y-auto bg-zinc-950 text-zinc-100"
>
  <!-- Top Header -->
  <div class="flex flex-wrap items-center justify-between gap-3 border-b border-zinc-800/80 bg-zinc-900/60 px-6 py-4">
    <div class="flex items-center gap-3">
      <div class="flex h-10 w-10 items-center justify-center rounded-xl border border-indigo-500/30 bg-indigo-500/15 text-indigo-300">
        <Blocks class="h-5 w-5" />
      </div>
      <div>
        <div class="flex items-center gap-2">
          <h1 class="text-base font-semibold tracking-tight text-zinc-100">
            Extensions Marketplace & Contribution Manager
          </h1>
          <span class="rounded-full border border-indigo-500/30 bg-indigo-500/15 px-2 py-0.5 font-mono text-[11px] font-medium text-indigo-200">
            {extensionRegistry.enabledExtensions.length} / {extensionRegistry.extensions.length} Active
          </span>
        </div>
        <p class="text-xs text-zinc-400">
          Manage first-party modular capabilities and custom external plugins (<code class="font-mono text-zinc-300">~/.pipan/extensions/</code>)
        </p>
      </div>
    </div>

    <div class="flex flex-wrap items-center gap-2">
      <button
        type="button"
        onclick={() => void extensionRegistry.refreshExternalExtensions()}
        disabled={extensionRegistry.loading}
        class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-700/80 bg-zinc-800/80 px-3 py-1.5 text-xs font-medium text-zinc-200 transition hover:border-zinc-600 hover:bg-zinc-700/80 disabled:opacity-50 cursor-pointer"
        title="Rescan ~/.pipan/extensions and app_local_data_dir/extensions"
      >
        <RefreshCw class="h-3.5 w-3.5 {extensionRegistry.loading ? 'animate-spin text-indigo-400' : 'text-zinc-400'}" />
        <span>Rescan Plugins</span>
      </button>

      <button
        type="button"
        onclick={() => {
          extensionRegistry.resetAllBuiltins();
          statusBanner = {
            type: 'success',
            message: 'All 5 built-in extensions restored and enabled.'
          };
        }}
        class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-700/80 bg-zinc-800/80 px-3 py-1.5 text-xs font-medium text-zinc-200 transition hover:border-zinc-600 hover:bg-zinc-700/80 cursor-pointer"
        title="Re-enable and reinstall all 5 first-party extensions"
      >
        <RotateCcw class="h-3.5 w-3.5 text-emerald-400" />
        <span>Restore Built-ins</span>
      </button>

      <button
        type="button"
        onclick={() => workspaceStore.openChatView()}
        class="inline-flex items-center gap-1.5 rounded-lg bg-indigo-600 px-3 py-1.5 text-xs font-semibold text-white shadow transition hover:bg-indigo-500 cursor-pointer"
      >
        <MessageSquare class="h-3.5 w-3.5" />
        <span>Back to Session</span>
      </button>
    </div>
  </div>

  <div class="mx-auto w-full max-w-6xl space-y-6 p-6">
    <!-- Status / Error Banner -->
    {#if statusBanner}
      <div
        class="flex items-center justify-between gap-3 rounded-xl border px-4 py-2.5 text-xs {statusBanner.type ===
        'error'
          ? 'border-rose-500/40 bg-rose-950/40 text-rose-200'
          : 'border-emerald-500/40 bg-emerald-950/40 text-emerald-200'}"
      >
        <div class="flex items-center gap-2">
          {#if statusBanner.type === 'error'}
            <AlertCircle class="h-4 w-4 shrink-0 text-rose-400" />
          {:else}
            <CheckCircle2 class="h-4 w-4 shrink-0 text-emerald-400" />
          {/if}
          <span>{statusBanner.message}</span>
        </div>
        <button
          type="button"
          onclick={() => (statusBanner = null)}
          class="rounded px-2 py-0.5 text-[11px] opacity-80 hover:opacity-100 cursor-pointer"
        >
          Dismiss
        </button>
      </div>
    {/if}

    <!-- Install External Plugin Controls (Folder + Git Repo) -->
    <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
      <!-- Install from Folder -->
      <div class="rounded-xl border border-zinc-800/90 bg-zinc-900/50 p-4 space-y-2.5">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2 text-xs font-semibold text-zinc-100">
            <FolderOpen class="h-4 w-4 text-sky-400" />
            <span>Install Extension from Local Folder</span>
          </div>
          <button
            type="button"
            onclick={() => void handleBrowseAndInstallFolder()}
            disabled={isPickingFolder || extensionRegistry.loading}
            class="inline-flex items-center gap-1.5 rounded-lg border border-sky-500/30 bg-sky-500/15 px-2.5 py-1 text-[11px] font-medium text-sky-200 transition hover:bg-sky-500/25 disabled:opacity-50 cursor-pointer"
          >
            <FolderOpen class="h-3 w-3" />
            <span>Browse Folder…</span>
          </button>
        </div>

        <p class="text-[11px] text-zinc-400">
          Copies a plugin directory containing <code class="font-mono text-zinc-300">pipan-extension.json</code> into <code class="font-mono text-zinc-300">~/.pipan/extensions/&lt;ext-id&gt;/</code>.
        </p>

        <div class="flex items-center gap-2">
          <input
            type="text"
            bind:value={folderPathInput}
            placeholder="/path/to/my-pipan-plugin"
            aria-label="Extension folder path"
            onkeydown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                void handleInstallFolderByPath();
              }
            }}
            class="min-w-0 flex-1 rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100 placeholder-zinc-500 focus:border-indigo-500/60 focus:outline-none"
          />
          <button
            type="button"
            onclick={() => void handleInstallFolderByPath()}
            disabled={!folderPathInput.trim() || extensionRegistry.loading}
            class="inline-flex shrink-0 items-center gap-1.5 rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-1.5 text-xs font-medium text-zinc-100 transition hover:bg-zinc-700 disabled:opacity-40 cursor-pointer"
          >
            <Download class="h-3.5 w-3.5 text-sky-400" />
            <span>Install Folder</span>
          </button>
        </div>
      </div>

      <!-- Install from Git URL -->
      <div class="rounded-xl border border-zinc-800/90 bg-zinc-900/50 p-4 space-y-2.5">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2 text-xs font-semibold text-zinc-100">
            <GitBranch class="h-4 w-4 text-emerald-400" />
            <span>Install Extension from Git Repository</span>
          </div>
          <span class="rounded bg-zinc-800 px-2 py-0.5 font-mono text-[10px] text-zinc-400">
            git clone --depth 1
          </span>
        </div>

        <p class="text-[11px] text-zinc-400">
          Clones an extension repository from GitHub (<code class="font-mono text-zinc-300">https://github.com/owner/repo.git</code> or <code class="font-mono text-zinc-300">owner/repo</code>).
        </p>

        <div class="flex items-center gap-2">
          <input
            type="text"
            bind:value={gitRepoUrlInput}
            placeholder="https://github.com/owner/pipan-extension.git"
            aria-label="Extension Git repository URL"
            onkeydown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                void handleInstallFromGit();
              }
            }}
            class="min-w-0 flex-1 rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100 placeholder-zinc-500 focus:border-indigo-500/60 focus:outline-none"
          />
          <button
            type="button"
            onclick={() => void handleInstallFromGit()}
            disabled={!gitRepoUrlInput.trim() || extensionRegistry.loading}
            class="inline-flex shrink-0 items-center gap-1.5 rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-1.5 text-xs font-medium text-zinc-100 transition hover:bg-zinc-700 disabled:opacity-40 cursor-pointer"
          >
            {#if extensionRegistry.loading}
              <Loader2 class="h-3.5 w-3.5 animate-spin text-emerald-400" />
            {:else}
              <Download class="h-3.5 w-3.5 text-emerald-400" />
            {/if}
            <span>Clone & Install</span>
          </button>
        </div>
      </div>
    </div>

    <!-- Search & Filter Bar -->
    <div class="flex flex-wrap items-center justify-between gap-3">
      <div class="relative min-w-[260px] flex-1">
        <Search class="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-zinc-400" />
        <input
          type="search"
          bind:value={searchQuery}
          placeholder="Search extensions by name, ID, panel, view, or slash command…"
          aria-label="Search extensions"
          class="w-full rounded-xl border border-zinc-800 bg-zinc-900/80 pl-9 pr-3 py-2 text-xs text-zinc-100 placeholder-zinc-500 focus:border-indigo-500/60 focus:outline-none"
        />
      </div>

      <div class="flex flex-wrap items-center gap-1.5" role="group" aria-label="Filter extensions">
        {#each [
          { id: 'all', label: `All (${extensionRegistry.extensions.length})` },
          { id: 'builtin', label: `Built-in (${extensionRegistry.builtinExtensions.length})` },
          { id: 'external', label: `External (${extensionRegistry.externalExtensions.length})` },
          { id: 'enabled', label: `Enabled (${extensionRegistry.enabledExtensions.length})` },
          { id: 'disabled', label: 'Disabled' }
        ] as pill (pill.id)}
          {@const active = filterTab === pill.id}
          <button
            type="button"
            onclick={() => (filterTab = pill.id as FilterTab)}
            aria-pressed={active}
            class="rounded-lg border px-2.5 py-1.5 text-xs font-medium transition cursor-pointer {active
              ? 'border-indigo-500/50 bg-indigo-500/20 text-indigo-200'
              : 'border-zinc-800 bg-zinc-900/70 text-zinc-400 hover:border-zinc-700 hover:text-zinc-200'}"
          >
            {pill.label}
          </button>
        {/each}
      </div>
    </div>

    <!-- Extensions Grid -->
    {#if filteredExtensions.length === 0}
      <div class="rounded-2xl border border-zinc-800/80 bg-zinc-900/40 p-10 text-center space-y-2">
        <Puzzle class="mx-auto h-7 w-7 text-zinc-500" />
        <p class="text-sm font-medium text-zinc-200">No matching extensions</p>
        <p class="text-xs text-zinc-400">
          Try clearing your search query or install a custom extension from a local folder or GitHub repository above.
        </p>
      </div>
    {:else}
      <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
        {#each filteredExtensions as ext (ext.manifest.id)}
          {@const Icon = primaryIconForExtension(ext)}
          {@const m = ext.manifest}
          {@const isActive = ext.installed && ext.enabled}
          <article
            class="flex flex-col justify-between rounded-2xl border p-4 transition {isActive
              ? 'border-zinc-800 bg-zinc-900/70 hover:border-zinc-700'
              : 'border-zinc-800/50 bg-zinc-950/70 opacity-80'}"
          >
            <div class="space-y-3">
              <!-- Card Header -->
              <div class="flex items-start justify-between gap-3">
                <div class="flex min-w-0 items-start gap-3">
                  <div
                    class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border {isActive
                      ? 'border-indigo-500/30 bg-indigo-500/15 text-indigo-300'
                      : 'border-zinc-800 bg-zinc-900 text-zinc-500'}"
                  >
                    <Icon class="h-5 w-5" />
                  </div>
                  <div class="min-w-0">
                    <div class="flex flex-wrap items-center gap-1.5">
                      <h2 class="truncate text-sm font-semibold text-zinc-100">{m.name}</h2>
                      <span class="rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-[10px] text-zinc-300">
                        v{m.version}
                      </span>
                      {#if ext.builtin}
                        <span class="rounded border border-indigo-500/30 bg-indigo-500/10 px-1.5 py-0.5 text-[10px] font-medium text-indigo-300">
                          Built-in
                        </span>
                      {:else}
                        <span class="rounded border border-sky-500/30 bg-sky-500/10 px-1.5 py-0.5 text-[10px] font-medium text-sky-300">
                          External
                        </span>
                      {/if}
                    </div>
                    <div class="mt-0.5 flex flex-wrap items-center gap-2 text-[11px] text-zinc-400">
                      <code class="font-mono text-zinc-300">{m.id}</code>
                      <span>·</span>
                      <span>by {m.author}</span>
                    </div>
                  </div>
                </div>

                <!-- Status Pill -->
                <span
                  class="shrink-0 rounded-full border px-2 py-0.5 text-[10px] font-medium {!ext.installed
                    ? 'border-zinc-700 bg-zinc-900 text-zinc-400'
                    : ext.enabled
                      ? 'border-emerald-500/30 bg-emerald-500/15 text-emerald-300'
                      : 'border-amber-500/30 bg-amber-500/10 text-amber-300'}"
                >
                  {!ext.installed ? 'Uninstalled' : ext.enabled ? 'Enabled' : 'Disabled'}
                </span>
              </div>

              <!-- Description -->
              <p class="text-xs leading-relaxed text-zinc-300">
                {m.description}
              </p>

              <!-- Contribution Points Badges -->
              <div class="space-y-1.5 rounded-xl border border-zinc-800/80 bg-zinc-950/70 p-2.5 text-[11px]">
                <div class="text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
                  Contributes
                </div>
                <div class="flex flex-wrap gap-1.5">
                  {#each m.contributes.panels ?? [] as panel (panel.id)}
                    <button
                      type="button"
                      disabled={!isActive}
                      onclick={() => {
                        workspaceStore.openChatView();
                        workspaceStore.openPanel(panel.id);
                      }}
                      class="inline-flex items-center gap-1 rounded-md border border-indigo-500/30 bg-indigo-500/10 px-2 py-0.5 text-[11px] text-indigo-200 transition hover:bg-indigo-500/20 disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer"
                      title={isActive ? `Click to open ${panel.title} panel` : 'Enable extension to open panel'}
                    >
                      <span>Panel: {panel.title}</span>
                      {#if isActive}
                        <ExternalLink class="h-2.5 w-2.5 opacity-75" />
                      {/if}
                    </button>
                  {/each}

                  {#each m.contributes.sidebarTabs ?? [] as tab (tab.id)}
                    <span class="inline-flex items-center gap-1 rounded-md border border-sky-500/30 bg-sky-500/10 px-2 py-0.5 text-[11px] text-sky-200">
                      Sidebar: {tab.title}
                    </span>
                  {/each}

                  {#each m.contributes.mainViews ?? [] as view (view.id)}
                    <button
                      type="button"
                      disabled={!isActive}
                      onclick={() => workspaceStore.openMainView(view.id)}
                      class="inline-flex items-center gap-1 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-0.5 text-[11px] text-amber-200 transition hover:bg-amber-500/20 disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer"
                      title={isActive ? `Click to open ${view.title} view` : 'Enable extension to open view'}
                    >
                      <span>View: {view.title}</span>
                      {#if isActive}
                        <ExternalLink class="h-2.5 w-2.5 opacity-75" />
                      {/if}
                    </button>
                  {/each}

                  {#each m.contributes.statusBarItems ?? [] as item (item.id)}
                    <span class="inline-flex items-center gap-1 rounded-md border border-emerald-500/30 bg-emerald-500/10 px-2 py-0.5 text-[11px] text-emerald-200">
                      Status Bar: {item.id} ({item.alignment})
                    </span>
                  {/each}

                  {#each m.contributes.slashCommands ?? [] as cmd (cmd.name)}
                    <span
                      class="inline-flex items-center gap-1 rounded-md border border-zinc-700 bg-zinc-800/80 px-2 py-0.5 font-mono text-[11px] text-zinc-200"
                      title={cmd.description}
                    >
                      <Sparkles class="h-2.5 w-2.5 text-indigo-400" />
                      /{cmd.name}
                    </span>
                  {/each}
                </div>
              </div>

              {#if ext.installPath}
                <div class="truncate font-mono text-[10px] text-zinc-500" title={ext.installPath}>
                  Path: {ext.installPath}
                </div>
              {/if}

              {#if ext.error}
                <div class="rounded-lg border border-rose-500/40 bg-rose-950/30 px-2.5 py-1.5 text-[11px] text-rose-300">
                  Plugin script error: {ext.error}
                </div>
              {/if}
            </div>

            <!-- Card Footer Actions -->
            <div class="mt-4 flex items-center justify-between gap-2 border-t border-zinc-800/70 pt-3">
              <div class="text-[11px] text-zinc-400">
                {#if ext.aliases.length > 0}
                  <span>Aliases: <code class="font-mono text-zinc-300">{ext.aliases[0]}</code></span>
                {:else if ext.sourceUrl}
                  <span class="truncate font-mono">{ext.sourceUrl}</span>
                {:else}
                  <span>Manifest v1</span>
                {/if}
              </div>

              <div class="flex items-center gap-2">
                {#if !ext.installed && ext.builtin}
                  <button
                    type="button"
                    onclick={() => extensionRegistry.installBuiltinExtension(m.id)}
                    class="inline-flex items-center gap-1.5 rounded-lg bg-indigo-600 px-3 py-1.5 text-xs font-semibold text-white transition hover:bg-indigo-500 cursor-pointer"
                  >
                    <Download class="h-3.5 w-3.5" />
                    <span>Install</span>
                  </button>
                {:else}
                  <button
                    type="button"
                    onclick={() => extensionRegistry.toggleExtension(m.id)}
                    class="inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs font-medium transition cursor-pointer {ext.enabled
                      ? 'border-zinc-700 bg-zinc-800 text-zinc-200 hover:bg-zinc-700'
                      : 'border-emerald-500/40 bg-emerald-500/15 text-emerald-200 hover:bg-emerald-500/25'}"
                  >
                    <Power class="h-3.5 w-3.5 {ext.enabled ? 'text-amber-400' : 'text-emerald-400'}" />
                    <span>{ext.enabled ? 'Disable' : 'Enable'}</span>
                  </button>

                  <button
                    type="button"
                    onclick={() => void handleUninstall(ext)}
                    class="inline-flex items-center gap-1 rounded-lg border border-rose-500/30 bg-rose-500/10 px-2.5 py-1 text-xs font-medium text-rose-300 transition hover:bg-rose-500/20 cursor-pointer"
                    title={ext.builtin ? 'Uninstall built-in extension (can be reinstalled)' : 'Remove external extension'}
                  >
                    <Trash2 class="h-3.5 w-3.5" />
                    <span>Uninstall</span>
                  </button>
                {/if}
              </div>
            </div>
          </article>
        {/each}
      </div>
    {/if}

    <!-- Developer Host API Reference -->
    <div class="rounded-2xl border border-zinc-800/80 bg-zinc-900/40 p-4 space-y-2 text-xs">
      <div class="flex items-center gap-2 font-semibold text-zinc-200">
        <Terminal class="h-4 w-4 text-indigo-400" />
        <span>External Plugin Host API (<code class="font-mono text-indigo-300">window.pipan.registerExtension</code>)</span>
      </div>
      <p class="text-[11px] leading-relaxed text-zinc-400">
        Custom extensions in <code class="font-mono text-zinc-300">~/.pipan/extensions/&lt;ext-id&gt;/</code> declare contribution points in <code class="font-mono text-zinc-300">pipan-extension.json</code> and can optionally provide an <code class="font-mono text-zinc-300">entry</code> script that registers custom DOM panel renderers, sidebar tabs, status bar widgets, and slash command handlers via <code class="font-mono text-zinc-300">window.pipan.registerExtension(&#123; id, panels, sidebarTabs, mainViews, statusBarItems, slashCommands &#125;)</code>.
      </p>
    </div>
  </div>
</section>
