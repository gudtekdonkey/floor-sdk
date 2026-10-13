//! `cargo test -p floor-quote`: FLOOR pools through `carpenter-quote` + `FloorHookQuoter`.
//!
//! What a chain-free test can prove: the callbacks' fees, bindings and refusals by name, the in-swap
//! bid re-place and its booking, and the identities that make the fee breakdown complete. The plain
//! walk is `carpenter-quote`'s own suite; byte-for-byte equality with the pre-split quoter is
//! `tests/floor_vectors.rs`.

use floor_quote::clmm::tick_math;
use floor_quote::floor::{self, AfterSwapIn, FeesTaken, SwapCtx};
use floor_quote::synth::{build_pool, floor_pool_defaults, PoolKeys, SynthPool, SynthPosition, FLOOR};
use floor_quote::types::hook::{BalanceDelta, FEE_CREATOR, FEE_HOLDER, FEE_PLATFORM, FEE_SKIM, FEE_TEAM};
use floor_quote::types::ids::FLOOR_HOOK_ID;
use floor_quote::types::layout::FloorPool;
use floor_quote::mech::floor_to_spacing;
use floor_quote::{no_limit, quote_swap, FloorError, QuoteError, SwapQuoteParams};

fn keys(hook: [u8; 32]) -> PoolKeys {
    PoolKeys {
        pool: [7; 32],
        mint0: [1; 32],
        mint1: [2; 32],
        vault0: [3; 32],
        vault1: [4; 32],
        token_program0: [5; 32],
        token_program1: [6; 32],
        hook,
        hook_authority: if hook == [0; 32] { [0; 32] } else { [9; 32] },
    }
}

fn params(zfo: bool, amount: i128) -> SwapQuoteParams {
    SwapQuoteParams { zero_for_one: zfo, amount_specified: amount, sqrt_price_limit_x64: no_limit(zfo), supply0: 0, cushion_top: None, supply1: 1_000_000_000_000_000 }
}

fn pos(lo: i32, up: i32, l: u128) -> SynthPosition {
    SynthPosition { tick_lower: lo, tick_upper: up, liquidity: l }
}

/// ⛔ ONE implementation of the spacing snaps: v4's usable-tick clamps and the EVM's disagree by exactly one spacing
/// AT the bounds — where `floor_pool_defaults` uses it, and where `register_pool` / `before_initialize` write it.
/// Pinned against `mech`, which is the only caller of `floor-math`.
#[test]
fn the_spacing_snaps_are_floor_maths_at_the_bounds() {
    use floor_quote::price;
    use floor_quote::types::constants::{FLOOR_TICK_SPACING, MAX_TICK, MIN_TICK};
    let s = i32::from(FLOOR_TICK_SPACING);
    assert_eq!(price::ceil_to_spacing(MAX_TICK, FLOOR_TICK_SPACING).unwrap(), floor_quote::mech::ceil_to_spacing(MAX_TICK, s).unwrap());
    assert_eq!(price::floor_to_spacing(MIN_TICK, FLOOR_TICK_SPACING).unwrap(), floor_quote::mech::floor_to_spacing(MIN_TICK, s).unwrap());
    // The measured pair: floor-math's clamp, not v4's usable tick (443,580).
    assert_eq!(price::ceil_to_spacing(MAX_TICK, 60).unwrap(), 443_520);
    assert_eq!(price::floor_to_spacing(MIN_TICK, 60).unwrap(), -443_520);
    // ⛔ `floor_pool_defaults` claims to leave `floor_tick` "as `register_pool` + `before_initialize`
    // leave it"; that claim is this assertion.
    let fp = floor_pool_defaults([1; 32], [2; 32], [3; 32], [4; 32]);
    assert_eq!(fp.floor_tick, floor_quote::mech::ceil_to_spacing(MAX_TICK, s).unwrap());
    // An out-of-range tick is a refusal, never a panic (the wasm crate's "never panics" contract).
    assert_eq!(price::ceil_to_spacing(i32::MIN, 60).unwrap_err().name(), "TickOutOfRange");
    assert_eq!(price::floor_to_spacing(i32::MIN, 60).unwrap_err().name(), "TickOutOfRange");
    // v4's usable ticks keep their own, different answer under their own names.
    assert_eq!(price::max_usable_tick(60).unwrap(), 443_580);
    assert_eq!(price::min_usable_tick(60).unwrap(), -443_580);
}

// ---- FLOOR ------------------------------------------------------------------------------------

fn floor_pool(tick: i32) -> (SynthPool, FloorPool) {
    let k = keys(FLOOR_HOOK_ID);
    let sqrt = tick_math::get_sqrt_price_at_tick(tick).unwrap();
    let sp = build_pool(&k, FLOOR, 60, sqrt, 9, 6, &[pos(tick - 60 * 200, tick + 60 * 200, 50_000_000_000_000)]).unwrap();
    let mut fp = floor_pool_defaults(k.pool, k.mint0, k.mint1, [8; 32]);
    fp.ladder_seeded = 1; // a graduated pool: `floor_pool_defaults` is the unseeded post-registration state
    fp.floor_tick = tick + 60 * 500; // far above the market (a cheaper price): the far rates apply
    (sp, fp)
}

/// A pool whose launch has not graduated (`ladder_seeded != 1`): `floor_hook`'s `before_swap` refuses
/// every swap with `NotSeeded` (7021), right after `amm_pool` / `pool_bound` and before any other
/// check, exact-out included. The quoter refuses by the same name and in the same order, instead of
/// answering a swap the program fails.
#[test]
fn an_unseeded_floor_pool_refuses_every_swap_with_not_seeded() {
    let (sp, mut fp) = floor_pool(100_020);
    let set = sp.tick_array_set().unwrap();
    for seeded in [0u8, 2, 0xFF] {
        fp.ladder_seeded = seeded;
        for zfo in [true, false] {
            for amount in [-1i128, -1_000, -1_000_000_000_000, 1_000] {
                assert_eq!(quote_swap(&sp.account, &set, Some(&fp), &params(zfo, amount)).unwrap_err(), FloorError::NotSeeded, "seeded {seeded} zfo {zfo} amount {amount}");
            }
        }
    }
    assert_eq!(FloorError::NotSeeded.name(), "NotSeeded");
    assert_eq!(QuoteError::from(FloorError::NotSeeded).to_string(), "NotSeeded");
    fp.ladder_seeded = 0;
    fp.pool_bound = 0;
    assert_eq!(quote_swap(&sp.account, &set, Some(&fp), &params(true, -1_000)).unwrap_err(), FloorError::NotBound, "NotBound first");
    fp.pool_bound = 1;
    fp.amm_pool = [0xAB; 32];
    assert_eq!(quote_swap(&sp.account, &set, Some(&fp), &params(true, -1_000)).unwrap_err(), FloorError::WrongPool, "WrongPool first");
    fp.amm_pool = sp.account.key;
    fp.ladder_seeded = 1;
    assert!(quote_swap(&sp.account, &set, Some(&fp), &params(true, -1_000)).is_ok(), "seeded: quotes");
}

#[test]
fn floor_refuses_exact_out_by_name() {
    let (sp, fp) = floor_pool(100_020);
    let set = sp.tick_array_set().unwrap();
    for zfo in [true, false] {
        assert_eq!(quote_swap(&sp.account, &set, Some(&fp), &params(zfo, 1_000)).unwrap_err(), FloorError::ExactOutNotSupported);
    }
}

#[test]
fn floor_needs_its_state_and_binds_its_pool() {
    let (sp, mut fp) = floor_pool(100_020);
    let set = sp.tick_array_set().unwrap();
    assert_eq!(quote_swap(&sp.account, &set, None, &params(true, -1_000)).unwrap_err(), QuoteError::QuoteHookStateMissing);
    fp.amm_pool = [0xAB; 32];
    assert_eq!(quote_swap(&sp.account, &set, Some(&fp), &params(true, -1_000)).unwrap_err(), FloorError::WrongPool);
    fp.amm_pool = sp.account.key;
    fp.pool_bound = 0;
    assert_eq!(quote_swap(&sp.account, &set, Some(&fp), &params(true, -1_000)).unwrap_err(), FloorError::NotBound);

    // The binding's clause 2: the mints and the spacing are checked against the bound pool's fields. A
    // `FloorPool` claiming the other orientation decides which leg pays the sell skim, so accepting
    // one would let a hand-built state charge the wrong side with no refusal.
    fp.pool_bound = 1;
    // The REVERSE orientation is refused by name, whether or not the mints agree with it — the quoter's tick-space
    // math is written for quote = token0 and does not negate ticks the way the EVM's `quoteIsCurrency1` branches do; on
    // chain the state is unreachable, so this is a refusal of hand-built bytes.
    let mut swapped = fp;
    swapped.quote_is_token1 = 1;
    assert_eq!(quote_swap(&sp.account, &set, Some(&swapped), &params(true, -1_000)).unwrap_err(), FloorError::QuoteReverseOrientation);
    let mut agreed = swapped;
    agreed.quote_mint = sp.account.pool.mint1;
    agreed.base_mint = sp.account.pool.mint0;
    assert_eq!(quote_swap(&sp.account, &set, Some(&agreed), &params(true, -1_000)).unwrap_err(), FloorError::QuoteReverseOrientation);
    assert_eq!(FloorError::QuoteReverseOrientation.name(), "QuoteReverseOrientation", "quoter-only: no on-chain twin");
    let mut foreign = fp;
    foreign.base_mint = [0xEE; 32];
    assert_eq!(quote_swap(&sp.account, &set, Some(&foreign), &params(true, -1_000)).unwrap_err(), FloorError::WrongPool);
    let mut mis_spaced = fp;
    mis_spaced.tick_spacing = 10;
    assert_eq!(quote_swap(&sp.account, &set, Some(&mis_spaced), &params(true, -1_000)).unwrap_err(), FloorError::WrongPool);

    // Clause 3: a FLOOR-hooked pool whose key fee is static is `NotDynamicFee`, the hook's own name; otherwise such a
    // pool would quote at `pool.lp_fee` with the 2,500-pip override silently dropped.
    let mut static_fee = sp.clone();
    static_fee.account.pool.fee = 3_000;
    static_fee.account.pool.lp_fee = 3_000;
    let set2 = static_fee.tick_array_set().unwrap();
    assert_eq!(quote_swap(&static_fee.account, &set2, Some(&fp), &params(true, -1_000)).unwrap_err(), FloorError::NotDynamicFee);
    // The pool as built IS dynamic, and quotes.
    assert!(quote_swap(&sp.account, &set, Some(&fp), &params(true, -1_000)).is_ok());
}

#[test]
fn floor_buy_far_from_the_floor_pays_the_far_skim_and_the_creator_fee() {
    let (sp, fp) = floor_pool(100_020);
    let set = sp.tick_array_set().unwrap();
    let leg: u64 = 1_000_000_000; // 1 SOL
    let q = quote_swap(&sp.account, &set, Some(&fp), &params(true, -(leg as i128))).unwrap();
    // distance ≥ ramp → closeness 0 → buy rate = buy_far exactly (skim_bps).
    let skim = leg * u64::from(fp.skim_buy_far_bps) / 10_000;
    let creator = leg * u64::from(fp.creator_buy_bps) / 10_000;
    // `floor_pool_defaults` carries register_pool's team rate (25 bps).
    let team = leg * 25 / 10_000;
    assert_eq!(q.fees.skim, skim);
    assert_eq!(q.fees.creator, creator);
    assert_eq!(q.fees.team, team);
    assert_eq!(q.fees.platform_cut, skim * 250 / 10_000);
    // The fixture's pool carries a zero holder rate — and the tax is REPORTED whatever the rate, so the identity that
    // makes the breakdown complete is `skim + creator + holder + team == hook_delta`.
    assert_eq!(q.fees.holder, 0);
    assert_eq!(q.fees.hook_total() as i128, q.hook_delta.amount0);
    assert_eq!(q.amount_in, leg, "exact-in: the trader pays exactly what they specified");
    assert_eq!(q.hook_delta.amount0, (skim + creator + team) as i128);
    let f = q.floor_after.unwrap();
    assert_eq!(f.vault_eth, skim - skim * 250 / 10_000);
    assert_eq!(f.creator_fees_owed, creator);
    assert_eq!(f.cumulative_volume_eth, leg);
    assert_eq!(q.pool_after.hook_claims0, skim + creator + team);
    // The walk swapped what was left, at the 0.25 % override.
    let swapped = leg - skim - creator - team;
    assert_eq!(q.fees.lp, swapped - swapped * (1_000_000 - 2_500) / 1_000_000);
    assert!(q.replace_actions.is_empty() && q.replace.is_none(), "a buy never re-places the bid");
}

// ---- graduation: the POL is the tail (starting price → infinity) ---------------------------------

/// A graduated pool the way graduation seeds one: the POL position over `pol_range` and the
/// ladder over `[floor(open − 20794), floor(open − 6932)]`, priced at the open.
///
/// ⚠ The tick axis is INVERTED against price here — the quote is token0, so a buy is
/// `zero_for_one` and walks the tick DOWN, and the 2×→8× ask sits BELOW the open. "Above the
/// ladder top" therefore means `tick < ladder_lower`.
fn graduated(open: i32, pol_lower: i32, pol_upper: i32) -> (SynthPool, FloorPool, i32) {
    let s = 60i32;
    let lad_lower = floor_to_spacing(open - 20_794, s).unwrap();
    let lad_upper = floor_to_spacing(open - 6_932, s).unwrap();
    let k = keys(FLOOR_HOOK_ID);
    let sqrt = tick_math::get_sqrt_price_at_tick(open).unwrap();
    let positions = [pos(pol_lower, pol_upper, 40_000_000_000_000), pos(lad_lower, lad_upper, 20_000_000_000_000)];
    let sp = build_pool(&k, FLOOR, 60, sqrt, 9, 6, &positions).unwrap();
    let mut fp = floor_pool_defaults(k.pool, k.mint0, k.mint1, [8; 32]);
    fp.ladder_seeded = 1; // a graduated pool: `floor_pool_defaults` is the unseeded post-registration state
    fp.seed_lower = pol_lower;
    fp.seed_upper = pol_upper;
    fp.ladder_lower = lad_lower;
    fp.ladder_upper = lad_upper;
    (sp, fp, lad_lower)
}

/// ⛔ **A GRADUATED POOL QUOTES A BUY FAR ABOVE THE LADDER TOP.** A POL bounded to `[open − 6932, open]` leaves
/// nothing to trade against past the ladder (on the EVM, `V4Quoter` reverting `NotEnoughLiquidity` on ordinary sizes
/// right after graduation); the POL runs from the starting price to infinity so that past the 2×→8× ladder the POL is
/// the ONLY protocol counterparty, and a buy of any size has to fill.
///
/// Both halves are measured against the SAME pool, positions and leg — only the POL's range
/// differs — so what the assertions separate is the range and nothing else. MEASURED
/// at `open = 80_040`, `ladder_lower = 59_220`, 50,000 SOL in: BOUNDED delivers 1,029,135,690,610,578
/// and the price runs clean out to tick **−443,636** (`sqrt_after == no_limit(true)` — nothing left
/// to trade against); FULL-RANGE delivers 2,541,165,669,347,110 and settles at tick **−3,944**,
/// past the ladder with the tail still bidding. 2.47× the tokens for the same SOL.
///
/// ⛔ **IT IS NOT A DRIFT GUARD ON GRADUATION.** The quoter neither depends on `floor_launch` nor names it, and BOTH
/// ranges are literals inside this function — so if graduation drifted back to the bounded band, this case would
/// still pass, unchanged. What it PINS is the quoter's arithmetic under each range: that the full range is the one a
/// buy past the ladder can fill against, and by how much. The net on graduation itself lives with the program.
///
/// ⚠ **The SELL side is measured here too, and it is unaffected** — `pol_lower` is the only edge
/// that moved and it sits strictly BELOW the current tick, while a sell is `one_for_zero` and walks
/// the tick UP, away from it. The two arrangements return byte-identical sell quotes; both are the
/// degenerate one, because `graduated()` seeds no counterparty above `floor(open)` (the bid is the
/// ratchet's, not graduation's). The assertion's real reach is the UPPER edge: move
/// `pol_upper` in one arm and the two sell quotes stop agreeing.
/// ⚠ **There is no exact-out leg, and there cannot be one**: exact-out is refused on every FLOOR pool by name, in the
/// quoter and on chain, which `floor_refuses_exact_out_by_name` above already pins. The EVM's `V4Quoter` symptom was
/// an exact-IN quote of ordinary sizes; the Solana direction that can bottom out past the ladder is the one measured
/// here.
#[test]
fn a_graduated_pool_fills_a_buy_past_the_ladder_top_only_with_the_full_range_pol() {
    let open = 80_040;
    let leg: i128 = -50_000_000_000_000; // far more SOL than POL + ladder can absorb near the open
    let limit = no_limit(true);

    // The bounded one-doubling range: its LOWER edge is the ladder's UPPER edge, so the two are contiguous and STOP
    // at `lad_lower`.
    let (sp, fp, lad_lower) = graduated(open, floor_to_spacing(open - 6_932, 60).unwrap(), floor_to_spacing(open, 60).unwrap());
    let bounded = quote_swap(&sp.account, &sp.tick_array_set().unwrap(), Some(&fp), &params(true, leg)).unwrap();
    assert_eq!(
        bounded.sqrt_after, limit,
        "the bounded POL runs out at the ladder's far end and the price walks free to the bound"
    );

    // The full-range tail: `[ceil_to_spacing(MIN_TICK, 60), floor(open)]`.
    let (sp, fp, lad_lower_full) = graduated(open, -443_580, floor_to_spacing(open, 60).unwrap());
    assert_eq!(lad_lower, lad_lower_full);
    let full = quote_swap(&sp.account, &sp.tick_array_set().unwrap(), Some(&fp), &params(true, leg)).unwrap();
    assert!(full.sqrt_after > limit, "the tail is still a counterparty, so the price settles rather than running out");
    let after = tick_math::get_tick_at_sqrt_price(full.sqrt_after).unwrap();
    assert!(after < lad_lower, "and the buy really did walk past the ladder top ({after} < {lad_lower})");
    assert!(
        full.amount_out > bounded.amount_out,
        "the same SOL buys strictly more with a counterparty above 8× ({} > {})",
        full.amount_out,
        bounded.amount_out
    );

    // The sell side, against the same two arrangements: `one_for_zero` walks the tick UP, away from
    // the edge that moved, so the two must agree to the unit. (Both are the degenerate quote —
    // `graduated()` seeds nothing above `floor(open)` — which is why the reach of this is the UPPER
    // edge, not the lower one.)
    let sell = params(false, -1_000_000_000_000);
    let (sp_b, fp_b, _) = graduated(open, floor_to_spacing(open - 6_932, 60).unwrap(), floor_to_spacing(open, 60).unwrap());
    let (sp_f, fp_f, _) = graduated(open, -443_580, floor_to_spacing(open, 60).unwrap());
    let sb = quote_swap(&sp_b.account, &sp_b.tick_array_set().unwrap(), Some(&fp_b), &sell).unwrap();
    let sf = quote_swap(&sp_f.account, &sp_f.tick_array_set().unwrap(), Some(&fp_f), &sell).unwrap();
    assert_eq!((sb.amount_in, sb.amount_out, sb.sqrt_after), (sf.amount_in, sf.amount_out, sf.sqrt_after));
    assert_eq!(sb.amount_out, 0, "nothing is seeded above the open in either arrangement");
}

/// ⛔ The holder tax is REPORTED, not dropped. `register_pool` caps the rate per side, so this is the quoter's ONE
/// holder-rate case: the quoter takes any `FloorPool` bytes, and `floor-math` books the fee to `holder_owed` and
/// counts it in the hook delta. Without the field, `skim + creator` would silently miss it and the wasm breakdown
/// would under-report what the trader paid.
#[test]
fn a_holder_tax_is_reported_and_closes_the_fee_identity() {
    let (sp, mut fp) = floor_pool(100_020);
    fp.holder_buy_bps = 50; // 0.5 %, a legal Custom rate
    let set = sp.tick_array_set().unwrap();
    let leg: u64 = 1_000_000_000;
    let q = quote_swap(&sp.account, &set, Some(&fp), &params(true, -(leg as i128))).unwrap();
    let holder = leg * u64::from(fp.holder_buy_bps) / 10_000;
    assert_eq!(q.fees.holder, holder);
    assert_ne!(holder, 0);
    assert_eq!(q.fees.hook_total() as i128, q.hook_delta.amount0, "skim + creator + holder + team IS the hook delta");
    assert_eq!(q.floor_after.unwrap().holder_owed, holder);
    assert_eq!(q.pool_after.hook_claims0, q.fees.hook_total());
}

/// A synthesised pool carries register_pool's rate; the fee is on top.
#[test]
fn a_floor_quote_carries_the_team_fee_on_top() {
    let (sp, fp) = floor_pool(100_020);
    assert_eq!(fp.team_fee_bps, 25);
    let set = sp.tick_array_set().unwrap();
    let q = quote_swap(&sp.account, &set, Some(&fp), &params(true, -1_000_000_000)).unwrap();
    assert_eq!((q.fees.team, q.fee_split0[FEE_TEAM], q.floor_after.unwrap().team_owed), (2_500_000, 2_500_000, 2_500_000));
    assert_eq!(q.fees.hook_total(), q.fees.skim + q.fees.creator + q.fees.holder + q.fees.team);
    let mut old = fp;
    old.team_fee_bps = 0;
    let q0 = quote_swap(&sp.account, &set, Some(&old), &params(true, -1_000_000_000)).unwrap();
    assert_eq!(q0.fees.team, 0);
    assert!(q.amount_out < q0.amount_out);
}

#[test]
fn a_tiny_leg_quotes_no_team_fee() {
    let (sp, fp) = floor_pool(100_020);
    let set = sp.tick_array_set().unwrap();
    let q = quote_swap(&sp.account, &set, Some(&fp), &params(true, -399)).unwrap();
    assert_eq!((q.fees.team, q.floor_after.unwrap().team_owed), (0, 0));
}

/// A pool quote follows the POOL's own `team_fee_bps` — the 0.5 % cap, a lowered 0.10 %, and 0 (a pool created after
/// `set_team_fees(_, 0)`), which quotes no platform fee at all — on the same 1 SOL buy; more fee, fewer tokens.
/// Mutations: the quoter reading a constant (the default 25) instead of the pool's rate; a zero rate that
/// still floors a fee.
#[test]
fn a_floor_quote_follows_the_pools_own_rate_the_cap_a_lower_one_or_zero() {
    let (sp, fp) = floor_pool(100_020);
    let set = sp.tick_array_set().unwrap();
    let mut outs = Vec::new();
    for (bps, fee) in [(floor_quote::types::constants::TEAM_POOL_FEE_BPS_MAX, 5_000_000u64), (10, 1_000_000), (0, 0)] {
        let mut p = fp;
        p.team_fee_bps = bps;
        let q = quote_swap(&sp.account, &set, Some(&p), &params(true, -1_000_000_000)).unwrap();
        assert_eq!((q.fees.team, q.fee_split0[FEE_TEAM], q.floor_after.unwrap().team_owed), (fee, fee, fee), "{bps} bps");
        outs.push(q.amount_out);
    }
    assert!(outs[0] < outs[1] && outs[1] < outs[2], "the dearer the pool's own rate, the fewer tokens out: {outs:?}");
}

/// The `SwapCtx` `quote_swap` builds for a pool, for calling a callback directly.
fn ctx_for(sp: &SynthPool, zfo: bool, amount_specified: i128) -> SwapCtx {
    let p = &sp.account.pool;
    SwapCtx {
        pool_key: sp.account.key,
        zero_for_one: zfo,
        amount_specified,
        pre_tick: p.tick_current,
        supply1: params(zfo, amount_specified).supply1,
        claims0: p.hook_claims0,
        claims1: p.hook_claims1,
        mint0: p.mint0,
        mint1: p.mint1,
        tick_spacing: p.tick_spacing,
        // `fee` is the pool binding's THIRD clause; this helper mirrors `quote_swap`'s own builder (`swap.rs`), so it
        // takes `p.fee`.
        fee: p.fee,
        cushion_top: None,
    }
}

/// ⛔ The record's `fees` is the quoter's half of the chain's wire format, and the hook program's own split must
/// match it — so its ORDER must be pinned by a test that reads it, not by the mock's echo of an
/// arbitrary array. `validate_record` checks only `skim + creator + holder ≤ |delta|` and `platform ≤ skim`, so
/// `[kept, creator, holder, cut]` or a swapped creator / holder would pass everything else. Here: the buy's
/// `before_swap` record against the full quote's breakdown (and its per-currency `fee_split0/1`), and
/// the sell's `after_swap` record, on the unspecified quote leg, against the same `take_fees`.
#[test]
fn the_record_s_fee_split_is_the_breakdown_in_order() {
    // -- the buy: quote-specified exact-in, so before_swap fills fees and delta_specified ----------
    let (sp, fp) = floor_pool(100_020);
    let set = sp.tick_array_set().unwrap();
    let leg: u64 = 1_000_000_000;
    let q = quote_swap(&sp.account, &set, Some(&fp), &params(true, -(leg as i128))).unwrap();
    let mut state = fp;
    let mut taken = FeesTaken::default();
    let rec = floor::before_swap(&mut state, &ctx_for(&sp, true, -(leg as i128)), &mut taken).unwrap();
    assert_eq!(rec.fees, [q.fees.skim, q.fees.creator, q.fees.holder, q.fees.platform_cut, q.fees.team], "[skim, creator, holder, platform, team]");
    assert_eq!(rec.fees, [taken.skim, taken.creator, taken.holder, taken.platform_cut, taken.team]);
    assert_ne!(rec.fees[FEE_SKIM], 0);
    assert_ne!(rec.fees[FEE_CREATOR], 0);
    assert_ne!(rec.fees[FEE_SKIM], rec.fees[FEE_CREATOR], "the two non-zero entries are distinguishable");
    assert_eq!(rec.fees[FEE_HOLDER], 0, "the default state carries a zero holder rate");
    assert_eq!(rec.fees_charged(), Some(rec.delta_specified as u64), "the split explains delta_specified exactly");
    assert_eq!(rec.fee_delta(), Some(rec.delta_specified));
    assert_eq!(rec.delta_unspecified, 0);
    // the platform cut is INSIDE the skim: 2.5 % of the gross skim, never a fourth charge
    assert_eq!(rec.fees[FEE_PLATFORM], rec.fees[FEE_SKIM] * 250 / 10_000);
    assert!(rec.fees[FEE_PLATFORM] < rec.fees[FEE_SKIM]);
    assert_eq!(rec.fees[FEE_SKIM] - rec.fees[FEE_PLATFORM], state.vault_eth, "the vault keeps skim − platform");
    // the quote's per-currency pair is the same split on the specified currency (0), zeros on 1
    assert_eq!((q.fee_split0, q.fee_split1), (rec.fees, [0; 5]));
    assert_eq!(q.fee_split0[FEE_SKIM] + q.fee_split0[FEE_CREATOR] + q.fee_split0[FEE_HOLDER] + q.fee_split0[FEE_TEAM], q.hook_delta.amount0 as u64);
    assert_eq!(q.hook_delta.amount1, 0);
    // and the state the direct call leaves is the state the full quote left (one implementation)
    assert_eq!(bytemuck::bytes_of(&state), bytemuck::bytes_of(&q.floor_after.unwrap()));

    // -- the sell: base-specified exact-in, so the quote leg is UNSPECIFIED and after_swap fills it --
    let (sp, fp) = floor_pool(100_020);
    let sell: i128 = -50_000_000; // 0.05 base tokens (6 dp) for currency 0
    let mut state = fp;
    let mut taken = FeesTaken::default();
    let ctx = ctx_for(&sp, false, sell);
    let rec0 = floor::before_swap(&mut state, &ctx, &mut taken).unwrap();
    assert_eq!(rec0.fees, [0; 5], "before_swap takes nothing when the quote leg is unspecified");
    assert_eq!(rec0.delta_specified, 0);
    // a post-walk input the walk would have produced: the trader paid `sell` base and got `out` quote
    let out: i128 = 700_000_000;
    let post_tick = sp.account.pool.tick_current + 3; // the walk moved a little
    let a = AfterSwapIn {
        post_sqrt_price_x64: tick_math::get_sqrt_price_at_tick(post_tick).unwrap(),
        post_tick,
        swap_delta: BalanceDelta { amount0: out, amount1: sell },
        before_delta_specified: 0,
        before_delta_unspecified: 0,
    };
    let rec1 = floor::after_swap(&mut state, &ctx, &a, &mut taken).unwrap();
    assert_eq!(rec1.fees, [taken.skim, taken.creator, taken.holder, taken.platform_cut, taken.team], "[skim, creator, holder, platform, team]");
    assert_ne!(rec1.fees[FEE_SKIM], 0);
    assert_ne!(rec1.fees[FEE_CREATOR], 0);
    assert_ne!(rec1.fees[FEE_SKIM], rec1.fees[FEE_CREATOR]);
    assert_eq!(rec1.delta_specified, 0);
    assert_eq!(rec1.fees_charged(), Some(rec1.delta_unspecified as u64), "the split explains delta_unspecified exactly");
    assert_eq!(rec1.fee_delta(), Some(rec1.delta_unspecified));
    assert_eq!(rec1.fees[FEE_PLATFORM], rec1.fees[FEE_SKIM] * 250 / 10_000);
    // far from the floor the sell pays the far rates on the quote leg (`out`), as the buy did
    assert_eq!(rec1.fees[FEE_SKIM], (out as u64) * u64::from(fp.skim_sell_far_bps) / 10_000);
    assert_eq!(rec1.fees[FEE_CREATOR], (out as u64) * u64::from(fp.creator_sell_bps) / 10_000);
    assert!(rec1.actions.is_empty() && !rec1.want_callback, "no re-place: the vault is empty");
    // the AMM's mapping puts the sell's split on the UNSPECIFIED currency, 0
    let (f0, f1) = floor_quote::types::hook::HookDeltaAssembly::assemble_fees(sell, false, rec0.fees, rec1.fees);
    assert_eq!((f0, f1), (rec1.fees, [0; 5]));
}

// ---- the in-swap bid re-place --------------------------------------------------------------------------

/// A FLOOR pool with the market at `tick`, the bid BAND `[floor_tick − 120, floor_tick]` deployed above
/// it out of `bid_quote` lamports, third-party liquidity across the whole neighbourhood, and — when
/// `pol_edge_at_dear` — a token-heavy position whose UPPER edge sits exactly on the band's dear edge (as
/// the EVM's `raisePol` leaves the POL, so that tick is initialised). `vault` idle lamports sit in claims.
fn replace_fixture(tick: i32, bid_quote: u64, vault: u64, pol_edge_at_dear: bool) -> (SynthPool, FloorPool) {
    let k = keys(FLOOR_HOOK_ID);
    let sqrt = tick_math::get_sqrt_price_at_tick(tick).unwrap();
    let floor_tick = tick + 300; // 100_320 at 100_020: the band is [100_200, 100_320], the dear edge 180 ticks up
    let (lower, upper) = (floor_tick - 120, floor_tick);
    let bid_liq = floor_quote::mech::position_liquidity(sqrt, lower, upper, bid_quote, 0).unwrap();
    let mut positions = vec![pos(tick - 60 * 200, tick + 60 * 200, 50_000_000_000_000), pos(lower, upper, bid_liq)];
    if pol_edge_at_dear {
        positions.push(pos(lower - 6_000, lower, 20_000_000_000_000));
    }
    let mut sp = build_pool(&k, FLOOR, 60, sqrt, 9, 6, &positions).unwrap();
    sp.account.pool.hook_claims0 = vault;
    let mut fp = floor_pool_defaults(k.pool, k.mint0, k.mint1, [8; 32]);
    fp.ladder_seeded = 1; // a graduated pool: `floor_pool_defaults` is the unseeded post-registration state
    fp.floor_tick = floor_tick;
    fp.vault_eth = vault;
    fp.bid_lower = lower;
    fp.bid_width = upper - lower;
    fp.bid_liquidity.set(bid_liq);
    // what the add charged: rounded up, the quote leg (the band is all quote above the market)
    fp.bid_eth_deployed = floor_quote::amounts_for_liquidity(sqrt, tick, lower, upper, bid_liq as i128).unwrap().0;
    (sp, fp)
}

fn sell_to(limit_tick: i32) -> SwapQuoteParams {
    // a sell is one_for_zero on a quote-is-token0 pool: the tick rises toward the (cheaper) band
    SwapQuoteParams { sqrt_price_limit_x64: tick_math::get_sqrt_price_at_tick(limit_tick).unwrap(), ..params(false, -900_000_000_000_000) }
}

#[test]
fn a_sell_that_stops_short_of_the_band_re_places_nothing() {
    let (sp, fp) = replace_fixture(100_020, 10_000_000_000, 1_000_000_000, true);
    let set = sp.tick_array_set().unwrap();
    let q = quote_swap(&sp.account, &set, Some(&fp), &sell_to(100_140)).unwrap();
    assert_eq!(q.tick_after_trade, 100_140);
    assert!(q.replace_actions.is_empty(), "the bid still holds no tokens: nothing to re-place");
    assert!(q.replace.is_none());
    let f = q.floor_after.unwrap();
    assert_eq!((f.bid_lower, f.bid_width, f.bid_liquidity.get()), (fp.bid_lower, 120, fp.bid_liquidity.get()));
    assert_eq!(f.needs_ratchet, 0);
    // the skim is measured to the DEAR edge (100_200), 60 ticks up: on the ramp, not at the floor
    assert!(q.fees.skim > 0);
    let leg = (q.hook_delta.amount0 as u64) - q.fees.creator - q.fees.team; // skim + holder(0)
    assert_eq!(leg, q.fees.skim);
}

#[test]
fn a_sell_into_the_band_closes_nudges_on_dust_and_re_opens_the_bid_narrowed_from_the_dear_side() {
    let vault = 1_000_000_000u64;
    let (sp, fp) = replace_fixture(100_020, 10_000_000_000, vault, true);
    let set = sp.tick_array_set().unwrap();
    let q = quote_swap(&sp.account, &set, Some(&fp), &sell_to(100_260)).unwrap();
    assert_eq!(q.tick_after_trade, 100_260, "the sell lands inside the band");

    // Round 1: close the band, nudge back down to the dear edge; round 2: re-open.
    use floor_quote::types::hook::HookAction as A;
    assert_eq!(q.replace_actions.len(), 3, "{:?}", q.replace_actions);
    let dear = tick_math::get_sqrt_price_at_tick(100_200).unwrap();
    let bid_eth_left = floor_quote::mech::bid_amounts(q.sqrt_after_trade, 100_200, 120, fp.bid_liquidity.get(), fp.bid_eth_deployed).unwrap().0;
    assert!(bid_eth_left > 0 && bid_eth_left < fp.bid_eth_deployed, "half filled: {bid_eth_left}");
    // `_takeFees` runs before the re-place: the skim this sell just paid is already vault quote when the nudge's
    // budget is sized
    let kept_skim = q.fees.skim - q.fees.platform_cut;
    assert!(kept_skim > 0);
    let idle = vault + kept_skim;
    match q.replace_actions[0] {
        A::ModifyPosition { salt, tick_lower, tick_upper, liquidity_delta } => {
            assert_eq!((salt, tick_lower, tick_upper), (1, 100_200, 100_320));
            assert_eq!(liquidity_delta, -(fp.bid_liquidity.get() as i128));
        }
        a => panic!("round 1[0] is the close, got {a:?}"),
    }
    match q.replace_actions[1] {
        A::Swap { zero_for_one, amount_specified, sqrt_price_limit_x64 } => {
            assert!(zero_for_one, "a buy");
            // the budget is DUST, capped at `min_bid` (the EVM hands the nudge the WHOLE idle vault).
            let budget = (idle + bid_eth_left).min(fp.min_bid);
            assert!(budget < idle + bid_eth_left, "the cap BINDS here: {budget} < {}", idle + bid_eth_left);
            assert_eq!(amount_specified, -(budget as i128), "min(idle + bid_eth_left, min_bid)");
            assert_eq!(sqrt_price_limit_x64, dear, "limited AT the dear edge");
        }
        a => panic!("round 1[1] is the nudge, got {a:?}"),
    }
    let r = q.replace.expect("a re-place happened");
    // On a DUST budget the nudge no longer reaches the dear edge (100_199). It used to,
    // because it had the whole vault to spend getting there.
    assert_eq!((r.from_tick, r.to_tick), (100_260, 100_256), "the nudge stops where the dust ends");
    let b = r.booking;
    assert!(b.absorbed > 0, "the tokens the bid held joined the wall");
    assert_eq!(b.eth_back, bid_eth_left, "the close returns the bid's live quote (principal; fees unknown to a quote)");
    assert!(b.fees_unknown);
    assert!(b.nudge_spent > 0 && b.nudge_got > 0, "third-party liquidity sat in the gap: {b:?}");
    assert!(b.nudge_spent < idle + bid_eth_left);
    assert!(b.redeployed);
    // The dust nudge stops at 100_256, so `_deployBid`'s ceil_to_spacing(current + 1) puts
    // the band back NARROWED from the dear side — [100_260, 100_320], not the whole [100_200,
    // 100_320] it reached when the nudge had the entire vault to spend. A withheld rise, never a
    // lowered floor: `floor_tick` is asserted unmoved below.
    assert_eq!((b.from_lower, b.to_lower, b.to_upper), (100_200, 100_260, 100_320), "narrowed from the dear side");
    match q.replace_actions[2] {
        A::ModifyPosition { salt, tick_lower, tick_upper, liquidity_delta } => {
            assert_eq!((salt, tick_lower, tick_upper), (1, 100_260, 100_320), "narrowed from the dear side");
            assert_eq!(liquidity_delta, b.bid_liquidity as i128);
            assert!(liquidity_delta > 0);
        }
        a => panic!("round 2[0] is the open, got {a:?}"),
    }

    // The hook's post-state, as `after_actions` writes it.
    let f = q.floor_after.unwrap();
    assert_eq!((f.bid_lower, f.bid_width), (100_260, 60), "one spacing wide: the dust nudge did not reach 100_200");
    assert_eq!(f.bid_liquidity.get(), b.bid_liquidity);
    assert_eq!(f.bid_eth_deployed, b.bid_eth_deployed);
    assert_eq!(f.needs_ratchet, 1);
    // ⛔ every pool is a staircase: absorbed and nudged tokens are the vault's bought-back inventory WITH their cost
    // (`vault_bought`).
    assert_eq!(f.wall_held_tokens, fp.wall_held_tokens, "the held reserve is untouched");
    assert_eq!(f.inventory_tokens, fp.inventory_tokens + b.absorbed + b.nudge_got);
    assert!(f.inventory_cost_eth > fp.inventory_cost_eth, "and their cost is kept");
    assert_eq!(f.vault_eth, idle + b.eth_back - b.nudge_spent - b.bid_eth_deployed);
    assert_eq!(f.floor_tick, fp.floor_tick, "the cheap edge never moves in a re-place");

    // The pool's post-state: the price rests at the dear edge, the band's liquidity is above it.
    // The market rests where the DUST ran out, not at the dear edge.
    assert_eq!(q.tick_after, 100_256);
    // The nudge walks the tick DOWN toward the dear edge (100_200), so stopping short leaves it
    // ABOVE: 100_256 > 100_200, and the sqrt with it.
    assert!(q.sqrt_after > dear, "short of the dear edge: it never got down to it");
    // The market rests at 100_256, ABOVE the POL's upper edge at the dear edge 100_200, so the POL
    // is no longer active either — only the wide position is. It used to be both, because the
    // uncapped nudge walked the price all the way down to 100_199, just inside the POL.
    let wide_only = 50_000_000_000_000u128;
    assert_eq!(q.pool_after.liquidity.get(), wide_only, "the new band AND the POL sit below the market: neither active");
    // the open was charged exactly what the hook booked (the DV named in floor.rs), out of claims
    let sell_fee = q.hook_delta.amount0 as u64;
    assert_eq!(q.pool_after.hook_claims0, vault + sell_fee + b.eth_back - b.nudge_spent - b.bid_eth_deployed);
    // ⛔ I-3 through THE ONE SPELLING, never an inline sum: a three-term sum (no `holder_owed`, no `referral_owed`)
    // would be looser than the chain by both and would pass a quoter that promised a swap the hook reverts.
    let owed = floor_math::FloorState::quote_liabilities(bytemuck::cast_ref(&f));
    assert!(q.pool_after.hook_claims0 as u128 >= owed, "I-3 holds after the open");
    // every account the transaction must name: the trader's walk, the close, the nudge, the open
    assert!(q.action_arrays.contains(&97_200), "{:?}", q.action_arrays);
    assert!(q.all_arrays().contains(&97_200));
    // determinism
    let again = quote_swap(&sp.account, &set, Some(&fp), &sell_to(100_260)).unwrap();
    assert_eq!(again.replace_actions, q.replace_actions);
    assert_eq!(bytemuck::bytes_of(&again.pool_after), bytemuck::bytes_of(&q.pool_after));
}

/// v4's tick rule, replayed: a nudge that stops at a limit whose tick is NOT initialised leaves
/// `tick_current` AT the dear edge, and `_deployBid`'s `ceil_to_spacing(current + 1)` then keeps the
/// band one spacing narrower from the dear side. On chain the POL's upper edge (`raisePol`) sits on
/// that tick, so the whole band comes back (the test above); this pins what happens when it does not.
#[test]
fn an_uninitialised_dear_edge_narrows_the_re_placed_band_by_one_spacing() {
    let (sp, fp) = replace_fixture(100_020, 10_000_000_000, 1_000_000_000, false);
    let set = sp.tick_array_set().unwrap();
    let q = quote_swap(&sp.account, &set, Some(&fp), &sell_to(100_260)).unwrap();
    let r = q.replace.expect("a re-place happened");
    // On a dust budget the nudge stops SHORT of the dear edge (100_200), where it used to
    // land exactly on it with the whole vault behind it.
    assert!(r.to_tick > 100_200, "stopped short of the dear edge: {}", r.to_tick);
    assert_eq!(r.to_tick, 100_256);
    assert_eq!((r.booking.to_lower, r.booking.to_upper), (100_260, 100_320));
    let f = q.floor_after.unwrap();
    assert_eq!((f.bid_lower, f.bid_width), (100_260, 60));
}

/// A sell that takes the market THROUGH the band (past the cheap edge) leaves the bid all tokens; the
/// re-place still closes it, and the nudge buys back toward the dear edge with DUST —
/// which, against third-party liquidity in the gap, RUNS OUT short of the edge ("third-party liquidity in the gap
/// stops it"). The band is then entirely behind the market, so `_deployBid`'s last resort places one spacing above
/// the market, under the published floor; `floor_tick` itself never moves.
#[test]
fn a_sell_through_the_band_still_re_places_and_the_nudge_stops_where_the_budget_ends() {
    let vault = 1_000_000_000u64;
    let (sp, fp) = replace_fixture(100_020, 10_000_000_000, vault, true);
    let set = sp.tick_array_set().unwrap();
    let q = quote_swap(&sp.account, &set, Some(&fp), &sell_to(100_500)).unwrap();
    assert_eq!(q.tick_after_trade, 100_500);
    let r = q.replace.expect("a re-place happened");
    assert_eq!(r.booking.eth_back, 0, "fully crossed: the close returns no quote");
    assert!(r.booking.absorbed > 0);
    let idle = vault + q.fees.skim - q.fees.platform_cut;
    // DUST into the gap, never the whole idle vault.
    assert_eq!(r.booking.nudge_spent, idle.min(fp.min_bid), "the budget is capped at min_bid");
    assert!(r.booking.nudge_spent < idle, "the cap BINDS: one sell no longer empties the vault");
    assert!(q.tick_after > 100_200 && q.tick_after < 100_500, "stopped short of the edge: {}", q.tick_after);
    assert_eq!(q.tick_after, r.to_tick);
    let f = q.floor_after.unwrap();
    assert_eq!(f.floor_tick, fp.floor_tick, "the cheap edge is a promise; it does not move");
    // ⛔ a staircase pool keeps the absorbed tokens' basis (`vault_bought`).
    assert!(f.inventory_cost_eth > fp.inventory_cost_eth, "the basis is kept");
    assert!(f.inventory_tokens > fp.inventory_tokens, "the absorbed tokens are the vault's inventory");
    // ⛔ THIS IS WHAT THE CAP BOUGHT. Uncapped, the nudge spent the whole idle vault into the
    // gap, `vault_eth` landed at 0, `plan_deploy` returned None and the pool published a floor with
    // NO BID BEHIND IT — the harm the dust budget exists to prevent. On dust the vault keeps what the nudge
    // did not spend, and the bid comes back.
    assert!(r.booking.redeployed, "the bid is re-placed: the vault was not emptied");
    // The quote the nudge did not spend is not lost — it is DEPLOYED as the new bid. That is the
    // identity to assert; `vault_eth` itself is 0 here because all of it went into the band.
    assert_eq!(f.vault_eth + f.bid_eth_deployed, idle - r.booking.nudge_spent, "nothing leaks");
    assert_eq!(q.replace_actions.len(), 3, "close, nudge, open: {:?}", q.replace_actions);
    assert!(f.bid_liquidity.get() > 0 && f.bid_eth_deployed > 0, "a real bid stands behind the floor");
}

/// ⛔ **THE CAP DID NOT OPEN A HOLE — the POL's upper edge and the bid's lower edge still MEET.**
/// `raise_pol` leaves `seed_upper = dear_edge` (`positions.rs:667`) and `plan_deploy` places the bid
/// at `max(dear_edge, ceil_to_spacing(current + 1))` (the program's re-place planner), so the two are flush
/// whenever the nudge reaches the dear edge — and on a gap that is EMPTY it always does, because
/// crossing empty ticks costs no input at all. The dust budget therefore changes nothing here:
/// `nudge_spent == 0` with `min_bid` to spend, exactly as it was with the whole vault.
///
/// The bid narrows from the dear side ONLY when a third party has parked liquidity in the band —
/// and in that case what sits in the narrowed range is THEIR liquidity, so there is still no price
/// at which nobody can trade. (If they then withdraw it, the range is empty until the next ratchet
/// re-solves; a swap traverses an empty range instantly without filling, it is not refused.)
#[test]
fn an_empty_gap_costs_the_dust_nudge_nothing_and_the_bid_comes_back_flush_with_the_pol() {
    // replace_fixture's geometry with NO third-party wide position: protocol liquidity only.
    let k = keys(FLOOR_HOOK_ID);
    let tick = 100_020;
    let sqrt = tick_math::get_sqrt_price_at_tick(tick).unwrap();
    let floor_tick = tick + 300;
    let (lower, upper) = (floor_tick - 120, floor_tick);
    let (bid_quote, vault) = (10_000_000_000u64, 1_000_000_000u64);
    let bid_liq = floor_quote::mech::position_liquidity(sqrt, lower, upper, bid_quote, 0).unwrap();
    let pol_liq = 20_000_000_000_000u128;
    let positions = vec![pos(lower, upper, bid_liq), pos(lower - 6_000, lower, pol_liq)];
    let mut sp = build_pool(&k, FLOOR, 60, sqrt, 9, 6, &positions).unwrap();
    sp.account.pool.hook_claims0 = vault;
    let mut fp = floor_pool_defaults(k.pool, k.mint0, k.mint1, [8; 32]);
    fp.ladder_seeded = 1; // a graduated pool: `floor_pool_defaults` is the unseeded post-registration state
    fp.floor_tick = floor_tick;
    fp.vault_eth = vault;
    fp.bid_lower = lower;
    fp.bid_width = upper - lower;
    fp.bid_liquidity.set(bid_liq);
    fp.bid_eth_deployed = floor_quote::amounts_for_liquidity(sqrt, tick, lower, upper, bid_liq as i128).unwrap().0;

    let set = sp.tick_array_set().unwrap();
    let q = quote_swap(&sp.account, &set, Some(&fp), &sell_to(100_260)).unwrap();
    let r = q.replace.expect("a re-place happened");
    let f = q.floor_after.unwrap();
    let dear = fp.floor_tick - fp.bid_width;

    assert_eq!(r.booking.nudge_spent, 0, "an EMPTY gap is crossed on zero input, whatever the budget");
    assert_eq!(r.to_tick, dear - 1, "the nudge still reaches the dear edge (v4: next − 1)");
    assert_eq!(f.bid_lower, dear, "FLUSH: the bid's lower edge IS the POL's upper edge — no hole");
    assert_eq!(f.bid_width, 120, "and the whole band came back");
    assert_eq!(q.pool_after.liquidity.get(), pol_liq, "the POL is active at the resting price");
    assert_eq!(f.floor_tick, fp.floor_tick, "the published floor never moves in a re-place");
}

// ---- the Carpenter Platform fee on a FLOOR pool -------------------------------------------------
//
// Every swap pays it, FLOOR pools included, on the SOL / quote side. The plain-pool cases are
// `carpenter-quote`'s; this is the FLOOR one.

mod carpenter_platform_fee {
    use super::*;
    use floor_quote::types::constants::{PROTOCOL_FEE_DEFAULT_PACKED, PROTOCOL_FEE_ON_CURRENCY0};
    use floor_quote::types::protocol_fee::output_share;

    /// On a FLOOR pool (quote = currency 0) a SELL pays the fee off its quote output — so the hook's
    /// skim is taken on the net output — and the in-swap NUDGE, the backing's own buy, pays it too
    /// ("Yes, every swap pays"): with `lp_fee` 0 the whole step fee is Carpenter's. The owed on the
    /// pool is the trader's share PLUS the nudge's; the hook's books still close exactly, because it
    /// books the nudge from the measured (gross) spend.
    #[test]
    fn a_floor_sell_pays_on_the_quote_and_the_nudge_pays_out_of_the_backing() {
        let vault = 1_000_000_000u64;
        let (sp, fp) = replace_fixture(100_020, 10_000_000_000, vault, true);
        let sp = sp.with_protocol_fee(PROTOCOL_FEE_DEFAULT_PACKED, PROTOCOL_FEE_ON_CURRENCY0);
        let set = sp.tick_array_set().unwrap();
        let q = quote_swap(&sp.account, &set, Some(&fp), &sell_to(100_260)).unwrap();
        let r = q.replace.expect("a re-place happened");
        let b = r.booking;
        assert!(b.nudge_spent > 0, "the nudge actually bought");

        // the trader's fee: currency 0, off the gross quote output, before the hook saw it
        assert_eq!(q.fees.protocol_currency, 0);
        let net_out = q.swap_delta.amount0 as u64;
        assert_eq!(q.fees.protocol, output_share(net_out + q.fees.protocol, 500), "floor(gross · 500 / 1e6)");
        assert!(q.fees.protocol > 0);

        // the nudge's fee: the rest of what the pool owes, all in currency 0, ~500 pips of its spend
        let owed0 = q.pool_after.protocol_fees_owed0;
        assert_eq!(q.pool_after.protocol_fees_owed1, 0);
        let nudge_fee = owed0 - q.fees.protocol;
        let floor_share = (b.nudge_spent as u128 * 500 / 1_000_000) as u64;
        assert!(nudge_fee >= floor_share && nudge_fee <= floor_share + 3, "nudge fee {nudge_fee} vs 500 pips of {}", b.nudge_spent);
        assert!(nudge_fee > 0, "the vault's own buy paid");

        // the hook's books close exactly: `vault_eth −= spent` with the fee inside `spent`
        let idle = vault + q.fees.skim - q.fees.platform_cut;
        let f = q.floor_after.unwrap();
        assert_eq!(f.vault_eth, idle + b.eth_back - b.nudge_spent - b.bid_eth_deployed);
        assert!(f.inventory_cost_eth >= fp.inventory_cost_eth + b.nudge_spent, "the basis is kept, the nudge's spend inside it");
        let sell_fee = q.hook_delta.amount0 as u64;
        assert_eq!(q.pool_after.hook_claims0, vault + sell_fee + b.eth_back - b.nudge_spent - b.bid_eth_deployed);
        let owed = floor_math::FloorState::quote_liabilities(bytemuck::cast_ref(&f));
        assert!(q.pool_after.hook_claims0 as u128 >= owed, "I-3 holds");
    }
}
