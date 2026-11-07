<script lang="ts">
  import { Wand2, Search, Sparkles, Terminal, Puzzle, Check } from 'lucide-svelte';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import type { PiSkillItem } from '#lib/types/pi.ts';

  let {
    open = false,
    slashQuery = null,
    selectedSkillNames = [],
    onSelect,
    onClose
  }: {
    open?: boolean;
    slashQuery?: string | null;
    selectedSkillNames?: string[];
    onSelect: (skill: PiSkillItem) => void;
    onClose: () => void;
  } = $props();

  let internalSearch = $state('');
  let sourceFilter = $state<'all' | 'skill' | 'prompt' | 'extension'>('all');
  let activeIndex = $state(0);
  let searchInputEl = $state<HTMLInputElement | null>(null);

  const effectiveQuery = $derived.by(() => {
    if (slashQuery !== null) {
      return slashQuery.replace(/^\/+/, '').replace(/^skill:/i, '').trim().toLowerCase();
    }
    return internalSearch.trim().toLowerCase();
  });

  const filteredItems = $derived.by((): PiSkillItem[] => {
    const all = chatStore.availableSkills;
    return all.filter((item) => {
      if (sourceFilter !== 'all' && item.source !== sourceFilter) {
        return false;
      }
      if (!effectiveQuery) return true;
      return (
        item.name.toLowerCase().includes(effectiveQuery) ||
        item.description.toLowerCase().includes(effectiveQuery) ||
        `skill:${item.name}`.toLowerCase().includes(effectiveQuery)
      );
    });
  });

  $effect(() => {
    if (open) {
      void chatStore.refreshInstalledSkills(chatStore.sessionStatus.cwd);
      if (chatStore.sessionStatus.state === 'running') {
        void chatStore.requestCommands();
      }
      activeIndex = 0;
      if (slashQuery === null) {
        queueMicrotask(() => {
          searchInputEl?.focus();
        });
      }
    }
  });

  export function moveSelection(delta: number): void {
    if (filteredItems.length === 0) return;
    activeIndex = (activeIndex + delta + filteredItems.length) % filteredItems.length;
  }

  export function confirmHighlighted(): boolean {
    const item = filteredItems[activeIndex];
    if (!item) return false;
    onSelect(item);
    return true;
  }

  function formatInvocationToken(item: PiSkillItem): string {
    return item.source === 'skill' ? `/skill:${item.name}` : `/${item.name}`;
  }
</script>

{#if open}
  <div
    role="dialog"
    aria-label="Installed skills and slash commands"
    class="absolute bottom-full left-0 z-50 mb-2 w-96 rounded-xl border border-zinc-700/90 bg-zinc-900/95 p-2.5 shadow-2xl backdrop-blur-xl"
  >
    <!-- Header & Source Filter Tabs -->
    <div class="mb-2 flex items-center justify-between gap-2">
      <div class="flex items-center gap-1.5 text-xs font-semibold text-zinc-100">
        <Wand2 class="h-3.5 w-3.5 text-indigo-400" />
        <span>Skills & Commands</span>
        <span class="rounded-full bg-zinc-800 px-1.5 py-0.2 font-mono text-[10px] text-zinc-300">
          {filteredItems.length}
        </span>
      </div>

      <div class="flex items-center gap-1 text-[10px]">
        {#each [
          { id: 'all', label: 'All' },
          { id: 'skill', label: 'Skills' },
          { id: 'prompt', label: 'Prompts' },
          { id: 'extension', label: 'Ext' }
        ] as tab (tab.id)}
          <button
            type="button"
            onclick={() => {
              sourceFilter = tab.id as 'all' | 'skill' | 'prompt' | 'extension';
              activeIndex = 0;
            }}
            class="rounded-md px-1.5 py-0.5 font-medium transition cursor-pointer {sourceFilter ===
            tab.id
              ? 'bg-indigo-500/20 text-indigo-200 border border-indigo-500/40'
              : 'text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200'}"
          >
            {tab.label}
          </button>
        {/each}
      </div>
    </div>

    <!-- Search Input (when opened via toolbar button) -->
    {#if slashQuery === null}
      <div class="relative mb-2">
        <Search class="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-zinc-400" />
        <input
          bind:this={searchInputEl}
          bind:value={internalSearch}
          onkeydown={(e) => {
            if (e.key === 'ArrowDown') {
              e.preventDefault();
              moveSelection(1);
            } else if (e.key === 'ArrowUp') {
              e.preventDefault();
              moveSelection(-1);
            } else if (e.key === 'Enter' && filteredItems[activeIndex]) {
              e.preventDefault();
              onSelect(filteredItems[activeIndex]);
            } else if (e.key === 'Escape') {
              e.preventDefault();
              onClose();
            }
          }}
          type="search"
          placeholder="Search skills, prompt templates, commands…"
          aria-label="Search skills"
          class="w-full rounded-lg border border-zinc-800 bg-zinc-950 pl-8 pr-2.5 py-1.5 text-xs text-zinc-100 placeholder-zinc-500 focus:border-indigo-500/60 focus:outline-none"
        />
      </div>
    {/if}

    <!-- Skills List -->
    <div class="max-h-64 overflow-y-auto space-y-1 pr-0.5">
      {#if filteredItems.length === 0}
        <div class="py-5 px-2 text-center text-xs text-zinc-400 space-y-1">
          <p class="font-medium text-zinc-300">No matching skills found</p>
          <p class="text-[11px] text-zinc-500">
            Skills are discovered from ~/.pi/agent/skills, ~/.agents/skills, and .pi/skills.
          </p>
        </div>
      {:else}
        {#each filteredItems as item, idx (`${item.source}:${item.name}`)}
          {@const isHighlighted = idx === activeIndex}
          {@const isSelected = selectedSkillNames.includes(item.name)}
          {@const token = formatInvocationToken(item)}
          <button
            type="button"
            onmouseenter={() => (activeIndex = idx)}
            onclick={() => onSelect(item)}
            class="flex w-full flex-col gap-0.5 rounded-lg border px-2.5 py-2 text-left text-xs transition cursor-pointer {isHighlighted
              ? 'border-indigo-500/50 bg-indigo-950/35 text-zinc-100'
              : 'border-zinc-800/70 bg-zinc-950/60 text-zinc-200 hover:border-zinc-700'}"
          >
            <div class="flex items-center justify-between gap-2">
              <div class="flex min-w-0 items-center gap-1.5">
                {#if item.source === 'skill'}
                  <Sparkles class="h-3.5 w-3.5 shrink-0 text-indigo-400" />
                {:else if item.source === 'prompt'}
                  <Terminal class="h-3.5 w-3.5 shrink-0 text-amber-400" />
                {:else}
                  <Puzzle class="h-3.5 w-3.5 shrink-0 text-emerald-400" />
                {/if}
                <span class="truncate font-mono font-medium text-indigo-200">{token}</span>
              </div>

              <div class="flex shrink-0 items-center gap-1">
                {#if item.scope}
                  <span class="rounded bg-zinc-800 px-1.5 py-0.2 text-[10px] text-zinc-400">
                    {item.scope}
                  </span>
                {:else}
                  <span class="rounded bg-zinc-800 px-1.5 py-0.2 text-[10px] text-zinc-400">
                    {item.source}
                  </span>
                {/if}
                {#if isSelected}
                  <Check class="h-3.5 w-3.5 text-emerald-400" />
                {/if}
              </div>
            </div>

            {#if item.description}
              <p class="line-clamp-2 text-[11px] leading-snug text-zinc-400">
                {item.description}
              </p>
            {/if}
          </button>
        {/each}
      {/if}
    </div>
  </div>
{/if}
