<script lang="ts">
  import { getPanelDefinition } from '#lib/workspace/panels.ts';
  import type { PanelId } from '#lib/stores/workspace.svelte.ts';

  let { id }: { id: PanelId } = $props();
  const def = $derived(getPanelDefinition(id));
  const Icon = $derived(def.icon);
</script>

<div class="flex h-full items-center justify-center bg-zinc-950 p-6">
  <div class="max-w-xs text-center">
    <div class="mx-auto mb-3 flex h-10 w-10 items-center justify-center rounded-xl border border-zinc-800 bg-zinc-900 text-zinc-400">
      <Icon class="h-5 w-5" />
    </div>
    <h3 class="text-sm font-medium text-zinc-100">{def.label}</h3>
    <p class="mt-1 text-xs text-zinc-400">{def.description}.</p>
    <p class="mt-3 text-[11px] text-zinc-500">This panel is not available yet.</p>
  </div>
</div>
