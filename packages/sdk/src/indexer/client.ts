

import { decode } from "./decode.ts";
import { INDEXER_CONTRACT, clampLimit, routeLimit } from "./contract.ts";
import type { RouteBody, RouteName } from "./contract.ts";
import { indexerPath, query } from "./paths.ts";
import type { CandlesQuery, LaunchesQuery, Page, StreamQuery, TradesQuery, WalletTradesQuery } from "./paths.ts";
import { IndexerError } from "./read.ts";
import type { Get } from "./read.ts";
import type { TraderWindow } from "./bodies.ts";

type L = { launch: string };
type W = { wallet: string };

export interface RouteArgs {
  head: void;
  latestBlock: void;
  stream: StreamQuery;
  quotes: void;
  summary: void;
  floorRouted: void;
  events: { fromBlock: number; toBlock: number };
  launches: LaunchesQuery | void;
  launch: L;
  launchState: L;
  milestones: L;
  holders: L & { limit?: number };
  trades: L & TradesQuery;
  launchClaims: L & { claimer?: string };
  devBuys: L;
  traders: L & { window?: TraderWindow; limit?: number };
  quoteChain: L;
  devTrades: L & { limit?: number };
  burns: L;
  launchLocks: L;
  marks: L & Page;
  price: L;
  highs: L & { atLeast?: bigint | string; limit?: number };
  candles: CandlesQuery;
  candleBundle: L & { intervals?: readonly string[]; limit?: number };
  walletTrades: W & WalletTradesQuery;
  positions: W;
  inbox: W & { limit?: number };
  walletLocks: W;
  claimHistory: W;
  firsts: W;
  firstsMany: { wallets: readonly string[] };
  claimView: W;
  people: W;
  roles: W;
  traderBoard: { window?: TraderWindow; limit?: number } | void;
  launcherBoard: { limit?: number } | void;
  dexAsset: { id: string };
  dexSummary: { id: string };
  dexPair: { id: string };
  cgPairs: void;
  cgTickers: void;
  cgHistoricalTrades: { ticker_id: string; type?: "buy" | "sell"; limit?: number; start_time?: number; end_time?: number };
  llamaTvl: void;
  llamaFees: { from: number; to: number } | void;
  health: void;
}

const need = (s: string, what: string): string => {
  if (s === "") throw new RangeError(`an empty ${what}`);
  return s;
};

export const routePath: { [N in RouteName]: (a: RouteArgs[N]) => string } = {
  head: () => indexerPath.head(),
  latestBlock: () => indexerPath.latestBlock(),
  stream: (a) => indexerPath.stream(a),
  quotes: () => indexerPath.quotes(),
  summary: () => indexerPath.summary(),
  floorRouted: () => indexerPath.floorRouted(),
  events: (a) => indexerPath.events(a.fromBlock, a.toBlock),
  launches: (a) => indexerPath.launches(a ?? {}),
  launch: (a) => indexerPath.launch(a.launch),
  launchState: (a) => indexerPath.launchState(a.launch),
  milestones: (a) => indexerPath.milestones(a.launch),
  holders: ({ launch, ...q }) => indexerPath.holders(launch, q),
  trades: ({ launch, ...q }) => indexerPath.trades(launch, q),
  launchClaims: ({ launch, ...q }) => indexerPath.launchClaims(launch, q),
  devBuys: (a) => indexerPath.devBuys(a.launch),
  traders: ({ launch, ...q }) => indexerPath.traders(launch, q),
  quoteChain: (a) => indexerPath.quoteChain(a.launch),
  devTrades: ({ launch, ...q }) => indexerPath.devTrades(launch, q),
  burns: (a) => indexerPath.burns(a.launch),
  launchLocks: (a) => indexerPath.launchLocks(a.launch),
  marks: ({ launch, ...q }) => indexerPath.marks(launch, q),
  price: (a) => indexerPath.price(a.launch),
  highs: ({ launch, ...q }) => indexerPath.highs(launch, q),
  candles: (a) => indexerPath.candles(a),
  candleBundle: ({ launch, ...q }) => indexerPath.candleBundle(launch, q),
  walletTrades: ({ wallet, ...q }) => indexerPath.walletTrades(wallet, q),
  positions: (a) => indexerPath.positions(a.wallet),
  inbox: ({ wallet, ...q }) => indexerPath.inbox(wallet, q),
  walletLocks: (a) => indexerPath.walletLocks(a.wallet),
  claimHistory: (a) => indexerPath.claimHistory(a.wallet),
  firsts: (a) => indexerPath.firsts(a.wallet),
  firstsMany: (a) => indexerPath.firstsMany(a.wallets),
  claimView: (a) => indexerPath.claimView(a.wallet),
  people: (a) => indexerPath.people(a.wallet),
  roles: (a) => indexerPath.roles(a.wallet),
  traderBoard: (a) => indexerPath.traderBoard(a ?? {}),
  launcherBoard: (a) => indexerPath.launcherBoard(a ?? {}),
  dexAsset: (a) => `/v2/dex/asset${query({ id: need(a.id, "id") })}`,
  dexSummary: (a) => `/v2/dex/summary${query({ id: need(a.id, "id") })}`,
  dexPair: (a) => `/v2/dex/pair${query({ id: need(a.id, "id") })}`,
  cgPairs: () => "/v2/coingecko/pairs",
  cgTickers: () => "/v2/coingecko/tickers",
  cgHistoricalTrades: (a) => `/v2/coingecko/historical_trades${query({ ticker_id: need(a.ticker_id, "ticker_id"), type: a.type, limit: a.limit, start_time: a.start_time, end_time: a.end_time })}`,
  llamaTvl: () => "/v2/defillama/tvl",
  llamaFees: (a) => {
    if (a && a.to < a.from) throw new RangeError("from > to");
    return `/v2/defillama/fees${query({ from: a?.from, to: a?.to })}`;
  },
  health: () => "/health",
};

export type DrainName = "trades" | "walletTrades" | "launches" | "marks";

export type ReadName = Exclude<RouteName, "stream">;

export interface IndexerClientOptions {

  get: Get;

  getRoot?: Get;
}

export type IndexerClient = {

  read<N extends ReadName>(name: N, ...args: RouteArgs[N] extends void ? [] : [RouteArgs[N]]): Promise<RouteBody<N>>;

  path<N extends RouteName>(name: N, ...args: RouteArgs[N] extends void ? [] : [RouteArgs[N]]): string;

  drain<N extends DrainName>(name: N, args: RouteArgs[N], opts?: { maxPages?: number }): AsyncGenerator<RouteBody<N>>;
} & { readonly [N in ReadName]: (...args: RouteArgs[N] extends void ? [] : [RouteArgs[N]]) => Promise<RouteBody<N>> };

export function createIndexerClient(opts: IndexerClientOptions): IndexerClient {

  const clamp = (name: RouteName, a: unknown): unknown => {
    if (typeof a !== "object" || a === null || !("limit" in a) || typeof (a as { limit: unknown }).limit !== "number") return a;
    return { ...a, limit: clampLimit(name, (a as { limit: number }).limit) };
  };
  const path = (name: RouteName, a?: unknown): string => (routePath[name] as (x: unknown) => string)(clamp(name, a));
  const read = async (name: ReadName, a?: unknown): Promise<unknown> => {
    const c = INDEXER_CONTRACT[name];
    const get = c.mount === "root" ? opts.getRoot : opts.get;
    if (get === undefined) throw new TypeError(`${name} is served at the server's root: pass getRoot`);
    const r = await get(path(name, a));
    if (r.status !== 200) throw new IndexerError(r.status, r.body);
    return decode(c.body as (v: unknown, p: string) => unknown, r.body);
  };
  async function* drain(name: DrainName, a: Record<string, unknown>, o: { maxPages?: number } = {}): AsyncGenerator<unknown> {
    const page = INDEXER_CONTRACT[name].page;
    const max = o.maxPages ?? 100;
    const lim = routeLimit(name)!;
    let args: Record<string, unknown> = { ...a };
    for (let i = 0; i < max; i++) {
      const body = (await read(name, args)) as { next?: string | null; total?: number };
      yield body;
      if (page.kind === "cursor") {
        if (body.next === null || body.next === undefined) return;
        args = { ...args, before: body.next };
      } else if (page.kind === "offset") {
        const step = clampLimit(name, (args.limit as number | undefined) ?? lim.default);
        const offset = ((args.offset as number | undefined) ?? 0) + step;
        if (offset >= (body.total ?? 0)) return;
        args = { ...args, offset };
      } else throw new TypeError(`${name} does not page`);
    }
  }
  const client: Record<string, unknown> = { read, path, drain };
  for (const name of Object.keys(INDEXER_CONTRACT) as RouteName[]) if (name !== "stream") client[name] = (a?: unknown) => read(name, a);
  return client as IndexerClient;
}
