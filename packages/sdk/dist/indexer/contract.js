

import { unknown } from "./decode.js";
import * as B from "./bodies.js";
import * as P from "./partners.js";
import { CANDLES_MANY_IDS_MAX, CANDLES_MANY_LIMIT_DEFAULT, CANDLES_MANY_ROWS_MAX, FIRSTS_MAX, IDS_MAX, LAUNCH_LISTS_QUOTES_MAX, LAUNCH_LISTS_ROWS_MAX, INBOX_PAGE_DEFAULT, INBOX_PAGE_MAX, LAUNCHES_PAGE_DEFAULT, LAUNCHES_PAGE_MAX, LAUNCH_CURSOR_MAX, LAUNCH_SORTS, PRESET_FILTER_MAX, STREAM_STATE_MAX, WALLET_CURSOR_MAX, WALLET_PAGE_DEFAULT, WALLET_PAGE_MAX } from "./paths.js";
import { SEARCH_PAGE_DEFAULT, SEARCH_PAGE_MAX, SEARCH_Q_MAX } from "./search.js";

export const UNITS = {
  site: "raw quote base units, token math units (6 dp), Q64.64 prices, unix s; bigint as decimal strings; USD as decimal strings",
  partner: "the partner's format: whole-unit decimal strings unless a field says raw base units; unix s",
}         ;

export const INDEXER_ERROR_CODES = [

  "not_ready",

  "unknown_route",

  "unknown_launch",

  "not_found",

  "bad_query",

  "too_many",

  "no_reader",

  "not_indexed",

  "upstream",

  "method",

  "unknown",
]         ;

const errorWord = (body         )                => (typeof body === "object" && body !== null && "error" in body ? String((body                      ).error) : null);

export function errorCode(status        , body         )                   {
  const w = errorWord(body);
  switch (status) {
    case 400:
      return "bad_query";
    case 404:
      return w === "unknown route" ? "unknown_route" : w === "unknown launch" ? "unknown_launch" : "not_found";
    case 405:
      return "method";
    case 413:
      return "too_many";
    case 501:
      return "no_reader";
    case 502:
      return "upstream";
    case 503:
      return w === "not ready" ? "not_ready" : "not_indexed";
    default:
      return "unknown";
  }
}

const NONE = { kind: "none" }         ;
const limit = (dflt        , max        ) => ({ kind: "limit", limit: { default: dflt, max } })         ;
const cursor = (dflt        , max        ) => ({ kind: "cursor", limit: { default: dflt, max }, cursor: "before", next: "next" })         ;
const LIMIT = (dflt        , max        )           => ({ kind: "int", doc: "rows", min: 1, max, default: dflt });
const BEFORE           = { kind: "cursor", doc: "a block, or a row id <block>:<txnIndex>:<eventIndex> (a page's next): rows strictly before it" };
const keyset = (dflt        , max        ) => ({ kind: "keyset", limit: { default: dflt, max }, cursor: "cursor", next: "next" })         ;
const PRESETS           = { kind: "list", doc: "only launches of these presets (ids 0..255)", max: PRESET_FILTER_MAX, item: { kind: "int", min: 0, max: 255 } }         ;

const WALLET_PAGE = { kind: "keyset", limit: { default: WALLET_PAGE_DEFAULT, max: WALLET_PAGE_MAX }, cursor: "cursor", next: "next", optIn: true }         ;
const WALLET_PAGE_KEYS = (cursorDoc        )                           => ({
  limit: { kind: "int", doc: "rows (asks a page)", min: 1, max: WALLET_PAGE_MAX, default: WALLET_PAGE_DEFAULT },
  cursor: { kind: "string", doc: `a page's next (asks a page): ${cursorDoc}`, max: WALLET_CURSOR_MAX },
});
const KEYSET_CURSOR = (without                   )           => ({ kind: "string", doc: "a page's next: the rows after its last one (opaque)", max: LAUNCH_CURSOR_MAX, ...(without.length > 0 ? { without } : {}) });
const WINDOW           = { kind: "enum", doc: "the volume window", values: B.TRADER_WINDOWS, default: "24h" };

const GATED = ["not_ready", "upstream"]         ;
const LAUNCH_ERR = [...GATED, "unknown_launch"]         ;
const WALLET_ERR = [...GATED, "bad_query"]         ;

function site   (s                            )                   {
  const params = [...s.pattern.matchAll(/<(launch|wallet|token|handle)>/g)].map((m) => m[1]             );
  const { v2, ...rest } = s;
  const aliases = s.aliases ?? (v2 ? [s.pattern.replace(/^\/floor-amm\//, "/floor-amm/v2/")] : []);
  return { mount: "chain", query: {}, page: NONE, errors: GATED, gated: true, units: "site", audience: "site", ...rest, params, aliases };
}
function partner   (s         )                   {
  return { mount: "chain", params: [], query: {}, page: NONE, errors: [...GATED, "bad_query"], gated: true, units: "partner", audience: "partner", aliases: [], ...s };
}

export const INDEXER_CONTRACT = {

  head: site({ pattern: "/floor-amm/head", aliases: ["/head"], gated: false, errors: [], body: B.HEAD }),
  latestBlock: site({ pattern: "/floor-amm/latest-block", aliases: ["/latest-block"], errors: [...GATED, "not_found"], body: B.LATEST_BLOCK }),
  stream: site({
    pattern: "/floor-amm/stream",
    aliases: ["/stream"],
    sse: true,
    query: {
      launch: { kind: "list", doc: "only these launches' frames (and their quote chains' relatives)" },
      state: { kind: "list", doc: "state frames (the launch whole on each change) for these", max: STREAM_STATE_MAX },
      lastEventId: { kind: "string", doc: "resume after this frame id (EventSource sends Last-Event-ID itself)" },
    },
    body: unknown,
  }),
  quotes: site({ pattern: "/floor-amm/quotes", body: B.QUOTES }),
  summary: site({ pattern: "/floor-amm/summary", aliases: ["/floor-amm/v2/summary"], body: B.SUMMARY }),
  floorRouted: site({
    pattern: "/floor-amm/floor-routed",
    aliases: ["/floor-amm/v2/floor-routed"],
    query: {
      limit: LIMIT(LAUNCHES_PAGE_DEFAULT, LAUNCHES_PAGE_MAX),
      cursor: { kind: "string", doc: "a page's next: the launches after this launch id, in create order", max: LAUNCH_CURSOR_MAX },
    },
    page: keyset(LAUNCHES_PAGE_DEFAULT, LAUNCHES_PAGE_MAX),
    body: B.FLOOR_ROUTED,
  }),
  events: site({
    pattern: "/floor-amm/events",
    aliases: ["/events", "/v2/dex/events"],
    query: { fromBlock: { kind: "block", doc: "first block, inclusive", required: true }, toBlock: { kind: "block", doc: "last block, inclusive", required: true } },
    page: { kind: "range", from: "fromBlock", to: "toBlock" },
    errors: [...GATED, "bad_query", "too_many"],
    body: B.EVENTS,
  }),

  launches: site({
    pattern: "/floor-amm/launches",
    v2: true,
    query: {
      sort: { kind: "enum", doc: "the order (larger first with order=desc)", values: LAUNCH_SORTS, default: "created" },
      order: { kind: "enum", doc: "asc or desc", values: ["asc", "desc"], default: "desc" },
      q: { kind: "string", doc: "name or symbol contains, or an id, token or creator equals", max: SEARCH_Q_MAX },
      ids: { kind: "list", doc: "these launches (ids or tokens), in the order asked without sort", max: IDS_MAX },
      quote: { kind: "string", doc: "launches quoted in this coin" },
      creator: { kind: "string", doc: "launches by this creator" },
      rankable: { kind: "bool", doc: "true (or 1) listed only, false (or 0) unlisted only" },
      graduated: { kind: "bool", doc: "graduated or not (true / false, or 1 / 0)" },
      preset: PRESETS,
      limit: LIMIT(LAUNCHES_PAGE_DEFAULT, LAUNCHES_PAGE_MAX),
      cursor: KEYSET_CURSOR(["offset", "ids"]),
      offset: { kind: "int", doc: "rows skipped (with total: an offset page)", min: 0, default: 0 },
    },

    page: keyset(LAUNCHES_PAGE_DEFAULT, LAUNCHES_PAGE_MAX),
    errors: [...GATED, "bad_query"],
    body: B.LAUNCHES,
  }),

  launchLists: site({
    pattern: "/floor-amm/launch-lists",
    v2: true,
    query: {
      quotes: { kind: "list", doc: "one list per quote coin, in this order (no repeat)", required: true, min: 1, max: LAUNCH_LISTS_QUOTES_MAX },
      cursors: { kind: "list", doc: "parallel to quotes, one each: that list's page next (- = its first page)", max: LAUNCH_LISTS_QUOTES_MAX },
      sort: { kind: "enum", doc: "the order (larger first with order=desc)", values: LAUNCH_SORTS, default: "created" },
      order: { kind: "enum", doc: "asc or desc", values: ["asc", "desc"], default: "desc" },
      creator: { kind: "string", doc: "launches by this creator" },
      rankable: { kind: "bool", doc: "true (or 1) listed only, false (or 0) unlisted only" },
      graduated: { kind: "bool", doc: "graduated or not (true / false, or 1 / 0)" },
      preset: PRESETS,
      limit: LIMIT(LAUNCHES_PAGE_DEFAULT, LAUNCHES_PAGE_MAX),
    },
    page: limit(LAUNCHES_PAGE_DEFAULT, LAUNCHES_PAGE_MAX),
    errors: [...GATED, "bad_query"],
    body: B.LAUNCH_LISTS,
  }),
  search: site({
    pattern: "/floor-amm/search",
    v2: true,
    query: {
      q: { kind: "string", doc: "the search: symbol, name or token (tiers 0-4, search.ts)", required: true, max: SEARCH_Q_MAX },
      limit: LIMIT(SEARCH_PAGE_DEFAULT, SEARCH_PAGE_MAX),
      cursor: KEYSET_CURSOR([]),
      rankable: { kind: "bool", doc: "true (or 1) listed only, false (or 0) unlisted only" },
      graduated: { kind: "bool", doc: "graduated or not (true / false, or 1 / 0)" },
      quote: { kind: "string", doc: "launches quoted in this coin" },
      preset: PRESETS,
    },
    page: keyset(SEARCH_PAGE_DEFAULT, SEARCH_PAGE_MAX),
    errors: [...GATED, "bad_query"],
    body: B.LAUNCH_SEARCH,
  }),
  launch: site({ pattern: "/floor-amm/launches/<launch>", v2: true, errors: LAUNCH_ERR, body: B.LAUNCH_ONE }),
  launchState: site({ pattern: "/floor-amm/launches/<launch>/state", v2: true, errors: ["not_ready", "unknown_launch"], body: B.LAUNCH_STATE }),
  milestones: site({ pattern: "/floor-amm/launches/<launch>/milestones", v2: true, errors: LAUNCH_ERR, body: B.LAUNCH_MILESTONES }),
  holders: site({ pattern: "/floor-amm/launches/<launch>/holders", v2: true, query: { limit: LIMIT(100, 500) }, page: limit(100, 500), errors: [...LAUNCH_ERR, "bad_query"], body: B.HOLDERS }),
  trades: site({
    pattern: "/floor-amm/launches/<launch>/trades",
    v2: true,
    query: { limit: LIMIT(100, 500), before: BEFORE, traders: { kind: "list", doc: "only these wallets' trades (EVM case-blind)" } },
    page: cursor(100, 500),
    errors: [...LAUNCH_ERR, "bad_query"],
    body: B.TRADES,
  }),
  launchClaims: site({ pattern: "/floor-amm/launches/<launch>/claims", v2: true, query: { limit: LIMIT(100, 500), before: BEFORE, claimer: { kind: "string", doc: "only this claimer's rows" } }, page: cursor(100, 500), errors: [...LAUNCH_ERR, "bad_query"], body: B.LAUNCH_CLAIMS }),
  devBuys: site({ pattern: "/floor-amm/launches/<launch>/dev-buys", v2: true, query: { limit: LIMIT(100, 500), before: BEFORE }, page: cursor(100, 500), errors: [...LAUNCH_ERR, "bad_query"], body: B.DEV_BUYS }),

  memberSold: site({ pattern: "/floor-amm/launches/<launch>/member-sold", v2: true, query: { limit: LIMIT(100, 500), before: BEFORE }, page: cursor(100, 500), errors: [...LAUNCH_ERR, "bad_query"], body: B.MEMBER_SOLD }),

  launchAccount: site({ pattern: "/floor-amm/launches/<launch>/account", v2: true, errors: LAUNCH_ERR, body: B.LAUNCH_ACCOUNT }),
  traders: site({ pattern: "/floor-amm/launches/<launch>/traders", v2: true, query: { window: WINDOW, limit: LIMIT(100, 500) }, page: limit(100, 500), errors: [...LAUNCH_ERR, "bad_query"], body: B.LAUNCH_TRADERS }),
  quoteChain: site({ pattern: "/floor-amm/launches/<launch>/chain", v2: true, errors: LAUNCH_ERR, body: B.QUOTE_CHAIN }),
  devTrades: site({ pattern: "/floor-amm/launches/<launch>/dev", v2: true, query: { limit: LIMIT(100, 500) }, page: limit(100, 500), errors: [...LAUNCH_ERR, "bad_query"], body: B.DEV_TRADES }),
  burns: site({ pattern: "/floor-amm/launches/<launch>/burns", v2: true, query: { limit: LIMIT(100, 500), before: BEFORE }, page: cursor(100, 500), errors: [...LAUNCH_ERR, "bad_query"], body: B.BURNS }),
  launchLocks: site({ pattern: "/floor-amm/launches/<launch>/locks", v2: true, errors: LAUNCH_ERR, body: B.LAUNCH_LOCKS }),
  marks: site({
    pattern: "/floor-amm/launches/<launch>/marks",
    v2: true,
    query: { limit: LIMIT(100, 1_000), before: BEFORE },
    page: cursor(100, 1_000),
    errors: [...LAUNCH_ERR, "bad_query"],
    body: B.MARKS,
  }),
  price: site({ pattern: "/floor-amm/launches/<launch>/price", v2: true, errors: LAUNCH_ERR, body: B.PRICE }),
  highs: site({
    pattern: "/floor-amm/launches/<launch>/highs",
    v2: true,
    query: { atLeast: { kind: "int", doc: "only the highs whose market cap reaches this (quote base units)", min: 0 }, limit: LIMIT(100, 1_000) },
    page: limit(100, 1_000),
    errors: [...LAUNCH_ERR, "bad_query"],
    body: B.HIGHS,
  }),
  candles: site({
    pattern: "/floor-amm/candles",
    aliases: ["/candles", "/floor-amm/v2/candles", "/floor-amm/v2/launches/<launch>/candles"],
    query: {
      launch: { kind: "string", doc: "a launch id or its token", required: true },
      interval: { kind: "enum", doc: "the bucket", values: B.CANDLE_INTERVALS, default: "1h" },
      from: { kind: "unix", doc: "range start (absent: the most candles one read spans, back from to)" },
      to: { kind: "unix", doc: "range end (absent: the indexed head's time)" },
      before: { kind: "unix", doc: "instead of from / to: the newest limit traded buckets starting before it" },
      limit: { kind: "int", doc: "with before only", min: 1, max: B.CANDLES_MAX, default: B.CANDLES_MAX },
    },
    page: { kind: "range", from: "from", to: "to", span: { interval: "interval", max: B.CANDLES_MAX } },
    errors: [...GATED, "bad_query", "unknown_launch", "not_found"],
    body: B.CANDLES,
  }),

  candlesMany: site({
    pattern: "/floor-amm/candles/many",
    aliases: ["/floor-amm/v2/candles/many"],
    query: {
      ids: { kind: "list", doc: "these launches (ids or tokens), one series each in the order asked", required: true, min: 1, max: CANDLES_MANY_IDS_MAX },
      interval: { kind: "enum", doc: "the bucket", values: B.CANDLE_INTERVALS, default: "1h" },
      before: { kind: "unix", doc: "the newest limit traded buckets starting before it (absent: past the indexed head)" },
      limit: { kind: "int", doc: "buckets per launch", min: 1, max: B.CANDLES_MAX, default: CANDLES_MANY_LIMIT_DEFAULT },
    },
    page: limit(CANDLES_MANY_LIMIT_DEFAULT, B.CANDLES_MAX),
    errors: [...GATED, "bad_query"],
    body: B.CANDLES_MANY,
  }),
  candleBundle: site({
    pattern: "/floor-amm/launches/<launch>/candle-bundle",
    v2: true,
    query: {
      intervals: { kind: "list", doc: "these buckets' pages (absent: every one, in CANDLE_INTERVALS order)", max: B.CANDLE_INTERVALS.length },
      limit: LIMIT(B.CANDLE_BUNDLE_LIMIT, B.CANDLES_MAX),
    },
    page: limit(B.CANDLE_BUNDLE_LIMIT, B.CANDLES_MAX),
    errors: [...LAUNCH_ERR, "bad_query", "not_found"],
    body: B.CANDLE_BUNDLE,
  }),

  walletTrades: site({
    pattern: "/floor-amm/wallets/<wallet>/trades",
    v2: true,
    query: { limit: LIMIT(100, 500), before: BEFORE, launch: { kind: "string", doc: "only this launch (an id or its token)" } },
    page: cursor(100, 500),
    errors: [...WALLET_ERR, "unknown_launch"],
    body: B.WALLET_TRADES,
  }),

  tonActivity: site({
    pattern: "/floor-amm/wallets/<wallet>/ton-activity",
    query: { limit: LIMIT(100, 200), before: BEFORE },
    page: cursor(100, 200),
    errors: WALLET_ERR,
    body: B.TON_ACTIVITY,
  }),
  tonOutcome: site({
    pattern: "/floor-amm/wallets/<wallet>/ton-outcome",
    query: { queryId: { kind: "string", doc: "the query id (uint64, decimal) the SDK message carried", required: true } },
    errors: WALLET_ERR,
    body: B.TON_OUTCOME,
  }),

  positions: site({ pattern: "/floor-amm/wallets/<wallet>/positions", v2: true, query: WALLET_PAGE_KEYS("opaque, launch create order"), page: WALLET_PAGE, errors: WALLET_ERR, body: B.POSITIONS }),

  inbox: site({
    pattern: "/floor-amm/wallets/<wallet>/inbox",
    v2: true,
    query: { limit: LIMIT(1_000, 1_000), before: { kind: "cursor", doc: "asks a page: empty = the newest; a page's next (opaque): items strictly before it" } },
    page: { kind: "cursor", limit: { default: 1_000, max: 1_000 }, cursor: "before", next: "next", optIn: { default: INBOX_PAGE_DEFAULT, max: INBOX_PAGE_MAX } },
    errors: WALLET_ERR,
    body: B.INBOX,
  }),
  walletLocks: site({ pattern: "/floor-amm/wallets/<wallet>/locks", v2: true, query: WALLET_PAGE_KEYS("opaque, launch create order"), page: WALLET_PAGE, errors: WALLET_ERR, body: B.WALLET_LOCKS }),
  claimHistory: site({ pattern: "/floor-amm/wallets/<wallet>/claims", query: WALLET_PAGE_KEYS("opaque, newest first"), page: WALLET_PAGE, errors: WALLET_ERR, body: B.CLAIM_HISTORY }),
  firsts: site({ pattern: "/floor-amm/wallets/<wallet>/firsts", errors: WALLET_ERR, body: B.FIRSTS }),
  firstsMany: site({ pattern: "/floor-amm/wallets/firsts", query: { wallets: { kind: "list", doc: "the wallets", required: true, min: 1, max: FIRSTS_MAX } }, errors: WALLET_ERR, body: B.FIRSTS_MANY }),
  claimView: site({ pattern: "/floor-amm/claims/<wallet>", query: WALLET_PAGE_KEYS("a page's next: the last row key (l:<launch>, r:<account>, p:<traderRef>, o:<quote>), string order"), page: WALLET_PAGE, errors: [...WALLET_ERR, "bad_query", "no_reader"], body: B.CLAIM_VIEW }),
  people: site({ pattern: "/floor-amm/referrals/<wallet>/people", query: WALLET_PAGE_KEYS("opaque, referred wallet order"), page: WALLET_PAGE, errors: WALLET_ERR, body: B.PEOPLE }),
  roles: site({ pattern: "/floor-amm/roles/<wallet>", query: WALLET_PAGE_KEYS("opaque, a page of launches in create order"), page: WALLET_PAGE, errors: WALLET_ERR, body: B.ROLES }),

  handlePlaces: site({
    pattern: "/floor-amm/handles/<handle>/places",
    query: { limit: LIMIT(100, 500), cursor: { kind: "string", doc: "a page's next: <launch>:<place>, rows strictly after it", max: WALLET_CURSOR_MAX } },
    page: keyset(100, 500),
    errors: WALLET_ERR, body: B.HANDLE_PLACES }),

  traderBoard: site({ pattern: "/floor-amm/leaderboards/traders", v2: true, query: { window: WINDOW, limit: LIMIT(100, 500) }, page: limit(100, 500), errors: [...GATED, "bad_query"], body: B.TRADER_BOARD }),
  launcherBoard: site({ pattern: "/floor-amm/leaderboards/launchers", v2: true, query: { limit: LIMIT(100, 500) }, page: limit(100, 500), errors: [...GATED, "bad_query"], body: B.LAUNCHER_BOARD }),

  tokenHolders: partner({
    pattern: "/floor-amm/holders/<token>",
    aliases: ["/holders/<token>"],
    params: ["token"],
    query: { limit: LIMIT(100, 500), offset: { kind: "int", doc: "rows skipped", min: 0, default: 0 } },
    page: { kind: "offset", limit: { default: 100, max: 500 }, total: "total" },
    errors: [...GATED, "bad_query", "not_found"],
    body: P.TOKEN_HOLDERS,
  }),
  dexAsset: partner({ pattern: "/v2/dex/asset", aliases: ["/asset"], query: { id: { kind: "string", doc: "a launch's token, or a quote", required: true } }, errors: [...GATED, "bad_query", "not_found", "not_indexed"], body: P.DEX_ASSET }),
  dexSummary: partner({ pattern: "/v2/dex/summary", aliases: ["/summary"], query: { id: { kind: "string", doc: "the pair id", required: true } }, errors: [...GATED, "bad_query", "not_found"], body: P.DEX_SUMMARY }),
  dexPair: partner({ pattern: "/v2/dex/pair", aliases: ["/pair"], query: { id: { kind: "string", doc: "the pair id", required: true } }, errors: [...GATED, "unknown_launch"], body: P.DEX_PAIR }),
  cgPairs: partner({ pattern: "/v2/coingecko/pairs", aliases: ["/coingecko/pairs"], errors: GATED, body: P.CG_PAIRS }),
  cgTickers: partner({ pattern: "/v2/coingecko/tickers", aliases: ["/coingecko/tickers"], errors: GATED, body: P.CG_TICKERS }),
  cgHistoricalTrades: partner({
    pattern: "/v2/coingecko/historical_trades",
    aliases: ["/coingecko/historical_trades", "/historical_trades"],
    query: {
      ticker_id: { kind: "string", doc: "<token>_<quote>", required: true },
      type: { kind: "enum", doc: "one side only", values: ["buy", "sell"] },
      limit: { kind: "int", doc: "rows per side (absent: 500; 0: all, refused past 5000 rows in all: 413, page with before)", min: 0, max: 5000, default: 500 },
      before: BEFORE,
      start_time: { kind: "unix", doc: "from (10^11 and over: milliseconds)" },
      end_time: { kind: "unix", doc: "to (10^11 and over: milliseconds)" },
    },

    page: cursor(500, 5000),
    errors: [...GATED, "bad_query", "not_found", "too_many"],
    body: P.CG_HISTORICAL,
  }),
  llamaTvl: partner({ pattern: "/v2/defillama/tvl", errors: GATED, body: P.LLAMA_TVL }),
  llamaFees: partner({
    pattern: "/v2/defillama/fees",
    query: { from: { kind: "unix", doc: "window start, inclusive (with to)" }, to: { kind: "unix", doc: "window end, exclusive (with from)" } },
    page: { kind: "range", from: "from", to: "to" },
    body: P.LLAMA_FEES,
  }),

  health: { pattern: "/health", aliases: [], mount: "root", params: [], query: {}, page: NONE, errors: [], gated: false, units: "site", audience: "ops", body: P.HEALTH },
}                                                      ;

export const ROUTE_NAMES = Object.keys(INDEXER_CONTRACT)               ;

export function matchRoute(path        )                                                                         {
  const p = path.replace(/\/+$/, "") || "/";
  for (const name of ROUTE_NAMES) {
    const c                = INDEXER_CONTRACT[name];
    for (const pat of [c.pattern, ...c.aliases]) {
      const keys              = [];
      const re = new RegExp(`^${pat.replace(/[.*+?^${}()|[\]\\]/g, "\\$&").replace(/<(launch|wallet|token|handle)>/g, (_, k           ) => (keys.push(k), "([^/]+)"))}$`);
      const m = re.exec(p);
      if (m === null) continue;
      const params                                     = {};
      keys.forEach((k, i) => (params[k] = decodeURIComponent(m[i + 1])));
      return { name, params };
    }
  }
  return null;
}

export const OPEN_ROUTES = {

  genesisSeries: { pattern: "/floor-amm/genesis/series", served: false, open: "undecided: kept or dropped with the site's row" },

  points: { pattern: null, served: false, open: "undecided: a points route, or the trades drain as now" },
}         ;

export function routeLimit(name           )                                                            {
  const p             = INDEXER_CONTRACT[name].page;
  return p.kind === "limit" || p.kind === "offset" || p.kind === "cursor" || p.kind === "keyset" ? p.limit : null;
}

export const LIMITS = Object.fromEntries(ROUTE_NAMES.flatMap((n) => {
  const l = routeLimit(n);
  return l === null ? [] : [[n, l]];
}))                                                                                            ;

export function clampLimit(name           , n        , paged = false)         {
  const p             = INDEXER_CONTRACT[name].page;
  const l = paged && p.kind === "cursor" && p.optIn !== undefined ? p.optIn : routeLimit(name);

  const lo = (INDEXER_CONTRACT[name].query                            ).limit?.min ?? 1;
  return l === null ? n : Math.min(Math.max(lo, Math.trunc(n)), l.max);
}

export function pageAsked(name           , _q                                            )          {
  const p             = INDEXER_CONTRACT[name].page;

  if (p.kind === "cursor" && p.optIn !== undefined) return true;
  if (p.kind === "keyset" && p.optIn === true) return true;
  return false;
}

export function pageLimit(name           , q                 )         {
  const p             = INDEXER_CONTRACT[name].page;
  if (p.kind === "cursor" && p.optIn !== undefined) {
    const raw = q.get("limit");
    return raw === null || raw === "" || !DIGITS.test(raw) ? p.optIn.default : Math.min(Math.max(1, Number(raw)), p.optIn.max);
  }
  return queryLimit(name, q);
}

const DIGITS = /^\d+$/;

export function queryLimit(name           , q                 )         {
  const l = routeLimit(name);
  if (l === null) throw new TypeError(`${name} takes no limit`);
  const raw = q.get("limit");
  return raw === null || raw === "" || !DIGITS.test(raw) ? l.default : clampLimit(name, Number(raw));
}

export function queryRefusal(name           , q                 )                {
  const spec                                     = INDEXER_CONTRACT[name].query;
  for (const [key, k] of Object.entries(spec)) {
    const raw = q.get(key);
    if (raw === null || raw === "") {
      if (k.required === true && TABLE_REQUIRED.has(name)) return `${key} is required`;
      continue;
    }
    for (const w of k.without ?? []) if ((q.get(w) ?? "") !== "") return `${key} cannot be given with ${w}`;
    switch (k.kind) {
      case "int": {
        const lo = k.min ?? 0;
        const n = DIGITS.test(raw) ? Number(raw) : NaN;
        if (!(n >= lo) || (k.max !== undefined && n > k.max)) return k.max === undefined ? `${key} must be an integer ≥ ${lo}` : `${key} must be ${lo}..${k.max}`;
        break;
      }
      case "list": {
        const parts = raw.split(",").filter((s) => s !== "");
        if (k.max !== undefined && parts.length > k.max) return `at most ${k.max} ${key}`;
        const it = k.item;
        if (it !== undefined && parts.some((x) => !DIGITS.test(x) || Number(x) < it.min || Number(x) > it.max)) return `each ${key} must be an integer ${it.min}..${it.max}`;
        break;
      }
      case "string":
        if (k.max !== undefined && raw.length > k.max) return `${key} must be at most ${k.max} characters`;
        break;
      case "enum":
        if (k.values !== undefined && !k.values.includes(raw)) return `${key} must be one of ${k.values.join(", ")}`;
        break;
      case "bool":
        if (!(raw in BOOLS)) return `${key} must be true or false`;
        break;
      default:
        break;
    }
  }

  const prod = ROWS_PRODUCT[name];
  if (prod !== undefined) {
    const n = (q.get(prod.list) ?? "").split(",").filter((s) => s !== "").length;
    const raw = q.get("limit");
    const lim = raw === null || raw === "" ? routeLimit(name) .default : Number(raw);
    if (n * lim > prod.max) return `${prod.list} × limit must be at most ${prod.max} (asked ${n} × ${lim})`;
  }

  const pg             = INDEXER_CONTRACT[name].page;
  if (pg.kind === "cursor" && pg.optIn !== undefined && q.get(pg.cursor) !== null) {
    const raw = q.get("limit");
    if (raw !== null && raw !== "" && Number(raw) > pg.optIn.max) return `limit must be 1..${pg.optIn.max} with ${pg.cursor}`;
  }
  return spanRefusal(name, q);
}

const ROWS_PRODUCT                                                            = {
  launchLists: { list: "quotes", max: LAUNCH_LISTS_ROWS_MAX },
  candlesMany: { list: "ids", max: CANDLES_MANY_ROWS_MAX },
};

const TABLE_REQUIRED                         = new Set           (["search", "launchLists", "candlesMany"]);

const BOOLS                                    = { true: true, false: false, 1: true, 0: false };

export function queryArg(name           , q                 , key        )                     {
  const k                       = (INDEXER_CONTRACT[name].query                                      )[key];
  if (k === undefined) throw new TypeError(`${name} reads no ${key}`);
  const raw = q.get(key);
  return raw !== null && raw !== "" ? raw : k.default === undefined ? undefined : String(k.default);
}

export function queryBool(name           , q                 , key        )                 {
  const raw = queryArg(name, q, key);
  return raw === undefined ? null : (BOOLS[raw] ?? null);
}

export function spanRefusal(name           , q                 , toDefault         )                {
  const p             = INDEXER_CONTRACT[name].page;
  if (p.kind !== "range" || p.span === undefined) return null;
  const sec = (B.CANDLE_INTERVAL_SECS                                    )[queryArg(name, q, p.span.interval) ?? ""];
  const fromRaw = q.get(p.from), toRaw = q.get(p.to);
  if (sec === undefined || fromRaw === null || !DIGITS.test(fromRaw)) return null;
  const to = toRaw === null ? toDefault : DIGITS.test(toRaw) ? Number(toRaw) : undefined;
  if (to === undefined) return null;
  const from = Number(fromRaw);
  if (from > to) return null;
  const bucket = (t        ) => Math.floor(t / sec) * sec;
  return (bucket(to) - bucket(from)) / sec + 1 > p.span.max ? `the range spans more than ${p.span.max} candles` : null;
}
