// SPDX-License-Identifier: Apache-2.0
use crate::u256::U256;
use crate::{MathError, BPS};

pub const MAX_MEMBERS: usize = 10;
pub const WEIGHT_ONE: u64 = 1_000_000_000;
pub const PPM: u64 = 1_000_000;

pub const TRADER_REF_BPS: [u16; 3] = [2_400, 420, 180];
pub const CREATOR_REF_BPS: [u16; 3] = [800, 140, 60];
pub const TRADER_REF_OG_BPS: [u16; 3] = [4_000, 700, 300];
pub const CREATOR_REF_OG_BPS: [u16; 3] = [2_400, 420, 180];
pub const OG_WINDOW_SECS: u64 = 15_811_200;
pub const OG_MAX: u16 = 100;

pub const CREATOR_BUY_LOCK_SECS: u64 = 86_400;

pub const PRESET_WAGMI: u8 = 1;
pub const PRESET_MOONSHOT: u8 = 3;

pub const PRESET_RETIRED_ID: u8 = 2;

pub const fn preset_retired(id: u8) -> bool {
    id == PRESET_RETIRED_ID
}

fn part(leg: u128, bps: u16) -> Result<u128, MathError> {
    if bps == 0 || leg == 0 {
        return Ok(0);
    }
    U256::mul_u128(leg, bps as u128).div_rem_u128(BPS as u128).0.to_u128().ok_or(MathError::Overflow)
}

pub fn slices(fee: u128, present: [bool; 3], bps: [u16; 3]) -> Result<([u128; 3], u128), MathError> {
    let mut out = [0u128; 3];
    let mut left = fee;
    for i in 0..3 {
        if !present[i] || bps[i] == 0 {
            continue;
        }
        let s = part(fee, bps[i])?.min(left);
        out[i] = s;
        left -= s;
    }
    Ok((out, left))
}

pub const fn og_until(from: u64) -> u64 {
    if from == 0 {
        0
    } else {
        from.saturating_add(OG_WINDOW_SECS)
    }
}

pub const fn og_live(until: u64, now: u64) -> bool {
    now < until
}

pub const fn og_launch(from: u64, until: u64, created_at: u64, now: u64) -> bool {
    from != 0 && from <= created_at && created_at < until && now < until
}

pub const fn tier_bps(bps: [u16; 3], og_bps: [u16; 3], og: [bool; 3]) -> [u16; 3] {
    [
        if og[0] { og_bps[0] } else { bps[0] },
        if og[1] { og_bps[1] } else { bps[1] },
        if og[2] { og_bps[2] } else { bps[2] },
    ]
}

pub fn platform_legs(platform: u128, trader_present: [bool; 3]) -> Result<([u128; 3], u128), MathError> {
    platform_legs_og(platform, trader_present, [false; 3])
}

pub fn platform_legs_og(platform: u128, trader_present: [bool; 3], og: [bool; 3]) -> Result<([u128; 3], u128), MathError> {
    slices(platform, trader_present, tier_bps(TRADER_REF_BPS, TRADER_REF_OG_BPS, og))
}

pub fn creator_legs(creator: u128, creator_present: [bool; 3]) -> Result<([u128; 3], u128), MathError> {
    creator_legs_og(creator, creator_present, [false; 3])
}

pub fn creator_legs_og(creator: u128, creator_present: [bool; 3], og: [bool; 3]) -> Result<([u128; 3], u128), MathError> {
    slices(creator, creator_present, tier_bps(CREATOR_REF_BPS, CREATOR_REF_OG_BPS, og))
}

pub fn weights(paid: &[u128; MAX_MEMBERS], bought: u128) -> Result<[u32; MAX_MEMBERS], MathError> {
    let mut w = [0u32; MAX_MEMBERS];
    if bought == 0 {
        return Ok(w);
    }
    if paid.iter().try_fold(0u128, |a, &p| a.checked_add(p)).is_none_or(|t| t > bought) {
        return Err(MathError::Overflow);
    }
    for (i, &p) in paid.iter().enumerate() {
        let v = U256::mul_u128(p.min(bought), WEIGHT_ONE as u128).div_rem_u128(bought).0.to_u128().ok_or(MathError::Overflow)?;
        w[i] = v as u32;
    }
    Ok(w)
}

pub fn slot_weights(shares: &[u128; MAX_MEMBERS]) -> Result<[u32; MAX_MEMBERS], MathError> {
    let total = shares.iter().try_fold(0u128, |a, &x| a.checked_add(x)).ok_or(MathError::Overflow)?;
    if total == 0 {
        let mut w = [0u32; MAX_MEMBERS];
        w[0] = WEIGHT_ONE as u32;
        return Ok(w);
    }
    weights(shares, total)
}

pub fn split_members(amount: u128, w: &[u32; MAX_MEMBERS]) -> Result<([u128; MAX_MEMBERS], u128), MathError> {
    let mut parts = [0u128; MAX_MEMBERS];
    let mut given = 0u128;
    for i in 0..MAX_MEMBERS {
        parts[i] = U256::mul_u128(amount, (w[i] as u128).min(WEIGHT_ONE as u128)).div_rem_u128(WEIGHT_ONE as u128).0.to_u128().ok_or(MathError::Overflow)?;
        parts[i] = parts[i].min(amount - given);
        given += parts[i];
    }
    Ok((parts, amount.saturating_sub(given)))
}

pub fn split_release(tokens: u128, w: &[u32; MAX_MEMBERS]) -> Result<[u128; MAX_MEMBERS], MathError> {
    let mut parts = [0u128; MAX_MEMBERS];
    if w.iter().all(|&x| x == 0) {
        parts[0] = tokens;
        return Ok(parts);
    }
    let (p, dust) = split_members(tokens, w)?;
    parts = p;
    parts[0] = parts[0].checked_add(dust).ok_or(MathError::Overflow)?;
    Ok(parts)
}

pub fn split_team_buy(tokens: u128, w: &[u32; MAX_MEMBERS], created_at: u64, now: u64) -> Result<[u128; MAX_MEMBERS], MathError> {
    if now < created_at.saturating_add(CREATOR_BUY_LOCK_SECS) {
        return Err(MathError::NotDue);
    }
    split_release(tokens, w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_team_buy_waits_its_day_then_splits_whole() {
        let mut w = [0u32; MAX_MEMBERS];
        assert_eq!(split_team_buy(1_000, &w, 100, 100 + CREATOR_BUY_LOCK_SECS - 1), Err(MathError::NotDue));
        assert_eq!(split_team_buy(1_000, &w, 100, 100 + CREATOR_BUY_LOCK_SECS).unwrap()[0], 1_000, "no shares → place 0");
        w[0] = 333_333_333;
        w[1] = 666_666_667;
        let p = split_team_buy(1_000, &w, 0, CREATOR_BUY_LOCK_SECS).unwrap();
        assert_eq!(p.iter().sum::<u128>(), 1_000, "the parts add up to the buy");
        assert_eq!((p[0], p[1]), (334, 666), "the dust to place 0");
    }

    #[test]
    fn the_referrer_slices_never_pass_the_fee() {
        let (r, rest) = platform_legs(10_000, [true, true, true]).unwrap();
        assert_eq!((r, rest), ([2_400, 420, 180], 7_000), "30 % split 80 / 14 / 6");
        let (r, rest) = creator_legs(10_000, [true, false, true]).unwrap();
        assert_eq!((r, rest), ([800, 0, 60], 9_140), "a missing tier's slice stays with the fee");
        let (p, d) = split_members(7, &[WEIGHT_ONE as u32, WEIGHT_ONE as u32, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(p.iter().sum::<u128>() + d, 7, "never past the amount, whatever the weights");
    }

    #[test]
    fn no_shares_is_all_to_the_creator_and_shares_never_sum_past_one() {
        assert_eq!(slot_weights(&[0; MAX_MEMBERS]).unwrap()[0], WEIGHT_ONE as u32);
        let w = slot_weights(&[1, 1, 1, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert!(w.iter().map(|&x| x as u64).sum::<u64>() <= WEIGHT_ONE && w[0] == 333_333_333);
        assert_eq!(weights(&[2, 0, 0, 0, 0, 0, 0, 0, 0, 0], 1), Err(MathError::Overflow), "paid past what is bought");
    }

    #[test]
    fn every_rate_is_80_14_6_of_its_total() {
        for (b, total) in [(TRADER_REF_BPS, 3_000u32), (TRADER_REF_OG_BPS, 5_000), (CREATOR_REF_BPS, 1_000), (CREATOR_REF_OG_BPS, 3_000)] {
            assert_eq!(b.iter().map(|&x| x as u32).sum::<u32>(), total);
            assert_eq!([b[0] as u32 * 100, b[1] as u32 * 100, b[2] as u32 * 100], [total * 80, total * 14, total * 6]);
        }
    }

    #[test]
    fn each_tier_follows_its_own_referrers_og_window() {
        let (r, rest) = platform_legs_og(10_000, [true; 3], [true, false, true]).unwrap();
        assert_eq!((r, rest), ([4_000, 420, 300], 5_280), "tiers 1 and 3 OG, tier 2 standard");
        let (r, rest) = platform_legs_og(10_000, [true; 3], [true; 3]).unwrap();
        assert_eq!((r, rest), ([4_000, 700, 300], 5_000), "all OG: 50 %");
        let (r, rest) = creator_legs_og(10_000, [true; 3], [true; 3]).unwrap();
        assert_eq!((r, rest), ([2_400, 420, 180], 7_000), "all OG: 30 % of the Team Share");
        let (r, _) = platform_legs_og(10_000, [false, true, true], [true; 3]).unwrap();
        assert_eq!(r, [0, 700, 300], "an OG flag on a missing tier pays nothing");
    }

    #[test]
    fn the_og_window_is_183_days_and_launches_count_only_inside_it() {
        let from = 1_800_000_000u64;
        let until = og_until(from);
        assert_eq!(until - from, 183 * 86_400);
        assert_eq!(og_until(0), 0, "no window");
        assert!(og_live(until, from) && og_live(until, until - 1) && !og_live(until, until), "ends at until");
        assert!(!og_live(0, 0), "a stored 0 is never OG");
        assert!(og_launch(from, until, from, from + 1), "created at the window's start");
        assert!(!og_launch(from, until, from - 1, from + 1), "created before the window: standard (new launches only)");
        assert!(!og_launch(from, until, until, until - 1), "created at the end");
        assert!(!og_launch(from, until, from + 5, until), "the window is over");
        assert!(!og_launch(0, 0, 0, 0), "no window");
        let revoked = from + 10;
        assert!(!og_launch(from, revoked, from + 10, from + 11), "a revoke ends it for new launches too");
    }

    #[test]
    fn id_2_is_retired() {
        assert!(preset_retired(2) && !preset_retired(1) && !preset_retired(3));
    }
}
