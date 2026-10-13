import { describe, expect, it } from 'vitest';
import { evmRef, solanaRef } from './chain.js';
import { APPROVED_QUOTE_TOKENS, approvedQuoteSymbol, isApprovedQuote } from './quotes.js';

describe('approved quotes', () => {
  it('lists the six live mainnets', () => {
    expect(Object.keys(APPROVED_QUOTE_TOKENS).sort()).toEqual(['eip155:1', 'eip155:4663', 'eip155:5042', 'eip155:56', 'eip155:8453', 'solana:mainnet']);
  });
  it('approves the native coin everywhere and a listed token in any case on EVM', () => {
    expect(isApprovedQuote(evmRef(8453), null)).toBe(true);
    expect(isApprovedQuote(evmRef(8453), '0x0000000000000000000000000000000000000000')).toBe(true);
    expect(isApprovedQuote(solanaRef('mainnet'), 'So11111111111111111111111111111111111111112')).toBe(true);
    expect(isApprovedQuote(evmRef(8453), '0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913')).toBe(true);
    expect(approvedQuoteSymbol(evmRef(8453), '0x833589fcd6edb6e08f4c7c32d4f71b54bda02913')).toBe('USDC');
  });
  it('refuses an unlisted token, a chain with no list, and a Solana mint in the wrong case', () => {
    expect(isApprovedQuote(evmRef(8453), '0x1111111111111111111111111111111111111111')).toBe(false);
    expect(isApprovedQuote(evmRef(11155111), '0x833589fcd6edb6e08f4c7c32d4f71b54bda02913')).toBe(false);
    expect(isApprovedQuote(solanaRef('mainnet'), 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v')).toBe(true);
    expect(isApprovedQuote(solanaRef('mainnet'), 'epjfwdd5aufqssqem2qn1xzybapc8g4wegGkZwyTDt1v')).toBe(false);
  });
});
