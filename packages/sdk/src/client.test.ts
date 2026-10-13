import { describe, expect, it } from 'vitest';
import { FloorClient, FloorSdkError, chainKey, evmRef, parseChainKey, solanaRef, type FloorChainAdapter } from './index.js';

const stub = (chains: FloorChainAdapter['chains']): FloorChainAdapter => ({
  chains,
  getLaunch: async () => { throw new Error('unused'); },
  listLaunches: async () => ({ launches: [], cursor: null }),
  quote: async () => { throw new Error('unused'); },
  buildTrade: async () => { throw new Error('unused'); },
});

describe('chain keys', () => {
  it('round-trips and refuses malformed keys', () => {
    expect(chainKey(evmRef(11155111))).toBe('eip155:11155111');
    expect(parseChainKey('solana:devnet')).toEqual(solanaRef('devnet'));
    for (const bad of ['eip155:', 'eip155:1e3', 'eip155:0', 'solana:moon', 'cosmos:1']) expect(parseChainKey(bad)).toBeNull();
  });
});

describe('FloorClient', () => {
  it('routes by chain and refuses an unserved chain by name', async () => {
    const c = new FloorClient([stub([solanaRef('devnet')])]);
    await expect(c.listLaunches(solanaRef('devnet'))).resolves.toEqual({ launches: [], cursor: null });
    expect(() => c.adapterFor(evmRef(1))).toThrow(/eip155:1/);
  });
  it('refuses two adapters for one chain, and non-positive amounts', () => {
    expect(() => new FloorClient([stub([evmRef(1)]), stub([evmRef(1)])])).toThrow(FloorSdkError);
    const c = new FloorClient([stub([evmRef(1)])]);
    expect(() => c.buildTrade({ launch: { chain: evmRef(1), token: '0x' }, side: 'buy', amountIn: 1n, minAmountOut: 0n, trader: '0x' })).toThrow(/minAmountOut/);
  });
});
