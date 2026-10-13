/**
 * FLOOR's constants shared with the `floor_hook` / `floor_launch` / `floor_holders` / `floor_vesting` programs: ids,
 * seeds, the account and fee constants a reader or a trade needs, and the compute-unit rows. Every value is a copy of the
 * programs' Rust constants (`crates/floor-types`); `test/rust-parity.test.ts` checks them when the program source is
 * present.
 * ⚠ `SEEDS` and `CU_LIMITS` here are the UNION — `@floorsupply/carpenter-sdk`'s rows spread first, then FLOOR's — so one table
 *   serves every builder.
 */

import { address, type Address } from '@solana/kit';

import { CU_LIMITS as CARPENTER_CU_LIMITS, SEEDS as CARPENTER_SEEDS, MAX_TICK, MIN_TICK } from '@floorsupply/carpenter-sdk';

// ---- program ids ----------------
export const FLOOR_HOOK_PROGRAM_ID: Address = address('9uXLyyZginMWdUeu22kp7XV7cpV8183xfsVqUxKi5Viy');
export const FLOOR_LAUNCH_PROGRAM_ID: Address = address('8vfdPWfaA3b9GtJoFUwc36JKDaMfVCJ4oT9HrCrQ6Tdi');
/** `floor_holders` — the Token-2022 TRANSFER-HOOK holder tally. Equal to `floor_types::holders::FLOOR_HOLDERS_ID`. */
export const FLOOR_HOLDERS_PROGRAM_ID: Address = address('2EUkJXSwCdRAuhesw2D9FXyGfbi6Kw2yZDyQUnCwc7te');
/** `floor_vesting`, for reading its accounts. */
export const FLOOR_VESTING_PROGRAM_ID: Address = address('HHiwbXs3egRwtgEPEKx1PHpBU8gq5VSbhfrGrcCd4CSC');

// ---- seeds ---------------------------------------------------------
const utf8 = (s: string): Uint8Array => new TextEncoder().encode(s);
export const SEEDS = {
  ...CARPENTER_SEEDS,
  hookConfig: utf8('hook_config'),
  floorPool: utf8('floor_pool'),
  launchConfig: utf8('launch_config'),
  registryEntry: utf8('entry'),
  launch: utf8('launch'),
  saleVault: utf8('sale_vault'),
  buyer: utf8('buyer'),
  mint: utf8('mint'),
  mintAuthority: utf8('mint_authority'),
  lockAuthority: utf8('lock_authority'),
  launchAuthority: utf8('launch_authority'),
  /** `["quote", quote_mint]` under `floor_hook` — the FLOOR quote roster entry a launch on any quote but wSOL is
   * created against. */
  quotePolicy: utf8('quote'),
  // `["grad_rent", launch]`, the launch's graduation rent reserve.
  gradRent: utf8('grad_rent'),
  /** `["stair_rent", amm_pool]` under floor_hook. */
  stairRent: utf8('stair_rent'),
  /** `["stair", floor_pool]` under `floor_hook`, a staircase pool's
   *  `StairState` — created by `prepare_positions` (or inside the graduating buy), filled by graduation. */
  stair: utf8('stair'),
  /** `["platform_treasury"]` under `floor_hook` — the program-owned PLATFORM TREASURY. Its ADDRESS owns every token
   * account the 2.5 % `$FLOOR` cut is swept into (`ATA(treasury, quote)`, one per quote); its DATA is the governance that
   * alone may withdraw. No private key exists for it. */
  platformTreasury: utf8('platform_treasury'),
  /** `["team_treasury"]` under `floor_hook` — the program-owned
   * TEAM treasury, `PlatformTreasury`'s layout. Its ADDRESS owns every token account the platform fee
   * is swept into (`ATA(team, quote)`, opened by the first sweep that moves a team fee in that quote);
   * its DATA is the governance that alone may `withdraw_team`. Never crosses the platform treasury. */
  teamTreasury: utf8('team_treasury'),
  /** `["floor_platform_pool"]`
   * under `floor_hook` — the ONE record of `$FLOOR`'s pool (`FloorPlatformPool`), set once by
   * `designate_floor_pool`; its ADDRESS owns the wSOL TRANSIT account `credit_floor_pool` stages a cut
   * through (`ATA(record, wSOL)`, zero at rest). Never a treasury. */
  floorPlatformPool: utf8('floor_platform_pool'),
  /** `["trader_link", trader]` under `floor_launch`. */
  traderLink: utf8('trader_link'),
  /** `["trader_ref", trader, quote_mint]` under `floor_hook` — a trader's bucket in one quote. */
  traderRef: utf8('trader_ref'),
  /** `["trader_ref_vault"]` under `floor_hook` — the shared vault's authority (one ATA per quote). */
  traderRefVault: utf8('trader_ref_vault'),
  /** `["trader_ref_sale", launch]` under `floor_launch` — a launch's sale-path referral account. */
  traderRefSale: utf8('trader_ref_sale'),
} as const;

/**
 * `floor_holders`' seeds, every one under `FLOOR_HOLDERS_PROGRAM_ID`.
 * ⚠ `extraAccountMetas` is the Token-2022 transfer-hook INTERFACE's own seed, `["extra-account-metas", mint]` with a
 *   HYPHEN — `spl_transfer_hook_interface::get_extra_account_metas_address` derives it under the hook program, and
 *   Token-2022 looks the account up by exactly that address.
 */
export const HOLDERS_SEEDS = {
  /** `["holders_authority"]` — signs `floor_hook::pay_holder` (the only door holder money leaves by). */
  holdersAuthority: utf8('holders_authority'),
  /** `["holder_book", mint]` — one launch's tally (acc, eligible, indexed, unallocated, paid). */
  holderBook: utf8('holder_book'),
  /** `["extra-account-metas", mint]` — the transfer-hook interface's validation account. */
  extraAccountMetas: utf8('extra-account-metas'),
  /** `["holder", mint, token_account]` — ONE holder's record, keyed by its ATA, never its wallet. */
  holderRecord: utf8('holder'),
} as const;

/** `0x20CC | 0x4000`. */
export const FLOOR_HOOK_FLAGS = 0x60cc;
/**
 * The accounts every FLOOR hook callback takes (`ApprovedHook.callback_accounts`, copied into each pool as
 * `Pool.hook_callback_accounts`): `[floor_pool w, trader_bucket w, floor_hook event_authority r, floor_hook r]`. The
 * program set this SDK builds for carries 4; an older set carries fewer, and `createSolanaAdapter` refuses to build
 * trades against it (`program_version`).
 */
export const FLOOR_CALLBACK_ACCOUNTS = 4;

// ---- FLOOR --------------------------------------
export const FLOOR_TICK_SPACING = 60;
export const TOKEN_DECIMALS = 6;
export const SUPPLY = 1_000_000_000_000_000n;
/** The standard supply split: 30 / 10 / 60 (sale / POL / ladder). */
export const SALE_TOKENS = 300_000_000_000_000n;
export const POL_TOKENS = 100_000_000_000_000n;
export const LADDER_TOKENS = 600_000_000_000_000n;
export const TICKS_PER_DOUBLING = 6_932;
/**
 * The bid is a BAND `[floor_tick − W, floor_tick]`, `W = ceil_to_spacing(BID_BAND_TICKS, s)` — 120 at
 * spacing 60 (`floor_types::constants::BID_BAND_TICKS`, `floor_math::band`). The SDK reads it to
 * predict the bid's arrays at graduation.
 */
export const BID_BAND_TICKS = 120;
export const LP_FEE_PIPS = 2_500;
export const PLATFORM_BPS = 250;
/**
 * THE FLOOR PLATFORM FEE ("team fee" in code): a SALE rate and a POOL rate, each a governance-set CURRENT SETTING in
 * `LaunchConfig` under a cap, COPIED into each launch / pool at its creation and kept for life. ⛔ Informational: every
 * figure uses the ACCOUNT's own `Launch.teamBps` / `FloorPool.teamFeeBps` (0 on older accounts), and "what a new launch
 * pays now" is `launchConfigTeamSaleBps` / `launchConfigTeamPoolBps` on the decoded config — never these defaults.
 */
/** The sale's default, on every bonding-sale buy AND sell-back — 1 %. */
export const TEAM_SALE_FEE_BPS_DEFAULT = 100;
/** The most `set_team_fees` may set for the sale — 1 %. */
export const TEAM_SALE_FEE_BPS_MAX = 100;
/** The pool's default, on every swap after graduation — 0.25 %. */
export const TEAM_POOL_FEE_BPS_DEFAULT = 25;
/** The most `set_team_fees` may set for a pool — 0.5 %. */
export const TEAM_POOL_FEE_BPS_MAX = 50;
/** The FAR skim cap per side, 10 %. The sell skim ramps to its at-floor endpoint (`skimSellAtFloorBps`). */
export const MAX_SKIM_FAR_BPS = 1_000;
/** The creator fee cap per side: 5 %. */
export const MAX_CREATOR_BPS = 500;
/** The holder rate cap per side: 10 %. */
export const MAX_HOLDER_BPS = 1_000;

export const SALT_BID = 1n;
export const SALT_SEED = 3n;
/** The staircase's position salts (`floor_types::constants`): the open tail
 *  above the staircase, the cushion, the POL re-list, the RESERVE and step i's `SALT_STAIR_STEP0 + i`. Found BY KEY. */
export const SALT_STAIR_TAIL = 102n;
export const SALT_CUSHION = 103n;
export const SALT_POL_RELIST = 104n;
export const SALT_STAIR_RESERVE = 105n;
export const SALT_STAIR_STEP0 = 110n;
export const MAX_STAIR_STEPS = 16;
/** One staircase operation per ratchet (the compute budget); what is left sets `FloorPool.needsRatchet`. */
export const STAIR_STEPS_PER_RATCHET = 1;

/** `Launch.state`: the bonding sale, then graduated. Any other value is a launch that is not trading (read as `pending`). */
export const LAUNCH_STATE_BONDING = 0;
export const LAUNCH_STATE_GRADUATED = 1;
// ---- the PayeeSlots tail ----
// A launch names up to five creator-fee recipients and up to five team-buy payees; each slot is an ADDRESS (platform 0)
// or a PENDING HANDLE id (platform 1..=4).

/** At most five slots in a list. */
export const MAX_SLOTS = 5;
/** A list's shares sum to exactly this. */
export const SLOTS_TOTAL_BPS = 10_000;
/** 1 X / twitter, 2 github, 3 discord, 4 telegram; 0 = an address slot. */
export const MAX_SLOT_PLATFORM = 4;
/** The `PayeeSlots` tail's ACCOUNT offset in a `FloorPool` account: straight after the TWAP oracle (1,008). */
export const PAYEE_SLOTS_OFFSET = 1_008;
/** `size_of::<PayeeSlot>()`. */
export const PAYEE_SLOT_SIZE = 56;
/** `size_of::<PayeeSlots>()`. */
export const PAYEE_SLOTS_SIZE = 568;
/** The `FloorPool` account `register_pool` creates: 1,008 + 568 = 1,576 bytes. */
export const FLOOR_POOL_ACCOUNT_LEN_WITH_SLOTS = PAYEE_SLOTS_OFFSET + PAYEE_SLOTS_SIZE;
/**
 * The team's per-slot books (`floor_types::slots::TeamParts`), the `FloorPool` account's tail after the slots —
 * `out_mask` u128 @0 (bit 5·i + s: slot s exited step i), `part_tokens` @16, `part_quote` @24, `token_credit[5]` @32,
 * `quote_credit[5]` @72, `pending_ask[5]` @112.
 */
export const TEAM_PARTS_OFFSET = FLOOR_POOL_ACCOUNT_LEN_WITH_SLOTS;
/** `size_of::<TeamParts>()`. */
export const TEAM_PARTS_SIZE = 152;
/** The `FloorPool` account `register_pool` creates: 1,576 + 152 = 1,728 bytes. */
export const FLOOR_POOL_ACCOUNT_LEN_WITH_PARTS = TEAM_PARTS_OFFSET + TEAM_PARTS_SIZE;
/** `PayeeSlots.version` once `register_pool` wrote the lists. */
export const PAYEE_SLOTS_VERSION = 1;
/** `PayeeSlot.state`: an address slot from creation. */
export const SLOT_STATE_WALLET = 0;
/** A handle slot still waiting for its claim (`key` is the pending id; the share is HELD). */
export const SLOT_STATE_PENDING = 1;
/** A handle slot claimed (`key` is the claimed wallet; the pending id is in the `PayeeSlotsSet` event). */
export const SLOT_STATE_RESOLVED = 2;
/** Which list (`PayeeSlotsSet`'s `creator` / `team` list). */
export const SLOT_KIND_CREATOR = 0;
export const SLOT_KIND_TEAM = 1;
/** `FloorPool.stair_kind`: the staircase's parameter set — the presets' (0) or `$FLOOR`'s (1). */
export const STAIR_KIND_PRESET = 0;
export const STAIR_KIND_GENESIS = 1;

/**
 * The mode byte (`Launch.mode`, `FloorPool.mode`): 0 Custom, 1 Moonshot, 2 Cook, 3 Bluechip — the EVM's `FloorModes.Mode`
 * ordinals too. A zeroed byte is Custom.
 * ⛔ A DECODER MUST REFUSE AN ORDINAL IT CANNOT PLACE, never fall through to a default. Further modes append (4..=31).
 */
export const MODE_CUSTOM = 0;
export const MODE_MOONSHOT = 1;
export const MODE_COOK = 2;
export const MODE_BLUECHIP = 3;
/** Every mode byte the programs define, in `floor_math::Mode` order. */
export const MODES = [MODE_CUSTOM, MODE_MOONSHOT, MODE_COOK, MODE_BLUECHIP] as const;

export const PENDING_PLATFORM_NONE = 0;
export const PENDING_PLATFORM_TWITTER = 1;
export const PENDING_PLATFORM_GITHUB = 2;
export const PENDING_PLATFORM_DISCORD = 3;
export const PENDING_PLATFORM_TELEGRAM = 4;

/** Ticks from the price down to 0.9 × the price (`floor_math::stair::CUSHION_TICKS`): the cushion's width. */
export const CUSHION_TICKS = 1_054;
/** No protocol listing within ~3 % above the floor: `floor_math::stair::STAIR_CLEAR_TICKS`. */
export const STAIR_CLEAR_TICKS = 254;
/** The cushion's re-list cost markup, 1.03× in ticks (`floor_math::stair::RELIST_COST_MARKUP_TICKS`). */
export const RELIST_COST_MARKUP_TICKS = 296;
/** `floor_types::twap::CUSHION_TWAP_WINDOW_SECS`: the cushion sits under min(spot, the pool's TWAP over this window). */
export const CUSHION_TWAP_WINDOW_SECS = 900;
/** The cushion follows net buying: the net-flow meter's memory H in bps of the market cap
 *  (`floor_math::netflow::STAIR_CUSHION_NETFLOW_BPS`); 0 would be off. */
export const STAIR_CUSHION_NETFLOW_BPS: number = 130;
/** The meter's catch-up by volume K (`floor_math::netflow::STAIR_CUSHION_NETFLOW_RELAX_BPS`); 0 = none. */
export const STAIR_CUSHION_NETFLOW_RELAX_BPS: number = 0;
/** The half rule: a buy is remembered by its own last slice of quote, and one buy takes at most half of a meter that
 *  holds anything. */
export const STAIR_CUSHION_NETFLOW_HALF_RULE: boolean = true;
/**
 * `SetComputeUnitLimit` values: each an estimate (`[est.]`) or a measured figure × 1.5, capped at the 1.4 M
 * per-transaction limit. ⛔ Every builder prepends one: an unset limit is 0 under v1 transactions.
 */
export const CU_LIMITS = {
  ...CARPENTER_CU_LIMITS,
  // A graduated BUY on a HOOKED pool, the transfer-hook accounts appended (Token-2022's `execute` CPI into
  // floor_holders on the base leg): measured 144,895 CU; × 1.5 rounded up. Unhooked it measured 91,007.
  swapFloorBuy: 218_000,
  // A SELL: the in-swap bid re-place (up to 3 rounds + bisection) plus the hooked base leg's `execute`. A hooked sell
  // measured 243,762 CU (× 1.5 = 365,643); 520,000 [est.] is kept as the ceiling for the re-place's worst case.
  swapFloorSell: 520_000,
  buyFinal: 1_400_000, // 1.0M [est.] × 1.5, capped at the 1.4M tx limit. ⛔ EVERY sale buy uses it.
  // `initialize_pool` + up to four `initialize_tick_array` + four `initialize_position` [est.], deliberately generous:
  // the instruction is idempotent, so a call that runs out of compute is re-run rather than lost.
  prepareGraduation: 900_000,
  sellBack: 60_000, // 40k [est.] × 1.5
  closeBuyerPosition: 20_000, // [est.]
  /** `touch` / `open_record`: catch_up, a prefund-safe record create, one settle, emit_cpi [est.]. */
  holdersTouch: 60_000,
  /** `sync`: catch_up + one settle; no create, no CPI [est.]. */
  holdersSync: 40_000,
  // The trader link: Ed25519 read + the TraderLink create + the open_trader_bucket CPI + emit_cpi [est.].
  linkTrader: 90_000,
  saleTraderRef: 30_000, // a referred sale trade's extra: the credit_trader_bucket CPI + a transfer + emit_cpi [est.]
} as const;
export type CuLimitKey = keyof typeof CU_LIMITS;

/**
 * `RequestHeapFrame` sizes, in bytes (a multiple of 1,024, from 32 KiB to 256 KiB). `finalize` places one right after the
 * `SetComputeUnitLimit` of every builder whose `CU_LIMITS` key is listed here; none of this package's builders needs
 * more than the default 32 KiB.
 */
export const HEAP_FRAMES: Readonly<Partial<Record<CuLimitKey, number>>> = {};

/** The Instructions sysvar, which `link_trader` introspects for the attestor's Ed25519 signature. */
export const INSTRUCTIONS_SYSVAR_ID = address('Sysvar1nstructions1111111111111111111111111');
