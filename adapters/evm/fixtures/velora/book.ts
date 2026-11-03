

import { BIDS_MAX, RANGES_MAX, emptyRange } from './math/v2/book';
import type { Book, Range } from './math/v2/book';
import { quoteBookBuy, quoteBookSell } from './math/v2/trade';
import type { AbiValue } from './evm/abi-coder';

export const TOKEN_SCALE = 1_000_000_000_000n;

const n = (v: AbiValue): number => Number(v as bigint);

export function bookFromView(t: AbiValue[], curve: AbiValue[]): Book {
  const feeStartPpm = (curve[0] as bigint[]).map(Number);
  if (feeStartPpm.length !== 5)
    throw new RangeError(`launchFeeCurve: ${feeStartPpm.length} legs`);
  const nRanges = n(t[4]);
  if (nRanges > RANGES_MAX)
    throw new RangeError(`launchBook: nRanges ${nRanges} > ${RANGES_MAX}`);
  const bandMoney = t[3] as bigint[];
  const bandTokens = t[8] as bigint[];
  const bandEdge = t[9] as bigint[];
  const nBands = n(t[10]);
  const nC = n(t[11]);
  const bandF = t[12] as bigint[];
  const bandFClaim = t[13] as bigint[];
  if (
    nBands > BIDS_MAX ||
    nBands > bandMoney.length ||
    nC > nBands ||
    bandTokens.length !== bandMoney.length ||
    bandEdge.length !== bandMoney.length + 1 ||
    bandF.length !== bandMoney.length ||
    bandFClaim.length !== bandMoney.length
  )
    throw new RangeError(
      `launchBook: bands ${nBands} (C ${nC}) of ${bandMoney.length} / ${bandTokens.length} / ${bandEdge.length} / ${bandF.length} / ${bandFClaim.length}`,
    );
  for (let i = BIDS_MAX; i < bandMoney.length; i++)
    if (
      bandMoney[i] !== 0n ||
      bandTokens[i] !== 0n ||
      bandF[i] !== 0n ||
      bandFClaim[i] !== 0n
    )
      throw new RangeError(`launchBook: band ${i} is set`);
  const pad = (xs: bigint[], len: number): bigint[] =>
    Array.from({ length: len }, (_, i) => xs[i] ?? 0n);
  const view = t[5] as AbiValue[][];
  const ranges: Range[] = Array.from({ length: RANGES_MAX }, (_, i) => {
    if (i >= nRanges) return emptyRange();
    const r = view[i];
    return {
      sqrtA: r[0] as bigint,
      sqrtB: r[1] as bigint,
      tokens: r[2] as bigint,
      askLeft: r[3] as bigint,
      back: r[4] as bigint,
      place: r[5] as bigint,
      thetaBps: n(r[6]),
      psiBps: n(r[7]),
      kind: n(r[8]),
      class: n(r[9]),
      msCapPpm: 0,
    };
  });
  return {
    sqrtP: t[0] as bigint,
    sqrtF: t[1] as bigint,
    sqrtLMax: t[6] as bigint,
    floor: t[2] as bigint,
    floorTokens: t[7] as bigint,
    nBands,
    nC,
    bandEdge: pad(bandEdge, BIDS_MAX + 1),
    bandMoney: pad(bandMoney, BIDS_MAX),
    bandTokens: pad(bandTokens, BIDS_MAX),
    bandF: pad(bandF, BIDS_MAX),
    bandFClaim: pad(bandFClaim, BIDS_MAX),
    ckAtF: t[14] as bigint,
    ckFloorTokens: t[15] as bigint,
    feePpm: (t[17] as bigint[]).map(Number),
    feeStartPpm,
    sqrtFeeA: curve[1] as bigint,
    sqrtFeeB: curve[2] as bigint,
    graduated: t[16] as boolean,
    nRanges,
    ranges,
  };
}

export function isDustRefund(
  gross: bigint,
  refund: bigint,
  budgetHit: boolean,
): boolean {
  return refund > 0n && !budgetHit && refund * 1_000_000n <= gross;
}

export function amountOut(book: Book, isBuy: boolean, amountIn: bigint): bigint {
  return walk(book, isBuy, amountIn).amountOut;
}

export function walk(
  book: Book,
  isBuy: boolean,
  amountIn: bigint,
): { amountOut: bigint; steps: number } {
  try {
    if (isBuy) {
      const r = quoteBookBuy(book, amountIn);
      const whole =
        r.refund === 0n || isDustRefund(amountIn, r.refund, r.budgetHit);
      return { amountOut: whole ? r.tokens * TOKEN_SCALE : 0n, steps: r.steps };
    }

    const units = amountIn / TOKEN_SCALE;
    if (units === 0n) return { amountOut: 0n, steps: 0 };
    const r = quoteBookSell(book, units);
    return { amountOut: r.unfilled > 0n ? 0n : r.out, steps: r.steps };
  } catch {
    return { amountOut: 0n, steps: 0 };
  }
}

export const GAS_FLAT = { buy: 700_000, sell: 750_000 } as const;

export const GAS_PER_STEP = { buy: 40_000, sell: 40_000 } as const;
export const GAS_FLAT_STEPS = 4;
export const GAS_MAX = 1_500_000;

export function gasFor(side: 'buy' | 'sell', steps: number): number {
  const extra = Math.max(0, Math.trunc(steps) - GAS_FLAT_STEPS);
  return Math.min(GAS_MAX, GAS_FLAT[side] + extra * GAS_PER_STEP[side]);
}

export function dexLiquidity(b: Book): bigint {
  let s = b.floor;
  for (let i = 0; i < b.nBands; i++) s += b.bandMoney[i];
  for (let i = 0; i < b.nRanges; i++) s += b.ranges[i].place;
  return s;
}
