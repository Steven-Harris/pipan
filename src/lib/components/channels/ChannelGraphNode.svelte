<script lang="ts">
  import { Handle, Position, type NodeProps } from '@xyflow/svelte';
  import { kindGlyph, resolveTone, STATUS_STYLES, TONE_CLASSES } from '#lib/channels/graph-style.ts';
  import type { ChannelFlowNode } from '#lib/channels/flow-types.ts';

  let {
    data,
    selected = false,
    targetPosition = Position.Top,
    sourcePosition = Position.Bottom
  }: NodeProps<ChannelFlowNode> = $props();

  const node = $derived(data.node);
  const tone = $derived(TONE_CLASSES[resolveTone(node.kind, node.tone)]);
  const status = $derived(STATUS_STYLES[node.status]);
  const glyph = $derived(kindGlyph(node.kind));
</script>

<Handle type="target" position={targetPosition} class="!h-1.5 !w-1.5 !border-0 !bg-zinc-600" />
<div
  class="w-[220px] rounded-lg border bg-zinc-900/95 px-2.5 py-2 text-xs shadow-md {tone.border} {status.card} {selected
    ? 'outline outline-1 outline-indigo-400'
    : ''}"
  title={node.label}
>
  <div class="flex items-center gap-2">
    {#if glyph}
      <span
        class="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded text-[10px] font-semibold {tone.badge}"
        aria-hidden="true">{glyph}</span
      >
    {/if}
    <span class="min-w-0 flex-1 truncate font-medium text-zinc-100">{node.label}</span>
    {#if node.visits && node.visits > 1}
      <span class="shrink-0 rounded bg-zinc-800 px-1 font-mono text-[10px] text-zinc-300" title="Times entered"
        >x{node.visits}</span
      >
    {/if}
  </div>
  <div class="mt-1.5 flex items-center gap-1.5 text-[11px] {tone.text}">
    <span class="h-2 w-2 shrink-0 rounded-full {status.dot}" aria-hidden="true"></span>
    <span>{status.label}</span>
    {#if node.kind}
      <span class="truncate text-zinc-500">{node.kind}</span>
    {/if}
    {#if node.group}
      <span class="ml-auto truncate text-zinc-500" title="Group {node.group}">in {node.group}</span>
    {/if}
  </div>
</div>
<Handle type="source" position={sourcePosition} class="!h-1.5 !w-1.5 !border-0 !bg-zinc-600" />
