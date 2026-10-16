import { describe, expect, it } from 'vitest';
import { decimalize, decodeEventsBody, evmFees, grossLegs, isJoinExitShape, isSwapShape, mapEvents, priceFromSqrt, ratioString, solanaFees, type IxSwap, type PairInfo } from './dex-adapter.js';

describe('dex adapter mapping', () => {
  it('decimalize is exact, with no exponent and no trailing zeros', () => {
    expect(decimalize(1_500_000n, 6)).toBe('1.5');
    expect(decimalize(0n, 18)).toBe('0');
    expect(decimalize(10n ** 30n, 18)).toBe('1000000000000');
    expect(decimalize(-25n, 2)).toBe('-0.25');
    expect(() => decimalize(1n, -1)).toThrow();
  });

  it('ratioString truncates, carrying at least 18 significant digits, and refuses a non-positive ratio', () => {
    expect(ratioString(1n, 3n)).toBe('0.3333333333333333333');
    expect(ratioString(2n, 1n)).toBe('2');
    expect(ratioString(0n, 1n)).toBeNull();
    expect(ratioString(1n, 0n)).toBeNull();
  });

  it('priceFromSqrt reads quote per token in either orientation and both widths', () => {
    const one = 1n << 96n;
    expect(priceFromSqrt(one, true, 18, 18)).toBe('1');
    expect(priceFromSqrt(2n * one, true, 18, 18)).toBe('4');
    expect(priceFromSqrt(2n * one, false, 18, 18)).toBe('0.25');
    expect(priceFromSqrt(1n << 64n, true, 18, 6, 64)).toBe('1000000000000');
    expect(priceFromSqrt(0n, true, 18, 18)).toBeNull();
  });

  it('an EVM row\'s reserves are the ones the indexer serves (every standing position), null where unknown', () => {
    const body = (reserve0: unknown, reserve1: unknown) => ({
      swaps: [{ blockNumber: 1, logIndex: 0, txIndex: 0, txHash: '0x' + '01'.repeat(32), timestamp: 1, token: '0x' + '33'.repeat(20), side: 'buy',
        quoteAmount: '9398', tokenAmount: '1', quoteIsCurrency1: true, sqrtPriceX96: String(1n << 96n), trader: null,
        txFrom: '0x' + '33'.repeat(20), poolId: '0x' + 'ab'.repeat(32), liquidity: '1', fee: 2500, hookFee: '602', reserve0, reserve1 }],
      fills: [], liquidity: [],
    });
    expect(decodeEventsBody(body('11', '22')).swaps[0]!.reserves01).toEqual([11n, 22n]);
    expect(decodeEventsBody(body(null, null)).swaps[0]!.reserves01).toBeNull();
    expect(() => decodeEventsBody(body(undefined, undefined))).toThrow();
  });

  it('evmFees books the hook take on the quote leg: in on a buy, out on a sell, whatever LP fee the Swap logged', () => {
    expect(evmFees('buy', false, 7n)).toEqual({ in0: 7n, in1: 0n, out0: 0n, out1: 0n });
    expect(evmFees('sell', true, 7n)).toEqual({ in0: 0n, in1: 0n, out0: 0n, out1: 7n });
    const s = decodeEventsBody({
      swaps: [{ blockNumber: 1, logIndex: 0, txIndex: 0, txHash: '0x' + '01'.repeat(32), timestamp: 1, token: '0x' + '33'.repeat(20), side: 'buy',
        quoteAmount: '9398', tokenAmount: '1', quoteIsCurrency1: true, sqrtPriceX96: String(1n << 96n), trader: null,
        txFrom: '0x' + '33'.repeat(20), poolId: '0x' + 'ab'.repeat(32), liquidity: '1', fee: 2500, hookFee: '602', reserve0: null, reserve1: null }],
      fills: [], liquidity: [],
    }).swaps[0]!;
    expect(grossLegs(s, true)).toEqual([1n, 10_000n]);
  });

  it('a Solana trade\'s fees count the platform cut once (it is inside the skim), and a sell\'s output is the curve\'s', () => {
    const z = { skim: '0', creator: '0', holder: '0', platform: '0', team: '0' };
    const f = solanaFees({ zeroForOne: false, feeSplitKnown: true, feeSplit0: { skim: '10', creator: '5', holder: '0', platform: '2', team: '3' }, feeSplit1: z, lpFeeTotal: '7', protocolFeeTotal: '4', protocolFeeCurrency: 0 });
    expect(f).toEqual({ in0: 0n, in1: 7n, out0: 22n, out1: 0n });
    expect(grossLegs({ side: 'sell', tokenAmount: 5n, quoteAmount: 100n, fees01: f, legs: 'trader' }, false)).toEqual([5n, 122n]);
    expect(grossLegs({ side: 'buy', tokenAmount: 5n, quoteAmount: 100n, fees01: { in0: 0n, in1: 0n, out0: 9n, out1: 0n }, legs: 'trader' }, true)).toEqual([14n, 100n]);
  });

  it('mapEvents orients a pool buy to (token, quote) and leaves out a launch that is not shown', () => {
    const swap = (token: string): IxSwap => ({
      block: 10, txIndex: 1, logIndex: 2, txHash: '0xabc', timestamp: 1_700_000_000, token, side: 'buy',
      quoteAmount: 10n ** 18n, tokenAmount: 2n * 10n ** 18n, quoteIsCurrency1: false, sqrtPriceX96: 1n << 96n,
      trader: '0x1111111111111111111111111111111111111111', txFrom: '0x1111111111111111111111111111111111111111',
      pool: '0x' + 'ab'.repeat(32),
    } as unknown as IxSwap);
    const info: PairInfo = { saleId: '0x' + '22'.repeat(20), poolPairId: '0x' + 'ab'.repeat(32), tokenDecimals: 18, quoteDecimals: 18, saleTokens: 0n };
    const shown = '0x' + '33'.repeat(20);
    const { events, dropped } = mapEvents([swap(shown), swap('0x' + '44'.repeat(20))], [], (t) => (t === shown ? info : undefined));
    expect(dropped).toBe(0);
    expect(events.length).toBe(1);
    const e = events[0]! as any;
    expect(isSwapShape(e)).toBe(true);
    expect(isJoinExitShape(e)).toBe(false);
    expect(e.asset1In).toBe('1');
    expect(e.asset0Out).toBe('2');
  });
});
