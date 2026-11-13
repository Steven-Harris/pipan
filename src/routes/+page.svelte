<script lang="ts">
  import { onMount } from 'svelte';
  import { AlertCircle, X } from 'lucide-svelte';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import { flowStore } from '#lib/stores/flow.svelte.ts';
  import { workspaceStore } from '#lib/stores/workspace.svelte.ts';
  import { githubStore } from '#lib/stores/github.svelte.ts';
  import { doctorStore } from '#lib/stores/doctor.svelte.ts';
  import { extensionRegistry } from '#lib/extensions/registry.svelte.ts';
  import SessionHeader from '#lib/components/SessionHeader.svelte';
  import Sidebar from '#lib/components/Sidebar.svelte';
  import WorkspaceLayout from '#lib/components/WorkspaceLayout.svelte';
  import StatusBar from '#lib/components/StatusBar.svelte';
  import CostAnalyticsPage from '#lib/components/CostAnalyticsPage.svelte';
  import ExtensionsManagerPage from '#lib/components/ExtensionsManagerPage.svelte';
  import ExtensionContributionHost from '#lib/components/ExtensionContributionHost.svelte';
  import PrDetailDrawer from '#lib/components/PrDetailDrawer.svelte';
  import DoctorModal from '#lib/components/DoctorModal.svelte';
  import SettingsAuthModal from '#lib/components/SettingsAuthModal.svelte';
  import RequestHost from '#lib/components/channels/RequestHost.svelte';
  import ToastHost from '#lib/components/ToastHost.svelte';
  import { channelAttention } from '#lib/stores/channel-attention.svelte.ts';

  onMount(() => {
    workspaceStore.init();
    channelAttention.init();
    void extensionRegistry.init();
    chatStore.init();
    flowStore.init();
    void doctorStore.init();
  });

  // Auto-close open PR drawer if navigating to any non-chat view/extension
  $effect(() => {
    if (workspaceStore.currentView !== 'chat' && githubStore.selectedPrNumber !== null) {
      void githubStore.selectPr(null);
    }
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

  <!-- Application shell: sidebar + chat-first workspace with optional split panels -->
  <div class="flex flex-1 min-h-0 overflow-hidden">
    <Sidebar />

    <main class="flex flex-1 min-w-0 flex-col overflow-hidden">
      {#if workspaceStore.currentView === 'extensions'}
        <ExtensionsManagerPage />
      {:else if workspaceStore.currentView === 'cost' && extensionRegistry.isMainViewEnabled('cost')}
        <CostAnalyticsPage />
      {:else if workspaceStore.currentView !== 'chat'}
        {@const customView = extensionRegistry.contributedMainViews.find(
          (v) => v.id === workspaceStore.currentView
        )}
        {#if customView}
          <ExtensionContributionHost
            extensionId={customView.extensionId}
            contributionId={customView.id}
            title={customView.title}
            kind="mainView"
          />
        {:else}
          <WorkspaceLayout />
        {/if}
      {:else if githubStore.selectedPrNumber !== null}
        <PrDetailDrawer />
      {:else}
        <WorkspaceLayout />
      {/if}
    </main>
  </div>

  <StatusBar />
  <DoctorModal />
  <SettingsAuthModal />
  <RequestHost />
  <ToastHost />
</div>
