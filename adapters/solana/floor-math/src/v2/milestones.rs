// SPDX-License-Identifier: Apache-2.0
use super::V2Error;

pub fn booked_from_remainder(r: u128, theta_bps: u16) -> Result<u128, V2Error> {
    let keep = 10_000u128.checked_sub(theta_bps as u128).ok_or(V2Error::Overflow)?;
    r.checked_mul(keep).map(|x| x / 10_000).ok_or(V2Error::Overflow)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassSplit {
    pub platform: u128,
    pub floor_token: u128,
    pub class: u128,
}

pub fn split_by_row(amount: u128, platform_bps: u16, floor_bps: u16) -> Result<ClassSplit, V2Error> {
    if platform_bps as u32 + floor_bps as u32 > 10_000 {
        return Err(V2Error::RowInvalid);
    }
    let part = |b: u16| amount.checked_mul(b as u128).map(|x| x / 10_000).ok_or(V2Error::Overflow);
    let (platform, floor_token) = (part(platform_bps)?, part(floor_bps)?);
    Ok(ClassSplit { platform, floor_token, class: amount - platform - floor_token })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TeamSplit {
    pub referrers: [u128; 3],
    pub slot: u128,
}

pub fn team_split(amount: u128, levels_present: [bool; 3]) -> TeamSplit {
    let tenth = amount / 10;
    let w = [90u128, 9, 1];
    let mut referrers = [0u128; 3];
    let mut paid = 0;
    for i in 0..3 {
        if levels_present[i] {
            referrers[i] = tenth * w[i] / 100;
            paid += referrers[i];
        }
    }
    TeamSplit { referrers, slot: amount - paid }
}

pub fn slot_split(slot: u128, shares_bps: &[u16]) -> (Vec<u128>, u128) {
    let parts: Vec<u128> = shares_bps.iter().map(|s| slot * *s as u128 / 10_000).collect();
    let paid: u128 = parts.iter().sum();
    let full: u32 = shares_bps.iter().map(|s| *s as u32).sum();
    let unassigned = if full < 10_000 { slot - slot * full as u128 / 10_000 } else { 0 };
    let _ = unassigned;
    (parts, slot - paid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn booking_is_one_minus_theta_of_the_remainder() {
        assert_eq!(booked_from_remainder(1_000, 1_000), Ok(900));
        assert_eq!(booked_from_remainder(1_000, 0), Ok(1_000));
        assert_eq!(booked_from_remainder(1_000, 10_000), Ok(0));
        assert_eq!(booked_from_remainder(7, 3_333), Ok(4));
    }

    #[test]
    fn platform_and_floor_shares_are_off_by_default() {
        assert_eq!(split_by_row(1_000, 0, 0).unwrap(), ClassSplit { platform: 0, floor_token: 0, class: 1_000 });
        let s = split_by_row(1_000, 500, 250).unwrap();
        assert_eq!((s.platform, s.floor_token, s.class), (50, 25, 925));
        assert_eq!(split_by_row(1, 10_000, 1), Err(V2Error::RowInvalid));
    }

    #[test]
    fn the_team_split_pays_the_creators_referrers_first() {
        let t = team_split(10_000, [true, true, true]);
        assert_eq!(t.referrers, [900, 90, 10]);
        assert_eq!(t.slot, 9_000);
        let t = team_split(10_000, [true, false, false]);
        assert_eq!((t.referrers, t.slot), ([900, 0, 0], 9_100));
        for a in [0u128, 1, 9, 10, 11, 99, 12_345] {
            let t = team_split(a, [true; 3]);
            assert_eq!(t.referrers.iter().sum::<u128>() + t.slot, a);
        }
        let (parts, dust) = slot_split(1_001, &[6_000, 3_000, 1_000]);
        assert_eq!((parts, dust), (vec![600, 300, 100], 1));
    }
}
