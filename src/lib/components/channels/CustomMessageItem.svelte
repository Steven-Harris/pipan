<script lang="ts">
  // A log message drawn by the plugin that owns its channel.
  import { extensionRegistry } from '#lib/extensions/registry.svelte.ts';
  import type { LogItem } from '#lib/stores/channels.svelte.ts';

  let { item }: { item: LogItem } = $props();

  let container = $state<HTMLDivElement | null>(null);
  let failure = $state<string | null>(null);
  const renderer = $derived(extensionRegistry.getMessageRenderer(item.ch, item.name));

  $effect(() => {
    if (!container || !renderer) return;
    container.replaceChildren();
    failure = null;
    let cleanup: void | (() => void);
    try {
      cleanup = renderer(
        { ch: item.ch, name: item.name, data: item.data, content: item.content, timestamp: item.timestamp },
        container
      );
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
    }
    return () => {
      if (typeof cleanup === 'function') cleanup();
    };
  });
</script>

<div class="py-2">
  <div class="mb-1 text-[10px] text-zinc-600">{item.ch}</div>
  {#if failure}
    <p class="text-xs text-red-300">The message renderer from {item.ch} failed: {failure}</p>
  {/if}
  <div bind:this={container} class="plugin-surface text-sm text-zinc-300"></div>
</div>
