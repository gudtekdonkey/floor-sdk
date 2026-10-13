//! THE NET-FLOW METER's CROSS-CHAIN VECTORS (the EVM twin pins the SAME file). This test writes the
//! transcript of `floor_math::netflow` — the meter's update (`_noteFlow`), the cap (`_flowCap`), `share_of_mcap`,
//! `mul_div_trunc` and the climb hold's line (`_climbLine`, with every intermediate) — and compares it with
//! `fixtures/netflow/netflow-vectors.json`. A TypeScript port can replay the note-flow and cap halves against the same
//! file.
//!
//! Regenerate after a DELIBERATE change to the arithmetic: `FLOOR_WRITE_NETFLOW_VECTORS=1 cargo test -p floor-math --test
//! netflow_vectors`, commit the JSON with the change, and tell the EVM side: its harness pins this file byte for byte.
//!
//! Encoding: every integer that can pass 2^53 (`u64`, `u128`, `i128`) is a decimal STRING; ticks (`i32`) and bps are JSON
//! numbers; an absent value (`None`) is `null`. Ticks are Solana's raw ticks, which on a FLOOR pool are the EVM's
//! NORMALISED ticks (the quote is token0): a DEARER price is a LOWER tick.

use floor_math::netflow::{slice_tick, climb_hold_target, climb_line_parts, climb_line_parts_reach, flow_cap, mul_div_trunc, note_flow_with, share_of_mcap, NetFlow};
use floor_math::{FloorState, PodU128};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/netflow/netflow-vectors.json");

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        ((self.next() << 31) | self.next()) % n
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
}

fn meter(f: NetFlow) -> String {
    format!("{{\"a\":\"{}\",\"b\":\"{}\"}}", f.a, f.b)
}

fn opt_i32(x: Option<i32>) -> String {
    x.map_or("null".into(), |v| v.to_string())
}

fn note(f: NetFlow, buy: bool, v: u64, pre: i32, post: i32, mcap: u128, h: u64, k: u64) -> String {
    let out = note_flow_with(f, buy, v, pre, post, mcap, h, k);
    format!(
        "{{\"in\":{},\"buy\":{},\"v\":\"{}\",\"pre\":{},\"post\":{},\"mcap\":\"{}\",\"h_bps\":{},\"k_bps\":{},\"out\":{}}}",
        meter(f),
        buy,
        v,
        pre,
        post,
        mcap,
        h,
        k,
        meter(out)
    )
}

/// The hand-worked cases (`netflow::tests::vectors_note_flow`), then seeded swap sequences that carry the meter from one
/// swap to the next — at H 130 / K 0 (the setting under consideration), at K 500, at a tiny H (the scaling branch every swap), across
/// positive and negative ticks.
fn note_flow_cases() -> Vec<String> {
    const MCAP: u128 = 1_000_000_000_000;
    let m = |a: i128, b: u64| NetFlow { a, b };
    let mut out = vec![
        note(NetFlow::default(), true, 1_000, -1_000, -1_100, MCAP, 130, 0),
        note(NetFlow::default(), true, 10, -1_001, -1_000, MCAP, 130, 0),
        note(NetFlow::default(), true, 10, 7, 8, MCAP, 130, 0),
        note(m(-1_050_000, 1_000), true, 3_000, -1_150, -1_250, MCAP, 130, 0),
        note(m(-4_650_000, 4_000), false, 1_000, -1_100, -1_050, MCAP, 130, 0),
        note(m(-4_650_000, 4_000), false, 1_000, -1_300, -1_250, MCAP, 130, 0),
        note(m(-4_650_000, 4_000), false, 4_000, -1_300, -1_000, MCAP, 130, 0),
        note(NetFlow::default(), false, 4_000, -1_300, -1_000, MCAP, 130, 0),
        note(m(-4_650_000, 4_000), true, 13_000_000_001, -2_000, -2_100, MCAP, 130, 0),
        note(m(-12_000_000_000 * 1_000 - 7, 12_000_000_000), true, 2_000_000_000, -1_400, -1_600, MCAP, 130, 0),
        note(m(-4_650_000, 4_000), false, 1_000, -1_100, -1_050, MCAP, 130, 500),
        note(m(-4_650_000, 4_000), true, 0, -1, -2, MCAP, 130, 500),
        // a positive meter scaled down: a·(h−v)/b truncates toward zero on the positive side too
        note(m(12_000_000_000 * 1_000 + 7, 12_000_000_000), true, 2_000_000_000, 1_400, 1_600, MCAP, 130, 0),
        // the catch-up on a buy
        note(m(-4_650_000, 4_000), true, 1_000, -2_000, -2_200, MCAP, 130, 500),
        // a sell starting exactly at the mean
        note(m(-4_650_000, 4_000), false, 1_000, -1_162, -1_100, MCAP, 130, 0),
        // ⛔ THE HALF RULE AND THE LAST SLICE: a 4·H buy into an empty meter (its last H); a 2·H buy into a
        // full meter (scaled to ⌊h/2⌋, then w at its slice); a buy between H/2 and H into a holding meter; a buy of exactly
        // ⌊h/2⌋ + 1 (2v > h); one of ⌊h/2⌋ (2v ≤ h: the old path); a positive-tick half
        note(NetFlow::default(), true, 52_000_000_004, -2_000, -3_000, MCAP, 130, 0),
        note(m(-13_000_000_001_000, 13_000_000_001), true, 26_000_000_002, -2_000, -3_000, MCAP, 130, 0),
        note(m(-4_650_000, 4_000), true, 7_000_000_000, -1_000, -1_100, MCAP, 130, 0),
        note(m(-4_650_000, 4_000), true, 6_500_000_001, -1_000, -1_100, MCAP, 130, 0),
        note(m(-4_650_000, 4_000), true, 6_500_000_000, -1_000, -1_100, MCAP, 130, 0),
        note(m(9_000_000, 4_000), true, 20_000_000_000, 3_000, 2_001, MCAP, 130, 0),
        note(m(-4_650_000, 4_000), true, 20_000_000_000, -1_000, -1_100, MCAP, 130, 500),
    ];
    let seqs: &[(u64, u64, u64, u128)] = &[(1, 130, 0, MCAP), (2, 130, 0, 37_000_000_000_000), (3, 130, 500, MCAP), (4, 1, 0, 50_000_000), (5, 130, 0, 3_000_000_000_000_000), (6, 130, 0, MCAP), (7, 130, 500, 37_000_000_000_000)];
    for &(seed, h, k, mcap0) in seqs {
        let mut r = Lcg(seed);
        let mut f = NetFlow::default();
        let mut tick = r.range(-90_000, 90_000) as i32;
        let hq = share_of_mcap(mcap0, h);
        for _ in 0..40 {
            let buy = r.below(3) != 0;
            // seeds 6 and 7: buys up to 3·H, so the half rule and the last slice fire along the way
            let v = if seed >= 6 { 1 + r.below((hq * 3).max(2)) } else { 1 + r.below((hq / 6).max(2)) };
            let step = r.range(1, 400) as i32;
            let post = if buy { tick - step } else { tick + step };
            let mcap = mcap0 + r.below(1_000_000) as u128;
            out.push(note(f, buy, v, tick, post, mcap, h, k));
            f = note_flow_with(f, buy, v, tick, post, mcap, h, k);
            tick = post;
        }
    }
    out
}

/// `slice_tick(pre, post, w, v)`: the hand-worked ends, truncation toward zero on both sides, `w >= v`, `v = 0`.
fn slice_cases() -> Vec<String> {
    let mut cases: Vec<(i32, i32, u64, u64)> = vec![
        (-2_000, -3_000, 13_000_000_001, 52_000_000_004),
        (-1_000, -1_100, 6_500_000_001, 7_000_000_000),
        (3_000, 2_001, 1, 3),
        (-1, -2, 1, 3),
        (100, 0, 5, 5),
        (100, 0, 9, 5),
        (100, 0, 0, 5),
        (100, 0, 5, 0),
        (887_272, -887_272, u64::MAX, u64::MAX),
        (0, 0, 7, 9),
    ];
    let mut r = Lcg(91);
    for _ in 0..30 {
        let pre = r.range(-200_000, 200_000) as i32;
        let post = pre - r.range(0, 40_000) as i32;
        let v = 1 + r.below(1_000_000_000_000);
        cases.push((pre, post, r.below(v + 1), v));
    }
    cases
        .into_iter()
        .map(|(pre, post, w, v)| {
            format!("{{\"pre\":{},\"post\":{},\"w\":\"{}\",\"v\":\"{}\",\"out\":{}}}", pre, post, w, v, slice_tick(pre, post, w, v).map_or("null".into(), |x| format!("\"{x}\"")))
        })
        .collect()
}

fn flow_cap_cases() -> Vec<String> {
    let m = |a: i128, b: u64| NetFlow { a, b };
    let mut cases: Vec<(i32, Option<i32>, NetFlow)> = vec![
        (-2_000, None, m(-5_000_000, 1_000)),
        (-2_000, Some(-1_500), NetFlow::default()),
        (-1_000, Some(-1_500), NetFlow::default()),
        (-2_000, Some(-1_500), m(-1_750_500, 1_000)),
        (-2_000, Some(-1_500), m(-1_750_000, 1_000)),
        (1_000, Some(2_000), m(1_750_500, 1_000)),
        (-2_000, Some(-1_500), m(-2_500_000, 1_000)),
        (-2_000, Some(-1_500), m(-1_000_000, 1_000)),
        (-500, Some(-1_500), m(-1_000_000, 1_000)),
        (-1_502, Some(-1_500), m(-1_501_000, 1_000)),
        (0, Some(1), m(-1, 3)),
        (0, Some(1), m(1, 3)),
    ];
    let mut r = Lcg(77);
    for _ in 0..40 {
        let lower = r.range(-60_000, 60_000) as i32;
        let top = lower - r.range(-3_000, 6_000) as i32;
        let b = 1 + r.below(1_000_000_000);
        let mean = (lower as i64 - r.range(-3_000, 6_000)) as i128;
        let a = mean * b as i128 + r.range(-(b as i64 - 1).min(1_000_000), (b as i64 - 1).min(1_000_000)) as i128;
        let f = if r.below(8) == 0 { NetFlow::default() } else { m(a, b) };
        cases.push((top, if r.below(10) == 0 { None } else { Some(lower) }, f));
    }
    cases
        .into_iter()
        .map(|(top, lower, f)| format!("{{\"top\":{},\"standing_lower\":{},\"meter\":{},\"out\":{}}}", top, opt_i32(lower), meter(f), flow_cap(top, lower, f)))
        .collect()
}

fn small_cases() -> (Vec<String>, Vec<String>) {
    let shares: Vec<String> = [(0u128, 130u64), (1, 130), (1_000_000_000_000, 130), (76_923_076, 130), (76_923_077, 130), (u128::MAX, 10_000), (1_000_000_000_000_000_000_000, 130)]
        .iter()
        .map(|&(mcap, bps)| format!("{{\"mcap\":\"{}\",\"bps\":{},\"out\":\"{}\"}}", mcap, bps, share_of_mcap(mcap, bps)))
        .collect();
    let mdt: Vec<String> = [(-7i128, 3u128, 2u128), (7, 3, 2), (-1, 1, 3), (i128::MAX, 2, 3), (i128::MIN + 1, 1, 1), (-12_000_000_000_007, 11_000_000_001, 12_000_000_000), (5, 1, 0), (i128::MAX, 3, 2)]
        .iter()
        .map(|&(x, m, d)| {
            let o = mul_div_trunc(x, m, d).map_or("null".into(), |v| format!("\"{v}\""));
            format!("{{\"x\":\"{}\",\"m\":\"{}\",\"d\":\"{}\",\"out\":{}}}", x, m, d, o)
        })
        .collect();
    (shares, mdt)
}

/// A FloorState carrying exactly the fields the solve reads, every other byte zero.
fn state(spacing: i32, floor: i32, seed: (i32, i32, u128), inventory: u64, ladder: u64, min_sellable: u64) -> FloorState {
    let mut s: FloorState = bytemuck::Zeroable::zeroed();
    s.tick_spacing = spacing;
    s.floor_tick = floor;
    s.seed_lower = seed.0;
    s.seed_upper = seed.1;
    s.seed_liq = PodU128::new(seed.2);
    s.inventory_tokens = inventory;
    s.ladder_tokens_remaining = ladder;
    s.min_sellable = min_sellable;
    s
}

fn sqrt_at(t: i32) -> u128 {
    carpenter_clmm_math::tick_math::get_sqrt_price_at_tick(t).unwrap()
}

fn climb_cases() -> Vec<String> {
    const SUPPLY: u64 = 1_000_000_000_000_000;
    const W: i32 = 1_080; // the presets' cushion width at spacing 60 (ceil(1,054))
    // (label, market tick, extra on the sqrt price, state, vault_q, cushion)
    let base = state(60, 60_000, (-443_580, 58_200, 2_000_000_000_000_000), 0, 690_000_000_000_000, 1_000);
    let mut cases: Vec<(&str, i32, u128, FloorState, u64, Option<(i32, i32, u128)>, i32)> = vec![
        ("no cushion standing", 40_000, 0, base, 50_000_000_000, None, W),
        ("held under the spot: the cushion two widths cheaper", 40_000, 0, base, 50_000_000_000, Some((43_200, 44_280, 3_000_000_000_000_000)), W),
        ("held, the price between grid ticks", 40_017, 12_345_678, base, 50_000_000_000, Some((43_200, 44_280, 3_000_000_000_000_000)), W),
        ("already at the spot's range", 40_000, 0, base, 50_000_000_000, Some((40_020, 41_100, 3_000_000_000_000_000)), W),
        ("dearer than the spot's range", 40_000, 0, base, 50_000_000_000, Some((38_000, 39_080, 3_000_000_000_000_000)), W),
        ("an empty vault holds the floor", 40_000, 0, base, 0, Some((43_200, 44_280, 3_000_000_000_000_000)), W),
        ("the spot inside the band: no range", 59_900, 0, base, 50_000_000_000, Some((59_940, 60_000, 1)), W),
        ("a cushion with no liquidity holds no quote", 40_000, 0, base, 50_000_000_000, Some((43_200, 44_280, 0)), W),
        ("$FLOOR's 0.7x width", 30_000, 0, state(60, 60_000, (-443_580, 58_200, 2_000_000_000_000_000), 0, 460_000_000_000_000, 1_000), 80_000_000_000, Some((40_000, 43_600, 9_000_000_000_000_000)), 3_600),
        ("negative ticks", -20_000, 0, state(60, 0, (-443_580, -1_800, 2_000_000_000_000_000), 3_000_000_000_000, 500_000_000_000_000, 1_000), 5_000_000_000_000, Some((-16_000, -14_920, 70_000_000_000_000)), W),
        ("spacing 10", 40_005, 0, state(10, 60_000, (-443_630, 58_200, 2_000_000_000_000_000), 0, 690_000_000_000_000, 1_000), 50_000_000_000, Some((43_200, 44_260, 3_000_000_000_000_000)), 1_060),
        ("too little sellable: the dear edge of the standing floor", 40_000, 0, state(60, 60_000, (-443_580, 58_200, 2_000_000_000_000_000), 0, 999_999_000_000_000, 10_000_000_000_000), 50_000_000_000, Some((43_200, 44_280, 3_000_000_000_000_000)), W),
    ];
    let mut r = Lcg(4242);
    for i in 0..12 {
        let floor = 60 * r.range(500, 1_500) as i32;
        let spot = floor - 60 * r.range(40, 400) as i32 - r.range(0, 59) as i32;
        let lower = (spot - spot.rem_euclid(60)) + 60 * r.range(-5, 60) as i32;
        let liq = 1 + r.below(400_000_000_000_000) as u128;
        let s = state(60, floor, (-443_580, floor - 1_800, r.below(50_000_000_000_000) as u128), r.below(1_000_000_000_000), r.below(800_000_000_000_000), 1_000);
        let vq = r.below(50_000_000_000);
        cases.push((["seeded 0", "seeded 1", "seeded 2", "seeded 3", "seeded 4", "seeded 5", "seeded 6", "seeded 7", "seeded 8", "seeded 9", "seeded 10", "seeded 11"][i], spot, r.below(1_000_000) as u128, s, vq, Some((lower, lower + W, liq)), W));
    }
    let _ = &mut cases;
    cases
        .into_iter()
        .map(|(label, tick, extra, s, vq, cushion, width)| {
            let sqrt = sqrt_at(tick) + extra;
            let c = climb_line_parts(sqrt, tick, &s, SUPPLY, vq, cushion, width).expect("in range");
            let cush = cushion.map_or("null".into(), |(l, u, q)| format!("{{\"lower\":{},\"upper\":{},\"liquidity\":\"{}\"}}", l, u, q));
            let range = c.spot_range.map_or("null".into(), |(l, u)| format!("[{},{}]", l, u));
            format!(
                "{{\"label\":\"{}\",\"sqrt_price_x64\":\"{}\",\"tick\":{},\"supply\":\"{}\",\"vault_q\":\"{}\",\"width\":{},\"cushion\":{},\
\"state\":{{\"tick_spacing\":{},\"floor_tick\":{},\"seed_lower\":{},\"seed_upper\":{},\"seed_liq\":\"{}\",\"inventory_tokens\":\"{}\",\
\"ladder_tokens_remaining\":\"{}\",\"wall_held_tokens\":\"0\",\"min_sellable\":\"{}\"}},\
\"out\":{{\"spot_range\":{},\"cushion_quote\":\"{}\",\"liquidity\":\"{}\",\"solved\":{},\"line\":{}}}}}",
                label, sqrt, tick, SUPPLY, vq, width, cush, s.tick_spacing, s.floor_tick, s.seed_lower, s.seed_upper, s.seed_liq.get(),
                s.inventory_tokens, s.ladder_tokens_remaining, s.min_sellable, range, c.cushion_quote, c.liquidity, opt_i32(c.solved), opt_i32(c.line)
            )
        })
        .collect()
}

/// THE REACH (`FloorPolicy.STAIR_CLIMB_HOLD_REACH_TICKS`): the hold's line at an explicit reach, with
/// the clamped target `to` recorded. A separate array so `climb_line`'s rows (the shipped reach, 0) stay as they were.
fn climb_reach_cases() -> Vec<String> {
    const SUPPLY: u64 = 1_000_000_000_000_000;
    const W: i32 = 1_080;
    let base = state(60, 60_000, (-443_580, 58_200, 2_000_000_000_000_000), 0, 690_000_000_000_000, 1_000);
    let held = Some((43_200, 44_280, 3_000_000_000_000_000u128));
    let mut cases: Vec<(String, i32, u128, FloorState, u64, Option<(i32, i32, u128)>, i32, i32)> = Vec::new();
    for reach in [0, 100, 296, 488, 954, 3_200, 10_000] {
        cases.push((format!("held under the spot, reach {reach}"), 40_000, 0, base, 50_000_000_000, held, W, reach));
    }
    // the reach moves the line: the full hold 60,180, the reach of 296 ticks 60,120 (a floor one spacing dearer allowed)
    let moves = state(60, 30_000, (-443_580, 28_200, 20_000_000_000_000), 0, 100_000_000_000_000, 1_000);
    for reach in [0, 296] {
        cases.push((format!("the reach moves the line, reach {reach}"), 27_000, 0, moves, 2_000_000_000_000, Some((29_400, 30_480, 50_000_000_000_000)), W, reach));
    }
    cases.push(("no cushion standing, reach 954".into(), 40_000, 0, base, 50_000_000_000, None, W, 954));
    cases.push(("a cushion with no liquidity, reach 954".into(), 40_000, 0, base, 50_000_000_000, Some((43_200, 44_280, 0)), W, 954));
    cases.push(("already at the spot's range, reach 954".into(), 40_000, 0, base, 50_000_000_000, Some((40_020, 41_100, 3_000_000_000_000_000)), W, 954));
    cases.push(("negative ticks, reach 296".into(), -20_000, 0, state(60, 0, (-443_580, -1_800, 2_000_000_000_000_000), 3_000_000_000_000, 500_000_000_000_000, 1_000), 5_000_000_000_000, Some((-16_000, -14_920, 70_000_000_000_000)), W, 296));
    let mut r = Lcg(9191);
    for i in 0..12 {
        let floor = 60 * r.range(500, 1_500) as i32;
        let spot = floor - 60 * r.range(40, 400) as i32 - r.range(0, 59) as i32;
        let lower = (spot - spot.rem_euclid(60)) + 60 * r.range(-5, 60) as i32;
        let liq = 1 + r.below(400_000_000_000_000) as u128;
        let s = state(60, floor, (-443_580, floor - 1_800, r.below(50_000_000_000_000) as u128), r.below(1_000_000_000_000), r.below(800_000_000_000_000), 1_000);
        let vq = r.below(50_000_000_000);
        let reach = [100, 296, 488, 954][i % 4];
        cases.push((format!("seeded {i}, reach {reach}"), spot, r.below(1_000_000) as u128, s, vq, Some((lower, lower + W, liq)), W, reach));
    }
    cases
        .into_iter()
        .map(|(label, tick, extra, s, vq, cushion, width, reach)| {
            let sqrt = sqrt_at(tick) + extra;
            let c = climb_line_parts_reach(sqrt, tick, &s, SUPPLY, vq, cushion, width, reach).expect("in range");
            let to = climb_hold_target(tick, cushion, reach);
            let cush = cushion.map_or("null".into(), |(l, u, q)| format!("{{\"lower\":{},\"upper\":{},\"liquidity\":\"{}\"}}", l, u, q));
            let range = c.spot_range.map_or("null".into(), |(l, u)| format!("[{},{}]", l, u));
            format!(
                "{{\"label\":\"{}\",\"reach\":{},\"sqrt_price_x64\":\"{}\",\"tick\":{},\"supply\":\"{}\",\"vault_q\":\"{}\",\"width\":{},\"cushion\":{},\
\"state\":{{\"tick_spacing\":{},\"floor_tick\":{},\"seed_lower\":{},\"seed_upper\":{},\"seed_liq\":\"{}\",\"inventory_tokens\":\"{}\",\
\"ladder_tokens_remaining\":\"{}\",\"wall_held_tokens\":\"0\",\"min_sellable\":\"{}\"}},\
\"out\":{{\"to\":{},\"spot_range\":{},\"cushion_quote\":\"{}\",\"liquidity\":\"{}\",\"solved\":{},\"line\":{}}}}}",
                label, reach, sqrt, tick, SUPPLY, vq, width, cush, s.tick_spacing, s.floor_tick, s.seed_lower, s.seed_upper, s.seed_liq.get(),
                s.inventory_tokens, s.ladder_tokens_remaining, s.min_sellable, to, range, c.cushion_quote, c.liquidity, opt_i32(c.solved), opt_i32(c.line)
            )
        })
        .collect()
}

fn transcript() -> String {
    let (shares, mdt) = small_cases();
    let j = |v: Vec<String>| v.join(",\n");
    format!(
        "{{\"schema\":\"floor-netflow/v1\",\"setting_under_consideration\":{{\"h_bps\":130,\"k_bps\":0,\"half_rule\":true}},\n\"share_of_mcap\":[\n{}\n],\n\"slice_tick\":[\n{}\n],\n\"mul_div_trunc\":[\n{}\n],\n\"note_flow\":[\n{}\n],\n\"flow_cap\":[\n{}\n],\n\"climb_line\":[\n{}\n],\n\"climb_line_reach\":[\n{}\n]}}\n",
        j(shares),
        j(slice_cases()),
        j(mdt),
        j(note_flow_cases()),
        j(flow_cap_cases()),
        j(climb_cases()),
        j(climb_reach_cases())
    )
}

#[test]
fn the_netflow_vectors_are_the_rust_arithmetic() {
    let t = transcript();
    if std::env::var("FLOOR_WRITE_NETFLOW_VECTORS").is_ok() {
        std::fs::write(FIXTURE, &t).expect("write the vectors");
        return;
    }
    let on_disk = std::fs::read_to_string(FIXTURE).expect("fixtures/netflow/netflow-vectors.json (regenerate: FLOOR_WRITE_NETFLOW_VECTORS=1)");
    assert!(on_disk == t, "the net-flow vectors drifted from floor_math::netflow — regenerate with FLOOR_WRITE_NETFLOW_VECTORS=1, re-run the SDK's netflow.test.ts and tell the EVM side");
}

/// The transcript is not vacuous: every branch of the climb line is reached, and the meter's scaling branch fires.
#[test]
fn the_vectors_reach_every_branch() {
    let t = transcript();
    assert!(t.contains("\"line\":null"), "a no-hold case");
    assert!(t.matches("\"solved\":-").count() + t.matches("\"solved\":1").count() + t.matches("\"solved\":2").count() + t.matches("\"solved\":3").count() + t.matches("\"solved\":4").count() + t.matches("\"solved\":5").count() >= 5, "solved cases");
    assert!(t.contains("\"spot_range\":null"), "a spot with no range");
    assert!(t.contains("\"mul_div_trunc\""));
    assert!(t.contains("\"label\":\"the reach moves the line, reach 0\""), "the reach rows");
    let line_of = |label: &str| {
        let row = &t[t.find(&format!("\"label\":\"{label}\"")).expect(label)..];
        let i = row.find("\"line\":").unwrap() + 7;
        row[i..row[i..].find('}').unwrap() + i].to_string()
    };
    assert_ne!(line_of("the reach moves the line, reach 0"), line_of("the reach moves the line, reach 296"), "a reach that moves the line");
}
