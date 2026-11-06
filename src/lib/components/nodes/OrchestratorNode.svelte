<script lang="ts">
  import { Handle, Position, type NodeProps, type Node } from '@xyflow/svelte';
  import { Terminal, Cpu } from 'lucide-svelte';
  import type { FlowNodeMeta } from '#lib/types/pi.ts';
  import NodeStatusIcon from './NodeStatusIcon.svelte';

  type OrchestratorFlowNode = Node<FlowNodeMeta, 'orchestrator'>;

  let {
    data,
    selected = false,
    targetPosition = Position.Top,
    sourcePosition = Position.Bottom
  }: NodeProps<OrchestratorFlowNode> = $props();

  const isSupervisor = $derived(data.category === 'supervisor');

  const frame = $derived.by(() => {
    if (selected) return 'border-indigo-400 shadow-[0_10px_28px_-6px_rgba(99,102,241,0.35)]';
    if (data.status === 'error') return 'border-rose-500/40 shadow-[0_8px_20px_-4px_rgba(0,0,0,0.6)]';
    if (data.status === 'running') {
      return 'border-indigo-500/50 shadow-[0_8px_24px_-4px_rgba(99,102,241,0.22)]';
    }
    return 'border-zinc-700/90 shadow-[0_8px_20px_-4px_rgba(0,0,0,0.6)]';
  });

  const iconTone = $derived(
    isSupervisor
      ? 'text-amber-300 bg-amber-500/15 border-amber-500/30'
      : 'text-indigo-300 bg-indigo-500/15 border-indigo-500/30'
  );
</script>

<div class="w-[260px] rounded-xl border bg-zinc-900 px-3 py-2.5 text-left transition-colors {frame}">
  <Handle type="target" position={targetPosition} class="!h-2 !w-2 !border-zinc-700 !bg-zinc-400" />

  <div class="flex items-start justify-between gap-2">
    <div class="flex min-w-0 items-center gap-2">
      <div class="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border {iconTone}">
        {#if isSupervisor}
          <Terminal class="h-4 w-4" />
        {:else}
          <Cpu class="h-4 w-4" />
        {/if}
      </div>
      <div class="min-w-0">
        <div class="truncate text-xs font-semibold text-zinc-100">{data.title}</div>
        {#if data.subtitle}
          <div class="truncate font-mono text-[11px] text-zinc-300 tabular-nums">{data.subtitle}</div>
        {/if}
      </div>
    </div>
    <div class="shrink-0 pt-0.5"><NodeStatusIcon status={data.status} /></div>
  </div>

  {#if data.detail}
    <p class="mt-2 line-clamp-2 break-words text-[11px] leading-snug text-zinc-300">{data.detail}</p>
  {/if}

  {#if data.badge}
    <div class="mt-2 border-t border-zinc-800/80 pt-1.5">
      <span class="inline-block max-w-full truncate rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-[10px] text-zinc-200">
        {data.badge}
      </span>
    </div>
  {/if}

  <Handle type="source" position={sourcePosition} class="!h-2 !w-2 !border-zinc-700 !bg-zinc-400" />
</div>
