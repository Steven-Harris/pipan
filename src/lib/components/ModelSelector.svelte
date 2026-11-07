<script lang="ts">
  import {
    Cpu,
    Sparkles,
    Search,
    Check,
    ChevronDown,
    Image,
    Brain,
    Coins
  } from 'lucide-svelte';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import type { PiFullModelInfo } from '#lib/types/pi.ts';

  let {
    placement = 'down',
    compact = false
  }: {
    placement?: 'down' | 'up';
    compact?: boolean;
  } = $props();

  let isOpen = $state(false);
  let searchQuery = $state('');
  let containerEl = $state<HTMLDivElement | null>(null);
  let searchInputEl = $state<HTMLInputElement | null>(null);

  const activeModelId = $derived(chatStore.modelInfo?.id ?? '');
  const activeProvider = $derived(chatStore.modelInfo?.provider ?? '');
  const activeThinking = $derived(
    chatStore.modelInfo?.thinkingLevel ?? chatStore.selectedThinkingLevel ?? 'medium'
  );

  const activeFullModel = $derived.by((): PiFullModelInfo | undefined => {
    if (!activeModelId) return chatStore.availableModels[0];
    return chatStore.availableModels.find(
      (m) =>
        m.id.toLowerCase() === activeModelId.toLowerCase() &&
        (!activeProvider || m.provider.toLowerCase() === activeProvider.toLowerCase())
    );
  });

  const groupedModels = $derived.by(
    (): Array<{ provider: string; models: PiFullModelInfo[] }> => {
      const q = searchQuery.trim().toLowerCase();
      const source =
        chatStore.availableModels.length > 0
          ? chatStore.availableModels
          : chatStore.configuredModels;

      const filtered = q
        ? source.filter(
            (m) =>
              m.id.toLowerCase().includes(q) ||
              (m.name && m.name.toLowerCase().includes(q)) ||
              m.provider.toLowerCase().includes(q)
          )
        : source;

      const groups = new Map<string, PiFullModelInfo[]>();
      for (const model of filtered) {
        const prov = model.provider || 'custom';
        const list = groups.get(prov);
        if (list) {
          list.push(model);
        } else {
          groups.set(prov, [model]);
        }
      }

      return Array.from(groups.entries()).map(([provider, models]) => ({
        provider,
        models
      }));
    }
  );

  function formatContextWindow(tokens?: number): string | null {
    if (!tokens || tokens <= 0) return null;
    if (tokens >= 1_000_000) {
      const m = tokens / 1_000_000;
      return `${Number.isInteger(m) ? m : m.toFixed(1)}M ctx`;
    }
    return `${Math.round(tokens / 1000)}K ctx`;
  }

  function formatPricing(cost?: PiFullModelInfo['cost']): string | null {
    if (!cost) return null;
    const inp = cost.input;
    const out = cost.output;
    if (typeof inp !== 'number' && typeof out !== 'number') return null;
    const fmt = (n?: number) => {
      if (typeof n !== 'number') return '–';
      return n < 1 ? `$${n.toFixed(2).replace(/\.?0+$/, '')}` : `$${n}`;
    };
    return `${fmt(inp)} / ${fmt(out)} per 1M`;
  }

  function togglePopover() {
    isOpen = !isOpen;
    if (isOpen) {
      searchQuery = '';
      void chatStore.refreshConfiguredModels();
      if (chatStore.sessionStatus.state === 'running') {
        void chatStore.requestAvailableModels();
        void chatStore.requestAvailableThinkingLevels();
      }
      queueMicrotask(() => {
        searchInputEl?.focus();
      });
    }
  }

  async function handleSelectModel(model: PiFullModelInfo) {
    await chatStore.selectModel(model.provider, model.id);
    isOpen = false;
  }

  async function handleSelectThinking(level: string) {
    await chatStore.selectThinkingLevel(level);
  }

  function handleWindowClick(e: MouseEvent) {
    if (!isOpen || !containerEl) return;
    if (!containerEl.contains(e.target as Node)) {
      isOpen = false;
    }
  }

  function handleWindowKeydown(e: KeyboardEvent) {
    if (isOpen && e.key === 'Escape') {
      e.stopPropagation();
      isOpen = false;
    }
  }
</script>

<svelte:window onclick={handleWindowClick} onkeydown={handleWindowKeydown} />

<div bind:this={containerEl} class="relative inline-flex">
  <button
    type="button"
    onclick={togglePopover}
    aria-haspopup="dialog"
    aria-expanded={isOpen}
    class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-800 bg-zinc-900/80 px-2.5 py-1 text-xs text-zinc-200 transition hover:border-zinc-700 hover:bg-zinc-800/90 cursor-pointer"
    title={activeProvider && activeModelId
      ? `Model: ${activeProvider}/${activeModelId} (Thinking: ${activeThinking})`
      : 'Select Model & Thinking Level'}
  >
    <Cpu class="h-3.5 w-3.5 shrink-0 text-indigo-400" />
    {#if activeProvider && !compact}
      <span class="hidden md:inline text-[11px] text-zinc-400">{activeProvider}/</span>
    {/if}
    <span class="truncate {compact ? 'max-w-[130px]' : 'max-w-[170px]'} font-medium text-zinc-100">
      {activeFullModel?.name ?? (activeModelId || 'Select model')}
    </span>
    {#if activeThinking && activeThinking !== 'off'}
      <span class="inline-flex items-center gap-0.5 rounded bg-indigo-500/15 px-1.5 py-0.2 text-[10px] font-medium text-indigo-300">
        <Sparkles class="h-2.5 w-2.5" />
        {activeThinking}
      </span>
    {/if}
    <ChevronDown class="h-3 w-3 shrink-0 text-zinc-400" />
  </button>

  {#if isOpen}
    <div
      role="dialog"
      aria-label="Model and thinking level selector"
      class="absolute z-50 w-96 rounded-xl border border-zinc-700/90 bg-zinc-900/95 p-2.5 shadow-2xl backdrop-blur-xl {placement ===
      'up'
        ? 'bottom-full left-0 mb-2'
        : 'top-full left-0 mt-2'}"
    >
      <!-- Search Input -->
      <div class="relative mb-2">
        <Search class="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-zinc-400" />
        <input
          bind:this={searchInputEl}
          bind:value={searchQuery}
          type="search"
          placeholder="Search models by name, ID, or provider…"
          aria-label="Search models"
          class="w-full rounded-lg border border-zinc-800 bg-zinc-950 pl-8 pr-2.5 py-1.5 text-xs text-zinc-100 placeholder-zinc-500 focus:border-indigo-500/60 focus:outline-none"
        />
      </div>

      <!-- Thinking Level Bar -->
      <div class="mb-2.5 rounded-lg border border-zinc-800/90 bg-zinc-950/70 p-2">
        <div class="mb-1.5 flex items-center justify-between text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
          <span class="inline-flex items-center gap-1">
            <Sparkles class="h-3 w-3 text-indigo-400" />
            Thinking Level
          </span>
          <span class="font-mono text-indigo-300">{activeThinking}</span>
        </div>
        <div class="grid grid-cols-6 gap-1">
          {#each chatStore.availableThinkingLevels as level (level)}
            {@const isSelected = activeThinking === level}
            <button
              type="button"
              onclick={() => void handleSelectThinking(level)}
              class="rounded-md border px-1.5 py-1 text-[10px] font-medium capitalize transition cursor-pointer {isSelected
                ? 'border-indigo-500/60 bg-indigo-500/20 text-indigo-200'
                : 'border-zinc-800 bg-zinc-900 text-zinc-400 hover:border-zinc-700 hover:text-zinc-200'}"
            >
              {level}
            </button>
          {/each}
        </div>
      </div>

      <!-- Grouped Models List -->
      <div class="max-h-72 overflow-y-auto space-y-2.5 pr-0.5">
        {#if groupedModels.length === 0}
          <div class="py-6 text-center text-xs text-zinc-400">
            No models matching "{searchQuery}"
          </div>
        {:else}
          {#each groupedModels as group (group.provider)}
            <div class="space-y-1">
              <div class="flex items-center justify-between px-1.5 text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
                <span>{group.provider}</span>
                <span class="font-mono text-zinc-500">{group.models.length}</span>
              </div>

              <div class="space-y-1">
                {#each group.models as model (`${model.provider}/${model.id}`)}
                  {@const isCurrent =
                    model.id.toLowerCase() === activeModelId.toLowerCase() &&
                    (!activeProvider ||
                      model.provider.toLowerCase() === activeProvider.toLowerCase())}
                  {@const ctxLabel = formatContextWindow(model.contextWindow)}
                  {@const priceLabel = formatPricing(model.cost)}
                  {@const hasVision =
                    Boolean(model.supportsImages) || Boolean(model.input?.includes('image'))}
                  <button
                    type="button"
                    onclick={() => void handleSelectModel(model)}
                    class="flex w-full flex-col gap-1 rounded-lg border px-2.5 py-2 text-left text-xs transition cursor-pointer {isCurrent
                      ? 'border-indigo-500/50 bg-indigo-950/30 text-zinc-100'
                      : 'border-zinc-800/70 bg-zinc-950/60 text-zinc-200 hover:border-zinc-700 hover:bg-zinc-800/60'}"
                  >
                    <div class="flex items-center justify-between gap-2">
                      <div class="min-w-0 flex-1">
                        <div class="flex items-center gap-1.5">
                          <span class="truncate font-medium text-zinc-100">
                            {model.name ?? model.id}
                          </span>
                          {#if model.name && model.name !== model.id}
                            <span class="truncate font-mono text-[10px] text-zinc-400">
                              {model.id}
                            </span>
                          {/if}
                        </div>
                      </div>
                      {#if isCurrent}
                        <Check class="h-3.5 w-3.5 shrink-0 text-indigo-400" />
                      {/if}
                    </div>

                    <!-- Capability & Pricing Badges -->
                    <div class="flex flex-wrap items-center gap-1.5 text-[10px]">
                      {#if ctxLabel}
                        <span class="rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-zinc-300">
                          {ctxLabel}
                        </span>
                      {/if}
                      {#if hasVision}
                        <span class="inline-flex items-center gap-0.5 rounded bg-sky-500/15 px-1.5 py-0.5 text-sky-300">
                          <Image class="h-2.5 w-2.5" />
                          <span>Vision</span>
                        </span>
                      {/if}
                      {#if model.reasoning}
                        <span class="inline-flex items-center gap-0.5 rounded bg-purple-500/15 px-1.5 py-0.5 text-purple-300">
                          <Brain class="h-2.5 w-2.5" />
                          <span>Reasoning</span>
                        </span>
                      {/if}
                      {#if priceLabel}
                        <span class="inline-flex items-center gap-0.5 rounded bg-emerald-500/10 px-1.5 py-0.5 font-mono text-emerald-300">
                          <Coins class="h-2.5 w-2.5" />
                          <span>{priceLabel}</span>
                        </span>
                      {/if}
                    </div>
                  </button>
                {/each}
              </div>
            </div>
          {/each}
        {/if}
      </div>
    </div>
  {/if}
</div>
