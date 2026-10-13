//! THE POOL-PRICE TWAP's SDK PARITY VECTORS. The TypeScript SDK's `twap.ts` is a line-for-line twin of
//! `floor_types::twap`, so a ratchet builder names the cushion's target off the FloorPool's own oracle. This test is
//! the twin's golden: it replays seeded sequences of writes and reads through the REAL `TwapOracle` and compares the
//! transcript with `fixtures/twap/twap-vectors.json`, which the TypeScript test replays.
//!
//! Regenerate after a deliberate change to the oracle: `FLOOR_WRITE_TWAP_VECTORS=1 cargo test -p floor-types --test
//! twap_sdk_vectors` (then commit the JSON with the change). Covered: the first write, same-second and clock-behind no-ops,
//! long quiet gaps, the ring's eviction, young rings (the settled fallback and `last_tick`), negative ticks (rounding toward
//! −∞), times across the `u32` wrap, and a cumulative across the `i64` wrap.

use floor_types::twap::{TwapError, TwapOracle, CUSHION_TWAP_WINDOW_SECS};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/twap/twap-vectors.json");

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn answer(r: Result<i32, TwapError>) -> String {
    match r {
        Ok(t) => t.to_string(),
        Err(TwapError::Uninitialized) => "\"Uninitialized\"".into(),
        Err(TwapError::WindowTooLong) => "\"WindowTooLong\"".into(),
    }
}

fn state(o: &TwapOracle) -> String {
    let cps: Vec<String> = o
        .checkpoints
        .iter()
        .map(|c| format!("[\"{}\",{}]", c.tick_cumulative, c.timestamp))
        .collect();
    format!(
        "{{\"version\":{},\"newest\":{},\"count\":{},\"cum\":\"{}\",\"ts\":{},\"last\":{},\"cps\":[{}]}}",
        o.version,
        o.newest,
        o.count,
        o.current.tick_cumulative,
        o.current.timestamp,
        o.current.last_tick,
        cps.join(",")
    )
}

/// One case: a start time, a tick scale, and the seeded walk. Every op is a line of the transcript.
fn case(seed: u64, start: u32, scale: i64, preset_cum: Option<i64>) -> String {
    let mut g = Lcg(seed);
    let mut o = TwapOracle::empty();
    let mut ops: Vec<String> = Vec::new();
    let mut now = start;
    let mut tick: i32 = ((g.below(2 * scale as u64 + 1) as i64) - scale) as i32;
    // a read of a never-written oracle
    ops.push(format!(
        "{{\"r\":[{},{},{}],\"twap\":{},\"settled\":{}}}",
        now,
        tick,
        CUSHION_TWAP_WINDOW_SECS,
        answer(o.twap_tick(now, tick, CUSHION_TWAP_WINDOW_SECS)),
        answer(o.settled_tick(now, tick, CUSHION_TWAP_WINDOW_SECS))
    ));
    for k in 0..48 {
        // the gap to the next touch: mostly short, sometimes the same second, a quiet spell, or a clock behind
        let roll = g.below(100);
        let gap: i64 = if roll < 15 {
            0
        } else if roll < 20 {
            -((g.below(5) + 1) as i64)
        } else if roll < 85 {
            (g.below(400) + 1) as i64
        } else if roll < 95 {
            (g.below(3_000) + 300) as i64
        } else {
            (g.below(40_000) + 2_000) as i64
        };
        now = (now as i64 + gap).rem_euclid(1i64 << 32) as u32;
        o.observe(now, tick);
        let mut set = String::new();
        if k == 0 {
            if let Some(c) = preset_cum {
                // a cumulative a few steps from the i64 wrap, as the oracle's own test does (the twin applies `setcum`
                // to the current observation and the newest checkpoint after this write)
                o.current.tick_cumulative = c;
                o.checkpoints[o.newest as usize].tick_cumulative = c;
                set = format!(",\"setcum\":\"{}\"", c);
            }
        }
        ops.push(format!("{{\"w\":[{},{}]{},\"state\":{}}}", now, tick, set, state(&o)));
        // the price after the touch
        tick = (tick as i64 + (g.below(2 * (scale as u64 / 8) + 1) as i64 - scale / 8)).clamp(-887_272, 887_272) as i32;
        // reads at a few later seconds and windows, the ratchet's read among them
        for _ in 0..2 {
            let at = now.wrapping_add(g.below(2_500) as u32);
            let window = match g.below(4) {
                0 => 0,
                1 => (g.below(4_000) + 1) as u32,
                _ => CUSHION_TWAP_WINDOW_SECS,
            };
            let mut after = o;
            after.observe(at, tick);
            ops.push(format!(
                "{{\"r\":[{},{},{}],\"twap\":{},\"settled\":{},\"ratchet\":{}}}",
                at,
                tick,
                window,
                answer(o.twap_tick(at, tick, window)),
                answer(o.settled_tick(at, tick, window)),
                answer(after.settled_tick(at, tick, CUSHION_TWAP_WINDOW_SECS))
            ));
        }
    }
    format!("{{\"seed\":{},\"ops\":[\n{}\n]}}", seed, ops.join(",\n"))
}

fn transcript() -> String {
    let mut cases = Vec::new();
    for seed in 1..=8u64 {
        cases.push(case(seed, 1_790_000_000 + seed as u32 * 7, 120_000, None));
    }
    // ticks near the grid's ends, negative and positive
    cases.push(case(101, 1_800_000_000, 887_000, None));
    // times across the u32 wrap
    cases.push(case(102, u32::MAX - 3_000, 50_000, None));
    cases.push(case(103, u32::MAX - 20, 80_000, None));
    // a cumulative near the i64 wrap
    cases.push(case(104, 1_790_000_000, 887_000, Some(i64::MAX - 1_000_000)));
    cases.push(case(105, 1_790_000_000, 887_000, Some(i64::MIN + 1_000_000)));
    format!("{{\"window\":{},\"cases\":[\n{}\n]}}\n", CUSHION_TWAP_WINDOW_SECS, cases.join(",\n"))
}

#[test]
fn the_sdk_twin_replays_the_rust_oracle() {
    let t = transcript();
    if std::env::var("FLOOR_WRITE_TWAP_VECTORS").is_ok() {
        std::fs::write(FIXTURE, &t).expect("write the vectors");
        return;
    }
    let on_disk = std::fs::read_to_string(FIXTURE).expect("fixtures/twap/twap-vectors.json (regenerate: FLOOR_WRITE_TWAP_VECTORS=1)");
    assert!(on_disk == t, "the TWAP vectors drifted from floor_types::twap — regenerate with FLOOR_WRITE_TWAP_VECTORS=1 and re-run the SDK's twap.test.ts");
}
