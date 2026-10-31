

import { build, milestonesTotal } from "./ask.js";

import { cloneRow, f64ToU128, rowWcBps } from "./rows.js";

import { V2MathError } from "./error.js";

                 

                                                               

                                                                                                               

                                                                                       

export function r1Cover(phi        , sA        , x        , a        , b        )         {
  const u = Math.sqrt(b / a);
  if (phi * u >= 1.0) return 0.0;
  const need = (phi * sA * (u * u - 1.0)) / (u * (1.0 - phi * u));
  return need <= 0.0 ? Infinity : x / need;
}

function askRules(row     , a     , v             )       {
  const phi = row.phiBps / 1e4;
  const clamp = (1.0 - row.wBps / 1e4) * (1.0 - rowWcBps(row) / 1e4);
  let s = 0.0;
  let bigF = 0.0;
  let f = 0.0;
  for (let i = 0; i < a.segs.length; i++) {
    const g = a.segs[i] ;
    const ranged = (g.kind === "Climb" || g.kind === "Sale") && g.b > g.a * (1.0 + 1e-12);
    if (ranged && g.a < a.tailStart * (1.0 - 1e-12)) {
      const c = r1Cover(phi, s, g.tokens, g.a, g.b);
      if (c < 1.0 - 1e-9) v.push({ kind: "RangeCover", segment: i, cover: c });
    }
    if (g.kind === "Tail") break;
    s += g.tokens;
    bigF += g.toFloor;
    const fNew = s > 0.0 ? fmax(f, bigF / s) : f;
    if (g.kind !== "Bonding" && fNew > f * (1.0 + 1e-12) && fNew > clamp * g.b * (1.0 + 1e-9)) {
      v.push({ kind: "OrderClamp", segment: i });
    }
    f = fNew;
  }
}

const fmax = (a        , b        )         => (Number.isNaN(a) ? b : Number.isNaN(b) ? a : a >= b ? a : b);

                                                                                      

function tryBuild(row     )                    {
  try {
    return build(row);
  } catch (e) {
    if (e instanceof V2MathError) return e;
    throw e;
  }
}

function violationsOf(row     )                            {
  const a = tryBuild(row);
  if (a instanceof V2MathError) {
    if (a.code === "TargetUnreachable") return [[{ kind: "TargetUnreachable", stack: a.stackIndex  }], null];
    return [[{ kind: "Structure" }], null];
  }
  const v              = [];
  if (a.supplyExcess > 1e-9) v.push({ kind: "SupplyShort", excess: a.supplyExcess });
  if (a.floorBreak !== null) v.push({ kind: "FloorBelowRatio", segment: a.floorBreak });
  if (!a.contiguous) v.push({ kind: "EmptyStretch" });
  a.stacks.forEach((s, i) => {
    if (!s.skipped && s.short > 1e-6 * Math.max(s.milestone + s.short, 1.0)) v.push({ kind: "TargetShort", stack: i, short: s.short });
  });
  askRules(row, a, v);
  return [v, a];
}

export const passes = (row     )          => violationsOf(row)[0].length === 0;

export function withTopLevel(row     , level        )      {
  const r = cloneRow(row);
  const t = r.stacks[r.stacks.length - 1];
  if (t !== undefined) t.level = level;
  return r;
}

export function scaled(row     , k        )      {
  const r = cloneRow(row);
  for (const s of r.stacks) s.milestone = f64ToU128(Math.floor(Number(s.milestone) * k));
  return r;
}

function room(row     )                {
  const last = row.stacks[row.stacks.length - 1];
  if (last === undefined) return null;
  const lvl = Number(last.level);
  let lo = lvl;
  let hi = lvl * 1e6;
  if (passes(withTopLevel(row, f64ToU128(hi)))) return f64ToU128(hi);
  for (let i = 0; i < 80; i++) {
    const mid = Math.sqrt(lo * hi);
    if (passes(withTopLevel(row, f64ToU128(mid)))) lo = mid;
    else hi = mid;
  }
  return f64ToU128(lo);
}

export function check(row     )         {
  const [violations, ask] = violationsOf(row);
  const allowed = 1.0 - row.tailBps / 1e4;
  const sold = ask === null ? 0.0 : ask.supplyAfterStacks;
  const ok = violations.length === 0;
  const rep         = { ok, violations, sold, allowed, roomTopLevel: null, fix: null };
  if (ok) {
    rep.roomTopLevel = room(row);
    return rep;
  }
  if (rep.violations.some((v) => v.kind === "SupplyShort")) {

    let best                       = null;
    const offer = (rel        , f     )       => {
      if (Number.isFinite(rel) && (best === null || rel < best[0])) best = [rel, f];
    };
    const tailNew = Math.max(10_000.0 - Math.ceil(sold * 1e4), 0.0);
    if (tailNew >= 1.0) {
      const t = cloneRow(row);
      t.tailBps = Math.min(Math.trunc(tailNew), 65_535);
      if (passes(t)) offer(1.0 - tailNew / Math.max(row.tailBps, 1), { kind: "TailBps", bps: t.tailBps });
    }
    let lo = 0.0;
    let hi = 1.0;
    if (passes(scaled(row, 0.0))) {
      for (let i = 0; i < 60; i++) {
        const mid = (lo + hi) / 2.0;
        if (passes(scaled(row, mid))) lo = mid;
        else hi = mid;
      }
      offer(1.0 - lo, { kind: "ScaleMilestones", factor: lo });
    }
    if (row.stacks.length > 1) {
      const prev = Number(row.stacks[row.stacks.length - 2] .level);
      const top = Number(row.stacks[row.stacks.length - 1] .level);
      let lo = prev * 1.0001;
      let hi = top;
      if (passes(withTopLevel(row, f64ToU128(lo)))) {
        for (let i = 0; i < 80; i++) {
          const mid = Math.sqrt(lo * hi);
          if (passes(withTopLevel(row, f64ToU128(mid)))) lo = mid;
          else hi = mid;
        }
        offer(1.0 - lo / top, { kind: "TopLevel", level: f64ToU128(lo) });
      }
    }
    rep.fix = best === null ? null : (best                 )[1];
  }
  return rep;
}

export function fitStacks(row     )      {
  let best                       = null;
  for (let n = row.stacks.length; n >= 1; n--) {
    const r = cloneRow(row);
    r.stacks.length = n;
    const a = passes(r) ? tryBuild(r) : null;
    if (a !== null && !(a instanceof V2MathError)) {
      const t = milestonesTotal(a);
      if (best === null || t > best[0]) best = [t, r];
    }
  }
  if (best !== null) return (best                 )[1];
  const r = cloneRow(row);
  r.stacks.length = Math.min(r.stacks.length, 1);
  return r;
}
