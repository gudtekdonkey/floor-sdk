//! Independent host tests of `floor_math::slots`: nothing here calls a helper of the crate's own tests, and the oracle is
//! spelled from the rule's text ("`earned_i(L) = floor(L × bps_i / 10,000)`", "`due = min(earned − paid, owed)`, saturating", "the
//! unassigned dust `L − Σ earned_i ≤ n − 1` raw units stays in custody", "one slot at 10,000 → `earned = L` exactly").
//!
//! - `mr_review_earned_is_the_spec_oracle_over_edges` — every bps 0..=10,000 at edge lifetimes (0, 1, 9,999, 10,000,
//!   2^63, u64::MAX − 1, u64::MAX) against `L·bps/10,000` in u128.
//! - `mr_review_pv1_fixture_reread_independently` — MR-S-PV1 parsed here by hand (no serde), every row re-derived from
//!   the oracle AND matched against `slot_earned`.
//! - `mr_review_random_books_conserve_and_leave_dust_below_n` — 400 random lists of 1..=5 slots summing to 10,000, each
//!   driven through ~300 random accruals and random per-slot claims (some slots "pending", i.e. never claiming until the
//!   end); after every claim: Σ slot.paid == the pool's paid, no claim above owed, no slot above its earned; at the end
//!   every slot holds exactly `earned(L)` and the dust left owed is ≤ n − 1.
//! - `mr_review_due_saturates_caps_and_never_panics` — corrupt books (paid > earned, owed 0, every u64::MAX corner).
//! - `mr_review_cliff_parts_partition_the_vested_whole` — random vesting curves of 1..=6 cliffs: Σ_k part_k ==
//!   `earned(V_last)` for each slot, claimed in any order.
//! - `mr_review_check_codes_first_failing_rule_wins` — the order of `FloorSlots.check`'s codes on lists that break two
//!   rules at once.

use floor_math::slots::{check_slots, slot_cliff_part, slot_due, slot_earned, Slot};

fn oracle(l: u64, bps: u16) -> u64 {
    ((l as u128 * bps.min(10_000) as u128) / 10_000) as u64
}

/// xorshift64*, deterministic.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// `n` positive bps summing to exactly 10,000.
fn random_split(r: &mut Rng, n: usize) -> Vec<u16> {
    let mut cuts: Vec<u16> = (0..n - 1).map(|_| 1 + r.below(9_999) as u16).collect();
    cuts.sort_unstable();
    cuts.dedup();
    while cuts.len() < n - 1 {
        let c = 1 + r.below(9_999) as u16;
        if !cuts.contains(&c) {
            cuts.push(c);
            cuts.sort_unstable();
        }
    }
    let mut out = Vec::with_capacity(n);
    let mut prev = 0u16;
    for c in cuts.iter().copied().chain(std::iter::once(10_000)) {
        out.push(c - prev);
        prev = c;
    }
    assert_eq!(out.iter().map(|&b| b as u32).sum::<u32>(), 10_000);
    assert!(out.iter().all(|&b| b > 0));
    out
}

#[test]
fn mr_review_earned_is_the_spec_oracle_over_edges() {
    let ls = [0u64, 1, 2, 9_999, 10_000, 10_001, 1 << 63, u64::MAX - 1, u64::MAX];
    for &l in &ls {
        for bps in 0..=10_000u16 {
            assert_eq!(slot_earned(l, bps), oracle(l, bps), "L={l} bps={bps}");
        }
        // D2: one slot at 10,000 is the whole book, exactly
        assert_eq!(slot_earned(l, 10_000), l);
        // a corrupt bps above the whole never earns more than the book
        for bps in [10_001u16, 20_000, u16::MAX] {
            assert_eq!(slot_earned(l, bps), l, "clamped at L={l} bps={bps}");
        }
    }
}

/// The fixture's rows as `(L, bps, earned)`, parsed without serde: every `["a", "b", "c"]` triple in `"rows"`.
fn pv1_rows() -> Vec<(u64, u16, u64)> {
    let src = include_str!("../fixtures/mr-s-pv1.json");
    let rows = &src[src.find("\"rows\"").expect("rows")..];
    let mut out = Vec::new();
    let mut i = 0;
    let b = rows.as_bytes();
    while let Some(o) = rows[i..].find('[') {
        let start = i + o + 1;
        let Some(c) = rows[start..].find(']') else { break };
        let inner = &rows[start..start + c];
        i = start + c + 1;
        let nums: Vec<&str> = inner.split(',').map(|s| s.trim().trim_matches('"')).collect();
        if nums.len() != 3 || nums.iter().any(|s| s.is_empty() || !s.bytes().all(|x| x.is_ascii_digit())) {
            continue; // the outer "[" of rows itself
        }
        out.push((nums[0].parse().unwrap(), nums[1].parse().unwrap(), nums[2].parse().unwrap()));
        let _ = b;
    }
    out
}

#[test]
fn mr_review_pv1_fixture_reread_independently() {
    let rows = pv1_rows();
    assert!(rows.len() >= 90, "the fixture's rows were read ({} found)", rows.len());
    assert!(rows.iter().any(|r| r.0 == u64::MAX), "the u64::MAX row is in the vector");
    assert!(rows.iter().any(|r| r.1 == 10_000), "a one-slot (10,000) row is in the vector");
    for (l, bps, e) in rows {
        assert_eq!(oracle(l, bps), e, "the FIXTURE disagrees with the reference formula at L={l} bps={bps}");
        assert_eq!(slot_earned(l, bps), e, "slot_earned disagrees with MR-S-PV1 at L={l} bps={bps}");
    }
}

/// The pool's one book and its slots, booked exactly as `claim_creator_fees` / `sweep_stair_payee` book them
/// (`owed −= due; paid += due; slot.paid += due`).
struct Book {
    owed: u64,
    paid: u64,
    slot_paid: Vec<u64>,
    bps: Vec<u16>,
}
impl Book {
    fn lifetime(&self) -> u64 {
        self.paid + self.owed
    }
    fn claim(&mut self, i: usize) -> u64 {
        let due = slot_due(self.lifetime(), self.bps[i], self.slot_paid[i], self.owed);
        assert!(due <= self.owed, "a claim above owed");
        self.owed -= due;
        self.paid += due;
        self.slot_paid[i] += due;
        due
    }
}

#[test]
fn mr_review_random_books_conserve_and_leave_dust_below_n() {
    let mut r = Rng(0xD1B5_4A32_D192_ED03);
    for case in 0..400 {
        let n = 1 + (case % 5) as usize;
        let bps = random_split(&mut r, n);
        // some slots stay "pending" (never claim) until the very end, as a pending handle does
        let pending: Vec<bool> = (0..n).map(|_| r.below(4) == 0).collect();
        let mut b = Book { owed: 0, paid: 0, slot_paid: vec![0; n], bps: bps.clone() };
        let scale = [1u64, 7, 1_000, 1_000_000_007, 1 << 40][r.below(5) as usize];
        for _ in 0..300 {
            b.owed += r.below(scale) + r.below(3);
            for _ in 0..r.below(n as u64 + 1) {
                let i = r.below(n as u64) as usize;
                if !pending[i] {
                    b.claim(i);
                }
            }
            let l = b.lifetime();
            assert_eq!(b.slot_paid.iter().sum::<u64>(), b.paid, "Σ slot.paid is the pool's paid");
            for i in 0..n {
                assert!(b.slot_paid[i] <= oracle(l, bps[i]), "slot {i} above its earned share");
            }
        }
        // the pending slots resolve; every slot claims (twice: the second must be a no-op)
        for _ in 0..2 {
            for i in 0..n {
                b.claim(i);
            }
        }
        let l = b.lifetime();
        for i in 0..n {
            assert_eq!(b.slot_paid[i], oracle(l, bps[i]), "case {case}: slot {i} holds exactly earned(L)");
            assert_eq!(b.claim(i), 0, "nothing left for slot {i}");
        }
        assert!(b.owed <= n as u64 - 1, "case {case}: dust {} with {n} slots at L={l} (bps {bps:?})", b.owed);
        if n == 1 {
            assert_eq!(b.owed, 0, "one slot at 10,000 leaves no dust (D2)");
        }
    }
}

#[test]
fn mr_review_due_saturates_caps_and_never_panics() {
    let m = u64::MAX;
    // corrupt: paid above earned → 0, whatever is owed
    assert_eq!(slot_due(100, 5_000, 51, 1_000), 0);
    assert_eq!(slot_due(0, 10_000, m, m), 0);
    // nothing owed → 0 whatever is earned
    assert_eq!(slot_due(m, 10_000, 0, 0), 0);
    // capped at owed
    assert_eq!(slot_due(m, 10_000, 0, 5), 5);
    assert_eq!(slot_due(m, 5_000, 0, m), m / 2);
    // the widest exact case: L = u64::MAX, a 1-bps slot
    assert_eq!(slot_due(m, 1, 0, m), m / 10_000);
    for bps in [0u16, 1, 3_333, 9_999, 10_000, 10_001, u16::MAX] {
        for (l, p, o) in [(0, 0, 0), (m, m, m), (m, 0, m), (1, m, 1), (m, m - 1, 1)] {
            let d = slot_due(l, bps, p, o);
            assert!(d <= o);
            assert_eq!(d, oracle(l, bps).saturating_sub(p).min(o));
        }
    }
}

#[test]
fn mr_review_cliff_parts_partition_the_vested_whole() {
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..500 {
        let total = r.below(1 << 50) + 1;
        let k = 1 + r.below(6);
        // a monotone curve V_0 = 0 ≤ V_1 ≤ … ≤ V_k = total
        let mut v: Vec<u64> = (0..k - 1).map(|_| r.below(total + 1)).collect();
        v.sort_unstable();
        v.insert(0, 0);
        v.push(total);
        let n = 1 + r.below(5) as usize;
        let bps = random_split(&mut r, n);
        let mut sum_slots = 0u64;
        for &b in &bps {
            // cliffs claimed one at a time
            let s: u64 = (1..v.len()).map(|j| slot_cliff_part(v[j - 1], v[j], b)).sum();
            assert_eq!(s, oracle(total, b));
            // and skipping cliffs, claimed in two jumps
            let mid = v[r.below(v.len() as u64) as usize];
            assert_eq!(slot_cliff_part(0, mid, b) + slot_cliff_part(mid, total, b), oracle(total, b));
            sum_slots += s;
        }
        assert!(total - sum_slots <= n as u64 - 1, "vesting dust ≤ n − 1");
    }
}

#[test]
fn mr_review_check_codes_first_failing_rule_wins() {
    let w = |b: u8, bps: u16| Slot { bps, platform: 0, key: [b; 32] };
    let h = |b: u8, bps: u16, p: u8| Slot { bps, platform: p, key: [b; 32] };
    // six slots, one with bps 0: rule 1 first
    assert_eq!(check_slots(&[w(1, 0), w(2, 2_000), w(3, 2_000), w(4, 2_000), w(5, 2_000), w(6, 2_000)]), 1);
    // bps 0 AND a zero key AND a bad sum: rule 2 first
    assert_eq!(check_slots(&[w(0, 0), w(2, 5_000)]), 2);
    // a bad sum AND a bad platform: rule 3 first
    assert_eq!(check_slots(&[h(1, 5_000, 9), w(2, 4_000)]), 3);
    // a bad platform AND a duplicate wallet: rule 4 first
    assert_eq!(check_slots(&[h(1, 2_000, 5), w(2, 4_000), w(2, 4_000)]), 4);
    // a duplicate wallet before a duplicate pending id in list order: rule 5
    assert_eq!(check_slots(&[w(2, 2_500), w(2, 2_500), h(3, 2_500, 1), h(3, 2_500, 2)]), 5);
    // a duplicate pending id only: rule 6
    assert_eq!(check_slots(&[h(3, 5_000, 1), h(3, 5_000, 4)]), 6);
    // the same 32 bytes as a wallet AND as a pending id are different identities: valid
    assert_eq!(check_slots(&[w(7, 5_000), h(7, 5_000, 1)]), 0);
    // the widest valid list
    assert_eq!(check_slots(&[w(1, 2_000), h(2, 2_000, 1), w(3, 2_000), h(4, 2_000, 2), w(5, 2_000)]), 0);
    // a single wallet at 10,000 (D2)
    assert_eq!(check_slots(&[w(9, 10_000)]), 0);
    // u16 bps that sum past 10,000 only by wrapping in u16 are still refused (the sum is u32)
    assert_eq!(check_slots(&[w(1, 60_000), w(2, 15_536)]), 3);
}
