

import { fail } from "../index.js";
import { KIND_BONDING, KIND_TAIL, PIECES_MAX } from "./book.js";

import { flatMoneyForTokens, sqrtOfRatio, sqrtScaleDownBps } from "./sqrt.js";

import { U128_MAX, UP } from "./wide.js";

export const MS_TEAM = 1;
export const MS_HOLDER = 2;

export { PIECES_MAX };

const ov =    (x          )    => (x === null ? fail("Overflow") : x);
const u128 = (x        )         => (x > U128_MAX ? fail("Overflow") : x);
const min = (a        , b        )         => (a < b ? a : b);
const max = (a        , b        )         => (a > b ? a : b);
const sat = (a        , b        )         => (a > b ? a - b : 0n);
const cdiv = (a        , d        )         => (a + d - 1n) / d;

export const shareSplit = (x        , cShareBps        )                   => {
  const c = cdiv(x * BigInt(cShareBps), 10_000n);
  return [c, x - c];
};

export const potsSplit = (x        , cShareBps        , gapC        , gapK        )                   => {
  let [c, k] = shareSplit(x, cShareBps);
  if (c > gapC) {
    k += c - gapC;
    c = gapC;
  } else if (k > gapK) {
    c += k - gapK;
    k = gapK;
  }
  return [c, k];
};

const flatUp = (s      , n        )         => ov(flatMoneyForTokens(s, n, UP));

                              

export function splitRowOfRowV2(r                                                                                                                                                     )           {
  const ck = r.ckTargetBps ?? r.ckBps;
  if (!Number.isInteger(ck)) throw new RangeError("splitRowOfRowV2: the row has no C + K share (`ckTargetBps`)");
  if (r.ckTargetBps !== undefined && r.ckBps !== undefined && r.ckTargetBps !== r.ckBps) {
    throw new RangeError(`splitRowOfRowV2: ckTargetBps ${r.ckTargetBps} and ckBps ${r.ckBps} disagree`);
  }
  return {
    phiBps: r.phiBps,
    ckBps: ck          ,

    wcBps: r.wcBps ?? 300,
    cShareBps: 5_000,
    msClass: r.msClass,
    platformMsBps: (r.flags & 1) !== 0 ? r.platformMsBps : 0,
    floorMsBps: (r.flags & 2) !== 0 ? r.floorMsBps : 0,
  };
}

export function surplus(sqrtF      , back        , n        )         {
  const base = flatMoneyForTokens(sqrtF, n, UP);
  return base === null || base === undefined ? 0n : sat(back, base);
}

export function split(row          , ranges                  , inp         , pieces                  )        {
  let tokensSum = 0n;
  for (const p of pieces) tokensSum += p.tokens;
  let sr = inp.sNew - tokensSum;
  if (sr < 0n) fail("Overflow");
  let fr = inp.fTotal;

  const s0 = surplus(inp.sqrtF, fr + inp.fClaims, sr + inp.held);
  const phi = BigInt(row.phiBps);
  let cAdd = 0n;
  let kAdd = 0n;
  let ms = 0n;
  const cs = BigInt(row.cShareBps);

  const force = (x        )       => {
    const xc = cdiv(x * cs, 10_000n);
    cAdd += xc;
    kAdd += x - xc;
  };
  let topNeed = 0n;
  const booked                                   = [0n, 0n, 0n, 0n];

  for (const pc of pieces) {
    const r = ranges[pc.range];
    sr += pc.tokens;

    const lo = pc.sqrtLo;
    const capA = cdiv(pc.money * BigInt(r.psiBps), 10_000n);
    const capB = cdiv(cdiv(pc.money * inp.sqrtF, lo) * inp.sqrtF, lo);
    const cap = min(max(capA, capB), pc.money);
    const n = sr + inp.held;
    const needAll = max(flatUp(inp.sqrtF, n) + s0, cdiv(flatUp(pc.sqrtHi, n) * phi, 10_000n));
    if (r.kind !== KIND_TAIL) topNeed = needAll;
    const need = sat(needAll, fr + inp.fClaims);
    const dF = min(cap, need);
    fr += dF;
    const rem = pc.money - dF;
    if (r.thetaBps >= 10_000) {

      force(rem);
      continue;
    }

    const t = cdiv(flatUp(pc.sqrtHi, sr) * BigInt(row.ckBps), 10_000n);
    const tc = cdiv(t * cs, 10_000n);
    const gapC = sat(tc, inp.c0 + cAdd);
    const gapK = sat(t - tc, inp.k0 + kAdd);
    const m = min(rem, gapC + gapK);
    let mc = cdiv(m * cs, 10_000n);
    let mk = m - mc;

    if (mc > gapC) {
      mk += mc - gapC;
      mc = gapC;
    } else if (mk > gapK) {
      mc += mk - gapK;
      mk = gapK;
    }
    cAdd += mc;
    kAdd += mk;
    let mm = rem - m;
    if (r.msCapPpm > 0) {
      const capm = (pc.money * BigInt(r.msCapPpm)) / 1_000_000n;
      if (mm > capm) {
        force(mm - capm);
        mm = capm;
      }
    }
    ms += mm;
  }

  let cTake = 0n;
  let kTake = 0n;
  const short = sat(topNeed, fr + inp.fClaims);
  if (short > 0n && inp.c0 + inp.k0 + cAdd + kAdd > 0n) {
    const x = min(short, cAdd + kAdd);
    const [ac, ak] = potsSplit(x, 5_000, cAdd, kAdd);
    let left = short - x;
    const m2 = min(left, ms);
    left -= m2;
    const y = min(left, inp.c0 + inp.k0);
    [cTake, kTake] = potsSplit(y, 5_000, inp.c0, inp.k0);
    cAdd -= ac;
    kAdd -= ak;
    ms -= m2;
    fr += ac + ak + m2 + cTake + kTake;
  }

  booked[2] = (ms * BigInt(row.platformMsBps)) / 10_000n;
  booked[3] = (ms * BigInt(row.floorMsBps)) / 10_000n;
  booked[row.msClass === MS_HOLDER ? 1 : 0] += ms - booked[2] - booked[3];

  const ratio = sqrtOfRatio(fr + inp.fClaims, inp.sNew + inp.held);
  const capSqrt = ov(sqrtScaleDownBps(inp.sqrtP1, row.wcBps));
  const sqrtFNew = max(inp.sqrtF, ratio === null ? inp.sqrtF : min(ratio, capSqrt));
  return { dF: fr - inp.fTotal, ckAdd: u128(cAdd + kAdd), ckTake: cTake + kTake, cAdd: u128(cAdd), kAdd: u128(kAdd), cTake, kTake, ms: u128(ms), booked, sqrtFNew };
}

export const pieceEntersSplit = (r       )          => r.kind !== KIND_BONDING;
