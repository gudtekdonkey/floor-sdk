/**
 * The DEX adapter's pure mapping: FLOOR's indexed trades (the indexer's chain-wide `/events` rows) as GeckoTerminal's
 * "Integration API Standards v0.1" events, which DEX Screener reads too. No network and no environment: every function is
 * pure, so an indexer of its own can use the same pair model and answer the same numbers as FLOOR's `/api/dex`.
 *
 * - An invalid answer halts a screener's indexing, so nothing here guesses. Amounts are exact decimal strings of the raw
 *   units (`decimalize`), a price is an exact bigint ratio to `PRICE_SIG` significant digits (`ratioString`), never a
 *   float, and an event that cannot be stated exactly (a zero amount, a zero price) is dropped by `mapEvents` and
 *   counted, never served as a guess.
 * - A pair is immutable: asset0 is always the launch token and asset1 always the quote (the native coin is the zero
 *   address, as in a v4 currency0). A swap is exactly `asset0In + asset1Out` (a sell) or `asset1In + asset0Out` (a buy).
 * - Two pairs per launch: the sale (its id the FloorLaunch address; every sale fill) and, once graduated, the pool (its id
 *   the v4 pool id on EVM, the Carpenter pool account on Solana; every pool trade and liquidity change).
 */

/** A price's significant digits: past a uint256 sqrt's own precision is noise; under it, a rounding a reader can see. */
export const PRICE_SIG = 18;

/** `raw / 10^decimals`, exact: `1500000` at 6 → `1.5`; no exponent, no trailing zeros, `0` for zero. */
export function decimalize(raw: bigint, decimals: number): string {
  if (!Number.isInteger(decimals) || decimals < 0 || decimals > 77) throw new RangeError('decimals');
  const neg = raw < 0n;
  const s = (neg ? -raw : raw).toString().padStart(decimals + 1, '0');
  const int = s.slice(0, s.length - decimals);
  const frac = s.slice(s.length - decimals).replace(/0+$/, '');
  return `${neg ? '-' : ''}${int}${frac ? `.${frac}` : ''}`;
}

/** `num / den` (both positive) to `sig` significant digits, truncated, as an exact decimal string; `null` when not positive. */
export function ratioString(num: bigint, den: bigint, sig = PRICE_SIG): string | null {
  if (num <= 0n || den <= 0n) return null;
  const intDigits = (num / den).toString();
  /* Decimal places so the answer carries `sig` significant digits: past the integer part, or past the leading zeros of a
     fraction (log10 of den / num, rounded up). */
  let scale: number;
  if (num >= den) scale = Math.max(0, sig - intDigits.length);
  else scale = sig + (den / num).toString().length;
  const q = (num * 10n ** BigInt(scale)) / den;
  if (q === 0n) return null;
  return decimalize(q, scale);
}

/** The token's price in its quote, decimalized, from a square-root price (currency1 per currency0, raw units): a v4
 *  `sqrtPriceX96` (`bits` 96, the default) or a Carpenter `sqrtPriceX64` (`bits` 64 — Q64.64, never through the X96 scale). */
export function priceFromSqrt(sqrtPriceX96: bigint, quoteIsCurrency1: boolean, tokenDecimals: number, quoteDecimals: number, bits: 96 | 64 = 96): string | null {
  if (sqrtPriceX96 <= 0n) return null;
  const sq = sqrtPriceX96 * sqrtPriceX96;
  const Q192 = 1n << BigInt(2 * bits);
  /* raw quote per raw token: sq / Q192 when the quote is currency1, its inverse when it is currency0; then × 10^(td − qd). */
  let num = quoteIsCurrency1 ? sq : Q192;
  let den = quoteIsCurrency1 ? Q192 : sq;
  const shift = tokenDecimals - quoteDecimals;
  if (shift >= 0) num *= 10n ** BigInt(shift);
  else den *= 10n ** BigInt(-shift);
  return ratioString(num, den);
}

/** Raw amounts in the POOL's currency order (currency0, currency1) — oriented to (token, quote) by `mapEvents` alone. */
export type Pair01 = readonly [bigint, bigint];

/**
 * A trade's fees in the POOL's currency order, raw (GeckoTerminal's `fees0In` / `fees1In` / `fees0Out` /
 * `fees1Out`): `in` taken from what the trader paid in before the curve, `out` from what the curve paid out.
 */
export interface Fees01 { in0: bigint; in1: bigint; out0: bigint; out1: bigint }

/** One pool trade off the indexer's `/events` (`swapJson` + `txIndex`), the fields this adapter reads. */
export interface IxSwap {
  blockNumber: number; logIndex: number; txIndex: number; txHash: string; timestamp: number;
  token: string; side: 'buy' | 'sell';
  quoteAmount: bigint; tokenAmount: bigint; quoteIsCurrency1: boolean; sqrtPriceX96: bigint;
  trader: string | null; txFrom: string;
  /** The square root's width: 96 (EVM, the default) or 64 (Carpenter's Q64.64). */
  sqrtBits?: 96 | 64;
  /** The POOL the trade ran on — the v4 PoolId (EVM, lower-case) or the Carpenter pool
   *  account (Solana) — held to the launch's own (`PairInfo.poolPairId`). */
  pool?: string;
  /** THE RESERVES after the trade, currency order, raw; `null` = unknown (left out, never 0). EVM: the in-range
   *  VIRTUAL reserves off the `Swap` log's own `liquidity` and `sqrtPriceX96` (`virtualReserves`); Solana: the pool's two
   *  VAULTS after the transaction. */
  reserves01?: Pair01 | null;
  /** The trade's fees, currency order; `null` = not stateable exactly (left out). */
  fees01?: Fees01 | null;
}

/** One sale fill off the indexer's `/events` (`saleFillJson` + `txIndex`, `launch`, `token`, the running totals). */
export interface IxFill {
  blockNumber: number; logIndex: number; txIndex: number; txHash: string; timestamp: number;
  launch: string; token: string; side: 'buy' | 'sell';
  quoteAmount: bigint; tokenAmount: bigint; trader: string; soldTotal: bigint; raisedTotal: bigint;
  /** The fill's FLOOR Platform fee, raw quote — on TOP of a buy's price (taken from what the buyer paid in),
   *  OUT of a sell-back's gross (taken from what the sale paid out). `null` = not read. */
  fee?: bigint | null;
}

/**
 * One POOL liquidity change (the DEX adapter's `join` / `exit`): a position opened, grown, shrunk or
 * closed — the HOOK's own positions included (protocol liquidity is the pool's liquidity) — or a donation. Amounts are the
 * PRINCIPAL the change moved, currency order, raw, unsigned; `null` = not stated (the event is dropped and counted).
 */
export interface IxLiquidity {
  blockNumber: number; logIndex: number; txIndex: number; txHash: string; timestamp: number;
  token: string; pool: string; kind: 'join' | 'exit';
  amount01: Pair01 | null;
  maker: string;
  /** The pool's reserves after the change (`IxSwap.reserves01`' rule per family); `null` unknown. */
  reserves01: Pair01 | null;
  /** The pool's orientation where the row carries it (EVM); Solana's comes from `PairInfo`. */
  quoteIsCurrency1?: boolean;
}

/** What a launch's pairs need, keyed by its token (lower-cased): present only for a launch that is shown (default-deny). */
export interface PairInfo {
  /** The sale pair's id (the FloorLaunch address, as served). */
  saleId: string;
  /** The pool pair's id, `null` before graduation: the v4 POOL ID (EVM, lower-case 0x + 64 hex) or the Carpenter POOL
   *  account (Solana, base58). */
  poolPairId: string | null;
  tokenDecimals: number;
  quoteDecimals: number;
  saleTokens: bigint;
  /** The pool's orientation where the swap row does not carry it (Solana: off the pool's own mints). */
  quoteIsCurrency1?: boolean;
}

export type FeeMetadata = { fees0In?: string; fees1In?: string; fees0Out?: string; fees1Out?: string };

export interface DexSwapEvent {
  block: { blockNumber: number; blockTimestamp: number };
  eventType: 'swap';
  txnId: string;
  txnIndex: number;
  eventIndex: number;
  maker: string;
  pairId: string;
  asset0In?: string; asset1In?: string; asset0Out?: string; asset1Out?: string;
  priceNative: string;
  reserves?: { asset0: string; asset1: string };
  /** GeckoTerminal's per-swap fees, decimalized, each leg only where non-zero; the key only where one is. */
  metadata?: FeeMetadata;
}

/** GeckoTerminal's `JoinExitEvent`: `amount0` the token, `amount1` the quote, decimalized. */
export interface DexJoinExitEvent {
  block: { blockNumber: number; blockTimestamp: number };
  eventType: 'join' | 'exit';
  txnId: string;
  txnIndex: number;
  eventIndex: number;
  maker: string;
  pairId: string;
  amount0: string;
  amount1: string;
  reserves?: { asset0: string; asset1: string };
}

export type DexEvent = DexSwapEvent | DexJoinExitEvent;

const POSITIVE = (v: string | undefined) => v !== undefined && /^[0-9]+(\.[0-9]+)?$/.test(v) && /[1-9]/.test(v);
const AMOUNT = (v: string | undefined) => v !== undefined && /^[0-9]+(\.[0-9]+)?$/.test(v);

/** Is `e` exactly one of the two swap shapes, every amount positive and the price positive? */
export function isSwapShape(e: DexEvent): boolean {
  if (e.eventType !== 'swap') return false;
  const sell = POSITIVE(e.asset0In) && POSITIVE(e.asset1Out) && e.asset1In === undefined && e.asset0Out === undefined;
  const buy = POSITIVE(e.asset1In) && POSITIVE(e.asset0Out) && e.asset0In === undefined && e.asset1Out === undefined;
  return (sell || buy) && POSITIVE(e.priceNative);
}

/** Is `e` a join / exit with both amounts stated and at least one positive (a fee-only poke moves no principal)? */
export function isJoinExitShape(e: DexEvent): boolean {
  if (e.eventType !== 'join' && e.eventType !== 'exit') return false;
  return AMOUNT(e.amount0) && AMOUNT(e.amount1) && (POSITIVE(e.amount0) || POSITIVE(e.amount1));
}

/**
 * A v4 / Carpenter pool's IN-RANGE VIRTUAL reserves, currency order, raw: `L / √P` and `L · √P` with √P in
 * Q64.96 (`bits` 96) or Q64.64 (`bits` 64), rounded down — the amounts a constant-product pool of the same liquidity would
 * hold at this price (their ratio IS the price). `null` for a zero liquidity or price (nothing to state).
 */
export function virtualReserves(liquidity: bigint, sqrtPrice: bigint, bits: 96 | 64 = 96): Pair01 | null {
  if (liquidity <= 0n || sqrtPrice <= 0n) return null;
  const Q = 1n << BigInt(bits);
  return [(liquidity * Q) / sqrtPrice, (liquidity * sqrtPrice) / Q];
}

const amounts = (side: 'buy' | 'sell', token: string, quote: string) =>
  side === 'buy' ? { asset1In: quote, asset0Out: token } : { asset0In: token, asset1Out: quote };

/** Currency order → (token, quote): the token is currency0 when the quote is currency1. */
const orient = (v: Pair01, q1: boolean): [bigint, bigint] => (q1 ? [v[0], v[1]] : [v[1], v[0]]);

const reservesOf = (r: Pair01 | null | undefined, q1: boolean, p: PairInfo) => {
  if (r === null || r === undefined || r[0] < 0n || r[1] < 0n) return {};
  const [t, q] = orient(r, q1);
  return { reserves: { asset0: decimalize(t, p.tokenDecimals), asset1: decimalize(q, p.quoteDecimals) } };
};

/** The fee metadata, asset0 the token and asset1 the quote, each leg only where non-zero; `{}` where nothing is stated. */
function feeMetadata(f: Fees01 | null | undefined, q1: boolean, p: PairInfo): { metadata?: FeeMetadata } {
  if (f === null || f === undefined) return {};
  const [tin, qin] = orient([f.in0, f.in1], q1);
  const [tout, qout] = orient([f.out0, f.out1], q1);
  const m: FeeMetadata = {};
  if (tin > 0n) m.fees0In = decimalize(tin, p.tokenDecimals);
  if (qin > 0n) m.fees1In = decimalize(qin, p.quoteDecimals);
  if (tout > 0n) m.fees0Out = decimalize(tout, p.tokenDecimals);
  if (qout > 0n) m.fees1Out = decimalize(qout, p.quoteDecimals);
  return Object.keys(m).length > 0 ? { metadata: m } : {};
}

/**
 * The events of one block range, sorted by (block, txnIndex, eventIndex). `info(token)` answers a SHOWN launch's pair info,
 * `undefined` for one that is not shown — its events are left out (default-deny), as the grid leaves it out. An event
 * that cannot be stated exactly is dropped and counted in `dropped`.
 */
export function mapEvents(swaps: readonly IxSwap[], fills: readonly IxFill[], info: (token: string) => PairInfo | undefined, key: (id: string) => string = (id) => id.toLowerCase(), liquidity: readonly IxLiquidity[] = []): { events: DexEvent[]; dropped: number } {
  const events: DexEvent[] = [];
  let dropped = 0;
  for (const s of swaps) {
    const p = info(key(s.token));
    /* Every FLOOR pool trade the indexer serves is its launch's ONE pool (`swap.token`); the row's pool must be it. */
    if (!p || p.poolPairId === null) continue;
    if (s.pool !== undefined && key(s.pool) !== key(p.poolPairId)) { dropped++; continue; }
    const q1 = p.quoteIsCurrency1 ?? s.quoteIsCurrency1;
    const price = priceFromSqrt(s.sqrtPriceX96, q1, p.tokenDecimals, p.quoteDecimals, s.sqrtBits ?? 96);
    const e: DexSwapEvent = {
      block: { blockNumber: s.blockNumber, blockTimestamp: s.timestamp }, eventType: 'swap',
      txnId: s.txHash, txnIndex: s.txIndex, eventIndex: s.logIndex, maker: s.trader ?? s.txFrom, pairId: p.poolPairId,
      ...amounts(s.side, decimalize(s.tokenAmount, p.tokenDecimals), decimalize(s.quoteAmount, p.quoteDecimals)),
      priceNative: price ?? '0',
      ...reservesOf(s.reserves01, q1, p),
      ...feeMetadata(s.fees01, q1, p),
    };
    if (isSwapShape(e)) events.push(e); else dropped++;
  }
  for (const f of fills) {
    const p = info(key(f.token));
    if (!p || key(p.saleId) !== key(f.launch)) continue;
    /* The fill's own price: quote over tokens, both decimalized — (q / 10^qd) / (t / 10^td). */
    const price = ratioString(f.quoteAmount * 10n ** BigInt(p.tokenDecimals), f.tokenAmount * 10n ** BigInt(p.quoteDecimals));
    const left = p.saleTokens - f.soldTotal;
    /* the FLOOR Platform fee: on top of a buy (in), out of a sell-back's gross (out) — the quote leg. */
    const fee = f.fee ?? 0n;
    const e: DexSwapEvent = {
      block: { blockNumber: f.blockNumber, blockTimestamp: f.timestamp }, eventType: 'swap',
      txnId: f.txHash, txnIndex: f.txIndex, eventIndex: f.logIndex, maker: f.trader, pairId: p.saleId,
      ...amounts(f.side, decimalize(f.tokenAmount, p.tokenDecimals), decimalize(f.quoteAmount, p.quoteDecimals)),
      priceNative: price ?? '0',
      ...(left >= 0n && f.raisedTotal >= 0n ? { reserves: { asset0: decimalize(left, p.tokenDecimals), asset1: decimalize(f.raisedTotal, p.quoteDecimals) } } : {}),
      ...(fee > 0n ? { metadata: f.side === 'buy' ? { fees1In: decimalize(fee, p.quoteDecimals) } : { fees1Out: decimalize(fee, p.quoteDecimals) } } : {}),
    };
    if (isSwapShape(e)) events.push(e); else dropped++;
  }
  for (const l of liquidity) {
    const p = info(key(l.token));
    if (!p || p.poolPairId === null) continue;
    if (key(l.pool) !== key(p.poolPairId) || l.amount01 === null) { dropped++; continue; }
    const q1 = p.quoteIsCurrency1 ?? l.quoteIsCurrency1;
    if (q1 === undefined) { dropped++; continue; }
    const [t, q] = orient(l.amount01, q1);
    const e: DexJoinExitEvent = {
      block: { blockNumber: l.blockNumber, blockTimestamp: l.timestamp }, eventType: l.kind,
      txnId: l.txHash, txnIndex: l.txIndex, eventIndex: l.logIndex, maker: l.maker, pairId: p.poolPairId,
      amount0: decimalize(t, p.tokenDecimals), amount1: decimalize(q, p.quoteDecimals),
      ...reservesOf(l.reserves01, q1, p),
    };
    if (isJoinExitShape(e)) events.push(e); else dropped++;
  }
  events.sort((a, b) => a.block.blockNumber - b.block.blockNumber || a.txnIndex - b.txnIndex || a.eventIndex - b.eventIndex);
  return { events, dropped };
}

/* ---------------------------------------------------------------- the indexer's `/events` body, strictly */

const INT = (v: unknown): number => {
  if (typeof v !== 'number' || !Number.isSafeInteger(v) || v < 0) throw new TypeError('int');
  return v;
};
const BIG = (v: unknown): bigint => {
  if (typeof v !== 'string' || !/^(0|[1-9][0-9]{0,77})$/.test(v)) throw new TypeError('uint');
  return BigInt(v);
};
const HEX = (v: unknown, bytes: number): string => {
  if (typeof v !== 'string' || !new RegExp(`^0x[0-9a-fA-F]{${bytes * 2}}$`).test(v)) throw new TypeError('hex');
  return v.toLowerCase();
};
const SIDE = (v: unknown): 'buy' | 'sell' => {
  if (v !== 'buy' && v !== 'sell') throw new TypeError('side');
  return v;
};

const BIG_OR_NULL = (v: unknown): bigint | null => (v === null ? null : BIG(v));
const SIGNED = (v: unknown): bigint => {
  if (typeof v !== 'string' || !/^-?(0|[1-9][0-9]{0,77})$/.test(v)) throw new TypeError('int string');
  return BigInt(v);
};
const BOOL_ = (v: unknown): boolean => {
  if (typeof v !== 'boolean') throw new TypeError('bool');
  return v;
};

/**
 * An EVM trade's fees, currency order: the hook's whole take (`hookFee`, its paired `HookFee` log — always the
 * QUOTE leg: a buy's off the quote IN, a sell's off the quote OUT). The pool's own LP fee is
 * 0 on a FLOOR pool (the hook's dynamic-fee override); a `Swap` that logs a non-zero `fee` (pips) states no exact LP amount,
 * so its fees are `null` (left out), never a guess.
 */
export function evmFees(side: 'buy' | 'sell', quoteIsCurrency1: boolean, hookFee: bigint, lpFeePips: number): Fees01 | null {
  if (lpFeePips !== 0) return null;
  const f: Fees01 = { in0: 0n, in1: 0n, out0: 0n, out1: 0n };
  const leg = side === 'buy' ? 'in' : 'out';
  f[`${leg}${quoteIsCurrency1 ? 1 : 0}` as keyof Fees01] = hookFee;
  return f;
}

/** The indexer's `/events` body, decoded; a row that breaks the contract throws (the whole answer is refused, never part). */
export function decodeEventsBody(body: unknown): { swaps: IxSwap[]; fills: IxFill[]; liquidity: IxLiquidity[] } {
  const b = body as { swaps?: unknown; fills?: unknown; liquidity?: unknown } | null;
  if (b === null || typeof b !== 'object' || !Array.isArray(b.swaps) || !Array.isArray(b.fills) || !Array.isArray(b.liquidity)) throw new TypeError('events body');
  const swaps = b.swaps.map((r: Record<string, unknown>): IxSwap => {
    const side = SIDE(r.side);
    const q1 = BOOL_(r.quoteIsCurrency1);
    const sqrt = BIG(r.sqrtPriceX96);
    return {
      blockNumber: INT(r.blockNumber), logIndex: INT(r.logIndex), txIndex: INT(r.txIndex), txHash: HEX(r.txHash, 32), timestamp: INT(r.timestamp),
      token: HEX(r.token, 20), side,
      quoteAmount: BIG(r.quoteAmount), tokenAmount: BIG(r.tokenAmount),
      quoteIsCurrency1: q1, sqrtPriceX96: sqrt,
      trader: r.trader === null ? null : HEX(r.trader, 20), txFrom: HEX(r.txFrom, 20),
      pool: HEX(r.poolId, 32),
      /* the in-range VIRTUAL reserves off the log's own liquidity and price */
      reserves01: virtualReserves(BIG(r.liquidity), sqrt, 96),
      fees01: evmFees(side, q1, BIG(r.hookFee), INT(r.fee)),
    };
  });
  const fills = b.fills.map((r: Record<string, unknown>): IxFill => ({
    blockNumber: INT(r.blockNumber), logIndex: INT(r.logIndex), txIndex: INT(r.txIndex), txHash: HEX(r.txHash, 32), timestamp: INT(r.timestamp),
    launch: HEX(r.launch, 20), token: HEX(r.token, 20), side: SIDE(r.side),
    quoteAmount: BIG(r.quoteAmount), tokenAmount: BIG(r.tokenAmount), trader: HEX(r.trader, 20),
    soldTotal: BIG(r.soldTotal), raisedTotal: BIG(r.raisedTotal),
    fee: BIG(r.teamFee),
  }));
  /* every pool liquidity change: `add` / `donate` a join, `remove` an exit; the
     maker the HOOK for its own positions, else the transaction's sender (the position manager is never the LP). */
  const liquidity = b.liquidity.map((r: Record<string, unknown>): IxLiquidity => {
    const kind = r.kind;
    if (kind !== 'add' && kind !== 'remove' && kind !== 'donate') throw new TypeError('liquidity kind');
    const a0 = BIG_OR_NULL(r.amount0);
    const a1 = BIG_OR_NULL(r.amount1);
    const sqrt = BIG_OR_NULL(r.sqrtPriceX96);
    const liq = BIG_OR_NULL(r.liquidity);
    return {
      blockNumber: INT(r.blockNumber), logIndex: INT(r.logIndex), txIndex: INT(r.txIndex), txHash: HEX(r.txHash, 32), timestamp: INT(r.timestamp),
      token: HEX(r.token, 20), pool: HEX(r.poolId, 32), kind: kind === 'remove' ? 'exit' : 'join',
      amount01: a0 === null || a1 === null ? null : [a0, a1],
      maker: BOOL_(r.isHook) ? HEX(r.sender, 20) : HEX(r.txFrom, 20),
      reserves01: sqrt === null || liq === null ? null : virtualReserves(liq, sqrt, 96),
      quoteIsCurrency1: BOOL_(r.quoteIsCurrency1),
    };
  });
  return { swaps, fills, liquidity };
}

/** Split `[from, to]` into two halves (a busy range the indexer refused); `null` for a single block, which cannot split. */
export function halves(from: number, to: number): [[number, number], [number, number]] | null {
  if (to <= from) return null;
  const mid = Math.floor((from + to) / 2);
  return [[from, mid], [mid + 1, to]];
}

/* ---------------------------------------------------------------- the Carpenter indexer's `/events` body, strictly */

const B58 = (v: unknown): string => {
  if (typeof v !== 'string' || !/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(v)) throw new TypeError('base58');
  return v;
};
const SIG = (v: unknown): string => {
  if (typeof v !== 'string' || !/^[1-9A-HJ-NP-Za-km-z]{64,90}$/.test(v)) throw new TypeError('signature');
  return v;
};
const BOOL = (v: unknown): boolean => {
  if (typeof v !== 'boolean') throw new TypeError('bool');
  return v;
};

const SPLIT = (v: unknown): bigint => {
  const o = v as Record<string, unknown> | null;
  if (o === null || typeof o !== 'object') throw new TypeError('fee split');
  return BIG(o.skim) + BIG(o.creator) + BIG(o.holder) + BIG(o.platform) + BIG(o.team);
};
const U64_OR_NULL = (v: unknown): bigint | null => (v === null ? null : BIG(v));
const pair01 = (a: bigint | null, b: bigint | null): Pair01 | null => (a === null || b === null ? null : [a, b]);

/**
 * A Carpenter trade's fees, currency order, every part the `Swapped` event carries: the hook's five-part split per
 * currency (`feeSplit0` / `feeSplit1`: skim, creator, holder, platform, team), the Carpenter Platform fee
 * (`protocolFeeTotal` in `protocolFeeCurrency`) and the LPs' fee (`lpFeeTotal`, in the INPUT currency). A fee in the trade's
 * input currency is `in`, in its output currency `out`. `null` where the split is not known (`feeSplitKnown` false): never a
 * partial figure.
 */
export function solanaFees(r: Record<string, unknown>): Fees01 | null {
  if (!BOOL(r.feeSplitKnown)) return null;
  const zeroForOne = BOOL(r.zeroForOne);
  const c: [bigint, bigint] = [SPLIT(r.feeSplit0), SPLIT(r.feeSplit1)];
  const pc = INT(r.protocolFeeCurrency);
  if (pc !== 0 && pc !== 1) throw new TypeError('protocol fee currency');
  c[pc as 0 | 1] += BIG(r.protocolFeeTotal);
  const input = zeroForOne ? 0 : 1;
  c[input] += BIG(r.lpFeeTotal);
  return input === 0 ? { in0: c[0]!, in1: 0n, out0: 0n, out1: c[1]! } : { in0: 0n, in1: c[1]!, out0: c[0]!, out1: 0n };
}

/**
 * Carpenter's `GET /chains/:c/events?fromSlot&toSlot` into the
 * SAME rows the EVM body decodes to, so one mapper states both: the slot is the block, `innerIxIndex` the event index, the
 * signature the transaction id, every id BYTE-EXACT (base58 — never lower-cased); the swap's price is the TRADE's own,
 * `walkSqrtPriceX64 ?? postSqrtPriceX64` (Q64.64, `sqrtBits` 64: where the trader's walk landed,
 * before the hook's nudge moved the pool, as the EVM row's own `Swap` price is; `null` / absent walk = a swap before the
 * upgrade, and only then `post`), its orientation the pool's own (`PairInfo.quoteIsCurrency1`, the row carries none). A range with
 * `undecidedSwaps` above 0 is NOT final (a pool not read yet): the caller serves nothing, never a partial page.
 * every swap carries the pool's two VAULTS after the transaction (`reserve0` /
 * `reserve1`, `null` unknown) and its fees (`solanaFees`); `liquidity` (`LiquidityModified`: a positive delta a join, a
 * negative an exit, a zero-delta poke left out; the principal `|amount − fees|` per currency — the event's caller delta less
 * the fees it also paid) and `donations` (`Donated`, a join) ride beside, the maker the event's `owner` / `donor`.
 */
export function decodeSolanaEventsBody(body: unknown): { swaps: IxSwap[]; fills: IxFill[]; liquidity: IxLiquidity[]; undecided: number } {
  const b = body as { swaps?: unknown; fills?: unknown; liquidity?: unknown; donations?: unknown; undecidedSwaps?: unknown } | null;
  if (b === null || typeof b !== 'object' || !Array.isArray(b.swaps) || !Array.isArray(b.fills) || !Array.isArray(b.liquidity) || !Array.isArray(b.donations)) throw new TypeError('events body');
  const undecided = INT(b.undecidedSwaps);
  const swaps = b.swaps.map((r: Record<string, unknown>): IxSwap => {
    const trader = B58(r.trader);
    return {
      blockNumber: INT(r.slot), logIndex: INT(r.innerIxIndex), txIndex: INT(r.txIndex), txHash: SIG(r.signature), timestamp: INT(r.blockTime),
      token: B58(r.mint), side: BOOL(r.isBuy) ? 'buy' : 'sell',
      quoteAmount: BIG(r.quoteAmount), tokenAmount: BIG(r.tokenAmount),
      quoteIsCurrency1: false, sqrtPriceX96: r.walkSqrtPriceX64 === null || r.walkSqrtPriceX64 === undefined ? BIG(r.postSqrtPriceX64) : BIG(r.walkSqrtPriceX64), sqrtBits: 64,
      trader, txFrom: trader,
      pool: B58(r.pool),
      reserves01: pair01(U64_OR_NULL(r.reserve0), U64_OR_NULL(r.reserve1)),
      fees01: solanaFees(r),
    };
  });
  const fills = b.fills.map((r: Record<string, unknown>): IxFill => ({
    blockNumber: INT(r.slot), logIndex: INT(r.innerIxIndex), txIndex: INT(r.txIndex), txHash: SIG(r.signature), timestamp: INT(r.blockTime),
    launch: B58(r.launch), token: B58(r.mint), side: BOOL(r.isBuy) ? 'buy' : 'sell',
    quoteAmount: BIG(r.quoteAmount), tokenAmount: BIG(r.tokenAmount), trader: B58(r.trader),
    soldTotal: BIG(r.sold), raisedTotal: BIG(r.raised),
    fee: r.teamFee === undefined ? null : U64_OR_NULL(r.teamFee),
  }));
  const liquidity: IxLiquidity[] = [];
  for (const r of b.liquidity as Record<string, unknown>[]) {
    const delta = SIGNED(r.liquidityDelta);
    const at = { blockNumber: INT(r.slot), logIndex: INT(r.innerIxIndex), txIndex: INT(r.txIndex), txHash: SIG(r.signature), timestamp: INT(r.blockTime) };
    const principal = (amount: bigint, fees: bigint) => { const v = amount - fees; return v < 0n ? -v : v; };
    const row: IxLiquidity = {
      ...at, token: B58(r.mint), pool: B58(r.pool), kind: delta > 0n ? 'join' : 'exit',
      amount01: [principal(SIGNED(r.amount0), BIG(r.fees0)), principal(SIGNED(r.amount1), BIG(r.fees1))],
      maker: B58(r.owner),
      reserves01: pair01(U64_OR_NULL(r.reserve0), U64_OR_NULL(r.reserve1)),
    };
    if (delta !== 0n) liquidity.push(row);
  }
  for (const r of b.donations as Record<string, unknown>[]) {
    liquidity.push({
      blockNumber: INT(r.slot), logIndex: INT(r.innerIxIndex), txIndex: INT(r.txIndex), txHash: SIG(r.signature), timestamp: INT(r.blockTime),
      token: B58(r.mint), pool: B58(r.pool), kind: 'join',
      amount01: [BIG(r.amount0), BIG(r.amount1)],
      maker: B58(r.donor),
      reserves01: pair01(U64_OR_NULL(r.reserve0), U64_OR_NULL(r.reserve1)),
    });
  }
  return { swaps, fills, liquidity, undecided };
}

/** Carpenter's widest range (`span` past it is its 400): the Solana `/events` walk reads windows of this many slots. */
export const SOLANA_EVENTS_SPAN = 2_000;
