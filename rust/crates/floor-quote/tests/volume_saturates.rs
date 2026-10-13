//! Regression: the quoter saturates `cumulative_volume_eth` exactly as the program does.
//! A "phantom" exact-in buy of u64::MAX with a price limit one unit away trades ~2 units, pays no fee on a
//! fee-free buy side, and pins the chain's counter at u64::MAX; the quoter must keep answering afterwards.
use floor_quote::clmm::tick_math;
use floor_quote::synth::{build_pool, floor_pool_defaults, PoolKeys, SynthPosition, FLOOR};
use floor_quote::types::ids::FLOOR_HOOK_ID;
use floor_quote::{no_limit, quote_swap, SwapQuoteParams};

fn keys() -> PoolKeys {
    PoolKeys { pool: [7; 32], mint0: [1; 32], mint1: [2; 32], vault0: [3; 32], vault1: [4; 32],
        token_program0: [5; 32], token_program1: [6; 32], hook: FLOOR_HOOK_ID, hook_authority: [9; 32] }
}

#[test]
fn a_pinned_volume_counter_never_stops_the_quoter() {
    let tick = 100_020;
    let k = keys();
    let sqrt = tick_math::get_sqrt_price_at_tick(tick).unwrap();
    let sp = build_pool(&k, FLOOR, 60, sqrt, 9, 6,
        &[SynthPosition { tick_lower: tick - 12_000, tick_upper: tick + 12_000, liquidity: 50_000_000_000_000 }]).unwrap();
    let set = sp.tick_array_set().unwrap();
    let mut fp = floor_pool_defaults(k.pool, k.mint0, k.mint1, [8; 32]);
    fp.ladder_seeded = 1; // a graduated pool: `floor_pool_defaults` is the unseeded post-registration state
    fp.floor_tick = tick + 60;      // market inside the band: buy skim = skim_buy_at_floor = 0
    fp.skim_buy_far_bps = 250;      // a pool that DOES skim buys far from the floor
    fp.creator_buy_bps = 0;         // Custom: no creator / holder rate on buys
    fp.holder_buy_bps = 0;
    // The team fee: `floor_pool_defaults` hands the pool default (25 bps), charged on the specified quote leg like
    // every hook fee on an exact-in buy — so a u64::MAX phantom owes a fee no wallet can pay, on chain and in the
    // quote. The fee-free buy side this regression needs is a pool created at a 0 rate.
    fp.team_fee_bps = 0;
    fp.cumulative_volume_eth = 1_000;

    // The phantom buy: exact-in u64::MAX, limit one unit away.
    let p = SwapQuoteParams { zero_for_one: true, amount_specified: -(u64::MAX as i128), sqrt_price_limit_x64: sqrt - 1,
        supply0: 0, cushion_top: None, supply1: 1_000_000_000_000_000 };
    let q = quote_swap(&sp.account, &set, Some(&fp), &p).expect("the chain accepts the phantom buy; so must the quote");
    assert!(q.amount_in <= 2, "almost nothing trades");
    assert_eq!(q.fees.hook_total(), 0, "no fee on the nominal amount");
    assert_eq!(q.floor_after.map(|f| f.cumulative_volume_eth), Some(u64::MAX), "saturated, as on chain");

    fp.cumulative_volume_eth = u64::MAX; // what the chain holds afterwards
    let buy = SwapQuoteParams { zero_for_one: true, amount_specified: -1_000_000_000, sqrt_price_limit_x64: no_limit(true),
        supply0: 0, cushion_top: None, supply1: 1_000_000_000_000_000 };
    let sell = SwapQuoteParams { zero_for_one: false, sqrt_price_limit_x64: no_limit(false), ..buy };
    let (b, s) = (quote_swap(&sp.account, &set, Some(&fp), &buy), quote_swap(&sp.account, &set, Some(&fp), &sell));
    assert!(b.unwrap().amount_out > 0, "a buy still quotes");
    assert!(s.unwrap().amount_out > 0, "a sell still quotes");
}
