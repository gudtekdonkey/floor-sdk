/**
 * FLOOR's transaction builders: the sale buy (with the graduation accounts and, while unprepared, the `prepare` step)
 * and sell-back, and `buildSwap` for FLOOR pools — `@carpenter/sdk`'s swap encoder with FLOOR's defaults (the hook
 * slice, the holders transfer-hook leg, the buyer's record, the FLOOR CU rows). Liquidity on any pool is
 * `@carpenter/sdk`'s.
 * Each returns `{ instructions, accounts, cuLimit, addressCount }`; signing and sending stay with the caller.
 *
 * Every builder:
 * - ⛔ prepends `SetComputeUnitLimit` from `CU_LIMITS` (an unset limit is 0 under v1 transactions);
 * - ⛔ asserts ≤ 64 unique accounts (the account-lock limit) and throws BY NAME above it;
 * - adds ATA `CreateIdempotent` for an output account and wSOL wrap / unwrap where it moves SOL;
 * - on a CPI-Guard Token-2022 source puts `ApproveChecked(delegate = pool or launch)` IMMEDIATELY before the program
 *   instruction, in the same message, and `Revoke` after it when a residual delegation can remain.
 * ⛔ Every sale buy uses the FINAL-buy CU limit, the graduation accounts and `max_graduation_lamports`, because finality
 *    is decided at execution.
 */

import { AccountRole, isOffCurveAddress, type AccountMeta, type Address,  type Instruction } from '@solana/kit';

import {
  addressToBytes,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  SPL_TOKEN_PROGRAM_ID,

  MAX_SQRT_PRICE_X64,
  MIN_SQRT_PRICE_X64,
  TOKEN_2022_PROGRAM_ID,
  WSOL_MINT,
  type Pool,
  findAta,
  findBitmap,
  findEventAuthority,
  findPosition,
  findTickArray,
  NO_HOOK,
  type RentExemptMinimum,
  tickArrayStartIndex,
  initializeTickArrayIx,
  swapIx,
  finalizeUnits,

  compileSendable,
  type LookupTableContents,
  type V0MessageArgs,
  approveCheckedIx,
  createAtaIdempotentIx,
  revokeIx,
  tempWsol,
} from '@carpenter/sdk';
// the slot list's check and words.
import {  TOKEN_DECIMALS, type CuLimitKey } from '../constants.ts';
import { findHolderBook, findHolderRecord, touchIx, transferHookAccounts, withoutAccountsAlreadyIn } from '../holders.ts';
import { assertLaunchTableFrozen, type LaunchTableState } from './launchAlt.ts';
import type { FloorPool, Launch,  } from '../layouts.ts';
import {
  DEFAULT_PROGRAM_IDS,
  findBuyerPosition,
  findFloorPool,
  findHookConfig,
  findTraderBucket,
  findLaunch,
  findLockAuthority,
  findSaleVault,
  type ProgramIds,
} from '../pda.ts';
import { graduationAccounts,  type GraduationPrediction } from '../predict.ts';
import { buyIx, closeBuyerPositionIx, prepareGraduationIx, sellBackIx, sellBackWsolIx, type FloorInstruction } from './instructions.ts';
import { saleRefPrefix } from './traderRef.ts';
import { finalize,  sumCu,  type BuiltTransaction, type QuoteSpec,  } from './common.ts';

// The shared shape lives in `./common.ts` so `../holders.ts` can use it without importing
// this module (which imports `../holders.ts`). Re-exported here.
export { finalize, lockCount, sumCu, WSOL_QUOTE } from './common.ts';
export { finalizeUnits, TooManyAccountsError, uniqueAccounts } from '@carpenter/sdk';
export type { BuiltTransaction, QuoteSpec } from './common.ts';

const isDefault = (a: Address) => a === NO_HOOK;
const W = AccountRole.WRITABLE;

/**
 * ⛔ **A destination with `MemoTransfer` is REFUSED BY NAME, never repaired**. The SDK
 * decodes `MemoTransfer.require_incoming_transfer_memos` (`getAtaExtensions(...).memoRequired`): a buy or a swap
 * into such an account reverts `TokenError::NoMemo` (36) on chain.
 * ⛔ Not the CPI-Guard treatment (an `ApproveChecked` the builder inserts): the memo has to be the
 * transfer's immediately-preceding SIBLING, and the transfer here happens inside the program's own
 * CPI, so a memo the client prepends is not sibling to it, and the program never CPIs Memo on the
 * buyer's behalf. The builder names the blocker instead.
 * ⚠ `buildSellBack` takes no such flag ON PURPOSE: its payout is LAMPORTS to the buyer's wallet and
 * the token leg moves OUT of the buyer's ATA, so no incoming token transfer exists to require one.
 */
const MEMO_REFUSAL = (fn: string, what: string): string =>
  `${fn}: ${what} requires an incoming transfer memo (Token-2022 MemoTransfer, TokenError::NoMemo); disable it on the account first`;

// ---- the hooked base leg ------

/**
 * A graduated swap / liquidity move on a HOOKED launch's pool: what the AMM's hook-aware base leg needs.
 * `floorPool` is the launch's `FloorPool`; `openBuyerRecord`
 * (a BUY only) PREPENDS the buyer's ATA create-idempotent and `touch`, the buyer paying their own record
 * deposit inside their own buy.
 * ⛔ Omit it on an UNHOOKED launch — the AMM refuses the extra accounts nowhere, but they name a
 *    `floor_holders` book that does not exist and cost bytes and locks for nothing.
 */
export interface TransferHookOption {
  floorPool: Address;
  /** ⚠ Ignored for an OFF-CURVE trader: no record can exist for a program-owned wallet. */
  openBuyerRecord?: boolean;
}

/**
 * The base leg's six extra accounts, MINUS what the AMM instruction already names: the AMM finds them BY KEY in its unordered remaining set, so a duplicate buys
 * nothing but a byte. ⛔ FLOOR's base is currency 1, so the leg is between
 * `vault1` and the trader's base token account, in the direction the trade moves the base.
 */
async function baseLegHookAccounts(a: {
  baseMint: Address;
  vault1: Address;
  traderBase: Address;
  /** True when the base moves INTO the trader (a buy, a liquidity removal), false when it moves into the vault. */
  toTrader: boolean;
  floorPool: Address;
  already: readonly AccountMeta[];
  ids: ProgramIds;
}): Promise<AccountMeta[]> {
  const [source, destination] = a.toTrader ? [a.vault1, a.traderBase] : [a.traderBase, a.vault1];
  const extra = await transferHookAccounts({ mint: a.baseMint, floorPool: a.floorPool, source, destination, ids: a.ids });
  return withoutAccountsAlreadyIn(extra, a.already);
}

/**
 * `touch` for the BUYER of a hooked FLOOR pool. ⛔ A record keys the buyer's Token-2022 ATA and nothing
 * else (`NotAssociatedTokenAccount`), so a caller that routed the output into ANOTHER account is refused by
 * name: the hook would settle that account's record, which can never exist, and the buyer would earn nothing.
 */
async function buyerTouch(trader: Address, baseMint: Address, tokenOut: Address, floorPool: Address, ids: ProgramIds): Promise<FloorInstruction> {
  const ata = (await findAta(trader, baseMint, TOKEN_2022_PROGRAM_ID)).address;
  if (tokenOut !== ata) throw new Error(`openBuyerRecord: the output account ${tokenOut} is not the buyer's Token-2022 ATA ${ata} — a holder record keys the ATA alone (NotAssociatedTokenAccount)`);
  return touchIx({ payer: trader, owner: trader, mint: baseMint, floorPool, ids });
}

// ---- swap ------------------------------------------------------------------------

export interface BuildSwapArgs {
  trader: Address;
  pool: Address;
  poolState: Pick<Pool, 'mint0' | 'mint1' | 'vault0' | 'vault1' | 'tokenProgram0' | 'tokenProgram1' | 'hook' | 'decimals0' | 'decimals1'>;
  zeroForOne: boolean;
  /** v4 sign: `< 0` exact-in (spend `-amountSpecified`), `> 0` exact-out. ⛔ FLOOR pools refuse exact-out. */
  amountSpecified: bigint;
  /** Exact-in: the minimum out. Exact-out: the maximum in. */
  amountLimit: bigint;
  sqrtPriceLimitX64?: bigint;
  /** From `predict.swapTickArrays` (the quoter's walk ∪ its actions' walks). */
  tickArrayStarts: readonly number[];
  /**
   * Tick arrays to create-if-missing BEFORE the swap — on a FLOOR sell, the predicted ACTIONS'
   * arrays (`quote.actionArrays`). `initialize_tick_array` is idempotent, so an existing array is left untouched; one
   * its rent owner closed just before this sell (an empty array, which `close_tick_array` allows) is re-created here,
   * paid by the trader and recorded as theirs, instead of the in-swap bid re-place failing the sell
   * (`HookActionAccountMissing`). Each must also be in `tickArrayStarts`.
   */
  ensureTickArrayStarts?: readonly number[];
  actionPositions?: readonly Address[];
  /** Defaults: the trader's ATAs. */
  traderTokenIn?: Address;
  traderTokenOut?: Address;
  /** `getAtaExtensions(traderTokenIn).cpiGuardLocked`. */
  sourceHasCpiGuard?: boolean;
  /**
   * `getAtaExtensions(traderTokenOut).memoRequired` — the output leg is a transfer INTO that
   * account, and Token-2022 refuses one without a preceding Memo. ⛔ REFUSED by name, never fixed
   * by prepending a Memo: see `buildBuy`.
   */
  destinationRequiresMemo?: boolean;
  /**
   * The hook slice; default for a FLOOR pool is `[floor_pool (w), trader_bucket (w), hook event_authority (r), floor_hook (r)]`
   * (`floorSwapHookSlice`) — the bucket is `findTraderBucket(trader, poolState.mint0)`.
   * Required for any other hook.
   */
  hookSlice?: readonly AccountMeta[];
  hookData?: Uint8Array;
  /**
   * Move a wSOL side as native SOL (default true): through a temporary wSOL account (`tempWsol`) created, funded with
   * exactly what the swap may spend (an input) and closed inside this transaction, so the trader's own wSOL account and
   * its balance are never touched. Ignored for a side given as `traderTokenIn` / `traderTokenOut`. `false`: the wSOL side
   * is the trader's wSOL ATA (or the named account), neither wrapped nor closed.
   */
  wrapSol?: boolean;
  /** The temporary wSOL account's seed (default: random). */
  wsolSeed?: string;
  /** The temporary wSOL account's rent (default: the 165-byte token account's rent-exempt minimum). */
  wsolRentLamports?: bigint;
  /**
   * ⛔ on a HOOKED launch's FLOOR pool, the transfer hook's extra accounts for the
   * base leg (and, `openBuyerRecord`, the buyer's record opened first). Refused on any other pool.
   */
  transferHook?: TransferHookOption;
  ids?: ProgramIds;
}

/** ⛔ The hook option belongs to a FLOOR pool alone, and `openBuyerRecord` to a BUY alone — refused by name otherwise. */
function checkTransferHookOption(fn: string, t: TransferHookOption | undefined, isFloor: boolean, zeroForOne: boolean): void {
  if (!t) return;
  if (!isFloor) throw new Error(`${fn}: transferHook is for a FLOOR pool — a hookless pool's base carries no floor_holders hook`);
  if (t.openBuyerRecord && !zeroForOne) throw new Error(`${fn}: openBuyerRecord is a BUY option (the buyer opens their record in their own buy); a seller already holds`);
}

/**
 * A FLOOR pool's SWAP hook slice,
 * `[floor_pool w, trader_bucket w, hook event_authority r, floor_hook r]` (`FLOOR_CALLBACK_ACCOUNTS` = 4). The bucket is the swap SIGNER's
 * `["trader_ref", trader, quote_mint]` under `floor_hook`, the quote being the pool's `mint0`. It is ALWAYS the derived
 * PDA and ALWAYS writable: the hook validates it and books nothing if it is absent or uninitialised (the swap never fails over
 * it), so a missing or read-only slot only loses the trader's referrers their slice. ⚠ Per trader: never in a deployment ALT.
 */
export async function floorSwapHookSlice(pool: Address, trader: Address, quoteMint: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<AccountMeta[]> {
  const [floorPool, bucket, hookEvents] = await Promise.all([
    findFloorPool(pool, ids),
    findTraderBucket(trader, quoteMint, ids),
    findEventAuthority(ids.floorHook),
  ]);
  return [
    { address: floorPool.address, role: W },
    { address: bucket.address, role: W },
    ...floorHookEventCpiPair(hookEvents.address, ids),
  ];
}

/**
 * Slots 3 and 4 of every FLOOR hook slice: the callbacks' `#[event_cpi]` pair — `floor_hook`'s `["__event_authority"]`
 * PDA and the `floor_hook` program, both read-only (the callbacks emit their events with `emit_cpi!`). Both are
 * deployment constants (ALT-able); the program is already a key of every hooked swap.
 */
function floorHookEventCpiPair(hookEventAuthority: Address, ids: ProgramIds): AccountMeta[] {
  return [
    { address: hookEventAuthority, role: AccountRole.READONLY },
    { address: ids.floorHook, role: AccountRole.READONLY },
  ];
}

/**
 * The FLOOR hook slice of `initialize_pool` (graduation / `prepare_graduation`, built by `floor_launch` on chain, or any
 * client-built pool on a FLOOR hook): `[floor_pool w, hook_config r, hook event_authority r, floor_hook r]`. `before_initialize`
 * never reads slot 2, so it carries
 * `hook_config` (the floor_hook `["hook_config"]` PDA) — `graduate.rs`' own choice: an account the graduation list already
 * holds and no AMM account (the alias refusal).
 */
export async function floorInitializeHookSlice(pool: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<AccountMeta[]> {
  const [floorPool, hookConfig, hookEvents] = await Promise.all([findFloorPool(pool, ids), findHookConfig(ids), findEventAuthority(ids.floorHook)]);
  return [
    { address: floorPool.address, role: W },
    { address: hookConfig.address, role: AccountRole.READONLY },
    ...floorHookEventCpiPair(hookEvents.address, ids),
  ];
}

export async function buildSwap(a: BuildSwapArgs): Promise<BuiltTransaction> {
  const ids = a.ids ?? DEFAULT_PROGRAM_IDS;
  const p = a.poolState;
  if (a.amountSpecified === 0n) throw new Error('buildSwap: amountSpecified is 0 (AmountZero)');
  const hooked = !isDefault(p.hook);
  const isFloor = p.hook === ids.floorHook;
  if (isFloor && a.amountSpecified > 0n) throw new Error('buildSwap: FLOOR pools refuse exact-out (ExactOutNotSupported)');
  if (a.destinationRequiresMemo) throw new Error(MEMO_REFUSAL('buildSwap', 'the output account'));
  checkTransferHookOption('buildSwap', a.transferHook, isFloor, a.zeroForOne);

  const [mintIn, mintOut] = a.zeroForOne ? [p.mint0, p.mint1] : [p.mint1, p.mint0];
  const [tpIn, tpOut] = a.zeroForOne ? [p.tokenProgram0, p.tokenProgram1] : [p.tokenProgram1, p.tokenProgram0];
  const decimalsIn = a.zeroForOne ? p.decimals0 : p.decimals1;
  const maxIn = a.amountSpecified < 0n ? -a.amountSpecified : a.amountLimit;
  const wrap = a.wrapSol ?? true;
  // A temporary wSOL account for a wSOL side the caller did not name (only one side of a pool can be wSOL).
  const wsolOpts = { ...(a.wsolSeed !== undefined ? { seed: a.wsolSeed } : {}), ...(a.wsolRentLamports !== undefined ? { rentLamports: a.wsolRentLamports } : {}) };
  const tempIn = wrap && mintIn === WSOL_MINT && !a.traderTokenIn ? await tempWsol(a.trader, maxIn, wsolOpts) : null;
  const tempOut = wrap && mintOut === WSOL_MINT && !a.traderTokenOut ? await tempWsol(a.trader, 0n, wsolOpts) : null;
  const tokenIn = tempIn?.account ?? a.traderTokenIn ?? (await findAta(a.trader, mintIn, tpIn)).address;
  const tokenOut = tempOut?.account ?? a.traderTokenOut ?? (await findAta(a.trader, mintOut, tpOut)).address;

  let hookSlice: readonly AccountMeta[] = a.hookSlice ?? [];
  if (hooked && a.hookSlice === undefined) {
    if (!isFloor) throw new Error('buildSwap: a non-FLOOR hook needs an explicit hookSlice');
    hookSlice = await floorSwapHookSlice(a.pool, a.trader, p.mint0, ids);
  }

  const [bitmap, eventAuthority, arrays] = await Promise.all([
    findBitmap(a.pool, ids),
    findEventAuthority(ids.carpenterAmm),
    Promise.all(a.tickArrayStarts.map(async (s) => (await findTickArray(a.pool, s, ids)).address)),
  ]);

  const accounts = {
    program: ids.carpenterAmm,
    eventAuthority: eventAuthority.address,
    trader: a.trader,
    pool: a.pool,
    bitmap: bitmap.address,
    traderTokenIn: tokenIn,
    traderTokenOut: tokenOut,
    vault0: p.vault0,
    vault1: p.vault1,
    mint0: p.mint0,
    mint1: p.mint1,
    tokenProgram0: p.tokenProgram0,
    tokenProgram1: p.tokenProgram1,
    hookProgram: hooked ? p.hook : null,
    hookSlice,
    tickArrays: arrays,
    actionPositions: a.actionPositions ?? [],
  };
  const args = {
    zeroForOne: a.zeroForOne,
    amountSpecified: a.amountSpecified,
    sqrtPriceLimitX64: a.sqrtPriceLimitX64 ?? (a.zeroForOne ? MIN_SQRT_PRICE_X64 + 1n : MAX_SQRT_PRICE_X64 - 1n),
    amountLimit: a.amountLimit,
    hookData: a.hookData,
  };
  const bare = swapIx(accounts, args);
  const hookExtra = a.transferHook
    ? await baseLegHookAccounts({
        baseMint: p.mint1, vault1: p.vault1, traderBase: a.zeroForOne ? tokenOut : tokenIn, toTrader: a.zeroForOne, floorPool: a.transferHook.floorPool, already: bare.accounts, ids,
      })
    : [];
  const main = hookExtra.length ? swapIx({ ...accounts, transferHookAccounts: hookExtra }, args) : bare;

  const ensure = [...new Set(a.ensureTickArrayStarts ?? [])];
  for (const st of ensure) {
    if (!a.tickArrayStarts.includes(st)) throw new Error(`buildSwap: ensureTickArrayStarts ${st} is not in tickArrayStarts`);
  }
  const ensureIxs = await Promise.all(
    ensure.map(async (st) =>
      initializeTickArrayIx({ program: ids.carpenterAmm, payer: a.trader, pool: a.pool, tickArray: (await findTickArray(a.pool, st, ids)).address, startIndex: st }),
    ),
  );
  const body: Instruction[] = [...ensureIxs, ...(tempOut ? tempOut.open : [createAtaIdempotentIx(a.trader, tokenOut, a.trader, mintOut, tpOut)])];
  // the buyer's record, opened right after their base ATA exists and BEFORE the swap, so the
  //    hook's `execute` settles THIS buy into it (a record opened after would miss the buy's tokens).
  // SKIPPED for an OFF-CURVE trader (a Squads vault or any other PDA
  //    wallet) — no record can exist for one, and asking would fail the whole buy; the
  //    buy lands and that wallet earns nothing, as for any program-owned holder.
  const openRecord = !!a.transferHook?.openBuyerRecord && !isOffCurveAddress(a.trader);
  if (openRecord) body.push(await buyerTouch(a.trader, p.mint1, tokenOut, a.transferHook!.floorPool, ids));
  if (tempIn) body.push(...tempIn.open);
  if (a.sourceHasCpiGuard) body.push(approveCheckedIx(tokenIn, mintIn, a.pool, a.trader, maxIn, decimalsIn, tpIn));
  body.push(main);
  // The walk may consume less than the approval (a price-limit stop, an exact-out quote): revoke the rest.
  if (a.sourceHasCpiGuard) body.push(revokeIx(tokenIn, a.trader, tpIn));
  if (tempIn) body.push(tempIn.close);
  if (tempOut) body.push(tempOut.close);

  const cu: CuLimitKey = !hooked ? 'swapPlain' : isFloor && a.zeroForOne ? 'swapFloorBuy' : 'swapFloorSell';
  const ensureCu = ensure.map(() => 'initializeTickArray' as const);
  return openRecord || ensureCu.length
    ? finalizeUnits('buildSwap', sumCu(cu, ...(openRecord ? (['holdersTouch'] as const) : []), ...ensureCu), body, main)
    : finalize('buildSwap', cu, body, main);
}

// ---- sale ------------------------------------------------------------------------

async function saleAccounts(buyer: Address, launch: Address, l: Pick<Launch, 'mint' | 'tokenVault'>, ids: ProgramIds) {
  const [saleVault, buyerPosition, buyerAta, lockAuthority, ev] = await Promise.all([
    findSaleVault(launch, ids),
    findBuyerPosition(launch, buyer, ids),
    findAta(buyer, l.mint, TOKEN_2022_PROGRAM_ID),
    findLockAuthority(ids),
    findEventAuthority(ids.floorLaunch),
  ]);
  return {
    program: ids.floorLaunch,
    eventAuthority: ev.address,
    buyer,
    launch,
    saleVault: saleVault.address,
    tokenVault: l.tokenVault,
    buyerPosition: buyerPosition.address,
    buyerAta: buyerAta.address,
    mint: l.mint,
    lockAuthority: lockAuthority.address,
    token2022: TOKEN_2022_PROGRAM_ID,
  };
}

export interface BuildBuyArgs {
  buyer: Address;
  /** The decoded `Launch` (its address is re-derived from `mint`). */
  launch: Pick<Launch, 'mint' | 'tokenVault' | 'priceX64' | 'openTick' | 'ammPool' | 'floorPool' | 'wsolVault'> & Partial<Pick<Launch, 'saleTokens'>>;
  /**
   * The launch's quote when it is not wSOL — `launchQuoteMint(launch)` names
   * the mint, the caller supplies its token program (`decimals` is unused here). The buyer pays in
   * it: their quote ATA rides as the first remaining account. Omitted = the wSOL path, unchanged.
   */
  quote?: Pick<QuoteSpec, 'mint' | 'tokenProgram'>;
  /**
   * `lamportsIn` is an amount of the QUOTE in its base units — lamports on a wSOL launch.
   * `lamportsIn` is the GROSS cap: the program buys with `saleNetOf(lamportsIn, launch.teamBps)`
   * and moves `cost + fee`; take `minTokens` from `quote.saleBuy(launch, lamportsIn).tokens`; a guarded
   * quote's approval of `lamportsIn` covers `cost + fee`.
   */
  lamportsIn: bigint;
  /** Slippage: from `quote.saleBuy(lamportsIn)` less the buyer's tolerance. */
  minTokens: bigint;
  rent: RentExemptMinimum;
  existing?: ReadonlySet<Address>;
  bidLowerHint?: number;
  /**
   * The launch's decoded `FloorPool` (`getFloorPool(src, launch.ammPool)`), for the
   * `min_bid` reserve graduation holds back — `graduationAccounts`' `floorPool`. ⛔ REQUIRED with a
   * non-wSOL `quote` (the prediction throws by name without it); optional on wSOL.
   * its `stairSteps` (or `stairSteps` below) is REQUIRED on every quote — every pool is a
   * staircase, and the prediction throws by name without it.
   */
  // ⛔ (THE STAIRCASE): `stairSteps` — the graduating buy carries the StairState, the three positions and the
  // staircase's inner-edge arrays (`graduationAccounts`).
  floorPool?: Pick<FloorPool, 'minBid'> & Partial<Pick<FloorPool, 'stairSteps'>>;
  /** `FloorPool.stairSteps`, when `floorPool` is not passed (`graduationAccounts`' `stairSteps`). */
  stairSteps?: number;
  /**
   * `getAtaExtensions(the buyer's ATA).memoRequired` — the buy transfers tokens INTO it. ⛔ REFUSED
   * by name (see `MEMO_REFUSAL`): the program will not CPI a Memo for the buyer.
   */
  buyerAtaRequiresMemo?: boolean;
  /**
   * ⛔ On a HOOKED launch, APPEND `touch` after the buy — the buyer's record
   * opens (or syncs) in the buyer's own transaction and the buyer pays its deposit. AFTER, not before: the
   * buy creates the buyer's ATA, and `touch` needs it to exist. The hook is not armed before graduation,
   * so the record simply snapshots the post-buy balance; on the GRADUATING buy graduation arms it first.
   * `floorPool` defaults to `launch.floorPool`. ⛔ Omit it on an unhooked launch: there is no book, and
   * `touch` would fail the whole buy. ⚠ Ignored for an OFF-CURVE buyer: no record can exist
   * for a program-owned wallet, so the buy is built without it.
   */
  touch?: { floorPool?: Address };
  /**
   * The launch is HOOKED (a holder rate) — its graduation prices the 175-byte base vault. Defaults to `!!touch` (send
   * `touch` exactly on a hooked launch).
   */
  hooked?: boolean;
  /**
   * `true` puts the 5-account prefix (`saleRefPrefix`: the trader's
   * bucket in THIS launch's quote, the launch's sale account, launch_authority, hook_config, floor_hook) at the FRONT of the
   * remaining accounts, so the FLOOR Platform fee's 30 / 5 / 2 % slices are booked to the trader's tiers. The program
   * recognises the prefix by its shape, so a bucket that is not opened yet (or a sale referral account that is not opened,
   * or frozen) books nothing and the trade proceeds; a prefix naming another trader's bucket or another launch's sale
   * account is refused `TraderRefMismatch`. Omitted / false: no prefix.
   */
  traderRef?: boolean;
  ids?: ProgramIds;
}

export async function buildBuy(a: BuildBuyArgs): Promise<BuiltTransaction & { graduation: GraduationPrediction }> {
  const ids = a.ids ?? DEFAULT_PROGRAM_IDS;
  if (a.lamportsIn <= 0n) throw new Error('buildBuy: lamportsIn must be > 0 (Nothing)');
  if (a.buyerAtaRequiresMemo) throw new Error(MEMO_REFUSAL('buildBuy', "the buyer's ATA"));
  const launch = (await findLaunch(a.launch.mint, ids)).address;
  const graduation = await graduationAccounts({
    launch: a.launch,
    launchAddress: launch,
    rent: a.rent,
    ids,
    hooked: a.hooked ?? !!a.touch,
    ...(a.existing ? { existing: a.existing } : {}),
    ...(a.bidLowerHint !== undefined ? { bidLowerHint: a.bidLowerHint } : {}),
    ...(a.quote ? { quote: a.quote } : {}),
    ...(a.floorPool ? { floorPool: a.floorPool } : {}),
    ...(a.stairSteps !== undefined ? { stairSteps: a.stairSteps } : {}),
  });
  const sale = await saleAccounts(a.buyer, launch, a.launch, ids);
  const buyerQuoteAta = a.quote && a.quote.mint !== WSOL_MINT ? (await findAta(a.buyer, a.quote.mint, a.quote.tokenProgram)).address : undefined;
  const saleRef = a.traderRef ? await saleRefPrefix({ trader: a.buyer, launchMint: a.launch.mint, quoteMint: a.quote?.mint ?? WSOL_MINT, ids }) : undefined;
  const main = buyIx(
    { ...sale, ataProgram: ASSOCIATED_TOKEN_PROGRAM_ID, graduationAccounts: graduation.accounts, ...(buyerQuoteAta ? { buyerQuoteAta } : {}), ...(saleRef ? { saleRef } : {}) },
    { lamportsIn: a.lamportsIn, minTokens: a.minTokens, maxGraduationLamports: graduation.maxGraduationLamports },
  );
  const body: Instruction[] = [main];
  // no `touch` for an OFF-CURVE buyer (a PDA wallet) — no record can
  //    exist for one, and asking would fail the whole buy. The buy lands; it earns nothing.
  if (a.touch && !isOffCurveAddress(a.buyer)) body.push(await touchIx({ payer: a.buyer, owner: a.buyer, mint: a.launch.mint, floorPool: a.touch.floorPool ?? a.launch.floorPool, ids }));
  // ⚠ The limit stays `buyFinal` (already the 1.4 M transaction cap) with a `touch` appended.
  return { ...finalize('buildBuy', 'buyFinal', body, main), graduation };
}

export interface BuildPrepareGraduationArgs {
  /** Whoever presses it. Permissionless: no relation to the creator or to any buyer. */
  payer: Address;
  launch: Pick<Launch, 'mint' | 'tokenVault' | 'priceX64' | 'openTick' | 'ammPool' | 'floorPool' | 'wsolVault'> & Partial<Pick<Launch, 'saleTokens'>>;
  /** The launch's quote when it is not wSOL; the list carries it in slots 7 / 10–14. */
  quote?: Pick<QuoteSpec, 'mint' | 'tokenProgram'>;
  rent: RentExemptMinimum;
  existing?: ReadonlySet<Address>;
  bidLowerHint?: number;
  /** As `BuildBuyArgs.floorPool` — the SAME list `buildBuy` sends needs the same input. */
  floorPool?: Pick<FloorPool, 'minBid'> & Partial<Pick<FloorPool, 'stairSteps'>>;
  /** As `BuildBuyArgs.stairSteps`. */
  stairSteps?: number;
  /** As `BuildBuyArgs.hooked` — a hooked launch's base vault is 175 B. */
  hooked?: boolean;
  ids?: ProgramIds;
}

/**
 * `prepare_graduation` — the pool, the four tick arrays graduation's steps 6 and 7 ask for, and the four hook positions,
 * in their own transaction so the FINAL BUY does not have to carry them (a first-time last buyer's graduating buy is over
 * the 64-entry instruction-trace cap without it).
 *
 * ⛔ **Call it before the sale sells out, not after** — there is no "after": the buy that sells the last token graduates
 * inline, so an unprepared sale simply cannot be finished. Anyone may call it, any number of times, from the moment the
 * launch exists. {@link planBuy} puts it in front of a buy whenever the launch is not prepared.
 * ⛔ The list it sends is `graduationAccounts`' — the SAME one `buildBuy` sends.
 */
export async function buildPrepareGraduation(
  a: BuildPrepareGraduationArgs,
): Promise<BuiltTransaction & { graduation: GraduationPrediction }> {
  const ids = a.ids ?? DEFAULT_PROGRAM_IDS;
  const launch = (await findLaunch(a.launch.mint, ids)).address;
  const graduation = await graduationAccounts({
    launch: a.launch,
    launchAddress: launch,
    rent: a.rent,
    ids,
    ...(a.hooked ? { hooked: true } : {}),
    ...(a.existing ? { existing: a.existing } : {}),
    ...(a.bidLowerHint !== undefined ? { bidLowerHint: a.bidLowerHint } : {}),
    ...(a.quote ? { quote: a.quote } : {}),
    ...(a.floorPool ? { floorPool: a.floorPool } : {}),
    ...(a.stairSteps !== undefined ? { stairSteps: a.stairSteps } : {}),
  });
  const main = prepareGraduationIx({
    program: ids.floorLaunch,
    payer: a.payer,
    launch,
    mint: a.launch.mint,
    token2022: TOKEN_2022_PROGRAM_ID,
    eventAuthority: (await findEventAuthority(ids.floorLaunch)).address,
    graduationAccounts: graduation.accounts,
  });
  return { ...finalize('buildPrepareGraduation', 'prepareGraduation', [main], main), graduation };
}

// ---- the prepare step a buy carries ----------------------------------------------------------

/** The slots `prepare_graduation` creates: when all of them exist the launch is prepared (`graduationPrepared`). */
export const PREPARED_SLOTS = [
  'pool', 'bitmap', 'vault0', 'vault1', 'pool_index_entry', 'position_seed', 'position_bid',
  'stair_state', 'position_stair_step0', 'position_stair_reserve', 'position_stair_tail',
] as const;

/**
 * Has `prepare_graduation` run to the end? Every account in {@link PREPARED_SLOTS} of `graduation`'s list is in
 * `existing` (the caller's `getMultipleAccounts` over `graduation.accounts`). ⚠ The planted tick arrays are not
 * checked: one missing (a creator closed it while empty) costs the final buy three trace entries, never the graduation.
 */
export function graduationPrepared(graduation: Pick<GraduationPrediction, 'accounts' | 'names'>, existing: ReadonlySet<Address>): boolean {
  return PREPARED_SLOTS.every((name) => {
    const i = graduation.names.indexOf(name);
    if (i < 0) throw new Error(`graduationPrepared: the graduation list has no ${name}`);
    return existing.has(graduation.accounts[i]!.address);
  });
}

/**
 * Is a failed `link_trader` step benign for the trade that follows? `AlreadyLinked` (floor_launch 6093 — the trader linked
 * after the plan read its state) and `Expired` (6014, the attestation's deadline passed) both are: the trade still lands.
 * Any other failure is a real error.
 */
export function isBenignLinkFailure(customCode: number | null | undefined, logs: readonly string[] = []): boolean {
  if (customCode === 6093 || customCode === 6014) return true; // AlreadyLinked, Expired (floor_launch)
  return logs.some((l) => l.includes('Error Code: Expired') || l.includes('Error Code: AlreadyLinked'));
}

export interface PlanStep<S extends string> {
  readonly step: S;
  readonly tx: BuiltTransaction;
  /** A step whose failure is BENIGN — send the next step anyway (a `link` step; see {@link isBenignLinkFailure}). */
  readonly mayFail?: true;
  readonly compiled?: { transaction: ReturnType<typeof compileSendable>['transaction']; bytes: number };
}

function planPush<S extends string>(
  steps: PlanStep<S>[], what: string, payer: Address, step: S, tx: BuiltTransaction,
  a: { lookupTables?: readonly LookupTableContents[]; lifetime?: V0MessageArgs['lifetime'] },
): void {
  const compiled = a.lifetime
    ? compileSendable(`${what}/${step}`, { feePayer: payer, instructions: tx.instructions, lifetime: a.lifetime, ...(a.lookupTables ? { lookupTables: a.lookupTables } : {}) })
    : undefined;
  steps.push(compiled ? { step, tx, compiled } : { step, tx });
}

/**
 * A sale buy as ORDERED transactions for one wallet prompt: `prepare` (only while the launch is not prepared), then `buy`.
 * The caller sends each and waits for it to CONFIRM before the next, and stops at the first failure
 * (`sendSolanaTransactions`).
 *
 * ⛔ **The prepare rides whenever the launch is unprepared, not only when THIS buy looks final.** A buy sells out
 * whatever is left when it lands, and another buy can land first — so "could this buy complete the sale?" is a race the
 * client cannot win. An unprepared final buy dies `MaxInstructionTraceLengthExceeded`; a prepare in front of a buy that
 * turns out not to be final costs the buyer one transaction fee and nothing else (the creator's reserve reimburses its
 * rent, and it is idempotent). `existing` decides it ({@link graduationPrepared}); WITHOUT `existing` the plan cannot tell
 * and prepares (a fee, at worst a no-op). `prepared: true` skips the read and the step.
 * ⚠ The prepare's list is minimal (`bidLowerHint` at the open): preparation plants only the derived arrays, which every
 * list carries.
 */
export async function planBuy(
  a: BuildBuyArgs & {
    /** The caller knows the launch is prepared (skips the step). */
    prepared?: boolean;
    /**
     * The launch's own lookup table: its address and its ON-CHAIN contents in stored order. Every step is compiled
     * through it as well as `lookupTables` (the deployment table). A table that does not hold the launch's pool is refused
     * by name — it is not this launch's; a table that is deactivated or still has an authority is refused by name
     * (`LaunchTableDeactivatedError` / `LaunchTableNotFrozenError`) — pass `{ address, ...decodeLookupTable(data) }`.
     */
    launchAlt: LaunchTableState;
    lookupTables?: readonly LookupTableContents[];
    lifetime?: V0MessageArgs['lifetime'];
  },
): Promise<{ steps: PlanStep<'prepare' | 'buy'>[]; graduation: GraduationPrediction }> {
  if (!a.launchAlt) throw new Error('planBuy: the per-launch lookup table is mandatory — pass `launchAlt` (the launch table, recorded when it was created)');
  if (!a.launchAlt.addresses.includes(a.launch.ammPool)) throw new Error(`planBuy: lookup table ${a.launchAlt.address} does not hold the launch's pool ${a.launch.ammPool} — not this launch's table`);
  assertLaunchTableFrozen('planBuy', a.launchAlt);
  a = { ...a, lookupTables: [...(a.lookupTables ?? []), a.launchAlt] };
  const buy = await buildBuy(a);
  const steps: PlanStep<'prepare' | 'buy'>[] = [];
  const prepared = a.prepared ?? (a.existing ? graduationPrepared(buy.graduation, a.existing) : false);
  if (!prepared) {
    const prep = await buildPrepareGraduation({
      payer: a.buyer,
      launch: a.launch,
      rent: a.rent,
      bidLowerHint: buy.graduation.openTick + 1,
      ...(a.quote ? { quote: a.quote } : {}),
      ...(a.floorPool ? { floorPool: a.floorPool } : {}),
      ...(a.stairSteps !== undefined ? { stairSteps: a.stairSteps } : {}),
      ...((a.hooked ?? !!a.touch) ? { hooked: true } : {}),
      ...(a.existing ? { existing: a.existing } : {}),
      ...(a.ids ? { ids: a.ids } : {}),
    });
    planPush(steps, 'planBuy', a.buyer, 'prepare', prep, a);
  }
  planPush(steps, 'planBuy', a.buyer, 'buy', buy, a);
  return { steps, graduation: buy.graduation };
}

export interface BuildSellBackArgs {
  buyer: Address;
  launch: Pick<Launch, 'mint' | 'tokenVault' | 'wsolVault'>;
  /**
   * The launch's quote when it is not wSOL. The payout is the quote, out of
   * the launch's quote ATA (`launch.wsolVault`) into the buyer's; the four accounts ride as
   * remaining accounts. Omitted = the wSOL path (a lamport payout), unchanged.
   */
  quote?: Pick<QuoteSpec, 'mint' | 'tokenProgram'>;
  tokens: bigint;
  /** `getAtaExtensions(buyer's ATA)`: CPI Guard and the delegation already on the account. */
  buyerAta: { cpiGuardLocked: boolean; delegatedAmount: bigint };
  /**
   * ⛔ REQUIRED on a HOOKED launch — the
   * 4-account holder tail `[floor_holders, holder_book (w), floor_pool (ro), the seller's record (w)]`, so
   * the program can CPI `floor_holders::sync` for the seller's record when it exists (a sell-back is the one
   * thing that lowers a balance before graduation). Without it a hooked launch refuses
   * `HolderRecordMismatch`; ABSENT on an unhooked launch, whose counts are unchanged.
   * `floorPool` is the launch's (`Launch.floorPool`).
   */
  holder?: { floorPool: Address };
  /**
   * `true` puts the 5-account prefix (`saleRefPrefix`: the trader's
   * bucket in THIS launch's quote, the launch's sale account, launch_authority, hook_config, floor_hook) at the FRONT of the
   * remaining accounts, so the FLOOR Platform fee's 30 / 5 / 2 % slices are booked to the trader's tiers. The program
   * recognises the prefix by its shape, so a bucket that is not opened yet (or a sale referral account that is not opened,
   * or frozen) books nothing and the trade proceeds; a prefix naming another trader's bucket or another launch's sale
   * account is refused `TraderRefMismatch`. Omitted / false: no prefix.
   */
  traderRef?: boolean;
  ids?: ProgramIds;
}

/**
 *  `buildSellBack`. On a CPI-Guard ATA, `ApproveChecked(delegate = launch, amount = tokens)` is
 * the instruction IMMEDIATELY before `sell_back`, in the same message, and never approves more than
 * `tokens`. `Revoke` is appended when a delegation can remain afterwards: on the delegate path the
 * approval is replaced and fully consumed, so none remains; off it, a pre-existing approval would.
 * A failed transaction rolls the approve back with it, so there is no revoke-on-failure step.
 */
export async function buildSellBack(a: BuildSellBackArgs): Promise<BuiltTransaction> {
  const ids = a.ids ?? DEFAULT_PROGRAM_IDS;
  if (a.tokens <= 0n) throw new Error('buildSellBack: tokens must be > 0 (Nothing)');
  const launch = (await findLaunch(a.launch.mint, ids)).address;
  const sale = await saleAccounts(a.buyer, launch, a.launch, ids);
  const quoteLeg =
    a.quote && a.quote.mint !== WSOL_MINT
      ? {
          quoteMint: a.quote.mint,
          quoteTokenProgram: a.quote.tokenProgram,
          quoteVault: a.launch.wsolVault,
          buyerQuoteAta: (await findAta(a.buyer, a.quote.mint, a.quote.tokenProgram)).address,
        }
      : undefined;
  const holderTail = a.holder
    ? {
        floorHolders: ids.floorHolders,
        holderBook: (await findHolderBook(a.launch.mint, ids)).address,
        floorPool: a.holder.floorPool,
        holderRecord: (await findHolderRecord(a.launch.mint, sale.buyerAta, ids)).address,
      }
    : undefined;
  const saleRef = a.traderRef ? await saleRefPrefix({ trader: a.buyer, launchMint: a.launch.mint, quoteMint: a.quote?.mint ?? WSOL_MINT, ids }) : undefined;
  const main = sellBackIx({ ...sale, ...(quoteLeg ? { quoteLeg } : {}), ...(holderTail ? { holderTail } : {}), ...(saleRef ? { saleRef } : {}) }, a.tokens);
  const body: Instruction[] = [];
  if (a.buyerAta.cpiGuardLocked) body.push(approveCheckedIx(sale.buyerAta, a.launch.mint, launch, a.buyer, a.tokens, TOKEN_DECIMALS, TOKEN_2022_PROGRAM_ID));
  body.push(main);
  const residual = a.buyerAta.cpiGuardLocked ? 0n : a.buyerAta.delegatedAmount;
  if (residual > 0n) body.push(revokeIx(sale.buyerAta, a.buyer, TOKEN_2022_PROGRAM_ID));
  // ⚠ A hooked sell-back also runs the `sync` CPI: the two rows summed [est.].
  // ⚠ A referred sell-back also runs `floor_hook::credit_trader_bucket` and emits its event: `saleTraderRef` added [est.].
  const rows: CuLimitKey[] = ['sellBack', ...(holderTail ? (['holdersSync'] as const) : []), ...(saleRef ? (['saleTraderRef'] as const) : [])];
  return rows.length > 1 ? finalizeUnits('buildSellBack', sumCu(...rows), body, main) : finalize('buildSellBack', 'sellBack', body, main);
}

export interface BuildSellBackWsolArgs extends Omit<BuildSellBackArgs, 'quote'> {
  /**
   * The seller's SPL Token native (wSOL) account the payout lands in. Default: the seller's wSOL ATA
   * (`findAta(buyer, WSOL_MINT, SPL_TOKEN_PROGRAM_ID)`).
   */
  sellerWsol?: Address;
  /**
   * Prepend an ATA `CreateIdempotent` for the default wSOL ATA (the program requires an initialized account). Default
   * `true`; ignored when `sellerWsol` is given (that account must already exist).
   */
  createWsolAccount?: boolean;
}

/**
 * `buildSellBackWsol`: the same sell-back as `buildSellBack` on a wSOL-quoted launch, its payout paid as wSOL into the
 * seller's native SPL Token account instead of lamports (`sell_back_wsol`). Same payout, same fee, same CPI-Guard
 * approve / revoke handling; the wSOL ATA is created idempotently first unless the caller names its own account.
 * Leaves the wSOL wrapped: unwrap it with a `CloseAccount` when lamports are wanted.
 */
export async function buildSellBackWsol(a: BuildSellBackWsolArgs): Promise<BuiltTransaction> {
  const ids = a.ids ?? DEFAULT_PROGRAM_IDS;
  if (a.tokens <= 0n) throw new Error('buildSellBackWsol: tokens must be > 0 (Nothing)');
  const launch = (await findLaunch(a.launch.mint, ids)).address;
  const sale = await saleAccounts(a.buyer, launch, a.launch, ids);
  const defaultWsol = (await findAta(a.buyer, WSOL_MINT, SPL_TOKEN_PROGRAM_ID)).address;
  const sellerWsol = a.sellerWsol ?? defaultWsol;
  const createAta = a.sellerWsol === undefined && (a.createWsolAccount ?? true);
  const holderTail = a.holder
    ? {
        floorHolders: ids.floorHolders,
        holderBook: (await findHolderBook(a.launch.mint, ids)).address,
        floorPool: a.holder.floorPool,
        holderRecord: (await findHolderRecord(a.launch.mint, sale.buyerAta, ids)).address,
      }
    : undefined;
  const saleRef = a.traderRef ? await saleRefPrefix({ trader: a.buyer, launchMint: a.launch.mint, quoteMint: WSOL_MINT, ids }) : undefined;
  const main = sellBackWsolIx(
    { ...sale, sellerWsol, splToken: SPL_TOKEN_PROGRAM_ID, ...(holderTail ? { holderTail } : {}), ...(saleRef ? { saleRef } : {}) },
    a.tokens,
  );
  const body: Instruction[] = [];
  if (createAta) body.push(createAtaIdempotentIx(a.buyer, sellerWsol, a.buyer, WSOL_MINT, SPL_TOKEN_PROGRAM_ID));
  if (a.buyerAta.cpiGuardLocked) body.push(approveCheckedIx(sale.buyerAta, a.launch.mint, launch, a.buyer, a.tokens, TOKEN_DECIMALS, TOKEN_2022_PROGRAM_ID));
  body.push(main);
  const residual = a.buyerAta.cpiGuardLocked ? 0n : a.buyerAta.delegatedAmount;
  if (residual > 0n) body.push(revokeIx(sale.buyerAta, a.buyer, TOKEN_2022_PROGRAM_ID));
  const rows: CuLimitKey[] = [
    'sellBack',
    ...(createAta ? (['ataCreate'] as const) : []),
    ...(holderTail ? (['holdersSync'] as const) : []),
    ...(saleRef ? (['saleTraderRef'] as const) : []),
  ];
  return rows.length > 1 ? finalizeUnits('buildSellBackWsol', sumCu(...rows), body, main) : finalize('buildSellBackWsol', 'sellBack', body, main);
}

export async function buildCloseBuyerPosition(a: { buyer: Address; mint: Address; ids?: ProgramIds }): Promise<BuiltTransaction> {
  const ids = a.ids ?? DEFAULT_PROGRAM_IDS;
  const launch = (await findLaunch(a.mint, ids)).address;
  const pos = await findBuyerPosition(launch, a.buyer, ids);
  const main = closeBuyerPositionIx({ program: ids.floorLaunch, buyer: a.buyer, launch, buyerPosition: pos.address });
  return finalize('buildCloseBuyerPosition', 'closeBuyerPosition', [main], main);
}

/**
 * ⛔ **THE QUOTE LEG IS THE POOL'S OWN, READ OFF THE TWO ACCOUNTS THE CALLER ALREADY HOLDS**. Every hook instruction
 * pins `quote_mint` to `FloorPool.quote_mint` (`#[account(address = floor_pool.load?.quote_mint)]`) and then
 * `require_pool_side(&p, 0, …)` binds that mint, `vault0` and the token program to the AMM pool's CURRENCY 0 — so the
 * quote is always `mint0` (register_pool orders `base > quote`) and its token program is always `Pool.token_program0`.
 * ⛔ Two `Pick`s, not one: `quoteMint` is a FloorPool field and `tokenProgram0` a Pool field, and neither account
 * carries the other's half. Nothing is defaulted: a wSOL fallback would be refused `WrongPool` on any other quote.
 */
export interface QuoteLegSource {
  poolState: Pick<Pool, 'tokenProgram0'>;
  floorPool: Pick<FloorPool, 'quoteMint'>;
}

/** The hook's quote leg for one pool: `FloorPool.quoteMint` under `Pool.tokenProgram0`. */
export function quoteLegOf(a: QuoteLegSource): { quoteMint: Address; quoteTokenProgram: Address } {
  return { quoteMint: a.floorPool.quoteMint, quoteTokenProgram: a.poolState.tokenProgram0 };
}
