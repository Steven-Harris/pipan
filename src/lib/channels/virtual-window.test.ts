import { describe, expect, it } from 'vitest';
import { computeOffsets, indexAt, visibleRange } from './virtual-window';

describe('virtual window', () => {
  const offsets = computeOffsets(5, (i) => (i === 2 ? 100 : 20)); // 0,20,40,140,160,180
  it('computes cumulative offsets and the total', () => {
    expect(Array.from(offsets)).toEqual([0, 20, 40, 140, 160, 180]);
  });
  it('finds the row containing y', () => {
    expect(indexAt(offsets, 0)).toBe(0);
    expect(indexAt(offsets, 19)).toBe(0);
    expect(indexAt(offsets, 20)).toBe(1);
    expect(indexAt(offsets, 100)).toBe(2);
    expect(indexAt(offsets, 9999)).toBe(4);
    expect(indexAt(computeOffsets(0, () => 1), 5)).toBe(0);
  });
  it('returns the rows in view plus overscan', () => {
    expect(visibleRange(offsets, 0, 30, 0)).toEqual({ start: 0, end: 2 });
    expect(visibleRange(offsets, 150, 10, 0)).toEqual({ start: 3, end: 5 });
    expect(visibleRange(offsets, 150, 10, 50)).toEqual({ start: 2, end: 5 });
    expect(visibleRange(computeOffsets(0, () => 1), 0, 100, 10)).toEqual({ start: 0, end: 0 });
  });
  it('handles a 50,000-row list quickly', () => {
    const big = computeOffsets(50_000, () => 20);
    const t = performance.now();
    for (let i = 0; i < 1000; i++) visibleRange(big, i * 900, 800, 400);
    expect(performance.now() - t).toBeLessThan(200);
    expect(big[50_000]).toBe(1_000_000);
  });
});
