<script lang="ts">
  import { onMount } from 'svelte';
  import { AlertCircle, X } from 'lucide-svelte';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import { flowStore } from '#lib/stores/flow.svelte.ts';
  import SessionHeader from '#lib/components/SessionHeader.svelte';
  import Sidebar from '#lib/components/Sidebar.svelte';
  import FlowCanvas from '#lib/components/FlowCanvas.svelte';
  import BottomDrawer from '#lib/components/BottomDrawer.svelte';

  onMount(() => {
    chatStore.init();
    flowStore.init();
  });
</script>

<div class="flex h-screen flex-col overflow-hidden bg-zinc-950 text-zinc-100">
  <SessionHeader />

  <!-- Error Banner -->
  {#if chatStore.lastError}
    <div class="flex items-center justify-between gap-3 border-b border-rose-500/30 bg-rose-950/50 px-4 py-2 text-xs text-rose-200">
      <div class="flex min-w-0 items-center gap-2">
        <AlertCircle class="h-4 w-4 shrink-0 text-rose-400" />
        <span class="truncate">{chatStore.lastError}</span>
      </div>
      <button
        type="button"
        onclick={() => chatStore.dismissError()}
        class="rounded p-1 text-rose-200 hover:bg-rose-900/50 cursor-pointer"
        title="Dismiss error"
      >
        <X class="h-3.5 w-3.5" />
      </button>
    </div>
  {/if}

  <!-- Phase 1 Application Shell: Left Sidebar + Main Canvas & Bottom Chat/Steering Drawer -->
  <div class="flex flex-1 min-h-0 overflow-hidden">
    <Sidebar />

    <main class="flex flex-1 min-w-0 flex-col overflow-hidden">
      <FlowCanvas />
      <BottomDrawer />
    </main>
  </div>
</div>
