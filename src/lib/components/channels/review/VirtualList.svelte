<script lang="ts" generics="T">
  // Renders only the rows in view. Rows with a known height (`fixedHeight`) are laid out without
  // measuring; the rest are measured once rendered, with `estimate` standing in until then.
  import type { Snippet } from 'svelte';
  import { computeOffsets, visibleRange } from '#lib/channels/virtual-window.ts';

  let {
    items,
    itemKey,
    fixedHeight,
    estimate = 96,
    overscan = 400,
    row,
    onvisible,
    label
  }: {
    items: readonly T[];
    itemKey: (item: T) => string;
    /** Height in px for rows that never change height, or undefined for rows to measure. */
    fixedHeight: (item: T) => number | undefined;
    estimate?: number;
    overscan?: number;
    row: Snippet<[T, number]>;
    /** Called with the index of the first row in view. */
    onvisible?: (index: number) => void;
    label: string;
  } = $props();

  let viewport: HTMLDivElement;
  let scrollTop = $state(0);
  let viewHeight = $state(0);
  // Measured heights live outside reactive state. `version` tells the offsets to recompute.
  const measured = new Map<string, number>();
  let version = $state(0);
  let pending = false;

  function bump() {
    if (pending) return;
    pending = true;
    requestAnimationFrame(() => {
      pending = false;
      version++;
    });
  }

  const heightOf = (item: T) => fixedHeight(item) ?? measured.get(itemKey(item)) ?? estimate;

  const offsets = $derived.by(() => {
    void version;
    return computeOffsets(items.length, (i) => heightOf(items[i]));
  });
  const range = $derived(visibleRange(offsets, scrollTop, viewHeight, overscan));
  const total = $derived(offsets[items.length] ?? 0);
  const rendered = $derived(items.slice(range.start, range.end));

  $effect(() => {
    onvisible?.(range.start);
  });

  function measure(node: HTMLElement, key: string) {
    let current = key;
    const observer = new ResizeObserver(() => {
      const h = node.offsetHeight;
      if (h > 0 && measured.get(current) !== h) {
        measured.set(current, h);
        bump();
      }
    });
    observer.observe(node);
    return {
      update(next: string) {
        current = next;
      },
      destroy() {
        observer.disconnect();
      }
    };
  }

  /** Scrolls so the row is at the top of the view. */
  export function scrollToIndex(index: number) {
    const clamped = Math.max(0, Math.min(index, items.length - 1));
    viewport.scrollTop = offsets[clamped] ?? 0;
  }
</script>

<div
  bind:this={viewport}
  bind:clientHeight={viewHeight}
  onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
  class="min-h-0 flex-1 overflow-auto"
  role="region"
  aria-label={label}
  tabindex="-1"
>
  <div style="height: {total}px; position: relative; min-width: max-content;">
    <div style="position: absolute; top: 0; left: 0; right: 0; transform: translateY({offsets[range.start] ?? 0}px);">
      {#each rendered as item, i (itemKey(item))}
        {@const fixed = fixedHeight(item)}
        {#if fixed !== undefined}
          <div style="height: {fixed}px;">{@render row(item, range.start + i)}</div>
        {:else}
          <div use:measure={itemKey(item)}>{@render row(item, range.start + i)}</div>
        {/if}
      {/each}
    </div>
  </div>
</div>
