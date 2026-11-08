<script lang="ts">
  import { onMount } from 'svelte';
  import {
    Coins,
    ArrowLeft,
    RefreshCw,
    Cpu,
    History,
    TrendingUp,
    FolderOpen,
    BarChart3,
    Sparkles,
    Calendar,
    Layers,
    ArrowUpRight
  } from 'lucide-svelte';
  import { usageStore } from '#lib/stores/usage.svelte.ts';
  import { workspaceStore } from '#lib/stores/workspace.svelte.ts';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import type { AnalyticsPeriod, UsageBucket } from '#lib/types/usage.ts';

  let hoveredBucket = $state<UsageBucket | null>(null);

  onMount(() => {
    void usageStore.init();
  });

  const PERIODS: Array<{ id: AnalyticsPeriod; label: string }> = [
    { id: 'day', label: 'Day' },
    { id: 'week', label: 'Week' },
    { id: 'month', label: 'Month' },
    { id: 'year', label: 'Year' }
  ];

  function formatCost(val?: number): string {
    if (!val || val <= 0) return '$0.00';
    if (val < 0.01) return `$${val.toFixed(4)}`;
    return `$${val.toFixed(2)}`;
  }

  function formatTokens(count?: number): string {
    if (!count || count <= 0) return '0';
    if (count >= 1_000_000) {
      const m = count / 1_000_000;
      return `${m >= 10 ? m.toFixed(1) : m.toFixed(2)}M`;
    }
    if (count >= 1_000) {
      const k = count / 1_000;
      return `${k >= 10 ? k.toFixed(0) : k.toFixed(1)}K`;
    }
    return count.toLocaleString();
  }

  function formatRelativeDate(unixSecs: number): string {
    if (!unixSecs) return '–';
    const ms = unixSecs < 10_000_000_000 ? unixSecs * 1000 : unixSecs;
    const d = new Date(ms);
    return d.toLocaleDateString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  }

  function formatFolderDisplay(cwd?: string | null): string {
    if (!cwd) return 'Headless Sandbox';
    const cleaned = cwd.replace(/^\\\\\?\\/, '');
    const sep = cleaned.includes('\\') ? '\\' : '/';
    const parts = cleaned.split(/[\\/]+/).filter(Boolean);
    if (parts.length <= 2) return cleaned;
    return `…${sep}${parts.slice(-2).join(sep)}`;
  }

  const buckets = $derived(usageStore.activeBuckets);

  const maxBucketCost = $derived.by((): number => {
    if (buckets.length === 0) return 1;
    let m = 0;
    for (const b of buckets) {
      if (b.costUsd > m) m = b.costUsd;
    }
    return m > 0 ? m : 1;
  });

  const totals = $derived(usageStore.analytics?.totals);
  const models = $derived(usageStore.analytics?.byModel ?? []);
  const sessions = $derived(usageStore.analytics?.bySession ?? []);

  async function handleResumeSession(sessionId: string) {
    const saved = chatStore.savedSessions.find((s) => s.id === sessionId);
    if (saved) {
      await chatStore.openSavedSession(saved);
      workspaceStore.openChatView();
    }
  }
</script>

<div class="flex h-full min-h-0 flex-1 flex-col overflow-y-auto bg-zinc-950 text-zinc-100 select-none">
  <!-- Top Navigation & Controls Header -->
  <header class="sticky top-0 z-20 flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-zinc-800/80 bg-zinc-900/90 px-6 py-3 backdrop-blur-md">
    <div class="flex items-center gap-3">
      <button
        type="button"
        onclick={() => workspaceStore.openChatView()}
        class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-700/80 bg-zinc-800/90 px-2.5 py-1.5 text-xs font-medium text-zinc-200 transition hover:border-zinc-600 hover:bg-zinc-700 cursor-pointer"
        title="Return to Chat workspace"
      >
        <ArrowLeft class="h-3.5 w-3.5 text-indigo-400" />
        <span>Back to Chat</span>
      </button>

      <div class="flex items-center gap-2">
        <div class="flex h-7 w-7 items-center justify-center rounded-lg border border-amber-500/30 bg-amber-500/15 text-amber-300">
          <Coins class="h-4 w-4" />
        </div>
        <div>
          <h1 class="text-sm font-semibold tracking-tight text-zinc-100">Cost & Usage Analytics</h1>
          <p class="text-[10px] text-zinc-400">Indexed from Pipan sessions and ~/.pi/agent transcripts</p>
        </div>
      </div>
    </div>

    <!-- Period Tabs & Rescan Button -->
    <div class="flex items-center gap-2.5">
      <!-- Period Selector Tabs: Day | Week | Month | Year -->
      <div class="inline-flex rounded-lg border border-zinc-800 bg-zinc-950 p-0.5" role="tablist" aria-label="Select analytics timeframe">
        {#each PERIODS as p (p.id)}
          {@const isActive = usageStore.selectedPeriod === p.id}
          <button
            type="button"
            role="tab"
            aria-selected={isActive}
            onclick={() => usageStore.setPeriod(p.id)}
            class="rounded-md px-3 py-1 text-xs font-medium transition cursor-pointer {isActive
              ? 'bg-indigo-600 text-white shadow-sm'
              : 'text-zinc-400 hover:text-zinc-200'}"
          >
            {p.label}
          </button>
        {/each}
      </div>

      <!-- Sub-granularity toggle pill -->
      {#if usageStore.selectedPeriod === 'day'}
        <div class="hidden sm:inline-flex rounded-lg border border-zinc-800 bg-zinc-950/80 p-0.5 text-[11px]">
          <button
            type="button"
            onclick={() => usageStore.setSubPeriod('primary')}
            class="rounded px-2 py-0.5 transition cursor-pointer {usageStore.subPeriod === 'primary'
              ? 'bg-zinc-800 text-zinc-100'
              : 'text-zinc-400 hover:text-zinc-200'}"
          >
            Today (Hourly)
          </button>
          <button
            type="button"
            onclick={() => usageStore.setSubPeriod('secondary')}
            class="rounded px-2 py-0.5 transition cursor-pointer {usageStore.subPeriod === 'secondary'
              ? 'bg-zinc-800 text-zinc-100'
              : 'text-zinc-400 hover:text-zinc-200'}"
          >
            Last 30 Days (Daily)
          </button>
        </div>
      {:else if usageStore.selectedPeriod === 'week'}
        <div class="hidden sm:inline-flex rounded-lg border border-zinc-800 bg-zinc-950/80 p-0.5 text-[11px]">
          <button
            type="button"
            onclick={() => usageStore.setSubPeriod('primary')}
            class="rounded px-2 py-0.5 transition cursor-pointer {usageStore.subPeriod === 'primary'
              ? 'bg-zinc-800 text-zinc-100'
              : 'text-zinc-400 hover:text-zinc-200'}"
          >
            This Week (Daily)
          </button>
          <button
            type="button"
            onclick={() => usageStore.setSubPeriod('secondary')}
            class="rounded px-2 py-0.5 transition cursor-pointer {usageStore.subPeriod === 'secondary'
              ? 'bg-zinc-800 text-zinc-100'
              : 'text-zinc-400 hover:text-zinc-200'}"
          >
            Last 12 Weeks
          </button>
        </div>
      {:else if usageStore.selectedPeriod === 'month'}
        <div class="hidden sm:inline-flex rounded-lg border border-zinc-800 bg-zinc-950/80 p-0.5 text-[11px]">
          <button
            type="button"
            onclick={() => usageStore.setSubPeriod('primary')}
            class="rounded px-2 py-0.5 transition cursor-pointer {usageStore.subPeriod === 'primary'
              ? 'bg-zinc-800 text-zinc-100'
              : 'text-zinc-400 hover:text-zinc-200'}"
          >
            This Month
          </button>
          <button
            type="button"
            onclick={() => usageStore.setSubPeriod('secondary')}
            class="rounded px-2 py-0.5 transition cursor-pointer {usageStore.subPeriod === 'secondary'
              ? 'bg-zinc-800 text-zinc-100'
              : 'text-zinc-400 hover:text-zinc-200'}"
          >
            Last 12 Months
          </button>
        </div>
      {/if}

      <!-- Rescan / Refresh Button -->
      <button
        type="button"
        onclick={() => void usageStore.rescan()}
        disabled={usageStore.loading}
        class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-700/80 bg-zinc-800/80 px-2.5 py-1 text-xs font-medium text-zinc-200 transition hover:border-zinc-600 hover:bg-zinc-700 disabled:opacity-50 cursor-pointer"
        title="Rescan session transcripts and refresh data"
      >
        <RefreshCw class="h-3.5 w-3.5 text-zinc-400 {usageStore.loading ? 'animate-spin text-indigo-400' : ''}" />
        <span class="hidden md:inline">Rescan</span>
      </button>
    </div>
  </header>

  <div class="flex-1 space-y-6 p-6">
    <!-- 4 Summary KPI Cards -->
    <section aria-label="Summary KPI Cards" class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
      <!-- KPI 1: Total Spend -->
      <div class="relative overflow-hidden rounded-xl border border-zinc-800/80 bg-zinc-900/60 p-4 shadow-sm">
        <div class="flex items-center justify-between">
          <span class="text-xs font-medium text-zinc-400">Total Spend</span>
          <div class="rounded-lg border border-emerald-500/20 bg-emerald-500/10 p-1.5 text-emerald-400">
            <Coins class="h-4 w-4" />
          </div>
        </div>
        <div class="mt-2 flex items-baseline gap-2">
          <span class="font-mono text-2xl font-bold tracking-tight text-zinc-100">
            {formatCost(totals?.totalSpendUsd)}
          </span>
          <span class="text-[11px] text-zinc-400">cumulative</span>
        </div>
        <p class="mt-1 text-[11px] text-zinc-400">
          Period spend: <span class="font-mono font-medium text-emerald-300">{formatCost(usageStore.activePeriodTotals.costUsd)}</span>
        </p>
      </div>

      <!-- KPI 2: Total Tokens -->
      <div class="relative overflow-hidden rounded-xl border border-zinc-800/80 bg-zinc-900/60 p-4 shadow-sm">
        <div class="flex items-center justify-between">
          <span class="text-xs font-medium text-zinc-400">Total Tokens</span>
          <div class="rounded-lg border border-indigo-500/20 bg-indigo-500/10 p-1.5 text-indigo-400">
            <Sparkles class="h-4 w-4" />
          </div>
        </div>
        <div class="mt-2 flex items-baseline gap-2">
          <span class="font-mono text-2xl font-bold tracking-tight text-zinc-100">
            {formatTokens(totals?.totalTokens)}
          </span>
          <span class="text-[11px] text-zinc-400">tokens</span>
        </div>
        <div class="mt-1 flex flex-wrap items-center gap-1.5 text-[10px] text-zinc-400 font-mono">
          <span title="Input tokens">In: {formatTokens(totals?.inputTokens)}</span>
          <span>·</span>
          <span title="Output tokens">Out: {formatTokens(totals?.outputTokens)}</span>
          <span>·</span>
          <span title="Cache Read tokens">Cache R: {formatTokens(totals?.cacheReadTokens)}</span>
        </div>
      </div>

      <!-- KPI 3: Active & Total Sessions -->
      <div class="relative overflow-hidden rounded-xl border border-zinc-800/80 bg-zinc-900/60 p-4 shadow-sm">
        <div class="flex items-center justify-between">
          <span class="text-xs font-medium text-zinc-400">Sessions</span>
          <div class="rounded-lg border border-sky-500/20 bg-sky-500/10 p-1.5 text-sky-400">
            <History class="h-4 w-4" />
          </div>
        </div>
        <div class="mt-2 flex items-baseline gap-2">
          <span class="font-mono text-2xl font-bold tracking-tight text-zinc-100">
            {totals?.activeSessions ?? 0}
          </span>
          <span class="text-xs text-zinc-400 font-mono">/ {totals?.totalSessions ?? 0} total</span>
        </div>
        <p class="mt-1 text-[11px] text-zinc-400">
          <span class="font-mono text-zinc-300">{totals?.totalMessages ?? 0}</span> messages indexed across sessions
        </p>
      </div>

      <!-- KPI 4: Average Cost per Session -->
      <div class="relative overflow-hidden rounded-xl border border-zinc-800/80 bg-zinc-900/60 p-4 shadow-sm">
        <div class="flex items-center justify-between">
          <span class="text-xs font-medium text-zinc-400">Avg Cost / Session</span>
          <div class="rounded-lg border border-purple-500/20 bg-purple-500/10 p-1.5 text-purple-400">
            <TrendingUp class="h-4 w-4" />
          </div>
        </div>
        <div class="mt-2 flex items-baseline gap-2">
          <span class="font-mono text-2xl font-bold tracking-tight text-zinc-100">
            {formatCost(totals?.avgCostPerSession)}
          </span>
          <span class="text-[11px] text-zinc-400">per session</span>
        </div>
        <p class="mt-1 text-[11px] text-zinc-400">
          Indexed <span class="font-mono text-zinc-300">{usageStore.analytics?.scannedFilesCount ?? 0}</span> transcripts
        </p>
      </div>
    </section>

    <!-- Interactive Spend Over Time Bar Chart -->
    <section aria-label="Spend Over Time Chart" class="rounded-xl border border-zinc-800/80 bg-zinc-900/70 p-5 shadow-sm">
      <div class="mb-4 flex flex-wrap items-center justify-between gap-2">
        <div class="flex items-center gap-2">
          <BarChart3 class="h-4 w-4 text-indigo-400" />
          <h2 class="text-xs font-semibold uppercase tracking-wider text-zinc-300">
            Spend Over Time ({usageStore.selectedPeriod.toUpperCase()})
          </h2>
        </div>

        {#if hoveredBucket}
          <div class="flex items-center gap-3 rounded-lg border border-zinc-700/80 bg-zinc-950 px-2.5 py-1 text-xs">
            <span class="font-medium text-zinc-300">{hoveredBucket.label}:</span>
            <span class="font-mono font-semibold text-emerald-400">{formatCost(hoveredBucket.costUsd)}</span>
            <span class="font-mono text-[11px] text-zinc-400">({formatTokens(hoveredBucket.totalTokens)} tokens)</span>
            <span class="font-mono text-[10px] text-zinc-400">{hoveredBucket.messageCount} msgs</span>
          </div>
        {/if}
      </div>

      <!-- Bar Chart Container -->
      {#if buckets.length === 0}
        <div class="flex h-48 flex-col items-center justify-center text-xs text-zinc-400">
          <p>No usage recorded for this timeframe</p>
          <p class="mt-1 text-[11px] text-zinc-400">Run sessions to generate cost and token metrics</p>
        </div>
      {:else}
        <div class="flex h-56 w-full flex-col justify-end pt-4 pb-2">
          <!-- Bars Row -->
          <div class="flex h-44 w-full items-end gap-1 sm:gap-1.5 border-b border-zinc-800 px-1">
            {#each buckets as bucket (bucket.key)}
              {@const heightPercent = Math.max(3, (bucket.costUsd / maxBucketCost) * 100)}
              {@const isHovered = hoveredBucket?.key === bucket.key}
              {@const hasCost = bucket.costUsd > 0}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class="group relative flex min-w-0 flex-1 flex-col items-center justify-end h-full"
                onmouseenter={() => (hoveredBucket = bucket)}
                onmouseleave={() => {
                  if (hoveredBucket?.key === bucket.key) hoveredBucket = null;
                }}
              >
                <!-- Tooltip floating on hover -->
                {#if isHovered}
                  <div class="pointer-events-none absolute bottom-full mb-2 z-30 whitespace-nowrap rounded-md border border-zinc-700 bg-zinc-900 px-2 py-1 text-[11px] shadow-xl">
                    <p class="font-semibold text-zinc-100">{bucket.label}</p>
                    <p class="font-mono text-emerald-400">{formatCost(bucket.costUsd)}</p>
                    <p class="font-mono text-[10px] text-zinc-400">{formatTokens(bucket.totalTokens)} tokens · {bucket.messageCount} msgs</p>
                  </div>
                {/if}

                <!-- Bar Column -->
                <div
                  style:height="{heightPercent}%"
                  class="w-full rounded-t transition-all duration-150 {hasCost
                    ? isHovered
                      ? 'bg-indigo-400 shadow-md shadow-indigo-500/20'
                      : 'bg-indigo-600/80 hover:bg-indigo-500'
                    : 'bg-zinc-800/40 hover:bg-zinc-700/60'}"
                ></div>
              </div>
            {/each}
          </div>

          <!-- X-Axis Labels -->
          <div class="mt-2 flex w-full justify-between px-1 text-[10px] text-zinc-400 font-mono overflow-hidden">
            {#if buckets.length <= 12}
              {#each buckets as b (b.key)}
                <span class="truncate text-center" style:flex="1">{b.label}</span>
              {/each}
            {:else}
              <span class="text-left">{buckets[0]?.label}</span>
              <span class="text-center">{buckets[Math.floor(buckets.length / 2)]?.label}</span>
              <span class="text-right">{buckets[buckets.length - 1]?.label}</span>
            {/if}
          </div>
        </div>
      {/if}
    </section>

    <!-- Two Breakdown Tables: By Model & By Workspace/Session -->
    <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <!-- Table 1: Model & Provider Breakdown -->
      <section aria-label="Model Breakdown Table" class="rounded-xl border border-zinc-800/80 bg-zinc-900/70 p-5 shadow-sm">
        <div class="mb-3 flex items-center justify-between">
          <div class="flex items-center gap-2">
            <Cpu class="h-4 w-4 text-sky-400" />
            <h2 class="text-xs font-semibold uppercase tracking-wider text-zinc-300">Spend by Model & Provider</h2>
          </div>
          <span class="font-mono text-[11px] text-zinc-400">{models.length} models</span>
        </div>

        {#if models.length === 0}
          <div class="py-8 text-center text-xs text-zinc-400">
            No model usage data recorded yet
          </div>
        {:else}
          <div class="overflow-x-auto">
            <table class="w-full text-left text-xs">
              <thead>
                <tr class="border-b border-zinc-800 text-[10px] font-semibold uppercase text-zinc-400">
                  <th class="pb-2">Model</th>
                  <th class="pb-2 text-right">Spend</th>
                  <th class="pb-2 pl-3">% Cost</th>
                  <th class="pb-2 text-right">Tokens</th>
                  <th class="pb-2 text-right">Msgs</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-zinc-800/60 font-mono text-[11px]">
                {#each models as m (`${m.provider}/${m.model}`)}
                  <tr class="transition hover:bg-zinc-800/30">
                    <td class="py-2.5 font-sans">
                      <div class="font-medium text-zinc-100">{m.model}</div>
                      <div class="text-[10px] text-zinc-400 font-mono">{m.provider}</div>
                    </td>
                    <td class="py-2.5 text-right font-medium text-emerald-400">
                      {formatCost(m.costUsd)}
                    </td>
                    <td class="py-2.5 pl-3">
                      <div class="flex items-center gap-1.5">
                        <div class="h-1.5 w-14 rounded-full bg-zinc-800 overflow-hidden">
                          <div class="h-full bg-indigo-500 rounded-full" style:width="{Math.min(100, Math.max(2, m.percentCost))}%"></div>
                        </div>
                        <span class="text-[10px] text-zinc-400">{m.percentCost.toFixed(1)}%</span>
                      </div>
                    </td>
                    <td class="py-2.5 text-right text-zinc-300">
                      {formatTokens(m.totalTokens)}
                    </td>
                    <td class="py-2.5 text-right text-zinc-400">
                      {m.messageCount}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </section>

      <!-- Table 2: Workspace & Session Breakdown -->
      <section aria-label="Session Breakdown Table" class="rounded-xl border border-zinc-800/80 bg-zinc-900/70 p-5 shadow-sm">
        <div class="mb-3 flex items-center justify-between">
          <div class="flex items-center gap-2">
            <FolderOpen class="h-4 w-4 text-amber-400" />
            <h2 class="text-xs font-semibold uppercase tracking-wider text-zinc-300">Spend by Session & Workspace</h2>
          </div>
          <span class="font-mono text-[11px] text-zinc-400">{sessions.length} sessions</span>
        </div>

        {#if sessions.length === 0}
          <div class="py-8 text-center text-xs text-zinc-400">
            No session records found
          </div>
        {:else}
          <div class="overflow-x-auto">
            <table class="w-full text-left text-xs">
              <thead>
                <tr class="border-b border-zinc-800 text-[10px] font-semibold uppercase text-zinc-400">
                  <th class="pb-2">Session / Workspace</th>
                  <th class="pb-2 text-right">Spend</th>
                  <th class="pb-2 text-right">Tokens</th>
                  <th class="pb-2 text-right">Last Active</th>
                  <th class="pb-2 text-right"></th>
                </tr>
              </thead>
              <tbody class="divide-y divide-zinc-800/60 font-mono text-[11px]">
                {#each sessions as s (s.sessionId)}
                  <tr class="transition hover:bg-zinc-800/30">
                    <td class="py-2.5 font-sans min-w-[130px]">
                      <div class="font-medium text-zinc-100 truncate max-w-[160px]" title={s.sessionName}>
                        {s.sessionName}
                      </div>
                      <div class="text-[10px] text-zinc-400 font-mono truncate max-w-[160px]" title={s.cwd ?? 'Headless'}>
                        {formatFolderDisplay(s.cwd)}
                      </div>
                    </td>
                    <td class="py-2.5 text-right font-medium text-emerald-400">
                      {formatCost(s.costUsd)}
                    </td>
                    <td class="py-2.5 text-right text-zinc-300">
                      {formatTokens(s.totalTokens)}
                    </td>
                    <td class="py-2.5 text-right text-[10px] text-zinc-400 font-sans">
                      {formatRelativeDate(s.lastActiveTs)}
                    </td>
                    <td class="py-2.5 text-right">
                      <button
                        type="button"
                        onclick={() => void handleResumeSession(s.sessionId)}
                        class="inline-flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[10px] font-medium text-indigo-400 hover:bg-indigo-500/10 hover:text-indigo-300 cursor-pointer"
                        title="Open this session in chat"
                      >
                        <span>Open</span>
                        <ArrowUpRight class="h-2.5 w-2.5" />
                      </button>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </section>
    </div>
  </div>
</div>
