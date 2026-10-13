//! Constants every program, the quoter and the SDK share; where one exists, each names its EVM
//! twin. `crates/floor-math/tests/floor_types_pin.rs` holds every shared value equal to
//! floor-math's.
//!
//! ⛔ The AMM's constants — ticks, tick arrays, the fee caps, the hook flag BITS — are
//! `carpenter_types::constants`, re-exported here whole, so every path a FLOOR-side consumer spells
//! (`floor_types::constants::MAX_TICK`) still resolves; what is DEFINED here is the mechanism's own
//! table and nothing the AMM program may read.

pub use carpenter_types::constants::*;

// ---- FLOOR's hook flags — the mechanism's own set over the AMM's bits ----------------------
/// EVM FLOOR's mined address carries `0x20CC`.
pub const FLOOR_EVM_HOOK_FLAGS: u16 = BEFORE_INITIALIZE_FLAG
    | BEFORE_SWAP_FLAG
    | AFTER_SWAP_FLAG
    | BEFORE_SWAP_RETURNS_DELTA_FLAG
    | AFTER_SWAP_RETURNS_DELTA_FLAG;
/// FLOOR's Solana flags, `0x20CC | 0x4000 = 0x60CC`.
pub const FLOOR_HOOK_FLAGS: u16 = FLOOR_EVM_HOOK_FLAGS | REQUESTS_ACTIONS_FLAG;

// =========================================================================================
// FLOOR hook
// =========================================================================================

/// `HookConfig.max_rounds`: `before_swap` 0, `after_swap` 1, one `after_actions`. The in-swap bid
/// re-place uses round 1 `[ModifyPosition BID −L, Swap nudge]` and round 2 `[ModifyPosition BID +L]`.
pub const FLOOR_MAX_ROUNDS: u8 = 3;
/// `HookConfig.callback_accounts`: `[floor_pool w, trader_bucket w, event_authority r, floor_hook r]` on EVERY phase.
/// The second slot is the swapping trader's `["trader_ref", trader, quote_mint]` bucket under `floor_hook` — written only
/// by that trader's own swaps, so it adds no waiting between traders. The non-swap phases pass the same slot and the hook
/// ignores it there. Slots 3 and 4 are the callbacks' `#[event_cpi]` pair: `floor_hook`'s `["__event_authority"]` PDA and
/// the `floor_hook` program, both read-only (the callbacks emit their events with `emit_cpi!`).
/// ⛔ Part of the hook's interface header: every client builds the 4-account slice.
pub const FLOOR_CALLBACK_ACCOUNTS: u8 = 4;
/// Every FLOOR pool's tick spacing.
pub const FLOOR_TICK_SPACING: u16 = 60;

/// FLOOR token decimals. ⛔ The BASE mint's, minted by `floor_launch`; it has nothing to do
/// with the QUOTE mint's decimals, which are the quote's own and may be ANY value in
/// `MIN_QUOTE_DECIMALS..=MAX_QUOTE_DECIMALS`. The two always differ on this chain (wSOL is 9),
/// which is why the Solana mechanism never had the EVM's decimals-cancellation assumption.
pub const TOKEN_DECIMALS: u8 = 6;

/// The admissible decimals of a launch's QUOTE mint, inclusive: quote tokens of any decimals.
/// The only thing the ARITHMETIC bounds is the tranche price itself: `0 < p < 2^77`, and past
/// `d = 36` NO (cap, quote price) a u64 can carry prices a launch
/// (`p_min(d) = 100·10^d·2^64/(u64::MAX·SUPPLY) ≥ 2^77` from 37 up). So the ceiling is 36,
/// DERIVED, not chosen, by `floor_math::constants::some_launch_prices_at` and the `const`
/// asserts beside it.
/// ⛔ Pinned equal to `floor_math::constants::MIN/MAX_QUOTE_DECIMALS` by
/// `floor-math/tests/floor_types_pin.rs`; the derivation is written on that crate's pair. Enforced
/// at `floor_hook::list_quote` and again in `floor_math::tranche_price_x64`. ⚠ The EFFECTIVE
/// ceiling for a given launch is far lower and value-shaped — `MAX_TRANCHE_PRICE_X64` refuses a
/// (cap, quote price, d) triple by name (`d ≤ 14` at the Meme cap against a $1.00 quote) — which
/// is the honest place for a bound that depends on all three.
pub const MIN_QUOTE_DECIMALS: u8 = 0;
pub const MAX_QUOTE_DECIMALS: u8 = 36;
/// Total supply, base units (1B tokens × 1e6).
pub const SUPPLY: u64 = 1_000_000_000_000_000;
/// The classic three-way split's sale share, 30 %.
pub const SALE_TOKENS: u64 = 300_000_000_000_000;
/// The classic split's POL share, 10 %.
pub const POL_TOKENS: u64 = 100_000_000_000_000;
/// The classic split's ladder share, 60 %. Every pool is a staircase; a launch's own split is its
/// `sale_bps` / `pol_bps`.
pub const LADDER_TOKENS: u64 = 600_000_000_000_000;

/// The skim ramp's length in ticks.
pub const SKIM_RAMP_TICKS: i32 = 16_094;
/// The bid band's width before spacing rounding. The bid is
/// `[floor_tick − ceil_to_spacing(120, s), floor_tick]`; `floor_tick` is its CHEAP edge, the published
/// floor, and the backing solves to its DEAR edge.
pub const BID_BAND_TICKS: i32 = 120;
/// One doubling of the price, in ticks (the EVM's `FloorMath`).
pub const TICKS_PER_DOUBLING: i32 = 6_932;
/// The dynamic fee override every FLOOR swap carries (pips).
pub const LP_FEE_PIPS: u32 = 2_500;
/// The flat 10 % at-floor sell skim the EVM's per-launch family reads (`FloorPolicy.DEFAULT_SKIM_SELL_AT_FLOOR_BPS`);
/// kept for the EVM pin. A Solana pool is registered with [`skim_sell_at_floor_bps`]`(skim_sell_far_bps)` instead.
pub const DEFAULT_SKIM_SELL_AT_FLOOR_BPS: u16 = 1_000;
/// The buy-at-floor skim (`FloorPool.skim_buy_at_floor_bps`), written at `register_pool`: buys slide to 0 at the
/// floor.
pub const DEFAULT_SKIM_BUY_AT_FLOOR_BPS: u16 = 0;
/// ⛔ **THE SELL RAMP'S AT-FLOOR MULTIPLE**: the dynamic fee scales only the floor skim
/// (the EVM's `FloorPolicy.SKIM_SELL_AT_FLOOR_MULTIPLE_BPS = 15_000`). At the floor a sell pays
/// `skim_sell_far_bps · 15_000 / 10_000` (rounded down), written ONCE at `register_pool` into
/// `FloorPool.skim_sell_at_floor_bps` and NOT capped by `MAX_SKIM_FAR_BPS` (10 % far → 15 %). The creator
/// fee and holder tax never ramp; the ramp itself (`skim_bps`, seven arguments) is unchanged.
pub const SKIM_SELL_AT_FLOOR_MULTIPLE_BPS: u32 = 15_000;
/// The at-floor SELL endpoint a pool with far sell rate `sell_far` is registered with:
/// `sell_far · SKIM_SELL_AT_FLOOR_MULTIPLE_BPS / BPS`, rounded down (the EVM's integer division).
/// At the cap, `1_000 → 1_500`, so it always fits the field's `u16`.
pub const fn skim_sell_at_floor_bps(sell_far: u16) -> u16 {
    (sell_far as u32 * SKIM_SELL_AT_FLOOR_MULTIPLE_BPS / BPS as u32) as u16
}
/// ⛔ **THE AT-FLOOR SELL MULTIPLE IS PER LAUNCH**: a sell INTO the bid pays **1.5×** the far skim on the three
/// presets (Cook, Bluechip, Moonshot); Custom launches and `$FLOOR`'s genesis (a Custom launch) keep
/// [`SKIM_SELL_AT_FLOOR_MULTIPLE_BPS`] (1.5×). Buys still ramp to 0. Carried from `floor_launch` into
/// `floor_hook::register_pool` as the trailing `register::SkimSellMultipleArg` (u32 bps); an ABSENT argument reads
/// 1.5×.
pub const PRESET_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS: u32 = 15_000;
/// The window `register_pool` admits for the carried multiple (1× ..= 5×), refused by name outside it
/// (`floor_hook` `SkimSellMultipleOutOfRange`). At the far-skim cap (1,000) the top gives 5,000, a `u16`.
pub const MIN_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS: u32 = 10_000;
pub const MAX_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS: u32 = 50_000;
/// The at-floor SELL skim a pool may register with — `far × multiple / 10,000` — is at most 1,500 bps (15 %).
/// `floor_hook::register_pool` refuses a higher one (`SkimSellAtFloorAboveCap`, 7056) and `floor_launch` refuses a
/// Custom far sell × resolved multiple above it before the CPI (`SkimSellAtFloorAboveCap`, 6063). Exactly 1,500 is
/// admitted: 10 % far × 1.5×. Presets (at most 100 × 5 = 500) and `$FLOOR` (250 × 1.5 = 375) sit under it.
pub const MAX_SKIM_SELL_AT_FLOOR_BPS: u16 = 1_500;
const _: () = assert!(skim_sell_at_floor_bps(MAX_SKIM_FAR_BPS) == MAX_SKIM_SELL_AT_FLOOR_BPS);
/// The at-floor SELL endpoint at a given multiple: `sell_far · multiple_bps / BPS`, rounded down.
/// ⚠ Callers bound both inputs first (far `<= MAX_SKIM_FAR_BPS`, the multiple inside the window above); the
/// product is taken in u64 and the result saturates at `u16::MAX` rather than wrapping.
pub const fn skim_sell_at_floor_bps_at(sell_far: u16, multiple_bps: u32) -> u16 {
    let v = sell_far as u64 * multiple_bps as u64 / BPS as u64;
    if v > u16::MAX as u64 { u16::MAX } else { v as u16 }
}
const _: () = assert!(skim_sell_at_floor_bps_at(MAX_SKIM_FAR_BPS, MAX_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS) == 5_000);
const _: () =
    assert!(skim_sell_at_floor_bps_at(MAX_SKIM_FAR_BPS, SKIM_SELL_AT_FLOOR_MULTIPLE_BPS) == skim_sell_at_floor_bps(MAX_SKIM_FAR_BPS));
/// 1,000 tokens.
pub const MIN_SELLABLE: u64 = 1_000_000_000;
/// The DEFAULT `LaunchConfig.min_bid_lamports` — 0.05 SOL. The EVM keeps `MIN_BID_ETH = 0.001 ether`
/// as a hard library constant; on Solana it is CONFIGURATION with a hard range, because Solana spam
/// is cheaper.
pub const MIN_BID_LAMPORTS: u64 = 50_000_000;
/// The hard FLOOR on `min_bid_lamports`, 0.001 SOL — the EVM's own constant read as lamports. Below
/// it the three non-creation uses stop protecting anything: the ratchet's no-op guard admits every
/// spam call, the vault reserve stops holding back a placeable bid, and the nudge's dust budget
/// rounds to nothing.
pub const MIN_BID_LAMPORTS_FLOOR: u64 = 1_000_000;
/// The hard CEILING on `min_bid_lamports`, 5 SOL. Chosen against the same three uses, not against
/// pool creation: the ratchet returns early while `vault_eth` is under it, so a pool whose backing
/// never reaches the ceiling never ratchets again — its floor is frozen; the reserve is withheld
/// from every solve, so it is a permanent haircut on the published floor; and the nudge may spend up
/// to it per ratchet. ⛔ It is 100× the default, so a fat finger of one decimal place is still
/// inside the range and a typo of three is refused by name.
pub const MIN_BID_LAMPORTS_CEIL: u64 = 5_000_000_000;
// The ratchet bounty: the DEFAULT rate and ceiling, and the hard ranges governance may tune them in.
/// The DEFAULT ratchet bounty rate: **100 bps — half the EVM's `BOUNTY_BPS = 200`**, because a Solana
/// crank costs a fraction of a cent where EVM gas costs dollars.
pub const BOUNTY_BPS: u16 = 100;
/// The hard CEILING on the bounty RATE, 200 bps — the EVM's own rate. The bounty is backing leaving
/// the vault, so the rate may be tuned DOWN to zero and never above the number the EVM pays.
pub const MAX_BOUNTY_BPS: u16 = 200;
/// The DEFAULT bounty ceiling, 0.05 SOL — the same NUMBER as the EVM's `FloorPolicy.MAX_BOUNTY`
/// (0.05 ether) in the chain's own native unit.
pub const MAX_BOUNTY_LAMPORTS: u64 = 50_000_000;
/// The hard CEILING on the bounty CAP, 0.5 SOL — ten times the default. Above that one ratchet could
/// move more backing than a small pool's whole bid, and the shortfall the bid carries (it is
/// deployed up to `MAX_BOUNTY` short of the floor it was solved for) stops being a rounding matter.
pub const MAX_BOUNTY_LAMPORTS_CEIL: u64 = 500_000_000;

/// The team payee sweep's tip: keepers cover their cost plus a small incentive, never more.
/// `floor_hook::sweep_stair_payee` tips its sender `min(owed · bounty_bps / 10,000, max_bounty / TEAM_TIP_CAP_DIVISOR,
/// owed)` — the pool's own ratchet cap divided by 20: 0.0025 SOL at the 0.05 SOL default. An untipped sweep is always
/// open to the payee. The ratchet's own bounty (`bounty_for`) and `sweep_stair_fees`' tip are unchanged.
pub const TEAM_TIP_CAP_DIVISOR: u64 = 20;
/// At most ONE TIPPED payee sweep per pool per 24 hours (`StairOwners.last_tip_at`), so keepers take at most about
/// 365 × the cap per year. A sweep inside the
/// interval still pays the payee the WHOLE owed; it simply tips nobody (not an error).
pub const TEAM_TIP_INTERVAL_SECS: i64 = 86_400;
const _: () = assert!(MAX_BOUNTY_LAMPORTS / TEAM_TIP_CAP_DIVISOR == 2_500_000);

/// The share of a GROSS creator fee that goes to the referrer **on a referred launch**: 1000 bps = 10 %, the
/// EVM's own default.
///
/// ⛔ **It is a CONSTANT copied into `FloorPool.referral_share_bps` at `register_pool`, never a live
/// cross-program read.** The fee path is a SWAP CALLBACK whose account list is fixed at
/// `FLOOR_CALLBACK_ACCOUNTS`, so reading another account inside it would add an account lock to every swap of
/// every FLOOR pool and would make a swap FAIL when that account is absent. The rate is therefore resolved ONCE,
/// at registration, exactly as `min_bid` / `bounty_bps` / `max_bounty` are.
/// ⚠ The cost: retuning needs a `floor_hook` upgrade and reaches only pools registered after it; nobody can raise
/// the referral take on a pool that already holds backing.
/// The share is paid to the pool's own `FloorPool.referrer` WALLET by a permissionless claim. `register_pool`
/// writes this value when the referrer is non-zero and 0 otherwise.
pub const REFERRAL_CREATOR_SHARE_BPS: u16 = 1_000;
/// The hard ceiling on the referral share — the EVM's own bound (`bps > 10_000` reverts there). 10,000 means
/// the whole creator fee funds referrals and the creator accrues nothing: legal on both chains, and never the
/// default.
pub const MAX_REFERRAL_SHARE_BPS: u16 = 10_000;

/// Basis-point denominator.
pub const BPS: u64 = 10_000;
/// The 2.5 % platform cut of every quote inflow.
pub const PLATFORM_BPS: u16 = 250;
// ---- THE FLOOR PLATFORM FEE ("team fee" in code) -------------------------------------------
// On top of skim, creator fee and holder tax, rounded DOWN, never part of PLATFORM_BPS, never backing.
// ⛔ The two numbers are `LaunchConfig`'s CURRENT SETTING (`team_sale_bps()` / `team_pool_bps()`, all-zero
//    bytes read as the DEFAULTS below), set by `floor_launch::set_team_fees`, signed by the TEAM treasury's
//    governance only. `create_launch` COPIES the sale setting into `Launch.team_bps` and hands the pool
//    setting to `register_pool` (`RegisterPoolArgs.team_fee_bps`), which re-bounds it and writes
//    `FloorPool.team_fee_bps`. Each account keeps its rate for life: no live re-read, no batch rewrite; a
//    new setting reaches only launches created after it.
// ⛔ The CAPS are code, never governance data: only a program upgrade raises one. 0 is allowed.
// Each is pinned to its floor-math twin by `crates/floor-math/tests/floor_types_pin.rs`.
/// The FLOOR Platform fee on a bonding-sale buy AND sell-back, by default — 1 %.
pub const TEAM_SALE_FEE_BPS_DEFAULT: u16 = 100;
/// The most `set_team_fees` may set for the sale — 1 %.
pub const TEAM_SALE_FEE_BPS_MAX: u16 = 100;
/// The FLOOR Platform fee on a pool swap after graduation, by default — 0.25 %.
pub const TEAM_POOL_FEE_BPS_DEFAULT: u16 = 25;
/// The most `set_team_fees` may set for a pool — 0.5 %.
/// `register_pool` re-checks the rate it is handed against it (`TeamFeeOutOfRange`).
pub const TEAM_POOL_FEE_BPS_MAX: u16 = 50;
/// The hard cap on a launch's FAR skim rate, each side (the EVM's `FloorPolicy.MAX_SKIM_FAR_BPS`). The at-floor
/// SELL endpoint is DERIVED from the far rate ([`SKIM_SELL_AT_FLOOR_MULTIPLE_BPS`]) and is not capped by this
/// (10 % far → 15 % at the floor); the at-floor BUY endpoint is 0.
pub const MAX_SKIM_FAR_BPS: u16 = 1_000;
/// Creator fee cap per side (`MAX_CREATOR_BPS`): 5 % (the EVM's `FloorPolicy.MAX_CREATOR_BPS = 500`).
pub const MAX_CREATOR_BPS: u16 = 500;
/// The holder-rewards cap per side: 10 % (the EVM's `FloorPolicy.MAX_HOLDER_BPS = 1000`). `register_pool`
/// bounds `holder_buy_bps` / `holder_sell_bps` by it the way the creator and skim caps beside it are bounded.
/// `floor_math::take_fees` books `hfee = leg · holder_bps / 10_000` into `FloorPool.holder_owed`, a quote
/// liability; the exit is `floor_hook::pay_holder`, called by `floor_holders::claim`. The rate is copied once
/// at `register_pool` and the hook has no setter for it.
pub const MAX_HOLDER_BPS: u16 = 1_000;

/// Hook position salts.
pub const SALT_BID: u64 = 1;
pub const SALT_SEED: u64 = 3;
// Salts 2, 100 and 101 are retired. ⛔ Never re-use those numbers.
/// The staircase's position salts. Every one is found BY KEY among an instruction's remaining accounts
/// (`position_key(pool, hook_authority, salt)`), so no `#[derive(Accounts)]` slot moved for any caller.
/// - the OPEN TAIL above the staircase's top: 102;
/// - the CUSHION (the POL's fed quote, from 0.9 × the price up to it): 103;
/// - the POL RE-LIST (tokens the cushion bought back, from the price to 2 × it): 104;
/// - the RESERVE (every step not yet split off, as ONE token-only position over their whole range; graduation
///   happens inside the buy): 105;
/// - step i: `SALT_STAIR_STEP0 + i`, 110..=125 (`MAX_STAIR_STEPS` 16).
pub const SALT_STAIR_TAIL: u64 = 102;
pub const SALT_CUSHION: u64 = 103;
pub const SALT_POL_RELIST: u64 = 104;
pub const SALT_STAIR_RESERVE: u64 = 105;
pub const SALT_STAIR_STEP0: u64 = 110;
/// The staircase's storage width (`StairState.step_*`), and the most steps a pool may carry.
pub const MAX_STAIR_STEPS: u8 = 16;
/// How many STAIRCASE OPERATIONS one ratchet runs at most (Solana-only: the compute budget). An operation is one
/// of: collecting the step the market entered (its close and, when part of it is left, its re-open — 1 or 2
/// `modify_liquidity` CPIs), SPLITTING the next step off the reserve (the reserve's close, the step's open and the
/// reserve's re-open — 3), or collecting the open tail (1 or 2). What is left waits for the next ratchet (the ratchet
/// flags `needs_ratchet`), so a buy that ran through the whole staircase cannot push one ratchet past the compute
/// budget. An operation that needs no CPI (a step or reserve already empty) does not count. Raise it only on a
/// measured compute budget.
pub const STAIR_STEPS_PER_RATCHET: u8 = 1;

/// The staircase rent reserve's size in tick arrays: five follow-up rounds that each create one array. The budget
/// `create_launch` prepays is this many arrays plus `stair_steps + 1` position accounts (the cushion, the re-list and
/// every step after step 0, each opened once), at the rent rate in force at create
/// (`floor_launch::graduation_rent::stair_rent_budget_lamports`).
pub const STAIR_RENT_ROUNDS: u64 = 5;
/// What `floor_hook::reclaim_stair_array` pays its caller per array it closes (out of that array's reclaimed rent,
/// ≈ 0.05 % of it), and what `floor_launch::fund_stair_rent` pays its caller (out of the budget). A keeper sends only
/// what raises its own balance, so without it nobody would fund a reserve or close a reserve-owned array, and the
/// reserve would never fill or recycle. Two base fees.
pub const STAIR_RENT_TIP_LAMPORTS: u64 = 10_000;

// ---- Team steps and the team buy ----------------------------------------------------------------------------------
// A staircase pool may carry ONE set of OWNER-held step tokens (`FloorPool.stair_owner`, the `StairOwners` tail of its
// `StairState`): `$FLOOR`'s TEAM steps or a platform launch's TEAM-BUY steps (an equal share of every step). Their sale
// proceeds pay a PAYEE after FLOOR FIRST; their LP fees are never the payee's (all to the floor).

/// `FloorPool.stair_owner`: no owner-held steps.
pub const STAIR_OWNER_NONE: u8 = 0;
/// `$FLOOR`'s TEAM steps: layout "top", COUNTED against the floor, 40 % to the payee, 30-day
/// cliffs, a free withdrawal.
pub const STAIR_OWNER_TEAM: u8 = 1;
/// A platform launch's TEAM-BUY steps: BLENDED, COUNTED against the floor (paid for), 80 % to the payee, a ≥ 90-day lock, a
/// free withdrawal after it.
pub const STAIR_OWNER_TEAM_BUY: u8 = 2;

/// The team's HELD tokens: `withdraw_stair_tokens(STAIR_HELD_STEP)` pays the payee the owner's HELD tokens
/// (`StairOwners::held` — a step that left the staircase unsold), once the first step is unlocked: the EVM's
/// `withdrawTeamTokens` pays its `held` the same way. No real step has this index (`MAX_STAIR_STEPS` is 16).
pub const STAIR_HELD_STEP: u8 = u8::MAX;

/// The team buy: at most 5 % of supply, more than 0 when present — every mode but Bluechip ([`team_buy_max_bps`]).
pub const TEAM_BUY_MAX_BPS: u16 = 500;
/// Bluechip's team buy: at most 10 % of supply (the EVM twin's `FloorPolicy.BLUECHIP_MAX_TEAM_BUY_BPS`). Half of
/// Bluechip's 20 % sale; its team steps at 10 % are 1,000 / 7,900 = 12.7 % of its staircase, under the 25 % cap.
pub const BLUECHIP_TEAM_BUY_MAX_BPS: u16 = 1_000;
/// THE ONE READ of the team buy's ceiling per mode, bps of supply: Bluechip [`BLUECHIP_TEAM_BUY_MAX_BPS`], else
/// [`TEAM_BUY_MAX_BPS`] (the EVM's `FloorModes.maxTeamBuyBps`).
pub const fn team_buy_max_bps(mode: u8) -> u16 {
    if mode == MODE_BLUECHIP { BLUECHIP_TEAM_BUY_MAX_BPS } else { TEAM_BUY_MAX_BPS }
}
/// At least 80 % of the team buy on TEAM STEPS, at most 20 % (≤ 1 % of supply) to the wallet, on every preset;
/// up to 100 % may go to the steps.
pub const TEAM_STEPS_MIN_BPS: u16 = 8_000;
/// The team steps' cap: a launch's TEAM-BUY step tokens are at most 25 % of its staircase's step tokens (the team's
/// included) — a third of the protocol's own — and a team buy on a launch with no staircase is refused. The EVM twin
/// is `FloorPolicy.MAX_TEAM_STEP_SHARE_BPS` 2,500. `$FLOOR`'s TOP layout (`STAIR_OWNER_TEAM`) is not a team buy and
/// is not capped by this.
pub const TEAM_STEPS_MAX_SHARE_BPS: u16 = 2_500;
/// Team steps lock at least 90 days, the deployer's pick.
pub const TEAM_LOCK_MIN_DAYS: u16 = 90;
/// The longest team-steps lock a launch may pick: 1,825 days (five years) — the EVM's
/// `FloorPolicy.MAX_TEAM_BUY_LOCK_DAYS`.
pub const TEAM_DAYS_MAX: u16 = 1_825;
/// The payee's share of what a TEAM-BUY step's tokens sell for, after FLOOR FIRST (80 %; 20 % to the floor and the
/// pool through the two-number split).
pub const TEAM_BUY_PAYEE_BPS: u16 = 8_000;
/// The deployer's WALLET part vests 25 % at each of four cliffs (days after graduation): Cook and Moonshot 10 / 20 /
/// 40 / 80, Bluechip 30 / 60 / 90 / 120, Custom the deployer's choice.
pub const TEAM_VEST_DAYS_COOK_MOONSHOT: [u16; 4] = [10, 20, 40, 80];
pub const TEAM_VEST_DAYS_BLUECHIP: [u16; 4] = [30, 60, 90, 120];
/// A CUSTOM launch's four wallet cliffs: each `TEAM_CUSTOM_VEST_DAYS_MIN..=TEAM_CUSTOM_VEST_DAYS_MAX` days, STRICTLY
/// rising (no Custom launch vests faster than the shortest preset; parity with the EVM twin).
pub const TEAM_CUSTOM_VEST_DAYS_MIN: u16 = 10;
pub const TEAM_CUSTOM_VEST_DAYS_MAX: u16 = 365;
/// A team-buy payee's state: the deployer (the creator) / a pending handle (the bytes are its id, claimed like the fee
/// recipient) / resolved (the bytes are the wallet).
pub const TEAM_PAYEE_DEPLOYER: u8 = 0;
pub const TEAM_PAYEE_PENDING: u8 = 1;
pub const TEAM_PAYEE_RESOLVED: u8 = 2;


// =========================================================================================
// floor_launch
// =========================================================================================

/// `Launch.state`.
pub const LAUNCH_STATE_BONDING: u8 = 0;
pub const LAUNCH_STATE_GRADUATED: u8 = 1;
/// `$FLOOR`'s timed pro-rata raise. The ONE launch `init_genesis` moves out of
/// `LAUNCH_STATE_BONDING`. ⛔ Every instruction of the flat-price sale — `buy`, `sell_back`,
/// `graduate`, and `prepare_graduation` until `end_genesis_sale` has fixed the open — requires
/// `BONDING` and so REFUSES this state by construction; `graduate_genesis` moves it to
/// `LAUNCH_STATE_GRADUATED`, after which every generic post-graduation reader sees an ordinary launch.
pub const LAUNCH_STATE_GENESIS: u8 = 2;
/// `$FLOOR`'s raise AFTER `end_genesis_sale`: the clock has passed `saleEnd`, `contribute` is over and
/// `Launch.open_tick` is FIXED from the raise. `prepare_graduation` accepts this state (and BONDING);
/// `graduate_genesis` requires it. Solana-only: the EVM's `graduate` is one call, so it has no such step.
pub const LAUNCH_STATE_GENESIS_CLOSED: u8 = 3;

// ---- $FLOOR's Solana genesis — the twin of the EVM's `FloorProtocolSetup` ------------------------
// Every figure below is the EVM's in this chain's 6-decimal base units.

/// `S`: the whole supply (the same 1B every launch mints).
pub const GENESIS_SUPPLY: u64 = SUPPLY;
/// `$FLOOR`'s supply: **up to 42.5M sale (phase 1 20M + phase 2 22.5M) / 10M classic POL / 197M protocol steps /
/// 700.5M team steps / 10M open tail / 40M vested**. The team's allocation stands ON THE STAIRCASE
/// (`floor_math::stair::owner_tokens_top`); the 40M vested goes to `GenesisSale.beneficiary` in four 10M locks
/// ([`GENESIS_LOCKS`]). Both phases sell at the ONE FIXED price ([`GENESIS_SALE_PRICE_LAMPORTS_PER_MILLION`]): every
/// buyer gets SOL ÷ price. What is not sold joins the PROTOCOL's steps at graduation.
pub const GENESIS_SALE_TOKENS: u64 = 42_500_000_000_000;
const _: () = assert!(GENESIS_SALE_TOKENS == GENESIS_CORE_TOKENS + GENESIS_FF_TOKENS);
/// Phase 1 — Core: 20M (2 %) at the fixed price; 80 SOL fills it.
pub const GENESIS_CORE_TOKENS: u64 = 20_000_000_000_000;
/// Phase 2 — Friends & Family + Core: up to 22.5M (2.25 %) at the same fixed price; 90 SOL fills it (15 wallets × 6 SOL).
pub const GENESIS_FF_TOKENS: u64 = 22_500_000_000_000;
/// The fixed sale price: **4 SOL per 1M whole tokens** — phase 1's 80 SOL buys the whole 20M; the open is this price,
/// a 4,000-SOL market cap. A CONSTANT, not a setting (`floor_math::genesis` has the reasoning).
pub const GENESIS_SALE_PRICE_LAMPORTS_PER_MILLION: u64 = 4_000_000_000;
/// Core's 20M at the fixed price, 80 SOL (4 wallets × 20): phase 1's ceiling — a contribution past it is TRIMMED to what is left, and one
/// with nothing left is refused `GenesisPhaseSoldOut`.
pub const GENESIS_CORE_CAP_LAMPORTS: u64 = 80_000_000_000;
/// Phase 2's 22.5M at the fixed price, 90 SOL (15 wallets × 6): phase 2's ceiling — trimmed and refused exactly like Core's.
pub const GENESIS_FF_CAP_LAMPORTS: u64 = 90_000_000_000;
/// `P`: the classic protocol-owned liquidity, 10M (1 %), seeded tokens-only over the full-range tail.
pub const GENESIS_POL_TOKENS: u64 = 10_000_000_000_000;
/// The step layout: the PROTOCOL's steps, 197M (19.7 %): 60 % in steps 1–3 and 40 % over steps 4..N, each 0.95 × the
/// one below — a share of EVERY step (`floor_math::genesis_layout`).
pub const GENESIS_PROTOCOL_STAIR_TOKENS: u64 = 197_000_000_000_000;
/// The TEAM's steps, **700.5M** (70.05 %): [`GENESIS_TEAM_STEP3_TOKENS`] in step 3 and the rest (655.5M) over steps 4..N,
/// none in steps 1–2; paid 40 % of what they sell for after FLOOR FIRST, withdrawable in 30-day cliffs (lowest team
/// step first — step 3), free.
pub const GENESIS_TEAM_STAIR_TOKENS: u64 = 700_500_000_000_000;
/// The team's tokens in step 3 (index 2), 45M.
pub const GENESIS_TEAM_STEP3_TOKENS: u64 = 45_000_000_000_000;
/// The step layout needs step 4: `end_genesis_sale` takes at least 4 steps and `set_genesis_stair` refuses fewer on a
/// team pool (a typical raise gives 12–15, so it binds only on a raise of thousands of SOL or a top set too low).
pub const GENESIS_MIN_STAIR_STEPS: u8 = 4;
/// The staircase's steps, **897.5M** (89.75 %): protocol + team, one doubling of the price each from the open,
/// front-loaded ([`GENESIS_STAIR_GROWTH_BPS`], 0.95 × the step below), as many as reach the genesis authority's top.
pub const GENESIS_STAIR_TOKENS: u64 = 897_500_000_000_000;
const _: () = assert!(GENESIS_STAIR_TOKENS == GENESIS_PROTOCOL_STAIR_TOKENS + GENESIS_TEAM_STAIR_TOKENS);
/// The open tail above the steps, 10M (1 %).
pub const GENESIS_STAIR_TAIL_TOKENS: u64 = 10_000_000_000_000;
/// `I`: what graduation deposits for the staircase and its tail together, **907.5M** — plus every UNSOLD sale token
/// (they join the protocol's steps) — `Launch.ladder_tokens` (on `$FLOOR` it is the staircase's deposit).
pub const GENESIS_LADDER_TOKENS: u64 = 907_500_000_000_000;
const _: () = assert!(GENESIS_LADDER_TOKENS == GENESIS_STAIR_TOKENS + GENESIS_STAIR_TAIL_TOKENS);
/// `V`: 40M — four 10M `floor_vesting` locks to the beneficiary ([`GENESIS_LOCKS`]).
pub const GENESIS_VESTED_TOKENS: u64 = 40_000_000_000_000;
const _: () = assert!(
    GENESIS_SALE_TOKENS + GENESIS_POL_TOKENS + GENESIS_LADDER_TOKENS + GENESIS_VESTED_TOKENS == GENESIS_SUPPLY
);
/// `$FLOOR`'s steps are FRONT-loaded: step i holds `0.95^i` parts, each step 0.95 × the tokens of the one below.
pub const GENESIS_STAIR_GROWTH_BPS: u16 = 9_500;
/// Team steps: the payee's share of what a TEAM step's tokens SELL for, after FLOOR FIRST (40 %; 60 % goes through the
/// two-number split).
pub const GENESIS_TEAM_PAYEE_BPS: u16 = 4_000;
/// Team steps' cliffs: every 30 days from day 30 (anchored at the sale's end) one more team step — the lowest still
/// locked — becomes withdrawable.
pub const GENESIS_TEAM_FIRST_CLIFF_SECS: u32 = 30 * 86_400;
pub const GENESIS_TEAM_CLIFF_EVERY_SECS: u32 = 30 * 86_400;
/// `FloorPool.stair_kind`: which staircase parameter set a pool runs —
/// `floor_math::stair::{STAIR_KIND_PRESET, STAIR_KIND_GENESIS}`, pinned equal by `floor_types_pin.rs`.
pub const STAIR_KIND_PRESET: u8 = 0;
pub const STAIR_KIND_GENESIS: u8 = 1;
/// `$FLOOR`'s pool (the EVM's `FloorProtocolSetupFactoryModule`): floor skim 2.5 % far on both sides, creator fee
/// 0.5 % both sides to the deployer key, NO holder rewards, no referrer.
pub const GENESIS_SKIM_FAR_BPS: u16 = 250;
pub const GENESIS_CREATOR_BPS: u16 = 50;
/// Phase 2 has no clock: the ONE length a schedule carries is phase 1's [`GENESIS_CORE_PHASE_SECS`]. `init_genesis`
/// writes it and `set_genesis_schedule` REFUSES any other (`GenesisDurationNotFixed`); the instruction keeps its
/// `duration` argument so the wire does not move. Phase 2 runs from phase 1's end until `GenesisSale.authority` closes it
/// with `end_genesis_sale`, which rewrites `duration` to the real sale length (`now − open`), so `sale_end()` is the
/// moment it was closed (`GenesisSale::sale_end`).
pub const GENESIS_DEFAULT_DURATION: u64 = GENESIS_CORE_PHASE_SECS;
/// The ONE length a genesis schedule may carry — `GENESIS_DEFAULT_DURATION` (300 s, phase 1's).
pub const GENESIS_DURATION_SECS: u64 = GENESIS_DEFAULT_DURATION;
/// Phase 1 — Core: the first 300 s from the open.
pub const GENESIS_CORE_PHASE_SECS: u64 = 300;
/// Phase 2 ends when the authority closes it.
/// `contribute`'s phase bytes: 1 Core, 2 F&F. A contribution names its phase and the program refuses it in the other
/// (`GenesisWrongPhase`) — a Core transaction that lands late is refused, never re-priced as F&F.
pub const GENESIS_PHASE_CORE: u8 = 1;
pub const GENESIS_PHASE_FF: u8 = 2;
/// **20 SOL per wallet in phase 1**, CUMULATIVE over the wallet's phase-1 contributions
/// (`GenesisContribution.contributed − contributed_ff`), refused `GenesisWalletCapExceeded`. Lamports; no setter.
pub const GENESIS_CORE_WALLET_CAP_LAMPORTS: u64 = 20_000_000_000;
/// **5 SOL per wallet in phase 2** (18 × 5 = 90 SOL fits the 90-SOL phase), cumulative over its phase-2
/// contributions (`contributed_ff`), independent of its phase-1 cap; phase 2's total is [`GENESIS_FF_CAP_LAMPORTS`].
pub const GENESIS_FF_WALLET_CAP_LAMPORTS: u64 = 5_000_000_000;
/// The whitelist raise: the longest Merkle proof `contribute` accepts — a
/// tree of up to 2^24 (16.7M) wallets. A longer proof is refused `GenesisNotWhitelisted` before any hashing.
pub const GENESIS_MAX_PROOF_LEN: usize = 24;
/// The raise buyers' lock: each contributor's allocation vests LINEARLY over 30 days from the SALE END with a 14-day
/// cliff — nothing before day 14, `allocation · elapsed / 30 d` (floored) from day 14, all of it from day 30
/// (`floor_math::genesis::genesis_buyer_vested`). `claim_genesis` pays the newly vested part on every call.
pub const GENESIS_BUYER_VEST_SECS: u64 = 2_592_000; // 30 days
pub const GENESIS_BUYER_CLIFF_SECS: u64 = 1_209_600; // 14 days
const _: () = assert!(GENESIS_BUYER_CLIFF_SECS < GENESIS_BUYER_VEST_SECS);
/// Only `GenesisSale.authority` may run `end_genesis_sale` or `graduate_genesis` (`GenesisGraduationDeployerOnly`);
/// there is no time-based fallback. The refund is the safety valve: if `$FLOOR`'s raise has NOT graduated
/// `GENESIS_REFUND_DELAY_SECS` after the authority closed phase 2 (`end_genesis_sale`, i.e.
/// `GenesisSale::sale_end()`), every contributor may take back exactly what it contributed (`refund_genesis`), and
/// `graduate_genesis` is refused from that same second (`GenesisRefundWindowOpen`) — so a raise is either graduated
/// whole or refunded, never both. While phase 2 is still OPEN a lost key reopens nothing, so the key is backed up
/// before the open. ONE constant (the SDK's and the indexer's twins move with it).
pub const GENESIS_REFUND_DELAY_SECS: u64 = 14 * 86_400;

/// How old a `QuoteWalk` may be, counted from its FIRST step (`QuoteWalk.started_at`), when a later step continues
/// it or `create_launch` consumes it (`QuoteWalkStale` past it). Each hop's tick is read at its own step's clock, so
/// this bounds how far apart in time the hops of one price may be read: 60 s is ~150 slots, room for a dozen walk
/// transactions sent back to back under one `signAllTransactions`, and far inside the 1,800-s TWAP window every hop
/// reads. A stale walk is restarted (`walk_quote_chain(restart = true)`) or closed (`close_quote_walk`).
pub const QUOTE_WALK_MAX_AGE_SECS: i64 = 60;
/// `setSchedule`'s headroom: `openTime + duration + 545 days` must fit — the longest lock's end.
pub const GENESIS_SCHEDULE_HEADROOM: u64 = 545 * 86_400;
/// The locks, `(amount, cliff seconds, linear seconds)`, each `floor_vesting::create_schedule_at(start = saleEnd, …)` to
/// `GenesisSale.beneficiary`: four 10M chunks, one WHOLLY unlocked on each of day 3, 4, 5 and 6 after the sale's end.
/// ⚠ `create_schedule_at` refuses a zero linear length (the EVM's `lockAt` rule, `BadSchedule`), so each is the
/// smallest honest equivalent: a cliff ONE SECOND short of the day and a ONE-SECOND line — nothing vested before
/// `day − 1 s`, everything at exactly the day (`vested_after_cliff`: 0 at the cliff, the whole at `cliff + linear`).
pub const GENESIS_LOCKS: [(u64, u64, u64); 4] = [
    (GENESIS_LOCK_CHUNK, 3 * 86_400 - 1, 1),
    (GENESIS_LOCK_CHUNK, 4 * 86_400 - 1, 1),
    (GENESIS_LOCK_CHUNK, 5 * 86_400 - 1, 1),
    (GENESIS_LOCK_CHUNK, 6 * 86_400 - 1, 1),
];
/// One of the four locks: 10M.
pub const GENESIS_LOCK_CHUNK: u64 = 10_000_000_000_000;
const _: () = assert!(4 * GENESIS_LOCK_CHUNK == GENESIS_VESTED_TOKENS);
/// The EVM's opening-tick clamp, `[-120_000, 600_000]`, in ITS raw space (18-decimal base over
/// 18-decimal quote) — recorded, never used directly.
pub const GENESIS_EVM_OPEN_TICK_MIN: i32 = -120_000;
pub const GENESIS_EVM_OPEN_TICK_MAX: i32 = 600_000;
/// ⛔ THE ONE PLACE THE CLAMP IS NOT A COPY. A tick here is a price in RAW units — 6-decimal base per
/// lamport (9 decimals) — so the same PRICE sits `ln(10⁻³)/ln(1.0001) = −69,081.007` ticks below the
/// EVM's. The clamp keeps its MEANING (the same $FLOOR-per-SOL bounds): each bound shifted by −69,081.
/// The upper one (530,919) then lies past what a FLOOR pool may open at on this AMM — `create_launch`'s
/// own bound, `open < MAX_TICK − 120` (the bid band's width above the open; `sale::open_tick_in_bounds`)
/// — so it is that bound instead, 443,515: what "zero raise opens at the top" means on this chain.
/// ⚠ The lower bound never binds here: `u64::MAX` lamports opens at ≈ −133,600 (`floor_math::genesis`).
pub const GENESIS_DECIMALS_SHIFT_TICKS: i32 = -69_081;
pub const GENESIS_OPEN_TICK_MIN: i32 = -189_081;
const _: () = assert!(GENESIS_OPEN_TICK_MIN == GENESIS_EVM_OPEN_TICK_MIN + GENESIS_DECIMALS_SHIFT_TICKS);
pub const GENESIS_OPEN_TICK_MAX: i32 = 443_515;

/// `CreateLaunchParams.mode`: ONE table on both chains (the EVM's `FloorModes.Mode`), the presets by graduation
/// cap: Custom 0, Moonshot 1, Cook 2, Bluechip 3.
///
/// ⛔ These bytes are PERMANENT STORAGE on `FloorPool` / `Launch` / `RegistryEntry`, not just parameters —
/// `floor_math::Mode` carries the full hazard note, and the two crates are held equal by
/// `floor-math/tests/floor_types_pin.rs`, which is what catches a half-done renumber.
/// ⛔ A zeroed mode byte is Custom, whose cap of 0 is refused, never a silent preset. `floor_hook::mech::stair` and
/// `genesis` read the STORED byte after create, so a renumber needs a FRESH program set, never an in-place upgrade of
/// one with pools. The next mode APPENDS.
pub const MODE_CUSTOM: u8 = 0;
pub const MODE_COOK: u8 = 2;
pub const MODE_BLUECHIP: u8 = 3;
/// The Moonshot preset.
pub const MODE_MOONSHOT: u8 = 1;
/// The highest mode byte.
pub const MODE_MAX: u8 = MODE_BLUECHIP;

/// The preset registry. A preset is a ROW of `floor_launch`'s `["preset_registry"]` account, written once by its
/// authority (`add_preset`) and never edited; `set_preset_enabled` stops or restarts NEW launches choosing it. Ids are
/// the mode byte: 0 is Custom and never a row, 1 Moonshot / 2 Cook / 3 Bluechip are SEEDED by `init_preset_registry`,
/// and new presets APPEND from 4. ⛔ An id is never reused or reordered.
pub const PRESET_REGISTRY_SEED: &[u8] = b"preset_registry";
/// The highest id a row may take: 32 slots counting Custom's (the account is fixed-size), the EVM's `MAX_PRESET_ID` too.
pub const MAX_PRESET_ID: u8 = 31;
/// `PresetRegistry`'s struct length (the account is 8 more, the discriminator): 72 header bytes + 32 rows of 48.
pub const PRESET_REGISTRY_LEN: usize = 1_608;
/// One `PresetRowZc`.
pub const PRESET_ROW_LEN: usize = 48;
const _: () = assert!(PRESET_REGISTRY_LEN == 72 + PRESET_ROW_LEN * (MAX_PRESET_ID as usize + 1));

/// The supply split per launch: sale, POL and staircase are each at least `MIN_ZONE_BPS` of supply and add up to all
/// of it; each preset carries its own split, a Custom launch picks one inside the rule, and `0 / 0` on the wire is the
/// standard 30 / 10 / 60 (`SALE_TOKENS` / `POL_TOKENS` / `LADDER_TOKENS`).
pub const MIN_ZONE_BPS: u16 = 1_000;
pub const STANDARD_SALE_BPS: u16 = 3_000;
pub const STANDARD_POL_BPS: u16 = 1_000;

/// `LaunchConfig.cluster`.
pub const CLUSTER_MAINNET: u8 = 0;
pub const CLUSTER_DEVNET: u8 = 1;
pub const CLUSTER_LOCALNET: u8 = 2;

/// `CreateLaunchParams.pending_platform`. 0 = not pending.
pub const PENDING_PLATFORM_NONE: u8 = 0;
pub const PENDING_PLATFORM_TWITTER: u8 = 1;
pub const PENDING_PLATFORM_GITHUB: u8 = 2;
pub const PENDING_PLATFORM_DISCORD: u8 = 3;
pub const PENDING_PLATFORM_TELEGRAM: u8 = 4;

/// `FloorFactory._config` bounds.
/// ⚠ `MAX_URI_LEN` has no EVM twin (an ERC-20 has no metadata URI). The uri is DERIVED from the
/// template, so it can never reach this bound — `metadata.rs`'s const proof is what makes that a
/// fact, not a hope.
pub const MAX_NAME_LEN: usize = 64;
pub const MAX_SYMBOL_LEN: usize = 16;
pub const MAX_URI_LEN: usize = 200;
pub const MAX_PENDING_HANDLE_LEN: usize = 64;

/// The metadata host stored in `LaunchConfig.metadata_base`, and the template's fixed tail.
/// ⛔ Widening `MAX_METADATA_BASE_LEN` shrinks `LaunchConfig.reserved` by the same amount — move
/// both halves of the layout together, or the size assert fails the build.
pub const MAX_METADATA_BASE_LEN: usize = 64;
pub const METADATA_URI_SUFFIX: &str = ".json";

/// How long after `set_metadata_base` the permissionless `fill_metadata_uri` stays SHUT.
///
/// ⛔ **The window exists because the fill is permissionless AND irreversible.** A fill writes a
/// uri that nothing — not governance, not an upgrade, not the creator — can ever move, and its
/// value comes from a base governance supplies. With no delay, a wrong base could be locked onto
/// the WHOLE pre-host backlog by any third party in the first slot after `MetadataBaseSet`, for the
/// price of rent; governance's only remedy — set a corrected base first — is a race a bot wins by
/// default. ⚠ A launch created AFTER a bad set still locks that uri in at `create_launch`: that
/// exposure is one mint, at one launcher's own pace.
/// ⛔ **The size is not a taste**: it must cover the governance timelock
/// [`GOVERNANCE_TIMELOCK_SECONDS`], or governance could not execute a correction inside it. The
/// const proof below keeps the two in step: move the timelock and the build fails until this moves
/// with it.
/// ⚠ It costs the BACKLOG one wait, once, and a launch created after the base is set nothing at
/// all. Set the base before the first `create_launch` and the backlog is empty, so the window is
/// free.
pub const METADATA_FILL_DELAY_SLOTS: u64 = 1_800_000;

/// The governance timelock [`METADATA_FILL_DELAY_SLOTS`] must clear (seven days).
pub const GOVERNANCE_TIMELOCK_SECONDS: u64 = 604_800;
/// Solana's nominal slot time. ⚠ A slot count is a wall-clock ESTIMATE; this is the rate it is
/// estimated at, and a network running SLOWER than nominal makes the window longer, never shorter.
pub const NOMINAL_SLOT_MS: u64 = 400;
const _: () = assert!(METADATA_FILL_DELAY_SLOTS * NOMINAL_SLOT_MS >= GOVERNANCE_TIMELOCK_SECONDS * 1_000);

/// The recipient-claim attestation prefix.
pub const RECIPIENT_CLAIM_PREFIX: &[u8; 24] = b"FLOOR_RECIPIENT_CLAIM_V1";
/// `prefix ‖ floor_launch id ‖ cluster ‖ launch ‖ pending_id ‖ recipient ‖ deadline i64`.
pub const RECIPIENT_CLAIM_MSG_LEN: usize = 24 + 32 + 1 + 32 + 32 + 32 + 8;
/// Team steps: the attestor's message for a STEPS PAYEE claim
/// (`floor_launch::claim_team_payee` / `claim_genesis_payee`) — the fee recipient's message, field for field, under its
/// own 24-byte prefix so a fee-recipient attestation can never be replayed as a payee one (or the reverse). The
/// attestor signs `prefix ‖ floor_launch id ‖ cluster ‖ launch ‖ pending_id ‖ payee ‖ deadline i64 LE` (161 B).
pub const STAIR_PAYEE_CLAIM_PREFIX: &[u8; 24] = b"FLOOR_STAIR_PAYEE_CLM_V1";

/// The attestor's trader-link message prefix.
/// Its own 24 bytes, so a recipient-claim or payee attestation can never be replayed as a trader link (or the reverse).
pub const TRADER_LINK_PREFIX: &[u8; 24] = b"FLOOR_TRADER_REF_LINK_V1";
/// `prefix ‖ floor_launch id (32) ‖ cluster u8 ‖ trader (32) ‖ tier1 (32) ‖ tier2 (32) ‖ tier3 (32) ‖ deadline i64 LE`,
/// an absent tier the default key. `floor_launch::link_trader` builds it; the client never supplies it.
pub const TRADER_LINK_MSG_LEN: usize = 24 + 32 + 1 + 32 * 4 + 8;
const _: () = assert!(TRADER_LINK_MSG_LEN == 193);

/// The 193-byte message the attestor signs for a trader link. ONE spelling: `floor_launch::link_trader`,
/// the tests and (as a pinned port) the site's `traderLinkMessage`.
pub fn trader_link_message(
    floor_launch_id: &[u8; 32],
    cluster: u8,
    trader: &[u8; 32],
    tiers: [&[u8; 32]; 3],
    deadline: i64,
) -> [u8; TRADER_LINK_MSG_LEN] {
    let mut m = [0u8; TRADER_LINK_MSG_LEN];
    let cluster = [cluster];
    let deadline = deadline.to_le_bytes();
    let parts: [&[u8]; 8] =
        [TRADER_LINK_PREFIX, floor_launch_id, &cluster, trader, tiers[0], tiers[1], tiers[2], &deadline];
    let mut at = 0;
    for p in parts {
        m[at..at + p.len()].copy_from_slice(p);
        at += p.len();
    }
    debug_assert_eq!(at, TRADER_LINK_MSG_LEN);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The trader-link message, byte for byte.
    #[test]
    fn the_trader_link_message_is_193_bytes_in_the_spec_order() {
        assert_ne!(&TRADER_LINK_PREFIX[..], &RECIPIENT_CLAIM_PREFIX[..]);
        assert_ne!(&TRADER_LINK_PREFIX[..], &STAIR_PAYEE_CLAIM_PREFIX[..]);
        let m = trader_link_message(&[7; 32], 2, &[1; 32], [&[3; 32], &[4; 32], &[0; 32]], 1_900_000_000);
        assert_eq!(m.len(), 193);
        assert_eq!(&m[..24], b"FLOOR_TRADER_REF_LINK_V1");
        assert_eq!(&m[24..56], &[7u8; 32]);
        assert_eq!(m[56], 2);
        assert_eq!(&m[57..89], &[1u8; 32]);
        assert_eq!(&m[89..121], &[3u8; 32]);
        assert_eq!(&m[121..153], &[4u8; 32]);
        assert_eq!(&m[153..185], &[0u8; 32], "an absent tier is the default key");
        assert_eq!(&m[185..193], &1_900_000_000i64.to_le_bytes());
    }

    #[test]
    fn floor_flags_are_0x60cc() {
        assert_eq!(FLOOR_EVM_HOOK_FLAGS, 0x20CC);
        assert_eq!(FLOOR_HOOK_FLAGS, 0x60CC);
    }

    #[test]
    fn supply_splits_exactly() {
        assert_eq!(SALE_TOKENS + POL_TOKENS + LADDER_TOKENS, SUPPLY);
    }

    // 30 / 10 / 60 — Custom's standard split (its 60 % the staircase's reserve).
    #[test]
    fn the_split_is_30_10_60() {
        assert_eq!((SALE_TOKENS * 100 / SUPPLY, POL_TOKENS * 100 / SUPPLY, LADDER_TOKENS * 100 / SUPPLY), (30, 10, 60));
    }

    // The bid band and the EVM's skim defaults.
    #[test]
    fn band_multiple_and_skim_defaults() {
        assert_eq!(BID_BAND_TICKS, 120);
        assert_eq!((DEFAULT_SKIM_BUY_AT_FLOOR_BPS, DEFAULT_SKIM_SELL_AT_FLOOR_BPS), (0, 1_000));
        assert!(DEFAULT_SKIM_SELL_AT_FLOOR_BPS <= MAX_SKIM_FAR_BPS);
    }

    // The caps, and the sell ramp's at-floor endpoint = 1.5x far, uncapped.
    // Mutations: a cap left at its old value; the multiple applied as a cap (`min(.., MAX_SKIM_FAR_BPS)`)
    // or rounded up.
    #[test]
    fn caps_and_the_at_floor_sell_multiple() {
        assert_eq!((MAX_CREATOR_BPS, MAX_HOLDER_BPS, MAX_SKIM_FAR_BPS), (500, 1_000, 1_000));
        assert_eq!(SKIM_SELL_AT_FLOOR_MULTIPLE_BPS, 15_000);
        assert_eq!(skim_sell_at_floor_bps(0), 0);
        assert_eq!(skim_sell_at_floor_bps(150), 225, "Meme: 1.5 % far -> 2.25 % at the floor");
        assert_eq!(skim_sell_at_floor_bps(200), 300, "2 % far -> 3 % at the floor");
        assert_eq!(skim_sell_at_floor_bps(100), 150, "1 % far -> 1.5 % at the floor");
        assert_eq!(skim_sell_at_floor_bps(300), 450, "3 % far -> 4.5 % at the floor");
        assert_eq!(skim_sell_at_floor_bps(MAX_SKIM_FAR_BPS), 1_500, "no cap: 10 % far -> 15 %");
        assert_eq!(skim_sell_at_floor_bps(1), 1, "rounded down: 1.5 -> 1");
        assert_eq!(skim_sell_at_floor_bps(333), 499, "rounded down: 499.5 -> 499");
    }

    // The at-floor sell cap is 15 %.
    #[test]
    fn the_at_floor_sell_cap_is_fifteen_percent() {
        assert_eq!(MAX_SKIM_SELL_AT_FLOOR_BPS, 1_500);
        assert!(skim_sell_at_floor_bps_at(1_000, 15_000) <= MAX_SKIM_SELL_AT_FLOOR_BPS, "10 % far at 1.5x: exactly the cap");
        assert!(skim_sell_at_floor_bps_at(1_000, 16_000) > MAX_SKIM_SELL_AT_FLOOR_BPS);
        assert!(skim_sell_at_floor_bps_at(300, 50_000) <= MAX_SKIM_SELL_AT_FLOOR_BPS, "3 % far at 5x: exactly the cap");
        assert!(skim_sell_at_floor_bps_at(310, 50_000) > MAX_SKIM_SELL_AT_FLOOR_BPS);
    }

    // The at-floor sell multiple per launch — 5x on the presets, 1.5x on Custom and $FLOOR — and the window
    // register_pool admits.
    // Mutations: the preset multiple left at 1.5x; the window's top below 5x; the `_at` form rounding up or
    // ignoring its multiple.

    #[test]
    fn the_at_floor_sell_multiple_per_launch() {
        assert_eq!(PRESET_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS, 15_000);
        assert_eq!((MIN_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS, MAX_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS), (10_000, 50_000));
        assert!((MIN_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS..=MAX_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS).contains(&SKIM_SELL_AT_FLOOR_MULTIPLE_BPS));
        assert_eq!(skim_sell_at_floor_bps_at(200, PRESET_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS), 300, "every preset: 2 % far -> 3 % at the floor");
        assert_eq!(skim_sell_at_floor_bps_at(250, 15_000), 375, "$FLOOR: 2.5 % far -> 3.75 % (unchanged)");
        for far in [0u16, 1, 150, 333, MAX_SKIM_FAR_BPS] {
            assert_eq!(skim_sell_at_floor_bps_at(far, SKIM_SELL_AT_FLOOR_MULTIPLE_BPS), skim_sell_at_floor_bps(far), "{far}");
        }
        assert_eq!(skim_sell_at_floor_bps_at(1, 15_000), 1, "rounded down");
        assert_eq!(skim_sell_at_floor_bps_at(u16::MAX, u32::MAX), u16::MAX, "saturates, never wraps");
    }

    // The referral share and its ceiling, against the EVM's own numbers.
    // Mutation: the default raised above the ceiling, or the ceiling moved off 10,000.
    #[test]
    fn referral_share_is_the_evm_default_under_the_evm_ceiling() {
        assert_eq!(REFERRAL_CREATOR_SHARE_BPS, 1_000); // FloorReferralDistributor.creatorShareBps
        assert_eq!(MAX_REFERRAL_SHARE_BPS as u64, BPS); // bps > 10_000 reverts on the EVM
        assert!(REFERRAL_CREATOR_SHARE_BPS <= MAX_REFERRAL_SHARE_BPS);
    }

    // The platform fee: sale 1 % default and cap; pool 0.25 % default under a 0.5 % cap; every default sits inside
    // its cap, and both stay under the 2.5 % $FLOOR cut and never a share of it.
    // Mutations this kills: a default above its cap (a fresh config would out-charge what governance may set);
    // a cap moved off its number; the sale and pool numbers swapped.
    #[test]
    fn the_platform_fee_defaults_sit_inside_their_caps() {
        assert_eq!((TEAM_SALE_FEE_BPS_DEFAULT, TEAM_SALE_FEE_BPS_MAX), (100, 100));
        assert_eq!((TEAM_POOL_FEE_BPS_DEFAULT, TEAM_POOL_FEE_BPS_MAX), (25, 50));
        assert!(TEAM_SALE_FEE_BPS_DEFAULT <= TEAM_SALE_FEE_BPS_MAX);
        assert!(TEAM_POOL_FEE_BPS_DEFAULT <= TEAM_POOL_FEE_BPS_MAX);
        assert!(TEAM_SALE_FEE_BPS_MAX < PLATFORM_BPS && TEAM_POOL_FEE_BPS_MAX < PLATFORM_BPS);
    }

    /// The two-phase raise: phase 1 300 s at 20 SOL a wallet (80 SOL fills its 20M), phase 2 at 5 SOL a wallet (90 SOL
    /// fills its 22.5M), 4 SOL per 1M tokens, proofs of at most 24; the schedule carries phase 1's 300 s alone.
    /// The refund: 14 days from the authority's close.
    #[test]
    fn the_refund_opens_fourteen_days_after_the_close() {
        assert_eq!(GENESIS_REFUND_DELAY_SECS, 1_209_600);
        assert_eq!(QUOTE_WALK_MAX_AGE_SECS, 60);
    }

    #[test]
    fn the_two_phase_raise_is_core_then_ff_at_one_fixed_price() {
        assert_eq!(GENESIS_CORE_PHASE_SECS, 300);
        assert_eq!((GENESIS_DEFAULT_DURATION, GENESIS_DURATION_SECS), (300, 300));
        assert_eq!((GENESIS_CORE_WALLET_CAP_LAMPORTS, GENESIS_FF_WALLET_CAP_LAMPORTS), (20_000_000_000, 5_000_000_000));
        assert_eq!(18 * GENESIS_FF_WALLET_CAP_LAMPORTS, GENESIS_FF_CAP_LAMPORTS);
        assert_eq!(GENESIS_SALE_PRICE_LAMPORTS_PER_MILLION, 4_000_000_000);
        assert_eq!(GENESIS_CORE_CAP_LAMPORTS as u128 * 1_000_000_000_000, GENESIS_CORE_TOKENS as u128 * GENESIS_SALE_PRICE_LAMPORTS_PER_MILLION as u128);
        assert_eq!(GENESIS_FF_CAP_LAMPORTS as u128 * 1_000_000_000_000, GENESIS_FF_TOKENS as u128 * GENESIS_SALE_PRICE_LAMPORTS_PER_MILLION as u128);
        assert_eq!((GENESIS_PHASE_CORE, GENESIS_PHASE_FF), (1, 2));
        assert_eq!(GENESIS_MAX_PROOF_LEN, 24);
    }

    #[test]
    fn claim_message_is_161_bytes() {
        assert_eq!(RECIPIENT_CLAIM_MSG_LEN, 161);
    }

    /// `$FLOOR`'s genesis: 42.5M sale (20M phase 1 + 22.5M phase 2) / 10M POL / 197M + 700.5M steps / 10M tail /
    /// 40M vested in four 10M locks, wholly unlocked on days 3, 4, 5 and 6 = 1B; growth 0.95; the payee's 40 %;
    /// cliffs every 30 days from day 30.
    #[test]
    fn floor_genesis_splits_one_billion_with_four_short_locks() {
        assert_eq!(
            (GENESIS_SALE_TOKENS, GENESIS_POL_TOKENS, GENESIS_PROTOCOL_STAIR_TOKENS, GENESIS_TEAM_STAIR_TOKENS, GENESIS_STAIR_TAIL_TOKENS, GENESIS_VESTED_TOKENS),
            (42_500_000_000_000, 10_000_000_000_000, 197_000_000_000_000, 700_500_000_000_000, 10_000_000_000_000, 40_000_000_000_000)
        );
        assert_eq!((GENESIS_CORE_TOKENS, GENESIS_FF_TOKENS), (20_000_000_000_000, 22_500_000_000_000));
        // the step layout: 197M protocol + 700.5M team (45M of it in step 3), N ≥ 4
        assert_eq!((GENESIS_TEAM_STEP3_TOKENS, GENESIS_MIN_STAIR_STEPS), (45_000_000_000_000, 4));
        assert_eq!(GENESIS_SALE_TOKENS + GENESIS_POL_TOKENS + GENESIS_STAIR_TOKENS + GENESIS_STAIR_TAIL_TOKENS + GENESIS_VESTED_TOKENS, SUPPLY);
        assert_eq!(GENESIS_LADDER_TOKENS, 907_500_000_000_000);
        assert_eq!(GENESIS_LOCKS.iter().map(|l| l.0).sum::<u64>(), GENESIS_VESTED_TOKENS);
        for (i, (amount, cliff, linear)) in GENESIS_LOCKS.iter().enumerate() {
            assert_eq!((*amount, cliff + linear), (10_000_000_000_000, (3 + i as u64) * 86_400), "lock {i}: 10M whole at day {}", 3 + i);
            assert_eq!(*linear, 1, "the smallest line create_schedule_at accepts");
        }
        assert!(GENESIS_SCHEDULE_HEADROOM >= 6 * 86_400, "the longest lock ends inside the headroom");
        assert_eq!((GENESIS_STAIR_GROWTH_BPS, STAIR_KIND_PRESET, STAIR_KIND_GENESIS), (9_500, 0, 1));
        assert_eq!(GENESIS_TEAM_PAYEE_BPS, 4_000);
        assert_eq!((GENESIS_TEAM_FIRST_CLIFF_SECS, GENESIS_TEAM_CLIFF_EVERY_SECS), (30 * 86_400, 30 * 86_400));
    }
}
