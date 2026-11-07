<script lang="ts">
  import { workspaceStore, MIN_SPLIT_RATIO, MAX_SPLIT_RATIO } from '#lib/stores/workspace.svelte.ts';
  import ChatPane from './ChatPane.svelte';
  import PanelHost from './PanelHost.svelte';

  const KEY_STEP = 0.02;

  let container = $state<HTMLDivElement | null>(null);
  let dragging = $state(false);

  const vertical = $derived(workspaceStore.orientation === 'vertical');
  const panelBasis = $derived(`${(workspaceStore.splitRatio * 100).toFixed(2)}%`);

  function ratioFromPointer(e: PointerEvent): number | null {
    if (!container) return null;
    const rect = container.getBoundingClientRect();
    if (vertical) {
      // Panels on the right: ratio is the share measured from the right edge.
      return rect.width > 0 ? (rect.right - e.clientX) / rect.width : null;
    }
    // Panels on top: ratio is the share measured from the top edge.
    return rect.height > 0 ? (e.clientY - rect.top) / rect.height : null;
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging = true;
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    const ratio = ratioFromPointer(e);
    if (ratio !== null) workspaceStore.setSplitRatio(ratio);
  }

  function endDrag(e: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    const el = e.currentTarget as HTMLElement;
    if (el.hasPointerCapture(e.pointerId)) el.releasePointerCapture(e.pointerId);
    workspaceStore.commitSplitRatio();
  }

  function onKeydown(e: KeyboardEvent) {
    // "Grow the panel area" is left in a vertical split (panels on the right)
    // and down in a horizontal split (panels on top).
    const grow = vertical ? 'ArrowLeft' : 'ArrowDown';
    const shrink = vertical ? 'ArrowRight' : 'ArrowUp';
    let next: number | null = null;
    if (e.key === grow) next = workspaceStore.splitRatio + KEY_STEP;
    else if (e.key === shrink) next = workspaceStore.splitRatio - KEY_STEP;
    else if (e.key === 'Home') next = MIN_SPLIT_RATIO;
    else if (e.key === 'End') next = MAX_SPLIT_RATIO;
    if (next === null) return;
    e.preventDefault();
    workspaceStore.setSplitRatio(next);
    workspaceStore.commitSplitRatio();
  }
</script>

{#snippet splitter()}
  <!-- A focusable separator is an ARIA window-splitter widget, so these a11y warnings are false positives. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    role="separator"
    aria-orientation={vertical ? 'vertical' : 'horizontal'}
    aria-label="Resize chat and panels"
    aria-valuemin={Math.round(MIN_SPLIT_RATIO * 100)}
    aria-valuemax={Math.round(MAX_SPLIT_RATIO * 100)}
    aria-valuenow={Math.round(workspaceStore.splitRatio * 100)}
    tabindex="0"
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={endDrag}
    onpointercancel={endDrag}
    onkeydown={onKeydown}
    class="group relative shrink-0 touch-none outline-none {vertical
      ? 'w-1.5 cursor-col-resize'
      : 'h-1.5 cursor-row-resize'} {dragging
      ? 'bg-indigo-500/40'
      : 'bg-zinc-800/80 hover:bg-indigo-500/30 focus-visible:bg-indigo-500/40'}"
  >
    <span
      class="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 rounded-full bg-zinc-600 group-hover:bg-indigo-300 {vertical
        ? 'h-8 w-0.5'
        : 'h-0.5 w-8'}"
    ></span>
  </div>
{/snippet}

<!--
  DOM order is always chat, splitter, panels so ChatPane (and its draft input)
  stays mounted when panels open/close or the orientation flips. The horizontal
  split reverses the flex direction to put panels on top.
-->
<div
  bind:this={container}
  class="flex min-h-0 min-w-0 flex-1 overflow-hidden {vertical
    ? 'flex-row'
    : 'flex-col-reverse'} {dragging ? 'select-none' : ''}"
>
  <ChatPane />
  {#if workspaceStore.hasPanels}
    {@render splitter()}
    <div class="min-h-0 min-w-0 shrink-0" style:flex-basis={panelBasis}>
      <PanelHost />
    </div>
  {/if}
</div>
