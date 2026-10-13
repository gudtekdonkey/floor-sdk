//! ⛔ MULTIPLE RECIPIENTS, native on chain.
//!
//! A launch names up to [`MAX_SLOTS`] slots, bps shares summing to [`TOTAL_BPS`], for the creator fee AND for the team
//! buy's payee. Each pool books ONE total per book (the swap path is untouched); a slot's part is computed LAZILY at
//! claim time from the book's LIFETIME (the referral-tier pattern, [`crate::referral::referral_tier_dues`]):
//!
//! - `earned_i(L) = floor(L × bps_i / 10,000)` — PER-SLOT FLOOR, NO REMAINDER SLOT. Monotone in `L` for every slot,
//!   so a later claim can never underflow; the unassigned dust `L − Σ earned_i ≤ n − 1` raw units stays in custody.
//!   One slot at 10,000 → `earned = L` exactly.
//! - `due_i = min(earned_i(L) − paid_i, owed)` (saturating; a corrupt book pays 0, never panics).
//!
//! ⛔ THE ONE SPELLING of the split on Solana. The EVM twin is `FloorSlots.earned`; the two agree to the unit on the
//! parity vector MR-S-PV1 (`fixtures/mr-s-pv1.json`, `tests::mr_s_pv1_*`).

/// At most five slots in either list (D1).
pub const MAX_SLOTS: usize = 5;
/// The shares of one list sum to exactly this.
pub const TOTAL_BPS: u32 = 10_000;
/// 1 X / twitter, 2 github, 3 discord, 4 telegram; 0 is an ADDRESS slot.
pub const MAX_PLATFORM: u8 = 4;

/// `floor(lifetime × bps / 10,000)`, in u128 so no input overflows. `bps > 10,000` cannot occur on a checked list; it is
/// clamped to 10,000 here so a corrupt slot can never earn more than the whole book.
pub fn slot_earned(lifetime: u64, bps: u16) -> u64 {
    let bps = (bps as u128).min(TOTAL_BPS as u128);
    // ≤ lifetime, so the narrowing is exact.
    ((lifetime as u128 * bps) / TOTAL_BPS as u128) as u64
}

/// What ONE slot may be paid now: `min(slot_earned(lifetime, bps) − paid, owed)`, saturating at 0.
pub fn slot_due(lifetime: u64, bps: u16, paid: u64, owed: u64) -> u64 {
    slot_earned(lifetime, bps).saturating_sub(paid).min(owed)
}

/// The team's staircase exit is per slot (the EVM's `FloorSlots.part`). Rule 2's `part(x, b, W)`: slot `b`'s part of `x` over the
/// in-weight `W` — `x` itself when `b == W` (the last slot in takes what is left), else `floor(x · b / W)`; 0 at `W == 0`
/// (cannot occur: the last slot out took everything; the caller pools it). `b > W` (a corrupt mask) is capped at `x`.
pub fn part(x: u64, b: u16, w: u32) -> u64 {
    if b as u32 == w {
        return x;
    }
    if w == 0 {
        return 0;
    }
    ((x as u128 * b as u128 / w as u128) as u64).min(x)
}

/// Rule 1: the in-weight `W` of a step — Σ bps of the slots `< n` whose bit in `out` (5 bits, bit s = slot s exited) is
/// clear.
pub fn in_weight(bps: &[u16; MAX_SLOTS], n: u8, out: u8) -> u32 {
    (0..(n as usize).min(MAX_SLOTS)).filter(|&s| out & (1 << s) == 0).map(|s| bps[s] as u32).sum()
}

/// Rule 2, THE CASCADE: `x` split over the in-slots (`< n`, bit clear in `out`) in ASCENDING slot order —
/// `c_s = part(rest, b_s, w)`, `rest −= c_s`, `w −= b_s` — so the last in-slot takes what is left and `Σ c_s == x` exactly.
/// Every out-slot gets 0. `W == 0` (no slot in) returns all zeros: the caller pools `x` (rule 2's last sentence).
pub fn cascade(x: u64, bps: &[u16; MAX_SLOTS], n: u8, out: u8) -> [u64; MAX_SLOTS] {
    let mut c = [0u64; MAX_SLOTS];
    let mut w = in_weight(bps, n, out);
    let mut rest = x;
    for s in 0..(n as usize).min(MAX_SLOTS) {
        if out & (1 << s) != 0 || w == 0 {
            continue;
        }
        c[s] = part(rest, bps[s], w);
        rest -= c[s];
        w -= bps[s] as u32;
    }
    c
}

/// Rule 4: a slot's quote credit out of a ratchet whose whole ask was `a0` and FLOOR FIRST let `pay` through —
/// `a_s` when `pay == a0`, else `floor(a_s · pay / a0)`. 0 at `a0 == 0`.
pub fn ask_paid(a_s: u64, pay: u64, a0: u64) -> u64 {
    if pay == a0 {
        return a_s;
    }
    if a0 == 0 {
        return 0;
    }
    ((a_s as u128 * pay as u128 / a0 as u128) as u64).min(a_s)
}

/// Rule 6, both books: `min(owed, earned(pooled, bps) + credit − paid)` (saturating at 0). `pooled` is the book's
/// lifetime LESS what was split per slot (tokens: `held + withdrawn − partTokens`; quote: `owed + paid − partQuote`),
/// `credit` the slot's own split part, `paid` what it was paid (tokens / quote, tips included). With no partial withdrawal
/// `credit == 0` and `pooled` the whole lifetime: [`slot_due`] exactly.
pub fn slot_due_with_credit(pooled: u64, bps: u16, credit: u64, paid: u64, owed: u64) -> u64 {
    (slot_earned(pooled, bps) as u128 + credit as u128).saturating_sub(paid as u128).min(owed as u128) as u64
}

/// A vesting slot's part of cliffs `(from, to]` of a wallet part vesting in equal cliffs: `earned(V_to) − earned(V_from)`,
/// `V_k` the cumulative vested after cliff `k` (the caller's `vested_after`). Monotone, so summed over any order of claims
/// it pays exactly `earned(V_last)`.
pub fn slot_cliff_part(v_from: u64, v_to: u64, bps: u16) -> u64 {
    slot_earned(v_to, bps).saturating_sub(slot_earned(v_from, bps))
}

/// One slot as `create_launch` receives it (`SlotArg { bps, platform, key }`): platform 0 → `key` is the
/// wallet; platform 1..=4 → `key` is the pending id `keccak256(abi.encode(uint8 platform, string handle))` (the handle
/// string never rides the create wire).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Slot {
    pub bps: u16,
    pub platform: u8,
    pub key: [u8; 32],
}

/// `FloorSlots.check`'s codes, in the order they are tested: 0 = valid; 1 length 0 or > 5; 2 a slot with bps 0;
/// 3 Σ bps ≠ 10,000; 4 a slot's shape (platform > 4, or a zero key — "neither"); 5 a wallet named twice; 6 a pending id
/// named twice. The FIRST failing rule is returned (rule 1 over the list, then rule 2 over every slot, …).
pub fn check_slots(slots: &[Slot]) -> u8 {
    if slots.is_empty() || slots.len() > MAX_SLOTS {
        return 1;
    }
    if slots.iter().any(|s| s.bps == 0) {
        return 2;
    }
    if slots.iter().map(|s| s.bps as u32).sum::<u32>() != TOTAL_BPS {
        return 3;
    }
    if slots.iter().any(|s| s.platform > MAX_PLATFORM || s.key == [0u8; 32]) {
        return 4;
    }
    for (i, a) in slots.iter().enumerate() {
        for b in &slots[i + 1..] {
            if a.key == b.key && (a.platform == 0) == (b.platform == 0) {
                return if a.platform == 0 { 5 } else { 6 };
            }
        }
    }
    0
}

/// The code a pending creator slot adds when both creator rates are 0 (the EVM's `BadRecipients(7)`): a handle named to
/// receive a fee that is never charged.
pub const CHECK_PENDING_WITHOUT_RATE: u8 = 7;

#[cfg(test)]
mod tests {
    use super::*;

    fn w(b: u8, bps: u16) -> Slot {
        Slot { bps, platform: 0, key: [b; 32] }
    }
    fn h(b: u8, bps: u16, platform: u8) -> Slot {
        Slot { bps, platform, key: [b; 32] }
    }

    #[test]
    fn mr_s_one_slot_is_today_exactly() {
        for l in [0u64, 1, 9_999, 10_000, 123_456_789, u64::MAX] {
            assert_eq!(slot_earned(l, 10_000), l);
            assert_eq!(slot_due(l, 10_000, 0, l), l);
        }
    }

    #[test]
    fn mr_s_earned_is_monotone_and_the_dust_is_below_n() {
        let bps = [3_333u16, 3_333, 3_334];
        let mut prev = [0u64; 3];
        for l in 0u64..20_000 {
            let e: Vec<u64> = bps.iter().map(|&b| slot_earned(l, b)).collect();
            for i in 0..3 {
                assert!(e[i] >= prev[i], "monotone");
                prev[i] = e[i];
            }
            let dust = l - e.iter().sum::<u64>();
            assert!(dust < 3, "dust {dust} at {l}");
        }
    }

    #[test]
    fn mr_s_due_saturates_and_caps_at_owed() {
        assert_eq!(slot_due(1_000, 5_000, 600, 1_000), 0); // corrupt paid > earned
        assert_eq!(slot_due(1_000, 5_000, 0, 100), 100); // capped at owed
        assert_eq!(slot_due(1_000, 5_000, 100, 1_000), 400);
    }

    #[test]
    fn mr_s_cliff_parts_sum_to_the_whole() {
        let total = 1_000_003u64;
        let v = |k: u64| total * k / 4;
        for bps in [1u16, 2_500, 3_333, 10_000] {
            let sum: u64 = (1..=4).map(|k| slot_cliff_part(v(k - 1), v(k), bps)).sum();
            assert_eq!(sum, slot_earned(total, bps));
        }
    }

    #[test]
    fn mr_s_check_codes_in_order() {
        assert_eq!(check_slots(&[]), 1);
        assert_eq!(check_slots(&[w(1, 2_000); 6]), 1);
        assert_eq!(check_slots(&[w(1, 10_000)]), 0);
        assert_eq!(check_slots(&[w(1, 10_000), w(2, 0)]), 2);
        assert_eq!(check_slots(&[w(1, 5_000), w(2, 4_999)]), 3);
        assert_eq!(check_slots(&[w(1, 5_000), h(2, 5_000, 5)]), 4);
        assert_eq!(check_slots(&[w(0, 5_000), w(2, 5_000)]), 4);
        assert_eq!(check_slots(&[w(1, 5_000), w(1, 5_000)]), 5);
        assert_eq!(check_slots(&[h(1, 5_000, 1), h(1, 5_000, 2)]), 6);
        // the same 32 bytes as a wallet AND as an id are not a duplicate
        assert_eq!(check_slots(&[w(1, 5_000), h(1, 5_000, 1)]), 0);
        // rule order: a zero bps beats a bad sum and a bad shape
        assert_eq!(check_slots(&[w(0, 0), h(2, 9_000, 9)]), 2);
    }

    const PV2_BPS: [u16; MAX_SLOTS] = [5_000, 3_000, 1_999, 1, 0];

    /// MR-S-PV2-1 (rule 2): the cascade, shared with the EVM's `FloorSlots.part` to the unit.
    #[test]
    fn mr_s_pv2_1_the_cascade() {
        let three = [3_334, 3_333, 3_333, 0, 0];
        assert_eq!(cascade(100, &three, 3, 0), [33, 33, 34, 0, 0]);
        assert_eq!(cascade(1_000_003, &PV2_BPS, 4, 0), [500_001, 300_001, 199_900, 101, 0]);
        // exact: the last in-slot takes the rest, whatever is out
        for out in 0u8..16 {
            for x in [0u64, 1, 7, 1_000_003, u64::MAX] {
                let c = cascade(x, &PV2_BPS, 4, out);
                let sum: u128 = c.iter().map(|&v| v as u128).sum();
                assert_eq!(sum, if out == 0b1111 { 0 } else { x as u128 }, "x {x} out {out:04b}");
                for s in 0..4 {
                    if out & (1 << s) != 0 {
                        assert_eq!(c[s], 0);
                    }
                }
            }
        }
    }

    /// MR-S-PV2-2 (rules 2 / 3): single withdrawals from one step, in the order slot 3, 0, 2, 1.
    #[test]
    fn mr_s_pv2_2_withdrawals_from_one_step() {
        let mut dv = 1_000_003u64;
        let mut out = 0u8;
        let mut got = vec![];
        for s in [3usize, 0, 2, 1] {
            let x = part(dv, PV2_BPS[s], in_weight(&PV2_BPS, 4, out));
            got.push(x);
            dv -= x;
            out |= 1 << s;
        }
        assert_eq!(got, vec![100, 500_001, 199_900, 300_002]);
        assert_eq!(dv, 0);
    }

    /// MR-S-PV2-3 (rule 4): a step's ask after slot 0 exited.
    #[test]
    fn mr_s_pv2_3_proceeds_after_a_slot_exited() {
        assert_eq!(cascade(777_777, &PV2_BPS, 4, 0b0001), [0, 466_666, 310_955, 156, 0]);
    }

    /// MR-S-PV2-4 (rule 4): the same ratchet also asks 500,000 pooled; FLOOR FIRST lets 900,000 through.
    #[test]
    fn mr_s_pv2_4_the_forfeit() {
        let a = cascade(777_777, &PV2_BPS, 4, 0b0001);
        let a0 = 500_000 + a.iter().sum::<u64>();
        assert_eq!(a0, 1_277_777);
        let q: Vec<u64> = a.iter().map(|&x| ask_paid(x, 900_000, a0)).collect();
        assert_eq!(q, vec![0, 328_695, 219_020, 109, 0]);
        assert_eq!(q.iter().sum::<u64>(), 547_824);
        assert_eq!(900_000 - 547_824, 352_176, "pooled");
        // nothing forfeited: each slot its whole ask
        assert_eq!(ask_paid(466_666, a0, a0), 466_666);
    }

    /// MR-S-PV2-5 (rule 6): the dues on a fresh pool after PV2-4.
    #[test]
    fn mr_s_pv2_5_the_dues() {
        let (owed, paid, part_quote) = (900_000u64, 0u64, 547_824u64);
        let credit = [0u64, 328_695, 219_020, 109];
        let pooled = owed + paid - part_quote;
        assert_eq!(pooled, 352_176);
        let due: Vec<u64> = (0..4).map(|s| slot_due_with_credit(pooled, PV2_BPS[s], credit[s], 0, owed)).collect();
        assert_eq!(due, vec![176_088, 434_347, 289_419, 144]);
        assert_eq!(due.iter().sum::<u64>(), 899_998, "dust 2 <= n - 1");
        // with no split it is slot_due exactly
        for l in [0u64, 9, 352_176] {
            assert_eq!(slot_due_with_credit(l, 3_000, 0, 5, l), slot_due(l, 3_000, 5, l));
        }
    }

    /// MR-S-PV2-tokens (rule 5): slot 1 OUT of a step that hands 1,000,003 tokens to held.
    #[test]
    fn mr_s_pv2_tokens_held_from_a_partly_exited_step() {
        assert_eq!(cascade(1_000_003, &PV2_BPS, 4, 0b0010), [714_287, 0, 285_573, 143, 0]);
    }

    /// Rule 2's edges: the last slot in takes all; nobody in takes nothing; a corrupt `b > W` never exceeds `x`.
    #[test]
    fn part_edges() {
        assert_eq!(part(1_000, 10_000, 10_000), 1_000);
        assert_eq!(part(1_000, 1, 1), 1_000);
        assert_eq!(part(1_000, 5, 0), 0);
        assert_eq!(part(1_000, 9, 3), 1_000);
        assert_eq!(part(u64::MAX, 9_999, 10_000), (u64::MAX as u128 * 9_999 / 10_000) as u64);
        assert_eq!(ask_paid(5, 0, 0), 5, "pay == a0 == 0");
        assert_eq!(ask_paid(5, 3, 0), 0);
    }

    /// MR-S-PV1: the parity vector shared with the EVM's `FloorSlots.earned`. Each row is `[lifetime, bps, earned]`,
    /// `earned` computed OUTSIDE this crate (Python big integers: `lifetime * bps // 10000`) and written to the fixture.
    #[test]
    fn mr_s_pv1_matches_the_shared_vector() {
        let src = include_str!("../fixtures/mr-s-pv1.json");
        let mut rows = 0;
        for line in src.lines() {
            let t = line.trim().trim_end_matches(',');
            if !t.starts_with("[\"") {
                continue;
            }
            let cells: Vec<&str> = t.trim_matches(|c| c == '[' || c == ']').split(',').map(|c| c.trim().trim_matches('"')).collect();
            let (l, b, e): (u64, u16, u64) = (cells[0].parse().unwrap(), cells[1].parse().unwrap(), cells[2].parse().unwrap());
            assert_eq!(slot_earned(l, b), e, "MR-S-PV1 row {rows}: earned({l}, {b})");
            rows += 1;
        }
        assert!(rows >= 40, "MR-S-PV1 has {rows} rows");
    }
}
