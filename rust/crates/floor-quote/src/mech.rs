//! ⛔ THE ONE PLACE `floor-math` IS CALLED. Everything the quoter needs from the mechanism math goes
//! through a function in this file, so a signature change in `floor-math` is fixed HERE and nowhere
//! else.
//!
//! | Call | `floor-math`'s shape |
//! |---|---|
//! | `skim_bps` | `(distance: i32, ramp: i32, buy: bool, buy_far: u16, sell_far: u16, buy_at_floor: u16, sell_at_floor: u16) -> u64` |
//! | `bid_amounts` | `(price: u128, lower: i32, width: i32, liquidity: u128, deployed: u64) -> FloorMathResult<(u64, u64)>` |
//! | `band_ticks` / `dear_edge` / `bid_width_or_spacing` | `floor-math/src/band.rs` |
//! | `position_liquidity` | `(price: u128, lower: i32, upper: i32, quote: u64, base: u64) -> FloorMathResult<u128>` |
//! | `solve_floor_tick_with` | `(q: u128, extra: u64, state: &FloorState, supply: u64) -> FloorMathResult<i32>` |
//! | `sale_tokens_for` | `(lamports: u64, price_x64: u128, remaining: u64) -> FloorMathResult<(u64, u64)>` |
//! | `sale_buy_with_team` | `(gross: u64, price_x64: u128, remaining: u64, team_bps: u16) -> FloorMathResult<(u64, u64, u64)>` = `(tokens, cost, fee)` |
//! | `sell_back_with_team` | `(payout: u64, team_bps: u16) -> FloorMathResult<(u64, u64)>` = `(fee, received)` |
//! | `take_fees` | `(state: &mut FloorState, leg: u64, is_buy: bool, tick: i32, cushion_top: Option<i32>) -> FloorMathResult<FeeTake>` |
//! | `ceil_to_spacing` / `floor_to_spacing` | `(tick: i32, spacing: i32) -> FloorMathResult<i32>` |
//! | `FloorState` | `#[repr(C)]` `Pod`, `FLOOR_STATE_SIZE == 784`, the `FloorPool` field list |
//! | `holders::claimable` | `(book: Book, rec: Record, lifetime: u64, live_balance: u64) -> u64` (never fails) |
//!
//! ⛔ **THE SPACING SNAPS ARE floor-math's, and the CLAMPS are why.** v4's min/max-usable-tick clamps
//! agree with floor-math's snaps away from the bounds and DISAGREE at them — the one place the value
//! is used: `ceil_to_spacing(MAX_TICK, 60)` is **443,520** here against v4's usable **443,580**
//! (the EVM's `FloorMath`), which is what the hook writes as the "nothing affordable" tick.
//! `carpenter_quote::price::min_usable_tick` / `max_usable_tick` keep the v4 semantics under their
//! own names: they are a DISPLAY range, never a FLOOR snap.
//!
//! ⛔ `take_fees` is floor-math's: the hook program books through the same function, so the quoter's
//! booking and the chain's are ONE implementation. `intake` is called inside it. It measures the skim
//! distance to the band's DEAR EDGE (`floor_tick − band_ticks`) and reads the at-floor endpoints off
//! the state.
//!
//! ⛔ The `FloorPool` ↔ `FloorState` reinterpretation is a CAST of the same 784 bytes, never a
//! field-by-field copy: a copy that forgot one field would solve against a different pool than the
//! hook does. `mirror_is_the_same_bytes` below pins the two structs field for field on a value with
//! every field distinct, so a reordered field in either crate fails here and not in a parity diff.
//!
//! A `floor-math` refusal reaches a caller through [`crate::error::fm`]: `Math(e)` keeps the
//! clmm-math name (`TickOutOfRange`, …); every other variant is checked arithmetic the program
//! reports as `MathOverflow`.

use floor_math::{FeeTake, FloorState};
use floor_types::constants::SKIM_RAMP_TICKS;
use floor_types::holders::{HolderBook, HolderRecord};
use floor_types::layout::FloorPool;

use crate::error::{fm, QuoteResult};

const _: () = assert!(core::mem::size_of::<FloorPool>() == floor_math::FLOOR_STATE_SIZE);

/// `floor_math::FloorState` over the same bytes as a `FloorPool` (both `#[repr(C)]` `Pod`, 784 B;
/// the size equality is the assertion above, the field order the test below).
fn mirror(state: &FloorPool) -> &FloorState {
    bytemuck::from_bytes(bytemuck::bytes_of(state))
}

fn mirror_mut(state: &mut FloorPool) -> &mut FloorState {
    bytemuck::from_bytes_mut(bytemuck::bytes_of_mut(state))
}

/// `skim_bps(distance, ramp, buy, buy_far, sell_far, buy_at_floor, sell_at_floor)`, ramp fixed at
/// `SKIM_RAMP_TICKS`; `distance` is to the band's DEAR edge.
pub fn skim_bps(distance: i32, is_buy: bool, buy_far: u16, sell_far: u16, buy_at_floor: u16, sell_at_floor: u16) -> u64 {
    floor_math::skim_bps(distance, SKIM_RAMP_TICKS, is_buy, buy_far, sell_far, buy_at_floor, sell_at_floor)
}

/// `_bandTicks()`: the band's width on this pool's grid.
pub fn band_ticks(state: &FloorPool) -> QuoteResult<i32> {
    floor_math::band_ticks(state.tick_spacing).map_err(fm)
}

/// `_dearEdge()`: `floor_tick − band_ticks`, where the backing solves to.
pub fn dear_edge(state: &FloorPool) -> QuoteResult<i32> {
    floor_math::dear_edge(state.floor_tick, state.tick_spacing).map_err(fm)
}

/// `_bidWidth()`: the deployed bid's width; one spacing on a pool with no bid yet.
pub fn bid_width(state: &FloorPool) -> i32 {
    floor_math::bid_width_or_spacing(state.bid_width, state.tick_spacing)
}

/// `position_liquidity(p, lower, upper, quote, base)` (the EVM's `FloorPositionMath`), what
/// `deploy_bid` sizes the band with.
pub fn position_liquidity(sqrt_price_x64: u128, lower: i32, upper: i32, quote: u64, base: u64) -> QuoteResult<u128> {
    floor_math::position_liquidity(sqrt_price_x64, lower, upper, quote, base).map_err(fm)
}

/// `take_fees(leg, is_buy, tick)`: the skim (through `intake`), the creator fee and the holder
/// fee, booked on `state` all-or-nothing. The hook delta on the quote leg is `FeeTake::total`.
/// `cushion_top`: the gap buy skim's input, as the chain's callback passes it.
pub fn take_fees(state: &mut FloorPool, leg: u64, is_buy: bool, tick: i32, cushion_top: Option<i32>) -> QuoteResult<FeeTake> {
    floor_math::take_fees(mirror_mut(state), leg, is_buy, tick, cushion_top).map_err(fm)
}

/// `intake(x)` (the EVM's proceeds path): the platform cut on one quote inflow, accrued on `state`; returns what the
/// vault keeps. The re-place's close routes the bid's LP fees through it.
pub fn intake(state: &mut FloorPool, x: u64) -> QuoteResult<u64> {
    floor_math::intake(mirror_mut(state), x).map_err(fm)
}

/// `bid_amounts(p, lower, width, L, deployed)` → `(bid_eth, bid_tok)`. The band is
/// `[lower, lower + width]`; pass [`bid_width`], never the spacing.
pub fn bid_amounts(sqrt_price_x64: u128, bid_lower: i32, width: i32, bid_liquidity: u128, deployed: u64) -> QuoteResult<(u64, u64)> {
    floor_math::bid_amounts(sqrt_price_x64, bid_lower, width, bid_liquidity, deployed).map_err(fm)
}

/// `solve_floor_tick_with(q, extra, state, supply)`.
///
/// ⛔ The state is passed as the account bytes reinterpreted, never re-assembled field by field: a copy
/// that forgot one field would solve against a different pool than the hook does.
pub fn solve_floor_tick_with(quote: u64, extra: u64, state: &FloorPool, supply: u64) -> QuoteResult<i32> {
    floor_math::solve_floor_tick_with(u128::from(quote), extra, mirror(state), supply).map_err(fm)
}

/// `sale_tokens_for(lamports, price_x64, remaining)` → `(tokens, cost)`; cost rounds UP.
pub fn sale_tokens_for(lamports: u64, price_x64: u128, remaining: u64) -> QuoteResult<(u64, u64)> {
    floor_math::sale_tokens_for(lamports, price_x64, remaining).map_err(fm)
}

/// floor-math's sale buy under the team fee -> (tokens, cost, fee).
pub fn sale_buy_with_team(gross: u64, price_x64: u128, remaining: u64, team_bps: u16) -> QuoteResult<(u64, u64, u64)> {
    floor_math::sale_buy_with_team(gross, price_x64, remaining, team_bps).map_err(fm)
}

/// floor-math's sell-back fee -> (fee, received).
pub fn sell_back_with_team(payout: u64, team_bps: u16) -> QuoteResult<(u64, u64)> {
    floor_math::sell_back_with_team(payout, team_bps).map_err(fm)
}

/// floor-math's stateless sell-back payout, `floor(tokens · price_x64 / 2^64)`.
pub fn sell_back_payout_at(tokens: u64, price_x64: u128) -> QuoteResult<u64> {
    floor_math::sell_back_payout_at(tokens, price_x64).map_err(fm)
}

/// `ceil_to_spacing(tick, s)` (the EVM's `_ceilToSpacing`). The arithmetic is i64 inside, so no
/// input tick overflows; the clamps are the EVM's, not v4's usable-tick edges (see the header).
pub fn ceil_to_spacing(tick: i32, spacing: i32) -> QuoteResult<i32> {
    floor_math::ceil_to_spacing(tick, spacing).map_err(fm)
}

/// `floor_to_spacing(tick, s)` (the EVM's `_floorToSpacing`).
pub fn floor_to_spacing(tick: i32, spacing: i32) -> QuoteResult<i32> {
    floor_math::floor_to_spacing(tick, spacing).map_err(fm)
}

/// Holder rewards: `floor_math::holders::claimable` over the
/// `floor_types::holders` accounts — the book's six tally fields, the record's three — and the
/// pool's lifetime `holder_owed + holder_paid_eth`, read off the SAME `FloorPool` bytes the rest of
/// the quoter reads. Never fails (the tally saturates); `crate::holders` does the account checks.
pub fn holder_claimable(book: &HolderBook, rec: &HolderRecord, pool: &FloorPool, live_balance: u64) -> u64 {
    use floor_math::holders as h;
    let b = h::Book {
        acc: book.acc.get(),
        eligible: book.eligible,
        indexed: book.indexed,
        unallocated: book.unallocated,
        pending: book.pending,
        last_slot: book.last_slot,
    };
    let r = h::Record { shares: rec.shares, snapshot: rec.snapshot.get(), owed: rec.owed };
    h::claimable(b, r, h::lifetime(pool.holder_owed, pool.holder_paid_eth), live_balance)
}

/// Three-tier referrals: `floor_math::referral_tier_dues` on the SAME `FloorPool` bytes — `[due1, due2, due3]`, what
/// `claim_referral_fees(tier)` pays each tier now.
pub fn referral_tier_dues(pool: &FloorPool) -> [u64; 3] {
    floor_math::referral_tier_dues(mirror(pool))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::Zeroable;

    /// Every field of `FloorPool` set to a distinct value reads back from the mirror under the same
    /// name: a field added, removed or reordered in either crate changes at least one of them.
    /// (The unused slots are still SET here: they are still bytes both crates carry.)
    #[test]
    fn mirror_is_the_same_bytes() {
        let mut p = FloorPool::zeroed();
        p.version = 1;
        p.bump = 2;
        p.pool_bound = 3;
        p.quote_is_token1 = 4;
        p.ladder_seeded = 5;
        p.needs_ratchet = 6;
        p.mode = 7;
        p.quote_suspended = 8;
        p.tick_spacing = 60;
        p.floor_tick = -1_200;
        p.amm_pool = [0xA1; 32];
        p.pool_initializer = [0xA2; 32];
        p.base_mint = [0xA3; 32];
        p.quote_mint = [0xA4; 32];
        p.pad9 = [0xA5; 32];
        p.pad10 = [0xA6; 32];
        p.skim_buy_far_bps = 201;
        p.skim_sell_far_bps = 202;
        p.creator_buy_bps = 203;
        p.creator_sell_bps = 204;
        p.holder_buy_bps = 205;
        p.holder_sell_bps = 206;
        p.wall_ticks = 207;
        p.platform_bps = 208;
        p.min_bid = 301;
        p.min_sellable = 302;
        p.owner_owed = 303;
        p.owner_paid = 304;
        p.vault_eth = 305;
        p.bid_eth_deployed = 306;
        p.unpaid_skim = 307;
        p.cumulative_volume_eth = 308;
        p.inventory_tokens = 309;
        p.inventory_cost_eth = 310;
        p.ask_tokens_deployed = 311;
        p.ladder_tokens_remaining = 312;
        p.bid_lower = -401;
        p.ask_upper = 402;
        p.ask_width = 403;
        p.seed_lower = -404;
        p.seed_upper = 405;
        p.ladder_lower = -406;
        p.ladder_upper = 407;
        p.wall_split_tick = -408;
        p.bid_liquidity.set(501 | (1 << 100));
        p.ask_liquidity.set(502 | (2 << 100));
        p.seed_liq.set(503 | (3 << 100));
        p.ladder_liq.set(504 | (4 << 100));
        p.creator_fees_owed = 601;
        p.platform_owed = 602;
        p.holder_owed = 603;
        // the wall's tail liquidity and held count — a u128 whose two words are distinct, so a word swap or a
        // one-slot shift fails.
        p.wall_tail_liq.set(604 | (5 << 100));
        p.wall_held_tokens = 606;
        p.seed_fees_swept_eth = 607;
        p.seed_fees_swept_tokens = 608;
        p.ladder_proceeds_eth = 609;
        p.platform_paid_eth = 610;
        p.platform_swept = 611;
        p.creator_fees_paid = 612;
        p.holder_paid_eth = 613;
        p.last_ratchet_slot = 614;
        // ⛔ EVERY field between `last_ratchet_slot` and the end. This mirror test enumerates the struct by hand, so a
        // field added to the account that is NOT listed here is silently NOT CHECKED — the canonical
        // test-that-still-asserts-the-old-shape.
        p.bid_width = 615;
        p.skim_buy_at_floor_bps = 616;
        p.skim_sell_at_floor_bps = 617;
        p.pad6 = [0xAB; 32];
        p.max_bounty = 618;
        p.bounty_bps = 619;
        // the wall's shape words in `pad2`'s bytes, three distinct u16.
        p.wall_band_ticks = 0xCD01;
        p.wall_tail_bps = 0xCD02;
        p.wall_list_bps = 0xCD03;
        // The referral carve-out (four fields @584..634; the share accrues on the pool and is claimed on demand).
        // ⛔ DISTINCT VALUES on every one of them: `referral_owed` @584 and `referral_fees_paid` @592 are two
        // adjacent `u64`s, so a TRANSPOSITION in either crate leaves the size, the alignment and every OTHER field's
        // offset unchanged — this test is the only place on the `floor-math` side where a swapped pair is caught, and
        // it is caught ONLY because the two carry different numbers. Every field is set: an unlisted field is
        // silently NOT CHECKED.
        p.referral_owed = 620;
        p.referral_fees_paid = 621;
        p.trader_ref_owed = 0xAF01;
        p.trader_ref_booked = 0xAF02;
        p.pad7 = [0xAF; 16];
        p.referral_share_bps = 622;
        p.stair_steps = 0xB1;
        p.stair_ready = 0xB2;
        p.stair_growth_bps = 0xB3B4; // the staircase's carve of `pad3`
        p.stair_kind = 0xBB; // the staircase parameter set's kind, carved from `pad3`
        p.stair_owner = 0xBA;
        // Holder rewards + the on-chain referrer: four fields @640..680, carved from the front of `reserved`. Distinct
        // values again, and the retired distributor bytes above keep a DIFFERENT value from `referrer`, so a crate
        // that aliased the two fails here.
        p.referrer = [0x5E; 32];
        p.pad8 = 0xC8; // the retired version byte, padding
        p.holders_hooked = 2;
        p.pad4 = [0xC4; 6];
        // Three-tier referrals: four fields @680..760, carved from the front of `reserved`. Distinct values, and the
        // two tier wallets differ from `referrer` and from each other, so an aliased or transposed pair fails here.
        p.referrer_t2 = [0x72; 32];
        p.referrer_t3 = [0x73; 32];
        p.referral_paid_t2 = 623;
        p.referral_paid_t3 = 624;
        // The team fee: three fields @760..784, the last of `reserved`. Distinct values from the tier counters above,
        // so a team/tier transposition fails here.
        p.team_owed = 625;
        p.team_fees_paid = 626;
        p.team_fee_bps = 627;
        // ⛔ `reserved` is SPENT: `pad5` ends the struct, and it is the bytes both crates carry at the tail.
        p.pad5 = [0xEE; 6];

        let m = mirror(&p);
        assert_eq!(m.version, 1);
        assert_eq!(m.bump, 2);
        assert_eq!(m.pool_bound, 3);
        assert_eq!(m.quote_is_token1, 4);
        assert_eq!(m.ladder_seeded, 5);
        assert_eq!(m.needs_ratchet, 6);
        assert_eq!(m.mode, 7);
        assert_eq!(m.quote_suspended, 8);
        assert_eq!(m.tick_spacing, 60);
        assert_eq!(m.floor_tick, -1_200);
        assert_eq!(m.amm_pool, [0xA1; 32]);
        assert_eq!(m.pool_initializer, [0xA2; 32]);
        assert_eq!(m.base_mint, [0xA3; 32]);
        assert_eq!(m.quote_mint, [0xA4; 32]);
        assert_eq!(m.pad9, [0xA5; 32]);
        assert_eq!(m.pad10, [0xA6; 32]);
        assert_eq!(m.skim_buy_far_bps, 201);
        assert_eq!(m.skim_sell_far_bps, 202);
        assert_eq!(m.creator_buy_bps, 203);
        assert_eq!(m.creator_sell_bps, 204);
        assert_eq!(m.holder_buy_bps, 205);
        assert_eq!(m.holder_sell_bps, 206);
        assert_eq!(m.wall_ticks, 207);
        assert_eq!(m.platform_bps, 208);
        assert_eq!(m.min_bid, 301);
        assert_eq!(m.min_sellable, 302);
        assert_eq!(m.owner_owed, 303);
        assert_eq!(m.owner_paid, 304);
        assert_eq!(m.vault_eth, 305);
        assert_eq!(m.bid_eth_deployed, 306);
        assert_eq!(m.unpaid_skim, 307);
        assert_eq!(m.cumulative_volume_eth, 308);
        assert_eq!(m.inventory_tokens, 309);
        assert_eq!(m.inventory_cost_eth, 310);
        assert_eq!(m.ask_tokens_deployed, 311);
        assert_eq!(m.ladder_tokens_remaining, 312);
        assert_eq!(m.bid_lower, -401);
        assert_eq!(m.ask_upper, 402);
        assert_eq!(m.ask_width, 403);
        assert_eq!(m.seed_lower, -404);
        assert_eq!(m.seed_upper, 405);
        assert_eq!(m.ladder_lower, -406);
        assert_eq!(m.ladder_upper, 407);
        assert_eq!(m.wall_split_tick, -408);
        assert_eq!(m.bid_liquidity.get(), 501 | (1 << 100));
        assert_eq!(m.ask_liquidity.get(), 502 | (2 << 100));
        assert_eq!(m.seed_liq.get(), 503 | (3 << 100));
        assert_eq!(m.ladder_liq.get(), 504 | (4 << 100));
        assert_eq!(m.creator_fees_owed, 601);
        assert_eq!(m.platform_owed, 602);
        assert_eq!(m.holder_owed, 603);
        assert_eq!(m.wall_tail_liq.get(), 604 | (5 << 100));
        assert_eq!(m.wall_held_tokens, 606);
        assert_eq!(m.seed_fees_swept_eth, 607);
        assert_eq!(m.seed_fees_swept_tokens, 608);
        assert_eq!(m.ladder_proceeds_eth, 609);
        assert_eq!(m.platform_paid_eth, 610);
        assert_eq!(m.platform_swept, 611);
        assert_eq!(m.creator_fees_paid, 612);
        assert_eq!(m.holder_paid_eth, 613);
        assert_eq!(m.last_ratchet_slot, 614);
        assert_eq!(m.bid_width, 615);
        assert_eq!(m.skim_buy_at_floor_bps, 616);
        assert_eq!(m.skim_sell_at_floor_bps, 617);
        assert_eq!(m.pad6, [0xAB; 32]);
        assert_eq!(m.max_bounty, 618);
        assert_eq!(m.bounty_bps, 619);
        assert_eq!((m.wall_band_ticks, m.wall_tail_bps, m.wall_list_bps), (0xCD01, 0xCD02, 0xCD03));
        assert_eq!(m.referral_owed, 620, "referral_owed @584 — a swap with referral_fees_paid lands here");
        assert_eq!(m.referral_fees_paid, 621, "referral_fees_paid @592 — the cumulative ACCRUED meter");
        assert_eq!((m.trader_ref_owed, m.trader_ref_booked), (0xAF01, 0xAF02), "trader referrals @600 / @608");
        assert_eq!(m.pad7, [0xAF; 16]);
        assert_eq!(m.referral_share_bps, 622);
        assert_eq!((m.stair_steps, m.stair_ready, m.stair_growth_bps), (0xB1, 0xB2, 0xB3B4));
        assert_eq!((m.stair_kind, m.stair_owner), (0xBB, 0xBA));
        assert_eq!(m.referrer, [0x5E; 32], "referrer @640 — never the retired distributor @600");
        assert_eq!(m.pad8, 0xC8);
        assert_eq!(m.holders_hooked, 2);
        assert_eq!(m.pad4, [0xC4; 6]);
        assert_eq!(m.referrer_t2, [0x72; 32], "referrer_t2 @680");
        assert_eq!(m.referrer_t3, [0x73; 32], "referrer_t3 @712");
        assert_eq!(m.referral_paid_t2, 623, "referral_paid_t2 @744");
        assert_eq!(m.referral_paid_t3, 624, "referral_paid_t3 @752");
        assert_eq!((m.team_owed, m.team_fees_paid, m.team_fee_bps), (625, 626, 627), "team @760 / @768 / @776");
        assert_eq!(m.pad5, [0xEE; 6], "the tail (no `reserved` left) mirrors");
        // ⛔ The one assertion that makes the list above self-checking: if the account grows a field and
        // nobody adds it here, the two struct sizes still agree, so this alone will not catch it —
        // but a field SHRINKING `reserved` will, and `floor_types::layout::FloorPool` is the
        // authority both sides cast through.
        assert_eq!(core::mem::size_of::<FloorPool>(), core::mem::size_of::<FloorState>());
    }

    // The band helpers read the same fields the engine does.
    #[test]
    fn band_helpers_read_the_pool() {
        let mut p = FloorPool::zeroed();
        p.tick_spacing = 60;
        p.floor_tick = 1_200;
        assert_eq!(band_ticks(&p).unwrap(), 120);
        assert_eq!(dear_edge(&p).unwrap(), 1_080);
        assert_eq!(bid_width(&p), 60, "a pre-band bid reads as one spacing");
        p.bid_width = 120;
        assert_eq!(bid_width(&p), 120);
    }
}
