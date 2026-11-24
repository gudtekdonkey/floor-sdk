

import { log2X64 } from "../fixed.ts";
import { DOWN, U128_MAX, UP, mulDiv } from "./wide.ts";

export const FEE_LEGS = 5;
export const FEE_DEFAULT_PPM = 100;
export const DEFAULT_FEE_PPM: readonly number[] = [100, 100, 100, 100, 100];
export const LEG_BACKING = 0;
export const LEG_TEAM = 1;
export const LEG_PLATFORM = 2;
export const LEG_FLOOR = 3;
export const LEG_HOLDER = 4;
const PPM = 1_000_000n;

export const feeLeg = (amount: bigint, ppm: number): bigint => mulDiv(amount, BigInt(ppm), PPM, UP) ?? amount;
export const feeLegs = (amount: bigint, ppm: readonly number[]): bigint[] => ppm.map((p) => feeLeg(amount, p));

export const feeTotal = (f: readonly bigint[]): bigint => f.reduce((a, b) => (a + b > U128_MAX ? U128_MAX : a + b), 0n);

export function ppmAt(end: readonly number[], start: readonly number[], sqrtA: bigint, sqrtB: bigint, sqrtP: bigint): number[] {
  if (sqrtB === 0n || sqrtP >= sqrtB) return [...end];
  if (sqrtP <= sqrtA) return [...start];
  const lg = (x: bigint): bigint => log2X64(x) ?? 0n;
  const [la, lb, lp] = [lg(sqrtA), lg(sqrtB), lg(sqrtP)];
  const one = 1n << 64n;
  let w = 0n;
  if (lb > la) {
    const q = mulDiv(lb > lp ? lb - lp : 0n, one, lb - la, DOWN) ?? 0n;
    w = q > one ? one : q;
  }
  return end.map((e, i) => {
    const x = BigInt(start[i]) * w + BigInt(e) * (one - w);
    return Number((x + one - 1n) / one);
  });
}

export const PREGRAD_PPM: readonly number[] = [0, 0, 4_000, 1_000, 0];

export function ratesAt(graduated: boolean, own: boolean, end: readonly number[], start: readonly number[], sqrtA: bigint, sqrtB: bigint, sqrtP: bigint): number[] {
  if (!graduated && !own) return [...PREGRAD_PPM];
  return ppmAt(end, start, sqrtA, sqrtB, sqrtP);
}

export const cheaper = (a: readonly number[], b: readonly number[]): number[] => a.map((x, i) => Math.min(x, b[i]));

export class FeeAcc {
  readonly pre: number[];
  whole: bigint[] = [0n, 0n, 0n, 0n, 0n];
  rem: bigint[] = [0n, 0n, 0n, 0n, 0n];
  constructor(pre: readonly number[]) {
    this.pre = [...pre];
  }

  add(q: bigint, at: readonly number[]): boolean {
    const [qh, ql] = [q / PPM, q % PPM];
    const w = [...this.whole];
    const r2 = [...this.rem];
    for (let i = 0; i < FEE_LEGS; i++) {
      const r = BigInt(Math.min(this.pre[i], at[i]));
      const lo = ql * r;
      const rem = r2[i] + (lo % PPM);
      const x = w[i] + qh * r + lo / PPM + rem / PPM;
      if (x > U128_MAX) return false;
      w[i] = x;
      r2[i] = rem % PPM;
    }
    this.whole = w;
    this.rem = r2;
    return true;
  }

  legs(): bigint[] {
    return this.whole.map((x, i) => {
      const v = x + (this.rem[i] > 0n ? 1n : 0n);
      return v > U128_MAX ? U128_MAX : v;
    });
  }
}

export function grossForNet(net: bigint, ppm: readonly number[]): bigint | null {
  const g = net + feeTotal(feeLegs(net, ppm));
  return g > U128_MAX ? null : g;
}

export function netFromGross(gross: bigint, ppm: readonly number[]): bigint {
  const sum = ppm.reduce((a, p) => a + BigInt(p), 0n);
  let net = mulDiv(gross, PPM, PPM + sum, DOWN) ?? 0n;
  for (;;) {
    if (net >= gross) break;
    const g = grossForNet(net + 1n, ppm);
    if (g === null || g > gross) break;
    net++;
  }
  for (;;) {
    if (net <= 0n) break;
    const g = grossForNet(net, ppm);
    if (g !== null && g <= gross) break;
    net--;
  }
  return net;
}

export const TRADER_REF_BPS: readonly [number, number, number] = [2_400, 420, 180];

export const CREATOR_REF_BPS: readonly [number, number, number] = [800, 140, 60];

export const TRADER_REF_OG_BPS: readonly [number, number, number] = [4_000, 700, 300];

export const CREATOR_REF_OG_BPS: readonly [number, number, number] = [2_400, 420, 180];

export const OG_WINDOW_SECS = 15_811_200n;

export const OG_MAX = 100;
const OG_U64_MAX = (1n << 64n) - 1n;

export const ogUntil = (from: bigint): bigint => (from === 0n ? 0n : from + OG_WINDOW_SECS > OG_U64_MAX ? OG_U64_MAX : from + OG_WINDOW_SECS);

export const ogLive = (until: bigint, now: bigint): boolean => now < until;

export const ogLaunch = (from: bigint, until: bigint, createdAt: bigint, now: bigint): boolean =>
  from !== 0n && from <= createdAt && createdAt < until && now < until;

export const tierBps = (bps: readonly number[], ogBps: readonly number[], og: readonly boolean[]): number[] =>
  [0, 1, 2].map((i) => (og[i] ? ogBps[i]! : bps[i]!));

export function slices(fee: bigint, present: readonly boolean[], bps: readonly number[]): [bigint[], bigint] {
  const out = [0n, 0n, 0n];
  let left = fee;
  for (let i = 0; i < 3; i++) {
    if (!present[i] || bps[i] === 0) continue;
    let s = (fee * BigInt(bps[i])) / 10_000n;
    if (s > left) s = left;
    out[i] = s;
    left -= s;
  }
  return [out, left];
}
const NO_OG: readonly boolean[] = [false, false, false];

export const platformLegsOg = (platform: bigint, traderPresent: readonly boolean[], og: readonly boolean[]): [bigint[], bigint] =>
  slices(platform, traderPresent, tierBps(TRADER_REF_BPS, TRADER_REF_OG_BPS, og));

export const creatorLegsOg = (creator: bigint, creatorPresent: readonly boolean[], og: readonly boolean[]): [bigint[], bigint] =>
  slices(creator, creatorPresent, tierBps(CREATOR_REF_BPS, CREATOR_REF_OG_BPS, og));
export const platformLegs = (platform: bigint, traderPresent: readonly boolean[]): [bigint[], bigint] => platformLegsOg(platform, traderPresent, NO_OG);
export const creatorLegs = (creator: bigint, creatorPresent: readonly boolean[]): [bigint[], bigint] => creatorLegsOg(creator, creatorPresent, NO_OG);
