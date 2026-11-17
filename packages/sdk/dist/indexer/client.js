

import { decode } from "./decode.js";
import { INDEXER_CONTRACT, clampLimit, pageAsked, routeLimit } from "./contract.js";

import { indexerPath, query } from "./paths.js";

import { IndexerError } from "./read.js";

const need = (s        , what        )         => {
  if (s === "") throw new RangeError(`an empty ${what}`);
  return s;
};

export const routePath                                                    = {
  head: () => indexerPath.head(),
  latestBlock: () => indexerPath.latestBlock(),
  stream: (a) => indexerPath.stream(a),
  quotes: () => indexerPath.quotes(),
  summary: () => indexerPath.summary(),
  floorRouted: (a) => indexerPath.floorRouted(a ?? {}),
  events: (a) => indexerPath.events(a.fromBlock, a.toBlock),
  launches: (a) => indexerPath.launches(a ?? {}),
  launchLists: (a) => indexerPath.launchLists(a),
  search: (a) => indexerPath.search(a),
  launch: (a) => indexerPath.launch(a.launch),
  launchState: (a) => indexerPath.launchState(a.launch),
  milestones: (a) => indexerPath.milestones(a.launch),
  holders: ({ launch, ...q }) => indexerPath.holders(launch, q),
  trades: ({ launch, ...q }) => indexerPath.trades(launch, q),
  launchClaims: ({ launch, ...q }) => indexerPath.launchClaims(launch, q),
  devBuys: ({ launch, ...q }) => indexerPath.devBuys(launch, q),
  memberSold: ({ launch, ...q }) => indexerPath.memberSold(launch, q),
  launchAccount: (a) => indexerPath.launchAccount(a.launch),
  traders: ({ launch, ...q }) => indexerPath.traders(launch, q),
  quoteChain: (a) => indexerPath.quoteChain(a.launch),
  devTrades: ({ launch, ...q }) => indexerPath.devTrades(launch, q),
  burns: ({ launch, ...q }) => indexerPath.burns(launch, q),
  launchLocks: (a) => indexerPath.launchLocks(a.launch),
  marks: ({ launch, ...q }) => indexerPath.marks(launch, q),
  price: (a) => indexerPath.price(a.launch),
  highs: ({ launch, ...q }) => indexerPath.highs(launch, q),
  candles: (a) => indexerPath.candles(a),
  candlesMany: (a) => indexerPath.candlesMany(a),
  candleBundle: ({ launch, ...q }) => indexerPath.candleBundle(launch, q),
  walletTrades: ({ wallet, ...q }) => indexerPath.walletTrades(wallet, q),
  tonActivity: ({ wallet, ...q }) => indexerPath.tonActivity(wallet, q),
  tonOutcome: (a) => indexerPath.tonOutcome(a.wallet, a.queryId),
  positions: ({ wallet, ...q }) => indexerPath.positions(wallet, q),
  inbox: ({ wallet, ...q }) => indexerPath.inbox(wallet, q),
  walletLocks: ({ wallet, ...q }) => indexerPath.walletLocks(wallet, q),
  claimHistory: ({ wallet, ...q }) => indexerPath.claimHistory(wallet, q),
  firsts: (a) => indexerPath.firsts(a.wallet),
  firstsMany: (a) => indexerPath.firstsMany(a.wallets),
  claimView: ({ wallet, ...q }) => indexerPath.claimView(wallet, q),
  people: ({ wallet, ...q }) => indexerPath.people(wallet, q),
  roles: ({ wallet, ...q }) => indexerPath.roles(wallet, q),
  handlePlaces: ({ handle, ...q }) => indexerPath.handlePlaces(handle, q),
  traderBoard: (a) => indexerPath.traderBoard(a ?? {}),
  launcherBoard: (a) => indexerPath.launcherBoard(a ?? {}),
  tokenHolders: (a) => `/floor-amm/holders/${encodeURIComponent(need(a.token, "token"))}${query({ limit: a.limit, offset: a.offset })}`,
  dexAsset: (a) => `/v2/dex/asset${query({ id: need(a.id, "id") })}`,
  dexSummary: (a) => `/v2/dex/summary${query({ id: need(a.id, "id") })}`,
  dexPair: (a) => `/v2/dex/pair${query({ id: need(a.id, "id") })}`,
  cgPairs: () => "/v2/coingecko/pairs",
  cgTickers: () => "/v2/coingecko/tickers",
  cgHistoricalTrades: (a) => `/v2/coingecko/historical_trades${query({ ticker_id: need(a.ticker_id, "ticker_id"), type: a.type, limit: a.limit, before: a.before, start_time: a.start_time, end_time: a.end_time })}`,
  llamaTvl: () => "/v2/defillama/tvl",
  llamaFees: (a) => {
    if (a && a.to < a.from) throw new RangeError("from > to");
    return `/v2/defillama/fees${query({ from: a?.from, to: a?.to })}`;
  },
  health: () => "/health",
};

export const DRAIN_NAMES = ["trades", "launchClaims", "memberSold", "burns", "walletTrades", "launches", "search", "marks", "inbox", "positions", "walletLocks", "claimHistory", "people", "roles", "tokenHolders", "tonActivity", "cgHistoricalTrades", "floorRouted", "devBuys", "claimView", "handlePlaces"]         ;

export function createIndexerClient(opts                      )                {

  const clamp = (name           , a         )          => {
    if (typeof a !== "object" || a === null || !("limit" in a) || typeof (a                      ).limit !== "number") return a;
    return { ...a, limit: clampLimit(name, (a                     ).limit, pageAsked(name, a                           )) };
  };
  const path = (name           , a          )         => (routePath[name]                          )(clamp(name, a));
  const read = async (name          , a          )                   => {
    const c = INDEXER_CONTRACT[name];
    const get = c.mount === "root" ? opts.getRoot : opts.get;
    if (get === undefined) throw new TypeError(`${name} is served at the server's root: pass getRoot`);
    const r = await get(path(name, a));
    if (r.status !== 200) throw new IndexerError(r.status, r.body);
    return decode(c.body                                      , r.body);
  };
  async function* drain(name           , a                         , o                        = {})                          {
    const page = INDEXER_CONTRACT[name].page;
    const max = o.maxPages ?? 100;
    const lim = routeLimit(name) ;
    let args                          = { ...a };

    if (page.kind === "cursor" && page.optIn !== undefined && args.before === undefined) args = { ...args, before: "" };
    if (page.kind === "keyset" && page.optIn === true && args.limit === undefined && args.cursor === undefined) args = { ...args, limit: lim.default };
    for (let i = 0; i < max; i++) {
      const body = (await read(name, args))                                            ;
      yield body;
      if (page.kind === "cursor") {
        if (body.next === null || body.next === undefined) return;
        args = { ...args, before: body.next };
      } else if (page.kind === "keyset") {

        if (args.ids !== undefined) return;
        if (args.offset !== undefined) {
          const step = clampLimit(name, (args.limit                      ) ?? lim.default);
          const offset = (args.offset          ) + step;
          if (offset >= (body.total ?? 0)) return;
          args = { ...args, offset };
          continue;
        }
        if (body.next === null || body.next === undefined) return;
        args = { ...args, cursor: body.next };
      } else if (page.kind === "offset") {
        const step = clampLimit(name, (args.limit                      ) ?? lim.default);
        const offset = ((args.offset                      ) ?? 0) + step;
        if (offset >= (body.total ?? 0)) return;
        args = { ...args, offset };
      } else throw new TypeError(`${name} does not page`);
    }
  }
  const client                          = { read, path, drain };
  for (const name of Object.keys(INDEXER_CONTRACT)               ) if (name !== "stream") client[name] = (a          ) => read(name, a);
  return client                 ;
}
