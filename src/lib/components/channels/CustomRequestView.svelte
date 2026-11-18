<script lang="ts">
  // Hands the request body to a plugin's own renderer. The plugin draws into a container and
  // finishes the request through `answer` or `cancel`.
  import type { ChannelRequestHandler } from '#lib/channels/plugin-api.svelte.ts';
  import { channels, type PendingRequest } from '#lib/stores/channels.svelte.ts';

  let { request, handler }: { request: PendingRequest; handler: ChannelRequestHandler } = $props();

  let container = $state<HTMLDivElement | null>(null);
  let failure = $state<string | null>(null);

  $effect(() => {
    if (!container) return;
    container.replaceChildren();
    failure = null;
    let cleanup: void | (() => void);
    try {
      cleanup = handler(container, {
        ch: request.ch,
        kind: request.kind,
        payload: request.payload,
        answer: (payload) => channels.answer(request.sessionId, request.requestId, payload),
        cancel: () => channels.cancel(request.sessionId, request.requestId)
      });
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
    }
    return () => {
      if (typeof cleanup === 'function') cleanup();
    };
  });
</script>

{#if failure}
  <div role="alert" class="px-4 py-4 text-xs text-red-300">
    The renderer from <code class="rounded bg-zinc-800 px-1">{request.ch}</code> failed: {failure}. Cancel to continue.
  </div>
{/if}
<div bind:this={container} class="plugin-surface min-h-0 flex-auto overflow-y-auto px-4 py-3 text-xs text-zinc-300"></div>
