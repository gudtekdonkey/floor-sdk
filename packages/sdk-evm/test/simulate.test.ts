import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { MAX_SQRT, MAX_TICK, MIN_SQRT, MIN_TICK, sqrtPriceAtTick } from '../src/pool.js';
import { computeSwapStepExactIn, getTickAtSqrtPrice, simulateSwap, type SwapSnapshot } from '../src/simulate.js';

// Each fixture: a FLOOR pool read at one or more states, with what Uniswap's V4Quoter answered for each swap at that block
// (`null` = the quoter reverted). `base-*.json`: pools created and graduated on a fork of Base, at several states, recorded by
// `scripts/fork-parity.mts`. `live-*.json`: each chain's live graduated pools, recorded by `scripts/live-parity.mts`.
const DIR = join(__dirname, 'fixtures', 'simulate');
type Fixture = { chainId: number; label: string; quote: string; cases: { state: string; block: string; snapshot: any; expected: { side: 'buy' | 'sell'; amountIn: string; amountOut: string | null }[] }[] };
const fixtures: Fixture[] = readdirSync(DIR).filter((f) => f.endsWith('.json')).map((f) => JSON.parse(readFileSync(join(DIR, f), 'utf8')));

function snapshotOf(j: any): SwapSnapshot {
  return {
    pool: {
      sqrtPriceX96: BigInt(j.pool.sqrtPriceX96), tick: j.pool.tick, liquidity: BigInt(j.pool.liquidity), tickSpacing: j.pool.tickSpacing,
      protocolFee: j.pool.protocolFee,
      bitmap: new Map(j.pool.bitmap.map(([w, v]: [number, string]) => [w, BigInt(v)])),
      liquidityNet: new Map(j.pool.liquidityNet.map(([t, v]: [number, string]) => [t, BigInt(v)])),
    },
    quoteIsCurrency1: j.quoteIsCurrency1, floorTick: j.floorTick, rates: j.rates,
    cushion: { cushionLower: j.cushion.cushionLower, cushionLiq: BigInt(j.cushion.cushionLiq) }, gapSkim: j.gapSkim,
  };
}

describe('simulateSwap against the V4Quoter (recorded on a fork and on live pools)', () => {
  it('has fixtures for a native quote and for an ERC-20 quote on each side of the pool', () => {
    const orientations = new Set(fixtures.flatMap((f) => f.cases.map((c) => `${BigInt(f.quote) === 0n ? 'native' : 'erc20'}:${c.snapshot.quoteIsCurrency1}`)));
    expect([...orientations].sort()).toEqual(['erc20:false', 'erc20:true', 'native:false']);
  });
  for (const f of fixtures) {
    for (const c of f.cases) {
      it(`${f.label}, ${c.state}: every amount equals the quoter's, and a swap the quoter refuses is refused`, () => {
        const snap = snapshotOf(c.snapshot);
        for (const e of c.expected) {
          if (e.amountOut === null) {
            expect(() => simulateSwap(snap, e.side, BigInt(e.amountIn)), `${e.side} ${e.amountIn}`).toThrow();
          } else {
            expect(simulateSwap(snap, e.side, BigInt(e.amountIn)).amountOut, `${e.side} ${e.amountIn}`).toBe(BigInt(e.amountOut));
          }
        }
      });
    }
  }
});

describe('v4 math', () => {
  it('getTickAtSqrtPrice inverts sqrtPriceAtTick, and a price one below a tick reads the tick under it', () => {
    for (const t of [MIN_TICK, -887000, -200000, -60, -1, 0, 1, 59, 60, 123456, 887000, MAX_TICK - 1]) {
      expect(getTickAtSqrtPrice(sqrtPriceAtTick(t))).toBe(t);
      if (t > MIN_TICK) expect(getTickAtSqrtPrice(sqrtPriceAtTick(t) - 1n)).toBe(t - 1);
    }
    expect(() => getTickAtSqrtPrice(MIN_SQRT - 1n)).toThrow();
    expect(() => getTickAtSqrtPrice(MAX_SQRT)).toThrow();
  });

  it('computeSwapStep (exact in) matches v4-core test vectors', () => {
    // v4-core test/libraries/SwapMath.t.sol: "exact amount in that gets capped at price target in one for zero".
    const price = 79228162514264337593543950336n; // 1:1
    const target = 79623317895830914510639640423n; // encodePriceSqrt(101, 100)
    const s = computeSwapStepExactIn(price, target, 2n * 10n ** 18n, 10n ** 18n, 600n);
    expect(s.amountIn).toBe(9975124224178055n);
    expect(s.feeAmount).toBe(5988667735148n);
    expect(s.amountOut).toBe(9925619580021728n);
    expect(s.sqrtNext).toBe(target);
    // "exact amount in that is fully spent in one for zero"
    const target2 = 250541448375047931186413801569n; // encodePriceSqrt(1000, 100)
    const s2 = computeSwapStepExactIn(price, target2, 2n * 10n ** 18n, 10n ** 18n, 600n);
    expect(s2.amountIn).toBe(999400000000000000n);
    expect(s2.feeAmount).toBe(600000000000000n);
    expect(s2.amountOut).toBe(666399946655997866n);
    expect(s2.sqrtNext < target2).toBe(true);
  });
});
