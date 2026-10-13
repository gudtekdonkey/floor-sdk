//! The EVM's `_ceilToSpacing` / `_floorToSpacing` with Solana's tick bounds (±443,636). Solidity's `/` and `%` on
//! int24 truncate toward zero, exactly as Rust's on i32.
//! The arithmetic runs in i64 so no input tick can overflow; the clamps are the EVM's.

use carpenter_clmm_math::tick_math::{MAX_TICK, MIN_TICK};

use crate::error::{FloorMathError, FloorMathResult};

/// Round `tick` UP to a multiple of `spacing`; a result above `MAX_TICK − spacing` becomes the
/// highest usable multiple, `(MAX_TICK / spacing)·spacing − spacing` (the EVM's `_ceilToSpacing`).
pub fn ceil_to_spacing(tick: i32, spacing: i32) -> FloorMathResult<i32> {
    if spacing <= 0 {
        return Err(FloorMathError::InvalidTickSpacing);
    }
    let (t, s) = (tick as i64, spacing as i64);
    let mut compressed = t / s;
    if t > 0 && t % s != 0 {
        compressed += 1;
    }
    let mut result = compressed * s;
    if result > MAX_TICK as i64 - s {
        result = (MAX_TICK as i64 / s) * s - s;
    }
    Ok(result as i32)
}

/// Round `tick` DOWN to a multiple of `spacing`; a result below `MIN_TICK + spacing` becomes the
/// lowest usable multiple, `(MIN_TICK / spacing)·spacing + spacing` (the EVM's `_floorToSpacing`).
pub fn floor_to_spacing(tick: i32, spacing: i32) -> FloorMathResult<i32> {
    if spacing <= 0 {
        return Err(FloorMathError::InvalidTickSpacing);
    }
    let (t, s) = (tick as i64, spacing as i64);
    let mut compressed = t / s;
    if t < 0 && t % s != 0 {
        compressed -= 1;
    }
    let mut result = compressed * s;
    if result < MIN_TICK as i64 + s {
        result = (MIN_TICK as i64 / s) * s + s;
    }
    Ok(result as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The ceil branch only bumps positive, off-grid ticks.
    #[test]
    fn ceil_rounds_toward_the_cheaper_token() {
        assert_eq!(ceil_to_spacing(0, 60).unwrap(), 0);
        assert_eq!(ceil_to_spacing(1, 60).unwrap(), 60);
        assert_eq!(ceil_to_spacing(60, 60).unwrap(), 60);
        assert_eq!(ceil_to_spacing(-1, 60).unwrap(), 0);
        assert_eq!(ceil_to_spacing(-61, 60).unwrap(), -60);
    }

    // The floor branch.
    #[test]
    fn floor_rounds_toward_the_dearer_token() {
        assert_eq!(floor_to_spacing(0, 60).unwrap(), 0);
        assert_eq!(floor_to_spacing(1, 60).unwrap(), 0);
        assert_eq!(floor_to_spacing(-1, 60).unwrap(), -60);
        assert_eq!(floor_to_spacing(-60, 60).unwrap(), -60);
        assert_eq!(floor_to_spacing(119, 60).unwrap(), 60);
    }

    // The bisection bounds at spacing 60 under Solana's ±443,636.
    #[test]
    fn edge_clamps_match_the_evm_rule_at_solana_bounds() {
        // 443636/60 = 7393 r 56 -> 7394·60 = 443640 > 443576 -> 7393·60 − 60 = 443520
        assert_eq!(ceil_to_spacing(MAX_TICK, 60).unwrap(), 443_520);
        // trunc(−443636/60) = −7393 -> −443580, not clamped by the ceil rule
        assert_eq!(ceil_to_spacing(MIN_TICK, 60).unwrap(), -443_580);
        // −443580 − 60 = −443640 < −443576 -> −443580 + 60
        assert_eq!(floor_to_spacing(MIN_TICK, 60).unwrap(), -443_520);
        assert_eq!(floor_to_spacing(MAX_TICK, 60).unwrap(), 443_580);
        // i32 extremes neither panic nor wrap
        assert_eq!(ceil_to_spacing(i32::MAX, 60).unwrap(), 443_520);
        assert_eq!(floor_to_spacing(i32::MIN, 60).unwrap(), -443_520);
        assert_eq!(ceil_to_spacing(i32::MIN, 1).unwrap(), i32::MIN);
        assert_eq!(floor_to_spacing(i32::MAX, 1).unwrap(), i32::MAX);
    }

    #[test]
    fn zero_or_negative_spacing_is_refused() {
        assert_eq!(ceil_to_spacing(5, 0), Err(FloorMathError::InvalidTickSpacing));
        assert_eq!(floor_to_spacing(5, -60), Err(FloorMathError::InvalidTickSpacing));
    }

    // Every result is a multiple of the spacing and within one spacing of the input (off the clamps).
    #[test]
    fn results_are_on_grid_and_adjacent() {
        for s in [1, 10, 60, 200] {
            for t in (-440_000..440_000).step_by(997) {
                let c = ceil_to_spacing(t, s).unwrap();
                let f = floor_to_spacing(t, s).unwrap();
                assert_eq!(c % s, 0);
                assert_eq!(f % s, 0);
                assert!(c >= t && c - t < s, "ceil {t} {s} -> {c}");
                assert!(f <= t && t - f < s, "floor {t} {s} -> {f}");
            }
        }
    }
}
