<script lang="ts">
  import { onMount } from 'svelte';
  import { Plus, X, Columns2, Rows2, Check } from 'lucide-svelte';
  import { SvelteFlowProvider } from '@xyflow/svelte';
  import { workspaceStore, type PanelId } from '#lib/stores/workspace.svelte.ts';
  import { getPanelDefinition } from '#lib/workspace/panels.ts';
  import { extensionRegistry } from '#lib/extensions/registry.svelte.ts';
  import FlowCanvas from './FlowCanvas.svelte';
  import FileExplorerPanel from './FileExplorerPanel.svelte';
  import PanelPlaceholder from './PanelPlaceholder.svelte';
  import PullRequestsPanel from './PullRequestsPanel.svelte';
  import TerminalPanel from './TerminalPanel.svelte';
  import ExtensionContributionHost from './ExtensionContributionHost.svelte';

  let showAddDropdown = $state(false);
  let dropdownRef = $state<HTMLDivElement | null>(null);

  function toggleAddDropdown() {
    showAddDropdown = !showAddDropdown;
  }

  function handleSelectPanel(id: PanelId) {
    workspaceStore.openPanel(id);
    showAddDropdown = false;
  }

  function handleDocumentClick(e: MouseEvent) {
    if (showAddDropdown && dropdownRef && !dropdownRef.contains(e.target as Node)) {
      showAddDropdown = false;
    }
  }

  function handleDocumentKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && showAddDropdown) {
      showAddDropdown = false;
    }
  }

  onMount(() => {
    document.addEventListener('click', handleDocumentClick, true);
    document.addEventListener('keydown', handleDocumentKeydown);
    return () => {
      document.removeEventListener('click', handleDocumentClick, true);
      document.removeEventListener('keydown', handleDocumentKeydown);
    };
  });

  function onTabKeydown(e: KeyboardEvent) {
    const ids = workspaceStore.openPanels;
    if (ids.length === 0) return;
    const current = workspaceStore.activePanel ? ids.indexOf(workspaceStore.activePanel) : -1;
    let next = -1;
    if (e.key === 'ArrowRight') next = (current + 1) % ids.length;
    else if (e.key === 'ArrowLeft') next = (current - 1 + ids.length) % ids.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = ids.length - 1;
    if (next === -1) return;
    e.preventDefault();
    workspaceStore.setActivePanel(ids[next]);
    document.getElementById(`panel-tab-${ids[next]}`)?.focus();
  }
</script>

<section aria-label="Session panels" class="flex h-full min-h-0 min-w-0 flex-col bg-zinc-950">
  <!-- Tab bar with open tabs, + add dropdown, and panel controls -->
  <div
    role="tablist"
    aria-label="Open panels"
    tabindex="-1"
    onkeydown={onTabKeydown}
    class="flex shrink-0 items-center justify-between border-b border-zinc-800/80 bg-zinc-900/80 px-2 pt-1 select-none min-h-[36px]"
  >
    <div class="flex items-end gap-1 overflow-x-auto min-w-0 flex-1">
      {#each workspaceStore.openPanels as id (id)}
        {@const def = getPanelDefinition(id)}
        {@const Icon = def.icon}
        {@const active = workspaceStore.activePanel === id}
        <div
          class="group flex items-center rounded-t-md border border-b-0 text-xs {active
            ? 'border-zinc-700 bg-zinc-950 text-zinc-100'
            : 'border-transparent text-zinc-400 hover:bg-zinc-800/60 hover:text-zinc-200'}"
        >
          <button
            type="button"
            role="tab"
            id="panel-tab-{id}"
            aria-selected={active}
            aria-controls="panel-body-{id}"
            tabindex={active ? 0 : -1}
            onclick={() => workspaceStore.setActivePanel(id)}
            class="inline-flex items-center gap-1.5 py-1.5 pl-2.5 pr-1 font-medium cursor-pointer"
          >
            <Icon class="h-3.5 w-3.5 {active ? 'text-indigo-400' : ''}" />
            <span>{def.label}</span>
          </button>
          <button
            type="button"
            onclick={() => workspaceStore.closePanel(id)}
            class="mr-1 rounded p-0.5 text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200 cursor-pointer"
            title="Close {def.label}"
            aria-label="Close {def.label}"
          >
            <X class="h-3 w-3" />
          </button>
        </div>
      {/each}

      <!-- Plus button with dropdown popover -->
      <div class="relative pb-0.5" bind:this={dropdownRef}>
        <button
          type="button"
          onclick={toggleAddDropdown}
          aria-haspopup="menu"
          aria-expanded={showAddDropdown}
          class="inline-flex h-6 w-6 items-center justify-center rounded text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100 transition cursor-pointer {showAddDropdown
            ? 'bg-zinc-800 text-zinc-100 ring-1 ring-zinc-700'
            : ''}"
          title="Add panel"
          aria-label="Add panel"
        >
          <Plus class="h-3.5 w-3.5" />
        </button>

        {#if showAddDropdown}
          <div
            role="menu"
            class="absolute left-0 top-full mt-1.5 z-50 w-56 rounded-lg border border-zinc-700/80 bg-zinc-900 p-1 shadow-xl ring-1 ring-black/40 backdrop-blur-md"
          >
            <div class="px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
              Load Panel
            </div>
            {#each extensionRegistry.contributedPanels as panel (panel.id)}
              {@const Icon = panel.iconComponent}
              {@const isOpen = workspaceStore.isOpen(panel.id)}
              <button
                type="button"
                role="menuitem"
                onclick={() => handleSelectPanel(panel.id)}
                class="flex w-full items-center justify-between gap-2 rounded-md px-2 py-1.5 text-left text-xs text-zinc-200 hover:bg-zinc-800 hover:text-zinc-100 transition cursor-pointer"
              >
                <div class="flex items-center gap-2 min-w-0">
                  <Icon class="h-3.5 w-3.5 text-indigo-400 shrink-0" />
                  <span class="truncate">{panel.label}</span>
                </div>
                {#if isOpen}
                  <Check class="h-3.5 w-3.5 text-indigo-400 shrink-0" />
                {/if}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    </div>

    <!-- Panel controls: split orientation & close panel area -->
    <div class="flex items-center gap-0.5 pl-2 pb-0.5">
      <button
        type="button"
        onclick={() => workspaceStore.toggleOrientation()}
        class="inline-flex h-6 w-6 items-center justify-center rounded text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 transition cursor-pointer"
        title={workspaceStore.orientation === 'vertical'
          ? 'Switch to top/bottom split'
          : 'Switch to side-by-side split'}
        aria-label="Toggle split orientation"
      >
        {#if workspaceStore.orientation === 'vertical'}
          <Columns2 class="h-3.5 w-3.5" />
        {:else}
          <Rows2 class="h-3.5 w-3.5" />
        {/if}
      </button>

      <button
        type="button"
        onclick={() => workspaceStore.closePanelArea()}
        class="inline-flex h-6 w-6 items-center justify-center rounded text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 transition cursor-pointer"
        title="Close side panel"
        aria-label="Close side panel"
      >
        <X class="h-3.5 w-3.5" />
      </button>
    </div>
  </div>

  <!-- Panel Body: active panel component or empty placeholder state -->
  {#if workspaceStore.activePanel && workspaceStore.isOpen(workspaceStore.activePanel)}
    {@const id = workspaceStore.activePanel}
    <div
      role="tabpanel"
      id="panel-body-{id}"
      aria-labelledby="panel-tab-{id}"
      class="relative flex min-h-0 flex-1 flex-col"
    >
      {#if id === 'nodes' && extensionRegistry.isPanelEnabled('nodes')}
        <SvelteFlowProvider>
          <FlowCanvas />
        </SvelteFlowProvider>
      {:else if id === 'terminal' && extensionRegistry.isPanelEnabled('terminal')}
        <TerminalPanel />
      {:else if id === 'files' && extensionRegistry.isPanelEnabled('files')}
        <FileExplorerPanel />
      {:else if id === 'prs' && extensionRegistry.isPanelEnabled('prs')}
        <PullRequestsPanel />
      {:else if extensionRegistry.contributedPanelsById[id]}
        {@const contributed = extensionRegistry.contributedPanelsById[id]}
        <ExtensionContributionHost
          extensionId={contributed.extensionId}
          contributionId={contributed.id}
          title={contributed.label}
          description={contributed.description}
          kind="panel"
        />
      {:else}
        <PanelPlaceholder {id} />
      {/if}
    </div>
  {:else}
    <!-- Empty panel state when opened with nothing -->
    <div class="flex flex-1 flex-col items-center justify-center p-6 text-center text-zinc-400">
      <div class="mb-3 rounded-full bg-zinc-900 border border-zinc-800 p-3 text-zinc-400">
        <Plus class="h-6 w-6 text-zinc-400" />
      </div>
      <h3 class="text-sm font-medium text-zinc-200">No panel loaded</h3>
      <p class="mt-1 max-w-xs text-xs text-zinc-400">
        Select the <strong class="text-zinc-300">+</strong> button above or choose a panel below to load:
      </p>
      <div class="mt-4 flex flex-col gap-1.5 w-full max-w-xs">
        {#each extensionRegistry.contributedPanels as panel (panel.id)}
          {@const Icon = panel.iconComponent}
          <button
            type="button"
            onclick={() => handleSelectPanel(panel.id)}
            class="flex items-center gap-2.5 rounded-lg border border-zinc-800 bg-zinc-900/80 px-3 py-2 text-left text-xs transition hover:border-zinc-700 hover:bg-zinc-800 text-zinc-200 cursor-pointer"
          >
            <Icon class="h-4 w-4 text-indigo-400 shrink-0" />
            <div class="min-w-0 flex-1">
              <div class="font-medium text-zinc-100">{panel.label}</div>
              {#if panel.description}
                <div class="truncate text-[10px] text-zinc-400">{panel.description}</div>
              {/if}
            </div>
            <Plus class="h-3 w-3 text-zinc-500 shrink-0" />
          </button>
        {/each}
      </div>
    </div>
  {/if}
</section>