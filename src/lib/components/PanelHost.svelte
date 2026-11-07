<script lang="ts">
  import { X } from 'lucide-svelte';
  import { SvelteFlowProvider } from '@xyflow/svelte';
  import { workspaceStore } from '#lib/stores/workspace.svelte.ts';
  import { PANEL_BY_ID } from '#lib/workspace/panels.ts';
  import FlowCanvas from './FlowCanvas.svelte';
  import PanelPlaceholder from './PanelPlaceholder.svelte';
  import PullRequestsPanel from './PullRequestsPanel.svelte';

  function onTabKeydown(e: KeyboardEvent) {
    const ids = workspaceStore.openPanels;
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
  <div
    role="tablist"
    aria-label="Open panels"
    tabindex="-1"
    onkeydown={onTabKeydown}
    class="flex shrink-0 items-end gap-0.5 overflow-x-auto border-b border-zinc-800/80 bg-zinc-900/80 px-2 pt-1.5 select-none"
  >
    {#each workspaceStore.openPanels as id (id)}
      {@const def = PANEL_BY_ID[id]}
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
  </div>

  {#if workspaceStore.activePanel}
    {@const id = workspaceStore.activePanel}
    <div
      role="tabpanel"
      id="panel-body-{id}"
      aria-labelledby="panel-tab-{id}"
      class="relative flex min-h-0 flex-1 flex-col"
    >
      {#if id === 'nodes'}
        <SvelteFlowProvider>
          <FlowCanvas />
        </SvelteFlowProvider>
      {:else if id === 'prs'}
        <PullRequestsPanel />
      {:else}
        <PanelPlaceholder {id} />
      {/if}
    </div>
  {/if}
</section>
