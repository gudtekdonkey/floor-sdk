

export interface StackEdgeBar {
  time: number;
  high: number;
  close: number;
}

export interface StackEdgePoint {
  i: number;
  edge: number;
}

function startBar(bars: readonly StackEdgeBar[], graduatedAt: number): number {
  let start = 0;
  for (let j = 0; j < bars.length; j++) {
    if (bars[j]!.time <= graduatedAt) start = j;
    else break;
  }
  return start;
}

export function stackEdgeWalk(bars: readonly StackEdgeBar[], graduatedAt: number, endPrice: number | null): StackEdgePoint[] {
  const out: StackEdgePoint[] = [];
  let m = 0;
  for (let j = startBar(bars, graduatedAt); j < bars.length; j++) {
    const b = bars[j]!;
    if (!(b.close > 0) || !(b.high > 0)) continue;
    m = Math.max(m, b.high);
    if (endPrice !== null && endPrice > 0 && m >= endPrice) break;
    out.push({ i: j, edge: m });
  }
  return out;
}

export function stackEdgeEnd(bars: readonly StackEdgeBar[], graduatedAt: number, endPrice: number | null): number | null {
  if (endPrice === null || !(endPrice > 0)) return null;
  let m = 0;
  for (let j = startBar(bars, graduatedAt); j < bars.length; j++) {
    const b = bars[j]!;
    if (!(b.close > 0) || !(b.high > 0)) continue;
    m = Math.max(m, b.high);
    if (m >= endPrice) return j;
  }
  return null;
}
