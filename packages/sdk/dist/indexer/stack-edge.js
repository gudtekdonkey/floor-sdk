

function startBar(bars                         , graduatedAt        )         {
  let start = 0;
  for (let j = 0; j < bars.length; j++) {
    if (bars[j] .time <= graduatedAt) start = j;
    else break;
  }
  return start;
}

export function stackEdgeWalk(bars                         , graduatedAt        , endPrice               )                   {
  const out                   = [];
  let m = 0;
  for (let j = startBar(bars, graduatedAt); j < bars.length; j++) {
    const b = bars[j] ;
    if (!(b.close > 0) || !(b.high > 0)) continue;
    m = Math.max(m, b.high);
    if (endPrice !== null && endPrice > 0 && m >= endPrice) break;
    out.push({ i: j, edge: m });
  }
  return out;
}

export function stackEdgeEnd(bars                         , graduatedAt        , endPrice               )                {
  if (endPrice === null || !(endPrice > 0)) return null;
  let m = 0;
  for (let j = startBar(bars, graduatedAt); j < bars.length; j++) {
    const b = bars[j] ;
    if (!(b.close > 0) || !(b.high > 0)) continue;
    m = Math.max(m, b.high);
    if (m >= endPrice) return j;
  }
  return null;
}
