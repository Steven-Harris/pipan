<script lang="ts">
  // Shows the oldest pending channel request of the viewed session. Others wait behind it.
  import { channels } from '#lib/stores/channels.svelte.ts';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import RequestDialog from './RequestDialog.svelte';

  const queue = $derived(channels.pending[chatStore.activeSessionId ?? ''] ?? []);
  const head = $derived(queue[0]);
</script>

{#if head}
  {#key head.requestId}
    <RequestDialog request={head} waiting={queue.length - 1} />
  {/key}
{/if}
