<script lang="ts">
  import {
    SvelteFlow,
    Background,
    BackgroundVariant,
    Controls,
    MiniMap,
    Panel,
    Position,
    MarkerType,
    type Node,
    type Edge,
    type NodeTypes
  } from '@xyflow/svelte';
  import dagre from 'dagre';
  import {
    GitFork,
    ArrowDownUp,
    ArrowLeftRight,
    X,
    Terminal,
    FolderOpen,
    Sparkles
  } from 'lucide-svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import type { FlowNodeMeta } from '#lib/types/pi.ts';
  import FlowNodeCard from './FlowNodeCard.svelte';

  type PipanNode = Node<FlowNodeMeta, 'pipan'>;

  const nodeTypes: NodeTypes = {
    pipan: FlowNodeCard
  };

  const NODE_WIDTH = 244;
  const NODE_HEIGHT = 112;

  let layoutDirection = $state<'TB' | 'LR'>('TB');
  let nodes = $state.raw<PipanNode[]>([]);
  let edges = $state.raw<Edge[]>([]);

  function summarizeToolArgs(args?: Record<string, unknown>, argsJson?: string): string {
    if (args && typeof args === 'object') {
      const keys = ['command', 'path', 'pattern', 'query', 'url', 'file'];
      for (const k of keys) {
        if (typeof args[k] === 'string' && String(args[k]).trim()) {
          return String(args[k]).trim();
        }
      }
    }
    if (argsJson) {
      return argsJson.replace(/\s+/g, ' ').trim();
    }
    return 'Executing tool call';
  }

  function applyDagreLayout(
    rawNodes: PipanNode[],
    rawEdges: Edge[],
    direction: 'TB' | 'LR'
  ): { nodes: PipanNode[]; edges: Edge[] } {
    const g = new dagre.graphlib.Graph();
    g.setDefaultEdgeLabel(() => ({}));
    g.setGraph({
      rankdir: direction,
      nodesep: direction === 'TB' ? 48 : 36,
      ranksep: direction === 'TB' ? 68 : 84,
      marginx: 32,
      marginy: 32
    });

    for (const node of rawNodes) {
      g.setNode(node.id, { width: NODE_WIDTH, height: NODE_HEIGHT });
    }

    for (const edge of rawEdges) {
      g.setEdge(edge.source, edge.target);
    }

    dagre.layout(g);

    const isHorizontal = direction === 'LR';
    const targetPosition = isHorizontal ? Position.Left : Position.Top;
    const sourcePosition = isHorizontal ? Position.Right : Position.Bottom;

    const layoutedNodes: PipanNode[] = rawNodes.map((node) => {
      const pos = g.node(node.id);
      return {
        ...node,
        targetPosition,
        sourcePosition,
        position: {
          x: (pos?.x ?? 0) - NODE_WIDTH / 2,
          y: (pos?.y ?? 0) - NODE_HEIGHT / 2
        }
      };
    });

    return { nodes: layoutedNodes, edges: rawEdges };
  }

  function buildOrchestrationGraph(direction: 'TB' | 'LR'): {
    nodes: PipanNode[];
    edges: Edge[];
  } {
    const builtNodes: PipanNode[] = [];
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
      type: 'pipan',
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
      id: 'agent-orchestrator',
      type: 'pipan',
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
            type: 'pipan',
            position: { x: 0, y: 0 },
            selected: chatStore.selectedNodeId === toolNodeId,
            data: {
              category: 'tool_call',
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

    return applyDagreLayout(builtNodes, builtEdges, direction);
  }

  // Reactive effect to recompute DAG layout when telemetry or layout direction changes
  $effect(() => {
    const _dir = layoutDirection;
    const _state = chatStore.sessionStatus.state;
    const _pid = chatStore.sessionStatus.pid;
    const _running = chatStore.isAgentRunning;
    const _model = chatStore.modelInfo?.id;
    const _selected = chatStore.selectedNodeId;
    const _msgCount = chatStore.messages.length;
    const lastMsg = chatStore.messages[_msgCount - 1];
    const _blockCount = lastMsg?.blocks.length ?? 0;
    const lastBlock = lastMsg?.blocks[_blockCount - 1];
    const _delta =
      lastBlock?.type === 'text'
        ? lastBlock.text.length
        : lastBlock?.type === 'thinking'
          ? lastBlock.thinking.length
          : lastBlock?.type === 'tool_call'
            ? (lastBlock.resultOutput?.length ?? 0) +
              (lastBlock.partialOutput?.length ?? 0) +
              lastBlock.status.length
            : 0;

    const layouted = buildOrchestrationGraph(_dir);
    nodes = layouted.nodes;
    edges = layouted.edges;
  });

  const selectedNode = $derived(
    chatStore.selectedNodeId
      ? nodes.find((n) => n.id === chatStore.selectedNodeId) ?? null
      : null
  );

  function toggleDirection() {
    layoutDirection = layoutDirection === 'TB' ? 'LR' : 'TB';
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

<div class="relative flex-1 min-h-[200px] w-full overflow-hidden bg-zinc-950">
  <SvelteFlow
    bind:nodes
    bind:edges
    {nodeTypes}
    colorMode="dark"
    fitView
    fitViewOptions={{ padding: 0.24, maxZoom: 1.15 }}
    minZoom={0.2}
    maxZoom={1.75}
    nodesDraggable={true}
    nodesConnectable={false}
    elementsSelectable={true}
    proOptions={{ hideAttribution: true }}
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
        if (meta?.category === 'tool_call') return '#10b981';
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

        <button
          type="button"
          onclick={toggleDirection}
          class="inline-flex items-center gap-1 rounded-lg border border-zinc-700/80 bg-zinc-800/90 px-2 py-1 text-[11px] font-medium text-zinc-200 transition hover:border-zinc-600 hover:bg-zinc-700 cursor-pointer"
          title="Toggle Dagre auto-layout orientation"
        >
          {#if layoutDirection === 'TB'}
            <ArrowDownUp class="h-3 w-3 text-indigo-400" />
            <span>Top-Down</span>
          {:else}
            <ArrowLeftRight class="h-3 w-3 text-indigo-400" />
            <span>Left-Right</span>
          {/if}
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
