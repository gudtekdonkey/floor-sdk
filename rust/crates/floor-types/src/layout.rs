//! Every FLOOR account layout, as `#[repr(C)]` + `Pod` structs, built with
//! [`carpenter_types::layout!`] on the same [`Layout`] trait as the AMM's, which are
//! `carpenter_types::layout` and re-exported here whole.
//!
//! These are the accounts' bytes. Each program declares its own `#[account(zero_copy)]` twin
//! (Anchor needs the struct in the program crate for the discriminator, `Owner` and the IDL),
//! and those twins are held to these: each program asserts
//! `size_of::<Twin>() == <floor_types::layout::T>::SIZE` at compile time, and a test walks each
//! built IDL and requires the same field names, in the same order, at the same offsets and sizes
//! as [`Layout::FIELDS`] below, and the same discriminator as [`Layout::DISCRIMINATOR`].
//!
//! Offsets are STRUCT offsets; account offset = struct offset + 8 (the discriminator).
//!
//! ⛔ Layout rules: every u64 / U128 / I128 sits at an offset that is a multiple of 8, no
//! implicit padding (`Pod` refuses to derive otherwise), `size % 8 == 0 && align <= 8`.
//! `HookConfig` aligns to 2 and `MintExclusivity` to 4, so `align == 8` is never asserted.
//! ⛔ The `layout!` macro also requires the field list to be CONTIGUOUS (`offset + size` of one
//! field is the next field's offset, and the last ends at `SIZE`), so a field whose listed type
//! disagrees with the struct cannot pass by accident.
//! ⛔ `HookConfig`'s first eight bytes ARE the [`HookInterfaceHeader`] the AMM reads under any
//! approved hook: the header is defined in `carpenter_types::layout` and the test below pins
//! FLOOR's config to it.
//! FLOOR's config to it.

pub use carpenter_types::layout::*;

use crate::constants::MAX_METADATA_BASE_LEN;
use crate::disc::account as disc;
use crate::pod::U128;
use bytemuck::{Pod, Zeroable};
use carpenter_types::layout;

// =========================================================================================
// floor_hook
// =========================================================================================

/// `["hook_config"]` under `floor_hook`. Struct 232 B, account 240 B.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct HookConfig {
    // ---- the interface header (bytes 0..8) ----
    pub version: u8,
    pub bump: u8,
    /// 3 (`FLOOR_MAX_ROUNDS`).
    pub max_rounds: u8,
    /// 4 (`FLOOR_CALLBACK_ACCOUNTS`).
    pub callback_accounts: u8,
    /// `0x60CC`.
    pub flags: u16,
    pub authority_bump: u8,
    pub pad0: u8,
    // ---- floor_hook's own fields ----
    /// Upgrade authority at init. Signs the quote roster (`list_quote` / `set_quote_economics` /
    /// `retire_quote`) and the other hook governance instructions; handed over in two steps
    /// (`pending_governance`).
    /// ⛔ It is NOT the platform treasury's governance: that key is `PlatformTreasury.governance`,
    /// rotatable in two steps; the two start as the same deployer key and may diverge.
    pub governance: [u8; 32],
    /// `== ATA(platform_owner, WSOL)`: `platform_owner` is the `["platform_treasury"]` PDA, so this
    /// is the TREASURY'S wSOL ATA.
    pub platform_destination: [u8; 32],
    pub launch_program: [u8; 32],
    /// `PDA(["launch_authority"], launch_program)`.
    pub launch_authority: [u8; 32],
    pub hook_authority: [u8; 32],
    /// The `["platform_treasury"]` PDA, written once by `initialize_hook_config`. `floor_launch` reads it at a fixed
    /// offset (`create_launch`'s `hook_platform_owner`).
    pub platform_owner: [u8; 32],
    /// The candidate governance `set_hook_governance` names, zero when none is pending.
    pub pending_governance: [u8; 32],
}
layout!(HookConfig, name = "HookConfig", size = 232, disc = disc::HOOK_CONFIG, {
    version: u8 = 0, bump: u8 = 1, max_rounds: u8 = 2, callback_accounts: u8 = 3, flags: u16 = 4,
    authority_bump: u8 = 6, pad0: u8 = 7, governance: [u8; 32] = 8, platform_destination: [u8; 32] = 40,
    launch_program: [u8; 32] = 72, launch_authority: [u8; 32] = 104, hook_authority: [u8; 32] = 136,
    platform_owner: [u8; 32] = 168, pending_governance: [u8; 32] = 200,
});

/// `["quote", quote_mint]` under `floor_hook` — the FLOOR-side quote roster. Struct 168 B, account 176 B.
///
/// ⛔ **WHY A SEPARATE ACCOUNT.** Three things live here per quote and none fits anywhere else:
/// - `HookConfig` has no reserve left, and it is a singleton anyway;
/// - `FloorPool` is per POOL, written once at `register_pool`, and the bounds here are what that
///   write is CHECKED against — they have to exist before the pool does;
/// - `MIN_BID_LAMPORTS_FLOOR..=MIN_BID_LAMPORTS_CEIL` is ONE hard range, which cannot serve every
///   quote (`1_000_000..=5_000_000_000` is 0.001..5 SOL but $1..$5,000 of a 6-decimal USDC).
///
/// ⛔ **THE BOUNDS ARE IN THE QUOTE’S OWN BASE UNITS**, not in lamports and not in dollars.
/// `min_bid` is the ratchet’s no-op guard, the vault reserve held out of every solve and the
/// nudge’s dust budget, so a value in the wrong unit does not block a creation — it wedges the
/// mechanism of a live pool, quietly.
///
/// ⛔ **THIS ACCOUNT BOUNDS AT THE HOOK AND SUPPLIES AT THE LAUNCH.** `create_launch` reads
/// `min_bid` / `max_bounty` from HERE for a listed quote and from `LaunchConfig` for wSOL. There is
/// exactly ONE owner per quote, by construction: wSOL can never be listed (`list_quote` refuses it,
/// `QuoteAlreadyListed`), so a listed wSOL policy can never override `set_launch_economics`. The
/// hook itself reads only the RANGE (`register_pool` bounds `args.min_bid`, it never supplies it):
/// the launch reads the value off the policy, the hook checks that value against the same policy's
/// range. `set_quote_economics` (hook governance) retunes the value inside the range, the per-quote
/// twin of `set_launch_economics`; it moves NEW pools only.
///
/// ⚠ This account does NOT admit a mint to the AMM: the AMM's own quote admission is what lets a
/// POOL exist with that currency at all. `register_pool` requires BOTH — the AMM’s admission and
/// this policy.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct QuotePolicy {
    pub version: u8,
    pub bump: u8,
    /// The mint’s OWN `decimals`, copied at `list_quote` from the mint account and never again:
    /// a mint’s decimals never change, so the copy can never go stale.
    /// ⛔ Always inside `MIN_QUOTE_DECIMALS..=MAX_QUOTE_DECIMALS`; `list_quote` refuses anything
    /// else by name, and `floor_math::tranche_price_x64` refuses it again.
    pub decimals: u8,
    /// 1 once `retire_quote` has run: no NEW pool may be registered against this mint. ⛔ It never
    /// touches a pool that already exists — there is no way to un-quote a live pool, and there
    /// must not be one.
    pub retired: u8,
    pub pad0: [u8; 4],
    /// The mint this policy is for; the seed is derived from it, and this is the cross-check.
    pub quote_mint: [u8; 32],
    /// SPL Token or Token-2022, read off the mint account’s OWNER at `list_quote`. Stored so a
    /// caller cannot hand a FLOOR instruction the wrong token program for this mint.
    pub token_program: [u8; 32],
    /// The hard range a pool’s `min_bid` must sit in, in this quote’s base units — the per-quote
    /// form of `MIN_BID_LAMPORTS_FLOOR..=MIN_BID_LAMPORTS_CEIL`. `register_pool` reads THIS and
    /// bounds `args.min_bid` against it; it never supplies the value.
    pub min_bid_floor: u64,
    pub min_bid_ceil: u64,
    /// The ceiling on a pool’s `max_bounty` (the per-quote `MAX_BOUNTY_LAMPORTS_CEIL`).
    /// `register_pool` bounds `args.max_bounty` against it.
    pub max_bounty_ceil: u64,
    /// The `min_bid` a launch on THIS quote is registered
    /// Written by `list_quote` and `set_quote_economics`, always inside
    /// `min_bid_floor..=min_bid_ceil`; read by `floor_launch::create_launch` and carried into
    /// `RegisterPoolArgs.min_bid`, where the hook checks it against the same range. ⛔ Copied into
    /// a pool ONCE at `register_pool` and never again: retuning it moves new launches only.
    pub min_bid: u64,
    /// The `max_bounty` a launch on this quote is registered with (`<= max_bounty_ceil`); the twin
    /// of `LaunchConfig.max_bounty_lamports`. Same writers, same reader, same one-way copy.
    pub max_bounty: u64,
    pub reserved: [u8; 56],
}
layout!(QuotePolicy, name = "QuotePolicy", size = 168, disc = disc::QUOTE_POLICY, {
    version: u8 = 0, bump: u8 = 1, decimals: u8 = 2, retired: u8 = 3, pad0: [u8; 4] = 4,
    quote_mint: [u8; 32] = 8, token_program: [u8; 32] = 40, min_bid_floor: u64 = 72,
    min_bid_ceil: u64 = 80, max_bounty_ceil: u64 = 88, min_bid: u64 = 96, max_bounty: u64 = 104,
    reserved: [u8; 56] = 112,
});

/// `["platform_treasury"]` under `floor_hook` — the program-owned platform treasury. Struct 104 B, account 112 B.
/// The Solana twin of the EVM's `FloorPlatformTreasury` (its `FloorGovernance` half): the account whose ADDRESS
/// owns every token account the 2.5 % platform cut lands in, and whose DATA is the governance that may withdraw
/// from them.
///
/// ⛔ **ITS OWN ACCOUNT, NOT A `HookConfig` FIELD**: `HookConfig` has no reserve left.
/// ⛔ **ITS OWN GOVERNANCE, NOT `HookConfig.governance`** — the EVM shape (each governed contract holds
/// its own `governance` / `pendingGovernance`), with room for a pending key. Both start as the same
/// deployer key.
/// ⛔ Created ONCE, by Anchor `init` at `initialize_hook_config`, so a second creation is the
/// framework's own already-in-use refusal. No instruction closes it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct PlatformTreasury {
    pub version: u8,
    pub bump: u8,
    /// 1 = `floor_hook::register_pool` admits a quote with NO `QuotePolicy` (its terms derived: `min_bid` 0.001 whole
    /// token rounded up, `max_bounty` 0 — or the smart cap when the quote is a qualifying FLOOR token). Written only by
    /// `set_allow_all_quotes`, signed by `governance`; default ON: `initialize_hook_config` writes 1 when it creates
    /// the treasury. Carved from the front of `pad0`.
    /// ⚠ Read on the PLATFORM treasury only; the team treasury shares this layout and never reads it.
    pub allow_all_quotes: u8,
    pub pad0: [u8; 5],
    /// Signs `withdraw_platform` and step one of the handover. Never zero, never the treasury PDA
    /// itself (it can sign no transaction), never `LaunchConfig.attestor`.
    pub governance: [u8; 32],
    /// The key `set_treasury_governance` proposed; zero when no handover is pending (and the
    /// proposal of the zero key is the CANCEL, the EVM's `setGovernance(address(0))`).
    pub pending_governance: [u8; 32],
    pub reserved: [u8; 32],
}
layout!(PlatformTreasury, name = "PlatformTreasury", size = 104, disc = disc::PLATFORM_TREASURY, {
    version: u8 = 0, bump: u8 = 1, allow_all_quotes: u8 = 2, pad0: [u8; 5] = 3, governance: [u8; 32] = 8,
    pending_governance: [u8; 32] = 40, reserved: [u8; 32] = 72,
});

/// `["floor_platform_pool"]` under `floor_hook` — the pool that receives every other wSOL pool's platform cut
/// (`$FLOOR`'s). Struct 224 B, account 232 B. The Solana twin of the EVM's `Layout.floorPool` (recorded ONCE by
/// `SingletonFloorSetup._platformDestination`) together with that pool's `platformReceivedEth`. Other quotes' cuts
/// stay in the platform treasury.
///
/// ⛔ **ITS OWN ACCOUNT**: `HookConfig` has no free byte and `FloorPool` no `reserved` left. Only ONE
/// pool ever receives, so the receiving pool's lifetime meter lives here and `FloorPool` does not grow
/// (no `floor_holders` size reader moves).
/// ⛔ Created ONCE, by Anchor `init`, in `designate_floor_pool` — signed by `HookConfig.launch_authority`
/// (floor_launch's PDA; its genesis launch kind is the only caller, the EVM's `msg.sender ==
/// genesisFactory`) — so a second designation is the framework's already-in-use refusal (the EVM's
/// `FloorPoolRecorded`). No instruction closes it or rewrites its keys.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FloorPlatformPool {
    pub version: u8,
    pub bump: u8,
    pub pad0: [u8; 6],
    /// `$FLOOR`'s `FloorPool` account (`["floor_pool", amm_pool]`).
    pub floor_pool: [u8; 32],
    /// `$FLOOR`'s AMM pool (the EVM's `floorPool` pool id).
    pub amm_pool: [u8; 32],
    /// The `$FLOOR` mint.
    pub base_mint: [u8; 32],
    /// wSOL, always: only native (wSOL) pools feed `$FLOOR`; every other quote's cut stays in the
    /// platform treasury. The EVM's `target.quoteToken != quoteToken` → `BadQuoteToken` reads this.
    pub quote_mint: [u8; 32],
    /// Lifetime quote credited into `$FLOOR`'s backing by `credit_floor_pool` — the EVM's
    /// `target.platformReceivedEth`. Moves only UP.
    pub received: u64,
    /// How many `credit_floor_pool` calls moved a non-zero amount.
    pub credits: u64,
    /// The slot `designate_floor_pool` ran in.
    pub designated_slot: u64,
    pub reserved: [u8; 64],
}
layout!(FloorPlatformPool, name = "FloorPlatformPool", size = 224, disc = disc::FLOOR_PLATFORM_POOL, {
    version: u8 = 0, bump: u8 = 1, pad0: [u8; 6] = 2, floor_pool: [u8; 32] = 8, amm_pool: [u8; 32] = 40,
    base_mint: [u8; 32] = 72, quote_mint: [u8; 32] = 104, received: u64 = 136, credits: u64 = 144,
    designated_slot: u64 = 152, reserved: [u8; 64] = 160,
});

/// `["stair", floor_pool]` under `floor_hook` — the staircase. Struct 584 B, account 592 B. A staircase pool's
/// positions: its `steps` fixed token-only steps (one doubling of the price each, from the open), the RESERVE (every
/// step not yet split off, as ONE token-only position over their whole range), the open TAIL above them, the CUSHION
/// (the POL's fed quote, from 0.9 × the price up to it) and the POL RE-LIST (what the cushion bought back, from the
/// price to 2 × it) — each a hook position found BY KEY (`SALT_STAIR_STEP0 + i`, `SALT_STAIR_RESERVE`,
/// `SALT_STAIR_TAIL`, `SALT_CUSHION`, `SALT_POL_RELIST`).
///
/// ⛔ **GRADUATION INSIDE THE BUY**: graduation's `seed_inventory` fills it and places step 0, the reserve and the
/// tail in the graduating buy itself. Steps `[0, placed)` stand as their own positions (`step_liq`), steps
/// `[placed, steps)` ride in the reserve; the ratchet splits the next one off when the market enters the reserve.
///
/// ⛔ **ITS OWN ACCOUNT**: `FloorPool` has no room for sixteen ranges, and every reader that sizes it (`floor_holders`
/// first) would have to move. Created by `prepare_positions` ahead of the graduating buy (or, unprepared, by
/// `seed_inventory` inside it; the creator's graduation-rent reserve pays either way); never closed. Only graduation's
/// `seed_inventory` and the ratchet write it. The pool's flag (`FloorPool.stair_steps`) is what makes the ratchet ask
/// for it, so a caller that omits it fails by name.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct StairState {
    /// 0 while the account is only CREATED (`prepare_positions`); 1 once graduation filled it.
    pub version: u8,
    pub bump: u8,
    /// The step count (a copy of `FloorPool.stair_steps`).
    pub steps: u8,
    /// The first step still inside the RESERVE: steps `[0, placed)` stand as their own positions, `[placed, steps)`
    /// ride in the reserve (== `steps` once no reserve is left: the last step becomes its own position, never a
    /// one-step reserve). 1 after graduation on a staircase of three or more steps.
    pub placed: u8,
    /// The first step not yet sold through: the ratchet collects from here and stops at the first step the market
    /// has not entered. ≤ `placed`; == `steps` once the whole staircase sold (the tail is next).
    pub first_open: u8,
    /// 1 once the open tail is placed (graduation).
    pub tail_placed: u8,
    pub growth_bps: u16,
    /// The staircase's cheap edge: `floor_to_spacing(open tick)`, where step 0 starts.
    pub open_edge: i32,
    pub tail_lower: i32,
    pub tail_upper: i32,
    pub cushion_lower: i32,
    pub cushion_upper: i32,
    pub relist_lower: i32,
    pub relist_upper: i32,
    /// The cushion's cost basis (a zero reads as "none"): the ORIGINAL cost line, in ticks, of the cushion-bought
    /// tokens standing in the POL re-list (`floor_math::stair::cushion_cost_tick`, never a marked-up price): the
    /// re-list never re-opens them under 1.1 × it (`relist_cost_line`). Written by every re-list placement (0 when it
    /// carries none), carried through its close.
    pub relist_cost_tick: i32,
    /// What graduation deposited for the steps (65 % of supply on the presets), split once into the steps.
    pub stair_total: u64,
    /// What graduation deposited for the open tail (5 %).
    pub tail_total: u64,
    /// Lifetime meters, quote base units: collected from the steps, the reserve, the tail and the re-list; of that,
    /// sent to the cushion; sent to the floor vault (before the `$FLOOR` cut).
    pub collected_total: u64,
    pub to_pol_total: u64,
    pub to_floor_total: u64,
    /// The quote the cushion was LAST placed with (its size before sellers moved it).
    pub cushion_quote: u64,
    pub tail_liq: U128,
    pub cushion_liq: U128,
    pub relist_liq: U128,
    /// Step i's range and liquidity. The ranges of EVERY step are written at graduation (the planned ones: step i =
    /// `[stair_edge(i + 1), stair_edge(i)]`); `step_upper[i]` moves only DEARER (down in tick) as a partly bought
    /// step is re-opened over what is left; `step_lower[i]` never moves; `step_liq[i]` is its own position's
    /// liquidity — 0 while the step rides in the reserve, and 0 once it sold through.
    pub step_lower: [i32; 16],
    pub step_upper: [i32; 16],
    pub step_liq: [U128; 16],
    /// THE RESERVE: one token-only position (`SALT_STAIR_RESERVE`) holding every
    /// step from `placed` on, over `[step_lower[steps − 1], step_upper[placed]]` — or less, `reserve_upper` pulled
    /// dearer when the market stood inside it at a split. `reserve_liq` 0 = no reserve.
    pub reserve_lower: i32,
    pub reserve_upper: i32,
    pub reserve_liq: U128,
    /// Lifetime meters of THE VAULT'S BOUGHT-BACK TOKENS (they join the step above the price at ≥ 2 × their cost):
    /// tokens listed so far (the free ones fed beside them included), and the cost basis they carried.
    pub vault_listed_tokens: u64,
    pub vault_listed_cost: u64,
    /// The BOUGHT tokens those feeds carried (the EVM twin's `boughtTokens`, summed):
    /// `vault_listed_cost / vault_fed_bought` is the average cost that set the feeds' 2 × line.
    pub vault_fed_bought: u64,
    /// The pool's OWN staircase numbers, copied by graduation from its `FloorPool.stair_kind` set
    /// (`floor_math::stair::stair_params`) and read by every ratchet — the cushion's depth (the POL's bids from
    /// `cushion_bottom_bps` of the price up to it: 9,000 on the presets, 7,000 on `$FLOOR`) and its width in ticks
    /// (1,054 / 3,567), and the POL target (the POL's quote ÷ market cap: 1,500 / 2,000). Graduation always writes
    /// them.
    pub cushion_bottom_bps: u16,
    pub pol_to_mcap_bps: u16,
    pub cushion_ticks: i32,
    /// The steps' swept fees: the quote LP fees `sweep_stair_fees` collected from the staircase's positions, waiting
    /// in the pool's claims as STEP MONEY until the next ratchet splits it with the rest (`mech::stair`'s
    /// `take.collected`).
    pub swept_quote: u64,
}
layout!(StairState, name = "StairState", size = 584, disc = disc::STAIR_STATE, {
    version: u8 = 0, bump: u8 = 1, steps: u8 = 2, placed: u8 = 3, first_open: u8 = 4, tail_placed: u8 = 5,
    growth_bps: u16 = 6, open_edge: i32 = 8, tail_lower: i32 = 12, tail_upper: i32 = 16, cushion_lower: i32 = 20,
    cushion_upper: i32 = 24, relist_lower: i32 = 28, relist_upper: i32 = 32, relist_cost_tick: i32 = 36,
    stair_total: u64 = 40, tail_total: u64 = 48, collected_total: u64 = 56, to_pol_total: u64 = 64,
    to_floor_total: u64 = 72, cushion_quote: u64 = 80, tail_liq: U128 = 88, cushion_liq: U128 = 104,
    relist_liq: U128 = 120, step_lower: [i32; 16] = 136, step_upper: [i32; 16] = 200, step_liq: [U128; 16] = 264,
    reserve_lower: i32 = 520, reserve_upper: i32 = 524, reserve_liq: U128 = 528, vault_listed_tokens: u64 = 544,
    vault_listed_cost: u64 = 552, vault_fed_bought: u64 = 560, cushion_bottom_bps: u16 = 568, pol_to_mcap_bps: u16 = 570,
    cushion_ticks: i32 = 572, swept_quote: u64 = 576,
});

/// `["floor_pool", amm_pool]`. Struct 784 B, account 792 B. Field NAMES and values match the EVM's
/// `PoolState`; `_eth` means lamports. ⛔ Offsets do NOT, and are not meant to: this is an
/// independent account format (a packed `repr(C)` struct), the EVM's is a Solidity struct of
/// 32-byte slots. Fields are ordered by ALIGNMENT so every carve is padding-free; do not "correct"
/// the order toward the EVM's: moving one offset silently rewrites every `FloorPool` on chain.
///
/// The layout is append-only, so unused slots stay in place as padding.
/// `crates/floor-math/src/state.rs` `FloorState` mirrors this field for field.
///
/// The referral fields at 584..: an ALL-ZERO `referrer` is the "this pool funds no referral"
/// sentinel; ⛔ read it through [`FloorPool::referral_destination`], never raw.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FloorPool {
    pub version: u8,
    pub bump: u8,
    pub pool_bound: u8,
    pub quote_is_token1: u8,
    pub ladder_seeded: u8,
    pub needs_ratchet: u8,
    pub mode: u8,
    /// 1 iff the last `floor_hook::observe_quote` read the pool's quote vault FROZEN or its quote mint
    /// PAUSED; every permissionless mechanism entry (`ratchet`, the `seed_*` steps) refuses
    /// `QuoteSuspended` while it stands. Written and cleared by that one instruction, from a live
    /// read — no authority sets it. 0 = live.
    pub quote_suspended: u8,
    pub tick_spacing: i32,
    pub floor_tick: i32,
    pub amm_pool: [u8; 32],
    /// The Launch PDA.
    pub pool_initializer: [u8; 32],
    pub base_mint: [u8; 32],
    pub quote_mint: [u8; 32],
    /// Unused, written zero. The creator slots are the account's `PayeeSlots` tail (`crate::slots`).
    pub pad9: [u8; 32],
    /// Unused, written zero.
    pub pad10: [u8; 32],
    pub skim_buy_far_bps: u16,
    pub skim_sell_far_bps: u16,
    pub creator_buy_bps: u16,
    pub creator_sell_bps: u16,
    pub holder_buy_bps: u16,
    pub holder_sell_bps: u16,
    /// The wall's offset from the floor in ticks, `ceil(ln(k / 100) / ln(1.0001))` for k in hundredths
    /// (954 at 1.10×, 6,932 at 2×, 16,096 at 5×, 23,028 at 10×, 29,959 at 20×), written ONCE by
    /// `register_pool` from `RegisterPoolArgs.wall_multiple` (`floor_math::wall_ticks_centi`). Inert on a
    /// staircase pool.
    pub wall_ticks: u16,
    pub platform_bps: u16,
    pub min_bid: u64,
    pub min_sellable: u64,
    /// Team steps. `owner_owed`: the quote the step OWNER's payee is owed (its share of what owner steps SOLD for,
    /// booked after FLOOR FIRST, never backing, never bids) — a QUOTE LIABILITY in `FloorState::quote_liabilities`;
    /// the ONE decrement is `floor_hook::sweep_stair_payee`. `owner_paid`: the lifetime total paid out of it; only
    /// rises. `register_pool` writes both 0.
    pub owner_owed: u64,
    pub owner_paid: u64,
    pub vault_eth: u64,
    pub bid_eth_deployed: u64,
    pub unpaid_skim: u64,
    pub cumulative_volume_eth: u64,
    pub inventory_tokens: u64,
    pub inventory_cost_eth: u64,
    /// The team's HELD tokens: owner tokens whose step left the staircase unsold, set aside in claims1 for the payee —
    /// a BASE LIABILITY, counted against the floor (`floor_math::FloorState::owner_held`; the books' twin
    /// `StairOwners::held`). The field keeps its historical name.
    pub ask_tokens_deployed: u64,
    pub ladder_tokens_remaining: u64,
    pub bid_lower: i32,
    pub ask_upper: i32,
    pub ask_width: i32,
    pub seed_lower: i32,
    pub seed_upper: i32,
    pub ladder_lower: i32,
    pub ladder_upper: i32,
    /// The wall's split tick: the band's dear edge = the tail's cheap edge; meaningful ONLY while
    /// `wall_tail_liq != 0` (⛔ a real split can be tick 0 — never test this field for "is there a
    /// tail?"), 0 otherwise. Inert on a staircase pool.
    pub wall_split_tick: i32,
    pub bid_liquidity: U128,
    pub ask_liquidity: U128,
    pub seed_liq: U128,
    pub ladder_liq: U128,
    pub creator_fees_owed: u64,
    pub platform_owed: u64,
    pub holder_owed: u64,
    /// The wall tail's liquidity (salt `SALT_WALL_TAIL`) on `[ladder_lower, wall_split_tick]`; 0 while
    /// the wall is ONE position. Inert on a staircase pool.
    pub wall_tail_liq: U128,
    /// The wall's tokens not listed at the last placement — idle in the pool's claims, outside
    /// circulating supply. Inert on a staircase pool.
    pub wall_held_tokens: u64,
    pub seed_fees_swept_eth: u64,
    pub seed_fees_swept_tokens: u64,
    pub ladder_proceeds_eth: u64,
    pub platform_paid_eth: u64,
    pub platform_swept: u64,
    pub creator_fees_paid: u64,
    pub holder_paid_eth: u64,
    pub last_ratchet_slot: u64,
    // ---- carved out of `reserved` at 528, ordered by ALIGNMENT (i32, u16, u16, [u8;32]) ---------
    /// The placed bid's width in ticks (the EVM's `bidWidth`); 0 on a pool with no bid yet.
    pub bid_width: i32,
    /// `DEFAULT_SKIM_BUY_AT_FLOOR_BPS` (0) at `register_pool`; no setter.
    pub skim_buy_at_floor_bps: u16,
    /// `constants::skim_sell_at_floor_bps(skim_sell_far_bps)` — 1.5x far, at most 1500 — at
    /// `register_pool`; no setter.
    pub skim_sell_at_floor_bps: u16,
    /// Unused padding (32 bytes).
    pub pad6: [u8; 32],
    /// The ratchet bounty ceiling, per pool, in lamports (the EVM's `MAX_BOUNTY`). No setter.
    pub max_bounty: u64,
    /// The ratchet bounty rate, per pool (the EVM's `BOUNTY_BPS`). No setter.
    pub bounty_bps: u16,
    /// The wall shape (inert: every pool is a staircase), written ONCE by `register_pool` from its
    /// trailing `WallShapeArg`, no setter: the band's raw width `floor_math::wall_ticks(W)` (0 = the
    /// open shape), the tail's share of the listed tokens (bps), and the list share (bps; 0 = list
    /// everything). `floor_launch::create_launch` reads the three back (`holders::FLOOR_POOL_ACCT_WALL_*`).
    pub wall_band_ticks: u16,
    pub wall_tail_bps: u16,
    pub wall_list_bps: u16,
    // ---- the CONDITIONAL creator-fee referral split ------------------------------------------
    // Ordered by ALIGNMENT so the carve starts on the 8-aligned 584 and is padding-free.
    // ⛔ **FALSE IS THE SAFE DEFAULT, AND THE ZEROS GIVE IT FOR FREE.** A zeroed pool reads "never
    // split". The opposite polarity would take 10 % of a creator's fee for a referrer who does not
    // exist. NEVER INVERT IT.
    // ⛔ **NONE OF THESE HAS A SETTER.** `register_pool` writes them once. Afterwards only the
    // meters move, and only through `floor_math::take_fees` (up) and
    // `floor_hook::claim_referral_fees` (down). A deployer referred after the fact does not
    // retroactively fund a referrer out of a launch that already exists (first-touch attribution).
    /// Lamports split off the creator fee and NOT YET MOVED to the referrer.
    /// ⚠ **The EVM has NO TWIN for this field, and that is the one real divergence of the mirror.**
    /// The EVM PAYS the referral out inside the swap; a Solana swap callback gets only its
    /// `FLOOR_CALLBACK_ACCOUNTS` accounts and no token account, so nothing can move a lamport there.
    /// It ACCRUES instead, exactly as the platform cut does, and `claim_referral_fees` is the only
    /// exit.
    /// ⛔ **IT IS A QUOTE LIABILITY AND IT IS PART OF THE CLAIMS-COVER INVARIANT IN EVERY
    /// SPELLING** (the program's fee and ratchet checks, and the quoter's epilogue). Leave it out of
    /// any one of them and the pool reports more free claims than it holds. The sum is UNCHANGED by
    /// the split itself — every lamport this gains, `creator_fees_owed` does not.
    pub referral_owed: u64,
    /// Lifetime lamports split off to the referrer (the EVM's `referralFeesPaid`). ⚠ Named `_paid`
    /// for EVM parity, where the wei really does leave inside the swap. Here it is the cumulative
    /// METER: `referral_fees_paid` is `referral_owed` plus everything `claim_referral_fees` has
    /// already moved out, and it only ever rises.
    pub referral_fees_paid: u64,
    /// Trader referrals: the trader slices this pool BOOKED out of `team_owed` and not yet swept to the shared vault,
    /// in the pool's quote — a QUOTE LIABILITY (`quote_liabilities`). The bytes of the retired `referral_distributor`
    /// @600.
    pub trader_ref_owed: u64,
    /// Lifetime trader slices BOOKED on this pool (owed included; it only ever rises).
    pub trader_ref_booked: u64,
    /// The rest of the retired `referral_distributor`, padding; reserved for a per-pool rate copy — never written.
    pub pad7: [u8; 16],
    /// The share of a GROSS creator fee that goes to the REFERRER, in bips
    /// (`REFERRAL_CREATOR_SHARE_BPS`, 1000 = 10 %, fixed per launch): `register_pool` writes 1000 when `referrer`
    /// is non-zero and 0 otherwise, so both halves of "not referred" agree. ⛔ A STORED COPY, never a live read.
    pub referral_share_bps: u16,
    /// The staircase. Non-zero `stair_steps` marks a STAIRCASE pool, written ONCE by `register_pool` from its trailing
    /// `StairArg`; `stair_ready` is 1 once graduation's `seed_inventory` placed the staircase INSIDE the graduating
    /// buy (no swap gate reads it); `stair_growth_bps` weights the steps. No setter.
    pub stair_steps: u8,
    pub stair_ready: u8,
    pub stair_growth_bps: u16,
    /// WHICH staircase parameter set the pool runs — `STAIR_KIND_PRESET` 0 (the presets: 69 / 1, the POL target 7 %,
    /// the cushion from 0.9 × the price) or `STAIR_KIND_GENESIS` 1 (`$FLOOR`: 45 / 1, 20 %, 0.7 ×).
    /// Written ONCE, with `stair_steps`, by `floor_hook::set_genesis_stair` on `$FLOOR`'s designated pool; graduation
    /// copies the set's numbers into the pool's `StairState`, which is what the ratchet reads. No other writer.
    pub stair_kind: u8,
    /// Team steps: `STAIR_OWNER_TEAM` 1 (`$FLOOR`'s team steps, written by `set_genesis_stair`) or
    /// `STAIR_OWNER_TEAM_BUY` 2 (a launch's team steps, written by `register_pool`'s trailing `StairOwnerArg`); 0 = no
    /// owner-held steps. Non-zero, the pool's `StairState` carries the `StairOwners` tail and `owner_owed` /
    /// `owner_paid` are live. Written ONCE.
    pub stair_owner: u8,
    // ---- holder rewards + the on-chain referrer ----------------------------------------------
    // Ordered by alignment ([u8;32], u8, u8, pad) so the carve is padding-free and the tail stays
    // 8-aligned at 680. A zeroed pool reads ZERO in all four: unreferred, unhooked.
    /// The referrer's WALLET (a system account), or the zero key when the launch was not referred.
    /// Written into the launch at create. A referrer exists only when the deployer is the fee
    /// recipient, enforced ON CHAIN (`floor_launch` checks the signer; `register_pool` re-checks
    /// that the referrer is not the creator).
    /// ⛔ **The zero key IS the gate**: `floor_math::take_fees` splits nothing while it is zero.
    /// ⛔ NO SETTER: `register_pool` writes it once. Only `referral_owed` moves afterwards — up in
    /// `take_fees`, down in `claim_referral_fees`, which pays THIS wallet's quote account and nobody
    /// else's.
    /// ⚠ The referrer's payout wallet is PUBLIC on chain in every launch it referred.
    /// ⛔ Read it through [`FloorPool::referral_destination`], never raw: 32 zero bytes are a valid
    /// pubkey (base58 `111…1`), so a raw read of an unreferred pool looks configured.
    pub referrer: [u8; 32],
    /// Unused padding (one byte).
    pub pad8: u8,
    /// 1 iff the base mint carries FLOOR's transfer-hook slot (iff the launch has a holder rate on
    /// either side). Written once by `register_pool`, after it checked the mint.
    pub holders_hooked: u8,
    pub pad4: [u8; 6],
    // ---- three-tier referrals: each tier gets 10 % of the tier below, from that tier's cut ---------
    // Ordered by alignment ([u8;32], [u8;32], u64, u64) so the carve is padding-free and the tail stays
    // 8-aligned at 760. A zeroed pool reads ZERO in all four: "single-tier, nothing paid to tiers 2/3".
    // `floor_holders`' raw offsets (440/520/648/680/681) sit below the carve.
    // ⛔ NO SETTER: `register_pool` writes the two wallets once (from the trailing `ReferralTiersArg`);
    // the two counters move only UP, only in `claim_referral_fees`.
    // ⛔ There is NO tier-1 paid counter: tier 1's paid is DERIVED (`floor_math::referral`).
    // ⚠ Tier-2/3 payout wallets are PUBLIC on chain in launches they did not directly refer.
    /// Tier 2 of the referral chain (the direct referrer's own referrer), or the zero key. Set ⇒
    /// `referrer` set; never the creator; distinct from `referrer` and `referrer_t3`.
    pub referrer_t2: [u8; 32],
    /// Tier 3 (tier 2's referrer), or the zero key. Set ⇒ `referrer_t2` set.
    pub referrer_t3: [u8; 32],
    /// Lifetime quote paid OUT to tier 2 by `claim_referral_fees`. Only ever rises.
    pub referral_paid_t2: u64,
    /// Lifetime quote paid OUT to tier 3 by `claim_referral_fees`. Only ever rises.
    pub referral_paid_t3: u64,
    // ---- the team fee, the last 24 bytes, ordered u64, u64, u16+pad so the carve is padding-free and
    // every u64 stays 8-aligned. ⛔ No `reserved` is left on this account: the next `FloorPool` field
    // needs a realloc or a new version.
    /// Platform fee owed to the team, not yet swept (quote base units). ⛔ A QUOTE LIABILITY (I-3).
    /// Up: `floor_math::take_fees`, `floor_hook::seed_floor`. Zero: `sweep_platform_cut` only.
    pub team_owed: u64,
    /// Lifetime swept to the team treasury; rises only in `sweep_platform_cut`.
    pub team_fees_paid: u64,
    /// This pool's FLOOR Platform fee rate: the trailing `TeamFeeArg` `register_pool` was handed
    /// (`LaunchConfig::team_pool_bps()` at the launch's CREATION, `<= TEAM_POOL_FEE_BPS_MAX`). Kept for
    /// the pool's life — no per-pool setter, no live re-read.
    pub team_fee_bps: u16,
    pub pad5: [u8; 6],
}

impl FloorPool {
    /// The referrer WALLET this pool's referral share is owed to, or `None` when it funds no
    /// referral: [`FloorPool::referrer`] (@640). The retired `referral_distributor` bytes (@600)
    /// are never read.
    ///
    /// ⛔ The absent sentinel is ALL-ZERO: 32 zero bytes are a valid pubkey (base58 `111…1`), so a
    /// raw read of an unreferred pool looks configured; this is the only honest read.
    pub fn referral_destination(&self) -> Option<&[u8; 32]> {
        if self.referrer == [0u8; 32] { None } else { Some(&self.referrer) }
    }

    /// Does this pool split its creator fee at all? The gate `take_fees` applies — the referrer
    /// WALLET, not the rate: `referral_share_bps` is 0 on an unreferred pool too, and a pool with a
    /// rate and no referrer is a defect in `register_pool`, not a second kind of "referred".
    pub fn funds_referrals(&self) -> bool {
        self.referral_destination().is_some()
    }

    /// Does the base mint carry FLOOR's transfer-hook slot (and so a `HolderBook`)?
    pub fn is_hooked(&self) -> bool {
        self.holders_hooked != 0
    }
}
layout!(FloorPool, name = "FloorPool", size = 784, disc = disc::FLOOR_POOL, {
    version: u8 = 0, bump: u8 = 1, pool_bound: u8 = 2, quote_is_token1: u8 = 3, ladder_seeded: u8 = 4,
    needs_ratchet: u8 = 5, mode: u8 = 6, quote_suspended: u8 = 7, tick_spacing: i32 = 8, floor_tick: i32 = 12,
    amm_pool: [u8; 32] = 16, pool_initializer: [u8; 32] = 48, base_mint: [u8; 32] = 80,
    quote_mint: [u8; 32] = 112, pad9: [u8; 32] = 144, pad10: [u8; 32] = 176,
    skim_buy_far_bps: u16 = 208, skim_sell_far_bps: u16 = 210, creator_buy_bps: u16 = 212,
    creator_sell_bps: u16 = 214, holder_buy_bps: u16 = 216, holder_sell_bps: u16 = 218,
    wall_ticks: u16 = 220, platform_bps: u16 = 222, min_bid: u64 = 224, min_sellable: u64 = 232,
    owner_owed: u64 = 240, owner_paid: u64 = 248, vault_eth: u64 = 256, bid_eth_deployed: u64 = 264,
    unpaid_skim: u64 = 272, cumulative_volume_eth: u64 = 280, inventory_tokens: u64 = 288,
    inventory_cost_eth: u64 = 296, ask_tokens_deployed: u64 = 304, ladder_tokens_remaining: u64 = 312,
    bid_lower: i32 = 320, ask_upper: i32 = 324, ask_width: i32 = 328, seed_lower: i32 = 332,
    seed_upper: i32 = 336, ladder_lower: i32 = 340, ladder_upper: i32 = 344, wall_split_tick: i32 = 348,
    bid_liquidity: U128 = 352, ask_liquidity: U128 = 368, seed_liq: U128 = 384, ladder_liq: U128 = 400,
    creator_fees_owed: u64 = 416, platform_owed: u64 = 424, holder_owed: u64 = 432, wall_tail_liq: U128 = 440,
    wall_held_tokens: u64 = 456, seed_fees_swept_eth: u64 = 464,
    seed_fees_swept_tokens: u64 = 472, ladder_proceeds_eth: u64 = 480, platform_paid_eth: u64 = 488,
    platform_swept: u64 = 496, creator_fees_paid: u64 = 504, holder_paid_eth: u64 = 512,
    last_ratchet_slot: u64 = 520, bid_width: i32 = 528, skim_buy_at_floor_bps: u16 = 532,
    skim_sell_at_floor_bps: u16 = 534, pad6: [u8; 32] = 536, max_bounty: u64 = 568,
    bounty_bps: u16 = 576, wall_band_ticks: u16 = 578, wall_tail_bps: u16 = 580, wall_list_bps: u16 = 582,
    referral_owed: u64 = 584,
    referral_fees_paid: u64 = 592, trader_ref_owed: u64 = 600, trader_ref_booked: u64 = 608, pad7: [u8; 16] = 616,
    referral_share_bps: u16 = 632, stair_steps: u8 = 634, stair_ready: u8 = 635, stair_growth_bps: u16 = 636,
    stair_kind: u8 = 638, stair_owner: u8 = 639, referrer: [u8; 32] = 640,
    pad8: u8 = 672, holders_hooked: u8 = 673, pad4: [u8; 6] = 674,
    referrer_t2: [u8; 32] = 680, referrer_t3: [u8; 32] = 712, referral_paid_t2: u64 = 744,
    referral_paid_t3: u64 = 752, team_owed: u64 = 760, team_fees_paid: u64 = 768, team_fee_bps: u16 = 776,
    pad5: [u8; 6] = 778,
});

// =========================================================================================
// floor_launch
// =========================================================================================

/// `["launch_config"]`. Struct 312 B, account 320 B.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LaunchConfig {
    pub version: u8,
    pub bump: u8,
    /// 0 main, 1 dev, 2 local.
    pub cluster: u8,
    pub pad0: [u8; 5],
    /// ⛔ Hands over in TWO steps (`set_governance` → `accept_governance`).
    pub governance: [u8; 32],
    /// Ed25519 attestor for `claim_recipient`. ⛔ ROTATABLE by `governance` (`set_attestor`): it
    /// is the ONLY key the attestation check compares against, so a rotation retires every
    /// outstanding attestation atomically. The attestation message carries no epoch byte — the key
    /// IS the epoch.
    pub attestor: [u8; 32],
    pub hook_program: [u8; 32],
    pub amm_program: [u8; 32],
    pub platform_bps: u16,
    pub tick_spacing: u16,
    /// Unused padding.
    pub pad1: [u8; 4],
    pub min_bid_lamports: u64,
    /// Unused padding (16 bytes).
    pub pad3: [u8; 16],
    pub min_sellable: u64,
    pub registry_count: u64,
    /// The ratchet bounty CEILING new launches are created with, in lamports. Range
    /// `0 ..= MAX_BOUNTY_LAMPORTS_CEIL`; `set_launch_economics` tunes it for launches made AFTER
    /// the call and for no live pool.
    pub max_bounty_lamports: u64,
    /// The slot `set_metadata_base` last wrote at. `fill_metadata_uri` refuses until
    /// `slot >= this + METADATA_FILL_DELAY_SLOTS`, which is governance's window to correct a wrong
    /// base before a permissionless, irreversible fill locks the whole pre-host backlog onto it.
    pub metadata_base_set_slot: u64,
    /// The ratchet bounty RATE new launches are created with, in bips of the skim collected since
    /// the last ratchet. Range `0 ..= MAX_BOUNTY_BPS`.
    pub bounty_bps: u16,
    /// How many bytes of `metadata_base` are the host. 0 = no host yet.
    pub metadata_base_len: u8,
    /// The metadata host, ASCII, NORMALISED (no trailing `/`).
    pub metadata_base: [u8; MAX_METADATA_BASE_LEN],
    /// Alignment for the account tail; `metadata_base` ends at an odd offset. ⛔ Explicit, because
    /// `Pod` refuses a struct with implicit padding.
    pub pad2: [u8; 5],
    /// The two-step governance handover's proposal. Zero = no proposal.
    pub pending_governance: [u8; 32],
    // ---- the FLOOR Platform fee's CURRENT SETTING, carved from the front of `reserved` (8 -> 3); the struct is still
    //      312 B.
    /// RAW — ⛔ never read it directly: read [`LaunchConfig::team_sale_bps`]. The sale rate
    /// `set_team_fees` last wrote, meaningful only once [`Self::team_bps_set`] is non-zero.
    pub team_sale_bps_raw: u16,
    /// RAW — ⛔ never read it directly: read [`LaunchConfig::team_pool_bps`]. The pool rate
    /// `set_team_fees` last wrote, meaningful only once [`Self::team_bps_set`] is non-zero.
    pub team_pool_bps_raw: u16,
    /// 0 = never set: the DEFAULTS apply (`TEAM_SALE_FEE_BPS_DEFAULT` 100 / `TEAM_POOL_FEE_BPS_DEFAULT` 25) — which is
    /// what a fresh (zeroed) config reads, with no setter step. Non-zero = the two raw fields above. `set_team_fees`
    /// writes 1 with both numbers, every time.
    pub team_bps_set: u8,
    pub reserved: [u8; 3],
}
layout!(LaunchConfig, name = "LaunchConfig", size = 312, disc = disc::LAUNCH_CONFIG, {
    version: u8 = 0, bump: u8 = 1, cluster: u8 = 2, pad0: [u8; 5] = 3, governance: [u8; 32] = 8,
    attestor: [u8; 32] = 40, hook_program: [u8; 32] = 72, amm_program: [u8; 32] = 104,
    platform_bps: u16 = 136, tick_spacing: u16 = 138, pad1: [u8; 4] = 140,
    min_bid_lamports: u64 = 144, pad3: [u8; 16] = 152, min_sellable: u64 = 168,
    registry_count: u64 = 176, max_bounty_lamports: u64 = 184, metadata_base_set_slot: u64 = 192,
    bounty_bps: u16 = 200, metadata_base_len: u8 = 202,
    metadata_base: [u8; MAX_METADATA_BASE_LEN] = 203, pad2: [u8; 5] = 267,
    pending_governance: [u8; 32] = 272, team_sale_bps_raw: u16 = 304, team_pool_bps_raw: u16 = 306,
    team_bps_set: u8 = 308, reserved: [u8; 3] = 309,
});

/// The ONE reading of `LaunchConfig`'s platform-fee setting, shared by `floor_launch`'s zero-copy twin (which cannot
/// carry this crate's `impl`). `set == 0` is the default; otherwise the raw value, never above `cap` — so no byte
/// pattern, however it got there, can make a launch charge more than the cap.
/// ⛔ The encoding trap: a RAW read of an unset config sees 0 where the rate is the default. Every
///    reader goes through this or its transcription.
pub const fn team_rate_setting(set: u8, raw: u16, default: u16, cap: u16) -> u16 {
    if set == 0 {
        default
    } else if raw > cap {
        cap
    } else {
        raw
    }
}

impl LaunchConfig {
    /// The FLOOR Platform fee a sale created NOW charges on every buy and sell-back
    /// (`create_launch` copies it into `Launch.team_bps`): the default 100 until `set_team_fees` runs.
    pub const fn team_sale_bps(&self) -> u16 {
        team_rate_setting(
            self.team_bps_set,
            self.team_sale_bps_raw,
            crate::constants::TEAM_SALE_FEE_BPS_DEFAULT,
            crate::constants::TEAM_SALE_FEE_BPS_MAX,
        )
    }
    /// The FLOOR Platform fee a pool registered NOW charges on every swap after graduation
    /// (`create_launch` hands it to `register_pool`): the default 25 until `set_team_fees` runs.
    pub const fn team_pool_bps(&self) -> u16 {
        team_rate_setting(
            self.team_bps_set,
            self.team_pool_bps_raw,
            crate::constants::TEAM_POOL_FEE_BPS_DEFAULT,
            crate::constants::TEAM_POOL_FEE_BPS_MAX,
        )
    }
}

/// `["entry", index u64 LE]`. Struct 120 B, account 128 B.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct RegistryEntry {
    pub index: u64,
    pub launch: [u8; 32],
    pub mint: [u8; 32],
    pub amm_pool: [u8; 32],
    pub mode: u8,
    pub pad0: [u8; 7],
    pub created_slot: u64,
}
layout!(RegistryEntry, name = "RegistryEntry", size = 120, disc = disc::REGISTRY_ENTRY, {
    index: u64 = 0, launch: [u8; 32] = 8, mint: [u8; 32] = 40, amm_pool: [u8; 32] = 72, mode: u8 = 104,
    pad0: [u8; 7] = 105, created_slot: u64 = 112,
});

/// `["launch", mint]`. Struct 512 B, account 520 B.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Launch {
    pub version: u8,
    pub bump: u8,
    pub sale_vault_bump: u8,
    pub lock_bump: u8,
    /// 0 Bonding, 1 Graduated.
    pub state: u8,
    pub mode: u8,
    pub mint_authority_bump: u8,
    /// Unused, written zero.
    pub pad0: u8,
    pub mint: [u8; 32],
    /// The deployer, the `creatorOf` analogue.
    pub creator: [u8; 32],
    /// The team wallet part's cliffs paid PER TEAM SLOT — slot i owns bits 4i..4i+3, bit k = cliff k paid to it
    /// (`claim_team_vested(slot)`).
    pub team_cliffs_paid_by_slot: u32,
    /// Unused, written zero; the creator slots are the pool's `PayeeSlots` tail.
    pub pad3: [u8; 28],
    /// Unused, written zero.
    pub pad4: [u8; 32],
    /// Predicted at create.
    pub amm_pool: [u8; 32],
    /// Predicted at create.
    pub floor_pool: [u8; 32],
    /// Launch's FLOOR ATA.
    pub token_vault: [u8; 32],
    /// Launch's wSOL ATA.
    pub wsol_vault: [u8; 32],
    /// QUOTE BASE UNITS per token base unit × 2^64; the unit is the quote mint's own (a lamport for wSOL).
    pub price_x64: U128,
    /// The quote asset's declared price in whole CENTS, fixed at create and never read again by any instruction.
    pub quote_usd_cents: u64,
    pub mcap_usd: u64,
    pub created_slot: u64,
    pub nonce: u64,
    pub sale_tokens: u64,
    pub pol_tokens: u64,
    pub ladder_tokens: u64,
    pub sold: u64,
    pub raised: u64,
    pub buyers: u64,
    pub open_tick: i32,
    /// The bump of `["trader_ref_sale", launch]`, the launch's sale referral token account `create_launch` opens —
    /// stored so a referred sale trade pins it without a PDA search.
    pub trader_ref_sale_bump: u8,
    pub pad1: [u8; 3],
    pub skim_buy_far: u16,
    pub skim_sell_far: u16,
    pub creator_buy: u16,
    pub creator_sell: u16,
    pub holder_buy: u16,
    pub holder_sell: u16,
    /// This sale's FLOOR Platform fee rate: `LaunchConfig::team_sale_bps()` copied at `create_launch`
    /// (`<= TEAM_SALE_FEE_BPS_MAX`). Kept for the sale's life — a later `set_team_fees` never reaches a running sale.
    pub team_bps: u16,
    pub pad2: u16,
    pub registry_index: u64,
    pub graduated_slot: u64,
    /// The mint this sale is QUOTED in — what `buy` takes, what `sell_back` pays, what the pool's currency-0 is.
    /// Read it through [`Launch::quote_mint`].
    pub quote_mint: [u8; 32],
    /// Fees this sale took and has not handed to its pool. Same escrow as the raise, never part of `raised`.
    /// Up: buy / sell_back. Zero: graduation step 1 (-> `seed_floor`'s `team`).
    /// ⛔ While bonding: escrow - rent == raised + team_owed (+ gifts).
    pub team_owed: u64,
    // ---- the team buy, carved from the FRONT of `reserved` (size still 512, nothing before 440 moved; zero = no team
    // buy). ----
    /// Tokens the deployer bought at creation onto TEAM STEPS (≥ 80 % of the team buy): they stay in the launch's token
    /// vault through the sale and graduation deposits them into the staircase, an equal share of every step.
    pub team_step_tokens: u64,
    /// Tokens the deployer bought at creation for the WALLET (≤ 20 %, ≤ 1 % of supply): held in the launch's token vault
    /// and released 25 % at each of `team_vest_days` after graduation (`claim_team_vested`).
    pub team_wallet_tokens: u64,
    /// Unix seconds of the graduation (the team wallet's cliffs and the team steps' lock count from here); 0 before.
    pub team_anchor: i64,
    /// The team steps' lock, days (≥ `TEAM_LOCK_MIN_DAYS`), counted from `team_anchor`.
    pub team_lock_days: u16,
    /// The wallet part's four cliffs, days after `team_anchor` (25 % each, ascending).
    pub team_vest_days: [u16; 4],
    /// Unused, written zero; the team slots are the pool's `PayeeSlots` tail.
    pub pad5: u8,
    /// Unused, written zero; the cliffs are per slot (`team_cliffs_paid_by_slot`).
    pub pad6: u8,
    /// Unused, written zero.
    pub pad7: [u8; 32],
    /// The staircase rent reserve: the lamports of the staircase budget `create_launch` prepaid into
    /// `["grad_rent", launch]`, still to be moved to the pool's `["stair_rent", amm_pool]` reserve. `fund_stair_rent`
    /// moves it after graduation and writes 0; until then `close_graduation_rent` leaves it in the reserve once the
    /// launch has graduated. 0 = nothing owed (moved, or a launch without a budget). Carved out of `reserved` (the last
    /// 4 bytes): the account stays 512 B.
    pub stair_rent_budget: u32,
}
layout!(Launch, name = "Launch", size = 512, disc = disc::LAUNCH, {
    version: u8 = 0, bump: u8 = 1, sale_vault_bump: u8 = 2, lock_bump: u8 = 3, state: u8 = 4, mode: u8 = 5,
    mint_authority_bump: u8 = 6, pad0: u8 = 7, mint: [u8; 32] = 8, creator: [u8; 32] = 40,
    team_cliffs_paid_by_slot: u32 = 72, pad3: [u8; 28] = 76, pad4: [u8; 32] = 104, amm_pool: [u8; 32] = 136,
    floor_pool: [u8; 32] = 168, token_vault: [u8; 32] = 200, wsol_vault: [u8; 32] = 232,
    price_x64: U128 = 264, quote_usd_cents: u64 = 280, mcap_usd: u64 = 288, created_slot: u64 = 296, nonce: u64 = 304,
    sale_tokens: u64 = 312, pol_tokens: u64 = 320, ladder_tokens: u64 = 328, sold: u64 = 336,
    raised: u64 = 344, buyers: u64 = 352, open_tick: i32 = 360, trader_ref_sale_bump: u8 = 364, pad1: [u8; 3] = 365,
    skim_buy_far: u16 = 368, skim_sell_far: u16 = 370, creator_buy: u16 = 372, creator_sell: u16 = 374,
    holder_buy: u16 = 376, holder_sell: u16 = 378, team_bps: u16 = 380, pad2: u16 = 382, registry_index: u64 = 384,
    graduated_slot: u64 = 392, quote_mint: [u8; 32] = 400, team_owed: u64 = 432, team_step_tokens: u64 = 440,
    team_wallet_tokens: u64 = 448, team_anchor: i64 = 456, team_lock_days: u16 = 464, team_vest_days: [u16; 4] = 466,
    pad5: u8 = 474, pad6: u8 = 475, pad7: [u8; 32] = 476, stair_rent_budget: u32 = 508,
});

impl Launch {
    /// The mint this launch is quoted in (`Launch.quote_mint`, written by `create_launch`).
    pub fn quote_mint(&self) -> [u8; 32] {
        self.quote_mint
    }
}

/// `["buyer", launch, buyer]`. Struct 40 B, account 48 B.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct BuyerPosition {
    pub bump: u8,
    pub pad0: [u8; 7],
    pub allocation: u64,
    pub contributed: u64,
    pub reserved: [u8; 16],
}
layout!(BuyerPosition, name = "BuyerPosition", size = 40, disc = disc::BUYER_POSITION, {
    bump: u8 = 0, pad0: [u8; 7] = 1, allocation: u64 = 8, contributed: u64 = 16, reserved: [u8; 16] = 24,
});

/// `["genesis_sale"]` — `$FLOOR`'s timed pro-rata raise (the twin of the EVM's `FloorProtocolSetup`
/// storage, `FLOOR_TIMED_V1`). ONE per program, ever. Struct 432 B.
///
/// Field for field the EVM's public state: `openTime` / `duration` / `floorShareBps` / `graduated`
/// / `raised` / `contributorCount` / `settledContributorCount` / `totalClaimed` /
/// `allocationDustPending` / `allocationDustRouted` / `feesApplied` / `openTick` /
/// `openingTickClamped` / `initialFloorEth` / `offeredPolEth` / `returnedPolEth` / `ladderTokens`,
/// plus what Solana needs said aloud: the launch and mint it converted, and the two step flags the
/// graduation split adds (`sale_closed` after `end_genesis_sale`, `locked` after `lock_genesis`).
/// ⛔ `open_time` is a Unix timestamp (`Clock::unix_timestamp`), 0 = unscheduled — the EVM's
/// `openTime == 0`. `sale_end` is `open_time + duration`, never stored.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GenesisSale {
    pub version: u8,
    pub bump: u8,
    pub graduated: u8,
    /// `end_genesis_sale` has run: `open_tick` is fixed and `prepare_graduation` may run.
    pub sale_closed: u8,
    /// `lock_genesis` has run: the five locks exist.
    pub locked: u8,
    pub opening_tick_clamped: u8,
    pub pad0: [u8; 2],
    pub floor_share_bps: u16,
    pub pad1: [u8; 2],
    pub open_tick: i32,
    /// The EVM's `owner()`: the key that may `set_genesis_schedule` / `set_genesis_floor_share`
    /// (the signer of `init_genesis`, which is `LaunchConfig.governance` then).
    pub authority: [u8; 32],
    pub beneficiary: [u8; 32],
    pub launch: [u8; 32],
    pub mint: [u8; 32],
    pub open_time: i64,
    pub duration: u64,
    pub raised: u64,
    pub contributor_count: u64,
    pub settled_contributor_count: u64,
    pub total_claimed: u64,
    pub allocation_dust_pending: u64,
    pub allocation_dust_routed: u64,
    pub fees_applied: u64,
    pub initial_floor: u64,
    pub offered_pol: u64,
    pub returned_pol: u64,
    pub ladder_tokens: u64,
    /// The two-step authority handover's pending key (the EVM's `Ownable2Step.pendingOwner`), carved from `reserved` @248.
    pub pending_authority: [u8; 32],
    /// The staircase's TOP as a MARKET CAP IN QUOTE BASE UNITS (lamports) — a program holds no USD, so the genesis
    /// authority sets it before the open (`set_genesis_stair_top`; frozen once opened, and a schedule cannot be set
    /// while it is 0). `end_genesis_sale` fixes the step count from it (`floor_math::genesis::genesis_stair_steps`).
    pub stair_top_mcap: u64,
    /// The step count `end_genesis_sale` fixed (1..=16); 0 before.
    pub stair_steps: u8,
    /// The team steps' PAYEE, fixed at `init_genesis`: the vesting beneficiary by default, or a social handle:
    /// `TEAM_PAYEE_DEPLOYER` 0 = `beneficiary`, `TEAM_PAYEE_PENDING` 1 = `payee` is a handle id (claimed through
    /// `claim_genesis_payee`, the fee recipient's path), `TEAM_PAYEE_RESOLVED` 2 = `payee` is the claimed wallet.
    pub payee_state: u8,
    /// The pending payee's platform, 1..=4 (0 with a wallet payee); written into `$FLOOR`'s team slot 0 at graduation.
    pub payee_platform: u8,
    pub pad2: [u8; 5],
    /// The lamports `refund_genesis` has paid back so far (carved from the front of `reserved` @296). `raised` stays
    /// the record of what was contributed.
    pub refunded: u64,
    pub reserved: [u8; 8],
    /// The team steps' payee: the pending handle's id or the resolved wallet (`payee_state`); zero with state 0.
    pub payee: [u8; 32],
    /// The phase-1 whitelist's 32-byte Merkle root, set by `set_genesis_whitelist` before the open (frozen once opened;
    /// zero = unset, and `set_genesis_schedule` refuses to open without one). `contribute` checks each wallet's proof
    /// against it (`floor_launch::whitelist`: leaf sha256(0x00‖wallet), parent sha256(0x01‖min‖max)).
    pub whitelist_root: [u8; 32],
    /// The two-phase raise: `whitelist_root` above is PHASE 1's; this is PHASE 2's — its own list, so a phase-1
    /// wallet contributes in phase 2 only if it is on this one too. Set by `set_genesis_whitelist(root, phase 2)`.
    pub ff_root: [u8; 32],
    /// Phase 1's lamports (≤ `GENESIS_CORE_CAP_LAMPORTS`, 80 SOL: a contribution past it is trimmed).
    pub raised_core: u64,
    /// Phase 2's lamports (≤ `GENESIS_FF_CAP_LAMPORTS`, 90 SOL: trimmed like Core's), at the same fixed price.
    pub raised_ff: u64,
    /// The tokens the raise SOLD (`floor_math::genesis::genesis_sold`, Core + F&F), fixed by `end_genesis_sale`: what
    /// graduation holds back for the claims, and the base of the allocation dust. The rest of the 42.5M is UNSOLD and joins the
    /// protocol's steps.
    pub sold_tokens: u64,
}
layout!(GenesisSale, name = "GenesisSale", size = 432, disc = disc::GENESIS_SALE, {
    version: u8 = 0, bump: u8 = 1, graduated: u8 = 2, sale_closed: u8 = 3, locked: u8 = 4,
    opening_tick_clamped: u8 = 5, pad0: [u8; 2] = 6, floor_share_bps: u16 = 8, pad1: [u8; 2] = 10,
    open_tick: i32 = 12, authority: [u8; 32] = 16, beneficiary: [u8; 32] = 48, launch: [u8; 32] = 80,
    mint: [u8; 32] = 112, open_time: i64 = 144, duration: u64 = 152, raised: u64 = 160,
    contributor_count: u64 = 168, settled_contributor_count: u64 = 176, total_claimed: u64 = 184,
    allocation_dust_pending: u64 = 192, allocation_dust_routed: u64 = 200, fees_applied: u64 = 208,
    initial_floor: u64 = 216, offered_pol: u64 = 224, returned_pol: u64 = 232, ladder_tokens: u64 = 240,
    pending_authority: [u8; 32] = 248, stair_top_mcap: u64 = 280, stair_steps: u8 = 288, payee_state: u8 = 289,
    payee_platform: u8 = 290, pad2: [u8; 5] = 291, refunded: u64 = 296, reserved: [u8; 8] = 304, payee: [u8; 32] = 312,
    whitelist_root: [u8; 32] = 344, ff_root: [u8; 32] = 376, raised_core: u64 = 408, raised_ff: u64 = 416,
    sold_tokens: u64 = 424,
});

/// `["genesis_contrib", wallet]` — one contributor (the EVM's `contributed[user]`, `claimed[user]`,
/// `claimSettled[user]`). Struct 72 B. Created by the wallet's first `contribute`, which it pays
/// the rent of; never closed, so a settled claim stays a readable fact.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GenesisContribution {
    pub bump: u8,
    pub settled: u8,
    pub pad0: [u8; 6],
    pub wallet: [u8; 32],
    /// The wallet's lamports over BOTH phases (its Core part is `contributed − contributed_ff`).
    pub contributed: u64,
    pub claimed: u64,
    /// The wallet's phase-2 lamports (carved from `reserved` @56).
    pub contributed_ff: u64,
    /// The lamports `refund_genesis` paid this wallet — all of `contributed`, once; non-zero = refunded (it can never
    /// claim). Carved from `reserved` @64.
    pub refunded: u64,
}
layout!(GenesisContribution, name = "GenesisContribution", size = 72, disc = disc::GENESIS_CONTRIBUTION, {
    bump: u8 = 0, settled: u8 = 1, pad0: [u8; 6] = 2, wallet: [u8; 32] = 8, contributed: u64 = 40,
    claimed: u64 = 48, contributed_ff: u64 = 56, refunded: u64 = 64,
});

/// `["quote_walk", payer, quote_mint]` — a walk up a FLOOR-token quote's chain, advanced across transactions by
/// `walk_quote_chain` and consumed (and closed, the rent to the payer) by `create_launch`, so a chain of any length can
/// be priced. Struct 128 B, account 136.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct QuoteWalk {
    /// Who funds it, signs every step and alone may consume or close it.
    pub payer: [u8; 32],
    /// The launch's IMMEDIATE quote (hop 0's base).
    pub quote_mint: [u8; 32],
    /// The quote the walk has reached: the next hop's base; the ROOT once `done`.
    pub cur: [u8; 32],
    /// Σ of the hops' ticks so far (`floor_math::chain_hop_tick` each).
    pub ticks: i64,
    /// The unix time of the FIRST step (`QUOTE_WALK_MAX_AGE_SECS` counts from it).
    pub started_at: i64,
    /// The slot of the first step (a record; the bound is the unix time).
    pub started_slot: u64,
    pub hops: u16,
    pub bump: u8,
    /// 1 once the root was proven (`cur` is the root, `root_decimals` its decimals).
    pub done: u8,
    pub root_decimals: u8,
    pub pad0: [u8; 3],
}
layout!(QuoteWalk, name = "QuoteWalk", size = 128, disc = disc::QUOTE_WALK, {
    payer: [u8; 32] = 0, quote_mint: [u8; 32] = 32, cur: [u8; 32] = 64, ticks: i64 = 96, started_at: i64 = 104,
    started_slot: u64 = 112, hops: u16 = 120, bump: u8 = 122, done: u8 = 123, root_decimals: u8 = 124, pad0: [u8; 3] = 125,
});

/// One preset: its numbers in chain units. Written once by
/// `add_preset` (or `init_preset_registry` for the seeded 1–3), never edited; only `enabled` moves
/// (`set_preset_enabled`). Struct 48 B, a row of [`PresetRegistry::rows`]. ⛔ The SDK decodes by these offsets.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
pub struct PresetRowZc {
    /// Whole USD, > 0.
    pub graduation_mcap_usd: u64,
    /// bps of the far SELL skim a sell INTO the bid pays (10,000..=50,000).
    pub skim_sell_at_floor_multiple_bps: u32,
    /// The creator fee, both sides.
    pub creator_bps: u16,
    pub skim_buy_far_bps: u16,
    pub skim_sell_far_bps: u16,
    /// Holder rewards, both sides (a non-zero rate makes the launch HOOKED).
    pub holder_bps: u16,
    /// The supply split: the sale's and the classic POL's share (`split_tokens`' rule).
    pub sale_bps: u16,
    pub pol_bps: u16,
    pub stair_growth_bps: u16,
    /// The team buy's ceiling, bps of supply (≤ `BLUECHIP_TEAM_BUY_MAX_BPS`).
    pub team_buy_max_bps: u16,
    /// The team buy's four wallet cliffs, days after graduation, strictly rising.
    pub team_vest_days: [u16; 4],
    pub stair_steps: u8,
    /// 0 / 1.
    pub enabled: u8,
    pub pad: [u8; 10],
}
const _: () = {
    assert!(core::mem::size_of::<PresetRowZc>() == crate::constants::PRESET_ROW_LEN);
    assert!(core::mem::offset_of!(PresetRowZc, graduation_mcap_usd) == 0);
    assert!(core::mem::offset_of!(PresetRowZc, skim_sell_at_floor_multiple_bps) == 8);
    assert!(core::mem::offset_of!(PresetRowZc, creator_bps) == 12);
    assert!(core::mem::offset_of!(PresetRowZc, skim_buy_far_bps) == 14);
    assert!(core::mem::offset_of!(PresetRowZc, skim_sell_far_bps) == 16);
    assert!(core::mem::offset_of!(PresetRowZc, holder_bps) == 18);
    assert!(core::mem::offset_of!(PresetRowZc, sale_bps) == 20);
    assert!(core::mem::offset_of!(PresetRowZc, pol_bps) == 22);
    assert!(core::mem::offset_of!(PresetRowZc, stair_growth_bps) == 24);
    assert!(core::mem::offset_of!(PresetRowZc, team_buy_max_bps) == 26);
    assert!(core::mem::offset_of!(PresetRowZc, team_vest_days) == 28);
    assert!(core::mem::offset_of!(PresetRowZc, stair_steps) == 36);
    assert!(core::mem::offset_of!(PresetRowZc, enabled) == 37);
    assert!(core::mem::offset_of!(PresetRowZc, pad) == 38);
};

/// `["preset_registry"]` under `floor_launch` — the preset registry. Struct 1,608 B,
/// account 1,616 B. Row `i` is id `i` at struct offset `72 + 48·i`; row 0 stays all zero (Custom is never a row) and ids
/// `1..count` are the registered rows. `authority` (the upgrade timelock's `["authority"]` PDA once handed over) adds and
/// enables rows; `pending_authority` is the two-step handover (zero = none).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct PresetRegistry {
    pub version: u8,
    pub bump: u8,
    /// One past the highest id (4 after seeding).
    pub count: u8,
    pub pad0: [u8; 5],
    pub authority: [u8; 32],
    pub pending_authority: [u8; 32],
    pub rows: [PresetRowZc; 32],
}
layout!(PresetRegistry, name = "PresetRegistry", size = 1_608, disc = disc::PRESET_REGISTRY, {
    version: u8 = 0, bump: u8 = 1, count: u8 = 2, pad0: [u8; 5] = 3, authority: [u8; 32] = 8,
    pending_authority: [u8; 32] = 40, rows: [PresetRowZc; 32] = 72,
});
const _: () = assert!(PresetRegistry::SIZE == crate::constants::PRESET_REGISTRY_LEN);

impl PresetRegistry {
    /// The row registered under `id`, disabled ones included (`enabled` says so); `None` for Custom (0) or an id at or
    /// past `count` — the one spelling of "registered" (`PresetUnknown` when `None`).
    pub fn row(&self, id: u8) -> Option<&PresetRowZc> {
        if id == 0 || id >= self.count || id > crate::constants::MAX_PRESET_ID {
            return None;
        }
        self.rows.get(id as usize)
    }
}

// =========================================================================================
// Trader referrals
// =========================================================================================

/// `["trader_link", trader]` under `floor_launch` — written ONCE by `link_trader` from the attestor's signed message,
/// never changed (no setter, no close). An absent tier is the zero key; the shape (tier 1 set, no gaps, no self, no
/// repeats) is `floor_math::trader_link_tiers_well_formed`. Struct 136 B, account 144.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct TraderLink {
    pub trader: [u8; 32],
    pub tier1: [u8; 32],
    pub tier2: [u8; 32],
    pub tier3: [u8; 32],
    pub bump: u8,
    pub pad0: [u8; 7],
}
layout!(TraderLink, name = "TraderLink", size = 136, disc = disc::TRADER_LINK, {
    trader: [u8; 32] = 0, tier1: [u8; 32] = 32, tier2: [u8; 32] = 64, tier3: [u8; 32] = 96, bump: u8 = 128,
    pad0: [u8; 7] = 129,
});

/// `["trader_ref", trader, quote_mint]` under `floor_hook` — one trader's referral bucket in one quote: what that
/// trader's trades owe each tier, `owed[i]` for tier `i + 1`. The tiers are COPIED from the `TraderLink` when the bucket
/// opens. ⛔ Only that trader's trades (and claims of its tiers) ever write it, so it never serialises two traders.
/// Struct 192 B, account 200.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct TraderBucket {
    pub trader: [u8; 32],
    pub quote_mint: [u8; 32],
    pub tier1: [u8; 32],
    pub tier2: [u8; 32],
    pub tier3: [u8; 32],
    pub owed: [u64; 3],
    pub bump: u8,
    pub pad0: [u8; 7],
}
layout!(TraderBucket, name = "TraderBucket", size = 192, disc = disc::TRADER_BUCKET, {
    trader: [u8; 32] = 0, quote_mint: [u8; 32] = 32, tier1: [u8; 32] = 64, tier2: [u8; 32] = 96, tier3: [u8; 32] = 128,
    owed: [u64; 3] = 160, bump: u8 = 184, pad0: [u8; 7] = 185,
});

#[cfg(test)]
mod tests {
    use super::*;

    /// The preset registry: the SDK decodes by these offsets, so every one is pinned here (the
    /// row's by `offset_of!`, the account's by `layout!`) and row `i` lands at `72 + 48·i`.
    #[test]
    fn the_preset_registry_is_the_designs_bytes() {
        use crate::constants::{MAX_PRESET_ID, PRESET_REGISTRY_LEN, PRESET_ROW_LEN};
        assert_eq!((PRESET_REGISTRY_LEN, PRESET_ROW_LEN, MAX_PRESET_ID), (1_608, 48, 31));
        assert_eq!(PresetRegistry::ACCOUNT_LEN, 1_616);
        let f = |n: &str| PresetRegistry::FIELDS.iter().find(|f| f.name == n).map(|f| (f.offset, f.size)).unwrap();
        assert_eq!(f("version"), (0, 1));
        assert_eq!(f("bump"), (1, 1));
        assert_eq!(f("count"), (2, 1));
        assert_eq!(f("pad0"), (3, 5));
        assert_eq!(f("authority"), (8, 32));
        assert_eq!(f("pending_authority"), (40, 32));
        assert_eq!(f("rows"), (72, 1_536));
        assert_eq!(core::mem::size_of::<PresetRowZc>(), 48);
        // row i at 72 + 48·i, each field at its documented byte (written by value, read back off the raw bytes)
        let mut r = PresetRegistry::zeroed();
        r.count = 5;
        r.rows[4] = PresetRowZc {
            graduation_mcap_usd: 0x0102_0304_0506_0708,
            skim_sell_at_floor_multiple_bps: 50_000,
            creator_bps: 11,
            skim_buy_far_bps: 12,
            skim_sell_far_bps: 13,
            holder_bps: 14,
            sale_bps: 2_000,
            pol_bps: 1_000,
            stair_growth_bps: 10_400,
            team_buy_max_bps: 500,
            team_vest_days: [10, 20, 40, 80],
            stair_steps: 15,
            enabled: 1,
            pad: [0; 10],
        };
        let b = bytemuck::bytes_of(&r);
        let at = 72 + 48 * 4;
        let u16_at = |o: usize| u16::from_le_bytes([b[at + o], b[at + o + 1]]);
        assert_eq!(u64::from_le_bytes(b[at..at + 8].try_into().unwrap()), 0x0102_0304_0506_0708);
        assert_eq!(u32::from_le_bytes(b[at + 8..at + 12].try_into().unwrap()), 50_000);
        assert_eq!((u16_at(12), u16_at(14), u16_at(16), u16_at(18)), (11, 12, 13, 14));
        assert_eq!((u16_at(20), u16_at(22), u16_at(24), u16_at(26)), (2_000, 1_000, 10_400, 500));
        assert_eq!((u16_at(28), u16_at(30), u16_at(32), u16_at(34)), (10, 20, 40, 80));
        assert_eq!((b[at + 36], b[at + 37]), (15, 1));
        assert_eq!(b[2], 5, "count @2");
        // `row`: Custom and every unregistered id are None; disabled rows are served
        assert!(r.row(0).is_none(), "Custom is never a row");
        assert!(r.row(5).is_none() && r.row(31).is_none() && r.row(255).is_none(), "past count");
        assert_eq!(r.row(4).unwrap().stair_steps, 15);
        r.rows[4].enabled = 0;
        assert_eq!(r.row(4).unwrap().enabled, 0, "a disabled row is still a row");
    }

    #[test]
    fn account_lengths_match_the_spec() {
        assert_eq!(HookConfig::ACCOUNT_LEN, 240);
        assert_eq!(FloorPool::ACCOUNT_LEN, 792);
        assert_eq!(LaunchConfig::ACCOUNT_LEN, 320);
        assert_eq!(RegistryEntry::ACCOUNT_LEN, 128);
        assert_eq!(Launch::ACCOUNT_LEN, 520);
        assert_eq!(BuyerPosition::ACCOUNT_LEN, 48);
        // the program-owned platform treasury
        assert_eq!(PlatformTreasury::ACCOUNT_LEN, 112);
        // trader referrals
        assert_eq!(TraderLink::ACCOUNT_LEN, 144);
        assert_eq!(TraderBucket::ACCOUNT_LEN, 200);
        // and the AMM's, through the re-export, so one `use` still finds every account
        assert_eq!(Pool::ACCOUNT_LEN, 640);
        assert_eq!(Position::ACCOUNT_LEN, 208);
    }

/// Rent at `(128 + len) × 5,080` lamports per account (a reference rate; the programs read rent at
/// runtime, never this).
    #[test]
    fn spec_rent_figures_are_consistent_with_the_lengths() {
        let rent = |len: usize| (128 + len as u64) * 5_080;
        assert_eq!(rent(HookConfig::ACCOUNT_LEN), 1_869_440);
        assert_eq!(rent(FloorPool::ACCOUNT_LEN), 4_673_600);
        assert_eq!(rent(LaunchConfig::ACCOUNT_LEN), 2_275_840);
        assert_eq!(rent(RegistryEntry::ACCOUNT_LEN), 1_300_480);
        assert_eq!(rent(Launch::ACCOUNT_LEN), 3_291_840);
        assert_eq!(rent(BuyerPosition::ACCOUNT_LEN), 894_080);
    }

    #[test]
    fn hook_config_header_is_its_first_eight_bytes() {
        let mut hc = HookConfig::zeroed();
        hc.version = 1;
        hc.bump = 254;
        hc.max_rounds = 3;
        hc.callback_accounts = 1;
        hc.flags = crate::constants::FLOOR_HOOK_FLAGS;
        hc.authority_bump = 253;
        let mut data = vec![0u8; 8];
        data.extend_from_slice(bytemuck::bytes_of(&hc));
        let h = HookInterfaceHeader::from_account_data(&data).unwrap();
        assert_eq!(
            h,
            HookInterfaceHeader { version: 1, bump: 254, max_rounds: 3, callback_accounts: 1, flags: 0x60CC, authority_bump: 253, pad0: 0 }
        );
        assert!(HookInterfaceHeader::from_account_data(&data[..15]).is_none());
    }

    // The fields carved out of `reserved` at 528 sit where they should, and the account is still
    // 792 B: a program twin that still carries `reserved: [u8; 256]` has the same size and is caught
    // by the IDL layout test, not by a size assert.
    #[test]
    fn floor_pool_carve_out_at_528() {
        let f = |n: &str| FloorPool::FIELDS.iter().find(|f| f.name == n).unwrap();
        assert_eq!((f("bid_width").offset, f("bid_width").size), (528, 4));
        assert_eq!((f("skim_buy_at_floor_bps").offset, f("skim_buy_at_floor_bps").size), (532, 2));
        assert_eq!((f("skim_sell_at_floor_bps").offset, f("skim_sell_at_floor_bps").size), (534, 2));
        assert_eq!((f("pad6").offset, f("pad6").size), (536, 32));
        assert_eq!((f("max_bounty").offset, f("max_bounty").size), (568, 8));
        assert_eq!((f("bounty_bps").offset, f("bounty_bps").size), (576, 2));
        // three u16 at 578..584
        assert_eq!((f("wall_band_ticks").offset, f("wall_band_ticks").size), (578, 2));
        assert_eq!((f("wall_tail_bps").offset, f("wall_tail_bps").size), (580, 2));
        assert_eq!((f("wall_list_bps").offset, f("wall_list_bps").size), (582, 2));
        // ⛔ No reserve is left: everything from 584 is carved (referrals, referrer / holders,
        // tiers, the team fee). This line FAILS on any carve-out and has to be changed by hand; the
        // next field needs a realloc or a new version.
        assert!(FloorPool::FIELDS.iter().all(|f| f.name != "reserved"), "the reserve is spent");
        assert_eq!((f("pad5").offset + f("pad5").size), 784);
        assert_eq!(FloorPool::SIZE, 784);
        let mut p = FloorPool::zeroed();
        p.bid_width = 120;
        p.skim_sell_at_floor_bps = 1_000;
        p.pad6 = [9; 32];
        let bytes = bytemuck::bytes_of(&p);
        assert_eq!(&bytes[528..532], &120i32.to_le_bytes());
        assert_eq!(&bytes[534..536], &1_000u16.to_le_bytes());
        assert_eq!(&bytes[536..568], &[9u8; 32]);
    }

/// The referral fields sit at 584..640, the struct is STILL 784 B, and nothing before 584 moved.
    ///
    /// Mutations this kills: any of the four moved or resized; `reserved` left at 200 (which would
    /// overrun 784 and fail `layout!`'s own contiguity check, but the offsets below say WHICH
    /// field); the carve started anywhere but the 8-aligned 584; a u64 placed after the `[u8;32]`
    /// (which `repr(C)` would pad, so `SIZE` would move).
    #[test]
    fn floor_pool_referral_carve_out_at_584() {
        let f = |n: &str| FloorPool::FIELDS.iter().find(|f| f.name == n).unwrap();
        assert_eq!((f("referral_owed").offset, f("referral_owed").size), (584, 8));
        assert_eq!((f("referral_fees_paid").offset, f("referral_fees_paid").size), (592, 8));
        assert_eq!((f("trader_ref_owed").offset, f("trader_ref_owed").size), (600, 8));
        assert_eq!((f("trader_ref_booked").offset, f("trader_ref_booked").size), (608, 8));
        assert_eq!((f("pad7").offset, f("pad7").size), (616, 16));
        assert_eq!((f("referral_share_bps").offset, f("referral_share_bps").size), (632, 2));
        assert_eq!((f("stair_steps").offset, f("stair_steps").size), (634, 1));
        assert_eq!((f("stair_ready").offset, f("stair_ready").size), (635, 1));
        assert_eq!((f("stair_growth_bps").offset, f("stair_growth_bps").size), (636, 2));
        // the staircase parameter set's kind, carved from `pad3`.
        assert_eq!((f("stair_kind").offset, f("stair_kind").size), (638, 1));
        assert_eq!((f("stair_owner").offset, f("stair_owner").size), (639, 1));
        // the next carve (the referrer) starts where `reserved` did; see the case below.
        assert_eq!((f("referrer").offset, f("referrer").size), (640, 32));
        // ⛔ The whole point of a carve: the account does not grow, so no deployed pool migrates.
        assert_eq!(FloorPool::SIZE, 784);
        assert_eq!(FloorPool::ACCOUNT_LEN, 792);
        // every u64 still 8-aligned (the layout rule) and the carve is padding-free
        assert_eq!(f("referral_owed").offset % 8, 0);
        assert_eq!(f("referral_fees_paid").offset % 8, 0);

        let mut p = FloorPool::zeroed();
        p.referral_owed = 7;
        p.referral_fees_paid = 11;
        p.trader_ref_owed = 13;
        p.trader_ref_booked = 17;
        p.pad7 = [3; 16];
        p.referral_share_bps = 1_000;
        let bytes = bytemuck::bytes_of(&p);
        assert_eq!(&bytes[584..592], &7u64.to_le_bytes());
        assert_eq!(&bytes[592..600], &11u64.to_le_bytes());
        assert_eq!(&bytes[600..608], &13u64.to_le_bytes());
        assert_eq!(&bytes[608..616], &17u64.to_le_bytes());
        assert_eq!(&bytes[616..632], &[3u8; 16]);
        assert_eq!(&bytes[632..634], &1_000u16.to_le_bytes());
    }

/// `pending_governance` occupies the 32 bytes at 200, the whole former reserve; the account is
/// still 240 B.
///
/// Mutation this kills: the field given an offset other than 200 (which would move it onto a live
/// field and change what an existing account reads).
    #[test]
    fn hook_config_referral_distributor_takes_the_whole_reserve() {
        let f = |n: &str| HookConfig::FIELDS.iter().find(|f| f.name == n).unwrap();
        // the last 32 bytes: `pending_governance`
        assert_eq!((f("pending_governance").offset, f("pending_governance").size), (200, 32));
        assert_eq!(HookConfig::SIZE, 232);
        assert_eq!(HookConfig::ACCOUNT_LEN, 240);
        // there is no `reserved` left on this account: the next HookConfig field needs a realloc
        // or a new version.
        assert!(HookConfig::FIELDS.iter().all(|f| f.name != "reserved"));
        // a zeroed config has no pending governance
        let hc = HookConfig::zeroed();
        assert_eq!(hc.pending_governance, [0u8; 32]);
    }

    /// Mutation: THE SIZE ASSERT NOT UPDATED — a carve-out that silently WIDENED the account.
    /// `layout!` asserts `size_of == SIZE` at compile time, so the way this goes wrong is the other
    /// one: growing `SIZE` (and the rent, and every deployed account) to make room instead of
    /// taking the room from `reserved`.
    #[test]
    fn the_referral_carve_out_never_widened_the_account() {
        assert_eq!(FloorPool::SIZE, 784, "the struct is the size it was before the referral fields");
        assert_eq!(FloorPool::ACCOUNT_LEN, 792);
        let last = FloorPool::FIELDS.last().unwrap();
        // the team fee spent the last of `reserved`, so the tail is the team carve's pad.
        assert_eq!(last.name, "pad5");
        assert_eq!(last.offset + last.size, FloorPool::SIZE, "the tail still ends at SIZE");
        // Every byte from 584 is a carved field or its pad: referrals 56 (8 + 8 + 32 + 2 + 6 pad),
        // referrer / holders 40 (32 + 1 + 1 + 6 pad), tiers 80 (32 + 32 + 8 + 8), team fee 24
        // (8 + 8 + 2 + 6 pad) — 200 in all.
        let carved: usize = FloorPool::FIELDS.iter().filter(|f| f.offset >= 584).map(|f| f.size).sum();
        assert_eq!(carved, 56 + 40 + 80 + 24, "every byte from 584 is a carved field or its pad");
        assert_eq!(200 - 56 - 40 - 80 - 24, 0, "the 200-byte reserve is spent");
    }

/// The referrer / holders fields sit at 640..680, the struct is still 784 B, and nothing before
/// 640 moved.
///
/// Mutations this kills: any of the four moved or resized; the referrer written over the retired
/// `referral_distributor` bytes @600; `reserved` left at 144 (which overruns 784 and fails
/// `layout!`, but this says WHICH field); the tail un-8-aligned.
    #[test]
    fn floor_pool_holders_carve_out_at_640() {
        let f = |n: &str| FloorPool::FIELDS.iter().find(|f| f.name == n).unwrap();
        // the retired distributor's bytes are `trader_ref_owed` / `_booked` / `pad7`.
        assert_eq!((f("trader_ref_owed").offset, f("trader_ref_owed").size), (600, 8));
        assert_eq!((f("pad7").offset, f("pad7").size), (616, 16), "reserved for a per-pool rate copy");
        assert_eq!((f("referral_share_bps").offset, f("referral_share_bps").size), (632, 2));
        assert_eq!((f("referrer").offset, f("referrer").size), (640, 32));
        assert_eq!((f("pad8").offset, f("pad8").size), (672, 1), "the retired version byte, padding");
        assert_eq!((f("holders_hooked").offset, f("holders_hooked").size), (673, 1));
        assert_eq!((f("pad4").offset, f("pad4").size), (674, 6));
        // the tiers carve follows (`floor_pool_tiers_carve_out_at_680`), then the team fee
        // (`floor_pool_team_carve_out_at_760`).
        assert_eq!((f("referrer_t2").offset, f("referrer_t2").size), (680, 32));
        assert_eq!(f("referrer_t2").offset % 8, 0);
        assert_eq!(FloorPool::SIZE, 784);

        let mut p = FloorPool::zeroed();
        p.trader_ref_owed = 0xD1D1_D1D1_D1D1_D1D1;
        p.trader_ref_booked = 0xD1D1_D1D1_D1D1_D1D1;
        p.pad7 = [0xD1; 16];
        p.referrer = [0x5E; 32];
        p.pad8 = 0xD2;
        p.holders_hooked = 1;
        let bytes = bytemuck::bytes_of(&p);
        assert_eq!(&bytes[600..632], &[0xD1u8; 32], "the retired distributor bytes are its own");
        assert_eq!(&bytes[640..672], &[0x5Eu8; 32]);
        assert_eq!(bytes[672], 0xD2, "the retired version byte is its own");
        assert_eq!(bytes[673], 1);
        assert!(p.is_hooked());
        assert_eq!(p.referral_destination(), Some(&[0x5Eu8; 32]), "the referrer is the destination, never @600");
    }

/// The team fee: three fields at the end of the struct, ordered u64, u64, u16+pad so the carve is
/// padding-free and ends exactly at 784.
    /// Mutations: a field moved or resized; the u16 first (repr(C) pads); the carve left over the tiers
    /// at 680 (transposes `referrer_t2`); a `reserved` kept (overruns 784 and fails `layout!`).
    #[test]
    fn floor_pool_team_carve_out_at_760() {
        let f = |n: &str| FloorPool::FIELDS.iter().find(|f| f.name == n).unwrap();
        assert_eq!((f("referral_paid_t3").offset, f("referral_paid_t3").size), (752, 8), "nothing before 760 moved");
        assert_eq!((f("team_owed").offset, f("team_owed").size), (760, 8));
        assert_eq!((f("team_fees_paid").offset, f("team_fees_paid").size), (768, 8));
        assert_eq!((f("team_fee_bps").offset, f("team_fee_bps").size), (776, 2));
        assert_eq!((f("pad5").offset, f("pad5").size), (778, 6));
        assert!(FloorPool::FIELDS.iter().all(|f| f.name != "reserved"), "the reserve is spent");
        assert_eq!(FloorPool::SIZE, 784);
        let mut p = FloorPool::zeroed();
        (p.team_owed, p.team_fees_paid, p.team_fee_bps) = (7, 11, 25);
        p.referrer_t2 = [0x72; 32];
        let b = bytemuck::bytes_of(&p);
        assert_eq!((&b[760..768], &b[768..776], &b[776..778]), (&7u64.to_le_bytes()[..], &11u64.to_le_bytes()[..], &25u16.to_le_bytes()[..]));
        assert_eq!(&b[680..712], &[0x72u8; 32], "the tiers are untouched by the team carve");
    }

/// A zeroed pool reads no rate, nothing owed, nothing paid (no setter).
    #[test]
    fn a_pool_written_before_the_team_fee_reads_a_zero_rate_and_owes_nothing() {
        let mut old = vec![0u8; FloorPool::SIZE];
        old[0] = 1;
        let p: FloorPool = bytemuck::pod_read_unaligned(&old);
        assert_eq!((p.team_fee_bps, p.team_owed, p.team_fees_paid), (0, 0, 0));
    }

/// `team_bps` takes the first half of the old `pad2: u32`, `team_owed` follows `quote_mint`; 512 B;
/// a zeroed launch reads zeros (no rate, nothing owed).
/// Mutations: team_bps at 382; team_owed over quote_mint @400; the struct widened.
    #[test]
    fn launch_team_carve_out() {
        let f = |n: &str| Launch::FIELDS.iter().find(|f| f.name == n).unwrap();
        assert_eq!((f("holder_sell").offset, f("team_bps").offset, f("team_bps").size), (378, 380, 2));
        assert_eq!((f("pad2").offset, f("pad2").size, f("registry_index").offset), (382, 2, 384));
        assert_eq!((f("quote_mint").offset, f("team_owed").offset, f("team_owed").size), (400, 432, 8));
        // the team buy fields follow `team_owed`
        assert_eq!((f("team_step_tokens").offset, f("pad7").offset, f("pad7").size), (440, 476, 32));
        // the per-slot team cliffs sit at @72; the trader-referral sale bump at @364.
        assert_eq!((f("team_cliffs_paid_by_slot").offset, f("team_cliffs_paid_by_slot").size), (72, 4));
        assert_eq!((f("pad3").offset, f("pad3").size, f("pad4").offset, f("pad0").offset), (76, 28, 104, 7));
        assert_eq!((f("trader_ref_sale_bump").offset, f("pad1").offset, f("pad1").size), (364, 365, 3));
        // The staircase rent reserve took the last 4 bytes of `reserved`: no `reserved` left, still 512 B.
        assert_eq!((f("stair_rent_budget").offset, f("stair_rent_budget").size, Launch::SIZE), (508, 4, 512));
        assert!(Launch::FIELDS.iter().all(|f| f.name != "reserved"));
        let mut old = vec![0u8; Launch::SIZE];
        old[0] = 3;
        let l: Launch = bytemuck::pod_read_unaligned(&old);
        assert_eq!((l.team_bps, l.team_owed), (0, 0));
        assert_eq!((l.team_step_tokens, l.team_wallet_tokens, l.team_cliffs_paid_by_slot), (0, 0, 0), "no team buy");
    }

/// The platform-fee setting takes the front of `reserved` @304 (8 -> 3), 312 B, nothing before
/// 304 moved; ALL-ZERO bytes read as the DEFAULTS, 100 / 25, with no setter step.
    /// Mutations: a field moved or resized; the struct widened; an unset config read raw (0 %, a
    /// window where every new launch is free); the flag ignored (a set 0 read as the default).
    #[test]
    fn launch_config_team_fee_carve_at_304_reads_the_defaults_from_zero_bytes() {
        let f = |n: &str| LaunchConfig::FIELDS.iter().find(|f| f.name == n).unwrap();
        assert_eq!((f("pending_governance").offset, f("pending_governance").size), (272, 32), "nothing before 304 moved");
        assert_eq!((f("team_sale_bps_raw").offset, f("team_sale_bps_raw").size), (304, 2));
        assert_eq!((f("team_pool_bps_raw").offset, f("team_pool_bps_raw").size), (306, 2));
        assert_eq!((f("team_bps_set").offset, f("team_bps_set").size), (308, 1));
        assert_eq!((f("reserved").offset, f("reserved").size, LaunchConfig::SIZE), (309, 3, 312));
        // a config written before v2: `reserved` was zero, so the carve is zero
        let mut old = vec![0u8; LaunchConfig::SIZE];
        old[0] = 1;
        let c: LaunchConfig = bytemuck::pod_read_unaligned(&old);
        assert_eq!((c.team_sale_bps(), c.team_pool_bps()), (100, 25), "zero bytes are the defaults");
        assert_eq!((c.team_sale_bps(), c.team_pool_bps()), (crate::constants::TEAM_SALE_FEE_BPS_DEFAULT, crate::constants::TEAM_POOL_FEE_BPS_DEFAULT));
        // set: the raw numbers, 0 included (a set 0 is 0, never the default)
        let mut s = c;
        (s.team_bps_set, s.team_sale_bps_raw, s.team_pool_bps_raw) = (1, 0, 0);
        assert_eq!((s.team_sale_bps(), s.team_pool_bps()), (0, 0), "a SET zero charges nothing");
        (s.team_sale_bps_raw, s.team_pool_bps_raw) = (40, 50);
        assert_eq!((s.team_sale_bps(), s.team_pool_bps()), (40, 50));
        let b = bytemuck::bytes_of(&s);
        assert_eq!((&b[304..306], &b[306..308], b[308]), (&40u16.to_le_bytes()[..], &50u16.to_le_bytes()[..], 1));
    }

    /// FEE SETTINGS v2: no byte pattern reads above its cap — the setter refuses one, and the reading
    /// clamps whatever else could have put it there. Mutation: the clamp dropped (a raw 101 charged).
    #[test]
    fn a_raw_rate_above_its_cap_reads_as_the_cap() {
        use crate::constants::{TEAM_POOL_FEE_BPS_MAX, TEAM_SALE_FEE_BPS_MAX};
        let mut c = LaunchConfig::zeroed();
        (c.team_bps_set, c.team_sale_bps_raw, c.team_pool_bps_raw) = (1, u16::MAX, TEAM_POOL_FEE_BPS_MAX + 1);
        assert_eq!((c.team_sale_bps(), c.team_pool_bps()), (TEAM_SALE_FEE_BPS_MAX, TEAM_POOL_FEE_BPS_MAX));
        for raw in 0..=TEAM_POOL_FEE_BPS_MAX {
            assert_eq!(team_rate_setting(1, raw, 25, TEAM_POOL_FEE_BPS_MAX), raw, "every rate up to the cap reads back");
            assert_eq!(team_rate_setting(0, raw, 25, TEAM_POOL_FEE_BPS_MAX), 25, "unset is the default whatever the raw bytes");
        }
    }

/// Three-tier referrals: the four tier fields sit at 680..760, the struct is still 784 B, and
/// nothing before 680 moved; a zeroed pool reads zero in all four, which is "single-tier".
    ///
    /// Mutations this kills: a tier wallet or counter moved, resized or transposed; the carve started
    /// anywhere but the 8-aligned 680 (over `floor_holders`' raw reads at 648/680/681 account
    /// offsets, i.e. struct 640/672/673); `reserved` left at 104 (overruns 784 and fails `layout!`).
    #[test]
    fn floor_pool_tiers_carve_out_at_680() {
        let f = |n: &str| FloorPool::FIELDS.iter().find(|f| f.name == n).unwrap();
        assert_eq!((f("referrer").offset, f("referrer").size), (640, 32), "tier 1 did not move");
        assert_eq!((f("referrer_t2").offset, f("referrer_t2").size), (680, 32));
        assert_eq!((f("referrer_t3").offset, f("referrer_t3").size), (712, 32));
        assert_eq!((f("referral_paid_t2").offset, f("referral_paid_t2").size), (744, 8));
        assert_eq!((f("referral_paid_t3").offset, f("referral_paid_t3").size), (752, 8));
        // `[760, 784)` is the team fee's (`floor_pool_team_carve_out_at_760`).
        assert_eq!(f("team_owed").offset, 760, "the next field starts at the tiers' last byte");
        assert_eq!(FloorPool::SIZE, 784);
        let mut p = FloorPool::zeroed();
        p.referrer_t2 = [0x72; 32];
        p.referrer_t3 = [0x73; 32];
        p.referral_paid_t2 = 0x0102_0304;
        p.referral_paid_t3 = 0x0506_0708;
        let bytes = bytemuck::bytes_of(&p);
        assert_eq!(&bytes[680..712], &[0x72u8; 32]);
        assert_eq!(&bytes[712..744], &[0x73u8; 32]);
        assert_eq!(&bytes[744..752], &0x0102_0304u64.to_le_bytes());
        assert_eq!(&bytes[752..760], &0x0506_0708u64.to_le_bytes());
        assert!(bytes[640..680].iter().all(|&b| b == 0), "tier 1 / the holders carve untouched");
    }

    /// Mutation: AN OLDER ACCOUNT DECODED AS CONFIGURED. A `FloorPool` written before these fields
    /// existed has the old zeroed `reserved` in [584, 784) — which is byte for byte what an
    /// unreferred pool written today has. ⛔ 32 zero bytes are a VALID pubkey (base58 `111…1`, the
    /// System Program), so a raw read reports a configured destination; only `referral_destination`
    /// is honest. ⚠ The two cases are INDISTINGUISHABLE from these bytes and are meant to be: both
    /// fund nobody, and neither can ever start (property (c), no per-pool setter).
    #[test]
    fn a_pool_written_before_referrals_reads_as_unreferred() {
        // exactly what the old layout left behind: 784 bytes, zeros from 584 on
        let mut old = vec![0u8; FloorPool::SIZE];
        old[0] = 1; // version
        old[520..528].copy_from_slice(&42u64.to_le_bytes()); // last_ratchet_slot, a pre-existing field
        let p: FloorPool = bytemuck::pod_read_unaligned(&old);
        assert_eq!(p.last_ratchet_slot, 42, "the bytes before the carve-out still decode");
        assert_eq!(p.referral_destination(), None, "an all-zero destination is ABSENT, never the System Program");
        assert!(!p.funds_referrals());
        // and every meter and the rate read as the zeros they are — nothing owed, nothing paid,
        // no share: an old pool is a whole, consistent "unreferred", not a half-configured one
        assert_eq!((p.referral_owed, p.referral_fees_paid, p.referral_share_bps), (0, 0, 0));
        // and a referred pool is not mistaken for one of them: "referred" is a REFERRER WALLET
        // @640, never the retired distributor bytes @600
        let mut new_pool = FloorPool::zeroed();
        new_pool.referrer = [0x11; 32];
        assert_eq!(new_pool.referral_destination(), Some(&[0x11u8; 32]));
        assert!(new_pool.funds_referrals());
        assert_eq!((p.referrer, p.holders_hooked), ([0u8; 32], 0), "an old pool is unreferred, unhooked");
    }
}
