import { chainKey, type ChainRef } from './chain.js';
import { APPROVED_QUOTE_TOKENS } from './quotes.gen.js';

export { APPROVED_QUOTE_TOKENS };

const ZERO_EVM = '0x0000000000000000000000000000000000000000';
const WSOL = 'So11111111111111111111111111111111111111112';

const sets = new Map<string, Map<string, string | null>>(
  Object.entries(APPROVED_QUOTE_TOKENS).map(([key, rows]) => [key, new Map(rows.map(([address, symbol]) => [address, symbol]))]),
);

const norm = (ref: ChainRef, address: string): string => (ref.family === 'evm' ? address.toLowerCase() : address);

/**
 * Whether FLOOR shows a launch quoted in `quote` on `chain`: the native coin (`null`, the zero address on EVM, wSOL on
 * Solana) always, a token when it is on the chain's list. A chain with no list approves no token. Launches quoted in another
 * FLOOR launch's token can also be shown by lineage, which this does not decide.
 */
export function isApprovedQuote(chain: ChainRef, quote: string | null): boolean {
  if (quote === null) return true;
  if (chain.family === 'evm' ? quote.toLowerCase() === ZERO_EVM : quote === WSOL) return true;
  return sets.get(chainKey(chain))?.has(norm(chain, quote)) ?? false;
}

/** The symbol FLOOR lists an approved quote token under, or `undefined` for a token not on the chain's list. */
export function approvedQuoteSymbol(chain: ChainRef, quote: string): string | null | undefined {
  return sets.get(chainKey(chain))?.get(norm(chain, quote));
}
