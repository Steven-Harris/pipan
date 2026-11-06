<script lang="ts">
  import { Handle, Position, type NodeProps, type Node } from '@xyflow/svelte';
  import { ShieldAlert, Check, Pencil, X, CornerDownLeft } from 'lucide-svelte';
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

  let editing = $state(false);
  let editedCommand = $state('');

  // Sync edited buffer when approval action changes and not actively editing
  $effect(() => {
    if (!editing) {
      editedCommand = data.approval?.action ?? '';
    }
  });

  function startEditing() {
    editedCommand = data.approval?.action ?? '';
    editing = true;
  }

  function cancelEditing() {
    editing = false;
  }

  function submitModified() {
    const val = editedCommand.trim();
    if (!val) return;
    editing = false;
    flowStore.resolveApproval(id, 'modified', val);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      submitModified();
    } else if (e.key === 'Escape') {
      cancelEditing();
    }
  }

  const frame = $derived.by(() => {
    if (selected) return 'border-amber-300 shadow-[0_10px_28px_-6px_rgba(251,191,36,0.3)]';
    if (pending) return 'border-amber-500/60 shadow-[0_8px_24px_-4px_rgba(245,158,11,0.2)]';
    if (resolution === 'rejected') return 'border-rose-900/60 shadow-[0_8px_20px_-4px_rgba(0,0,0,0.6)]';
    return 'border-zinc-700/90 shadow-[0_8px_20px_-4px_rgba(0,0,0,0.6)]';
  });
</script>

<div class="w-[290px] rounded-xl border bg-zinc-900 px-3 py-2.5 text-left transition-colors {frame}">
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

  {#if editing}
    <!-- Inline command editor when user clicks "Modify" -->
    <div class="nodrag nopan mt-2">
      <textarea
        bind:value={editedCommand}
        onkeydown={onKeydown}
        rows={3}
        class="w-full resize-none rounded-lg border border-amber-500/40 bg-zinc-950 p-2 font-mono text-[11px] leading-snug text-zinc-100 placeholder-zinc-500 focus:border-amber-400 focus:outline-none focus:ring-1 focus:ring-amber-400"
        placeholder="Edit command..."
      ></textarea>
      <div class="mt-1.5 flex items-center justify-between gap-1 text-[10px] text-zinc-400">
        <span>Ctrl/⌘+Enter to submit</span>
        <div class="flex items-center gap-1.5">
          <button
            type="button"
            onclick={cancelEditing}
            class="rounded px-1.5 py-0.5 hover:bg-zinc-800 text-zinc-300 transition"
          >
            Cancel
          </button>
          <button
            type="button"
            onclick={submitModified}
            class="inline-flex items-center gap-1 rounded bg-amber-500 px-2 py-0.5 font-semibold text-zinc-950 hover:bg-amber-400 transition"
          >
            <CornerDownLeft class="h-2.5 w-2.5" />
            <span>Apply</span>
          </button>
        </div>
      </div>
    </div>
  {:else}
    {#if data.approval}
      <pre class="mt-2 max-h-16 overflow-hidden whitespace-pre-wrap break-words rounded-lg border border-zinc-800 bg-zinc-950 p-2 font-mono text-[11px] leading-snug text-zinc-200">{data.approval.action}</pre>
    {/if}
    {#if data.approval?.replacement}
      <div class="mt-1 rounded border border-amber-900/40 bg-amber-950/20 px-2 py-1 font-mono text-[10px] text-amber-200">
        <span class="font-semibold text-amber-300">Modified: </span>{data.approval.replacement}
      </div>
    {/if}
    {#if data.detail && data.detail !== data.approval?.action && data.detail !== `Modified: ${data.approval?.replacement}`}
      <p class="mt-1.5 line-clamp-2 break-words text-[11px] leading-snug text-zinc-300">{data.detail}</p>
    {/if}
  {/if}

  <div class="mt-2 border-t border-zinc-800/80 pt-2">
    {#if pending && !editing}
      <!-- `nodrag nopan` keeps Svelte Flow from treating button presses as node drags. -->
      <div class="nodrag nopan flex items-center gap-1.5">
        <button
          type="button"
          onclick={() => flowStore.resolveApproval(id, 'approved')}
          class="inline-flex flex-1 cursor-pointer items-center justify-center gap-1 rounded-lg bg-emerald-600 px-2 py-1 text-[11px] font-semibold text-white transition hover:bg-emerald-500 shadow-sm"
          title="Approve this command and continue execution"
        >
          <Check class="h-3 w-3" />
          <span>Approve</span>
        </button>
        <button
          type="button"
          onclick={startEditing}
          class="inline-flex flex-1 cursor-pointer items-center justify-center gap-1 rounded-lg border border-zinc-700 bg-zinc-800 px-2 py-1 text-[11px] font-medium text-zinc-100 transition hover:bg-zinc-700 shadow-sm"
          title="Edit the command before running"
        >
          <Pencil class="h-3 w-3 text-amber-300" />
          <span>Modify</span>
        </button>
        <button
          type="button"
          onclick={() => flowStore.resolveApproval(id, 'rejected')}
          class="inline-flex cursor-pointer items-center justify-center rounded-lg border border-zinc-700/80 bg-zinc-800/60 p-1 text-zinc-400 hover:border-rose-900 hover:bg-rose-950/40 hover:text-rose-300 transition"
          title="Reject this command"
        >
          <X class="h-3.5 w-3.5" />
        </button>
      </div>
    {:else if !editing}
      <div class="flex items-center justify-between text-[10px]">
        <span
          class="rounded px-1.5 py-0.5 font-mono uppercase tracking-wide {resolution === 'approved'
            ? 'bg-emerald-950 text-emerald-300 border border-emerald-800/60'
            : resolution === 'modified'
              ? 'bg-amber-950 text-amber-300 border border-amber-800/60'
              : 'bg-rose-950 text-rose-300 border border-rose-800/60'}"
        >
          {resolution}
        </span>
        <span class="text-zinc-500 font-mono">Plan Gate</span>
      </div>
    {/if}
  </div>

  <Handle type="source" position={sourcePosition} class="!h-2 !w-2 !border-zinc-700 !bg-zinc-400" />
</div>
