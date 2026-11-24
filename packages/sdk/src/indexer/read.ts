

import { decode } from "./decode.ts";
import type { Decoder } from "./decode.ts";
import * as B from "./bodies.ts";
import { indexerPath } from "./paths.ts";
import { errorCode } from "./contract.ts";
import type { IndexerErrorCode } from "./contract.ts";
import type { CandlesManyQuery, CandlesQuery, FloorStepsQuery, PositionTotalsQuery, RaiseSeriesQuery, LaunchListsQuery, LaunchSearchQuery, LaunchesQuery, Page, PeopleQuery, TradesQuery, WalletPage, WalletTradesQuery } from "./paths.ts";

export * from "./bodies.ts";
export { ShapeError, decode } from "./decode.ts";
export type { Decoder, Infer } from "./decode.ts";
export { FIRSTS_MAX, POSITION_TOTALS_PAGE_MAX, TRADED_WALLETS_MAX, IDS_MAX, INDEXER_ROUTES, LAUNCHES_PAGE_DEFAULT, LAUNCHES_PAGE_MAX, LAUNCH_CURSOR_MAX, LAUNCH_SORTS, PEOPLE_PERSON_MAX, PRESET_FILTER_MAX, STREAM_STATE_MAX, INBOX_PAGE_DEFAULT, INBOX_PAGE_MAX, WALLET_CURSOR_MAX, WALLET_PAGE_DEFAULT, WALLET_PAGE_MAX, indexerPath, query } from "./paths.ts";
export { ADDRESS_PREFIX_MIN, SEARCH_PAGE_DEFAULT, SEARCH_PAGE_MAX, SEARCH_PARITY_CORPUS, SEARCH_Q_MAX, SEARCH_SUBSTRING_MIN, searchFold, searchNeedle, searchNeedles, searchServed, searchServedTier, searchTier, searchTierFolded } from "./search.ts";
export type { SearchSubject, SearchTier } from "./search.ts";
export * from "./partners.ts";
export * from "./stack-edge.ts";
export { INDEXER_CONTRACT, INDEXER_ERROR_CODES, LIMITS, OPEN_ROUTES, ROUTE_NAMES, UNITS, clampLimit, errorCode, matchRoute, pageAsked, pageLimit, queryArg, queryBool, queryLimit, queryRefusal, routeLimit, spanRefusal } from "./contract.ts";
export type { IndexerErrorCode, Pagination, PathParam, QueryKey, RouteBody, RouteContract, RouteName, Served, ServedBody, ServedKeys, Units } from "./contract.ts";
export { DRAIN_NAMES, createIndexerClient, routePath } from "./client.ts";
export type { DrainName, IndexerClient, IndexerClientOptions, ReadName, RouteArgs } from "./client.ts";
export type { CandlesManyQuery, CandlesQuery, FloorStepsQuery, PositionTotalsQuery, RaiseSeriesQuery, LaunchListsQuery, IndexerRoute, IndexerRouteName, LaunchSearchQuery, LaunchSort, LaunchesQuery, Page, PeopleQuery, StreamQuery, TradesQuery, WalletPage, WalletTradesQuery } from "./paths.ts";

export type LaunchId = string;

export type Get = (path: string) => Promise<{ status: number; body: unknown }>;

export class IndexerError extends Error {
  readonly status: number;
  readonly body: unknown;
  constructor(status: number, body: unknown) {
    super(`indexer ${status}: ${errorOf(body) ?? "error"}`);
    this.status = status;
    this.body = body;
  }

  get code(): IndexerErrorCode {
    return errorCode(this.status, this.body);
  }

  get notReady(): boolean {
    return this.status === 503 && errorOf(this.body) === "not ready";
  }

  get unknownLaunch(): boolean {
    return this.status === 404 && errorOf(this.body) === "unknown launch";
  }
}

const errorOf = (body: unknown): string | null => (typeof body === "object" && body !== null && "error" in body ? String((body as { error: unknown }).error) : null);

export async function readRoute<T>(get: Get, path: string, d: Decoder<T>): Promise<T> {
  const r = await get(path);
  if (r.status !== 200) throw new IndexerError(r.status, r.body);
  return decode(d, r.body);
}

type At = { indexed: B.Indexed | null };

export const readHead = (get: Get): Promise<B.Head> => readRoute(get, indexerPath.head(), B.HEAD);
export const readLatestBlock = (get: Get): Promise<B.LatestBlock> => readRoute(get, indexerPath.latestBlock(), B.LATEST_BLOCK);
export const readQuotes = (get: Get): Promise<B.Quotes> => readRoute(get, indexerPath.quotes(), B.QUOTES);
export const readSummary = (get: Get): Promise<B.Summary> => readRoute(get, indexerPath.summary(), B.SUMMARY);
export const readFloorRouted = (get: Get): Promise<B.FloorRouted> => readRoute(get, indexerPath.floorRouted(), B.FLOOR_ROUTED);

export const readEvents = (get: Get, fromBlock: number, toBlock: number): Promise<B.Events> => readRoute(get, indexerPath.events(fromBlock, toBlock), B.EVENTS);

export const readLaunchLists = (get: Get, q: LaunchListsQuery): Promise<B.LaunchLists> => readRoute(get, indexerPath.launchLists(q), B.LAUNCH_LISTS);
export const readLaunches = (get: Get, q: LaunchesQuery = {}): Promise<B.Launches> => readRoute(get, indexerPath.launches(q), B.LAUNCHES);

export const readLaunchSearch = (get: Get, q: LaunchSearchQuery): Promise<B.LaunchSearch> => readRoute(get, indexerPath.search(q), B.LAUNCH_SEARCH);

export const readLaunch = (get: Get, id: LaunchId): Promise<At & { launch: B.ListedLaunch }> => readRoute(get, indexerPath.launch(id), B.LAUNCH_ONE);

export const readLaunchState = (get: Get, id: LaunchId): Promise<B.LaunchState> => readRoute(get, indexerPath.launchState(id), B.LAUNCH_STATE);
export const readMilestones = (get: Get, id: LaunchId): Promise<B.LaunchMilestones> => readRoute(get, indexerPath.milestones(id), B.LAUNCH_MILESTONES);

export const readHolders = (get: Get, id: LaunchId, q: { limit?: number } = {}): Promise<B.Holders> => readRoute(get, indexerPath.holders(id, q), B.HOLDERS);

export const readTrades = (get: Get, id: LaunchId, q: TradesQuery = {}): Promise<B.Trades> => readRoute(get, indexerPath.trades(id, q), B.TRADES);
export const readLaunchClaims = (get: Get, id: LaunchId, q: Page & { claimer?: string } = {}): Promise<B.LaunchClaims> => readRoute(get, indexerPath.launchClaims(id, q), B.LAUNCH_CLAIMS);
export const readDevBuys = (get: Get, id: LaunchId, q: Page = {}): Promise<B.DevBuys> => readRoute(get, indexerPath.devBuys(id, q), B.DEV_BUYS);

export const readMemberSold = (get: Get, id: LaunchId, q: Page = {}): Promise<B.MemberSoldBody> => readRoute(get, indexerPath.memberSold(id, q), B.MEMBER_SOLD);

export const readLaunchAccount = (get: Get, id: LaunchId): Promise<B.LaunchAccountBody> => readRoute(get, indexerPath.launchAccount(id), B.LAUNCH_ACCOUNT);

export function launchAccountBytes(body: { data: string }): Uint8Array {
  const bin = atob(body.data);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export const readTraders = (get: Get, id: LaunchId, q: { window?: B.TraderWindow; limit?: number } = {}): Promise<B.LaunchTraders> => readRoute(get, indexerPath.traders(id, q), B.LAUNCH_TRADERS);
export const readQuoteChain = (get: Get, id: LaunchId): Promise<B.QuoteChain> => readRoute(get, indexerPath.quoteChain(id), B.QUOTE_CHAIN);

export const readDevTrades = (get: Get, id: LaunchId, q: { limit?: number } = {}): Promise<B.DevTrades> => readRoute(get, indexerPath.devTrades(id, q), B.DEV_TRADES);

export const readBurns = (get: Get, id: LaunchId, q: Page & { wallet?: string } = {}): Promise<B.Burns> => readRoute(get, indexerPath.burns(id, q), B.BURNS);
export const readLaunchLocks = (get: Get, id: LaunchId): Promise<B.LaunchLocks> => readRoute(get, indexerPath.launchLocks(id), B.LAUNCH_LOCKS);

export const readMarks = (get: Get, id: LaunchId, q: Page = {}): Promise<B.Marks> => readRoute(get, indexerPath.marks(id, q), B.MARKS);
export const readPrice = (get: Get, id: LaunchId): Promise<B.Price> => readRoute(get, indexerPath.price(id), B.PRICE);

export const readRaiseSeries = (get: Get, id: LaunchId, q: RaiseSeriesQuery): Promise<B.RaiseSeries> => readRoute(get, indexerPath.raiseSeries(id, q), B.RAISE_SERIES);

export const readFloorSteps = (get: Get, id: LaunchId, q: FloorStepsQuery = {}): Promise<B.FloorSteps> => readRoute(get, indexerPath.floorSteps(id, q), B.FLOOR_STEPS);

export const readLaunchTraded = (get: Get, id: LaunchId, wallets: readonly string[]): Promise<B.LaunchTraded> => readRoute(get, indexerPath.launchTraded(id, wallets), B.LAUNCH_TRADED);

export const readPositionTotals = (get: Get, wallet: string, q: PositionTotalsQuery = {}): Promise<B.PositionTotals> => readRoute(get, indexerPath.positionTotals(wallet, q), B.POSITION_TOTALS);

export const readCandles = (get: Get, q: CandlesQuery): Promise<B.Candles> => readRoute(get, indexerPath.candles(q), B.CANDLES);

export const readCandlesMany = (get: Get, q: CandlesManyQuery): Promise<B.CandlesMany> => readRoute(get, indexerPath.candlesMany(q), B.CANDLES_MANY);
export const readCandleBundle = (get: Get, id: LaunchId, q: { intervals?: readonly string[]; limit?: number } = {}): Promise<B.CandleBundle> => readRoute(get, indexerPath.candleBundle(id, q), B.CANDLE_BUNDLE);
export const readHighs = (get: Get, id: LaunchId, q: { atLeast?: bigint | string; limit?: number } = {}): Promise<B.Highs> => readRoute(get, indexerPath.highs(id, q), B.HIGHS);

export const readWalletTrades = (get: Get, wallet: string, q: WalletTradesQuery = {}): Promise<B.WalletTrades> => readRoute(get, indexerPath.walletTrades(wallet, q), B.WALLET_TRADES);
export const readTonActivity = (get: Get, wallet: string, q: Page = {}): Promise<B.TonActivity> => readRoute(get, indexerPath.tonActivity(wallet, q), B.TON_ACTIVITY);
export const readTonOutcome = (get: Get, wallet: string, queryId: bigint | string): Promise<B.TonOutcome> => readRoute(get, indexerPath.tonOutcome(wallet, queryId), B.TON_OUTCOME);
export const readPositions = (get: Get, wallet: string, q: WalletPage = {}): Promise<B.Positions> => readRoute(get, indexerPath.positions(wallet, q), B.POSITIONS);

export const readInbox = (get: Get, wallet: string, q: { limit?: number; before?: string } = {}): Promise<B.Inbox> => readRoute(get, indexerPath.inbox(wallet, q), B.INBOX);
export const readWalletLocks = (get: Get, wallet: string, q: WalletPage = {}): Promise<B.WalletLocks> => readRoute(get, indexerPath.walletLocks(wallet, q), B.WALLET_LOCKS);
export const readClaimHistory = (get: Get, wallet: string, q: WalletPage = {}): Promise<B.ClaimHistory> => readRoute(get, indexerPath.claimHistory(wallet, q), B.CLAIM_HISTORY);

export const readClaimView = (get: Get, wallet: string, q: WalletPage = {}): Promise<B.ClaimView> => readRoute(get, indexerPath.claimView(wallet, q), B.CLAIM_VIEW);
export const readFirsts = (get: Get, wallet: string): Promise<At & { wallet: string } & B.Firsts> => readRoute(get, indexerPath.firsts(wallet), B.FIRSTS);

export const readFirstsMany = (get: Get, wallets: readonly string[]): Promise<At & { wallets: Record<string, B.Firsts> }> => readRoute(get, indexerPath.firstsMany(wallets), B.FIRSTS_MANY);
export const readPeople = (get: Get, wallet: string, q: PeopleQuery = {}): Promise<B.People> => readRoute(get, indexerPath.people(wallet, q), B.PEOPLE);
export const readRoles = (get: Get, wallet: string, q: WalletPage = {}): Promise<B.Roles> => readRoute(get, indexerPath.roles(wallet, q), B.ROLES);

export const readHandlePlaces = (get: Get, handle: string, q: WalletPage = {}): Promise<B.HandlePlaces> => readRoute(get, indexerPath.handlePlaces(handle, q), B.HANDLE_PLACES);

export const readTraderBoard = (get: Get, q: { window?: B.TraderWindow; limit?: number } = {}): Promise<B.TraderBoard> => readRoute(get, indexerPath.traderBoard(q), B.TRADER_BOARD);
export const readLauncherBoard = (get: Get, q: { limit?: number } = {}): Promise<B.LauncherBoard> => readRoute(get, indexerPath.launcherBoard(q), B.LAUNCHER_BOARD);

export function chainCoins(c: Pick<B.QuoteChain, "chain">): string[] {
  if (c.chain.length === 0) return [];
  return [c.chain[0].token, ...c.chain.map((h) => h.quote)];
}

export function offPriceBps(price: Pick<B.Price, "livePriceX64">, quotedX64: bigint): number | null {
  const p = price.livePriceX64;
  if (p === 0n) return null;
  return Number(((quotedX64 - p) * 10_000n) / p);
}

export interface CursorPage<T> {
  rows: readonly T[];
  next: string | null;
}
export function cursorPage<K extends string, T>(body: { readonly [k in K]: readonly T[] } & { next?: string | null }, rows: K): CursorPage<T> {
  if (body.next === undefined) throw new Error(`not a page: the body has no next (rows ${rows})`);
  return { rows: body[rows], next: body.next };
}
