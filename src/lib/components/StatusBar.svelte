<script lang="ts">
  import { onMount } from 'svelte';
  import {
    Activity,
    Coins,
    GitBranch,
    FolderOpen,
    Radio,
    Terminal,
    History,
    Blocks,
    HeartPulse,
    Bot,
    GitPullRequest
  } from 'lucide-svelte';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import { workspaceStore } from '#lib/stores/workspace.svelte.ts';
  import { usageStore } from '#lib/stores/usage.svelte.ts';
  import { githubStore } from '#lib/stores/github.svelte.ts';
  import ModelSelector from './ModelSelector.svelte';

  onMount(() => {
    void usageStore.init();
    void githubStore.init();
  });

  const connectionStatus = $derived.by(() => {
    const s = chatStore.sessionStatus.state;
    if (s === 'running') {
      return {
        label: chatStore.isAgentRunning ? 'Streaming' : 'Connected',
        dotClass: chatStore.isAgentRunning ? 'bg-indigo-400 animate-ping' : 'bg-emerald-400',
        textClass: 'text-emerald-300'
      };
    }
    if (s === 'starting') {
      return {
        label: 'Starting…',
        dotClass: 'bg-amber-400 animate-pulse',
        textClass: 'text-amber-300'
      };
    }
    if (s === 'error') {
      return {
        label: 'Error',
        dotClass: 'bg-rose-500',
        textClass: 'text-rose-300'
      };
    }
    return {
      label: 'Disconnected',
      dotClass: 'bg-zinc-500',
      textClass: 'text-zinc-400'
    };
  });

  const activeBranchOrWorkspace = $derived.by((): { label: string; tooltip: string } => {
    if (chatStore.sessionStatus.mode === 'headless') {
      return { label: 'Headless Sandbox', tooltip: 'Isolated temporary workspace' };
    }
    const saved = chatStore.savedSessions.find((s) => s.id === chatStore.activeSessionId);
    if (saved?.branch) {
      return { label: saved.branch, tooltip: `Worktree branch: ${saved.branch}` };
    }
    const cwd = chatStore.sessionStatus.cwd;
    if (!cwd) return { label: 'Workspace', tooltip: 'No workspace bound' };
    const cleaned = cwd.replace(/^\\\\\?\\/, '');
    const sep = cleaned.includes('\\') ? '\\' : '/';
    const parts = cleaned.split(/[\\/]+/).filter(Boolean);
    const shortLabel = parts.length > 0 ? parts[parts.length - 1] : cwd;
    return { label: shortLabel, tooltip: cwd };
  });

  const activeSubagentsCount = $derived.by((): number => {
    const runningOthers = chatStore.savedSessions.filter(
      (s) => (s.status === 'running' || s.status === 'starting') && s.id !== chatStore.activeSessionId
    );
    return runningOthers.length;
  });

  const ghAccount = $derived(githubStore.status?.account ?? null);
  const webhookActive = $derived(githubStore.forwarder?.state === 'running');

  const contextPercent = $derived(chatStore.contextUsage.percent);
  const currentSessionCost = $derived(chatStore.activeSessionCost);
  const totalCumulativeCost = $derived(
    usageStore.analytics?.totals?.totalSpendUsd ?? currentSessionCost
  );

  const activeSessionsCount = $derived.by((): number => {
    const running = chatStore.savedSessions.filter(
      (s) => s.status === 'running' || s.status === 'starting'
    );
    return Math.max(chatStore.sessionStatus.state === 'running' ? 1 : 0, running.length);
  });

  const totalSessionsCount = $derived.by((): number => {
    return Math.max(chatStore.savedSessions.length, usageStore.analytics?.totals?.totalSessions ?? 0);
  });

  function formatCost(val?: number): string {
    if (!val || val <= 0) return '$0.00';
    if (val < 0.01) return `$${val.toFixed(4)}`;
    return `$${val.toFixed(2)}`;
  }

  function formatTokens(count?: number): string {
    if (!count || count <= 0) return '0';
    if (count >= 1_000_000) {
      const m = count / 1_000_000;
      return `${m.toFixed(1)}M`;
    }
    if (count >= 1_000) {
      const k = count / 1_000;
      return `${k.toFixed(1)}K`;
    }
    return count.toString();
  }
</script>

<footer
  aria-label="Application status bar"
  class="flex h-7 shrink-0 items-center justify-between border-t border-zinc-800/80 bg-zinc-950 px-3 text-[11px] text-zinc-400 select-none overflow-x-auto overflow-y-hidden"
>
  <!-- Left Segment: Pi connection, workspace/branch, subagents, GitHub & webhook status -->
  <div class="flex items-center gap-3 shrink-0">
    <!-- Pi Connection Status -->
    <div
      class="inline-flex items-center gap-1.5 font-medium transition hover:text-zinc-200"
      title="Pi RPC connection state: {chatStore.sessionStatus.state} (PID: {chatStore.sessionStatus.pid ?? 'none'})"
    >
      <span class="h-2 w-2 rounded-full {connectionStatus.dotClass}"></span>
      <span class={connectionStatus.textClass}>{connectionStatus.label}</span>
    </div>

    <span class="h-3 w-px bg-zinc-800" aria-hidden="true"></span>

    <!-- Active Workspace / Worktree Branch -->
    <div
      class="inline-flex items-center gap-1.5 truncate max-w-[170px] hover:text-zinc-200"
      title={activeBranchOrWorkspace.tooltip}
    >
      {#if chatStore.sessionStatus.mode === 'headless'}
        <Terminal class="h-3 w-3 text-amber-400 shrink-0" />
      {:else if activeBranchOrWorkspace.label.includes('/') || activeBranchOrWorkspace.label.includes('main') || activeBranchOrWorkspace.label.includes('feat')}
        <GitBranch class="h-3 w-3 text-emerald-400 shrink-0" />
      {:else}
        <FolderOpen class="h-3 w-3 text-sky-400 shrink-0" />
      {/if}
      <span class="truncate font-mono">{activeBranchOrWorkspace.label}</span>
    </div>

    <span class="h-3 w-px bg-zinc-800" aria-hidden="true"></span>

    <!-- Active Background Subagents Count -->
    <div
      class="inline-flex items-center gap-1.5 hover:text-zinc-200"
      title="{activeSubagentsCount} active background subagent worktrees"
    >
      <Bot class="h-3 w-3 {activeSubagentsCount > 0 ? 'text-indigo-400 animate-pulse' : 'text-zinc-500'}" />
      <span class="font-mono">{activeSubagentsCount} {activeSubagentsCount === 1 ? 'subagent' : 'subagents'}</span>
    </div>

    <span class="h-3 w-px bg-zinc-800" aria-hidden="true"></span>

    <!-- GitHub & Webhook Tunnel Status -->
    <div class="inline-flex items-center gap-2 hover:text-zinc-200">
      <div class="inline-flex items-center gap-1" title={ghAccount ? `GitHub account: @${ghAccount}` : 'GitHub CLI not authenticated'}>
        <GitPullRequest class="h-3 w-3 {ghAccount ? 'text-sky-400' : 'text-zinc-600'}" />
        <span class="font-mono truncate max-w-[100px]">{ghAccount ? `@${ghAccount}` : 'Signed out'}</span>
      </div>

      <div
        class="inline-flex items-center gap-1 font-mono text-[10px]"
        title={webhookActive ? 'gh webhook forward tunnel is live' : 'Webhook tunnel is standby'}
      >
        <Radio class="h-2.5 w-2.5 {webhookActive ? 'text-emerald-400 animate-pulse' : 'text-zinc-500'}" />
        <span class={webhookActive ? 'text-emerald-300' : 'text-zinc-500'}>
          {webhookActive ? 'Live' : 'Standby'}
        </span>
      </div>
    </div>
  </div>

  <!-- Right Segment: Token Context Window %, Cost (clickable), Sessions Count, Model Selector, Extensions, Doctor -->
  <div class="flex items-center gap-3 shrink-0 pl-3">
    <!-- Active Session Token / Context Window % -->
    <div
      class="inline-flex items-center gap-1.5 hover:text-zinc-200 font-mono"
      title="Context window tokens: {formatTokens(chatStore.contextUsage.currentTokens)} / {formatTokens(chatStore.contextUsage.contextWindow)} ({contextPercent}%)"
    >
      <Activity class="h-3 w-3 {contextPercent > 80 ? 'text-rose-400' : contextPercent > 50 ? 'text-amber-400' : 'text-indigo-400'}" />
      <span>{formatTokens(chatStore.contextUsage.currentTokens)}</span>
      <span class="text-zinc-500">/</span>
      <span>{formatTokens(chatStore.contextUsage.contextWindow)}</span>
      <span class="rounded bg-zinc-800/80 px-1 py-0.2 text-[10px] text-zinc-300 font-semibold">{contextPercent}%</span>
    </div>

    <span class="h-3 w-px bg-zinc-800" aria-hidden="true"></span>

    <!-- Current Session Cost & Total Cumulative Cost (Clicks to open Cost Page) -->
    <button
      type="button"
      onclick={() => workspaceStore.toggleCostView()}
      class="inline-flex items-center gap-1.5 rounded px-1.5 py-0.5 transition hover:bg-zinc-800/80 hover:text-zinc-100 cursor-pointer {workspaceStore.currentView === 'cost' ? 'bg-indigo-500/15 text-indigo-200 ring-1 ring-indigo-500/30' : ''}"
      title="Current session: {formatCost(currentSessionCost)} | Total spend: {formatCost(totalCumulativeCost)} (Click to toggle Cost & Usage Analytics)"
    >
      <Coins class="h-3 w-3 text-amber-400" />
      <span class="font-mono">
        <span class="text-zinc-400">Sess:</span> <span class="text-zinc-200 font-medium">{formatCost(currentSessionCost)}</span>
        <span class="text-zinc-600">|</span>
        <span class="text-zinc-400">Total:</span> <span class="text-emerald-400 font-medium">{formatCost(totalCumulativeCost)}</span>
      </span>
    </button>

    <span class="h-3 w-px bg-zinc-800" aria-hidden="true"></span>

    <!-- Total Session Count: active / total -->
    <div
      class="inline-flex items-center gap-1 hover:text-zinc-200 font-mono"
      title="{activeSessionsCount} active sessions, {totalSessionsCount} total saved sessions"
    >
      <History class="h-3 w-3 text-sky-400" />
      <span>{activeSessionsCount} / {totalSessionsCount}</span>
    </div>

    <span class="h-3 w-px bg-zinc-800" aria-hidden="true"></span>

    <!-- Active Model & Thinking Level Selector (opens popover upward) -->
    <div class="inline-flex items-center">
      <ModelSelector placement="up" compact={true} />
    </div>

    <span class="h-3 w-px bg-zinc-800" aria-hidden="true"></span>

    <!-- Extensions Count -->
    <button
      type="button"
      onclick={() => {}}
      class="inline-flex items-center gap-1 hover:text-zinc-200 cursor-pointer"
      title="5 Built-in Extensions active: node-graph, github-prs, terminal, files, cost-analytics"
    >
      <Blocks class="h-3 w-3 text-indigo-400" />
      <span class="font-mono">5 Extensions</span>
    </button>

    <span class="h-3 w-px bg-zinc-800" aria-hidden="true"></span>

    <!-- Doctor Health Status Indicator -->
    <div
      class="inline-flex items-center gap-1 hover:text-zinc-200"
      title="Pipan Doctor: System dependencies healthy"
    >
      <HeartPulse class="h-3 w-3 text-emerald-400" />
      <span class="text-emerald-400 font-medium">Doctor: OK</span>
    </div>
  </div>
</footer>
