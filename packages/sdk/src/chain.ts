/**
 * Chains. The same shapes and string keys the FLOOR site uses, so an id produced here is an id the
 * site understands.
 */
export type SolanaCluster = 'mainnet' | 'devnet' | 'testnet' | 'local';
export type EvmRef = { readonly family: 'evm'; readonly chainId: number };
export type SolanaRef = { readonly family: 'solana'; readonly cluster: SolanaCluster };
export type ChainRef = EvmRef | SolanaRef;
/** `eip155:<chainId>` or `solana:<cluster>` (CAIP-2 style). */
export type ChainKey = string;

const EVM_KEY = 'eip155:';
const SOL_KEY = 'solana:';
const CLUSTERS: readonly SolanaCluster[] = ['mainnet', 'devnet', 'testnet', 'local'];

export const evmRef = (chainId: number): EvmRef => ({ family: 'evm', chainId });
export const solanaRef = (cluster: SolanaCluster): SolanaRef => ({ family: 'solana', cluster });

export function chainKey(ref: ChainRef): ChainKey {
  return ref.family === 'evm' ? `${EVM_KEY}${ref.chainId}` : `${SOL_KEY}${ref.cluster}`;
}

/** A key → its ref, or `null` for anything malformed. Never coerces. */
export function parseChainKey(key: string): ChainRef | null {
  if (key.startsWith(EVM_KEY)) {
    const raw = key.slice(EVM_KEY.length);
    const id = Number(raw);
    return /^[0-9]+$/.test(raw) && Number.isSafeInteger(id) && id > 0 ? evmRef(id) : null;
  }
  if (key.startsWith(SOL_KEY)) {
    const c = key.slice(SOL_KEY.length) as SolanaCluster;
    return CLUSTERS.includes(c) ? solanaRef(c) : null;
  }
  return null;
}

export function sameRef(a: ChainRef, b: ChainRef): boolean {
  return chainKey(a) === chainKey(b);
}
