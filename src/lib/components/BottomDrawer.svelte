<script lang="ts">
  import {
    MessageSquare,
    Terminal,
    ChevronDown,
    ChevronUp,
    Maximize2,
    Minimize2,
    Columns2,
    GripHorizontal
  } from 'lucide-svelte';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import MessageList from './MessageList.svelte';
  import ChatInput from './ChatInput.svelte';

  let showStderrDrawer = $state(false);
  let customHeight = $state<number | null>(null);
  let isDragging = $state(false);

  const runningToolCount = $derived.by(() => {
    let count = 0;
    for (const msg of chatStore.messages) {
      for (const b of msg.blocks) {
        if (
          b.type === 'tool_call' &&
          (b.status === 'running' || b.status === 'streaming_args')
        ) {
          count += 1;
        }
      }
    }
    return count;
  });

  const drawerStyle = $derived.by(() => {
    if (chatStore.drawerMode === 'collapsed') {
      return 'height: auto;';
    }
    if (chatStore.drawerMode === 'expanded') {
      return 'height: 72%;';
    }
    if (customHeight !== null) {
      return `height: ${customHeight}px;`;
    }
    return 'height: 44%;';
  });

  function startResize(e: PointerEvent) {
    e.preventDefault();
    isDragging = true;
    if (chatStore.drawerMode === 'collapsed') {
      chatStore.setDrawerMode('split');
    }

    const onPointerMove = (moveEvent: PointerEvent) => {
      if (!isDragging) return;
      const viewportHeight = window.innerHeight;
      const desired = viewportHeight - moveEvent.clientY;
      const clamped = Math.max(180, Math.min(viewportHeight - 180, desired));
      customHeight = clamped;
      chatStore.setDrawerMode('split');
    };

    const onPointerUp = () => {
      isDragging = false;
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', onPointerUp);
    };

    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', onPointerUp);
  }
</script>

<section
  aria-label="Chat transcript and steering drawer"
  style={drawerStyle}
  class="flex shrink-0 flex-col border-t border-zinc-800 bg-zinc-950/95 transition-[height] duration-150 ease-out"
>
  <!-- Drag Handle + Drawer Header Bar -->
  <div class="flex items-center justify-between gap-3 border-b border-zinc-800/80 bg-zinc-900/80 px-4 py-1.5 text-xs select-none">
    <div class="flex min-w-0 items-center gap-2.5">
      <button
        type="button"
        onpointerdown={startResize}
        class="inline-flex items-center text-zinc-400 hover:text-zinc-200 cursor-ns-resize"
        title="Drag to resize chat drawer"
      >
        <GripHorizontal class="h-3.5 w-3.5" />
      </button>

      <div class="flex items-center gap-1.5 font-medium text-zinc-100">
        <MessageSquare class="h-3.5 w-3.5 text-indigo-400" />
        <span>Transcript &amp; Steering</span>
      </div>

      <span class="rounded-full bg-zinc-800 px-2 py-0.5 font-mono text-[10px] text-zinc-200 tabular-nums">
        {chatStore.messages.length} {chatStore.messages.length === 1 ? 'turn' : 'turns'}
      </span>

      {#if runningToolCount > 0}
        <span class="inline-flex items-center gap-1 rounded-full bg-indigo-500/15 px-2 py-0.5 font-mono text-[10px] text-indigo-200 tabular-nums">
          <span class="h-1.5 w-1.5 rounded-full bg-indigo-400 animate-ping"></span>
          {runningToolCount} {runningToolCount === 1 ? 'tool active' : 'tools active'}
        </span>
      {/if}

      {#if chatStore.stderrLines.length > 0}
        <button
          type="button"
          onclick={() => (showStderrDrawer = !showStderrDrawer)}
          class="inline-flex items-center gap-1.5 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-0.5 text-[11px] text-amber-200 hover:bg-amber-500/20 cursor-pointer tabular-nums"
        >
          <Terminal class="h-3 w-3 text-amber-400" />
          <span>Diagnostics ({chatStore.stderrLines.length})</span>
          {#if showStderrDrawer}
            <ChevronUp class="h-3 w-3" />
          {:else}
            <ChevronDown class="h-3 w-3" />
          {/if}
        </button>
      {/if}
    </div>

    <!-- Right: Drawer Sizing Controls -->
    <div class="flex items-center gap-1">
      <button
        type="button"
        onclick={() => {
          customHeight = null;
          chatStore.setDrawerMode('collapsed');
        }}
        class="inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium transition cursor-pointer {chatStore.drawerMode ===
        'collapsed'
          ? 'bg-zinc-800 text-zinc-100'
          : 'text-zinc-300 hover:bg-zinc-800/60 hover:text-zinc-100'}"
        title="Minimize transcript to focus on the DAG canvas"
      >
        <Minimize2 class="h-3 w-3" />
        <span class="hidden sm:inline">Input Only</span>
      </button>

      <button
        type="button"
        onclick={() => {
          customHeight = null;
          chatStore.setDrawerMode('split');
        }}
        class="inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium transition cursor-pointer {chatStore.drawerMode ===
        'split'
          ? 'bg-zinc-800 text-zinc-100'
          : 'text-zinc-300 hover:bg-zinc-800/60 hover:text-zinc-100'}"
        title="Balanced split between DAG canvas and chat transcript"
      >
        <Columns2 class="h-3 w-3" />
        <span class="hidden sm:inline">Split</span>
      </button>

      <button
        type="button"
        onclick={() => {
          customHeight = null;
          chatStore.setDrawerMode('expanded');
        }}
        class="inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium transition cursor-pointer {chatStore.drawerMode ===
        'expanded'
          ? 'bg-zinc-800 text-zinc-100'
          : 'text-zinc-300 hover:bg-zinc-800/60 hover:text-zinc-100'}"
        title="Expand chat transcript"
      >
        <Maximize2 class="h-3 w-3" />
        <span class="hidden sm:inline">Expand</span>
      </button>
    </div>
  </div>

  <!-- Collapsible Stderr Diagnostic Panel -->
  {#if showStderrDrawer && chatStore.stderrLines.length > 0}
    <div class="border-b border-zinc-800 bg-zinc-950 px-4 py-2 text-xs">
      <div class="mb-1.5 flex items-center justify-between text-[11px] text-zinc-300">
        <span class="font-medium">Pi Sidecar Stderr Stream</span>
        <button
          type="button"
          onclick={() => chatStore.clearStderr()}
          class="text-zinc-300 underline underline-offset-2 hover:text-zinc-100 cursor-pointer"
        >
          Clear logs
        </button>
      </div>
      <div class="max-h-28 overflow-y-auto font-mono text-[11px] text-zinc-300 space-y-1">
        {#each chatStore.stderrLines as item, idx (idx)}
          <div class="whitespace-pre-wrap break-words">{item.line}</div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Transcript Viewport (hidden when drawer is collapsed to input-only) -->
  {#if chatStore.drawerMode !== 'collapsed'}
    <MessageList />
  {/if}

  <!-- Always-accessible Prompt & Mid-flight Steering Input -->
  <ChatInput />
</section>
