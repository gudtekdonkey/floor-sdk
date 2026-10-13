import { describe, expect, it } from 'vitest';
import type { Address } from 'viem';
import {
  MAX_SQRT, MAX_TICK, MIN_SQRT, MIN_TICK, bandTicks, decodeSlot0, marketTick, poolIdOf, poolKey, priceX18AtTick, quoteAtCurrency1, sqrtPriceAtTick,
  saleBuyQuote, saleReadsFrom, saleTargetWei, sellBackQuote, tranchePrice, trancheSize, MalformedSale,
  teamFeeOf, saleNetOf, saleGrossFor, sellBackPayOf, rateWithinCap, RateAboveCap, skimBpsAt, hookTakes, poolFeeParts, bondingFeeParts, assertWholeTake,
  SKIM_RAMP_TICKS, type PoolRates, type SaleBook,
  gapSkimBps, GAP_SKIM_TICKS, GAP_SKIM_FROM_TICKS, GAP_SKIM_MAX_BPS, currentTranche, validTranches, trancheCost,
} from '../src/index.js';

const E18 = 10n ** 18n;

describe('pool keys and ids (reference fixtures)', () => {
  it('an older Sepolia set: the $FLOOR singleton pool: token + hook → the id the hook keys it by', () => {
    // Read off SingletonFloorHook.poolForAccount(launch) on an older Sepolia set.
    expect(poolIdOf('0x8961A353dF2a14c499802D4BcA8659EC0eA824aB', '0x483EFf3913CC30771fAA9e42826D6083775260Cc')).toBe('0x57f11c429c6ee4621f7eba5a7f9e52486b7c6b25c0c71d65b26d5dc06c0da90d');
  });
  it('two stock-quoted pools in both orders ', () => {
    const MSTOCK = '0x76B1Ad3252cAC3F01794634B1efB29BC5cC3b1ce' as Address, HOOK = '0x01AdA3D96ed87b2a6244336DFbE395b5AEFD20cC' as Address;
    const SHALE = '0x626331997f9ae3e998d831010c4d3cA2eE172b4c' as Address, TUFF = '0xC9B915F6720A794396369D6c04b6dE5F6da25C0b' as Address;
    expect(poolIdOf(SHALE, HOOK, MSTOCK)).toBe('0xc94853d21e3aaf35ce748ab2960b58d5c8b08d4f14098b73d5d7a962369e0b36');
    expect(poolIdOf(TUFF, HOOK, MSTOCK)).toBe('0x7b5f62a0915595245ad6d88a7617f8b762f37731eda711277e8958549be8ea26');
    expect(quoteAtCurrency1(poolKey(SHALE, HOOK, MSTOCK), MSTOCK)).toBe(true);
    expect(quoteAtCurrency1(poolKey(TUFF, HOOK, MSTOCK), MSTOCK)).toBe(false);
    expect(quoteAtCurrency1(poolKey(TUFF, HOOK, MSTOCK), HOOK)).toBeNull();
  });
  it('a native pool puts the quote at currency0, dynamic fee, spacing 60', () => {
    const k = poolKey('0x1111111111111111111111111111111111111111', '0x2222222222222222222222222222222222222222');
    expect(k).toEqual({ currency0: '0x0000000000000000000000000000000000000000', currency1: '0x1111111111111111111111111111111111111111', fee: 0x800000, tickSpacing: 60, hooks: '0x2222222222222222222222222222222222222222' });
  });
});

describe('tick math', () => {
  it('sqrtPriceAtTick is v4 TickMath at its ends and at 0', () => {
    expect(sqrtPriceAtTick(MIN_TICK)).toBe(MIN_SQRT);
    expect(sqrtPriceAtTick(MAX_TICK)).toBe(MAX_SQRT);
    expect(sqrtPriceAtTick(0)).toBe(1n << 96n);
    expect(() => sqrtPriceAtTick(MAX_TICK + 1)).toThrow(RangeError);
  });
  it('the price at a tick is 1.0001^-tick raw per raw, whole per whole ×1e18', () => {
    expect(priceX18AtTick(0, 18, 18)).toBe(E18);
    // 1.0001^-(-69082) ≈ 999.99 — a dearer token is a LOWER tick in the engine frame.
    const p = priceX18AtTick(-69082, 18, 18);
    expect(p > 999n * E18 && p < 1001n * E18).toBe(true);
    // A 6-decimal quote: the same raw price is 10^12 times more whole quote per whole token.
    expect(priceX18AtTick(276324, 18, 6) / 10n ** 12n).toBe(priceX18AtTick(276324, 18, 18));
  });
  it('slot0 decodes a negative tick, and the engine frame maps a currency1 quote with -tick - 1', () => {
    const word = (`0x${((BigInt(0x1000000 - 5) << 160n) | 123n).toString(16).padStart(64, '0')}`) as `0x${string}`;
    expect(decodeSlot0(word)).toEqual({ sqrtPriceX96: 123n, tick: -5, protocolFee: 0, lpFee: 0 });
    expect(marketTick(-5, false)).toBe(-5);
    expect(marketTick(-5, true)).toBe(4);
    expect(bandTicks(60)).toBe(120);
    expect(bandTicks(100)).toBe(200);
    expect(bandTicks(1)).toBe(120);
  });
});

/* Fixtures: a sale on an older Sepolia set (costs rounded down), read with eth_call, and what buying it out cost on chain. */
const LIVE: SaleBook = { saleTokens: 550_000_000n * E18, sold: 0n, p0: 225_000_000_000n, p1: 225_000_000_000n, tranches: 10, teamFeeBps: 0 };
const LIVE_BUYOUT_WEI = 123_750_000_000_000_000_000n;
const FLOOR_SEPOLIA: SaleBook = { saleTokens: 550_000_000n * E18, sold: 0n, p0: 20_833_333_333n, p1: 20_833_333_333n, tranches: 10, teamFeeBps: 0 };
const RISING: SaleBook = { ...LIVE, p0: 100n * 10n ** 9n, p1: 190n * 10n ** 9n };

describe('the bonding sale', () => {
  it('the whole raise matches what buying the live sale out cost on chain', () => {
    expect(saleTargetWei(LIVE, 'floor')).toBe(LIVE_BUYOUT_WEI);
    expect(saleBuyQuote(LIVE, LIVE_BUYOUT_WEI, 18, 'floor')).toEqual({ got: LIVE.saleTokens, spent: LIVE_BUYOUT_WEI, fee: 0n, maxGot: LIVE.saleTokens, fillWei: LIVE_BUYOUT_WEI, overfills: false });
    expect(saleBuyQuote(LIVE, LIVE_BUYOUT_WEI + 1n, 18, 'floor').overfills).toBe(true);
  });
  it('a buy never receives more than the sale has left (12 ETH on the $FLOOR sale is the whole 550M, not 576M)', () => {
    const q = saleBuyQuote(FLOOR_SEPOLIA, 12n * E18, 18, 'floor');
    expect(q.got).toBe(FLOOR_SEPOLIA.saleTokens);
    expect(q.overfills).toBe(true);
    const ok = saleBuyQuote(FLOOR_SEPOLIA, 1_500_000_000_000_000_000n, 18, 'floor');
    expect(ok.overfills).toBe(false);
    expect([71_999_999n, 72_000_000n]).toContain(ok.got / E18);
  });
  it('a rising sale prices each tranche at its own price', () => {
    const size = trancheSize(RISING);
    const t0 = (size * tranchePrice(RISING, 0)) / E18;
    const x = 1000n * E18;
    expect(saleBuyQuote(RISING, t0 + (x * tranchePrice(RISING, 1)) / E18, 18, 'floor').got).toBe(size + x);
  });
  it('the FLOOR Platform fee rides on top: the walk spends the NET and the fee is floor(spent × bps / 10_000)', () => {
    const book = { ...LIVE, teamFeeBps: 100 };
    const gross = 101n * 10n ** 16n;
    const q = saleBuyQuote(book, gross, 18, 'floor');
    expect(saleNetOf(gross, 100)).toBe(E18);
    expect(q.spent).toBe(saleBuyQuote(LIVE, E18, 18, 'floor').spent);
    expect(q.fee).toBe(teamFeeOf(q.spent, 100));
    expect(q.spent + q.fee <= gross).toBe(true);
    expect(saleGrossFor(saleNetOf(gross, 100), 100)).toBe(gross);
  });
  it('a sell-back pays the tokens at p0 rounded down, capped at contributed, less the sale\'s fee; refuses more than the allocation', () => {
    const p0 = 10n ** 16n; // 0.01 quote a whole token
    // 250 whole tokens of a 1,000-token position bought for 10 quote + a 7-wei residue: the pro-rata would pay 2.5e18 + 1
    expect(sellBackQuote(p0, 10n * E18 + 7n, 1000n * E18, 250n * E18, 100)).toEqual({ payout: 25n * 10n ** 17n, fee: 25n * 10n ** 15n, paid: 2_475n * 10n ** 15n });
    // rounded down: 3 base units at 0.01 is 0.03 wei, so nothing
    expect(sellBackQuote(p0, 10n * E18, 1000n * E18, 3n, 100)).toEqual({ payout: 0n, fee: 0n, paid: 0n });
    expect(sellBackQuote(3n * 10n ** 17n, 1000n * E18, 1000n * E18, 101n, 0)!.payout).toBe(30n); // floor(101 · 0.3)
    // capped at contributed (a position whose basis is under the price)
    expect(sellBackQuote(p0, 10n * E18 - 1n, 1000n * E18, 1000n * E18, 0)!.payout).toBe(10n * E18 - 1n);
    // the same tokens pay the same whatever the position paid
    expect(sellBackQuote(p0, 10n * E18, 1000n * E18, 4n * E18, 100)).toEqual(sellBackQuote(p0, 999n * E18, 5000n * E18, 4n * E18, 100));
    expect(sellBackQuote(p0, 10n * E18, 1000n * E18, 1000n * E18 + 1n, 100)).toBeNull();
    expect(sellBackQuote(p0, 10n * E18, 1000n * E18, 0n, 100)).toBeNull();
    expect(sellBackPayOf(25n * 10n ** 17n, 100)).toEqual({ fee: 25n * 10n ** 15n, paid: 2_475n * 10n ** 15n });
  });
  it('the current contracts round each tranche\'s cost UP: a buy never pays less than the price, and a 1-wei buy is not free', () => {
    // two tranches of 5e18+3 / 5e18+4 base units at 3 wei a whole token: each costs 15.000…009 / 15.000…012 wei
    const SMALL: SaleBook = { saleTokens: 10n * E18 + 7n, sold: 0n, p0: 3n, p1: 3n, tranches: 2, teamFeeBps: 0 };
    expect(saleTargetWei(SMALL, 'floor')).toBe(30n);
    expect(saleTargetWei(SMALL, 'ceil')).toBe(32n);
    expect(saleTargetWei(SMALL)).toBe(32n); // default: the current contracts
    expect(trancheCost(5n * E18 + 3n, 3n, E18, 'ceil')).toBe(16n);
    expect(trancheCost(5n * E18 + 3n, 3n, E18, 'floor')).toBe(15n);
    // 31 wei: ceil fills tranche 0 (16), then 15 wei buy 5e18 of tranche 1 at a cost of exactly 15; floor sells out for 30
    expect(saleBuyQuote(SMALL, 31n, 18, 'ceil')).toEqual({ got: 10n * E18 + 3n, spent: 31n, fee: 0n, maxGot: 10n * E18 + 7n, fillWei: 32n, overfills: false });
    expect(saleBuyQuote(SMALL, 31n, 18, 'floor')).toEqual({ got: 10n * E18 + 7n, spent: 30n, fee: 0n, maxGot: 10n * E18 + 7n, fillWei: 30n, overfills: true });
    // the boundary: the least gross that sells out is fillWei, in each rounding
    expect(saleBuyQuote(SMALL, 32n, 18, 'ceil').got).toBe(10n * E18 + 7n);
    expect(saleBuyQuote(SMALL, 31n, 18, 'ceil').got < 10n * E18 + 7n).toBe(true);
    // 1 wei: floor(1e18 / 3) base units, which cost 0.99… wei — 1 rounded up, 0 rounded down
    expect(saleBuyQuote(SMALL, 1n, 18, 'ceil')).toMatchObject({ got: 333_333_333_333_333_333n, spent: 1n });
    expect(saleBuyQuote(SMALL, 1n, 18, 'floor')).toMatchObject({ got: 333_333_333_333_333_333n, spent: 0n });
    // with a fee, fillWei is the least gross whose net walks to the end
    const fee = { ...SMALL, teamFeeBps: 100 };
    const q = saleBuyQuote(fee, 1n, 18, 'ceil');
    expect(saleNetOf(q.fillWei, 100) >= 32n && saleNetOf(q.fillWei - 1n, 100) < 32n).toBe(true);
  });
  it('the older contracts\' sell-back pays the position\'s pro-rata share, less the fee', () => {
    // the 250-of-1,000 position bought for 10 quote + 7 wei: 2.5e18 + 1 (the stateless rule pays 2.5e18)
    expect(sellBackQuote(10n ** 16n, 10n * E18 + 7n, 1000n * E18, 250n * E18, 100, 'proRata')).toEqual({ payout: 25n * 10n ** 17n + 1n, fee: 25n * 10n ** 15n, paid: 2_475n * 10n ** 15n + 1n });
    expect(sellBackQuote(10n ** 16n, 10n * E18 + 7n, 1000n * E18, 250n * E18, 100)!.payout).toBe(25n * 10n ** 17n);
    expect(sellBackQuote(10n ** 16n, 10n * E18, 1000n * E18, 1000n * E18 + 1n, 100, 'proRata')).toBeNull();
  });
  it('currentTranche and validTranches', () => {
    expect(validTranches(1)).toBe(true);
    expect(validTranches(255)).toBe(true);
    for (const n of [0, 256, 1.5, '10', -1, NaN]) expect(validTranches(n)).toBe(false);
    const size = trancheSize(LIVE);
    expect(currentTranche(LIVE)).toBe(0);
    expect(currentTranche({ ...LIVE, sold: size - 1n })).toBe(0);
    expect(currentTranche({ ...LIVE, sold: size })).toBe(1);
    expect(currentTranche({ ...LIVE, sold: LIVE.saleTokens })).toBe(9); // sold out: the last tranche, never past it
    expect(currentTranche({ ...LIVE, tranches: 0 })).toBe(0);
  });
  it('the getters are checked: tranches 1..255 and the fee under the sale\'s cap', () => {
    const base = { ...LIVE, funded: true, raised: 0n, graduated: false };
    expect(saleReadsFrom({ ...base, tranches: 10n }).tranches).toBe(10);
    expect(() => saleReadsFrom({ ...base, tranches: 0n })).toThrow(MalformedSale);
    expect(() => saleReadsFrom({ ...base, teamFeeBps: 101 })).toThrow(/above the sale's cap 100/);
  });
});

const RATES: PoolRates = {
  skimBuyFarBps: 100, skimSellFarBps: 100, skimBuyAtFloorBps: 0, skimSellAtFloorBps: 500,
  creatorBuyBps: 50, creatorSellBps: 75, holderBuyBps: 25, holderSellBps: 0, teamFeeBps: 25,
};

describe('fees', () => {
  it('the skim ramp is FloorHookMath.skimBps at its corners', () => {
    // At or inside the dear edge: the at-floor rate. At or past the ramp: the far rate.
    expect(skimBpsAt('buy', 0, RATES)).toBe(0);
    expect(skimBpsAt('buy', -50, RATES)).toBe(0);
    expect(skimBpsAt('buy', SKIM_RAMP_TICKS, RATES)).toBe(100);
    expect(skimBpsAt('sell', 0, RATES)).toBe(500);
    expect(skimBpsAt('sell', SKIM_RAMP_TICKS + 1, RATES)).toBe(100);
    // Half way: closeness 10_000 - floor(8047 × 10_000 / 16094) = 5000.
    expect(skimBpsAt('buy', 8047, RATES)).toBe(50);
    expect(skimBpsAt('sell', 8047, RATES)).toBe(300);
    // A sell whose far rate is at or above its at-floor rate does not ramp.
    expect(skimBpsAt('sell', 0, { ...RATES, skimSellAtFloorBps: 100 })).toBe(100);
    // A buy whose at-floor rate is above its far rate ramps the other way.
    expect(skimBpsAt('buy', 0, { ...RATES, skimBuyAtFloorBps: 300 })).toBe(300);
  });
  it('the gap buy skim is SingletonFloorEngine._gapSkim: 1× to 2,232 ticks, a linear ramp to 1.5× at 5,109, capped at 1,500 bps', () => {
    const top = 10_000;
    const at = (bps: number, gap: number, liq = 1n) => gapSkimBps(bps, { cushionLower: top, cushionLiq: liq }, top - gap);
    expect([GAP_SKIM_FROM_TICKS, GAP_SKIM_TICKS, GAP_SKIM_MAX_BPS]).toEqual([2_232, 5_109, 1_500]);
    // at and under 2,232 ticks (20%): untouched — not even capped
    for (const gap of [-500, 0, 2_232]) expect([at(75, gap), at(100, gap), at(1_000, gap), at(1_600, gap)]).toEqual([75, 100, 1_000, 1_600]);
    // 2,233: m = 10,000 + floor(5,000 × 1 / 2,877) = 10,001 — and a scaled skim is capped
    expect([at(1_000, 2_233), at(1_600, 2_233)]).toEqual([1_000, 1_500]);
    // half way, 3,670: m = 10,000 + floor(5,000 × 1,438 / 2,877) = 12,499
    expect([at(75, 3_670), at(100, 3_670), at(1_000, 3_670)]).toEqual([93, 124, 1_249]);
    // 5,108: m = 14,998; 5,109 and past: 15,000
    expect(at(1_000, 5_108)).toBe(1_499);
    for (const gap of [5_109, 5_110, 20_000]) expect([at(75, gap), at(100, gap), at(1_000, gap), at(1_001, gap)]).toEqual([112, 150, 1_500, 1_500]);
    // no cushion standing: untouched, whatever its edge says; a zero ramp stays zero
    expect(at(1_000, 20_000, 0n)).toBe(1_000);
    expect(at(0, 20_000)).toBe(0);
  });
  it('the hook takes four rates off ONE leg, each rounded down, and sums them', () => {
    expect(hookTakes('buy', 10_001n, 100, RATES)).toEqual({ floorSkim: 100n, creator: 50n, holder: 25n, floorPlatform: 25n, total: 200n });
  });
  it('a pool buy lists all five parts, the LP fee on what the hook leaves', () => {
    const fees = poolFeeParts('buy', E18, 1n, 100, RATES);
    assertWholeTake(fees);
    const by = Object.fromEntries(fees.map((f) => [f.kind, f.amount]));
    expect(by).toEqual({ lp: (98n * 10n ** 16n * 2500n + 999_999n) / 1_000_000n, floorSkim: 10n ** 16n, creator: 5n * 10n ** 15n, holder: 25n * 10n ** 14n, floorPlatform: 25n * 10n ** 14n });
  });
  it('a pool sell recovers the leg the hook took from: amountOut + takes = leg', () => {
    const amountOut = 987_654_321_000n;
    const fees = poolFeeParts('sell', 5n * E18, amountOut, 300, RATES);
    assertWholeTake(fees);
    const takes = fees.filter((f) => f.kind !== 'lp').reduce((s, f) => s + f.amount, 0n);
    const leg = amountOut + takes;
    expect(hookTakes('sell', leg, 300, RATES).total).toBe(takes);
    expect(leg - hookTakes('sell', leg - 1n, 300, RATES).total - 1n < amountOut).toBe(true);
  });
  it('a bonding trade lists the five parts with the pool four at zero', () => {
    const fees = bondingFeeParts(7n);
    assertWholeTake(fees);
    expect(fees.filter((f) => f.amount !== 0n)).toEqual([{ kind: 'floorPlatform', amount: 7n }]);
  });
  it('a partial take is refused — the whole take or none of it', () => {
    expect(() => assertWholeTake([{ kind: 'lp', amount: 1n }])).toThrow(/whole take/);
    expect(() => assertWholeTake([...bondingFeeParts(0n), { kind: 'carpenterPlatform', amount: 0n }])).toThrow(/whole take/);
  });
  it('rates above their cap are refused by name, never clamped', () => {
    expect(rateWithinCap(50, 50, 'x')).toBe(50);
    expect(() => rateWithinCap(51, 50, 'the pool')).toThrow(RateAboveCap);
    expect(() => rateWithinCap(1.5, 50, 'x')).toThrow(/not a rate/);
    expect(teamFeeOf(999n, 100)).toBe(9n);
  });
});
