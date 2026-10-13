//! Three-tier referrals — the ONE spelling of the split.
//!
//! Each tier gets 10 % of the tier below, from that tier's cut. The tiers come OUT OF the existing referral share (10 %
//! of the GROSS creator fee, booked by [`crate::take_fees`] exactly as before); the creator keeps its 90 % unchanged.
//! The cascade — tier 3 takes its 10 % from tier 2, not from tier 1 — is 90 / 9 / 1 on a three-tier launch, 90 / 10 on
//! a two-tier one, 100 on a single-tier one.
//!
//! The EVM twin is the view `referralTiers()` in `SingletonFloorViews`; a parity case pins the two equal.
//!
//! ```text
//! L       = referral_fees_paid              // Solana: the lifetime BOOKED meter (owed included)
//! up2     = t2 set ? L   / 10 : 0           // tier 1 passes 10 % of its cut up
//! up3     = t3 set ? up2 / 10 : 0           // tier 2 passes 10 % of its cut up
//! earned  = [L - up2, up2 - up3, up3]
//! paid1   = L - referral_owed - paid_t2 - paid_t3     // DERIVED, never stored
//! due_i   = earned_i - paid_i,  each ≤ referral_owed
//! ```
//!
//! ⛔ **`paid1` IS DERIVED, NEVER STORED.** A pool whose referrer was already paid before any tier counter existed
//! reads zero in that counter: a stored tier-1 counter would make tier 1's due exceed `referral_owed`, and the claim
//! would brick (checked) or overpay out of the backing (saturating). Derived, such a pool's tier 1 is due exactly
//! `referral_owed`.
//! ⛔ **COMPUTED FROM THE RUNNING TOTAL `L`, NEVER PER ACCRUAL EVENT.** The sum of per-event floors is
//! not the floor of the sum; every consumer (an indexer, the SDK) calls this, or a byte-for-byte port of it pinned by a
//! parity test, on the pool's own meters.
//! ⛔ **WHY THIS FORMULA AND NOT A WEIGHTED ONE**: it is EXACT (`Σ earned == L`, no dust)
//! and MONOTONE (each step keeps `x − ⌊x/10⌋` of a non-decreasing `x`, so no `earned_i` ever falls
//! as `L` grows, so no `due_i` underflows). A weighted split with tier 1 as the remainder (7/2/1) is
//! not monotone (L 9→10 gives tier 1 8→7) and renormalising over the present tiers pays the wrong
//! ratio and strands dust.
//! ⚠ The swap path never calls this: `take_fees` books ONE number, so the `take_fees` ⇔ `_accrueCreator` twin rule
//! holds.

use crate::state::FloorState;

/// Share of its own cut each tier passes UP to the next (10 %), as a divisor. Fixed: no per-pool rate,
/// no setter (B6's "no dial").
pub const TIER_PASS_UP_DIVISOR: u64 = 10;

/// The tiers a claim may name: 1 = the direct referrer (`FloorState::referrer`), 2 = `referrer_t2`,
/// 3 = `referrer_t3`. The wire byte of `claim_referral_fees` carries exactly this number.
pub const REFERRAL_TIERS: u8 = 3;

const ZERO: [u8; 32] = [0u8; 32];

/// What each tier has EARNED over the pool's life: `[earned1, earned2, earned3]`, summing to exactly
/// `lifetime` (`L`). `t2` / `t3` say whether that tier's wallet is set. Pure; never fails.
pub fn referral_tier_earned(lifetime: u64, t2: bool, t3: bool) -> [u64; 3] {
    let up2 = if t2 { lifetime / TIER_PASS_UP_DIVISOR } else { 0 };
    // ⛔ tier 3 counts only behind a tier 2 (`register_pool` refuses a gap; this is belt and braces
    // so a corrupt pool can never pay tier 3 out of tier 1's cut).
    let up3 = if t2 && t3 { up2 / TIER_PASS_UP_DIVISOR } else { 0 };
    // up2 ≤ L and up3 ≤ up2 by construction, so neither subtraction can underflow.
    [lifetime - up2, up2 - up3, up3]
}

/// Each tier's DUE right now: `[due1, due2, due3]`, in the pool's quote. The ONE spelling — the
/// program's `claim_referral_fees`, `carpenter-quote` / the wasm, the SDK and the indexer all run
/// this (or a port pinned to it).
///
/// Every due is `≤ referral_owed`, and on a consistent pool `due1 + due2 + due3 == referral_owed`.
/// A pool whose meters are inconsistent (`referral_owed + paid_t2 + paid_t3 > L`, which no writer can
/// produce) is due NOTHING: the safe answer, never a guess that pays out of the backing.
/// Arithmetic in u128, checked.
pub fn referral_tier_dues(s: &FloorState) -> [u64; 3] {
    let lifetime = s.referral_fees_paid as u128;
    let owed = s.referral_owed as u128;
    let paid2 = s.referral_paid_t2 as u128;
    let paid3 = s.referral_paid_t3 as u128;
    let Some(paid1) = owed
        .checked_add(paid2)
        .and_then(|x| x.checked_add(paid3))
        .and_then(|taken| lifetime.checked_sub(taken))
    else {
        return [0; 3];
    };
    let earned = referral_tier_earned(s.referral_fees_paid, s.referrer_t2 != ZERO, s.referrer_t3 != ZERO);
    let paid = [paid1, paid2, paid3];
    let mut due = [0u64; 3];
    for i in 0..3 {
        let d = (earned[i] as u128).saturating_sub(paid[i]).min(owed);
        // `d ≤ owed ≤ u64::MAX`, so the narrowing is exact.
        due[i] = d as u64;
    }
    due
}

/// The due of ONE tier (1..=3), or `None` for any other tier number.
pub fn referral_tier_due(s: &FloorState, tier: u8) -> Option<u64> {
    match tier {
        1..=REFERRAL_TIERS => Some(referral_tier_dues(s)[(tier - 1) as usize]),
        _ => None,
    }
}

/// Tier `tier`'s wallet on this pool (1 = `referrer`, 2 = `referrer_t2`, 3 = `referrer_t3`), or
/// `None` for any other tier number. The zero key means the tier is not set.
pub fn referral_tier_wallet(s: &FloorState, tier: u8) -> Option<[u8; 32]> {
    match tier {
        1 => Some(s.referrer),
        2 => Some(s.referrer_t2),
        3 => Some(s.referrer_t3),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    const W1: [u8; 32] = [1; 32];
    const W2: [u8; 32] = [2; 32];
    const W3: [u8; 32] = [3; 32];

    fn pool(tiers: u8, booked: u64) -> FloorState {
        let mut s = FloorState::default();
        s.referrer = W1;
        if tiers >= 2 {
            s.referrer_t2 = W2;
        }
        if tiers >= 3 {
            s.referrer_t3 = W3;
        }
        s.referral_fees_paid = booked;
        s.referral_owed = booked;
        s
    }

    /// Apply a claim of `tier` the way `claim_referral_fees` does: owed down, the tier's counter up
    /// (tier 1 has no counter), `referral_fees_paid` untouched.
    fn claim(s: &mut FloorState, tier: u8) -> u64 {
        let due = referral_tier_due(s, tier).unwrap();
        s.referral_owed = s.referral_owed.checked_sub(due).unwrap();
        match tier {
            2 => s.referral_paid_t2 += due,
            3 => s.referral_paid_t3 += due,
            _ => {}
        }
        due
    }

    /// The worked table: 500 → 500 / 450,50 / 450,45,5.
    ///
    /// Mutations this kills: tier 3 taking its 10 % from tier 1 (450 → 445); the split applied to a
    /// single-tier pool; the ratios renormalised over present tiers.
    #[test]
    fn the_spec_table_one_two_and_three_tiers() {
        assert_eq!(referral_tier_dues(&pool(1, 500)), [500, 0, 0]);
        assert_eq!(referral_tier_dues(&pool(2, 500)), [450, 50, 0]);
        assert_eq!(referral_tier_dues(&pool(3, 500)), [450, 45, 5]);
        // Bluechip's 1 % creator fee doubles every figure.
        assert_eq!(referral_tier_dues(&pool(3, 1_000)), [900, 90, 10]);
        // an unreferred pool is due nothing
        assert_eq!(referral_tier_dues(&FloorState::default()), [0, 0, 0]);
    }

    /// Exact and monotone over a range, for every tier count (0..200,000 per tier count, to stay fast in a debug
    /// build).
    ///
    /// Mutation this kills: a weighted split with tier 1 as the remainder (7/2/1), which is not
    /// monotone (L 9→10 gives tier 1 8→7); any split that strands dust.
    #[test]
    fn exact_and_monotone_over_a_range() {
        for (t2, t3) in [(false, false), (true, false), (true, true)] {
            let mut prev = [0u64; 3];
            for l in 0..200_000u64 {
                let e = referral_tier_earned(l, t2, t3);
                assert_eq!(e[0] + e[1] + e[2], l, "exact at L={l}");
                for i in 0..3 {
                    assert!(e[i] >= prev[i], "earned[{i}] fell at L={l} ({t2},{t3})");
                }
                prev = e;
            }
        }
        // and at the top of the range, no overflow and still exact
        let e = referral_tier_earned(u64::MAX, true, true);
        assert_eq!(e[0] as u128 + e[1] as u128 + e[2] as u128, u64::MAX as u128);
    }

    /// A tier 3 without a tier 2 (a gap `register_pool` refuses) earns nothing: tier 1 keeps it all.
    #[test]
    fn a_gap_pays_tier_three_nothing() {
        let mut s = pool(1, 500);
        s.referrer_t3 = W3;
        assert_eq!(referral_tier_dues(&s), [500, 0, 0]);
    }

    /// A PRE-UPGRADE pool (t2 = t3 = 0, the new counters zero) that already paid its referrer is due
    /// EXACTLY `referral_owed` — never more.
    ///
    /// Mutation this kills: a STORED tier-1 counter (reads 0 on such a pool, so due1 would be L).
    #[test]
    fn a_pre_upgrade_pool_that_already_claimed_pays_exactly_owed() {
        let mut s = FloorState::default();
        s.referrer = W1;
        s.referral_fees_paid = 98_235 + 1_234; // booked over its life
        s.referral_owed = 1_234; // 98,235 already left
        assert_eq!(referral_tier_dues(&s), [1_234, 0, 0]);
        assert_eq!(claim(&mut s, 1), 1_234);
        assert_eq!(s.referral_owed, 0);
        assert_eq!(referral_tier_dues(&s), [0, 0, 0]);
    }

    /// Claim ORDER does not change what each tier receives, dues are recomputed after every accrual
    /// on the running total, and Σ paid == L − owed throughout.
    ///
    /// Mutation this kills: a claim that moves `referral_fees_paid` (tier 1's derived paid would
    /// shift); a tier-2 claim that forgets its counter (tier 2 would be paid twice).
    #[test]
    fn claim_order_does_not_matter_and_the_meters_conserve() {
        let orders: [[u8; 3]; 6] = [[1, 2, 3], [1, 3, 2], [2, 1, 3], [2, 3, 1], [3, 1, 2], [3, 2, 1]];
        let accruals = [7u64, 93, 1, 400, 0, 12_345, 9, 10, 11];
        let mut totals: Vec<[u64; 3]> = Vec::new();
        for order in orders {
            let mut s = pool(3, 0);
            let mut got = [0u64; 3];
            for (k, a) in accruals.iter().enumerate() {
                // `take_fees` books both meters by the same amount
                s.referral_fees_paid += a;
                s.referral_owed += a;
                // claim a rotating subset, in this order
                for &t in &order[..(k % 3) + 1] {
                    got[(t - 1) as usize] += claim(&mut s, t);
                    let paid1 = s.referral_fees_paid - s.referral_owed - s.referral_paid_t2 - s.referral_paid_t3;
                    assert_eq!(paid1 + s.referral_paid_t2 + s.referral_paid_t3, s.referral_fees_paid - s.referral_owed);
                    // a second claim of the same tier right away pays nothing
                    assert_eq!(referral_tier_due(&s, t), Some(0));
                }
            }
            for t in 1..=3 {
                got[(t - 1) as usize] += claim(&mut s, t);
            }
            assert_eq!(s.referral_owed, 0);
            assert_eq!(got.iter().sum::<u64>(), s.referral_fees_paid);
            assert_eq!(got, referral_tier_earned(s.referral_fees_paid, true, true));
            totals.push(got);
        }
        assert!(totals.windows(2).all(|w| w[0] == w[1]), "{totals:?}");
    }

    /// Every due is ≤ owed, and an inconsistent pool is due nothing.
    #[test]
    fn dues_are_capped_and_a_corrupt_pool_pays_nothing() {
        let mut s = pool(3, 1_000);
        let d = referral_tier_dues(&s);
        assert!(d.iter().all(|&x| x <= s.referral_owed));
        assert_eq!(d.iter().sum::<u64>(), s.referral_owed);
        s.referral_paid_t2 = 2_000; // more than was ever booked
        assert_eq!(referral_tier_dues(&s), [0, 0, 0]);
        let mut s = pool(3, u64::MAX);
        s.referral_paid_t2 = u64::MAX;
        s.referral_paid_t3 = u64::MAX;
        assert_eq!(referral_tier_dues(&s), [0, 0, 0], "no overflow in the u128 sum");
    }

    #[test]
    fn tier_numbers_outside_one_to_three_are_none() {
        let s = pool(3, 500);
        assert_eq!(referral_tier_due(&s, 0), None);
        assert_eq!(referral_tier_due(&s, 4), None);
        assert_eq!(referral_tier_wallet(&s, 0), None);
        assert_eq!(referral_tier_wallet(&s, 2), Some(W2));
        assert_eq!(referral_tier_wallet(&s, 3), Some(W3));
    }
}
