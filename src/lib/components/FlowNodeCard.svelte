<script lang="ts">
  import { Handle, Position, type NodeProps, type Node } from '@xyflow/svelte';
  import {
    Terminal,
    Cpu,
    User,
    Sparkles,
    Wrench,
    Bot,
    Loader2,
    CheckCircle2,
    AlertCircle,
    CircleDot
  } from 'lucide-svelte';
  import type { FlowNodeMeta } from '#lib/types/pi.ts';

  type CustomFlowNode = Node<FlowNodeMeta, 'pipan'>;

  let {
    data,
    selected = false,
    targetPosition = Position.Top,
    sourcePosition = Position.Bottom
  }: NodeProps<CustomFlowNode> = $props();

  const durationText = $derived.by(() => {
    if (typeof data.durationMs !== 'number') return null;
    if (data.durationMs < 1000) return `${data.durationMs}ms`;
    return `${(data.durationMs / 1000).toFixed(1)}s`;
  });

  const frameStyle = $derived.by(() => {
    if (selected) {
      return 'border-indigo-400 bg-zinc-900 shadow-[0_10px_28px_-6px_rgba(99,102,241,0.35)]';
    }
    if (data.status === 'error') {
      return 'border-rose-500/40 bg-zinc-900/95 shadow-[0_8px_20px_-4px_rgba(0,0,0,0.6)]';
    }
    if (data.status === 'running') {
      return 'border-indigo-500/50 bg-zinc-900/95 shadow-[0_8px_24px_-4px_rgba(99,102,241,0.22)]';
    }
    if (data.category === 'supervisor' || data.category === 'orchestrator') {
      return 'border-zinc-700/90 bg-zinc-900/95 shadow-[0_8px_20px_-4px_rgba(0,0,0,0.6)]';
    }
    return 'border-zinc-800 bg-zinc-900/90 shadow-[0_6px_16px_-4px_rgba(0,0,0,0.5)]';
  });

  const iconColor = $derived.by(() => {
    switch (data.category) {
      case 'supervisor':
        return 'text-amber-300 bg-amber-500/15 border-amber-500/30';
      case 'orchestrator':
        return 'text-indigo-300 bg-indigo-500/15 border-indigo-500/30';
      case 'user_prompt':
        return 'text-sky-300 bg-sky-500/15 border-sky-500/30';
      case 'thinking':
        return 'text-violet-300 bg-violet-500/15 border-violet-500/30';
      case 'tool_call':
        return 'text-emerald-300 bg-emerald-500/15 border-emerald-500/30';
      default:
        return 'text-indigo-300 bg-indigo-500/15 border-indigo-500/30';
    }
  });
</script>

<div
  class="w-[244px] rounded-xl border px-3 py-2.5 text-left transition-colors {frameStyle}"
>
  <Handle
    type="target"
    position={targetPosition}
    class="!h-2 !w-2 !border-zinc-700 !bg-zinc-400"
  />

  <div class="flex items-start justify-between gap-2">
    <div class="flex min-w-0 items-center gap-2">
      <div class="flex h-6 w-6 shrink-0 items-center justify-center rounded-lg border {iconColor}">
        {#if data.category === 'supervisor'}
          <Terminal class="h-3.5 w-3.5" />
        {:else if data.category === 'orchestrator'}
          <Cpu class="h-3.5 w-3.5" />
        {:else if data.category === 'user_prompt'}
          <User class="h-3.5 w-3.5" />
        {:else if data.category === 'thinking'}
          <Sparkles class="h-3.5 w-3.5" />
        {:else if data.category === 'tool_call'}
          <Wrench class="h-3.5 w-3.5" />
        {:else}
          <Bot class="h-3.5 w-3.5" />
        {/if}
      </div>

      <div class="min-w-0">
        <div class="truncate text-xs font-semibold text-zinc-100">
          {data.title}
        </div>
        {#if data.subtitle}
          <div class="truncate font-mono text-[11px] text-zinc-300 tabular-nums">
            {data.subtitle}
          </div>
        {/if}
      </div>
    </div>

    <div class="flex shrink-0 items-center gap-1 pt-0.5">
      {#if data.status === 'running'}
        <Loader2 class="h-3.5 w-3.5 animate-spin text-indigo-300" />
      {:else if data.status === 'error'}
        <AlertCircle class="h-3.5 w-3.5 text-rose-400" />
      {:else if data.status === 'completed'}
        <CheckCircle2 class="h-3.5 w-3.5 text-emerald-400" />
      {:else if data.status === 'idle'}
        <CircleDot class="h-3.5 w-3.5 text-emerald-400" />
      {:else}
        <span class="h-2 w-2 rounded-full bg-zinc-500"></span>
      {/if}
    </div>
  </div>

  {#if data.detail}
    <p class="mt-2 line-clamp-2 text-[11px] leading-snug text-zinc-300 break-words">
      {data.detail}
    </p>
  {/if}

  {#if data.badge || durationText || typeof data.tokens === 'number'}
    <div class="mt-2 flex flex-wrap items-center justify-between gap-1.5 border-t border-zinc-800/80 pt-1.5 text-[10px] text-zinc-300 tabular-nums">
      {#if data.badge}
        <span class="truncate rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-zinc-200">
          {data.badge}
        </span>
      {:else}
        <span></span>
      {/if}

      <div class="flex items-center gap-1.5 font-mono">
        {#if durationText}
          <span>{durationText}</span>
        {/if}
        {#if typeof data.tokens === 'number' && data.tokens > 0}
          <span>{data.tokens.toLocaleString()} tok</span>
        {/if}
      </div>
    </div>
  {/if}

  <Handle
    type="source"
    position={sourcePosition}
    class="!h-2 !w-2 !border-zinc-700 !bg-zinc-400"
  />
</div>
