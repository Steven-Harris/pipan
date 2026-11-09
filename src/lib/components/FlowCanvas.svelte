<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import {
    SvelteFlow,
    Background,
    BackgroundVariant,
    Controls,
    MiniMap,
    Panel,
    MarkerType,
    useSvelteFlow,
    type Edge,
    type NodeTypes
  } from '@xyflow/svelte';
  import {
    GitFork,
    ArrowDown,
    ArrowRight,
    RotateCcw,
    X,
    Terminal,
    FolderOpen,
    Sparkles
  } from 'lucide-svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import { flowStore } from '#lib/stores/flow.svelte.ts';
  import { graphLayoutStore } from '#lib/stores/graph-layout.svelte.ts';
  import { workspaceStore } from '#lib/stores/workspace.svelte.ts';
  import {
    layoutGraph,
    estimatedSize,
    type FlowNode,
    type LayoutDirection
  } from '#lib/flow/layout.ts';
  import { ORCHESTRATOR_NODE_ID, summarizeToolArgs } from '#lib/flow/telemetry.ts';
  import type { FlowNodeMeta } from '#lib/types/pi.ts';
  import FlowNodeCard from './FlowNodeCard.svelte';
  import OrchestratorNode from './nodes/OrchestratorNode.svelte';
  import WorkerNode from './nodes/WorkerNode.svelte';
  import ActionNode from './nodes/ActionNode.svelte';
  import ApprovalNode from './nodes/ApprovalNode.svelte';

  type PiPanNode = FlowNode;

  const nodeTypes: NodeTypes = {
    pipan: FlowNodeCard,
    orchestrator: OrchestratorNode,
    worker: WorkerNode,
    action: ActionNode,
    approval: ApprovalNode
  };

  let nodes = $state.raw<PiPanNode[]>([]);
  let edges = $state.raw<Edge[]>([]);
  let canvasWidth = $state(0);
  let canvasHeight = $state(0);

  const flow = useSvelteFlow();

  const FIT_OPTIONS = { padding: 0.16, maxZoom: 1.0, duration: 200 } as const;

  const sessionId = $derived(chatStore.activeSessionId);
  // An explicit user choice always wins; otherwise the split orientation decides.
  const layoutDirection = $derived(
    graphLayoutStore.directionFor(sessionId, workspaceStore.orientation)
  );
  const pinnedCount = $derived(
    Object.values(graphLayoutStore.positionsFor(sessionId, layoutDirection)).filter(
      (p) => p.pinned !== false
    ).length
  );

  function buildOrchestrationGraph(): {
    nodes: PiPanNode[];
    edges: Edge[];
  } {
    const builtNodes: PiPanNode[] = [];
    const builtEdges: Edge[] = [];

    const sessionState = chatStore.sessionStatus.state;
    const isConnected = sessionState === 'running';
    const isStarting = sessionState === 'starting';
    const isError = sessionState === 'error';

    const supervisorStatus: FlowNodeMeta['status'] = isError
      ? 'error'
      : isStarting
        ? 'running'
        : isConnected
          ? 'idle'
          : 'disconnected';

    const workspaceBadge =
      chatStore.sessionStatus.mode === 'headless'
        ? 'headless temp'
        : chatStore.sessionStatus.cwd
          ? chatStore.sessionStatus.cwd.split(/[\\/]+/).filter(Boolean).slice(-2).join('/')
          : 'directory';

    builtNodes.push({
      id: 'session-supervisor',
      type: 'orchestrator',
      position: { x: 0, y: 0 },
      selected: chatStore.selectedNodeId === 'session-supervisor',
      data: {
        category: 'supervisor',
        title: 'Pi RPC Supervisor',
        subtitle: chatStore.sessionStatus.pid
          ? `PID ${chatStore.sessionStatus.pid} · JSONL stdio`
          : 'pi --mode rpc --no-session',
        badge: isConnected ? workspaceBadge : sessionState,
        status: supervisorStatus,
        detail: isConnected
          ? `Sidecar connected in ${chatStore.sessionStatus.mode} mode.`
          : 'Start a Headless or Folder session to spawn the Pi RPC sidecar.'
      }
    });

    const orchestratorStatus: FlowNodeMeta['status'] = !isConnected
      ? 'disconnected'
      : chatStore.isAgentRunning
        ? 'running'
        : 'idle';

    builtNodes.push({
      id: ORCHESTRATOR_NODE_ID,
      type: 'orchestrator',
      position: { x: 0, y: 0 },
      selected: chatStore.selectedNodeId === 'agent-orchestrator',
      data: {
        category: 'orchestrator',
        title: 'Orchestrator Agent',
        subtitle: chatStore.modelInfo?.id ?? (isConnected ? 'Awaiting model state' : 'Offline'),
        badge:
          chatStore.modelInfo?.thinkingLevel && chatStore.modelInfo.thinkingLevel !== 'off'
            ? `thinking: ${chatStore.modelInfo.thinkingLevel}`
            : isConnected
              ? 'ready'
              : 'standby',
        status: orchestratorStatus,
        detail: chatStore.isCompacting
          ? 'Compacting conversation context…'
          : chatStore.isAgentRunning
            ? 'Streaming response and tool telemetry over IPC.'
            : isConnected
              ? 'Listening for prompts, mid-flight steering, and follow-ups.'
              : 'No active agent loop.'
      }
    });

    builtEdges.push({
      id: 'e-supervisor-orchestrator',
      source: 'session-supervisor',
      target: 'agent-orchestrator',
      animated: isStarting || chatStore.isAgentRunning,
      style: isConnected ? 'stroke: #6366f1; stroke-width: 1.75px;' : 'stroke: #3f3f46; stroke-width: 1.5px;',
      markerEnd: {
        type: MarkerType.ArrowClosed,
        color: isConnected ? '#6366f1' : '#52525b'
      }
    });

    let spineNodeId = 'agent-orchestrator';

    for (let mIdx = 0; mIdx < chatStore.messages.length; mIdx++) {
      const msg = chatStore.messages[mIdx];

      if (msg.role === 'user') {
        const promptText =
          msg.blocks.find((b) => b.type === 'text')?.text ?? '(user message)';
        const userNodeId = `msg-user-${msg.id}`;

        builtNodes.push({
          id: userNodeId,
          type: 'pipan',
          position: { x: 0, y: 0 },
          selected: chatStore.selectedNodeId === userNodeId,
          data: {
            category: 'user_prompt',
            title: `User Turn #${mIdx + 1}`,
            subtitle: new Date(msg.timestamp).toLocaleTimeString([], {
              hour: '2-digit',
              minute: '2-digit',
              second: '2-digit'
            }),
            badge: msg.isOptimistic ? 'sending' : 'prompt',
            status: msg.isOptimistic ? 'running' : 'completed',
            detail: promptText,
            timestamp: msg.timestamp
          }
        });

        builtEdges.push({
          id: `e-${spineNodeId}-${userNodeId}`,
          source: spineNodeId,
          target: userNodeId,
          style: 'stroke: #38bdf8; stroke-width: 1.5px;',
          markerEnd: { type: MarkerType.ArrowClosed, color: '#38bdf8' }
        });

        spineNodeId = userNodeId;
        continue;
      }

      // Assistant message blocks: thinking, tool calls, and text reply
      let turnParentId = spineNodeId;
      let textContent = '';

      for (let bIdx = 0; bIdx < msg.blocks.length; bIdx++) {
        const block = msg.blocks[bIdx];

        if (block.type === 'thinking') {
          const thinkNodeId = `msg-${msg.id}-think-${bIdx}`;
          builtNodes.push({
            id: thinkNodeId,
            type: 'pipan',
            position: { x: 0, y: 0 },
            selected: chatStore.selectedNodeId === thinkNodeId,
            data: {
              category: 'thinking',
              title: block.isComplete ? 'Reasoning Trace' : 'Thinking…',
              subtitle: `Step ${bIdx + 1}`,
              badge: block.isComplete ? 'complete' : 'streaming',
              status: block.isComplete ? 'completed' : 'running',
              detail: block.thinking.trim() || 'Synthesizing reasoning trace…'
            }
          });

          builtEdges.push({
            id: `e-${turnParentId}-${thinkNodeId}`,
            source: turnParentId,
            target: thinkNodeId,
            animated: !block.isComplete,
            style: 'stroke: #8b5cf6; stroke-width: 1.5px;',
            markerEnd: { type: MarkerType.ArrowClosed, color: '#8b5cf6' }
          });

          turnParentId = thinkNodeId;
        } else if (block.type === 'tool_call') {
          const toolNodeId = `msg-${msg.id}-tool-${block.id}-${bIdx}`;
          const isToolRunning =
            block.status === 'running' || block.status === 'streaming_args';
          const isToolError = block.status === 'error';

          builtNodes.push({
            id: toolNodeId,
            type: 'action',
            position: { x: 0, y: 0 },
            selected: chatStore.selectedNodeId === toolNodeId,
            data: {
              category: 'tool_call',
              origin: 'pi',
              title: block.name,
              subtitle: summarizeToolArgs(block.args, block.argsJson),
              badge: block.status === 'streaming_args' ? 'args' : block.status,
              status: isToolError ? 'error' : isToolRunning ? 'running' : 'completed',
              detail:
                block.resultOutput?.slice(0, 240) ||
                block.partialOutput?.slice(0, 240) ||
                summarizeToolArgs(block.args, block.argsJson),
              durationMs: block.durationMs
            }
          });

          const strokeColor = isToolError
            ? '#f43f5e'
            : isToolRunning
              ? '#6366f1'
              : '#10b981';

          builtEdges.push({
            id: `e-${turnParentId}-${toolNodeId}`,
            source: turnParentId,
            target: toolNodeId,
            animated: isToolRunning,
            style: `stroke: ${strokeColor}; stroke-width: 1.75px;`,
            markerEnd: { type: MarkerType.ArrowClosed, color: strokeColor }
          });

          turnParentId = toolNodeId;
        } else if (block.type === 'text') {
          textContent += (textContent ? '\n' : '') + block.text;
        }
      }

      if (textContent.trim() || msg.isStreaming || msg.errorMessage) {
        const replyNodeId = `msg-reply-${msg.id}`;
        builtNodes.push({
          id: replyNodeId,
          type: 'pipan',
          position: { x: 0, y: 0 },
          selected: chatStore.selectedNodeId === replyNodeId,
          data: {
            category: 'assistant_reply',
            title: msg.isStreaming ? 'Assistant Streaming…' : 'Assistant Response',
            subtitle: new Date(msg.timestamp).toLocaleTimeString([], {
              hour: '2-digit',
              minute: '2-digit',
              second: '2-digit'
            }),
            badge: msg.stopReason ?? (msg.isStreaming ? 'streaming' : 'complete'),
            status: msg.errorMessage
              ? 'error'
              : msg.isStreaming
                ? 'running'
                : 'completed',
            detail:
              msg.errorMessage ??
              (textContent.trim() || 'Generating response…'),
            tokens: msg.usage?.totalTokens
          }
        });

        builtEdges.push({
          id: `e-${turnParentId}-${replyNodeId}`,
          source: turnParentId,
          target: replyNodeId,
          animated: Boolean(msg.isStreaming),
          style: 'stroke: #6366f1; stroke-width: 1.5px;',
          markerEnd: { type: MarkerType.ArrowClosed, color: '#6366f1' }
        });

        spineNodeId = replyNodeId;
      } else {
        spineNodeId = turnParentId;
      }
    }

    // Worktree agent sessions and GitHub activity come from IPC telemetry (flowStore).
    for (const node of flowStore.nodes) {
      builtNodes.push({ ...node, selected: chatStore.selectedNodeId === node.id });
    }
    builtEdges.push(...flowStore.edges);

    // Positions are assigned by the layout pass in `applyGraph`, not here.
    return { nodes: builtNodes, edges: builtEdges };
  }

  // The canvas unmounts with its panel tab, so saved layout is read from the store
  // (hydrated from pipan.db) before the first layout of a session is placed.
  $effect(() => {
    graphLayoutStore.hydrate(sessionId);
  });

  /**
   * Structural key: changes only when the graph's shape or layout inputs change (node or
   * edge added/removed, session, direction, Reset Layout). Streaming text, thinking and
   * tool status deltas never alter it, so they never re-run dagre.
   */
  let lastStructureKey = '';
  let mounted = false;

  onMount(() => {
    mounted = true;
    return () => {
      mounted = false;
    };
  });

  $effect(() => {
    const sid = sessionId;
    const dir = layoutDirection;
    if (!graphLayoutStore.isReady(sid)) return;
    const epoch = graphLayoutStore.epoch;
    const pinned = graphLayoutStore.positionsFor(sid, dir);
    // Reads (and so subscribes to) every chat/flow field the nodes display.
    const built = buildOrchestrationGraph();
    untrack(() => applyGraph(built, `${sid ?? ''}|${dir}|${epoch}`, dir, pinned));
  });

  function applyGraph(
    built: { nodes: PiPanNode[]; edges: Edge[] },
    context: string,
    dir: LayoutDirection,
    pinned: ReturnType<typeof graphLayoutStore.positionsFor>
  ) {
    const structureKey = [
      context,
      built.nodes.map((n) => n.id).join(','),
      built.edges.map((e) => e.id).join(',')
    ].join('\n');

    if (structureKey === lastStructureKey) {
      // Data-only update: refresh node data in place and keep every position.
      const fresh = new Map(built.nodes.map((n) => [n.id, n]));
      nodes = nodes.map((current) => {
        const next = fresh.get(current.id);
        return next
          ? { ...current, type: next.type, data: next.data, selected: next.selected }
          : current;
      });
      edges = built.edges;
      return;
    }

    const previousContext = lastStructureKey.split('\n', 1)[0];
    const sameContext = previousContext === context;
    lastStructureKey = structureKey;

    const placed = new Map<string, { x: number; y: number }>();
    const measured = new Map<string, { width: number; height: number }>();
    for (const n of nodes) {
      // Positions from another session, direction or a reset say nothing about this layout.
      if (sameContext) placed.set(n.id, { x: n.position.x, y: n.position.y });
      const { width, height } = n.measured ?? {};
      if (width && height) measured.set(n.id, { width, height });
    }

    const laidOut = layoutGraph(built.nodes, built.edges, dir, { pinned, placed, measured });
    const hadNodes = nodes.length > 0;
    nodes = laidOut.nodes;
    edges = laidOut.edges;

    const addedNodes = laidOut.nodes.filter((n) => !placed.has(n.id));
    if (!sameContext || !hadNodes || isOutsideViewport(addedNodes, measured)) {
      void fitToNodes();
    }
  }

  /** True if any of the nodes would render (even partly) outside the visible canvas. */
  function isOutsideViewport(
    added: PiPanNode[],
    measured: Map<string, { width: number; height: number }>
  ): boolean {
    if (!canvasWidth || !canvasHeight) return true;
    const { x, y, zoom } = flow.getViewport();
    return added.some((n) => {
      const { width, height } = measured.get(n.id) ?? estimatedSize(n);
      const left = n.position.x * zoom + x;
      const top = n.position.y * zoom + y;
      return (
        left < 0 ||
        top < 0 ||
        left + width * zoom > canvasWidth ||
        top + height * zoom > canvasHeight
      );
    });
  }

  /** Waits for the new nodes to render and be measured, then frames them. */
  async function fitToNodes() {
    await tick();
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        if (mounted) void flow.fitView(FIT_OPTIONS);
      })
    );
  }

  function onNodeDragStop({
    targetNode,
    nodes: dragged
  }: {
    targetNode: PiPanNode | null;
    nodes: PiPanNode[];
  }) {
    const moved: Record<string, { x: number; y: number }> = {};
    for (const n of dragged.length > 0 ? dragged : targetNode ? [targetNode] : []) {
      moved[n.id] = { x: n.position.x, y: n.position.y };
    }
    graphLayoutStore.pin(sessionId, layoutDirection, moved);
  }

  const selectedNode = $derived(
    chatStore.selectedNodeId
      ? nodes.find((n) => n.id === chatStore.selectedNodeId) ?? null
      : null
  );

  function setDirection(direction: LayoutDirection) {
    if (direction === layoutDirection) return;
    graphLayoutStore.setDirection(sessionId, direction);
  }

  function resetLayout() {
    graphLayoutStore.resetLayout(sessionId, layoutDirection);
  }

  async function handleSelectWorkspace() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Workspace Folder for Pi Session'
      });
      if (typeof selected === 'string' && selected.trim()) {
        await chatStore.startInDirectory(selected);
      }
    } catch (err) {
      chatStore.lastError = err instanceof Error ? err.message : String(err);
    }
  }
</script>

<div
  bind:clientWidth={canvasWidth}
  bind:clientHeight={canvasHeight}
  class="relative h-full min-h-0 w-full flex-1 overflow-hidden bg-zinc-950"
>
  <SvelteFlow
    bind:nodes
    bind:edges
    {nodeTypes}
    colorMode="dark"
    fitView
    fitViewOptions={{ padding: FIT_OPTIONS.padding, maxZoom: FIT_OPTIONS.maxZoom }}
    minZoom={0.2}
    maxZoom={1.75}
    nodesDraggable={true}
    nodesConnectable={false}
    elementsSelectable={true}
    proOptions={{ hideAttribution: true }}
    onnodedragstop={onNodeDragStop}
    onnodeclick={({ node }) => {
      chatStore.selectedNodeId = node.id;
    }}
    onpaneclick={() => {
      chatStore.selectedNodeId = null;
    }}
  >
    <Background variant={BackgroundVariant.Dots} gap={20} size={1.2} />
    <Controls position="bottom-left" showLock={false} />
    <MiniMap
      position="bottom-right"
      pannable
      zoomable
      height={92}
      width={140}
      nodeColor={(node) => {
        const meta = node.data as FlowNodeMeta | undefined;
        if (meta?.status === 'error') return '#f43f5e';
        if (meta?.status === 'running') return '#818cf8';
        if (meta?.category === 'worker') return '#22d3ee';
        if (meta?.category === 'approval') return '#f59e0b';
        if (meta?.category === 'tool_call' || meta?.category === 'action') return '#10b981';
        if (meta?.category === 'user_prompt') return '#38bdf8';
        return '#52525b';
      }}
    />

    <!-- Top-Left Canvas Toolbar -->
    <Panel position="top-left">
      <div class="flex flex-wrap items-center gap-2 rounded-xl border border-zinc-800/90 bg-zinc-900/90 px-3 py-1.5 text-xs text-zinc-200 shadow-lg backdrop-blur-md">
        <div class="flex items-center gap-1.5 font-medium text-zinc-100">
          <GitFork class="h-3.5 w-3.5 text-indigo-400" />
          <span>Orchestration DAG</span>
        </div>

        <span class="text-zinc-700">|</span>

        <span class="font-mono text-[11px] text-zinc-300 tabular-nums">
          {nodes.length} {nodes.length === 1 ? 'node' : 'nodes'} · {edges.length} {edges.length === 1 ? 'edge' : 'edges'}
        </span>

        <div
          role="group"
          aria-label="Graph layout direction"
          class="inline-flex overflow-hidden rounded-lg border border-zinc-700/80 bg-zinc-800/90"
        >
          <button
            type="button"
            aria-pressed={layoutDirection === 'TB'}
            onclick={() => setDirection('TB')}
            title="Lay out top to bottom"
            class="inline-flex items-center gap-1 px-2 py-1 text-[11px] font-medium cursor-pointer transition {layoutDirection ===
            'TB'
              ? 'bg-indigo-500/20 text-indigo-200'
              : 'text-zinc-300 hover:bg-zinc-700'}"
          >
            <ArrowDown class="h-3 w-3" />
            <span>Top → Bottom</span>
          </button>
          <button
            type="button"
            aria-pressed={layoutDirection === 'LR'}
            onclick={() => setDirection('LR')}
            title="Lay out left to right"
            class="inline-flex items-center gap-1 border-l border-zinc-700/80 px-2 py-1 text-[11px] font-medium cursor-pointer transition {layoutDirection ===
            'LR'
              ? 'bg-indigo-500/20 text-indigo-200'
              : 'text-zinc-300 hover:bg-zinc-700'}"
          >
            <ArrowRight class="h-3 w-3" />
            <span>Left → Right</span>
          </button>
        </div>

        <button
          type="button"
          onclick={resetLayout}
          disabled={pinnedCount === 0}
          title={pinnedCount === 0
            ? 'No dragged nodes to reset'
            : `Release ${pinnedCount} dragged ${pinnedCount === 1 ? 'node' : 'nodes'} and re-run auto-layout`}
          class="inline-flex items-center gap-1 rounded-lg border border-zinc-700/80 bg-zinc-800/90 px-2 py-1 text-[11px] font-medium text-zinc-200 transition hover:border-zinc-600 hover:bg-zinc-700 cursor-pointer disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:border-zinc-700/80 disabled:hover:bg-zinc-800/90"
        >
          <RotateCcw class="h-3 w-3 text-indigo-400" />
          <span>Reset Layout</span>
        </button>
      </div>
    </Panel>

    <!-- Disconnected Quick-Launch Banner on Canvas -->
    {#if chatStore.sessionStatus.state === 'disconnected'}
      <Panel position="top-right">
        <div class="flex items-center gap-2 rounded-xl border border-zinc-800/90 bg-zinc-900/95 px-3 py-2 text-xs text-zinc-200 shadow-lg backdrop-blur-md">
          <Sparkles class="h-3.5 w-3.5 text-indigo-400 shrink-0" />
          <span class="text-zinc-300">Launch a session to stream live telemetry:</span>
          <button
            type="button"
            onclick={() => chatStore.startHeadless()}
            class="inline-flex items-center gap-1.5 rounded-lg bg-indigo-600 px-2.5 py-1 text-xs font-semibold text-white transition hover:bg-indigo-500 cursor-pointer"
          >
            <Terminal class="h-3 w-3" />
            <span>Headless</span>
          </button>
          <button
            type="button"
            onclick={handleSelectWorkspace}
            class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-700 bg-zinc-800 px-2.5 py-1 text-xs font-medium text-zinc-200 transition hover:bg-zinc-700 cursor-pointer"
          >
            <FolderOpen class="h-3 w-3 text-sky-400" />
            <span>Folder…</span>
          </button>
        </div>
      </Panel>
    {/if}

    <!-- Selected Node Inspector Panel -->
    {#if selectedNode}
      <Panel position="top-right">
        <div class="w-80 rounded-xl border border-zinc-800 bg-zinc-900/95 p-3.5 text-xs text-zinc-200 shadow-xl backdrop-blur-md">
          <div class="flex items-start justify-between gap-2 border-b border-zinc-800 pb-2">
            <div class="min-w-0">
              <h3 class="truncate text-xs font-semibold text-zinc-100">
                {selectedNode.data.title}
              </h3>
              {#if selectedNode.data.subtitle}
                <p class="truncate font-mono text-[11px] text-zinc-300 tabular-nums">
                  {selectedNode.data.subtitle}
                </p>
              {/if}
            </div>
            <button
              type="button"
              onclick={() => (chatStore.selectedNodeId = null)}
              class="rounded p-1 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 cursor-pointer"
              title="Close node inspector"
            >
              <X class="h-3.5 w-3.5" />
            </button>
          </div>

          <div class="mt-2.5 space-y-2">
            <div class="flex items-center justify-between text-[11px] text-zinc-300 tabular-nums">
              <span>Status: <strong class="font-medium text-zinc-100">{selectedNode.data.status}</strong></span>
              {#if selectedNode.data.badge}
                <span class="rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-[10px] text-zinc-200">
                  {selectedNode.data.badge}
                </span>
              {/if}
            </div>

            {#if selectedNode.data.detail}
              <pre class="max-h-40 overflow-y-auto rounded-lg border border-zinc-800/90 bg-zinc-950 p-2.5 font-mono text-[11px] leading-relaxed text-zinc-300 whitespace-pre-wrap break-words">{selectedNode.data.detail}</pre>
            {/if}
          </div>
        </div>
      </Panel>
    {/if}
  </SvelteFlow>
</div>
