import { describe, expect, it } from 'vitest';
import { decimalize, evmFees, isJoinExitShape, isSwapShape, mapEvents, priceFromSqrt, ratioString, virtualReserves, type IxSwap, type PairInfo } from './dex-adapter.js';

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

  it('virtualReserves gives the in-range reserves of a liquidity at a price', () => {
    const r = virtualReserves(10n ** 18n, 1n << 96n);
    expect(r).toEqual([10n ** 18n, 10n ** 18n]);
  });

  it('evmFees books the hook take on the quote leg: in on a buy, out on a sell', () => {
    expect(evmFees('buy', false, 7n, 0)).toEqual({ in0: 7n, in1: 0n, out0: 0n, out1: 0n });
    expect(evmFees('sell', true, 7n, 0)).toEqual({ in0: 0n, in1: 0n, out0: 0n, out1: 7n });
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
