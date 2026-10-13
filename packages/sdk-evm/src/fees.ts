/**
 * What a FLOOR trade costs, in integers — the contracts' own arithmetic, never a float.
 *
 * Mirrors `FloorHookMath.skimBps` and the hook engine's `_takeFees`, and `FloorLaunch`'s fee rounding.
 *
 * The trader's take on a POOL swap is five parts, listed whole or not at all:
 *   floor skim + creator fee + holder tax + FLOOR Platform fee — four percentages of the SAME quote leg, each rounded down and
 *   summed (`_takeFees`) — and the 0.25% LP fee, which the pool charges on what the hook leaves.
 * A BONDING trade (before graduation) pays the SALE's FLOOR Platform fee and nothing else: there is no pool yet, so its skim,
 * creator fee, holder tax and LP fee are zero, and are still listed (as 0) so the take is whole.
 * ⛔ Every rate is READ per launch / per pool (each copied its rates at creation). Nothing here holds a rate.
 * ⛔ The 2.5% `$FLOOR` cut is NOT a take: it is a split of the skim the trader already paid. It is never in a quote.
 */
import type { FeePart } from '@floor/sdk';
import { RateAboveCap } from './errors.js';
import { LP_FEE_PIPS, SKIM_RAMP_TICKS } from './pool.js';

/** `FloorPolicy.MAX_SALE_TEAM_FEE_BPS` — the most a sale's FLOOR Platform fee can be. */
export const MAX_SALE_TEAM_FEE_BPS = 100;
/** `FloorPolicy.MAX_POOL_TEAM_FEE_BPS` — the most a pool's FLOOR Platform fee can be. */
export const MAX_POOL_TEAM_FEE_BPS = 50;

/** `value` as a rate in [0, cap], or a throw naming `what` (`RateAboveCap` past the cap). */
export function rateWithinCap(value: number, cap: number, what: string): number {
  if (!Number.isInteger(value) || value < 0) throw new RangeError(`${what}: ${String(value)} is not a rate`);
  if (value > cap) throw new RateAboveCap(what, value, cap);
  return value;
}

const rateOf = (bps: number): bigint => {
  if (!Number.isInteger(bps) || bps < 0 || bps >= 10_000) throw new RangeError(`the FLOOR Platform fee: ${bps} bps is not a rate in 0..9999`);
  return BigInt(bps);
};

/** `amount × bps / 10_000`, rounded DOWN — the one rounding the contracts use for the FLOOR Platform fee. */
export function teamFeeOf(amount: bigint, bps: number): bigint {
  const b = rateOf(bps);
  return amount <= 0n ? 0n : (amount * b) / 10_000n;
}

/** A sale buy's NET (what the walk runs on) out of the GROSS sent: `floor(gross × 10_000 / (10_000 + bps))`. */
export function saleNetOf(gross: bigint, bps: number): bigint {
  const b = rateOf(bps);
  return gross <= 0n ? 0n : (gross * 10_000n) / (10_000n + b);
}

/** The least gross whose net covers `spent`: `ceil(spent × (10_000 + bps) / 10_000)`. */
export function saleGrossFor(spent: bigint, bps: number): bigint {
  const n = spent * (10_000n + rateOf(bps));
  return n / 10_000n + (n % 10_000n === 0n ? 0n : 1n);
}

/** A sell-back's fee and pay off its `payout` (what `sellBackQuote` gives under the contract version's rule): paid `payout − fee`. */
export function sellBackPayOf(payout: bigint, bps: number): { fee: bigint; paid: bigint } {
  const fee = teamFeeOf(payout, bps);
  return { fee, paid: payout - fee };
}

/** A pool's own rates, in bps, as `poolState` stores them (copied once at registration, never moved). */
export interface PoolRates {
  skimBuyFarBps: number;
  skimSellFarBps: number;
  skimBuyAtFloorBps: number;
  skimSellAtFloorBps: number;
  creatorBuyBps: number;
  creatorSellBps: number;
  holderBuyBps: number;
  holderSellBps: number;
  /** The pool's FLOOR Platform fee, checked against `MAX_POOL_TEAM_FEE_BPS`. */
  teamFeeBps: number;
}

/**
 * `FloorHookMath.skimBps`, statement for statement: the skim in whole bps at `distance` ticks from the band's DEAR edge
 * (`dearEdge − current`, engine frame). A buy ramps from its at-floor rate at the bid to its far rate at the top of the
 * ramp; a sell from its at-floor rate down to its far rate; a sell whose far rate is at or above its at-floor rate is flat.
 */
export function skimBpsAt(side: 'buy' | 'sell', distance: number, r: Pick<PoolRates, 'skimBuyFarBps' | 'skimSellFarBps' | 'skimBuyAtFloorBps' | 'skimSellAtFloorBps'>): number {
  const ramp = SKIM_RAMP_TICKS;
  let closeness = 0;
  if (distance <= 0) closeness = 10_000;
  else if (distance < ramp) closeness = 10_000 - Math.floor((distance * 10_000) / ramp);
  if (side === 'buy') {
    const far = r.skimBuyFarBps, at = r.skimBuyAtFloorBps;
    if (far >= at) return far - Math.floor(((far - at) * closeness) / 10_000);
    return far + Math.floor(((at - far) * closeness) / 10_000);
  }
  const far = r.skimSellFarBps, at = r.skimSellAtFloorBps;
  if (far >= at) return far;
  return far + Math.floor(((at - far) * closeness) / 10_000);
}

/** `FloorPolicy.STAIR_GAP_SKIM_TICKS`: the gap (ticks) at and past which a buy's skim is scaled by the full multiplier. */
export const GAP_SKIM_TICKS = 5_109;
/** `FloorPolicy.STAIR_GAP_SKIM_FROM_TICKS`: the gap (ticks) at and under which a buy's skim is not scaled. */
export const GAP_SKIM_FROM_TICKS = 2_232;
/** `FloorPolicy.STAIR_GAP_SKIM_MULT_BPS`: the full multiplier, bps (1.5×). */
export const GAP_SKIM_MULT_BPS = 15_000;
/** `FloorPolicy.STAIR_GAP_SKIM_MAX_BPS`: the cap on a scaled buy skim, bps. */
export const GAP_SKIM_MAX_BPS = 1_500;

/** The pool's staircase cushion as `stairs()` answers it — the two fields the gap buy skim reads (engine-frame ticks). */
export interface CushionEdge {
  /** The cushion's dear edge (its top in price), engine frame. */
  cushionLower: number;
  /** 0 when no cushion stands in the pool. */
  cushionLiq: bigint;
}

/**
 * `SingletonFloorEngine._gapSkim`, statement for statement: a BUY's floor skim `bps` (already ramped, `skimBpsAt`) scaled by
 * how far the price stands above the cushion's top. `gap = cushionLower − current` (engine frame). At `gap ≥ 5,109` the
 * multiplier is 1.5×; between 2,232 (exclusive) and 5,109 it rises linearly in ticks,
 * `10_000 + 5_000 × (gap − 2,232) / (5,109 − 2,232)` (floored); at or under 2,232 the skim is returned untouched. A scaled
 * skim is `floor(bps × m / 10_000)`, capped at 1,500. No cushion standing (`cushionLiq == 0`): untouched.
 */
export function gapSkimBps(bps: number, cushion: CushionEdge, currentTick: number): number {
  if (cushion.cushionLiq === 0n) return bps;
  const gap = cushion.cushionLower - currentTick;
  let m: number;
  if (gap >= GAP_SKIM_TICKS) m = GAP_SKIM_MULT_BPS;
  else if (gap > GAP_SKIM_FROM_TICKS) m = 10_000 + Math.floor(((GAP_SKIM_MULT_BPS - 10_000) * (gap - GAP_SKIM_FROM_TICKS)) / (GAP_SKIM_TICKS - GAP_SKIM_FROM_TICKS));
  else return bps;
  const scaled = Math.floor((bps * m) / 10_000);
  return scaled > GAP_SKIM_MAX_BPS ? GAP_SKIM_MAX_BPS : scaled;
}

/** The hook's four takes off ONE quote leg, each `leg × bps / 10_000` rounded down (`_takeFees`). */
export function hookTakes(side: 'buy' | 'sell', leg: bigint, skimBps: number, r: PoolRates): { floorSkim: bigint; creator: bigint; holder: bigint; floorPlatform: bigint; total: bigint } {
  const of = (bps: number) => (leg * BigInt(bps)) / 10_000n;
  const floorSkim = of(skimBps);
  const creator = of(side === 'buy' ? r.creatorBuyBps : r.creatorSellBps);
  const holder = of(side === 'buy' ? r.holderBuyBps : r.holderSellBps);
  const floorPlatform = of(r.teamFeeBps);
  return { floorSkim, creator, holder, floorPlatform, total: floorSkim + creator + holder + floorPlatform };
}

const ceilDiv = (a: bigint, b: bigint): bigint => (a + b - 1n) / b;

/**
 * A pool swap's five parts, in the QUOTE's smallest unit.
 * - BUY (exact quote in, the leg is `amountIn`): the hook takes its four off the leg BEFORE the swap (`_beforeSwap`), at the
 *   PRE-swap tick; the LP fee is the pool's 0.25% of what is left (`ceil`, as v4's swap step rounds it).
 * - SELL (exact tokens in, the leg is the swap's quote output): the hook takes its four AFTER the swap, off the leg; `amountOut`
 *   is the leg less them, so the leg is the least value whose remainder covers `amountOut`. The LP fee was charged on the
 *   TOKEN input and is stated in quote at the swap's own rate, `leg × fee / (1e6 − fee)` — an estimate.
 * ⚠ A sell's skim is charged at the POST-swap tick (further up the ramp toward the floor); `skimBps` here is whatever the caller
 *   read. Pass the pre-trade figure and the skim part is a MINIMUM.
 * A buy's `skimBps` is the figure the engine charges: on contracts with the gap buy skim, `gapSkimBps(skimBpsAt(...), ...)`.
 */
export function poolFeeParts(side: 'buy' | 'sell', amountIn: bigint, amountOut: bigint, skimBps: number, r: PoolRates): FeePart[] {
  if (side === 'buy') {
    const t = hookTakes('buy', amountIn, skimBps, r);
    const lp = ceilDiv((amountIn - t.total) * LP_FEE_PIPS, 1_000_000n);
    return feeList(lp, t);
  }
  const bps = skimBps + r.creatorSellBps + r.holderSellBps + r.teamFeeBps;
  if (bps >= 10_000) throw new RangeError(`the pool's sell takes sum to ${bps} bps`);
  let leg = (amountOut * 10_000n) / BigInt(10_000 - bps);
  while (leg - hookTakes('sell', leg, skimBps, r).total < amountOut) leg += 1n;
  while (leg > 0n && leg - 1n - hookTakes('sell', leg - 1n, skimBps, r).total >= amountOut) leg -= 1n;
  const t = hookTakes('sell', leg, skimBps, r);
  const lp = ceilDiv(leg * LP_FEE_PIPS, 1_000_000n - LP_FEE_PIPS);
  return feeList(lp, t);
}

/** A bonding trade's take: the sale's FLOOR Platform fee, and the four pool parts at zero (there is no pool yet). */
export function bondingFeeParts(floorPlatform: bigint): FeePart[] {
  return feeList(0n, { floorSkim: 0n, creator: 0n, holder: 0n, floorPlatform });
}

function feeList(lp: bigint, t: { floorSkim: bigint; creator: bigint; holder: bigint; floorPlatform: bigint }): FeePart[] {
  return [
    { kind: 'lp', amount: lp },
    { kind: 'floorSkim', amount: t.floorSkim },
    { kind: 'creator', amount: t.creator },
    { kind: 'holder', amount: t.holder },
    { kind: 'floorPlatform', amount: t.floorPlatform },
  ];
}

/** The five kinds an EVM quote lists, in order. A list missing any of them is refused (`assertWholeTake`). */
export const EVM_FEE_KINDS = ['lp', 'floorSkim', 'creator', 'holder', 'floorPlatform'] as const;

/** Throws unless `fees` names each of the five EVM parts exactly once — the whole take, or none of it. */
export function assertWholeTake(fees: readonly FeePart[]): void {
  const kinds = fees.map((f) => f.kind);
  if (kinds.length !== EVM_FEE_KINDS.length || EVM_FEE_KINDS.some((k) => kinds.filter((x) => x === k).length !== 1)) {
    throw new RangeError(`a quote lists the trader's whole take or none of it; got [${kinds.join(', ')}]`);
  }
  for (const f of fees) if (f.amount < 0n) throw new RangeError(`the ${f.kind} part is negative`);
}
