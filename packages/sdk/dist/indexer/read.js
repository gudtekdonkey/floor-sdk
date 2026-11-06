

import { decode } from "./decode.js";

import * as B from "./bodies.js";
import { indexerPath } from "./paths.js";
import { errorCode } from "./contract.js";

export * from "./bodies.js";
export { ShapeError, decode } from "./decode.js";

export { FIRSTS_MAX, IDS_MAX, INDEXER_ROUTES, LAUNCHES_PAGE_MAX, LAUNCH_SORTS, STREAM_STATE_MAX, indexerPath, query } from "./paths.js";
export * from "./partners.js";
export { INDEXER_CONTRACT, INDEXER_ERROR_CODES, LIMITS, OPEN_ROUTES, ROUTE_NAMES, UNITS, clampLimit, errorCode, matchRoute, queryArg, queryBool, queryLimit, queryRefusal, routeLimit, spanRefusal } from "./contract.js";

export { createIndexerClient, routePath } from "./client.js";

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

export const readLaunches = (get     , q                = {})                      => readRoute(get, indexerPath.launches(q), B.LAUNCHES);

export const readLaunch = (get     , id          )                                           => readRoute(get, indexerPath.launch(id), B.LAUNCH_ONE);

export const readLaunchState = (get     , id          )                         => readRoute(get, indexerPath.launchState(id), B.LAUNCH_STATE);
export const readMilestones = (get     , id          )                              => readRoute(get, indexerPath.milestones(id), B.LAUNCH_MILESTONES);

export const readHolders = (get     , id          , q                     = {})                     => readRoute(get, indexerPath.holders(id, q), B.HOLDERS);

export const readTrades = (get     , id          , q              = {})                    => readRoute(get, indexerPath.trades(id, q), B.TRADES);
export const readLaunchClaims = (get     , id          , q                       = {})                                            => readRoute(get, indexerPath.launchClaims(id, q), B.LAUNCH_CLAIMS);
export const readDevBuys = (get     , id          )                     => readRoute(get, indexerPath.devBuys(id), B.DEV_BUYS);

export const readMemberSold = (get     , id          )                            => readRoute(get, indexerPath.memberSold(id), B.MEMBER_SOLD);

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
export const readBurns = (get     , id          )                   => readRoute(get, indexerPath.burns(id), B.BURNS);
export const readLaunchLocks = (get     , id          )                         => readRoute(get, indexerPath.launchLocks(id), B.LAUNCH_LOCKS);

export const readMarks = (get     , id          , q       = {})                   => readRoute(get, indexerPath.marks(id, q), B.MARKS);
export const readPrice = (get     , id          )                   => readRoute(get, indexerPath.price(id), B.PRICE);

export const readCandles = (get     , q              )                     => readRoute(get, indexerPath.candles(q), B.CANDLES);
export const readCandleBundle = (get     , id          , q                                                    = {})                          => readRoute(get, indexerPath.candleBundle(id, q), B.CANDLE_BUNDLE);
export const readHighs = (get     , id          , q                                                = {})                   => readRoute(get, indexerPath.highs(id, q), B.HIGHS);

export const readWalletTrades = (get     , wallet        , q                    = {})                          => readRoute(get, indexerPath.walletTrades(wallet, q), B.WALLET_TRADES);
export const readPositions = (get     , wallet        )                       => readRoute(get, indexerPath.positions(wallet), B.POSITIONS);

export const readInbox = (get     , wallet        , q                     = {})                   => readRoute(get, indexerPath.inbox(wallet, q), B.INBOX);
export const readWalletLocks = (get     , wallet        )                         => readRoute(get, indexerPath.walletLocks(wallet), B.WALLET_LOCKS);
export const readClaimHistory = (get     , wallet        )                          => readRoute(get, indexerPath.claimHistory(wallet), B.CLAIM_HISTORY);

export const readClaimView = (get     , wallet        )                       => readRoute(get, indexerPath.claimView(wallet), B.CLAIM_VIEW);
export const readFirsts = (get     , wallet        )                                              => readRoute(get, indexerPath.firsts(wallet), B.FIRSTS);

export const readFirstsMany = (get     , wallets                   )                                                      => readRoute(get, indexerPath.firstsMany(wallets), B.FIRSTS_MANY);
export const readPeople = (get     , wallet        )                    => readRoute(get, indexerPath.people(wallet), B.PEOPLE);
export const readRoles = (get     , wallet        )                   => readRoute(get, indexerPath.roles(wallet), B.ROLES);

export const readHandlePlaces = (get     , handle        )                          => readRoute(get, indexerPath.handlePlaces(handle), B.HANDLE_PLACES);

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
