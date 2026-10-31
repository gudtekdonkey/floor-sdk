

import { log2X64 } from "../fixed.js";
import { DOWN, U128_MAX, UP, mulDiv } from "./wide.js";

export const FEE_LEGS = 5;
export const FEE_DEFAULT_PPM = 100;
export const DEFAULT_FEE_PPM                    = [100, 100, 100, 100, 100];
export const LEG_BACKING = 0;
export const LEG_TEAM = 1;
export const LEG_PLATFORM = 2;
export const LEG_FLOOR = 3;
export const LEG_HOLDER = 4;
const PPM = 1_000_000n;

export const feeLeg = (amount        , ppm        )         => mulDiv(amount, BigInt(ppm), PPM, UP) ?? amount;
export const feeLegs = (amount        , ppm                   )           => ppm.map((p) => feeLeg(amount, p));

export const feeTotal = (f                   )         => f.reduce((a, b) => (a + b > U128_MAX ? U128_MAX : a + b), 0n);

export function ppmAt(end                   , start                   , sqrtA        , sqrtB        , sqrtP        )           {
  if (sqrtB === 0n || sqrtP >= sqrtB) return [...end];
  if (sqrtP <= sqrtA) return [...start];
  const lg = (x        )         => log2X64(x) ?? 0n;
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

export const PREGRAD_PPM                    = [0, 0, 4_000, 1_000, 0];

export function ratesAt(graduated         , own         , end                   , start                   , sqrtA        , sqrtB        , sqrtP        )           {
  if (!graduated && !own) return [...PREGRAD_PPM];
  return ppmAt(end, start, sqrtA, sqrtB, sqrtP);
}

export const cheaper = (a                   , b                   )           => a.map((x, i) => Math.min(x, b[i]));

export class FeeAcc {
           pre          ;
  whole           = [0n, 0n, 0n, 0n, 0n];
  rem           = [0n, 0n, 0n, 0n, 0n];
  constructor(pre                   ) {
    this.pre = [...pre];
  }

  add(q        , at                   )          {
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

  legs()           {
    return this.whole.map((x, i) => {
      const v = x + (this.rem[i] > 0n ? 1n : 0n);
      return v > U128_MAX ? U128_MAX : v;
    });
  }
}

export function grossForNet(net        , ppm                   )                {
  const g = net + feeTotal(feeLegs(net, ppm));
  return g > U128_MAX ? null : g;
}

export function netFromGross(gross        , ppm                   )         {
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

export const TRADER_REF_BPS                                    = [3_000, 2_000, 1_000];

export const CREATOR_REF_BPS                                    = [900, 90, 10];

export function slices(fee        , present                    , bps                   )                     {
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
export const platformLegs = (platform        , traderPresent                    )                     => slices(platform, traderPresent, TRADER_REF_BPS);
export const creatorLegs = (creator        , creatorPresent                    )                     => slices(creator, creatorPresent, CREATOR_REF_BPS);
