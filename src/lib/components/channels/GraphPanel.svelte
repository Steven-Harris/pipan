<script lang="ts">
  // Live panel for one `graph` topic published by an extension over the channel. The host draws
  // what the producer sends and reports button presses back as `graph.action` intents.
  import { tick, untrack } from 'svelte';
  import {
    Background,
    BackgroundVariant,
    Controls,
    MarkerType,
    Panel,
    SvelteFlow,
    useSvelteFlow,
    type Edge,
    type Node,
    type NodeTypes
  } from '@xyflow/svelte';
  import { RotateCcw, X } from 'lucide-svelte';
  import { handlePositions } from '#lib/flow/layout.ts';
  import type { ChannelFlowNode } from '#lib/channels/flow-types.ts';
  import { edgeIds, layoutChannelGraph, structureKey, type XY } from '#lib/channels/graph-layout.ts';
  import { TONE_CLASSES, STATUS_STYLES, resolveTone } from '#lib/channels/graph-style.ts';
  import { renderMarkdown } from '#lib/channels/markdown.ts';
  import type { GraphAction, GraphNode, GraphState } from '#lib/channels/types.ts';
  import { channelGraphLayout } from '#lib/stores/channel-graph-layout.svelte.ts';
  import { channels } from '#lib/stores/channels.svelte.ts';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import ChannelGraphNodeCard from './ChannelGraphNode.svelte';

  let { ch, topic }: { ch: string; topic: string } = $props();

  const nodeTypes: NodeTypes = { channel: ChannelGraphNodeCard };
  const flow = useSvelteFlow();

  const sessionId = $derived(chatStore.activeSessionId ?? '');
  const published = $derived(
    channels.statesFor(sessionId).find((s) => s.ch === ch && s.topic === topic && s.primitive === 'graph')
  );
  const graph = $derived(published?.data as GraphState | undefined);
  const direction = $derived(graph?.direction ?? 'TB');
  const pinned = $derived(graph ? channelGraphLayout.positionsFor(sessionId, graph.id) : {});
  const pinnedCount = $derived(graph ? channelGraphLayout.pinnedCount(sessionId, graph.id) : 0);
  const ready = $derived(graph ? channelGraphLayout.isReady(sessionId, graph.id) : false);

  $effect(() => {
    if (graph && sessionId) void channelGraphLayout.hydrate(sessionId, graph.id);
  });

  // Layout runs when the node or edge set, the direction or the pinned positions change, not
  // when only statuses or counts do.
  let positions = $state.raw<Record<string, XY>>({});
  let layoutKey = '';
  $effect(() => {
    if (!graph || !ready) return;
    const key = `${structureKey(graph, direction)}\u0002${JSON.stringify(pinned)}`;
    if (key === layoutKey) return;
    layoutKey = key;
    const g = graph;
    const nodeCountChanged = untrack(() => Object.keys(positions).length) !== g.nodes.length;
    positions = layoutChannelGraph(g, direction, pinned);
    if (nodeCountChanged) void fitSoon();
  });

  let nodes = $state.raw<ChannelFlowNode[]>([]);
  let edges = $state.raw<Edge[]>([]);
  let selectedId = $state<string | null>(null);
  const dragging = new Set<string>();

  $effect(() => {
    if (!graph) {
      nodes = [];
      edges = [];
      return;
    }
    const current = untrack(() => new Map(nodes.map((n) => [n.id, n.position])));
    const handles = handlePositions(direction);
    nodes = graph.nodes.map((node) => ({
      id: node.id,
      type: 'channel' as const,
      // A node being dragged keeps its live position through a data update.
      position:
        (dragging.has(node.id) ? current.get(node.id) : undefined) ?? positions[node.id] ?? { x: 0, y: 0 },
      data: { node },
      selected: selectedId === node.id,
      ...handles
    }));
    const ids = edgeIds(graph);
    const byId = new Map(graph.nodes.map((n) => [n.id, n]));
    const active = new Set(graph.active ?? []);
    edges = graph.edges.map((edge, i) => {
      const taken = (edge.taken ?? 0) > 0;
      const live = active.has(edge.to) || byId.get(edge.to)?.status === 'active';
      const color = taken ? '#38bdf8' : '#52525b';
      return {
        id: ids[i],
        source: edge.from,
        target: edge.to,
        label: [edge.label, (edge.taken ?? 0) > 1 ? `x${edge.taken}` : ''].filter(Boolean).join(' ') || undefined,
        animated: live,
        style: `stroke: ${color}; stroke-width: 1.5px;${taken ? '' : ' stroke-dasharray: 4 3;'}`,
        markerEnd: { type: MarkerType.ArrowClosed, color }
      };
    });
  });

  async function fitSoon() {
    await tick();
    requestAnimationFrame(() =>
      requestAnimationFrame(() => void flow.fitView({ padding: 0.16, maxZoom: 1, duration: 200 }))
    );
  }

  function onNodeDragStart({ targetNode }: { targetNode: Node | null }) {
    if (targetNode) dragging.add(targetNode.id);
  }

  function onNodeDragStop({ targetNode, nodes: dragged }: { targetNode: Node | null; nodes: Node[] }) {
    const moved: Record<string, XY> = {};
    for (const n of dragged.length > 0 ? dragged : targetNode ? [targetNode] : []) {
      moved[n.id] = { x: n.position.x, y: n.position.y };
    }
    dragging.clear();
    if (graph) channelGraphLayout.pin(sessionId, graph.id, moved);
  }

  const selectedNode = $derived<GraphNode | undefined>(graph?.nodes.find((n) => n.id === selectedId));
  // Drop the selection when the node leaves the graph.
  $effect(() => {
    if (selectedId && graph && !selectedNode) selectedId = null;
  });
  const detailHtml = $derived(selectedNode?.detail?.markdown ? renderMarkdown(selectedNode.detail.markdown) : '');

  // ---- Actions ----
  let confirming = $state<{ action: GraphAction; nodeId?: string } | null>(null);
  let sendFailed = $state(false);

  async function press(action: GraphAction, nodeId?: string, confirmed = false) {
    if (!graph || action.enabled === false) return;
    if (action.confirm && !confirmed) {
      confirming = { action, nodeId };
      return;
    }
    confirming = null;
    const sent = await channels.sendIntent(sessionId, ch, 'graph.action', {
      graphId: graph.id,
      ...(nodeId ? { nodeId } : {}),
      actionId: action.id
    });
    sendFailed = !sent;
  }

  const actionClass = (style: GraphAction['style']) =>
    style === 'primary'
      ? 'border-indigo-500 bg-indigo-600 text-white hover:bg-indigo-500'
      : style === 'danger'
        ? 'border-red-600/70 bg-red-900/40 text-red-100 hover:bg-red-800/60'
        : 'border-zinc-700 bg-zinc-800 text-zinc-200 hover:bg-zinc-700';

  function formatDuration(ms: number): string {
    return ms < 1000 ? `${Math.round(ms)}ms` : `${(ms / 1000).toFixed(1)}s`;
  }
  const formatTime = (epochMs: number) => new Date(epochMs).toLocaleTimeString();
</script>

<div class="relative flex h-full min-h-0 w-full flex-1 flex-col overflow-hidden bg-zinc-950">
  {#if !graph}
    <div class="flex flex-1 items-center justify-center p-6 text-center text-xs text-zinc-400">
      Waiting for {ch} to publish "{topic}".
    </div>
  {:else}
    <header class="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-zinc-800 px-3 py-2 text-xs">
      <div class="min-w-0">
        <div class="truncate font-medium text-zinc-100">{graph.title ?? topic}</div>
        {#if graph.subtitle}<div class="truncate text-[11px] text-zinc-400">{graph.subtitle}</div>{/if}
      </div>
      {#if graph.note}<span class="min-w-0 truncate text-[11px] text-zinc-300">{graph.note}</span>{/if}
      <div class="ml-auto flex items-center gap-1.5">
        {#each graph.actions ?? [] as action (action.id)}
          <button
            type="button"
            disabled={action.enabled === false}
            onclick={() => press(action)}
            class="rounded-md border px-2 py-1 text-[11px] font-medium transition cursor-pointer disabled:cursor-not-allowed disabled:opacity-40 {actionClass(
              action.style
            )}">{action.label}</button
          >
        {/each}
        <button
          type="button"
          onclick={() => graph && channelGraphLayout.reset(sessionId, graph.id)}
          disabled={pinnedCount === 0}
          title={pinnedCount === 0 ? 'No dragged nodes to reset' : `Release ${pinnedCount} dragged nodes`}
          class="inline-flex items-center gap-1 rounded-md border border-zinc-700 bg-zinc-800 px-2 py-1 text-[11px] text-zinc-200 transition hover:bg-zinc-700 cursor-pointer disabled:cursor-not-allowed disabled:opacity-40"
        >
          <RotateCcw class="h-3 w-3 text-indigo-400" /> Reset layout
        </button>
      </div>
    </header>

    {#if confirming}
      <div
        role="alertdialog"
        aria-label="Confirm action"
        class="flex shrink-0 items-center gap-3 border-b border-amber-700/50 bg-amber-950/40 px-3 py-2 text-xs text-amber-100"
      >
        <span class="min-w-0 flex-1">{confirming.action.confirm}</span>
        <button
          type="button"
          onclick={() => confirming && press(confirming.action, confirming.nodeId, true)}
          class="rounded-md border px-2 py-1 font-medium cursor-pointer {actionClass(confirming.action.style ?? 'primary')}"
          >{confirming.action.label}</button
        >
        <button
          type="button"
          onclick={() => (confirming = null)}
          class="rounded-md border border-zinc-700 bg-zinc-800 px-2 py-1 text-zinc-200 hover:bg-zinc-700 cursor-pointer"
          >Cancel</button
        >
      </div>
    {/if}
    {#if sendFailed}
      <div class="shrink-0 border-b border-red-800/60 bg-red-950/40 px-3 py-1.5 text-[11px] text-red-200">
        The action could not be sent because the session is not connected.
      </div>
    {/if}
    {#if published && published.notices.length > 0}
      <div class="shrink-0 border-b border-amber-800/50 bg-amber-950/30 px-3 py-1.5 text-[11px] text-amber-200">
        {published.notices.join(' ')}
      </div>
    {/if}

    <div class="relative min-h-0 flex-1">
      <SvelteFlow
        bind:nodes
        bind:edges
        {nodeTypes}
        colorMode="dark"
        fitView
        fitViewOptions={{ padding: 0.16, maxZoom: 1 }}
        minZoom={0.2}
        maxZoom={1.75}
        nodesDraggable={true}
        nodesConnectable={false}
        elementsSelectable={true}
        proOptions={{ hideAttribution: true }}
        onnodedragstart={onNodeDragStart}
        onnodedragstop={onNodeDragStop}
        onnodeclick={({ node }) => (selectedId = node.id)}
        onpaneclick={() => (selectedId = null)}
      >
        <Background variant={BackgroundVariant.Dots} gap={20} size={1.2} />
        <Controls position="bottom-left" showLock={false} />

        {#if selectedNode}
          {@const detail = selectedNode.detail}
          <Panel position="top-right">
            <div
              class="max-h-[70vh] w-80 overflow-y-auto rounded-xl border border-zinc-800 bg-zinc-900/95 p-3.5 text-xs text-zinc-200 shadow-xl backdrop-blur-md"
            >
              <div class="flex items-start justify-between gap-2 border-b border-zinc-800 pb-2">
                <div class="min-w-0">
                  <h3 class="truncate text-xs font-semibold text-zinc-100">{selectedNode.label}</h3>
                  <p class="text-[11px] {TONE_CLASSES[resolveTone(selectedNode.kind, selectedNode.tone)].text}">
                    {STATUS_STYLES[selectedNode.status].label}{selectedNode.kind
                      ? ` - ${selectedNode.kind}`
                      : ''}{selectedNode.visits && selectedNode.visits > 1
                      ? ` - entered ${selectedNode.visits} times`
                      : ''}
                  </p>
                </div>
                <button
                  type="button"
                  onclick={() => (selectedId = null)}
                  class="rounded p-1 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 cursor-pointer"
                  title="Close details"
                  aria-label="Close details"
                >
                  <X class="h-3.5 w-3.5" />
                </button>
              </div>

              <div class="mt-2.5 space-y-2.5">
                {#if detail?.summary}<p class="text-zinc-300">{detail.summary}</p>{/if}
                {#if detail?.fields && detail.fields.length > 0}
                  <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[11px]">
                    {#each detail.fields as field, i (i)}
                      <dt class="text-zinc-500">{field.label}</dt>
                      <dd class="min-w-0 break-words text-zinc-200">{field.value}</dd>
                    {/each}
                  </dl>
                {/if}
                {#if detail && (detail.startedAt !== undefined || detail.endedAt !== undefined || detail.durationMs !== undefined || detail.costUsd !== undefined || detail.tokens !== undefined)}
                  <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[11px] tabular-nums">
                    {#if detail.startedAt !== undefined}
                      <dt class="text-zinc-500">Started</dt>
                      <dd>{formatTime(detail.startedAt)}</dd>
                    {/if}
                    {#if detail.endedAt !== undefined}
                      <dt class="text-zinc-500">Ended</dt>
                      <dd>{formatTime(detail.endedAt)}</dd>
                    {/if}
                    {#if detail.durationMs !== undefined}
                      <dt class="text-zinc-500">Duration</dt>
                      <dd>{formatDuration(detail.durationMs)}</dd>
                    {/if}
                    {#if detail.tokens !== undefined}
                      <dt class="text-zinc-500">Tokens</dt>
                      <dd>{detail.tokens.toLocaleString()}</dd>
                    {/if}
                    {#if detail.costUsd !== undefined}
                      <dt class="text-zinc-500">Cost</dt>
                      <dd>${detail.costUsd.toFixed(4)}</dd>
                    {/if}
                  </dl>
                {/if}
                {#if detailHtml}
                  <!-- Sanitized by renderMarkdown: raw HTML is escaped, images dropped, links restricted. -->
                  <div class="channel-md text-zinc-300">{@html detailHtml}</div>
                {/if}
                {#if selectedNode.actions && selectedNode.actions.length > 0}
                  <div class="flex flex-wrap gap-1.5 border-t border-zinc-800 pt-2.5">
                    {#each selectedNode.actions as action (action.id)}
                      <button
                        type="button"
                        disabled={action.enabled === false}
                        onclick={() => press(action, selectedNode.id)}
                        class="rounded-md border px-2 py-1 text-[11px] font-medium transition cursor-pointer disabled:cursor-not-allowed disabled:opacity-40 {actionClass(
                          action.style
                        )}">{action.label}</button
                      >
                    {/each}
                  </div>
                {/if}
              </div>
            </div>
          </Panel>
        {/if}
      </SvelteFlow>
    </div>
  {/if}
</div>

<style>
  .channel-md :global(p) {
    margin: 0 0 0.5rem;
  }
  .channel-md :global(h1),
  .channel-md :global(h2),
  .channel-md :global(h3) {
    margin: 0.5rem 0 0.25rem;
    font-weight: 600;
    color: rgb(244 244 245);
  }
  .channel-md :global(ul),
  .channel-md :global(ol) {
    margin: 0 0 0.5rem 1.1rem;
    list-style: revert;
  }
  .channel-md :global(a) {
    color: rgb(56 189 248);
    text-decoration: underline;
  }
  .channel-md :global(code) {
    background: rgb(39 39 42);
    border-radius: 0.25rem;
    padding: 0 0.25rem;
    font-size: 0.7rem;
  }
  .channel-md :global(pre) {
    overflow-x: auto;
    background: rgb(9 9 11);
    border-radius: 0.375rem;
    padding: 0.5rem;
    margin: 0 0 0.5rem;
  }
  .channel-md :global(pre code) {
    background: none;
    padding: 0;
  }
</style>
