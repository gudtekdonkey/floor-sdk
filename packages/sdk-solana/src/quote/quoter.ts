/**
 * The typed quoter over FLOOR's `floor_quote_bg.wasm` (`rust/crates/floor-quote-wasm`): the AMM walk with the FLOOR
 * hook's callbacks (`quoteSwap`, with FLOOR's fee breakdown and the in-swap bid re-place), the liquidity / position /
 * tick helpers, and FLOOR's own four — the sale's buy and sell-back, the holder tally's claimable and the three referral
 * tiers' dues. Each number is the program's own arithmetic compiled to wasm, never a JS copy.
 *
 * The wire:
 * - accounts go in as their RAW BYTES exactly as `getAccountInfo` returns them (discriminator included); tick arrays go
 *   in CONCATENATED, each its 3,888-byte body in a 3,888-byte stride (this wrapper slices a 3,920-byte account); an
 *   empty `bitmap` / `floor_pool` means "not supplied";
 * - integers that can exceed 2^53 cross as decimal strings; ticks, decimals and spacings are numbers;
 * - every export returns `{"ok": …}` or `{"err": "<Name>", …}` (`<Name>` is the program's error name, the
 *   `QuoteError.code`) — except `version` (its object bare) and `holder_claimable` (u64 in and out as a bigint, which
 *   THROWS `Error(name)`).
 * Nothing here computes a number. This module never imports the wasm glue, so tests run it over a fake `QuoteFns`.
 * The quoter is advisory; `simulateTransaction` stays the final check before a wallet signs.
 */

import { QuoteError } from '@floorlaunchpad/carpenter-sdk';

export { QuoteError };

/** Every wasm-bindgen export the wrapper calls (`wasm/floor_quote.d.ts`). */
export interface QuoteFns {
  quote_swap(
    pool_key: Uint8Array,
    pool: Uint8Array,
    bitmap: Uint8Array,
    tick_arrays: Uint8Array,
    floor_pool: Uint8Array,
    zero_for_one: boolean,
    amount_specified: string,
    sqrt_price_limit_x64: string,
    supply1: string,
  ): string;
  amounts_for_liquidity(sqrt_price_x64: string, tick_current: number, tick_lower: number, tick_upper: number, liquidity_delta: string): string;
  quote_liquidity(sqrt_price_x64: string, tick_lower: number, tick_upper: number, amount0_max: string, amount1_max: string): string;
  fees_owed(pool_key: Uint8Array, pool: Uint8Array, position: Uint8Array, lower_array: Uint8Array, upper_array: Uint8Array): string;
  sqrt_price_at_tick(tick: number): string;
  tick_at_sqrt_price(sqrt_price_x64: string): string;
  tick_at_price_ui(price_ui: number, decimals0: number, decimals1: number): string;
  price_ui_at_tick(tick: number, decimals0: number, decimals1: number): number;
  floor_to_spacing(tick: number, spacing: number): string;
  ceil_to_spacing(tick: number, spacing: number): string;
  sale_buy(launch: Uint8Array, lamports_in: string): string;
  sale_sell_back(launch: Uint8Array, buyer_position: Uint8Array, tokens: string): string;
  /** u64 in/out as bigint; THROWS `Error(name)` (`WrongMint` / `NotHooked` / `QuoteBadAccount`). */
  holder_claimable(book_data: Uint8Array, record_data: Uint8Array, floor_pool_data: Uint8Array, live_balance: bigint): bigint;
  /** `{"ok":{"due1","due2","due3"}}`, decimal strings. */
  referral_tier_dues(floor_pool_data: Uint8Array): string;
  version(): string;
}

// ---- inputs ---------------------------------------------------------------------------------

/** A tick array's 3,888-byte BODY (discriminator ‖ struct): what the wasm takes, in a 3,888-byte stride. */
export const TICK_ARRAY_BODY_LEN = 3_888;
/** The lengths accepted for one tick array: its body, or the 3,920-byte account as `getAccountInfo` returns it. */
export const TICK_ARRAY_ACCOUNT_LENS: readonly number[] = [3_888, 3_920];

function tickArrayBody(a: Uint8Array, what: string): Uint8Array {
  if (!TICK_ARRAY_ACCOUNT_LENS.includes(a.length)) {
    throw new QuoteError('QuoteBadAccount', { account: `${what} is ${a.length} B, not ${TICK_ARRAY_ACCOUNT_LENS.join(' or ')}` });
  }
  return a.subarray(0, TICK_ARRAY_BODY_LEN);
}

/** A 3,920-byte account → its body; any other input unchanged (the wasm judges it). */
function accountToBody(a: Uint8Array): Uint8Array {
  return a.length === 3_920 ? a.subarray(0, TICK_ARRAY_BODY_LEN) : a;
}

/** Concatenate tick arrays as 3,888-byte bodies; any other length is refused `QuoteBadAccount`, naming the index. */
export function concatTickArrays(arrays: readonly Uint8Array[]): Uint8Array {
  const out = new Uint8Array(arrays.length * TICK_ARRAY_BODY_LEN);
  arrays.forEach((a, i) => out.set(tickArrayBody(a, `tick_arrays[${i}]`), i * TICK_ARRAY_BODY_LEN));
  return out;
}

export interface SwapQuoteInput {
  /** The pool's address bytes (32). */
  poolKey: Uint8Array;
  /** `Pool` account data. */
  pool: Uint8Array;
  /** `TickArrayBitmap` account data; omit only when every needed array is supplied. */
  bitmap?: Uint8Array | null;
  /** Every tick array the walk (and the bid re-place) may read; each the 3,920-byte account or its body. */
  tickArrays: readonly Uint8Array[];
  /** `FloorPool` account data on a FLOOR pool (required there); omit on a plain pool. */
  floorPool?: Uint8Array | null;
  zeroForOne: boolean;
  /** v4 sign: `< 0` exact-in (spend `−amountSpecified`), `> 0` exact-out. FLOOR pools refuse exact-out. */
  amountSpecified: bigint;
  /** Omit for "no limit". */
  sqrtPriceLimitX64?: bigint | null;
  /** The base mint's supply (currency 1), handed to FLOOR's callbacks. */
  supply1?: bigint | null;
}

export type HookActionQuote =
  | { kind: 'swap'; zeroForOne: boolean; amountSpecified: bigint; sqrtPriceLimitX64: bigint }
  | { kind: 'modify_position'; salt: bigint; tickLower: number; tickUpper: number; liquidityDelta: bigint };

/** The in-swap bid re-place: what `after_actions` booked, and where the price stood before and after the nudge. */
export interface ReplaceQuote {
  /** Tokens the closed bid held; joined inventory at cost. */
  absorbed: bigint;
  /** Quote the close returned to the vault. */
  ethBack: bigint;
  /** What the nudge spent and got (both `0n` across an empty gap). */
  nudgeSpent: bigint;
  nudgeGot: bigint;
  /** The closed band's lower tick. */
  fromLower: number;
  /** Whether a new band was deployed; when `false` the three `to*` fields describe no live band. */
  redeployed: boolean;
  toLower: number;
  toUpper: number;
  bidEthDeployed: bigint;
  bidLiquidity: bigint;
  /** True when the close's fees are not the position's real accrued fees (the quoter replays the actions without a `Position` account). */
  feesUnknown: boolean;
  /** The tick the sell left the market at (before the close and the nudge). */
  fromTick: number;
  /** The tick after the nudge (where the new band was sized). */
  toTick: number;
}

/** A FLOOR pool's fee breakdown. Every part is in the quote currency except `lp` (the INPUT currency). */
export interface SwapQuoteFees {
  /** The LPs' fee, in the input currency (the Carpenter Platform fee is never inside it). */
  lp: bigint;
  /** The floor skim (the `$FLOOR` cut, `platformCut`, is inside it). */
  skim: bigint;
  creator: bigint;
  /** The holder fee (0 on a launch with no holder rate). */
  holder: bigint;
  /** The part of `skim` accrued to the platform. */
  platformCut: bigint;
  /** The FLOOR Platform fee, charged on top (never inside `skim`). */
  team: bigint;
  /** The Carpenter Platform fee the trader's walk pays, in currency `protocolCurrency` — on a FLOOR pool the quote: off
   *  the input on a buy, off the OUTPUT on a sell (`amountOut` is already net of it). */
  protocol: bigint;
  protocolCurrency: 0 | 1;
}

export interface SwapQuote {
  /** What the trader pays. */
  amountIn: bigint;
  /** What the trader receives. */
  amountOut: bigint;
  fees: SwapQuoteFees;
  /** The pool price after the whole swap, actions included. */
  sqrtAfter: bigint;
  tickAfter: number;
  /** The price after the trader's walk, before any action. */
  sqrtAfterTrade: bigint;
  tickAfterTrade: number;
  /** Start indices of the arrays the trader's walk read. */
  arraysTouched: number[];
  /** Start indices the actions' walks read. */
  actionArrays: number[];
  /** `arraysTouched ∪ actionArrays`, deduplicated: every array the transaction must carry. */
  allArrays: number[];
  /** The actions the hook requested and the AMM executed, every round in order. */
  replaceActions: HookActionQuote[];
  /** The in-swap bid re-place, when the sell triggers one. */
  replace: ReplaceQuote | null;
  signals: number;
  /** The hook delta credited to claims, signed. */
  hookDelta0: bigint;
  hookDelta1: bigint;
}

export interface QuoterVersion {
  floorQuoteWasm: string;
  hookInterface: number;
  /** Hex of the `floor_hook` program id the quoter was built against. */
  floorHookId: string;
}

export interface SaleBuyQuote {
  tokens: bigint;
  /** The RAISE part: `ceil(tokens · price_x64 / 2^64)`. */
  cost: bigint;
  /** The FLOOR Platform fee on top, `floor(cost · Launch.team_bps / 10 000)`. */
  teamFee: bigint;
  /** What leaves the buyer for the tokens: `cost + teamFee` ≤ `lamportsIn` (the gross cap). */
  spent: bigint;
  /** This buy sells the sale out and graduates the launch inside it (the buyer then also pays the graduation accounts' rent, bounded by `max_graduation_lamports`). */
  graduates: boolean;
}

/** A sell-back pays the gross payout LESS the FLOOR Platform fee. The position is read only to check that it covers `tokens`. */
export interface SaleSellBackQuote {
  /** The gross payout, `floor(tokens · price_x64 / 2^64)` at the sale price. */
  payout: bigint;
  /** `floor(payout · Launch.team_bps / 10 000)`. */
  teamFee: bigint;
  /** What the seller receives: `payout − teamFee`. */
  received: bigint;
}

export interface Quoter {
  readonly version: QuoterVersion;
  quoteSwap(input: SwapQuoteInput): SwapQuote;
  amountsForLiquidity(sqrtPriceX64: bigint, tickCurrent: number, tickLower: number, tickUpper: number, liquidityDelta: bigint): { amount0: bigint; amount1: bigint };
  quoteLiquidity(sqrtPriceX64: bigint, tickLower: number, tickUpper: number, amount0Max: bigint, amount1Max: bigint): bigint;
  /** `lowerArray` / `upperArray`: the arrays holding the position's two ticks; every account must name `poolKey`'s pool. */
  feesOwed(poolKey: Uint8Array, pool: Uint8Array, position: Uint8Array, lowerArray: Uint8Array, upperArray: Uint8Array): { fees0: bigint; fees1: bigint };
  sqrtPriceAtTick(tick: number): bigint;
  tickAtSqrtPrice(sqrtPriceX64: bigint): number;
  tickAtPriceUi(priceUi: number, decimals0: number, decimals1: number): number;
  /** Display only; `NaN` for a tick out of range. */
  priceUiAtTick(tick: number, decimals0: number, decimals1: number): number;
  floorToSpacing(tick: number, spacing: number): number;
  ceilToSpacing(tick: number, spacing: number): number;
  saleBuy(launch: Uint8Array, lamportsIn: bigint): SaleBuyQuote;
  saleSellBack(launch: Uint8Array, buyerPosition: Uint8Array, tokens: bigint): SaleSellBackQuote;
  /**
   * What `floor_holders::claim` would pay this record now. Full account data: the `HolderBook`, the `HolderRecord`, the
   * launch's `FloorPool`, and the token account's live balance (0 for a closed one). A refusal is a `QuoteError`
   * (`WrongMint`, `NotHooked`, `QuoteBadAccount`).
   */
  holderClaimable(book: Uint8Array, record: Uint8Array, floorPool: Uint8Array, liveBalance: bigint): bigint;
  /** What `claim_referral_fees(tier)` would pay each tier now, `[due1, due2, due3]`, from the full `FloorPool` data. */
  referralTierDues(floorPool: Uint8Array): [bigint, bigint, bigint];
}

// ---- the wire -----------------------------------------------------------------------------------

type Json = Record<string, unknown>;

function parse(fn: string, text: string): Json {
  let v: unknown;
  try {
    v = JSON.parse(text);
  } catch {
    throw new QuoteError('QuoteBadResponse', { message: `${fn}: the wasm returned non-JSON` });
  }
  if (!v || typeof v !== 'object' || Array.isArray(v)) throw new QuoteError('QuoteBadResponse', { message: `${fn}: the wasm returned ${text}` });
  const o = v as Json;
  if (typeof o['err'] === 'string') {
    const extra: { startIndex?: number; account?: string } = {};
    if (typeof o['start_index'] === 'number') extra.startIndex = o['start_index'];
    if (typeof o['account'] === 'string') extra.account = o['account'];
    throw new QuoteError(o['err'], extra);
  }
  const ok = o['ok'];
  if (!ok || typeof ok !== 'object' || Array.isArray(ok)) throw new QuoteError('QuoteBadResponse', { message: `${fn}: no "ok" in ${text}` });
  return ok as Json;
}

const DECIMAL = /^-?\d+$/;

function big(o: Json, k: string): bigint {
  const v = o[k];
  if (typeof v !== 'string' || !DECIMAL.test(v)) throw new QuoteError('QuoteBadResponse', { message: `field ${k} is not a decimal string: ${String(v)}` });
  return BigInt(v);
}

function num(o: Json, k: string): number {
  const v = o[k];
  if (typeof v !== 'number' || !Number.isFinite(v)) throw new QuoteError('QuoteBadResponse', { message: `field ${k} is not a number: ${String(v)}` });
  return v;
}

function bool(o: Json, k: string): boolean {
  const v = o[k];
  if (typeof v !== 'boolean') throw new QuoteError('QuoteBadResponse', { message: `field ${k} is not a boolean: ${String(v)}` });
  return v;
}

function side(o: Json, k: string): 0 | 1 {
  const v = num(o, k);
  if (v !== 0 && v !== 1) throw new QuoteError('QuoteBadResponse', { message: `field ${k} is not a currency index (0 or 1): ${String(v)}` });
  return v;
}

function str(o: Json, k: string): string {
  const v = o[k];
  if (typeof v !== 'string') throw new QuoteError('QuoteBadResponse', { message: `field ${k} is not a string: ${String(v)}` });
  return v;
}

function ints(o: Json, k: string): number[] {
  const v = o[k];
  if (!Array.isArray(v) || !v.every((x) => typeof x === 'number' && Number.isInteger(x))) {
    throw new QuoteError('QuoteBadResponse', { message: `field ${k} is not an integer array` });
  }
  return v as number[];
}

function obj(o: Json, k: string): Json {
  const v = o[k];
  if (!v || typeof v !== 'object' || Array.isArray(v)) throw new QuoteError('QuoteBadResponse', { message: `field ${k} is not an object` });
  return v as Json;
}

/** The wasm parses integers with Rust's `parse`, so only a plain decimal is acceptable. */
export function toDecimal(v: bigint): string {
  return v.toString(10);
}

const EMPTY = new Uint8Array(0);

function decodeAction(a: unknown): HookActionQuote {
  if (!a || typeof a !== 'object') throw new QuoteError('QuoteBadResponse', { message: 'replace_actions entry is not an object' });
  const o = a as Json;
  const kind = str(o, 'kind');
  if (kind === 'swap') return { kind, zeroForOne: bool(o, 'zero_for_one'), amountSpecified: big(o, 'amount_specified'), sqrtPriceLimitX64: big(o, 'sqrt_price_limit_x64') };
  if (kind === 'modify_position') return { kind, salt: big(o, 'salt'), tickLower: num(o, 'tick_lower'), tickUpper: num(o, 'tick_upper'), liquidityDelta: big(o, 'liquidity_delta') };
  throw new QuoteError('QuoteBadResponse', { message: `unknown action kind ${kind}` });
}

function decodeReplace(r: Json): ReplaceQuote {
  return {
    absorbed: big(r, 'absorbed'),
    ethBack: big(r, 'eth_back'),
    nudgeSpent: big(r, 'nudge_spent'),
    nudgeGot: big(r, 'nudge_got'),
    fromLower: num(r, 'from_lower'),
    redeployed: bool(r, 'redeployed'),
    toLower: num(r, 'to_lower'),
    toUpper: num(r, 'to_upper'),
    bidEthDeployed: big(r, 'bid_eth_deployed'),
    bidLiquidity: big(r, 'bid_liquidity'),
    feesUnknown: bool(r, 'fees_unknown'),
    fromTick: num(r, 'from_tick'),
    toTick: num(r, 'to_tick'),
  };
}

/** `quote_swap`'s `ok` object → `SwapQuote`. */
export function decodeSwapQuote(o: Json): SwapQuote {
  const fees = obj(o, 'fees');
  const actions = o['replace_actions'];
  if (!Array.isArray(actions)) throw new QuoteError('QuoteBadResponse', { message: 'replace_actions is not an array' });
  const replace = o['replace'];
  return {
    amountIn: big(o, 'amount_in'),
    amountOut: big(o, 'amount_out'),
    fees: {
      lp: big(fees, 'lp'),
      skim: big(fees, 'skim'),
      creator: big(fees, 'creator'),
      holder: big(fees, 'holder'),
      platformCut: big(fees, 'platform_cut'),
      team: big(fees, 'team'),
      protocol: big(fees, 'protocol'),
      protocolCurrency: side(fees, 'protocol_currency'),
    },
    sqrtAfter: big(o, 'sqrt_after'),
    tickAfter: num(o, 'tick_after'),
    sqrtAfterTrade: big(o, 'sqrt_after_trade'),
    tickAfterTrade: num(o, 'tick_after_trade'),
    arraysTouched: ints(o, 'arrays_touched'),
    actionArrays: ints(o, 'action_arrays'),
    allArrays: ints(o, 'all_arrays'),
    replaceActions: actions.map(decodeAction),
    replace: replace === null || replace === undefined ? null : decodeReplace(replace as Json),
    signals: num(o, 'signals'),
    hookDelta0: big(o, 'hook_delta0'),
    hookDelta1: big(o, 'hook_delta1'),
  };
}

/** `version()` returns its object bare (no `{"ok"}` envelope). */
export function decodeVersion(text: string): QuoterVersion {
  let v: unknown;
  try {
    v = JSON.parse(text);
  } catch {
    throw new QuoteError('QuoteBadResponse', { message: 'version: the wasm returned non-JSON' });
  }
  if (!v || typeof v !== 'object' || Array.isArray(v)) throw new QuoteError('QuoteBadResponse', { message: `version: the wasm returned ${text}` });
  const o = v as Json;
  return { floorQuoteWasm: str(o, 'floor_quote_wasm'), hookInterface: num(o, 'hook_interface'), floorHookId: str(o, 'floor_hook_id') };
}

/** Build the typed `Quoter` over the wasm's exports (or a fake, in tests). */
export function createQuoter(fns: QuoteFns): Quoter {
  const version = decodeVersion(fns.version());
  return {
    version,
    quoteSwap(input) {
      // No pre-checks: `AmountZero`, a bad pool key and the like are the wasm's answers, surfaced unchanged.
      const text = fns.quote_swap(
        input.poolKey,
        input.pool,
        input.bitmap ?? EMPTY,
        concatTickArrays(input.tickArrays),
        input.floorPool ?? EMPTY,
        input.zeroForOne,
        toDecimal(input.amountSpecified),
        input.sqrtPriceLimitX64 === null || input.sqrtPriceLimitX64 === undefined ? '' : toDecimal(input.sqrtPriceLimitX64),
        input.supply1 === null || input.supply1 === undefined ? '' : toDecimal(input.supply1),
      );
      return decodeSwapQuote(parse('quote_swap', text));
    },
    amountsForLiquidity(sqrtPriceX64, tickCurrent, tickLower, tickUpper, liquidityDelta) {
      const o = parse('amounts_for_liquidity', fns.amounts_for_liquidity(toDecimal(sqrtPriceX64), tickCurrent, tickLower, tickUpper, toDecimal(liquidityDelta)));
      return { amount0: big(o, 'amount0'), amount1: big(o, 'amount1') };
    },
    quoteLiquidity(sqrtPriceX64, tickLower, tickUpper, amount0Max, amount1Max) {
      return big(parse('quote_liquidity', fns.quote_liquidity(toDecimal(sqrtPriceX64), tickLower, tickUpper, toDecimal(amount0Max), toDecimal(amount1Max))), 'liquidity');
    },
    feesOwed(poolKey, pool, position, lowerArray, upperArray) {
      const o = parse('fees_owed', fns.fees_owed(poolKey, pool, position, accountToBody(lowerArray), accountToBody(upperArray)));
      return { fees0: big(o, 'fees0'), fees1: big(o, 'fees1') };
    },
    sqrtPriceAtTick: (tick) => big(parse('sqrt_price_at_tick', fns.sqrt_price_at_tick(tick)), 'sqrt_price_x64'),
    tickAtSqrtPrice: (sqrtPriceX64) => num(parse('tick_at_sqrt_price', fns.tick_at_sqrt_price(toDecimal(sqrtPriceX64))), 'tick'),
    tickAtPriceUi: (priceUi, d0, d1) => num(parse('tick_at_price_ui', fns.tick_at_price_ui(priceUi, d0, d1)), 'tick'),
    priceUiAtTick: (tick, d0, d1) => fns.price_ui_at_tick(tick, d0, d1),
    floorToSpacing: (tick, spacing) => num(parse('floor_to_spacing', fns.floor_to_spacing(tick, spacing)), 'tick'),
    ceilToSpacing: (tick, spacing) => num(parse('ceil_to_spacing', fns.ceil_to_spacing(tick, spacing)), 'tick'),
    saleBuy(launch, lamportsIn) {
      const o = parse('sale_buy', fns.sale_buy(launch, toDecimal(lamportsIn)));
      return { tokens: big(o, 'tokens'), cost: big(o, 'cost'), teamFee: big(o, 'team_fee'), spent: big(o, 'spent'), graduates: bool(o, 'graduates') };
    },
    saleSellBack(launch, buyerPosition, tokens) {
      const o = parse('sale_sell_back', fns.sale_sell_back(launch, buyerPosition, toDecimal(tokens)));
      return { payout: big(o, 'payout'), teamFee: big(o, 'team_fee'), received: big(o, 'received') };
    },
    holderClaimable(book, record, floorPool, liveBalance) {
      try {
        return fns.holder_claimable(book, record, floorPool, liveBalance);
      } catch (e) {
        throw new QuoteError(e instanceof Error ? e.message : String(e));
      }
    },
    referralTierDues(floorPool) {
      const o = parse('referral_tier_dues', fns.referral_tier_dues(floorPool));
      return [big(o, 'due1'), big(o, 'due2'), big(o, 'due3')];
    },
  };
}
