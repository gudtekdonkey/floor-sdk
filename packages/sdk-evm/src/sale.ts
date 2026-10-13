/**
 * The bonding sale's arithmetic: tranche prices, the buy walk and the sell-back payout, in bigint.
 *
 * Mirrors `FloorLaunch._buy` / `_walk` / `_sell`. A sale prices at one price (`p0 == p1`); `p1` is still read and walked.
 *
 * ⛔ The FLOOR Platform fee rides ON TOP of the sale price: a buy's `amountIn` is the GROSS cap, the walk spends at most
 *    `saleNetOf(gross)`, the fee is `floor(spent × bps / 10_000)`, and the change comes back to the payer.
 * ⛔ A buy never receives more than the sale has left: an overfill is filled to the end and the rest refunded.
 * ⛔ ROUNDING is per contract version (`ContractRules.saleCost`, read off the deployment row): the current `FloorLaunch`
 *   rounds a tranche's cost UP (`(tokens × price + 1e18 − 1) / 1e18`), older ones DOWN
 *   (`tokens × price / 1e18`). Pass the row's rule; a quote in the other rounding is off by up to one base unit a tranche.
 * ⛔ The SELL-BACK is per contract version too (`ContractRules.sellBack`): stateless at the sale price, or pro-rata.
 */
import type { ContractRules } from './deployments.js';
import { MalformedSale } from './errors.js';
import { MAX_SALE_TEAM_FEE_BPS, saleGrossFor, saleNetOf, teamFeeOf } from './fees.js';

export const MAX_TRANCHES = 255;

/** A sale's book as its getters answer it. */
export interface SaleBook {
  saleTokens: bigint;
  sold: bigint;
  p0: bigint;
  p1: bigint;
  tranches: number;
  /** The SALE's own FLOOR Platform fee, bps (`FloorLaunch.teamFeeBps()`), 0 on a set without the fee. */
  teamFeeBps: number;
}

export interface SaleReads extends SaleBook {
  funded: boolean;
  raised: bigint;
  graduated: boolean;
}

export function validTranches(n: unknown): n is number {
  return typeof n === 'number' && Number.isInteger(n) && n >= 1 && n <= MAX_TRANCHES;
}

/** The getters' answers, checked: tranches in 1..255, the fee a rate under the sale's cap. Throws `MalformedSale`. */
export function saleReadsFrom(raw: Omit<SaleReads, 'tranches'> & { tranches: number | bigint }): SaleReads {
  const t = raw.tranches;
  const tranches = typeof t === 'bigint' ? (t >= 1n && t <= BigInt(MAX_TRANCHES) ? Number(t) : NaN) : t;
  if (!validTranches(tranches)) throw new MalformedSale(`tranches() returned ${String(t)}, outside 1..${MAX_TRANCHES}`);
  if (!Number.isInteger(raw.teamFeeBps) || raw.teamFeeBps < 0) throw new MalformedSale(`teamFeeBps ${String(raw.teamFeeBps)} is not a rate`);
  if (raw.teamFeeBps > MAX_SALE_TEAM_FEE_BPS) throw new MalformedSale(`teamFeeBps ${raw.teamFeeBps} is above the sale's cap ${MAX_SALE_TEAM_FEE_BPS} — refused, never walked`);
  return { ...raw, tranches };
}

export function trancheSize(s: SaleBook): bigint {
  return validTranches(s.tranches) ? s.saleTokens / BigInt(s.tranches) : 0n;
}

/** Tranche `i`'s price: raw quote per WHOLE (1e18-unit) token, linear from `p0` to `p1`. */
export function tranchePrice(s: SaleBook, i: number): bigint {
  if (!validTranches(s.tranches) || s.tranches === 1) return s.p0;
  return s.p0 + ((s.p1 - s.p0) * BigInt(i)) / BigInt(s.tranches - 1);
}

export function currentTranche(s: SaleBook): number {
  const size = trancheSize(s);
  if (size === 0n) return 0;
  return Math.min(Number(s.sold / size), Math.max(s.tranches - 1, 0));
}

/** How a tranche's cost is rounded (`ContractRules.saleCost`). */
export type SaleCostRounding = ContractRules['saleCost'];

/** A tranche slice's cost: `tokens × price / unit`, rounded as `rounding` says. */
export function trancheCost(tokens: bigint, price: bigint, unit: bigint, rounding: SaleCostRounding): bigint {
  const n = tokens * price;
  return rounding === 'ceil' ? (n + unit - 1n) / unit : n / unit;
}

/** The whole raise: what buying every sale token costs (before the FLOOR Platform fee). */
export function saleTargetWei(s: SaleBook, rounding: SaleCostRounding = 'ceil'): bigint {
  if (!validTranches(s.tranches)) return 0n;
  const size = trancheSize(s);
  if (size === 0n) return 0n;
  const E18 = 10n ** 18n;
  let total = 0n;
  for (let t = 0; t < s.tranches; t++) {
    const end = t === s.tranches - 1 ? s.saleTokens : BigInt(t + 1) * size;
    total += trancheCost(end - BigInt(t) * size, tranchePrice(s, t), E18, rounding);
  }
  return total;
}

const UINT256_MAX = (1n << 256n) - 1n;

/** `FloorLaunch._walk(budget)`: tokens received and quote spent, from the book's current `sold`. */
function walkBuy(s: SaleBook, amountIn: bigint, unit: bigint, rounding: SaleCostRounding): { got: bigint; spent: bigint } {
  const size = trancheSize(s);
  if (size === 0n || amountIn <= 0n) return { got: 0n, spent: 0n };
  const costOf = (tokens: bigint, price: bigint): bigint => trancheCost(tokens, price, unit, rounding);
  let sold = s.sold, left = amountIn, got = 0n;
  while (left > 0n && sold < s.saleTokens) {
    const t = Math.min(Number(sold / size), s.tranches - 1);
    const end = t === s.tranches - 1 ? s.saleTokens : BigInt(t + 1) * size;
    const capacity = end - sold;
    const price = tranchePrice(s, t);
    const costAll = costOf(capacity, price);
    if (left >= costAll && costAll > 0n) {
      sold += capacity; got += capacity; left -= costAll;
    } else {
      if (price === 0n) break;
      let amt = (left * unit) / price;
      if (amt === 0n) break;
      if (amt > capacity) amt = capacity;
      const cost = costOf(amt, price);
      sold += amt; got += amt; left -= cost;
      break;
    }
  }
  return { got, spent: amountIn - left };
}

export interface SaleBuyQuote {
  /** Tokens received. */
  got: bigint;
  /** Quote spent at the sale price (the raise grows by this). */
  spent: bigint;
  /** The sale's FLOOR Platform fee on `spent`. */
  fee: bigint;
  /** The most one buy can receive now (what is left). */
  maxGot: bigint;
  /** The least GROSS that buys everything left. */
  fillWei: bigint;
  /** True when `amountIn` exceeds `fillWei` (the rest is refunded). */
  overfills: boolean;
}

/**
 * A bonding buy of GROSS `amountInWei` (the fee rides on top). `tokenDecimals` is 18 on every EVM launch. `rounding` is the
 * deployment's `ContractRules.saleCost` (default: the current contracts, `ceil`). `fillWei` is the least gross whose net
 * walks to the end of the sale in that rounding.
 */
export function saleBuyQuote(s: SaleBook, amountInWei: bigint, tokenDecimals: number, rounding: SaleCostRounding = 'ceil'): SaleBuyQuote {
  const unit = 10n ** BigInt(tokenDecimals);
  const all = walkBuy(s, UINT256_MAX, unit, rounding);
  const fillWei = saleGrossFor(all.spent, s.teamFeeBps);
  const q = walkBuy(s, saleNetOf(amountInWei, s.teamFeeBps), unit, rounding);
  return { got: q.got, spent: q.spent, fee: teamFeeOf(q.spent, s.teamFeeBps), maxGot: all.got, fillWei, overfills: amountInWei > fillWei };
}

/**
 * A sell-back of `tokens` (`FloorLaunch._sell`), less the sale's FLOOR Platform fee (`floor(payout × bps / 10_000)`).
 * - `stateless` (the current contracts, default): `payout = tokens × p0 / 1e18` — the tokens at the sale's one price,
 *   rounded down — capped at the position's `contributed`.
 * - `proRata` (older contracts): `payout = contributed × tokens / allocation`, rounded down.
 * The position (`allocation` tokens bought for `contributed` quote) bounds both: `null` where the sale would revert
 * (`tokens` zero or above the allocation).
 */
export function sellBackQuote(p0: bigint, contributed: bigint, allocation: bigint, tokens: bigint, teamFeeBps: number, rule: ContractRules['sellBack'] = 'stateless'): { payout: bigint; fee: bigint; paid: bigint } | null {
  if (tokens <= 0n || allocation <= 0n || tokens > allocation) return null;
  let payout: bigint;
  if (rule === 'proRata') payout = (contributed * tokens) / allocation;
  else {
    const atPrice = (tokens * p0) / 10n ** 18n;
    payout = atPrice > contributed ? contributed : atPrice;
  }
  const fee = teamFeeOf(payout, teamFeeBps);
  return { payout, fee, paid: payout - fee };
}
