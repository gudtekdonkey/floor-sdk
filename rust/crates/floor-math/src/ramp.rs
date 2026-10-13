//! The skim ramp. ⛔ Byte-identical to the EVM's `FloorHookMath` — the same integer operations in
//! the same order, so the same truncation. Do not "simplify" `far − (far − at)·c/10_000` into
//! `far·(10_000 − c)/10_000 + at·c/10_000`: the two differ by one bp whenever a product is not a
//! multiple of 10,000.
//!
//! The at-floor endpoints are per-pool values (`FloorState.skim_buy_at_floor_bps` / `skim_sell_at_floor_bps`; a pool
//! registers with 0 / `skim_sell_far_bps × its multiple`), so `skim_bps` takes SEVEN arguments. At the 0 / 1000
//! endpoints it is bit-identical to the 5-arg curve (`defaults_reproduce_the_legacy_curve` below proves it over the
//! whole grid).

/// The floor skim in bps for a swap whose tick sits `distance` ticks above the band's DEAR edge
/// (`distance = dear_edge − tick`), the EVM's `FloorHookMath` skim.
///
/// `closeness` is 10,000 at or under the edge and falls linearly to 0 at `ramp` ticks.
/// A buy ramps from `buy_far` toward `buy_at_floor` (either direction); a sell ramps from
/// `sell_far` toward `sell_at_floor` and stays FLAT at `sell_far` when the far rate already meets or
/// exceeds the at-floor target.
/// All four rates are `u16` because that is `FloorPool`'s field width, so no product here can
/// overflow a `u64`.
pub fn skim_bps(
    distance: i32,
    ramp: i32,
    buy: bool,
    buy_far: u16,
    sell_far: u16,
    buy_at_floor: u16,
    sell_at_floor: u16,
) -> u64 {
    let (buy_far, sell_far) = (buy_far as u64, sell_far as u64);
    let (buy_at_floor, sell_at_floor) = (buy_at_floor as u64, sell_at_floor as u64);
    let closeness: u64 = if distance <= 0 {
        10_000 // :28
    } else if distance < ramp {
        // :29 — `uint24(distance)`, `uint24(ramp)`: both positive on this branch
        10_000 - (distance as u64) * 10_000 / (ramp as u64)
    } else {
        0
    };
    if buy {
        if buy_far >= buy_at_floor {
            return buy_far - (buy_far - buy_at_floor) * closeness / 10_000; // :31
        }
        return buy_far + (buy_at_floor - buy_far) * closeness / 10_000; // :32
    }
    if sell_far >= sell_at_floor {
        return sell_far; // :34
    }
    sell_far + (sell_at_floor - sell_far) * closeness / 10_000 // :35
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{DEFAULT_SKIM_BUY_AT_FLOOR_BPS, DEFAULT_SKIM_SELL_AT_FLOOR_BPS, SKIM_RAMP_TICKS};

    /// The 7-arg Solidity expression (the EVM's `FloorHookMath`) evaluated in u128 with `uint24`
    /// casts, written out independently.
    #[allow(clippy::too_many_arguments)]
    fn solidity(distance: i32, ramp: i32, buy: bool, buy_far: u128, sell_far: u128, buy_at: u128, sell_at: u128) -> u128 {
        let mut closeness: u128 = 0;
        if distance <= 0 {
            closeness = 10_000;
        } else if distance < ramp {
            closeness = 10_000 - (distance as u32 & 0xFF_FFFF) as u128 * 10_000 / (ramp as u32 & 0xFF_FFFF) as u128;
        }
        if buy {
            if buy_far >= buy_at {
                return buy_far - (buy_far - buy_at) * closeness / 10_000;
            }
            return buy_far + (buy_at - buy_far) * closeness / 10_000;
        }
        if sell_far >= sell_at {
            return sell_far;
        }
        sell_far + (sell_at - sell_far) * closeness / 10_000
    }

    /// The 5-arg expression the EVM's `FloorHookMath` had before per-pool endpoints, kept only as the oracle
    /// for `defaults_reproduce_the_legacy_curve`.
    fn legacy(distance: i32, ramp: i32, buy: bool, buy_far: u128, sell_far: u128) -> u128 {
        let mut closeness: u128 = 0;
        if distance <= 0 {
            closeness = 10_000;
        } else if distance < ramp {
            closeness = 10_000 - (distance as u32 & 0xFF_FFFF) as u128 * 10_000 / (ramp as u32 & 0xFF_FFFF) as u128;
        }
        if buy {
            return buy_far - buy_far * closeness / 10_000;
        }
        if sell_far >= 1000 {
            return sell_far;
        }
        sell_far + (1000 - sell_far) * closeness / 10_000
    }

    fn fars() -> Vec<u16> {
        (0..=1100).step_by(25).chain([1, 3, 7, 33, 999, 1001, 65535]).collect()
    }

    // The EVM's `FloorHookMath` skim, over every distance from well under the floor to well past the ramp,
    // for every far rate the caps allow plus the flat-sell region, against a grid of at-floor endpoints
    // on both sides of each far rate (so both buy branches and both sell branches are exercised).
    #[test]
    fn identical_to_the_solidity_expression_everywhere() {
        let ats: [u16; 6] = [0, 50, 400, 1000, 1001, 65535];
        for d in (-20_000..=20_000).step_by(7) {
            for &far in &fars() {
                for &at in &ats {
                    for buy in [true, false] {
                        assert_eq!(
                            skim_bps(d, SKIM_RAMP_TICKS, buy, far, far, at, at) as u128,
                            solidity(d, SKIM_RAMP_TICKS, buy, far as u128, far as u128, at as u128, at as u128),
                            "d={d} far={far} at={at} buy={buy}"
                        );
                    }
                }
            }
        }
        // and every distance, at the defaults, with no stride
        for d in -20_000..=20_000 {
            for &far in &fars() {
                for buy in [true, false] {
                    assert_eq!(
                        skim_bps(d, SKIM_RAMP_TICKS, buy, far, far, 0, 1000) as u128,
                        solidity(d, SKIM_RAMP_TICKS, buy, far as u128, far as u128, 0, 1000),
                        "d={d} far={far} buy={buy}"
                    );
                }
            }
        }
    }

    // ⛔ The test above passes `far` as BOTH far rates and `at` as BOTH endpoints, so an argument-order
    // slip between the two buy arguments and the two sell arguments survives it. A real pool is asymmetric on both
    // axes at once — `$FLOOR` is 250/250 far with 0/1000 at the floor, and (250, 250, 50, 400) is a legal shape —
    // so the four rates are crossed here: every (buy_far, sell_far) against every (buy_at, sell_at), which is what
    // makes the buy branches depend on the BUY pair alone and the sell branches on the SELL pair alone.
    #[test]
    fn asymmetric_rate_pairs_are_identical_to_the_solidity_expression() {
        let fars: [(u16, u16); 6] = [(0, 0), (250, 250), (100, 1000), (1000, 100), (0, 1000), (3, 999)];
        let ats: [(u16, u16); 6] = [(0, 1000), (50, 400), (400, 50), (0, 0), (1000, 1000), (65535, 0)];
        for d in (-20_000..=20_000).step_by(3) {
            for &(bf, sf) in &fars {
                for &(ba, sa) in &ats {
                    for buy in [true, false] {
                        assert_eq!(
                            skim_bps(d, SKIM_RAMP_TICKS, buy, bf, sf, ba, sa) as u128,
                            solidity(d, SKIM_RAMP_TICKS, buy, bf as u128, sf as u128, ba as u128, sa as u128),
                            "d={d} bf={bf} sf={sf} ba={ba} sa={sa} buy={buy}"
                        );
                    }
                }
            }
        }
        // What that kills, stated: with the four rates distinct, no two of them may be exchanged.
        // Each right-hand side is what the named argument-swap mutant would return. ⛔ The rates and the
        // distance are chosen, not arbitrary: at `closeness` 0, 5,000 or 10,000 several swaps are
        // value-preserving (at 5,000 `buy_far ↔ buy_at` is a reflection about the midpoint), and the
        // sell's flat branch ignores `sell_at` unless `sell_far < sell_at` — so the sell rows need
        // their own pair. `d = 12_070` gives `closeness = 2_501`.
        let (d, r) = (12_070, SKIM_RAMP_TICKS);
        assert_eq!(skim_bps(d, r, true, 0, 0, 10_000, 0), 2_501, "closeness is 2,501 at this distance");
        let real_buy = skim_bps(d, r, true, 100, 900, 300, 700);
        assert_eq!(real_buy, 150);
        assert_ne!(real_buy, skim_bps(d, r, true, 900, 100, 300, 700), "buy_far ↔ sell_far");
        assert_ne!(real_buy, skim_bps(d, r, true, 100, 900, 700, 300), "buy_at ↔ sell_at");
        assert_ne!(real_buy, skim_bps(d, r, true, 300, 900, 100, 700), "buy_far ↔ buy_at");
        let real_sell = skim_bps(d, r, false, 800, 200, 100, 600);
        assert_eq!(real_sell, 300);
        assert_ne!(real_sell, skim_bps(d, r, false, 200, 800, 100, 600), "buy_far ↔ sell_far");
        assert_ne!(real_sell, skim_bps(d, r, false, 800, 200, 600, 100), "buy_at ↔ sell_at");
        assert_ne!(real_sell, skim_bps(d, r, false, 800, 600, 100, 200), "sell_far ↔ sell_at");
    }

    // At the endpoints `register_pool` writes, a sell climbs from `far` to 1.5x far — 15 % at the 10 % cap — and a buy
    // slides to 0 (`test/SellRates.t.sol` on the EVM side).
    #[test]
    fn registered_endpoints_ramp_sells_to_one_and_a_half_times_far() {
        use crate::constants::skim_sell_at_floor_bps;
        for far in [0u16, 1, 150, 200, 333, 1000] {
            let at = skim_sell_at_floor_bps(far);
            assert_eq!(skim_bps(SKIM_RAMP_TICKS, SKIM_RAMP_TICKS, false, far, far, 0, at), far as u64, "far={far}");
            assert_eq!(skim_bps(0, SKIM_RAMP_TICKS, false, far, far, 0, at), at as u64, "far={far}");
            assert_eq!(skim_bps(-500, SKIM_RAMP_TICKS, false, far, far, 0, at), at as u64, "far={far}: under the edge");
            assert_eq!(skim_bps(0, SKIM_RAMP_TICKS, true, far, far, 0, at), 0, "far={far}: buys slide to 0");
            // monotone non-decreasing toward the floor, never past the endpoint
            let mut prev = 0u64;
            for d in (0..=SKIM_RAMP_TICKS).rev() {
                let v = skim_bps(d, SKIM_RAMP_TICKS, false, far, far, 0, at);
                assert!(v >= prev && v <= at as u64, "far={far} d={d}");
                prev = v;
            }
        }
        assert_eq!(skim_bps(0, SKIM_RAMP_TICKS, false, 1000, 1000, 0, skim_sell_at_floor_bps(1000)), 1500);
    }

    // test/SettableSkim.t.sol::test_defaultsMatchLegacyCurve, over the whole grid: at the default
    // endpoints (0, 1000) the 7-arg curve IS the old 5-arg curve, bit for bit, for every far rate.
    #[test]
    fn defaults_reproduce_the_legacy_curve() {
        let (b, s) = (DEFAULT_SKIM_BUY_AT_FLOOR_BPS as u16, DEFAULT_SKIM_SELL_AT_FLOOR_BPS as u16);
        assert_eq!((b, s), (0, 1000));
        for d in -20_000..=20_000 {
            for &far in &fars() {
                for buy in [true, false] {
                    assert_eq!(
                        skim_bps(d, SKIM_RAMP_TICKS, buy, far, far, b, s) as u128,
                        legacy(d, SKIM_RAMP_TICKS, buy, far as u128, far as u128),
                        "d={d} far={far} buy={buy}"
                    );
                }
            }
        }
        // the four literal rows of the Solidity test
        assert_eq!(skim_bps(20_000, 16_094, true, 100, 100, 0, 1000), 100);
        assert_eq!(skim_bps(20_000, 16_094, false, 100, 100, 0, 1000), 100);
        assert_eq!(skim_bps(0, 16_094, true, 100, 100, 0, 1000), 0);
        assert_eq!(skim_bps(0, 16_094, false, 100, 100, 0, 1000), 1000);
    }

    // test/SettableSkim.t.sol::test_customAtFloorEndpoints.
    #[test]
    fn custom_at_floor_endpoints() {
        assert_eq!(skim_bps(0, 16_094, true, 200, 200, 50, 400), 50, "buy at floor uses buy_at_floor");
        assert_eq!(skim_bps(0, 16_094, false, 200, 200, 50, 400), 400, "sell at floor uses sell_at_floor");
        assert_eq!(skim_bps(0, 16_094, false, 0, 500, 0, 400), 500, "sell far >= sell_at_floor stays flat");
        // a buy endpoint ABOVE the far rate ramps upward (:32)
        assert_eq!(skim_bps(0, 16_094, true, 100, 100, 300, 1000), 300);
        assert_eq!(skim_bps(8047, 16_094, true, 100, 100, 300, 1000), 100 + 200 * 5000 / 10_000);
    }

    // The at-floor endpoints are per-pool now; at the defaults they are the old hook constants.
    #[test]
    fn endpoints() {
        for far in [0u16, 100, 200, 500, 1000] {
            assert_eq!(skim_bps(0, SKIM_RAMP_TICKS, true, far, far, 0, 1000), 0);
            assert_eq!(skim_bps(-5, SKIM_RAMP_TICKS, false, far, far, 0, 1000), 1000);
            assert_eq!(skim_bps(SKIM_RAMP_TICKS, SKIM_RAMP_TICKS, true, far, far, 0, 1000), far as u64);
            assert_eq!(skim_bps(SKIM_RAMP_TICKS, SKIM_RAMP_TICKS, false, far, far, 0, 1000), far as u64);
        }
    }

    // A non-positive ramp never takes the `distance < ramp` branch for a positive distance.
    #[test]
    fn degenerate_ramp_is_far_rate_above_the_floor() {
        assert_eq!(skim_bps(1, 0, true, 200, 200, 0, 1000), 200);
        assert_eq!(skim_bps(1, -10, false, 200, 200, 0, 1000), 200);
        assert_eq!(skim_bps(0, 0, false, 200, 200, 0, 1000), 1000);
    }
}
