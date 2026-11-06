

import { array, big, bool, decimal, int, nullable, object, oneOf, optional, record, str } from "./decode.ts";
import type { Decoder, Infer } from "./decode.ts";
import type { DevBuy, DevBuyRole } from "../v2/devbuy.ts";

export const INDEXED = object({ block: int, timestamp: int });
export type Indexed = Infer<typeof INDEXED>;
const indexed = nullable(INDEXED);

export const FEES = object({ backing: big, team: big, platform: big, floorFee: big, holder: big });
export type Fees = Infer<typeof FEES>;

export const MILESTONE_CLASSES = ["team", "holder", "platform", "floor"] as const;
export const MILESTONES = object({ team: big, holder: big, platform: big, floor: big });
export type Milestones = Infer<typeof MILESTONES>;

export const DEV_BUY_ROLE: Decoder<DevBuyRole> = oneOf("creator", "team_member", "team_share_buy", "team_buy");

export const DEV_BUY = object({
  role: DEV_BUY_ROLE,
  trader: str,
  quoteIn: big,
  tokens: big,
  priceBeforeX64: big,
  priceAfterX64: big,
  impactBps: big,
  priceNowX64: big,
  changeSinceBps: big,
  block: int,
  timestamp: int,
  tx: str,
});

export type DevBuyItem = Infer<typeof DEV_BUY>;

type Assert<T extends true> = T;
type _DevBuyItemIsSdk = Assert<DevBuyItem extends Omit<DevBuy, "launch" | "slot"> ? true : false>;

export const BONDING = object({ tokens: big, sold: big, left: big, priceX64: big });

const BURN_FIELDS = {
  timestamp: int,
  block: int,
  txnId: nullable(str),
  source: oneOf("program", "dead", "wallet"),
  wallet: nullable(str),
  role: nullable(int),
  place: nullable(int),
  burned: big,
  fromTeamBuy: nullable(big),
  fromShareBought: nullable(big),
  fromWallet: nullable(big),
  balanceBefore: nullable(big),
  pctBps: nullable(int),
  supplyAfter: nullable(big),
  sOutAfter: nullable(big),
  floorAfterX64: nullable(big),
  sOutBefore: nullable(big),
  floorBeforeX64: nullable(big),
  scene: bool,

  floorRiseBps: nullable(int),
};
export const BURN = object(BURN_FIELDS);
export type Burn = Infer<typeof BURN>;
export type Bonding = Infer<typeof BONDING>;

const LAUNCH_FIELDS = {
  id: str,

  pair: str,
  chain: str,
  token: str,
  quote: str,
  creator: str,
  name: nullable(str),
  symbol: nullable(str),
  tokenDecimals: nullable(int),
  preset: int,
  presetHash: str,
  graduated: bool,

  graduation: nullable(object({ block: int, timestamp: int, tx: str })),

  teamSlot: array(object({ wallet: str, place: int })),
  phase: oneOf("bonding", "stacks", "tail"),
  floorX64: big,
  sqrtF: big,

  floorMoney: big,
  cushionMoney: big,
  vault: big,
  bidMoney: big,
  liquidity: big,
  liquidityMode: oneOf("all", "bid"),
  priceX64: big,
  sqrtPrice: big,
  sqrtLMax: big,
  circulating: object({ sCirc: big, dead: big }),
  marketCap: big,
  supply: big,
  unlisted: bool,
  quoteDecimals: nullable(int),
  teamSlotRateBps: nullable(int),
  bidTopX64: nullable(big),
  teamBuyUnlockAt: nullable(int),
  teamBuyUnlocked: bool,
  teamBuyUnlockedTs: nullable(int),
  milestones: object({ booked: MILESTONES, paid: MILESTONES }),
  holderIndex: object({ index: big, wMatured: big, wPending: big, carry: big, rolls: int }),
  volumeGross: big,
  trades: int,
  feesOut: FEES,
  conservation: nullable(object({ inBook: big, held: big, delta: big })),
  devBuy: nullable(object({ count: int, teamBuyUnlocked: bool, quoteIn: big, tokens: big, latest: DEV_BUY })),

  created: object({ block: int, timestamp: int, sqrtP0: nullable(big), teamBuy: big }),
  bonding: nullable(BONDING),

  burns: object({ count: int, burned: big, teamBuyBurned: big, shareBoughtBurned: big, walletBurned: big, last: nullable(BURN), lastScene: nullable(BURN) }),
};
export const LAUNCH = object(LAUNCH_FIELDS);
export type Launch = Infer<typeof LAUNCH>;

export const LISTED_LAUNCH = object({
  ...LAUNCH_FIELDS,
  volume24hGross: nullable(big),
  holders: int,
  backedPpm: nullable(int),
  volume24hUsd: nullable(decimal),
  marketCapUsd: nullable(decimal),

  trendingScore: nullable(big),
});
export type ListedLaunch = Infer<typeof LISTED_LAUNCH>;

export const LIVE_LAUNCH = object({ ...LAUNCH_FIELDS, volume24hGross: nullable(big), holders: int });
export type LiveLaunch = Infer<typeof LIVE_LAUNCH>;

const TRADE_FIELDS = {
  launch: str,
  trader: str,
  side: oneOf("buy", "sell"),
  tokens: big,
  quoteGross: big,
  quoteNet: big,
  fees: FEES,
  traderRef: big,
  creatorRef: nullable(big),
  recipient: str,
  refund: big,
  fromResale: big,
  fromFirst: big,
  unfilled: big,
  steps: int,
  budgetHit: bool,
  kind: nullable(int),
  credits: object({ backing: big, team: big, platform: big, floorFee: big, holder: big, traderRef: big, creatorRef: nullable(big), teamSlot: nullable(big), teamMilestone: big }),
  priceX64: big,
  floorX64: big,
  circulating: big,

  floorMoney: nullable(big),

  bidMoney: nullable(big),
  liquidity: big,
  block: int,
  timestamp: int,
  tx: str,
  txnIndex: int,
  eventIndex: int,
};
export const TRADE = object(TRADE_FIELDS);
export type Trade = Infer<typeof TRADE>;

export const PAGED_TRADE = object({ id: str, ...TRADE_FIELDS });
export type PagedTrade = Infer<typeof PAGED_TRADE>;

export const HEAD = object({
  chain: str,
  indexed,
  safe: nullable(int),
  chainHead: nullable(int),
  lag: nullable(int),
  ready: bool,
  servesUnsafe: bool,
  rollbacks: int,
  startBlock: int,
  stalled: optional(nullable(object({ slot: int, signature: str, cycles: int, error: str, ok: bool }))),
});
export type Head = Infer<typeof HEAD>;

export const LATEST_BLOCK = object({ indexed, block: object({ blockNumber: int, blockTimestamp: int }) });
export type LatestBlock = Infer<typeof LATEST_BLOCK>;

export const QUOTES = object({
  indexed,
  priceUpdater: nullable(str),
  priceUpdaterReported: bool,
  maxAgeS: int,
  quotes: array(object({ mint: str, enabled: bool, createFee: big, usdCentsX64: big, updatedAt: int, ageS: int, fresh: bool, symbol: nullable(str), name: nullable(str), decimals: nullable(int) })),
});
export type Quotes = Infer<typeof QUOTES>;

export const SUMMARY = object({
  indexed,
  liquidityMode: oneOf("all", "bid"),
  quotes: array(object({
    quote: str,
    launches: int,
    graduated: int,
    backing: big,
    liquidity: big,
    volume: big,
    marketCap: big,
    liquidityUsd: nullable(decimal),
    volumeUsd: nullable(decimal),
    marketCapUsd: nullable(decimal),
    usdSource: nullable(str),
  })),
});
export type Summary = Infer<typeof SUMMARY>;

export const FLOOR_ROUTED = object({
  indexed,
  launches: array(object({ launch: str, quote: str, floorFee: big, floorMilestone: big })),
  pending: array(object({ quote: str, amount: big })),
});
export type FloorRouted = Infer<typeof FLOOR_ROUTED>;

export const EVENTS = object({
  events: array(object({
    block: object({ blockNumber: int, blockTimestamp: int }),
    eventType: oneOf("swap"),
    txnId: str,
    txnIndex: int,
    eventIndex: int,
    maker: str,
    pairId: str,

    asset0In: optional(decimal),
    asset1In: optional(decimal),
    asset0Out: optional(decimal),
    asset1Out: optional(decimal),
    priceNative: decimal,
    reserves: object({ asset0: decimal, asset1: decimal }),
  })),
});
export type Events = Infer<typeof EVENTS>;

export const LAUNCHES_ALL = object({ indexed, launches: array(LISTED_LAUNCH) });

export const LAUNCHES = object({ indexed, total: int, offset: int, launches: array(LISTED_LAUNCH), missing: array(str) });
export type Launches = Infer<typeof LAUNCHES>;

export const LAUNCH_ONE = object({ indexed, launch: LISTED_LAUNCH });

export const LAUNCH_STATE = object({ launch: str, mint: str, block: nullable(int), ts: nullable(int), seq: int, state: LIVE_LAUNCH });
export type LaunchState = Infer<typeof LAUNCH_STATE>;

export const LAUNCH_MILESTONES = object({ indexed, classes: array(str), booked: MILESTONES, paid: MILESTONES });
export type LaunchMilestones = Infer<typeof LAUNCH_MILESTONES>;

export const HOLDERS = object({ indexed, source: oneOf("positions", "trades"), total: int, holders: array(object({ holder: str, amount: big, openedTs: int })) });
export type Holders = Infer<typeof HOLDERS>;

export const TRADES = object({ indexed, trades: array(PAGED_TRADE), next: nullable(str) });
export type Trades = Infer<typeof TRADES>;

export const LAUNCH_CLAIM = object({ launch: str, claimer: str, types: int, quote: str, amount: big, holder: big, teamBuy: optional(big), block: int, ts: int, tx: str });
export type LaunchClaim = Infer<typeof LAUNCH_CLAIM>;
export const LAUNCH_CLAIMS = object({ indexed, claims: array(LAUNCH_CLAIM) });

export const DEV_BUYS = object({ indexed, teamBuyUnlocked: bool, teamBuyUnlockedTs: nullable(int), devBuys: array(DEV_BUY) });
export type DevBuys = Infer<typeof DEV_BUYS>;

export const MEMBER_SOLD_ROW = object({ launch: str, member: str, place: int, amount: big, quoteOut: big, block: int, timestamp: int, tx: str, id: str });
export type MemberSoldRow = Infer<typeof MEMBER_SOLD_ROW>;
export const MEMBER_SOLD = object({ indexed, memberSold: array(MEMBER_SOLD_ROW) });
export type MemberSoldBody = Infer<typeof MEMBER_SOLD>;

export const LAUNCH_ACCOUNT = object({ indexed, launch: str, mint: str, slot: int, data: str });
export type LaunchAccountBody = Infer<typeof LAUNCH_ACCOUNT>;

export const CANDLE_INTERVALS = ["1s", "5s", "1m", "5m", "15m", "1h", "4h", "1d"] as const;
export type CandleInterval = (typeof CANDLE_INTERVALS)[number];

export const CANDLE_INTERVAL_SECS: Readonly<Record<CandleInterval, number>> = { "1s": 1, "5s": 5, "1m": 60, "5m": 300, "15m": 900, "1h": 3_600, "4h": 14_400, "1d": 86_400 };

export const CANDLES_MAX = 5_000;

export const CANDLE_BUNDLE_LIMIT = 300;

export const CANDLE = object({
  time: int,
  open: nullable(decimal),
  high: nullable(decimal),
  low: nullable(decimal),
  close: nullable(decimal),
  openX64: big,
  highX64: big,
  lowX64: big,
  closeX64: big,
  openUsd: nullable(decimal),
  highUsd: nullable(decimal),
  lowUsd: nullable(decimal),
  closeUsd: nullable(decimal),
  volumeQuoteRaw: big,
  volumeUsd: nullable(decimal),
  volumeTokensRaw: big,
  volumeBuyQuoteRaw: big,
  volumeSellQuoteRaw: big,
  buys: int,
  sells: int,
});
export type Candle = Infer<typeof CANDLE>;

const USD_RATE = object({ centsX64: nullable(big), source: nullable(str), ageS: nullable(int) });

export const CANDLES = object({
  indexed,
  launch: str,
  token: str,
  quote: str,
  interval: str,
  seconds: int,
  from: int,
  to: int,
  quietBuckets: oneOf("absent"),
  usd: USD_RATE,
  candles: array(CANDLE),
});
export type Candles = Infer<typeof CANDLES>;

export const CANDLE_BUNDLE = object({
  indexed,
  launch: str,
  token: str,
  quote: str,
  quietBuckets: oneOf("absent"),
  usd: USD_RATE,
  limit: int,
  pages: record(object({ interval: str, seconds: int, from: int, to: int, candles: array(CANDLE) })),
});
export type CandleBundle = Infer<typeof CANDLE_BUNDLE>;

export const HIGH = object({ timestamp: int, block: int, txnId: nullable(str), txnIndex: int, eventIndex: int, priceX64: big, supply: big, marketCap: big });
export type High = Infer<typeof HIGH>;

export const HIGHS = object({ indexed, launch: str, first: nullable(HIGH), highs: array(HIGH), more: bool });
export type Highs = Infer<typeof HIGHS>;

export const TRADER_WINDOWS = ["1h", "24h", "7d", "all"] as const;
export type TraderWindow = (typeof TRADER_WINDOWS)[number];

export const LAUNCH_TRADERS = object({
  indexed,
  launch: str,
  window: oneOf(...TRADER_WINDOWS),
  traders: array(object({ trader: str, volume: big, bought: big, sold: big, spent: big, received: big, buys: int, sells: int, lastTimestamp: int })),
});
export type LaunchTraders = Infer<typeof LAUNCH_TRADERS>;

export const QUOTE_CHAIN = object({ indexed, launch: str, chain: array(object({ launch: str, token: str, quote: str })) });
export type QuoteChain = Infer<typeof QUOTE_CHAIN>;

export const DEV_TRADES = object({ indexed, launch: str, creator: str, trades: array(TRADE) });
export type DevTrades = Infer<typeof DEV_TRADES>;

export const BURNS = object({ indexed, launch: str, burns: array(object({ amount: big, block: int, timestamp: int })), program: array(BURN), wallet: array(BURN) });
export type Burns = Infer<typeof BURNS>;

const LOCK_HOLDER = object({ wallet: str, place: nullable(int) });

export const TEAM_BUY_LOCK = object({ kind: oneOf("teamBuy"), tokens: big, createdAt: int, unlocked: bool, unlockedAt: nullable(int), unlockAt: nullable(int), locked: big, claimed: big, holders: array(LOCK_HOLDER) });
export type TeamBuyLock = Infer<typeof TEAM_BUY_LOCK>;
export const LAUNCH_LOCKS = object({ indexed, launch: str, locks: array(TEAM_BUY_LOCK) });
export type LaunchLocks = Infer<typeof LAUNCH_LOCKS>;

export const MARK_ROLES = ["creator", "teamSlot", "creatorRef", "top10"] as const;
export type MarkRole = (typeof MARK_ROLES)[number];

export const MARK = object({ id: str, ...TRADE_FIELDS, who: array(oneOf(...MARK_ROLES)) });
export type Mark = Infer<typeof MARK>;

export const MARKS = object({ indexed, launch: str, trades: array(MARK), next: nullable(str) });
export type Marks = Infer<typeof MARKS>;

export const PRICE = object({
  indexed,
  launch: str,
  phase: oneOf("bonding", "stacks", "tail"),
  priceX64: big,
  livePriceX64: big,
  floorX64: big,
  backedPpm: int,
  bidTopX64: nullable(big),
  lastTrade: nullable(object({ priceAfterX64: big, timestamp: int, block: int })),
  headTimestamp: nullable(int),
  ageS: nullable(int),
  quoteUsd: USD_RATE,
  priceUsd: nullable(decimal),
});
export type Price = Infer<typeof PRICE>;

export const WALLET_TRADES = object({ indexed, wallet: str, trades: array(PAGED_TRADE), next: nullable(str) });
export type WalletTrades = Infer<typeof WALLET_TRADES>;

export const POSITIONS = object({ indexed, wallet: str, positions: array(object({ launch: str, token: str, quote: str, tokens: big })) });
export type Positions = Infer<typeof POSITIONS>;

export const WALLET_LOCKS = object({
  indexed,
  wallet: str,
  locks: array(object({ launch: str, token: str, kind: oneOf("teamBuy"), direction: oneOf("incoming", "outgoing"), tokens: big, place: nullable(int), createdAt: int, unlocked: bool, unlockedAt: nullable(int), unlockAt: nullable(int), locked: big, claimed: big, holders: array(LOCK_HOLDER) })),
});
export type WalletLocks = Infer<typeof WALLET_LOCKS>;

const AT = { timestamp: int, block: int };

export const INBOX_CLAIMED = object({ ...AT, kind: oneOf("claimed"), launch: str, quote: str, types: int, amount: big, holder: big, txnId: nullable(str) });

export const INBOX_GRADUATED = object({ ...AT, kind: oneOf("graduated"), launch: str, priceX64: big, txnId: nullable(str) });

export const INBOX_SLOT = object({ ...AT, kind: oneOf("slot"), launch: str, place: int });

export const INBOX_DEV_BURN = object({ ...BURN_FIELDS, kind: oneOf("devBurn"), launch: str });

export const INBOX_DEV_BUY = object({ ...AT, kind: oneOf("devBuy"), launch: str, wallet: str, role: DEV_BUY_ROLE, count: int, quoteIn: big, tokens: big, txnId: nullable(str) });

export type InboxItem = Infer<typeof INBOX_CLAIMED> | Infer<typeof INBOX_GRADUATED> | Infer<typeof INBOX_SLOT> | Infer<typeof INBOX_DEV_BURN> | Infer<typeof INBOX_DEV_BUY>;
export const INBOX_KINDS = ["claimed", "graduated", "slot", "devBurn", "devBuy"] as const;

const INBOX_BY_KIND = { claimed: INBOX_CLAIMED, graduated: INBOX_GRADUATED, slot: INBOX_SLOT, devBurn: INBOX_DEV_BURN, devBuy: INBOX_DEV_BUY };

export function inboxItem(v: unknown, p: string): InboxItem | null {
  const kind = typeof v === "object" && v !== null ? (v as { kind?: unknown }).kind : undefined;
  const d = typeof kind === "string" && Object.hasOwn(INBOX_BY_KIND, kind) ? INBOX_BY_KIND[kind as keyof typeof INBOX_BY_KIND] : null;
  if (d === null) {
    str(kind, `${p}.kind`);
    return null;
  }
  return d(v, p) as InboxItem;
}

export const INBOX = object({ indexed, wallet: str, items: (v: unknown, p: string) => array(inboxItem)(v, p).filter((x): x is InboxItem => x !== null) });
export type Inbox = Infer<typeof INBOX>;

export const CLAIM_ROW = object({
  block: int,
  timestamp: int,
  txnId: nullable(str),
  sender: str,
  launch: str,
  quote: str,
  types: int,
  creator: big,
  creatorRef: big,
  teamSlot: big,
  traderRef: big,
  holder: big,
  extra: big,
  amount: big,
  teamBuy: big,
});
export type ClaimRow = Infer<typeof CLAIM_ROW>;

export const CLAIM_HISTORY = object({ indexed, wallet: str, claims: array(CLAIM_ROW) });
export type ClaimHistory = Infer<typeof CLAIM_HISTORY>;

const QUOTE_TOTAL = object({ quote: str, total: big, types: int });

export const CLAIM_VIEW = object({
  indexed,
  claims: object({
    wallet: str,
    atBlock: nullable(int),
    quotes: array(QUOTE_TOTAL),
    totals: record(big),
    launches: optional(array(object({
      launch: str,
      quoteMint: str,
      places: array(int),
      types: int,
      owed: object({ creator: big, creatorRef: big, teamSlot: big }),
      tokens: object({ teamBuy: big, shareBought: big, teamBuyLockedLot: big }),
    }))),
    refOwed: optional(array(object({ account: str, quoteMint: str, owed: big }))),
    pendingRollUp: optional(array(object({ traderRef: str, quoteMint: str, amount: big }))),
    owed: optional(array(object({ quote: str, creator: big, creatorRef: big, teamSlot: big, traderRef: big, holder: big, teamBuy: big }))),
  }),
});
export type ClaimView = Infer<typeof CLAIM_VIEW>;

export const FIRSTS_ROW = object({ firstActivityAt: nullable(int), firstFloorTradeAt: nullable(int) });
export type Firsts = Infer<typeof FIRSTS_ROW>;
export const FIRSTS = object({ indexed, wallet: str, firstActivityAt: nullable(int), firstFloorTradeAt: nullable(int) });
export const FIRSTS_MANY = object({ indexed, wallets: record(FIRSTS_ROW) });

const PER_QUOTE = array(object({ quote: str, amount: big }));

export const PEOPLE = object({
  indexed,
  wallet: str,
  people: array(object({ wallet: str, chain: str, roles: array(object({ kind: oneOf("trader", "creator"), tier: int })), earned: PER_QUOTE, teamMilestone: PER_QUOTE })),
});
export type People = Infer<typeof PEOPLE>;

export const ROLES = object({
  indexed,
  wallet: str,
  roles: object({
    traderRefOf: array(object({ quote: str, trader: str, tier: int })),
    registered: array(object({ launch: str, roles: int })),
    refOwedIn: array(str),
    payoutOwedIn: array(str),
    creatorOf: array(str),
    creatorRefOf: array(object({ launch: str, tier: int })),
    teamSlotOf: array(object({ launch: str, place: int })),

    handleSlotOf: array(object({ launch: str, place: int, handle: str })),
    holdings: array(object({ launch: str, token: str, amount: big })),
  }),
  claimed: record(big),
});
export type Roles = Infer<typeof ROLES>;

export const HANDLE_PLACES = object({
  indexed,
  handle: str,
  places: array(object({ launch: str, place: int })),
});
export type HandlePlaces = Infer<typeof HANDLE_PLACES>;

export const TRADER_BOARD = object({
  indexed,
  window: oneOf(...TRADER_WINDOWS),
  traders: array(object({ trader: str, volumeUsd: decimal, trades: int, launches: int, volumeByQuote: record(big), tradesByQuote: record(int) })),
});
export type TraderBoard = Infer<typeof TRADER_BOARD>;

export const LAUNCHER_BOARD = object({
  indexed,
  launchers: array(object({ creator: str, launches: int, graduated: int, volumeUsd: decimal, liquidityUsd: decimal, bestLaunch: nullable(str), byQuote: record(object({ volume: big, liquidity: big, launches: int })) })),
});
export type LauncherBoard = Infer<typeof LAUNCHER_BOARD>;
