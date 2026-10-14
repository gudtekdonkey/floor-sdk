/**
 * `predict`: the graduation accounts a sale buy needs before it is sent.
 *
 * - `graduationAccounts`: the 29 fixed accounts graduation reads (`hook_programdata` at 4, `pool_index_entry` from the
 *   pool key, `graduation_rent` at 28) then the tick arrays — and the rent the final buyer pays, from which `buildBuy`
 *   sets `max_graduation_lamports` = rent × 1.1. The bid's arrays cover every tick its band can take, out to
 *   `graduationFloorBound` — the floor solved on the vault LESS the pool's `min_bid` reserve.
 *
 * ⛔ Every address is DERIVED, then cross-checked against what the chain recorded (`Launch.amm_pool`,
 *    `Launch.floor_pool`); a disagreement throws by name instead of building a transaction against the wrong pool.
 */

import { AccountRole, type AccountMeta, type Address } from '@solana/kit';

import {
  BID_BAND_TICKS,
  FLOOR_TICK_SPACING,
  PLATFORM_BPS,
  SALE_TOKENS,
  SALT_BID,
  SALT_SEED,
  SALT_STAIR_RESERVE,
  SALT_STAIR_STEP0,
  SALT_STAIR_TAIL,
} from './constants.ts';
import {
  DYNAMIC_FEE_FLAG,
  MAX_TICK,
  MIN_TICK,
  SPL_TOKEN_PROGRAM_ID,
  SYSTEM_PROGRAM_ID,
  WSOL_MINT,
  MINT_POOL_COUNT,
  POOL,
  POOL_INDEX_ENTRY,
  POSITION,
  TICK_ARRAY_LEN_V2,
  findAmmConfig,
  findApprovedHook,
  findBitmap,
  findEventAuthority,
  findMintExclusivity,
  findMintPools,
  findQuoteMint,
  findPool,
  findPoolIndexEntry,
  findPosition,
  findProgramData,
  findTickArray,
  findVault,
  type RentExemptMinimum,
  bitmapAccountLen,
  ceilToSpacing,
  floorToSpacing,
  getSqrtPriceAtTick,
  tickArrayStartIndex,
} from '@floorlaunchpad/carpenter-sdk';
import type { FloorPool, Launch,  } from './layouts.ts';
import {
  DEFAULT_PROGRAM_IDS,
  findFloorPool,
  findGraduationRent,
  findHookAuthority,
  findHookConfig,
  findLaunch,
  findLaunchConfig,
  findStairState,
  type ProgramIds,
} from './pda.ts';
import { openTick, stairEdge } from './tickmath.ts';

/** An Anchor account's on-chain length: the 8-byte discriminator over the layout's struct size. */
const accountLen = (layout: { readonly size: number }): number => 8 + layout.size;

/**
 * Account lengths the final buy creates. ⛔ Lengths, never lamports: rent is read at runtime.
 * ⛔ DERIVED from the layouts, never typed — the program derives its own the same way (`graduation_rent.rs`
 *    `POSITION_LEN = 8 + Position::SIZE`). The two token-account lengths are SPL / Token-2022 shapes and stay written out.
 */
export const GRADUATION_ACCOUNT_LENS = {
  pool: accountLen(POOL),
  vaultSplToken: 165,
  /** A Token-2022 account for a Pausable mint: 165 + account_type + a zero-length `PausableAccount` TLV. */
  vaultToken2022Pausable: 170,
  /** The base vault of a HOOKED launch mint — 170 + the 5-byte `TransferHookAccount` TLV (`floor_launch` `base_vault_len`). */
  vaultToken2022PausableHooked: 175,
  poolIndexEntry: accountLen(POOL_INDEX_ENTRY),
  mintPoolCount: accountLen(MINT_POOL_COUNT),
  position: accountLen(POSITION),
  /** The `StairState` account, `8 + floor_types::layout::StairState::SIZE` (584). */
  stairState: 8 + 584,
  /** An owner pool's `StairState` carries the owner books' 440-byte tail —
   *  `floor_types::owners::STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS` (a team-buy launch, `$FLOOR`'s genesis). */
  stairStateWithOwners: 8 + 584 + 440,
  /** A tick array as created: the 3,888-byte body, then its 32-byte rent owner (3,920). */
  tickArray: TICK_ARRAY_LEN_V2,
} as const;

/**
 * The bid window: how far above the open tick the graduation bid is ASSUMED to land without a pool's `min_bid`.
 * ⚠ `[est.]`: a wSOL launch's day-one bid ≈ 96 % of open, ≈ 408 ticks; 1,200 ticks (≈ 11 %) covers it with margin and
 *   adds at most one tick array at spacing 60.
 * ⛔ A FLOOR, NEVER THE WHOLE ANSWER: graduation's ratchet solves the floor on `vault − min(vault, min_bid)`, and a
 *   token-quoted launch's `min_bid` can be a large share of its vault (one share of a stock quote), which lands the bid
 *   outside this window (`AccountNotEnoughKeys`, 3005, inside `floor_hook::Ratchet`). The window reaches
 *   {@link graduationFloorBound} whenever that is further, and is never narrower than this.
 */
export const GRADUATION_BID_WINDOW_TICKS = 1_200;

/**
 * Slack on the sellable supply in {@link graduationFloorBound}: `SALE_TOKENS / 10,000` (≈ 1 tick).
 * At graduation the ratchet's `sellable(t ≥ open) = C − pol_base` is `SALE_TOKENS` plus the POL's and
 * the ladder's liquidity-rounding dust (`graduate.rs` steps 6–7: the POL's refund is re-deposited as
 * ladder and cancels; what is left is each position's `amount1(L)` rounding). That dust is at most
 * `(√p_b − √p_a)/2^64 + 1` base units per position — ≈ 1.5e9 at the highest admissible open
 * (443,516), 2.7e-6 of the sale — so 1e-4 of the sale covers it 37× over.
 */
const SELLABLE_SLACK_DIVISOR = 10_000n;

/** `floor_math::position::quote_cost`: quote units to buy `base` at `tick`,
 * `floor(floor(base·2^64/sp)·2^64/sp)` — the solve's own cost function, exact. */
function quoteCost(base: bigint, tick: number): bigint {
  const sp = getSqrtPriceAtTick(tick);
  return (((base << 64n) / sp) << 64n) / sp;
}

/**
 * The CHEAPEST (highest) tick graduation's ratchet can publish as the floor, i.e. the furthest above
 * the open the bid band `[floor − W, floor]` can land — from what the launch fixed at create
 * (`price_x64`) and the pool's own `FloorPool.min_bid` (the quote's base units). Pure: no read.
 *
 * It is `floor_math::solve::solve_floor_tick_with` (the bisection, step for step) then
 * `band::publish`, run on the two WORST inputs the ratchet can meet at graduation:
 * - `q = backing_at_sellout(price_x64, PLATFORM_BPS) − min_bid` — the LEAST quote the solve can see.
 *   The vault is `intake(seedable − team_owed)`; step 4 requires `seedable ≥ raised + team_owed` (the sale's FLOOR
 *   Platform fee goes to the pool's `team_owed`, never through `intake`); the bound below still holds, `seedable − team_owed ≥ raised ≥ floor(SALE·p/2^64)` (`graduate.rs`
 *   step 4; a buy rounds its cost UP and a sell-back never pays out more than its tokens cost, so
 *   `contributed / allocation ≥ p` for every buyer); `intake` is monotone in its input and
 *   `PLATFORM_BPS` is the only cut `register_pool` admits; the reserve is `min(vault, min_bid) ≤
 *   min_bid`; and nothing leaves the vault before the solve — no bounty (`unpaid_skim` is 0: no swap
 *   precedes graduation), no teardown (no position exists yet). A donation only RAISES `q`.
 * - `sellable = SALE_TOKENS + SALE_TOKENS/10,000` — the MOST supply the bid must absorb above the
 *   open (`SELLABLE_SLACK_DIVISOR`).
 * `quote_cost` falls with the tick and rises with the base, so the smallest grid tick whose cost on
 * these inputs fits `q` is at or above the one the real solve finds: the real floor can only be
 * DEARER (a lower tick) than this, never cheaper.
 * ⚠ `q ≤ 0` (a launch created with `backing == min_bid`, which `create_launch` admits) returns the
 *   top of the grid — the floor stays undefined and a bid, if placed, sits there — and the window it
 *   implies is dozens of arrays: correct, and too large for one transaction.
 * ⚠ Assumes `min_sellable` ≤ the circulating supply at graduation. Above it the solve keeps the undefined floor and this does not apply.
 */
export function graduationFloorBound(args: { priceX64: bigint; minBid: bigint; spacing?: number; saleTokens?: bigint }): number {
  const s = args.spacing ?? FLOOR_TICK_SPACING;
  // The LAUNCH's own `sale_tokens` when known; `SALE_TOKENS` otherwise.
  const sale = args.saleTokens ?? SALE_TOKENS;
  const raised = (sale * args.priceX64) >> 64n; // sale.rs backing_at_sellout
  const backing = raised - (raised * BigInt(PLATFORM_BPS)) / 10_000n;
  const q = backing - args.minBid;
  const base = sale + sale / SELLABLE_SLACK_DIVISOR;
  let hi = ceilToSpacing(MAX_TICK, s); // solve.rs :946
  let lo = ceilToSpacing(MIN_TICK, s); // :947
  if (q > 0n && quoteCost(base, hi) <= q) {
    // :954-961. (`q ≤ 0` is `vault ≤ reserve`: no solve, the floor stays `ceil_to_spacing(MAX_TICK)`;
    // `cost(hi) > q` is :952. Both leave `hi`.)
    while (hi - lo > s) {
      let mid = ceilToSpacing(lo + Math.floor((hi - lo) / 2), s);
      if (mid >= hi) mid = hi - s;
      if (mid <= lo) mid = lo + s;
      if (quoteCost(base, mid) <= q) hi = mid;
      else lo = mid;
    }
  }
  return ceilToSpacing(hi + ceilToSpacing(BID_BAND_TICKS, s), s); // band::publish
}


/**
 * Graduation's ladder range, as `graduate.rs::ladder_range(open)` computes it:
 * `[ceil_to_spacing(MIN_TICK), floor_to_spacing(open)]`, the POL's own edges (so the POL's two arrays), which
 * `seed_inventory` STORES and places nothing over.
 */
export function ladderRange(openTick: number, s: number = FLOOR_TICK_SPACING): [lower: number, upper: number] {
  return [ceilToSpacing(MIN_TICK, s), floorToSpacing(openTick, s)];
}

/**
 * The names of graduation's 29 fixed accounts, in order — `programs/floor_launch/src/graduate.rs`'s `idx::*` constants.
 * The tick arrays follow them.
 */
export const GRADUATION_ACCOUNT_NAMES = [
  'launch_config',
  'carpenter_amm',
  'amm_config',
  'approved_hook',
  'hook_programdata',
  'pool',
  'bitmap',
  'vault0',
  'vault1',
  'pool_index_entry',
  'wsol_mint',
  'spl_token',
  'mint_exclusivity0',
  'mint_exclusivity1',
  'mint_pools0',
  'mint_pools1',
  'floor_hook',
  'hook_config',
  'floor_pool',
  'hook_authority',
  'position_seed',
  'position_bid',
  'system_program',
  'amm_event_authority',
  'hook_event_authority',
  'wsol_vault',
  'graduation_rent',
  // The AMM quote roster: two REQUIRED trailing slots on `initialize_pool` — the QUOTE's entry, then the base mint's.
  'quote_mint0',
  'quote_mint1',
  // The pool's StairState and the three positions the graduating buy opens on a staircase pool — step 0, the reserve,
  // the open tail.
  'stair_state',
  'position_stair_step0',
  'position_stair_reserve',
  'position_stair_tail',
] as const;

/**
 * Graduation's POL range, as `graduate.rs::pol_range(open)` computes it: `[ceil_to_spacing(MIN_TICK, s),
 * floor_to_spacing(open, s)]` — the FULL-RANGE tail, with the `upper <= lower → upper = lower + s` defence (unreachable
 * at every spacing). Graduation seeds the tail, so the array at the bottom of the grid (`start_index` −446,400 at spacing
 * 60) is in EVERY graduation's set; a list without it fails `AccountNotEnoughKeys` naming `tick_array`.
 */
export function polRange(openTick: number, s: number = FLOOR_TICK_SPACING): [lower: number, upper: number] {
  const lower = ceilToSpacing(MIN_TICK, s);
  let upper = floorToSpacing(openTick, s);
  if (upper <= lower) upper = lower + s;
  return [lower, upper];
}

export interface GraduationPrediction {
  /** 33 fixed accounts then the tick arrays (writable). */
  readonly accounts: AccountMeta[];
  /** `GRADUATION_ACCOUNT_NAMES` then `tick_array[<start>]` per array. */
  readonly names: string[];
  readonly pool: Address;
  readonly openTick: number;
  readonly tickArrayStarts: number[];
  /** Σ rent of every account the final buy creates (those not in `existing`). */
  readonly rentLamports: bigint;
  /** `ceil(rentLamports × 1.1)`. */
  readonly maxGraduationLamports: bigint;
}

/**
 *  `launch` is the decoded `Launch`; `rent` reads rent at runtime (`rentFromRpc`).
 * `existing` lists addresses already created (e.g. a tick array an outside LP opened), whose rent
 * the buyer will not pay.
 */
export async function graduationAccounts(args: {
  launch: Pick<Launch, 'mint' | 'priceX64' | 'openTick' | 'ammPool' | 'floorPool' | 'wsolVault'> & Partial<Pick<Launch, 'saleTokens' | 'teamStepTokens'>>;
  /**
   * The launch's quote when it is not wSOL — `launchQuoteMint(launch)` names the mint, the caller supplies its token
   * program. Slots 7, 10, 11, 12 and 14 of the list are the QUOTE's; omitted, they are wSOL's. ⛔ A wrong quote fails here, not on chain: the derived pool would not be `Launch.amm_pool`.
   */
  quote?: { mint: Address; tokenProgram: Address };
  /** The `Launch` PDA, for `["grad_rent", launch]` (index 28). `findLaunch(mint)` when omitted. */
  launchAddress?: Address;
  rent: RentExemptMinimum;
  existing?: ReadonlySet<Address>;
  /**
   * The bid's DEAR edge as a solve placed it. Pins the bid to the arrays of `[lower, lower + W]`
   * (the band, `W = ceil_to_spacing(BID_BAND_TICKS, s)`) and drops the window; a hint below the
   * lowest the bid can land (`ceil_to_spacing(open + 1)`) is raised to it.
   */
  bidLowerHint?: number;
  /**
   * The launch's decoded `FloorPool` (it exists from `create_launch`; `getFloorPool(src,
   * Launch.amm_pool)`), for its `min_bid` — the reserve graduation's ratchet holds back before it
   * solves the floor. With it the window reaches {@link graduationFloorBound}. ⛔ The POOL's value,
   * never `QuotePolicy.min_bid`: governance may retune the policy after the launch registered, and
   * the pool keeps what it was registered with.
   * ⛔ REQUIRED on a token-quoted launch (no `bidLowerHint`): without it this throws BY NAME rather
   * than predict a fixed window that can be short. Optional on wSOL, where omitting it keeps the fixed window.
   */
  floorPool?: Pick<FloorPool, 'minBid'> & Partial<Pick<FloorPool, 'stairSteps'>>;
  /**
   * The pool's `FloorPool.stairSteps` (read
   * from `floorPool` when omitted). A staircase pool's graduating buy places step 0, the reserve and the open tail INSIDE it
   * the arrays of the staircase's inner edges (`stairEdge(1)`,
   * `stairEdge(steps)`) ride in the list, with the StairState's and the three positions' rent priced. ⛔ REQUIRED since
   * (every pool is a staircase — a Custom launch's derived from its cap): 0 / absent throws by name.
   */
  stairSteps?: number;
  /**
   * The launch mint carries FLOOR's transfer-hook slot — its AMM
   * base vault is then 175 B, not 170 (`floor_launch` `base_vault_len`). Omitted = unhooked.
   */
  hooked?: boolean;
  ids?: ProgramIds;
}): Promise<GraduationPrediction> {
  const ids = args.ids ?? DEFAULT_PROGRAM_IDS;
  const { mint } = args.launch;
  const s = FLOOR_TICK_SPACING;
  const quoteMint = args.quote?.mint ?? WSOL_MINT;
  const quoteProgram = args.quote?.tokenProgram ?? SPL_TOKEN_PROGRAM_ID;

  const pool = (await findPool({ mint0: quoteMint, mint1: mint, fee: DYNAMIC_FEE_FLAG, tickSpacing: s, hook: ids.floorHook }, ids)).address;
  if (pool !== args.launch.ammPool) throw new Error(`graduationAccounts: derived pool ${pool} != Launch.amm_pool ${args.launch.ammPool}`);
  const floorPool = (await findFloorPool(pool, ids)).address;
  if (floorPool !== args.launch.floorPool) throw new Error(`graduationAccounts: derived floor_pool ${floorPool} != Launch.floor_pool ${args.launch.floorPool}`);
  const open = openTick(args.launch.priceX64);
  if (open !== args.launch.openTick) throw new Error(`graduationAccounts: open_tick(price_x64) = ${open} but Launch.open_tick = ${args.launch.openTick}`);
  const stairSteps = args.stairSteps ?? args.floorPool?.stairSteps ?? 0;
  if (!Number.isInteger(stairSteps) || stairSteps < 1) {
    throw new Error(
      `graduationAccounts: needs the pool's \`stairSteps\` (\`floorPool.stairSteps\`, or \`stairSteps\`; got ${String(stairSteps)}) — every pool is a staircase, and graduation's buy places it, so a list without its arrays is refused AccountNotEnoughKeys`,
    );
  }

  // Tick arrays: the BOUNDS of every position graduation opens (a ModifyPosition needs only the
  // arrays of its two bounds): the POL (`polRange`, the full-range tail — its lower bound
  // is the bottom array of the grid, ~440,000 ticks below `open`, ONE extra array, never the run
  // up to it), the ladder's stored edges (the POL's own two), the bid, and the staircase's inner edges.
  const [seedLower, seedUpper] = polRange(open, s);
  const [ladderLower, ladderUpper] = ladderRange(open, s);
  // deploy_bid: lower = max(floor_tick − W, ceil_to_spacing(current + 1)), upper = floor_tick; the
  // market is the open (nothing swaps before graduation) and the closing clamp keeps the floor at or
  // above `stand = ceil_to_spacing(open + 1 + W)` (ratchet.rs). So the bid's two bounds lie in
  // `[bidFloor, floor]`, and `floor` is at most `graduationFloorBound` (or `stand`, which the fixed
  // window below always covers: `stand ≤ bidFloor + W + s`).
  const band = ceilToSpacing(BID_BAND_TICKS, s);
  const bidFloor = ceilToSpacing(open + 1, s);
  const span = 60 * s;
  const starts = new Set<number>();
  for (const t of [seedLower, seedUpper, ladderLower, ladderUpper]) starts.add(tickArrayStartIndex(t, s));
  // the arrays of the staircase's inner edges — `graduate.rs::stair_array_starts`, the
  // one spelling graduation asks and `prepare_graduation` plants.
  {
    const openEdge = floorToSpacing(open, s); // a staircase reads no minimum start (graduate.rs step 7)
    for (const i of [1, stairSteps]) starts.add(tickArrayStartIndex(stairEdge(openEdge, i, s), s));
  }
  if (args.bidLowerHint !== undefined) {
    // [lower, lower + W] spans at most two arrays (W < 60·s), so its two ends name them both. It was
    // `lower + s` — the one-spacing bid that predates the band.
    const lower = Math.max(args.bidLowerHint, bidFloor);
    starts.add(tickArrayStartIndex(lower, s)).add(tickArrayStartIndex(lower + band, s));
  } else {
    const quoted = quoteMint !== WSOL_MINT;
    if (quoted && !args.floorPool) {
      throw new Error(
        `graduationAccounts: a launch quoted in ${quoteMint} needs \`floorPool\` (its FloorPool's min_bid) — graduation holds that reserve back before it solves the floor, and a token quote's reserve can put the bid past the fixed ${GRADUATION_BID_WINDOW_TICKS}-tick window (AccountNotEnoughKeys in floor_hook::Ratchet)`,
      );
    }
    // The historical window is the FLOOR of the prediction, never narrowed; the reserve-derived bound
    // extends it wherever it is further (a stock quote's one-share `min_bid`).
    let highest = bidFloor + GRADUATION_BID_WINDOW_TICKS + s;
    if (args.floorPool) {
      highest = Math.max(highest, graduationFloorBound({
        priceX64: args.launch.priceX64,
        minBid: args.floorPool.minBid,
        spacing: s,
        ...(args.launch.saleTokens !== undefined ? { saleTokens: args.launch.saleTokens } : {}),
      }));
    }
    const from = tickArrayStartIndex(bidFloor, s);
    const to = tickArrayStartIndex(highest, s);
    for (let st = from; st <= to; st += span) starts.add(st);
  }
  const tickArrayStarts = [...starts].sort((a, b) => a - b);
  return assembleGraduation({
    mint, pool, floorPool, quoteMint, quoteProgram, open, tickArrayStarts,
    launchAddress: args.launchAddress ?? (await findLaunch(mint, ids)).address,
    wsolVault: args.launch.wsolVault, quoted: !!args.quote, stair: true, rent: args.rent, ids,
    // a launch with team steps graduates into an owner pool (its StairState carries the books)
    ownerBooks: (args.launch.teamStepTokens ?? 0n) > 0n,
    ...(args.existing ? { existing: args.existing } : {}),
    ...(args.hooked ? { hooked: true } : {}),
  });
}

/**
 * The list once its tick arrays are chosen: the 32 fixed accounts, the arrays (writable), and the
 * rent of everything graduation creates ({@link graduationAccounts}: a sale's final buy and `prepare_graduation` send
 * the same list).
 */
async function assembleGraduation(args: {
  mint: Address; pool: Address; floorPool: Address; quoteMint: Address; quoteProgram: Address; open: number;
  tickArrayStarts: number[]; launchAddress: Address; wsolVault: Address; quoted: boolean;
  /** A staircase pool: its StairState and three positions are priced. */
  stair?: boolean;
  /** The StairState carries the owner books (priced at 1,032 bytes). */
  ownerBooks?: boolean;
  rent: RentExemptMinimum;
  existing?: ReadonlySet<Address>; hooked?: boolean; ids: ProgramIds;
}): Promise<GraduationPrediction> {
  const { mint, pool, floorPool, quoteMint, quoteProgram, open, tickArrayStarts, launchAddress, ids } = args;
  const s = FLOOR_TICK_SPACING;
  const hookAuthority = (await findHookAuthority(ids.floorHook)).address;
  const [
    launchConfig, ammConfig, approvedHook, programData, bitmap, vault0, vault1, poolIndexEntry,
    excl0, excl1, pools0, pools1, hookConfig, posSeed, posBid, ammEvents, hookEvents,
    graduationRent,
    quoteMint0,
    quoteMint1,
    stairState,
    posStairStep0,
    posStairReserve,
    posStairTail,
  ] = await Promise.all([
    findLaunchConfig(ids),
    findAmmConfig(ids),
    findApprovedHook(ids.floorHook, ids),
    findProgramData(ids.floorHook),
    findBitmap(pool, ids),
    findVault(pool, quoteMint, ids),
    findVault(pool, mint, ids),
    findPoolIndexEntry(pool, ids),
    findMintExclusivity(quoteMint, ids),
    findMintExclusivity(mint, ids),
    findMintPools(quoteMint, ids),
    findMintPools(mint, ids),
    findHookConfig(ids),
    findPosition(pool, hookAuthority, SALT_SEED, ids),
    findPosition(pool, hookAuthority, SALT_BID, ids),
    findEventAuthority(ids.carpenterAmm),
    findEventAuthority(ids.floorHook),
    findGraduationRent(launchAddress, ids),
    findQuoteMint(quoteMint, ids),
    findQuoteMint(mint, ids),
    findStairState(floorPool, ids),
    findPosition(pool, hookAuthority, SALT_STAIR_STEP0, ids),
    findPosition(pool, hookAuthority, SALT_STAIR_RESERVE, ids),
    findPosition(pool, hookAuthority, SALT_STAIR_TAIL, ids),
  ]);

  const R = AccountRole.READONLY;
  const W = AccountRole.WRITABLE;
  const fixed: [Address, AccountRole][] = [
    [launchConfig.address, R], // 0
    [ids.carpenterAmm, R], // 1
    [ammConfig.address, R], // 2
    [approvedHook.address, R], // 3
    [programData.address, R], // 4 hook_programdata
    [pool, W], // 5
    [bitmap.address, W], // 6
    [vault0.address, W], // 7 the QUOTE's vault (wSOL's unless `quote` says otherwise)
    [vault1.address, W], // 8 mint
    [poolIndexEntry.address, W], // 9
    [quoteMint, R], // 10 the quote mint
    [quoteProgram, R], // 11 its token program
    [excl0.address, R], // 12
    [excl1.address, R], // 13
    // 14: the QUOTE's MintPoolCount. Read-only on a wSOL launch (never written: wSOL is a LEGACY
    // quote) and WRITABLE on every other (the AMM's roster: a ROSTER quote is
    // COUNTED and its counter is created by the first pool on it; `graduate.rs::check_accounts_of`
    // requires the slot writable for every quote but wSOL, a legacy-array quote included).
    [pools0.address, args.quoted ? W : R], // 14
    [pools1.address, W], // 15
    [ids.floorHook, R], // 16
    [hookConfig.address, R], // 17
    [floorPool, W], // 18
    [hookAuthority, R], // 19
    [posSeed.address, W], // 20
    [posBid.address, W], // 21
    [SYSTEM_PROGRAM_ID, R], // 22
    [ammEvents.address, R], // 23
    [hookEvents.address, R], // 24
    [args.wsolVault, W], // 25
    // `graduation_rent`: its lamports pay for everything graduation creates.
    [graduationRent.address, W], // 26
    // The AMM's quote ROSTER: `initialize_pool` takes `quote_mint0` /
    // `quote_mint1` as two REQUIRED trailing slots. 27 is the QUOTE's entry — absent on wSOL (the implied quote), present on
    // a roster quote — and 28 the base mint's, which never exists. Both read-only.
    [quoteMint0.address, R], // 27
    [quoteMint1.address, R], // 28
    // The staircase: writable on every launch, created by `prepare_graduation` or inside the graduating buy.
    [stairState.address, W], // 29
    [posStairStep0.address, W], // 30
    [posStairReserve.address, W], // 31
    [posStairTail.address, W], // 32
  ];

  const arrays = await Promise.all(tickArrayStarts.map(async (st) => (await findTickArray(pool, st, ids)).address));

  // Rent of everything created, lengths from
  const existing = args.existing ?? new Set<Address>();
  const created: [Address, number][] = [
    [pool, GRADUATION_ACCOUNT_LENS.pool],
    [bitmap.address, bitmapAccountLen(s)],
    [vault0.address, GRADUATION_ACCOUNT_LENS.vaultSplToken],
    [vault1.address, args.hooked ? GRADUATION_ACCOUNT_LENS.vaultToken2022PausableHooked : GRADUATION_ACCOUNT_LENS.vaultToken2022Pausable],
    [poolIndexEntry.address, GRADUATION_ACCOUNT_LENS.poolIndexEntry],
    [pools1.address, GRADUATION_ACCOUNT_LENS.mintPoolCount],
    // The QUOTE's counter, priced on every non-wSOL launch exactly as `graduation_rent.rs`'s
    // `quote_pools` row prices it; `existing` credits it when a prior pool made it.
    ...(args.quoted ? [[pools0.address, GRADUATION_ACCOUNT_LENS.mintPoolCount] as [Address, number]] : []),
    // the SEED and BID positions
    [posSeed.address, GRADUATION_ACCOUNT_LENS.position],
    [posBid.address, GRADUATION_ACCOUNT_LENS.position],
    // the staircase: its StairState and three positions, on a staircase pool only (`graduation_rent.rs` `stair`).
    ...(args.stair
      ? ([
          [stairState.address, args.ownerBooks ? GRADUATION_ACCOUNT_LENS.stairStateWithOwners : GRADUATION_ACCOUNT_LENS.stairState],
          [posStairStep0.address, GRADUATION_ACCOUNT_LENS.position],
          [posStairReserve.address, GRADUATION_ACCOUNT_LENS.position],
          [posStairTail.address, GRADUATION_ACCOUNT_LENS.position],
        ] as [Address, number][])
      : []),
    ...arrays.map((a): [Address, number] => [a, GRADUATION_ACCOUNT_LENS.tickArray]),
  ];
  let rentLamports = 0n;
  for (const [addr, len] of created) if (!existing.has(addr)) rentLamports += BigInt(await args.rent(len));

  return {
    accounts: [...fixed.map(([address, role]) => ({ address, role })), ...arrays.map((address) => ({ address, role: W }))],
    names: [...GRADUATION_ACCOUNT_NAMES, ...tickArrayStarts.map((st) => `tick_array[${st}]`)],
    pool,
    openTick: open,
    tickArrayStarts,
    rentLamports,
    maxGraduationLamports: maxGraduationLamports(rentLamports),
  };
}

/** The predicted graduation rent × 1.1, rounded up. */
export function maxGraduationLamports(rentLamports: bigint): bigint {
  return (rentLamports * 11n + 9n) / 10n;
}

// floor_math::sale::{team_fee_of, team_net_of, team_gross_for} integer for integer (predict.test.ts
// pins the Rust vectors). `teamBps` is the LAUNCH's own Launch.teamBps (the config's sale setting
// at the launch's creation, kept for life), never a constant and never the config's CURRENT setting.
/** The platform fee on a sale amount (a buy's cost, a sell-back's payout): `floor(amount · teamBps / 10 000)`. */
export const saleTeamFee = (amount: bigint, teamBps: number): bigint => (amount * BigInt(teamBps)) / 10_000n;
/** What a GROSS `lamportsIn` buys with: `floor(gross · 10 000 / (10 000 + teamBps))` — the program's own net. */
export const saleNetOf = (gross: bigint, teamBps: number): bigint => (gross * 10_000n) / (10_000n + BigInt(teamBps));
/** The client's one ceiling: the gross whose net is exactly `net` — `ceil(net · (10 000 + teamBps) / 10 000)`. */
export const saleGrossFor = (net: bigint, teamBps: number): bigint => (net * (10_000n + BigInt(teamBps)) + 9_999n) / 10_000n;
