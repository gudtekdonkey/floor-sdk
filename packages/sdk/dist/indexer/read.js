

import { decode } from "./decode.js";

import * as B from "./bodies.js";
import { indexerPath } from "./paths.js";
import { errorCode } from "./contract.js";

export * from "./bodies.js";
export { ShapeError, decode } from "./decode.js";

export { FIRSTS_MAX, IDS_MAX, INDEXER_ROUTES, LAUNCHES_PAGE_DEFAULT, LAUNCHES_PAGE_MAX, LAUNCH_CURSOR_MAX, LAUNCH_SORTS, PRESET_FILTER_MAX, STREAM_STATE_MAX, INBOX_PAGE_DEFAULT, INBOX_PAGE_MAX, WALLET_CURSOR_MAX, WALLET_PAGE_DEFAULT, WALLET_PAGE_MAX, indexerPath, query } from "./paths.js";
export { ADDRESS_PREFIX_MIN, SEARCH_PAGE_DEFAULT, SEARCH_PAGE_MAX, SEARCH_PARITY_CORPUS, SEARCH_Q_MAX, SEARCH_SUBSTRING_MIN, searchFold, searchNeedle, searchNeedles, searchServed, searchServedTier, searchTier, searchTierFolded } from "./search.js";

export * from "./partners.js";
export * from "./stack-edge.js";
export { INDEXER_CONTRACT, INDEXER_ERROR_CODES, LIMITS, OPEN_ROUTES, ROUTE_NAMES, UNITS, clampLimit, errorCode, matchRoute, pageAsked, pageLimit, queryArg, queryBool, queryLimit, queryRefusal, routeLimit, spanRefusal } from "./contract.js";

export { DRAIN_NAMES, createIndexerClient, routePath } from "./client.js";

export class IndexerError extends Error {
           status        ;
           body         ;
  constructor(status        , body         ) {
    super(`indexer ${status}: ${errorOf(body) ?? "error"}`);
    this.status = status;
    this.body = body;
  }

  get code()                   {
    return errorCode(this.status, this.body);
  }

  get notReady()          {
    return this.status === 503 && errorOf(this.body) === "not ready";
  }

  get unknownLaunch()          {
    return this.status === 404 && errorOf(this.body) === "unknown launch";
  }
}

const errorOf = (body         )                => (typeof body === "object" && body !== null && "error" in body ? String((body                      ).error) : null);

export async function readRoute   (get     , path        , d            )             {
  const r = await get(path);
  if (r.status !== 200) throw new IndexerError(r.status, r.body);
  return decode(d, r.body);
}

export const readHead = (get     )                  => readRoute(get, indexerPath.head(), B.HEAD);
export const readLatestBlock = (get     )                         => readRoute(get, indexerPath.latestBlock(), B.LATEST_BLOCK);
export const readQuotes = (get     )                    => readRoute(get, indexerPath.quotes(), B.QUOTES);
export const readSummary = (get     )                     => readRoute(get, indexerPath.summary(), B.SUMMARY);
export const readFloorRouted = (get     )                         => readRoute(get, indexerPath.floorRouted(), B.FLOOR_ROUTED);

export const readEvents = (get     , fromBlock        , toBlock        )                    => readRoute(get, indexerPath.events(fromBlock, toBlock), B.EVENTS);

export const readLaunchLists = (get     , q                  )                         => readRoute(get, indexerPath.launchLists(q), B.LAUNCH_LISTS);
export const readLaunches = (get     , q                = {})                      => readRoute(get, indexerPath.launches(q), B.LAUNCHES);

export const readLaunchSearch = (get     , q                   )                          => readRoute(get, indexerPath.search(q), B.LAUNCH_SEARCH);

export const readLaunch = (get     , id          )                                           => readRoute(get, indexerPath.launch(id), B.LAUNCH_ONE);

export const readLaunchState = (get     , id          )                         => readRoute(get, indexerPath.launchState(id), B.LAUNCH_STATE);
export const readMilestones = (get     , id          )                              => readRoute(get, indexerPath.milestones(id), B.LAUNCH_MILESTONES);

export const readHolders = (get     , id          , q                     = {})                     => readRoute(get, indexerPath.holders(id, q), B.HOLDERS);

export const readTrades = (get     , id          , q              = {})                    => readRoute(get, indexerPath.trades(id, q), B.TRADES);
export const readLaunchClaims = (get     , id          , q                              = {})                          => readRoute(get, indexerPath.launchClaims(id, q), B.LAUNCH_CLAIMS);
export const readDevBuys = (get     , id          , q       = {})                     => readRoute(get, indexerPath.devBuys(id, q), B.DEV_BUYS);

export const readMemberSold = (get     , id          , q       = {})                            => readRoute(get, indexerPath.memberSold(id, q), B.MEMBER_SOLD);

export const readLaunchAccount = (get     , id          )                               => readRoute(get, indexerPath.launchAccount(id), B.LAUNCH_ACCOUNT);

export function launchAccountBytes(body                  )             {
  const bin = atob(body.data);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export const readTraders = (get     , id          , q                                              = {})                           => readRoute(get, indexerPath.traders(id, q), B.LAUNCH_TRADERS);
export const readQuoteChain = (get     , id          )                        => readRoute(get, indexerPath.quoteChain(id), B.QUOTE_CHAIN);

export const readDevTrades = (get     , id          , q                     = {})                       => readRoute(get, indexerPath.devTrades(id, q), B.DEV_TRADES);
export const readBurns = (get     , id          , q       = {})                   => readRoute(get, indexerPath.burns(id, q), B.BURNS);
export const readLaunchLocks = (get     , id          )                         => readRoute(get, indexerPath.launchLocks(id), B.LAUNCH_LOCKS);

export const readMarks = (get     , id          , q       = {})                   => readRoute(get, indexerPath.marks(id, q), B.MARKS);
export const readPrice = (get     , id          )                   => readRoute(get, indexerPath.price(id), B.PRICE);

export const readCandles = (get     , q              )                     => readRoute(get, indexerPath.candles(q), B.CANDLES);

export const readCandlesMany = (get     , q                  )                         => readRoute(get, indexerPath.candlesMany(q), B.CANDLES_MANY);
export const readCandleBundle = (get     , id          , q                                                    = {})                          => readRoute(get, indexerPath.candleBundle(id, q), B.CANDLE_BUNDLE);
export const readHighs = (get     , id          , q                                                = {})                   => readRoute(get, indexerPath.highs(id, q), B.HIGHS);

export const readWalletTrades = (get     , wallet        , q                    = {})                          => readRoute(get, indexerPath.walletTrades(wallet, q), B.WALLET_TRADES);
export const readTonActivity = (get     , wallet        , q       = {})                         => readRoute(get, indexerPath.tonActivity(wallet, q), B.TON_ACTIVITY);
export const readTonOutcome = (get     , wallet        , queryId                 )                        => readRoute(get, indexerPath.tonOutcome(wallet, queryId), B.TON_OUTCOME);
export const readPositions = (get     , wallet        , q             = {})                       => readRoute(get, indexerPath.positions(wallet, q), B.POSITIONS);

export const readInbox = (get     , wallet        , q                                      = {})                   => readRoute(get, indexerPath.inbox(wallet, q), B.INBOX);
export const readWalletLocks = (get     , wallet        , q             = {})                         => readRoute(get, indexerPath.walletLocks(wallet, q), B.WALLET_LOCKS);
export const readClaimHistory = (get     , wallet        , q             = {})                          => readRoute(get, indexerPath.claimHistory(wallet, q), B.CLAIM_HISTORY);

export const readClaimView = (get     , wallet        , q             = {})                       => readRoute(get, indexerPath.claimView(wallet, q), B.CLAIM_VIEW);
export const readFirsts = (get     , wallet        )                                              => readRoute(get, indexerPath.firsts(wallet), B.FIRSTS);

export const readFirstsMany = (get     , wallets                   )                                                      => readRoute(get, indexerPath.firstsMany(wallets), B.FIRSTS_MANY);
export const readPeople = (get     , wallet        , q             = {})                    => readRoute(get, indexerPath.people(wallet, q), B.PEOPLE);
export const readRoles = (get     , wallet        , q             = {})                   => readRoute(get, indexerPath.roles(wallet, q), B.ROLES);

export const readHandlePlaces = (get     , handle        , q             = {})                          => readRoute(get, indexerPath.handlePlaces(handle, q), B.HANDLE_PLACES);

export const readTraderBoard = (get     , q                                              = {})                         => readRoute(get, indexerPath.traderBoard(q), B.TRADER_BOARD);
export const readLauncherBoard = (get     , q                     = {})                           => readRoute(get, indexerPath.launcherBoard(q), B.LAUNCHER_BOARD);

export function chainCoins(c                             )           {
  if (c.chain.length === 0) return [];
  return [c.chain[0].token, ...c.chain.map((h) => h.quote)];
}

export function offPriceBps(price                               , quotedX64        )                {
  const p = price.livePriceX64;
  if (p === 0n) return null;
  return Number(((quotedX64 - p) * 10_000n) / p);
}

export function cursorPage                     (body                                                                , rows   )                {
  if (body.next === undefined) throw new Error(`not a page: the body has no next (rows ${rows})`);
  return { rows: body[rows], next: body.next };
}
