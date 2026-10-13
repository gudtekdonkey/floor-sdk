//! The bid band (the EVM's `SingletonFloorEngine`).
//!
//! The bid is ONE all-quote band `[floor_tick − W, floor_tick]`, `W = ceil_to_spacing(BID_BAND_TICKS, s)`.
//! Its DEAR edge (`floor_tick − W`) is where the backing solves to; its CHEAP edge is the published
//! `floor_tick`, the lowest price any fill inside the bid pays. Everything of ours measures to the dear
//! edge — the POL's top, the ladder's cap, the nudge's limit and the skim ramp's distance — so nothing
//! of ours sits inside the band and the market rests just above it. A partial fill narrows the band
//! from the dear side only (`[max(dear_edge, ceil(current + 1)), floor_tick]`); the cheap edge never
//! moves. With the band's token capacity `Q / geomean(edges) > sellable`, no sell can carry the market
//! through the cheap edge.

use crate::constants::BID_BAND_TICKS;
use crate::error::{FloorMathError, FloorMathResult};
use crate::spacing::ceil_to_spacing;

/// The EVM's `_bandTicks()`: `BID_BAND_TICKS` rounded UP to the pool's spacing, so the band is
/// always placeable. 120 at spacings 1, 10, 60; 200 at spacing 200.
pub fn band_ticks(spacing: i32) -> FloorMathResult<i32> {
    ceil_to_spacing(BID_BAND_TICKS, spacing)
}

/// The EVM's `_dearEdge()`: `floor_tick − band_ticks(spacing)`, the tick the backing solves to.
/// Checked: the crate's rule is `Err` on every overflow with the state untouched, and `take_fees` guards
/// `edge − tick` one line after this — the edge itself must not be the one unchecked step in that chain
/// (`floor_tick = i32::MIN` would wrap in release and panic in debug).
pub fn dear_edge(floor_tick: i32, spacing: i32) -> FloorMathResult<i32> {
    floor_tick.checked_sub(band_ticks(spacing)?).ok_or(FloorMathError::Overflow)
}

/// The EVM's `_publish(solved)`: a solved (dear-edge) tick as the floor it publishes — one band
/// width cheaper, capped inside the tick range by the same rounding the solve uses
/// (`ceil_to_spacing`'s `MAX_TICK − spacing` clamp). Checked, as `dear_edge` is.
pub fn publish(solved: i32, spacing: i32) -> FloorMathResult<i32> {
    let raised = solved.checked_add(band_ticks(spacing)?).ok_or(FloorMathError::Overflow)?;
    ceil_to_spacing(raised, spacing)
}

/// The EVM's `_bidWidth()`: the deployed bid's width. `bid_width == 0` only on a pool with no bid yet, read as one
/// spacing.
pub fn bid_width_or_spacing(bid_width: i32, spacing: i32) -> i32 {
    if bid_width == 0 {
        spacing
    } else {
        bid_width
    }
}

/// The EVM's `liveFloorTick()`: the floor the CURRENT backing publishes, `publish(solve)`. The caller
/// runs the solve (`solve::solve_floor_tick_with`) and hands the result here.
pub fn live_floor_tick(solved: i32, spacing: i32) -> FloorMathResult<i32> {
    publish(solved, spacing)
}

// Every pool is a staircase (`crate::stair`).

#[cfg(test)]
mod tests {

    use super::*;
    use carpenter_clmm_math::tick_math::{MAX_TICK, MIN_TICK};

    // The band width at the four spacings the harness sweeps.
    #[test]
    fn band_ticks_by_spacing() {
        assert_eq!(band_ticks(1).unwrap(), 120);
        assert_eq!(band_ticks(10).unwrap(), 120);
        assert_eq!(band_ticks(60).unwrap(), 120);
        assert_eq!(band_ticks(200).unwrap(), 200);
        assert_eq!(band_ticks(7).unwrap(), 126);
        assert!(band_ticks(0).is_err());
    }

    // The dear edge and the publish are inverses on the grid.
    #[test]
    fn dear_edge_and_publish_round_trip_on_the_grid() {
        for s in [1, 10, 60, 200] {
            for f in (-400_000..400_000).step_by(997) {
                let f = ceil_to_spacing(f, s).unwrap();
                let d = dear_edge(f, s).unwrap();
                assert_eq!(d, f - band_ticks(s).unwrap());
                assert_eq!(publish(d, s).unwrap(), f, "spacing {s} floor {f}");
            }
        }
        assert_eq!(dear_edge(0, 60).unwrap(), -120);
        assert_eq!(publish(-120, 60).unwrap(), 0);
        // an off-grid solve publishes UP (cheaper): the band is always placeable
        assert_eq!(publish(-119, 60).unwrap(), 60);
    }

    // `_publish` is capped by `_ceilToSpacing`'s MAX clamp, exactly as the solve's `hi` is.
    #[test]
    fn publish_clamps_at_max() {
        let hi = ceil_to_spacing(MAX_TICK, 60).unwrap();
        assert_eq!(hi, 443_520);
        assert_eq!(publish(hi, 60).unwrap(), hi);
        assert_eq!(publish(hi - 60, 60).unwrap(), hi);
        assert_eq!(publish(hi - 120, 60).unwrap(), hi);
        assert_eq!(publish(hi - 180, 60).unwrap(), hi - 60);
        assert_eq!(publish(MAX_TICK, 60).unwrap(), hi);
        // and the dear edge of the lowest usable floor is below MIN_TICK's grid point, not clamped
        let lo = ceil_to_spacing(MIN_TICK, 60).unwrap();
        assert_eq!(dear_edge(lo, 60).unwrap(), lo - 120);
    }

    // The band helpers are `Err` on overflow, never a wrap or a panic: the same
    // rule as the rest of the `take_fees` call chain. `i32::MIN` is not a tick, but the input is a
    // state field, and a hand-built state must get a refusal, not a wrapped edge.
    #[test]
    fn dear_edge_and_publish_are_checked() {
        assert_eq!(dear_edge(i32::MIN, 60), Err(crate::error::FloorMathError::Overflow));
        assert_eq!(dear_edge(i32::MIN + 120, 60), Ok(i32::MIN));
        assert_eq!(dear_edge(i32::MIN + 119, 60), Err(crate::error::FloorMathError::Overflow));
        assert_eq!(publish(i32::MAX, 60), Err(crate::error::FloorMathError::Overflow));
        assert_eq!(publish(i32::MAX - 119, 60), Err(crate::error::FloorMathError::Overflow));
        // once the add fits, `ceil_to_spacing`'s MAX clamp applies, as in `publish_clamps_at_max`
        assert_eq!(publish(i32::MAX - 120, 60), Ok(443_520));
    }

    // The bid width helper.
    #[test]
    fn zero_width_reads_as_one_spacing() {
        assert_eq!(bid_width_or_spacing(0, 60), 60);
        assert_eq!(bid_width_or_spacing(120, 60), 120);
        assert_eq!(bid_width_or_spacing(60, 60), 60);
    }

    #[test]
    fn live_floor_tick_is_publish() {
        assert_eq!(live_floor_tick(-120, 60).unwrap(), 0);
        assert_eq!(live_floor_tick(443_400, 60).unwrap(), 443_520);
    }

}
