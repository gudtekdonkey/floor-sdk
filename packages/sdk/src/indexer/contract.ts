

import type { Decoder } from "./decode.ts";
import { unknown } from "./decode.ts";
import * as B from "./bodies.ts";
import * as P from "./partners.ts";
import { FIRSTS_MAX, IDS_MAX, LAUNCHES_PAGE_MAX, LAUNCH_SORTS, STREAM_STATE_MAX } from "./paths.ts";

export const UNITS = {
  site: "raw quote base units, token math units (6 dp), Q64.64 prices, unix s; bigint as decimal strings; USD as decimal strings",
  partner: "the partner's format: whole-unit decimal strings unless a field says raw base units; unix s",
} as const;
export type Units = keyof typeof UNITS;

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
] as const;
export type IndexerErrorCode = (typeof INDEXER_ERROR_CODES)[number];

const errorWord = (body: unknown): string | null => (typeof body === "object" && body !== null && "error" in body ? String((body as { error: unknown }).error) : null);

export function errorCode(status: number, body: unknown): IndexerErrorCode {
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

export interface QueryKey {
  readonly kind: "int" | "string" | "bool" | "list" | "enum" | "unix" | "block" | "cursor";
  readonly doc: string;
  readonly values?: readonly string[];
  readonly min?: number;
  readonly max?: number;
  readonly default?: string | number;
  readonly required?: boolean;
}

export type Pagination =
  | { readonly kind: "none" }

  | { readonly kind: "limit"; readonly limit: { readonly default: number; readonly max: number } }

  | { readonly kind: "offset"; readonly limit: { readonly default: number; readonly max: number }; readonly total: "total" }

  | { readonly kind: "cursor"; readonly limit: { readonly default: number; readonly max: number }; readonly cursor: "before"; readonly next: "next" }

  | { readonly kind: "range"; readonly from: string; readonly to: string; readonly span?: { readonly interval: string; readonly max: number } };

export type PathParam = "launch" | "wallet";

export interface RouteContract<T = unknown> {
  readonly pattern: string;
  readonly aliases: readonly string[];
  readonly mount: "chain" | "root";
  readonly params: readonly PathParam[];
  readonly query: Readonly<Record<string, QueryKey>>;
  readonly page: Pagination;
  readonly errors: readonly IndexerErrorCode[];

  readonly gated: boolean;

  readonly sse?: true;
  readonly units: Units;
  readonly audience: "site" | "partner" | "ops";
  readonly body: Decoder<T>;
}

const NONE = { kind: "none" } as const;
const limit = (dflt: number, max: number) => ({ kind: "limit", limit: { default: dflt, max } }) as const;
const cursor = (dflt: number, max: number) => ({ kind: "cursor", limit: { default: dflt, max }, cursor: "before", next: "next" }) as const;
const LIMIT = (dflt: number, max: number): QueryKey => ({ kind: "int", doc: "rows", min: 1, max, default: dflt });
const BEFORE: QueryKey = { kind: "cursor", doc: "a block, or a row id <block>:<txnIndex>:<eventIndex> (a page's next): rows strictly before it" };
const WINDOW: QueryKey = { kind: "enum", doc: "the volume window", values: B.TRADER_WINDOWS, default: "24h" };

const GATED = ["not_ready", "upstream"] as const;
const LAUNCH_ERR = [...GATED, "unknown_launch"] as const;
const WALLET_ERR = [...GATED, "bad_query"] as const;

type Spec<T> = Omit<RouteContract<T>, "aliases" | "mount" | "params" | "query" | "page" | "errors" | "gated" | "units" | "audience"> & Partial<RouteContract<T>>;

function site<T>(s: Spec<T> & { v2?: boolean }): RouteContract<T> {
  const params = [...s.pattern.matchAll(/<(launch|wallet)>/g)].map((m) => m[1] as PathParam);
  const { v2, ...rest } = s;
  const aliases = s.aliases ?? (v2 ? [s.pattern.replace(/^\/floor-amm\//, "/floor-amm/v2/")] : []);
  return { mount: "chain", query: {}, page: NONE, errors: GATED, gated: true, units: "site", audience: "site", ...rest, params, aliases };
}
function partner<T>(s: Spec<T>): RouteContract<T> {
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
  floorRouted: site({ pattern: "/floor-amm/floor-routed", aliases: ["/floor-amm/v2/floor-routed"], body: B.FLOOR_ROUTED }),
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
      q: { kind: "string", doc: "name or symbol contains, or an id, token or creator equals" },
      ids: { kind: "list", doc: "these launches (ids or tokens), in the order asked without sort", max: IDS_MAX },
      quote: { kind: "string", doc: "launches quoted in this coin" },
      creator: { kind: "string", doc: "launches by this creator" },
      rankable: { kind: "bool", doc: "true (or 1) listed only, false (or 0) unlisted only" },
      graduated: { kind: "bool", doc: "graduated or not (true / false, or 1 / 0)" },
      limit: LIMIT(LAUNCHES_PAGE_MAX, LAUNCHES_PAGE_MAX),
      offset: { kind: "int", doc: "rows skipped", min: 0, default: 0 },
    },
    page: { kind: "offset", limit: { default: LAUNCHES_PAGE_MAX, max: LAUNCHES_PAGE_MAX }, total: "total" },
    errors: [...GATED, "bad_query"],
    body: B.LAUNCHES,
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
  launchClaims: site({ pattern: "/floor-amm/launches/<launch>/claims", v2: true, query: { claimer: { kind: "string", doc: "only this claimer's rows" } }, errors: LAUNCH_ERR, body: B.LAUNCH_CLAIMS }),
  devBuys: site({ pattern: "/floor-amm/launches/<launch>/dev-buys", v2: true, errors: LAUNCH_ERR, body: B.DEV_BUYS }),
  traders: site({ pattern: "/floor-amm/launches/<launch>/traders", v2: true, query: { window: WINDOW, limit: LIMIT(100, 500) }, page: limit(100, 500), errors: [...LAUNCH_ERR, "bad_query"], body: B.LAUNCH_TRADERS }),
  quoteChain: site({ pattern: "/floor-amm/launches/<launch>/chain", v2: true, errors: LAUNCH_ERR, body: B.QUOTE_CHAIN }),
  devTrades: site({ pattern: "/floor-amm/launches/<launch>/dev", v2: true, query: { limit: LIMIT(100, 500) }, page: limit(100, 500), errors: [...LAUNCH_ERR, "bad_query"], body: B.DEV_TRADES }),
  burns: site({ pattern: "/floor-amm/launches/<launch>/burns", v2: true, errors: LAUNCH_ERR, body: B.BURNS }),
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
  positions: site({ pattern: "/floor-amm/wallets/<wallet>/positions", v2: true, errors: WALLET_ERR, body: B.POSITIONS }),
  inbox: site({ pattern: "/floor-amm/wallets/<wallet>/inbox", v2: true, query: { limit: LIMIT(1_000, 1_000) }, page: limit(1_000, 1_000), errors: WALLET_ERR, body: B.INBOX }),
  walletLocks: site({ pattern: "/floor-amm/wallets/<wallet>/locks", v2: true, errors: WALLET_ERR, body: B.WALLET_LOCKS }),
  claimHistory: site({ pattern: "/floor-amm/wallets/<wallet>/claims", errors: WALLET_ERR, body: B.CLAIM_HISTORY }),
  firsts: site({ pattern: "/floor-amm/wallets/<wallet>/firsts", errors: WALLET_ERR, body: B.FIRSTS }),
  firstsMany: site({ pattern: "/floor-amm/wallets/firsts", query: { wallets: { kind: "list", doc: "the wallets", required: true, min: 1, max: FIRSTS_MAX } }, errors: WALLET_ERR, body: B.FIRSTS_MANY }),
  claimView: site({ pattern: "/floor-amm/claims/<wallet>", errors: [...WALLET_ERR, "no_reader"], body: B.CLAIM_VIEW }),
  people: site({ pattern: "/floor-amm/referrals/<wallet>/people", errors: WALLET_ERR, body: B.PEOPLE }),
  roles: site({ pattern: "/floor-amm/roles/<wallet>", errors: WALLET_ERR, body: B.ROLES }),

  traderBoard: site({ pattern: "/floor-amm/leaderboards/traders", v2: true, query: { window: WINDOW, limit: LIMIT(100, 500) }, page: limit(100, 500), errors: [...GATED, "bad_query"], body: B.TRADER_BOARD }),
  launcherBoard: site({ pattern: "/floor-amm/leaderboards/launchers", v2: true, query: { limit: LIMIT(100, 500) }, page: limit(100, 500), errors: [...GATED, "bad_query"], body: B.LAUNCHER_BOARD }),

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
      limit: { kind: "int", doc: "rows per side (0: all)", min: 0 },
      start_time: { kind: "unix", doc: "from (10^11 and over: milliseconds)" },
      end_time: { kind: "unix", doc: "to (10^11 and over: milliseconds)" },
    },
    page: { kind: "range", from: "start_time", to: "end_time" },
    errors: [...GATED, "bad_query", "not_found"],
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
} as const satisfies Record<string, RouteContract<any>>;

export type RouteName = keyof typeof INDEXER_CONTRACT;

export type RouteBody<N extends RouteName> = (typeof INDEXER_CONTRACT)[N]["body"] extends Decoder<infer T> ? T : never;

export const ROUTE_NAMES = Object.keys(INDEXER_CONTRACT) as RouteName[];

export function matchRoute(path: string): { name: RouteName; params: Partial<Record<PathParam, string>> } | null {
  const p = path.replace(/\/+$/, "") || "/";
  for (const name of ROUTE_NAMES) {
    const c: RouteContract = INDEXER_CONTRACT[name];
    for (const pat of [c.pattern, ...c.aliases]) {
      const keys: PathParam[] = [];
      const re = new RegExp(`^${pat.replace(/[.*+?^${}()|[\]\\]/g, "\\$&").replace(/<(launch|wallet)>/g, (_, k: PathParam) => (keys.push(k), "([^/]+)"))}$`);
      const m = re.exec(p);
      if (m === null) continue;
      const params: Partial<Record<PathParam, string>> = {};
      keys.forEach((k, i) => (params[k] = decodeURIComponent(m[i + 1])));
      return { name, params };
    }
  }
  return null;
}

export type Served<T> = T extends bigint
  ? bigint | string | number
  : T extends readonly (infer E)[]
    ? readonly Served<E>[]
    : T extends object
      ? { [K in keyof T]: Served<T[K]> }
      : T;

export type ServedBody<N extends RouteName> = Served<RouteBody<N>>;

export type ServedKeys<N extends RouteName> = RouteBody<N> extends readonly unknown[] ? readonly unknown[] : { [K in keyof RouteBody<N>]: unknown };

export const OPEN_ROUTES = {

  genesisSeries: { pattern: "/floor-amm/genesis/series", served: false, open: "undecided: kept or dropped with the site's row" },

  points: { pattern: null, served: false, open: "undecided: a points route, or the trades drain as now" },
} as const;

export function routeLimit(name: RouteName): { readonly default: number; readonly max: number } | null {
  const p: Pagination = INDEXER_CONTRACT[name].page;
  return p.kind === "limit" || p.kind === "offset" || p.kind === "cursor" ? p.limit : null;
}

export const LIMITS = Object.fromEntries(ROUTE_NAMES.flatMap((n) => {
  const l = routeLimit(n);
  return l === null ? [] : [[n, l]];
})) as Readonly<Partial<Record<RouteName, { readonly default: number; readonly max: number }>>>;

export function clampLimit(name: RouteName, n: number): number {
  const l = routeLimit(name);
  return l === null ? n : Math.min(Math.max(1, Math.trunc(n)), l.max);
}

const DIGITS = /^\d+$/;

export function queryLimit(name: RouteName, q: URLSearchParams): number {
  const l = routeLimit(name);
  if (l === null) throw new TypeError(`${name} takes no limit`);
  const raw = q.get("limit");
  return raw === null || raw === "" || !DIGITS.test(raw) ? l.default : clampLimit(name, Number(raw));
}

export function queryRefusal(name: RouteName, q: URLSearchParams): string | null {
  const spec: Readonly<Record<string, QueryKey>> = INDEXER_CONTRACT[name].query;
  for (const [key, k] of Object.entries(spec)) {
    const raw = q.get(key);
    if (raw === null || raw === "") continue;
    switch (k.kind) {
      case "int": {
        const lo = k.min ?? 0;
        const n = DIGITS.test(raw) ? Number(raw) : NaN;
        if (!(n >= lo) || (k.max !== undefined && n > k.max)) return k.max === undefined ? `${key} must be an integer ≥ ${lo}` : `${key} must be ${lo}..${k.max}`;
        break;
      }
      case "list": {
        const n = raw.split(",").filter((s) => s !== "").length;
        if (k.max !== undefined && n > k.max) return `at most ${k.max} ${key}`;
        break;
      }
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
  return spanRefusal(name, q);
}

const BOOLS: Readonly<Record<string, boolean>> = { true: true, false: false, 1: true, 0: false };

export function queryArg(name: RouteName, q: URLSearchParams, key: string): string | undefined {
  const k: QueryKey | undefined = (INDEXER_CONTRACT[name].query as Readonly<Record<string, QueryKey>>)[key];
  if (k === undefined) throw new TypeError(`${name} reads no ${key}`);
  const raw = q.get(key);
  return raw !== null && raw !== "" ? raw : k.default === undefined ? undefined : String(k.default);
}

export function queryBool(name: RouteName, q: URLSearchParams, key: string): boolean | null {
  const raw = queryArg(name, q, key);
  return raw === undefined ? null : (BOOLS[raw] ?? null);
}

export function spanRefusal(name: RouteName, q: URLSearchParams, toDefault?: number): string | null {
  const p: Pagination = INDEXER_CONTRACT[name].page;
  if (p.kind !== "range" || p.span === undefined) return null;
  const sec = (B.CANDLE_INTERVAL_SECS as Readonly<Record<string, number>>)[queryArg(name, q, p.span.interval) ?? ""];
  const fromRaw = q.get(p.from), toRaw = q.get(p.to);
  if (sec === undefined || fromRaw === null || !DIGITS.test(fromRaw)) return null;
  const to = toRaw === null ? toDefault : DIGITS.test(toRaw) ? Number(toRaw) : undefined;
  if (to === undefined) return null;
  const from = Number(fromRaw);
  if (from > to) return null;
  const bucket = (t: number) => Math.floor(t / sec) * sec;
  return (bucket(to) - bucket(from)) / sec + 1 > p.span.max ? `the range spans more than ${p.span.max} candles` : null;
}
