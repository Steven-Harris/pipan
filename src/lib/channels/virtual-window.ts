/** Geometry for a virtual list whose rows have different heights. */

/** Offsets[i] is the top of row i. The array has count + 1 entries; the last is the total height. */
export function computeOffsets(count: number, heightOf: (index: number) => number): Float64Array {
  const offsets = new Float64Array(count + 1);
  let y = 0;
  for (let i = 0; i < count; i++) {
    offsets[i] = y;
    y += heightOf(i);
  }
  offsets[count] = y;
  return offsets;
}

/** Index of the row that contains y. Clamped to the valid rows. */
export function indexAt(offsets: Float64Array, y: number): number {
  const count = offsets.length - 1;
  if (count <= 0) return 0;
  let lo = 0;
  let hi = count - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (offsets[mid] <= y) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

export interface VisibleRange {
  start: number;
  /** Exclusive. */
  end: number;
}

/** Rows to render for a viewport, with `overscan` pixels of extra rows on each side. */
export function visibleRange(offsets: Float64Array, scrollTop: number, viewport: number, overscan: number): VisibleRange {
  const count = offsets.length - 1;
  if (count <= 0) return { start: 0, end: 0 };
  const start = indexAt(offsets, Math.max(0, scrollTop - overscan));
  const end = Math.min(count, indexAt(offsets, scrollTop + viewport + overscan) + 1);
  return { start, end };
}
