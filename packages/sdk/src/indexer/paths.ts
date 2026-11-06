

export interface IndexerRoute {
  readonly pattern: string;
  readonly keys: readonly string[];
}

export const INDEXER_ROUTES = {
  head: { pattern: "/floor-amm/head", keys: [] },
  latestBlock: { pattern: "/floor-amm/latest-block", keys: [] },
  stream: { pattern: "/floor-amm/stream", keys: ["launch", "state", "lastEventId"] },
  quotes: { pattern: "/floor-amm/quotes", keys: [] },
  summary: { pattern: "/floor-amm/summary", keys: [] },
  floorRouted: { pattern: "/floor-amm/floor-routed", keys: [] },
  events: { pattern: "/floor-amm/events", keys: ["fromBlock", "toBlock"] },
  launches: { pattern: "/floor-amm/launches", keys: ["sort", "order", "q", "ids", "quote", "creator", "rankable", "graduated", "limit", "offset"] },
  launch: { pattern: "/floor-amm/launches/<launch>", keys: [] },
  launchState: { pattern: "/floor-amm/launches/<launch>/state", keys: [] },
  milestones: { pattern: "/floor-amm/launches/<launch>/milestones", keys: [] },
  holders: { pattern: "/floor-amm/launches/<launch>/holders", keys: ["limit"] },
  trades: { pattern: "/floor-amm/launches/<launch>/trades", keys: ["limit", "before", "traders"] },
  launchClaims: { pattern: "/floor-amm/launches/<launch>/claims", keys: ["claimer"] },
  devBuys: { pattern: "/floor-amm/launches/<launch>/dev-buys", keys: [] },
  memberSold: { pattern: "/floor-amm/launches/<launch>/member-sold", keys: [] },
  launchAccount: { pattern: "/floor-amm/launches/<launch>/account", keys: [] },
  traders: { pattern: "/floor-amm/launches/<launch>/traders", keys: ["window", "limit"] },
  quoteChain: { pattern: "/floor-amm/launches/<launch>/chain", keys: [] },
  devTrades: { pattern: "/floor-amm/launches/<launch>/dev", keys: ["limit"] },
  burns: { pattern: "/floor-amm/launches/<launch>/burns", keys: [] },
  launchLocks: { pattern: "/floor-amm/launches/<launch>/locks", keys: [] },
  marks: { pattern: "/floor-amm/launches/<launch>/marks", keys: ["limit", "before"] },
  price: { pattern: "/floor-amm/launches/<launch>/price", keys: [] },
  highs: { pattern: "/floor-amm/launches/<launch>/highs", keys: ["atLeast", "limit"] },
  candleBundle: { pattern: "/floor-amm/launches/<launch>/candle-bundle", keys: ["intervals", "limit"] },
  candles: { pattern: "/floor-amm/candles", keys: ["launch", "interval", "from", "to", "before", "limit"] },
  walletTrades: { pattern: "/floor-amm/wallets/<wallet>/trades", keys: ["limit", "before", "launch"] },
  positions: { pattern: "/floor-amm/wallets/<wallet>/positions", keys: [] },
  inbox: { pattern: "/floor-amm/wallets/<wallet>/inbox", keys: ["limit"] },
  walletLocks: { pattern: "/floor-amm/wallets/<wallet>/locks", keys: [] },
  claimHistory: { pattern: "/floor-amm/wallets/<wallet>/claims", keys: [] },
  firsts: { pattern: "/floor-amm/wallets/<wallet>/firsts", keys: [] },
  firstsMany: { pattern: "/floor-amm/wallets/firsts", keys: ["wallets"] },
  claimView: { pattern: "/floor-amm/claims/<wallet>", keys: [] },
  people: { pattern: "/floor-amm/referrals/<wallet>/people", keys: [] },
  roles: { pattern: "/floor-amm/roles/<wallet>", keys: [] },
  handlePlaces: { pattern: "/floor-amm/handles/<handle>/places", keys: [] },
  traderBoard: { pattern: "/floor-amm/leaderboards/traders", keys: ["window", "limit"] },
  launcherBoard: { pattern: "/floor-amm/leaderboards/launchers", keys: ["limit"] },
} as const satisfies Record<string, IndexerRoute>;

export type IndexerRouteName = keyof typeof INDEXER_ROUTES;

export const LAUNCH_SORTS = ["created", "newest", "volume1h", "volume24h", "volume7d", "volume", "marketCap", "liquidity", "backed", "trades24h", "holders", "change1h", "change24h", "lastTrade", "nearFloor", "trending", "newHolders24h"] as const;
export type LaunchSort = (typeof LAUNCH_SORTS)[number];

export const IDS_MAX = 100;

export const LAUNCHES_PAGE_MAX = 1_000;

export const FIRSTS_MAX = 100;

export const STREAM_STATE_MAX = 16;

type QueryValue = string | number | boolean | readonly string[] | undefined | null;

export function query(q: Record<string, QueryValue>): string {
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(q)) {
    if (v === undefined || v === null) continue;
    p.set(k, Array.isArray(v) ? v.join(",") : String(v));
  }
  const s = p.toString();
  return s === "" ? "" : `?${s}`;
}

const need = (s: string): string => {
  if (s === "") throw new RangeError("an empty id or wallet");
  return s;
};
const seg = (s: string): string => encodeURIComponent(need(s));

const launchPath = (id: string, tail = ""): string => `/floor-amm/launches/${seg(id)}${tail}`;
const walletPath = (w: string, tail: string): string => `/floor-amm/wallets/${seg(w)}${tail}`;

export interface LaunchesQuery {

  ids?: readonly string[];
  sort?: LaunchSort;
  order?: "asc" | "desc";

  q?: string;
  quote?: string;
  creator?: string;

  rankable?: boolean;
  graduated?: boolean;
  limit?: number;
  offset?: number;
}

export interface Page {
  limit?: number;

  before?: number | string;
}

export interface TradesQuery extends Page {
  traders?: readonly string[];
}

export interface WalletTradesQuery extends Page {
  launch?: string;
}

export interface CandlesQuery {

  launch: string;
  interval?: string;

  from?: number;

  to?: number;

  before?: number;

  limit?: number;
}

export interface StreamQuery {

  launch?: readonly string[];

  state?: readonly string[];

  lastEventId?: string;
}

export const indexerPath = {
  head: (): string => "/floor-amm/head",
  latestBlock: (): string => "/floor-amm/latest-block",
  stream: (q: StreamQuery = {}): string => {
    if ((q.state?.length ?? 0) > STREAM_STATE_MAX) throw new RangeError(`at most ${STREAM_STATE_MAX} state ids`);
    return `/floor-amm/stream${query({ launch: q.launch?.length ? q.launch : undefined, state: q.state?.length ? q.state : undefined, lastEventId: q.lastEventId })}`;
  },
  quotes: (): string => "/floor-amm/quotes",
  summary: (): string => "/floor-amm/summary",
  floorRouted: (): string => "/floor-amm/floor-routed",
  events: (fromBlock: number, toBlock: number): string => `/floor-amm/events${query({ fromBlock, toBlock })}`,

  launches: (q: LaunchesQuery = {}): string => {
    if (q.ids !== undefined && q.ids.length > IDS_MAX) throw new RangeError(`at most ${IDS_MAX} ids`);
    const sort = q.sort ?? (q.ids === undefined ? "created" : undefined);
    return `/floor-amm/launches${query({ sort, order: q.order, q: q.q, ids: q.ids, quote: q.quote, creator: q.creator, rankable: q.rankable, graduated: q.graduated, limit: q.limit, offset: q.offset })}`;
  },
  launch: (id: string): string => launchPath(id),
  launchState: (id: string): string => launchPath(id, "/state"),
  milestones: (id: string): string => launchPath(id, "/milestones"),
  holders: (id: string, q: { limit?: number } = {}): string => launchPath(id, `/holders${query({ limit: q.limit })}`),
  trades: (id: string, q: TradesQuery = {}): string => launchPath(id, `/trades${query({ limit: q.limit, before: q.before, traders: q.traders?.length ? q.traders : undefined })}`),
  launchClaims: (id: string, q: { claimer?: string } = {}): string => launchPath(id, `/claims${query({ claimer: q.claimer })}`),
  devBuys: (id: string): string => launchPath(id, "/dev-buys"),
  memberSold: (id: string): string => launchPath(id, "/member-sold"),
  launchAccount: (id: string): string => launchPath(id, "/account"),
  traders: (id: string, q: { window?: "1h" | "24h" | "7d" | "all"; limit?: number } = {}): string => launchPath(id, `/traders${query({ window: q.window, limit: q.limit })}`),
  quoteChain: (id: string): string => launchPath(id, "/chain"),
  devTrades: (id: string, q: { limit?: number } = {}): string => launchPath(id, `/dev${query({ limit: q.limit })}`),
  burns: (id: string): string => launchPath(id, "/burns"),
  launchLocks: (id: string): string => launchPath(id, "/locks"),
  marks: (id: string, q: Page = {}): string => launchPath(id, `/marks${query({ limit: q.limit, before: q.before })}`),
  price: (id: string): string => launchPath(id, "/price"),

  highs: (id: string, q: { atLeast?: bigint | string; limit?: number } = {}): string => launchPath(id, `/highs${query({ atLeast: q.atLeast === undefined ? undefined : String(q.atLeast), limit: q.limit })}`),

  candleBundle: (id: string, q: { intervals?: readonly string[]; limit?: number } = {}): string => launchPath(id, `/candle-bundle${query({ intervals: q.intervals?.length ? q.intervals : undefined, limit: q.limit })}`),
  candles: (q: CandlesQuery): string => {
    if (q.before !== undefined && (q.from !== undefined || q.to !== undefined)) throw new RangeError("before cannot be given with from / to");
    if (q.limit !== undefined && q.before === undefined) throw new RangeError("limit is read only with before");
    return `/floor-amm/candles${query({ launch: need(q.launch), interval: q.interval, from: q.from, to: q.to, before: q.before, limit: q.limit })}`;
  },
  walletTrades: (w: string, q: WalletTradesQuery = {}): string => walletPath(w, `/trades${query({ limit: q.limit, before: q.before, launch: q.launch })}`),
  positions: (w: string): string => walletPath(w, "/positions"),
  inbox: (w: string, q: { limit?: number } = {}): string => walletPath(w, `/inbox${query({ limit: q.limit })}`),
  walletLocks: (w: string): string => walletPath(w, "/locks"),
  claimHistory: (w: string): string => walletPath(w, "/claims"),
  firsts: (w: string): string => walletPath(w, "/firsts"),
  firstsMany: (wallets: readonly string[]): string => {
    if (wallets.length === 0 || wallets.length > FIRSTS_MAX) throw new RangeError(`1..${FIRSTS_MAX} wallets`);
    return `/floor-amm/wallets/firsts${query({ wallets })}`;
  },
  claimView: (w: string): string => `/floor-amm/claims/${seg(w)}`,
  people: (w: string): string => `/floor-amm/referrals/${seg(w)}/people`,
  roles: (w: string): string => `/floor-amm/roles/${seg(w)}`,
  handlePlaces: (handle: string): string => `/floor-amm/handles/${seg(handle)}/places`,
  traderBoard: (q: { window?: "1h" | "24h" | "7d" | "all"; limit?: number } = {}): string => `/floor-amm/leaderboards/traders${query({ window: q.window, limit: q.limit })}`,
  launcherBoard: (q: { limit?: number } = {}): string => `/floor-amm/leaderboards/launchers${query({ limit: q.limit })}`,
} as const satisfies Record<IndexerRouteName, (...a: never[]) => string>;
