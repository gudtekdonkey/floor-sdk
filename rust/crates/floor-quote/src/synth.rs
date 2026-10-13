//! Synthetic FLOOR pools: `carpenter_quote::synth` (re-exported here) builds the AMM side from a list
//! of positions; this module adds FLOOR's pool shape ([`FLOOR`]) and a bound `FloorPool`
//! ([`floor_pool_defaults`]). For tests, golden vectors and app mocks.

use floor_types::constants::{DYNAMIC_FEE_FLAG, FLOOR_CALLBACK_ACCOUNTS, FLOOR_HOOK_FLAGS, FLOOR_MAX_ROUNDS, MAX_TICK};
use floor_types::layout::FloorPool;

pub use carpenter_quote::synth::*;

/// A FLOOR pool as `initialize_pool` writes one: dynamic fee, `lp_fee` 0, FLOOR's flags, round cap
/// and callback-account count. Pass `floor_types::ids::FLOOR_HOOK_ID` as `PoolKeys::hook`.
pub const FLOOR: SynthKind = SynthKind::Hooked {
    fee: DYNAMIC_FEE_FLAG,
    lp_fee: 0,
    flags: FLOOR_HOOK_FLAGS,
    max_rounds: FLOOR_MAX_ROUNDS,
    callback_accounts: FLOOR_CALLBACK_ACCOUNTS,
};

/// A bound `FloorPool` as `register_pool` + `before_initialize` leave it, with the policy constants; the caller then
/// sets the mechanism state it wants (vault, bid, ask, ladder). The at-floor skim endpoints are the values
/// `register_pool` writes: 0 and `skim_sell_at_floor_bps(skim_sell_far_bps)` (1.5x far).
/// `team_fee_bps = TEAM_POOL_FEE_BPS_DEFAULT`, the rate a pool created on an UNSET launch config carries; set it to
/// the pool's own rate to model a pool created under another setting (0..=`TEAM_POOL_FEE_BPS_MAX`), or to 0 for a
/// zero-rate pool.
/// ⛔ A quote of a REAL pool reads that pool's `team_fee_bps`, never this default.
#[allow(deprecated)]
pub fn floor_pool_defaults(amm_pool: [u8; 32], quote_mint: [u8; 32], base_mint: [u8; 32], _creator: [u8; 32]) -> FloorPool {
    use floor_types::constants as c;
    let mut fp: FloorPool = bytemuck::Zeroable::zeroed();
    fp.version = 1;
    fp.pool_bound = 1;
    fp.quote_is_token1 = 0;
    fp.mode = c::MODE_CUSTOM;
    fp.tick_spacing = i32::from(c::FLOOR_TICK_SPACING);
    fp.floor_tick = crate::price::ceil_to_spacing(MAX_TICK, c::FLOOR_TICK_SPACING).unwrap_or(MAX_TICK);
    fp.amm_pool = amm_pool;
    fp.quote_mint = quote_mint;
    fp.base_mint = base_mint;
    // the recipients are the account tail's creator slots, which a quote never reads.
    fp.skim_buy_far_bps = 250;
    fp.skim_sell_far_bps = 250;
    fp.skim_buy_at_floor_bps = c::DEFAULT_SKIM_BUY_AT_FLOOR_BPS;
    fp.skim_sell_at_floor_bps = c::skim_sell_at_floor_bps(fp.skim_sell_far_bps);
    fp.creator_buy_bps = 25;
    fp.creator_sell_bps = 25;
    // the wall is unused: `register_pool` leaves the bytes zero.
    fp.wall_ticks = 0;
    // every pool is a staircase: `register_pool` refuses one without, and the quoter's `vault_bought` refuses a state
    // that is not one
    fp.stair_steps = 1;
    fp.stair_growth_bps = 10_000;
    fp.platform_bps = c::PLATFORM_BPS;
    fp.team_fee_bps = c::TEAM_POOL_FEE_BPS_DEFAULT; // an unset config's pool rate
    fp.min_bid = c::MIN_BID_LAMPORTS;
    fp.min_sellable = c::MIN_SELLABLE;
    // `owner_owed / owner_paid` are zero on every new pool.
    // ⛔ `referrer` (@640) is LEFT ZERO on purpose: a synthesised pool funds no referrer, so a quote taken against it
    // is the creator's whole fee. Never set it here to "make the quote realistic" -- the split does not change what a
    // TRADER pays (it splits a fee already taken), so a non-zero value here would change nothing but the honesty of
    // this function's name.
    fp
}
