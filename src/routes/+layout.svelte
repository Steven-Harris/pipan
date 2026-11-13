<script lang="ts">
  import "../app.css";
  import ChannelDebugPanel from "#lib/components/channels/ChannelDebugPanel.svelte";

  let { children } = $props();
</script>

{@render children()}

{#if import.meta.env.DEV}
  <ChannelDebugPanel />
{/if}
