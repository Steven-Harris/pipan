<script lang="ts">
  import { Handle, Position, type NodeProps, type Node } from '@xyflow/svelte';
  import { ShieldAlert, Check, Pencil } from 'lucide-svelte';
  import { flowStore } from '#lib/stores/flow.svelte.ts';
  import type { FlowNodeMeta } from '#lib/types/pi.ts';
  import NodeStatusIcon from './NodeStatusIcon.svelte';

  type ApprovalFlowNode = Node<FlowNodeMeta, 'approval'>;

  let {
    id,
    data,
    selected = false,
    targetPosition = Position.Top,
    sourcePosition = Position.Bottom
  }: NodeProps<ApprovalFlowNode> = $props();

  const resolution = $derived(data.approval?.resolution ?? 'pending');
  const pending = $derived(resolution === 'pending');

  const frame = $derived.by(() => {
    if (selected) return 'border-amber-300 shadow-[0_10px_28px_-6px_rgba(251,191,36,0.3)]';
    if (pending) return 'border-amber-500/60 shadow-[0_8px_24px_-4px_rgba(245,158,11,0.2)]';
    return 'border-zinc-700/90 shadow-[0_8px_20px_-4px_rgba(0,0,0,0.6)]';
  });
</script>

<div class="w-[276px] rounded-xl border bg-zinc-900 px-3 py-2.5 text-left transition-colors {frame}">
  <Handle type="target" position={targetPosition} class="!h-2 !w-2 !border-zinc-700 !bg-zinc-400" />

  <div class="flex items-start justify-between gap-2">
    <div class="flex min-w-0 items-center gap-2">
      <div class="flex h-6 w-6 shrink-0 items-center justify-center rounded-lg border border-amber-500/30 bg-amber-500/15 text-amber-300">
        <ShieldAlert class="h-3.5 w-3.5" />
      </div>
      <div class="min-w-0">
        <div class="truncate text-xs font-semibold text-zinc-100">{data.title}</div>
        {#if data.subtitle}
          <div class="truncate font-mono text-[11px] text-zinc-300">{data.subtitle}</div>
        {/if}
      </div>
    </div>
    <div class="shrink-0 pt-0.5"><NodeStatusIcon status={data.status} /></div>
  </div>

  {#if data.approval}
    <pre class="mt-2 max-h-16 overflow-hidden whitespace-pre-wrap break-words rounded-lg border border-zinc-800 bg-zinc-950 p-2 font-mono text-[11px] leading-snug text-zinc-200">{data.approval.action}</pre>
  {/if}
  {#if data.detail && data.detail !== data.approval?.action}
    <p class="mt-1.5 line-clamp-2 break-words text-[11px] leading-snug text-zinc-300">{data.detail}</p>
  {/if}

  <div class="mt-2 border-t border-zinc-800/80 pt-2">
    {#if pending}
      <!-- `nodrag nopan` keeps Svelte Flow from treating button presses as node drags. -->
      <div class="nodrag nopan flex items-center gap-2">
        <button
          type="button"
          onclick={() => flowStore.resolveApproval(id, 'approved')}
          class="inline-flex flex-1 cursor-pointer items-center justify-center gap-1 rounded-lg bg-emerald-600 px-2 py-1 text-[11px] font-semibold text-white transition hover:bg-emerald-500"
        >
          <Check class="h-3 w-3" />
          <span>Approve</span>
        </button>
        <button
          type="button"
          onclick={() => flowStore.resolveApproval(id, 'modified')}
          class="inline-flex flex-1 cursor-pointer items-center justify-center gap-1 rounded-lg border border-zinc-700 bg-zinc-800 px-2 py-1 text-[11px] font-medium text-zinc-100 transition hover:bg-zinc-700"
        >
          <Pencil class="h-3 w-3 text-amber-300" />
          <span>Modify</span>
        </button>
      </div>
    {:else}
      <span class="rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-[10px] text-zinc-200">
        {resolution}
      </span>
    {/if}
  </div>

  <Handle type="source" position={sourcePosition} class="!h-2 !w-2 !border-zinc-700 !bg-zinc-400" />
</div>
