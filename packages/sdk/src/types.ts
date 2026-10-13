import type { ChainRef } from './chain.js';

/**
 * A launch is named by its chain and its token address. ⛔ The address is kept VERBATIM: a Solana
 * mint is base58 and case-sensitive (a case-folded mint can decode to a different valid key), and an
 * EVM address is compared case-insensitively but never rewritten here.
 */
export interface LaunchRef {
  readonly chain: ChainRef;
  readonly token: string;
}

/** Every amount is a bigint in the token's or the quote asset's smallest unit. Never a float. */
export type Amount = bigint;

/** `pending`: created, but the sale is not open or funded yet (no quote). */
export type LaunchPhase = 'pending' | 'bonding' | 'graduated';

/** What every chain can say about a launch. Chain-specific detail lives on `raw`. */
export interface Launch<Raw = unknown> {
  readonly ref: LaunchRef;
  readonly name: string;
  readonly symbol: string;
  readonly decimals: number;
  readonly totalSupply: Amount;
  /** The asset a launch is bought with: native (ETH, SOL…) or a token. */
  readonly quote: { readonly address: string | null; readonly symbol: string; readonly decimals: number };
  readonly phase: LaunchPhase;
  /** The mode id (Custom 0, Moonshot 1, Cook 2, Bluechip 3, then presets appended). */
  readonly mode: number;
  /** `null` when the chain names no creator. */
  readonly creator: string | null;
  /** The published floor: WHOLE quote per WHOLE token, scaled by 1e18 (decimals already applied). `null` before one exists. */
  readonly floorPriceX18: Amount | null;
  readonly raw: Raw;
}

export type Side = 'buy' | 'sell';

/**
 * A quote. `amountIn` is what the trader pays; `amountOut` what they receive after every fee.
 * ⛔ `fees` lists the trader's whole take or nothing: a partial list is refused at construction.
 */
export interface Quote {
  readonly launch: LaunchRef;
  readonly side: Side;
  readonly amountIn: Amount;
  readonly amountOut: Amount;
  readonly fees: readonly FeePart[];
  /** `false` when a fee part is an estimate (e.g. read at the pre-trade price); the totals still match `amountOut`. */
  readonly feesExact: boolean;
  /** Quote asset handed back because the trade could not use all of `amountIn` (a sale that sells out). */
  readonly refund?: Amount;
  /** `true` when this trade sells out the bonding sale and graduates the launch inside the same transaction. */
  readonly graduates?: boolean;
  /**
   * The most NATIVE currency (lamports, wei) the trade may take from the trader ON TOP of `amountIn` — e.g. on Solana the
   * rent of the accounts a graduating buy creates, which the buy instruction bounds by this figure. The full wallet debit
   * is at most `amountIn` (when the quote asset is native) + `maxExtraNative` + network fees. Absent when the trade has
   * no such cost.
   */
  readonly maxExtraNative?: Amount;
}

export type FeePartKind =
  | 'lp'
  | 'floorSkim'
  | 'creator'
  | 'holder'
  | 'floorPlatform'
  /** Solana only: Carpenter's own protocol fee. */
  | 'carpenterPlatform';

export interface FeePart {
  readonly kind: FeePartKind;
  /** In the quote asset's smallest unit. */
  readonly amount: Amount;
}

/** One EVM call. */
export interface EvmCall {
  readonly to: string;
  readonly data: string;
  readonly value: Amount;
  /** A gas limit the builder knows a node's estimate can undershoot (e.g. a buy that graduates the sale). */
  readonly gas?: Amount;
}

/**
 * Everything one user action needs, in order, tagged by chain family. The caller's wallet signs and sends it.
 *
 * ONE WALLET PROMPT PER ACTION is the design on both chains:
 * - Solana: `transactions` are signed together with ONE `signAllTransactions`, then sent in order, each confirmed before
 *   the next. Signing them one by one costs one prompt each.
 * - EVM: `calls` (e.g. an approval, then the trade) go to the wallet as ONE EIP-5792 `wallet_sendCalls` batch where the
 *   wallet supports it (`sendEvmCalls` in `@floor/sdk-evm` does this and falls back to one transaction per call, one
 *   prompt each, where it does not). The SDK never asks for an off-chain signature (e.g. a Permit2 permit) when an
 *   on-chain call in the same batch does the job, so a batch is always one prompt.
 */
export type UnsignedTx =
  | { readonly family: 'evm'; readonly chainId: number; readonly calls: readonly EvmCall[] }
  /** One or more serialized (unsigned) Solana transactions, base64, in send order. */
  | { readonly family: 'solana'; readonly cluster: string; readonly transactions: readonly string[] };

export interface TradeParams {
  readonly launch: LaunchRef;
  readonly side: Side;
  /** Exact input: quote asset on a buy, tokens on a sell. */
  readonly amountIn: Amount;
  /** The least the trader accepts. Must be > 0. */
  readonly minAmountOut: Amount;
  readonly trader: string;
  /** Unix seconds after which the trade must revert, where the venue supports one. */
  readonly deadline?: bigint;
}
