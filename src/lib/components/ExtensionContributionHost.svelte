<script lang="ts">
  import { Puzzle, Blocks } from 'lucide-svelte';
  import { extensionRegistry } from '#lib/extensions/registry.svelte.ts';
  import { workspaceStore } from '#lib/stores/workspace.svelte.ts';

  let {
    extensionId,
    contributionId,
    title,
    description = '',
    kind
  }: {
    extensionId: string;
    contributionId: string;
    title: string;
    description?: string;
    kind: 'panel' | 'sidebarTab' | 'mainView';
  } = $props();

  let mountEl = $state<HTMLDivElement | null>(null);

  const ext = $derived(extensionRegistry.getExtension(extensionId));
  const runtimeReg = $derived(extensionRegistry.runtimeRegistrations[extensionId]);

  const customHandler = $derived.by(() => {
    if (!runtimeReg) return null;
    if (kind === 'panel') return runtimeReg.panels?.[contributionId] ?? null;
    if (kind === 'sidebarTab') return runtimeReg.sidebarTabs?.[contributionId] ?? null;
    return runtimeReg.mainViews?.[contributionId] ?? null;
  });

  $effect(() => {
    if (!mountEl || typeof customHandler !== 'function' || !ext) return;
    mountEl.innerHTML = '';
    const cleanup = customHandler(mountEl, {
      extensionId: ext.manifest.id,
      contributionId,
      kind,
      manifest: ext.manifest
    });
    return () => {
      if (typeof cleanup === 'function') {
        cleanup();
      }
    };
  });
</script>

<div class="flex h-full min-h-0 flex-1 flex-col overflow-auto bg-zinc-950 p-4 text-xs text-zinc-200">
  {#if typeof customHandler === 'function'}
    <div bind:this={mountEl} class="plugin-surface min-h-0 flex-1"></div>
  {:else if customHandler && typeof customHandler === 'object'}
    <div class="rounded-xl border border-zinc-800 bg-zinc-900/70 p-4 space-y-2">
      <div class="flex items-center justify-between gap-2">
        <div class="flex items-center gap-2 font-semibold text-zinc-100">
          <Puzzle class="h-4 w-4 text-indigo-400" />
          <span>{title}</span>
        </div>
        {#if ext}
          <span class="rounded bg-zinc-800 px-2 py-0.5 font-mono text-[10px] text-zinc-400">
            {ext.manifest.id} v{ext.manifest.version}
          </span>
        {/if}
      </div>
      {#if customHandler.text}
        <p class="whitespace-pre-wrap text-zinc-300 leading-relaxed">{customHandler.text}</p>
      {/if}
    </div>
  {:else}
    <div class="flex flex-1 items-center justify-center p-6">
      <div class="max-w-sm rounded-2xl border border-zinc-800/80 bg-zinc-900/60 p-6 text-center space-y-3">
        <div class="mx-auto flex h-11 w-11 items-center justify-center rounded-xl border border-indigo-500/30 bg-indigo-500/15 text-indigo-300">
          <Puzzle class="h-5 w-5" />
        </div>
        <div class="space-y-1">
          <h3 class="text-sm font-semibold text-zinc-100">{title}</h3>
          {#if description}
            <p class="text-xs text-zinc-400 leading-relaxed">{description}</p>
          {/if}
        </div>
        {#if ext}
          <div class="rounded-lg border border-zinc-800 bg-zinc-950/80 px-3 py-2 text-[11px] text-zinc-400 font-mono">
            Contributed by <span class="text-zinc-200">{ext.manifest.name}</span> ({ext.manifest.id} v{ext.manifest.version})
          </div>
        {/if}
        <button
          type="button"
          onclick={() => workspaceStore.openExtensionsView()}
          class="inline-flex items-center justify-center gap-1.5 rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-1.5 text-xs font-medium text-zinc-100 transition hover:bg-zinc-700 cursor-pointer"
        >
          <Blocks class="h-3.5 w-3.5 text-indigo-400" />
          <span>Manage in Extensions</span>
        </button>
      </div>
    </div>
  {/if}
</div>
