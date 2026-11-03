

import { array, big, bool, decimal, int, nullable, num, object, oneOf, optional, record, str, unknown } from "./decode.ts";
import type { Infer } from "./decode.ts";
import { FEES, MILESTONES } from "./bodies.ts";

const LIQUIDITY_MODE = oneOf("all", "bid");

export const DEX_ASSET = object({
  asset: object({
    id: str,
    name: str,
    symbol: str,

    decimals: int,
    totalSupply: optional(decimal),
    circulatingSupply: optional(decimal),

    coinGeckoId: optional(str),
  }),
});
export type DexAsset = Infer<typeof DEX_ASSET>;

export const TOKEN_HOLDERS = object({ total: int, holders: array(object({ address: str, balance: big })) });
export type TokenHolders = Infer<typeof TOKEN_HOLDERS>;

export const DEX_SUMMARY = object({
  pair: str,
  base: str,
  quote: str,
  priceNative: decimal,

  floorPriceNative: decimal,
  liquidityQuote: decimal,
  liquidityMode: LIQUIDITY_MODE,

  vault: decimal,

  bidMoney: decimal,
  floor: decimal,
  cushion: decimal,
  totalSupply: decimal,
  circulatingSupply: decimal,
  marketCapQuote: decimal,
  volumeQuote: decimal,
  trades: int,
  graduated: bool,
  unlisted: bool,
});
export type DexSummary = Infer<typeof DEX_SUMMARY>;

export const DEX_PAIR = object({
  pair: object({ id: str, asset0Id: str, asset1Id: str, createdAtBlockNumber: int, createdAtBlockTimestamp: int, feeBps: nullable(num) }),
});
export type DexPair = Infer<typeof DEX_PAIR>;

export const CG_PAIRS = array(object({ ticker_id: str, base: str, target: str, pool_id: str }));
export type CgPairs = Infer<typeof CG_PAIRS>;

export const CG_TICKERS = array(object({
  ticker_id: str,
  base_currency: str,
  target_currency: str,
  pool_id: str,
  last_price: decimal,
  ask: decimal,

  bid: decimal,
  base_volume: decimal,
  target_volume: decimal,
  high: nullable(decimal),
  low: nullable(decimal),
  liquidity_in_usd: nullable(decimal),
  liquidity_in_quote: big,

  depthUp2: nullable(decimal),
  depthDown2: nullable(decimal),
}));
export type CgTickers = Infer<typeof CG_TICKERS>;

export const CG_TRADE = object({ trade_id: int, price: decimal, base_volume: decimal, target_volume: decimal, trade_timestamp: int, type: oneOf("buy", "sell") });
export type CgTrade = Infer<typeof CG_TRADE>;

export const CG_HISTORICAL = object({ buy: optional(array(CG_TRADE)), sell: optional(array(CG_TRADE)) });
export type CgHistorical = Infer<typeof CG_HISTORICAL>;

export const LLAMA_TVL = object({ mode: LIQUIDITY_MODE, tvl: record(big), launchTokens: array(str) });
export type LlamaTvl = Infer<typeof LLAMA_TVL>;

const LLAMA_SUMS = {
  volume: big,
  fees: FEES,
  createFees: big,

  milestonesPaid: MILESTONES,
  protocolRevenue: big,
  holdersRevenue: big,

  traderRef: optional(big),
};

export const LLAMA_FEES = object({ ...LLAMA_SUMS, from: optional(int), to: optional(int), byQuote: record(object(LLAMA_SUMS)) });
export type LlamaFees = Infer<typeof LLAMA_FEES>;

export const HEALTH = object({
  ok: bool,
  head: nullable(int),
  launches: int,
  ready: bool,
  lag: nullable(int),
  stalled: nullable(object({ slot: int, signature: str, cycles: int, error: str, ok: bool })),

  metrics: optional(unknown),
});
export type Health = Infer<typeof HEALTH>;
