//! What each referral TIER can claim, from ACCOUNT BYTES (three-tier referrals: each tier gets 10 % of the tier below,
//! from that tier's cut): `floor_hook::claim_referral_fees(tier)`'s own arithmetic — `floor_math::referral_tier_dues`
//! — over the launch's `FloorPool`, exactly as `getAccountInfo` returns it.
//!
//! The wasm build exports it as `referral_tier_dues` so a client's per-tier rows and claim planner show the
//! PROGRAM's number per tier, never `referral_owed` (which overstates tier 1 by up to 10 % on a multi-tier launch) and
//! never a second implementation of the cascade.
//! ⚠ A due is claimable only by a set tier; this returns the arithmetic and leaves that gate to the
//! caller, as the program's `tier_to_pay` applies it before it reads the due.

use floor_types::layout::FloorPool;

use crate::error::QuoteResult;
use crate::mech;

/// `[due1, due2, due3]` in the pool's quote base units, from the FULL `FloorPool` account data
/// (discriminator included).
pub fn referral_tier_dues(floor_pool_data: &[u8]) -> QuoteResult<[u64; 3]> {
    let fp: FloorPool = crate::state::decode_floor_pool(floor_pool_data)?;
    // ⛔ through `mech`, the one place this crate calls `floor-math`
    Ok(mech::referral_tier_dues(&fp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::QuoteError;
    use crate::state::encode_account;
    use bytemuck::Zeroable;

    #[test]
    fn the_account_bytes_quote_is_the_programs_arithmetic() {
        let mut fp = FloorPool::zeroed();
        fp.referrer = [1; 32];
        fp.referrer_t2 = [2; 32];
        fp.referrer_t3 = [3; 32];
        fp.referral_fees_paid = 500;
        fp.referral_owed = 500;
        assert_eq!(referral_tier_dues(&encode_account(&fp)).unwrap(), [450, 45, 5]);
        // tier 2 claimed its 45: the others are unchanged
        fp.referral_owed = 455;
        fp.referral_paid_t2 = 45;
        assert_eq!(referral_tier_dues(&encode_account(&fp)).unwrap(), [450, 0, 5]);
        assert_eq!(referral_tier_dues(&encode_account(&fp)[..100]), Err(QuoteError::QuoteBadAccount("FloorPool")));
    }
}
