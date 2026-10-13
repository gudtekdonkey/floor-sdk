//! FLOOR's tick snaps. Price ↔ tick and v4's usable-tick bounds are `carpenter_quote::price`,
//! re-exported here.

use floor_types::constants::{MAX_TICK, MIN_TICK};

pub use carpenter_quote::price::*;

use crate::error::{QuoteError, QuoteResult};

/// `floor_to_spacing` — floor-math's, through [`crate::mech`].
///
/// ⛔ NOT v4's `minUsableTick` clamp. The two agree away from the bounds and DISAGREE at them, which
/// is the one place the value is used; the hook writes floor-math's, so this must too (`mech.rs`
/// carries the measured pair). `min_usable_tick` is the v4 rule, under its own name.
/// ⛔ The tick is range-checked first: an out-of-range tick is a refusal (`TickOutOfRange`), never a
/// wrapped multiple of the spacing and never a panic.
pub fn floor_to_spacing(tick: i32, spacing: u16) -> QuoteResult<i32> {
    let s = spacing_i32(spacing)?;
    crate::mech::floor_to_spacing(in_range(tick)?, s)
}

/// `ceil_to_spacing` — floor-math's, through [`crate::mech`]. See
/// [`floor_to_spacing`] for why it is not v4's `maxUsableTick` clamp.
pub fn ceil_to_spacing(tick: i32, spacing: u16) -> QuoteResult<i32> {
    let s = spacing_i32(spacing)?;
    crate::mech::ceil_to_spacing(in_range(tick)?, s)
}

/// A tick the tick math can represent (±443,636).
fn in_range(tick: i32) -> QuoteResult<i32> {
    if !(MIN_TICK..=MAX_TICK).contains(&tick) {
        return Err(QuoteError::Math(carpenter_quote::clmm::MathError::TickOutOfRange));
    }
    Ok(tick)
}

fn spacing_i32(spacing: u16) -> QuoteResult<i32> {
    if spacing == 0 {
        return Err(QuoteError::Math(carpenter_quote::clmm::MathError::InvalidTickSpacing));
    }
    Ok(i32::from(spacing))
}
