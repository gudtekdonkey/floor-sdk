import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { encodeAbiParameters, encodeFunctionData, encodeFunctionResult, type Hex } from 'viem';
import { HOOK_ABI, POOL_ABI, POOL_STATE_COMPONENTS } from '../src/abi/index.js';
import { hookTakes, skimBpsAt } from '../src/fees.js';
import { bandTicks, LP_FEE_PIPS, marketTick, MAX_SQRT, MAX_TICK, MIN_SQRT, MIN_TICK, sqrtPriceAtTick } from '../src/pool.js';
import { computeSwapStepExactIn, getTickAtSqrtPrice, simulateSwap, simulateSwapLive, swapFeeParts, v4SwapExactIn, type SwapSnapshot } from '../src/simulate.js';

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

/** A client that answers `simulateSwapLive`'s reads off a fixture snapshot, counting the bitmap words it is asked for. */
function fixtureClient(j: any) {
  const snap = snapshotOf(j);
  const zero = (t: string): unknown => (t === 'address' ? '0x0000000000000000000000000000000000000000' : t === 'bool' ? false : /^u?int(8|16|24)$/.test(t) ? 0 : 0n);
  const st: Record<string, unknown> = {};
  for (const c of POOL_STATE_COMPONENTS) st[c.name] = 'components' in c ? Object.fromEntries(c.components.map((x) => [x.name, zero(x.type)])) : zero(c.type);
  const r = snap.rates;
  Object.assign(st, {
    tickSpacing: snap.pool.tickSpacing, floorTick: snap.floorTick, quoteIsCurrency1: snap.quoteIsCurrency1,
    skimBuyFarBps: BigInt(r.skimBuyFarBps), skimSellFarBps: BigInt(r.skimSellFarBps), skimBuyAtFloorBps: BigInt(r.skimBuyAtFloorBps), skimSellAtFloorBps: BigInt(r.skimSellAtFloorBps),
    creatorBuyBps: BigInt(r.creatorBuyBps), creatorSellBps: BigInt(r.creatorSellBps), holderBuyBps: BigInt(r.holderBuyBps), holderSellBps: BigInt(r.holderSellBps), teamFeeBps: BigInt(r.teamFeeBps),
  });
  const stairsOut = (POOL_ABI.find((f) => f.type === 'function' && f.name === 'stairs') as { outputs: readonly { components: readonly { name: string; type: string; components?: readonly { name: string; type: string }[] }[] }[] }).outputs[0]!;
  const stairs: Record<string, unknown> = {};
  for (const c of stairsOut.components) stairs[c.name] = c.type === 'tuple[16]' ? Array.from({ length: 16 }, () => Object.fromEntries(c.components!.map((x) => [x.name, zero(x.type)]))) : zero(c.type);
  Object.assign(stairs, { cushionLower: snap.cushion.cushionLower, cushionLiq: snap.cushion.cushionLiq });
  const words: number[] = [];
  const client = {
    async call({ data }: { data: Hex }) {
      if (data.slice(0, 10) === POOL_STATE_SELECTOR) return { data: encodeAbiParameters([{ type: 'tuple', components: POOL_STATE_COMPONENTS }] as never, [st] as never) };
      const inner = encodeFunctionResult({ abi: POOL_ABI, functionName: 'stairs', result: stairs } as never);
      return { data: encodeFunctionResult({ abi: HOOK_ABI, functionName: 'execute', result: inner } as never) };
    },
    async readContract({ functionName, args }: { functionName: string; args: readonly unknown[] }) {
      if (functionName === 'getSlot0') return [snap.pool.sqrtPriceX96, snap.pool.tick, snap.pool.protocolFee, 0];
      if (functionName === 'getLiquidity') return snap.pool.liquidity;
      if (functionName === 'getTickBitmap') { words.push(Number(args[1])); return snap.pool.bitmap.get(Number(args[1])) ?? 0n; }
      if (functionName === 'getTickLiquidity') return [0n, snap.pool.liquidityNet.get(Number(args[1])) ?? 0n];
      throw new Error(`no fake for ${functionName}`);
    },
  };
  return { client, snap, words };
}

const POOL_ID = ('0x' + '11'.repeat(32)) as Hex;
const POOL_STATE_SELECTOR = encodeFunctionData({ abi: HOOK_ABI, functionName: 'poolState', args: [POOL_ID] }).slice(0, 10);
const SRC = { hook: '0x0000000000000000000000000000000000000001', stateView: '0x0000000000000000000000000000000000000002' } as const;

describe('a pool swap\'s fee parts on the recorded pools', () => {
  it('a sell\'s hook takes are charged at the POST-swap tick, off the swap\'s whole quote leg; the pre-swap figure falls short', () => {
    let rampedPast = 0;
    for (const f of fixtures) for (const c of f.cases) {
      const snap = snapshotOf(c.snapshot);
      const dear = snap.floorTick - bandTicks(snap.pool.tickSpacing);
      for (const e of c.expected) {
        if (e.side !== 'sell' || e.amountOut === null) continue;
        const r = simulateSwap(snap, 'sell', BigInt(e.amountIn));
        const swap = v4SwapExactIn(snap.pool, snap.quoteIsCurrency1, BigInt(e.amountIn), LP_FEE_PIPS);
        const post = skimBpsAt('sell', dear - marketTick(swap.tick, snap.quoteIsCurrency1), snap.rates);
        const pre = skimBpsAt('sell', dear - marketTick(snap.pool.tick, snap.quoteIsCurrency1), snap.rates);
        expect(r.skimBps).toBe(post);
        if (post > pre) rampedPast++;
        const t = hookTakes('sell', swap.amountOut, post, snap.rates);
        const { fees, exact } = swapFeeParts(r);
        const by = Object.fromEntries(fees.map((x) => [x.kind, x.amount]));
        expect(by).toMatchObject({ floorSkim: t.floorSkim, creator: t.creator, holder: t.holder, floorPlatform: t.floorPlatform });
        expect(r.amountOut + t.total).toBe(swap.amountOut);
        expect(r.amountOut).toBe(BigInt(e.amountOut));
        // the LP fee is the token's, stated in quote at the walk's average price
        const consumed = BigInt(e.amountIn) - swap.lpFee;
        expect(by.lp).toBe(consumed > 0n ? (swap.lpFee * swap.amountOut) / consumed : 0n);
        expect(exact).toBe(swap.lpFee === 0n);
      }
    }
    expect(rampedPast).toBeGreaterThan(0);
  });

  it('a buy\'s five parts are the walk\'s own: the hook\'s four off the quote paid in, the LP fee in the quote', () => {
    for (const f of fixtures) for (const c of f.cases) {
      const snap = snapshotOf(c.snapshot);
      for (const e of c.expected) {
        if (e.side !== 'buy' || e.amountOut === null) continue;
        const r = simulateSwap(snap, 'buy', BigInt(e.amountIn));
        const { fees, exact } = swapFeeParts(r);
        expect(exact).toBe(snap.pool.protocolFee === 0);
        const total = fees.filter((x) => x.kind !== 'lp').reduce((a, x) => a + x.amount, 0n);
        const swap = v4SwapExactIn(snap.pool, !snap.quoteIsCurrency1, BigInt(e.amountIn) - total, LP_FEE_PIPS);
        expect(fees.find((x) => x.kind === 'lp')!.amount).toBe(swap.lpFee);
        expect(swap.amountOut).toBe(BigInt(e.amountOut));
      }
    }
  });

  for (const f of fixtures) {
    it(`${f.label}: simulateSwapLive reads only the words the walk crosses and answers what the whole map does`, async () => {
      for (const c of f.cases) {
        for (const e of c.expected) {
          const { client, snap, words } = fixtureClient(c.snapshot);
          const src = { ...SRC, gapSkim: snap.gapSkim };
          if (e.amountOut === null) {
            await expect(simulateSwapLive(client as never, src, POOL_ID, e.side, BigInt(e.amountIn))).rejects.toThrow();
            continue;
          }
          const live = await simulateSwapLive(client as never, src, POOL_ID, e.side, BigInt(e.amountIn));
          expect(live).toEqual(simulateSwap(snap, e.side, BigInt(e.amountIn)));
          expect(new Set(words).size).toBe(words.length);
        }
      }
    });
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
