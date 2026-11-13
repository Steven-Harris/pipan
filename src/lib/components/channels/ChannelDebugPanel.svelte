<script lang="ts">
  // Development-only view of the extension channel store: live state, the request queue, the
  // durable log and producer errors. Mounted by the root layout only when `import.meta.env.DEV`.
  import { channels } from '#lib/stores/channels.svelte.ts';
  import { chatStore } from '#lib/stores/chat.svelte.ts';

  let open = $state(false);

  const sessionId = $derived(chatStore.activeSessionId ?? '');
  const host = $derived(channels.hosts[sessionId]);
  const states = $derived(channels.statesFor(sessionId));
  const pending = $derived(channels.pending[sessionId] ?? []);
  const log = $derived(channels.log[sessionId] ?? []);

  const preview = (value: unknown): string => {
    const text = JSON.stringify(value) ?? '';
    return text.length > 160 ? `${text.slice(0, 160)}... (${text.length} chars)` : text;
  };
</script>

<div class="fixed right-3 bottom-8 z-50 text-xs">
  <button
    type="button"
    class="rounded border border-zinc-600 bg-zinc-900/90 px-2 py-1 text-zinc-200"
    onclick={() => (open = !open)}
  >
    Channels {host ? `(shim, ${states.length} states, ${pending.length} pending)` : '(no shim)'}
  </button>

  {#if open}
    <div
      class="mt-1 max-h-96 w-[34rem] overflow-auto rounded border border-zinc-600 bg-zinc-900/95 p-2 font-mono text-zinc-200"
    >
      <p class="mb-1 text-zinc-400">session {sessionId || '(none)'}; host {host ? preview(host) : 'not announced'}</p>

      <h3 class="mt-2 font-semibold">State</h3>
      {#each states as state (state.ch + state.topic)}
        <div class="border-t border-zinc-700 py-1">
          <div>{state.ch} / {state.topic} [{state.primitive}] seq {state.seq ?? '-'}</div>
          <div class="text-zinc-400">{preview(state.data)}</div>
          {#each state.notices as notice (notice)}
            <div class="text-amber-400">{notice}</div>
          {/each}
        </div>
      {:else}
        <div class="text-zinc-500">none</div>
      {/each}

      <h3 class="mt-2 font-semibold">Pending requests</h3>
      {#each pending as request (request.requestId)}
        <div class="border-t border-zinc-700 py-1">
          <div>{request.ch} / {request.kind} ({request.requestId})</div>
          <div class="text-zinc-400">{preview(request.payload)}</div>
          <button
            type="button"
            class="mt-1 rounded border border-zinc-600 px-1"
            onclick={() => channels.cancel(sessionId, request.requestId)}>Cancel</button
          >
        </div>
      {:else}
        <div class="text-zinc-500">none</div>
      {/each}

      <h3 class="mt-2 font-semibold">Log</h3>
      {#each log.slice(-20) as item (item.entryId)}
        <div class="border-t border-zinc-700 py-1">
          {item.kind} {item.ch} / {item.name}: {preview(item.data)}
        </div>
      {:else}
        <div class="text-zinc-500">none</div>
      {/each}

      <h3 class="mt-2 font-semibold">Producer errors</h3>
      {#each channels.errors as error (error.id)}
        <div class="border-t border-zinc-700 py-1 text-red-400">{error.message}</div>
      {:else}
        <div class="text-zinc-500">none</div>
      {/each}
    </div>
  {/if}
</div>
