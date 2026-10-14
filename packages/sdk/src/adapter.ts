import type { ChainRef } from './chain.js';
import type { Launch, LaunchRef, Quote, Side, TradeParams, UnsignedTx, Amount } from './types.js';

/**
 * What a chain package implements. `@floorlaunchpad/sdk-evm` and `@floorlaunchpad/sdk-solana` each export one factory
 * that returns this. Reads are network calls; `build*` returns an unsigned transaction and never signs.
 */
export interface FloorChainAdapter {
  /** The chains this adapter instance serves. */
  readonly chains: readonly ChainRef[];
  getLaunch(ref: LaunchRef): Promise<Launch>;
  /** Paged, newest first. `cursor` is opaque and adapter-specific. */
  listLaunches(chain: ChainRef, opts?: { readonly limit?: number; readonly cursor?: string }): Promise<{ readonly launches: readonly Launch[]; readonly cursor: string | null }>;
  /**
   * `trader` is used where a venue checks the seller's own state: a Solana sell-back before graduation pays the same
   * fixed-price amount to any seller, and `trader` is read only to check that their sale position covers `amountIn`.
   */
  quote(ref: LaunchRef, side: Side, amountIn: Amount, opts?: { readonly trader?: string }): Promise<Quote>;
  buildTrade(params: TradeParams): Promise<UnsignedTx>;
}
