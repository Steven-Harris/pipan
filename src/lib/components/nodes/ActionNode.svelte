<script lang="ts">
  import { Handle, Position, type NodeProps, type Node } from '@xyflow/svelte';
  import { Wrench, Webhook } from 'lucide-svelte';
  import type { FlowNodeMeta } from '#lib/types/pi.ts';
  import NodeStatusIcon from './NodeStatusIcon.svelte';

  type ActionFlowNode = Node<FlowNodeMeta, 'action'>;

  let {
    data,
    selected = false,
    targetPosition = Position.Top,
    sourcePosition = Position.Bottom
  }: NodeProps<ActionFlowNode> = $props();

  const isGithub = $derived(data.origin === 'github');

  const durationText = $derived.by(() => {
    if (typeof data.durationMs !== 'number') return null;
    return data.durationMs < 1000
      ? `${data.durationMs}ms`
      : `${(data.durationMs / 1000).toFixed(1)}s`;
  });

  const frame = $derived.by(() => {
    if (selected) return 'border-indigo-400 shadow-[0_10px_28px_-6px_rgba(99,102,241,0.35)]';
    if (data.status === 'error') return 'border-rose-500/40 shadow-[0_8px_20px_-4px_rgba(0,0,0,0.6)]';
    if (data.status === 'running') {
      return 'border-indigo-500/50 shadow-[0_8px_24px_-4px_rgba(99,102,241,0.22)]';
    }
    return 'border-zinc-800 shadow-[0_6px_16px_-4px_rgba(0,0,0,0.5)]';
  });

  const iconTone = $derived(
    isGithub
      ? 'text-sky-300 bg-sky-500/15 border-sky-500/30'
      : 'text-emerald-300 bg-emerald-500/15 border-emerald-500/30'
  );
</script>

<div class="w-[244px] rounded-xl border bg-zinc-900/95 px-3 py-2 text-left transition-colors {frame}">
  <Handle type="target" position={targetPosition} class="!h-2 !w-2 !border-zinc-700 !bg-zinc-400" />

  <div class="flex items-start justify-between gap-2">
    <div class="flex min-w-0 items-center gap-2">
      <div class="flex h-6 w-6 shrink-0 items-center justify-center rounded-lg border {iconTone}">
        {#if isGithub}
          <Webhook class="h-3.5 w-3.5" />
        {:else}
          <Wrench class="h-3.5 w-3.5" />
        {/if}
      </div>
      <div class="min-w-0">
        <div class="truncate text-xs font-semibold text-zinc-100">{data.title}</div>
        {#if data.subtitle}
          <div class="truncate font-mono text-[11px] text-zinc-300" title={data.subtitle}>
            {data.subtitle}
          </div>
        {/if}
      </div>
    </div>
    <div class="shrink-0 pt-0.5"><NodeStatusIcon status={data.status} /></div>
  </div>

  {#if data.detail && data.detail !== data.subtitle}
    <p class="mt-1.5 line-clamp-2 break-words text-[11px] leading-snug text-zinc-300">{data.detail}</p>
  {/if}

  {#if data.badge || durationText}
    <div class="mt-1.5 flex items-center justify-between gap-1.5 border-t border-zinc-800/80 pt-1.5 text-[10px] text-zinc-300 tabular-nums">
      <span class="truncate rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-zinc-200">
        {data.badge ?? ''}
      </span>
      {#if durationText}<span class="font-mono">{durationText}</span>{/if}
    </div>
  {/if}

  <Handle type="source" position={sourcePosition} class="!h-2 !w-2 !border-zinc-700 !bg-zinc-400" />
</div>
