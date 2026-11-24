// SPDX-License-Identifier: Apache-2.0
use super::V2Error;

const Q64: u32 = 64;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HolderBook {
    pub index: u128,
    pub w: u64,
    pub carry: u64,
    pub booked_total: u64,
    pub paid_total: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Position {
    pub amount: u64,
    pub owed: u64,
    pub index_snap: u128,
    pub opted_out: bool,
}

impl Position {
    pub fn opted_out() -> Position {
        Position { opted_out: true, ..Position::default() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Payout {
    Indexed,
    Carried,
}

fn earned(amount: u64, d_index: u128) -> u128 {
    let a = amount as u128;
    let (hi, lo) = (d_index >> Q64, d_index & u64::MAX as u128);
    a * hi + ((a * lo) >> Q64)
}

impl HolderBook {
    pub fn new() -> HolderBook {
        HolderBook::default()
    }

    pub fn add_reward(&mut self, amount: u64, _s_circ: u64) -> Result<Payout, V2Error> {
        self.booked_total = self.booked_total.checked_add(amount).ok_or(V2Error::Overflow)?;
        let total = self.carry.checked_add(amount).ok_or(V2Error::Overflow)?;
        let w = self.w;
        if w == 0 || total == 0 {
            self.carry = total;
            return Ok(Payout::Carried);
        }
        let step = ((total as u128) << Q64) / w as u128;
        let out = ((step * w as u128) + (1u128 << Q64) - 1) >> Q64;
        self.index = self.index.checked_add(step).ok_or(V2Error::Overflow)?;
        self.carry = total - out.min(total as u128) as u64;
        Ok(Payout::Indexed)
    }

    pub fn settle(&self, p: &mut Position) {
        if !p.opted_out {
            p.owed += earned(p.amount, self.index - p.index_snap) as u64;
        }
        p.index_snap = self.index;
    }

    pub fn add(&mut self, p: &mut Position, amount: u64) -> Result<(), V2Error> {
        self.settle(p);
        p.amount = p.amount.checked_add(amount).ok_or(V2Error::Overflow)?;
        if !p.opted_out {
            self.w = self.w.checked_add(amount).ok_or(V2Error::Overflow)?;
        }
        Ok(())
    }

    pub fn sub(&mut self, p: &mut Position, amount: u64) -> Result<(), V2Error> {
        if amount > p.amount {
            return Err(V2Error::NotHeld);
        }
        self.settle(p);
        p.amount -= amount;
        if !p.opted_out {
            self.w -= amount;
        }
        Ok(())
    }

    pub fn sync(&mut self, p: &mut Position, real: u64) -> Result<(), V2Error> {
        if real > p.amount {
            return self.add(p, real - p.amount);
        }
        if real == p.amount {
            return Ok(());
        }
        if !p.opted_out {
            let d = self.index - p.index_snap;
            let (all, kept) = (earned(p.amount, d), earned(real, d));
            p.owed += kept as u64;
            self.carry = self.carry.checked_add((all - kept) as u64).ok_or(V2Error::Overflow)?;
            self.w -= p.amount - real;
        }
        p.index_snap = self.index;
        p.amount = real;
        Ok(())
    }

    pub fn opt_in(&mut self, p: &mut Position) {
        if !p.opted_out {
            return;
        }
        p.opted_out = false;
        p.index_snap = self.index;
        self.w += p.amount;
    }

    pub fn take(&mut self, p: &mut Position) -> u64 {
        self.settle(p);
        let x = p.owed;
        p.owed = 0;
        self.paid_total += x;
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    #[test]
    fn payouts_are_pro_rata_and_carry_while_nobody_holds() {
        let mut b = HolderBook::new();
        assert_eq!(b.add_reward(500, u64::MAX).unwrap(), Payout::Carried);
        let (mut a, mut c) = (Position::default(), Position::default());
        b.add(&mut a, 1_000).unwrap();
        b.add(&mut c, 3_000).unwrap();
        assert_eq!(b.add_reward(500, u64::MAX).unwrap(), Payout::Indexed);
        assert_eq!(b.take(&mut a), 250);
        assert_eq!(b.take(&mut c), 750);
        assert_eq!(b.carry, 0);
    }

    #[test]
    fn stale_high_w_reward_never_overpays() {
        let mut b = HolderBook::new();
        let (mut a, mut c) = (Position::default(), Position::default());
        b.add(&mut a, 1_000).unwrap();
        b.add(&mut c, 1_000).unwrap();
        assert_eq!(b.add_reward(2_000, 1_001).unwrap(), Payout::Indexed);
        assert_eq!(b.take(&mut c), 1_000);
        b.sync(&mut a, 1).unwrap();
        assert_eq!((a.owed, b.carry, b.w), (1, 999, 1_001));
        assert_eq!(b.take(&mut a), 1);
        assert!(b.paid_total + b.carry <= b.booked_total);
        b.add_reward(2, 1_001).unwrap();
        assert_eq!((b.take(&mut a), b.take(&mut c)), (1, 1_000));
        assert!(b.paid_total + b.carry <= b.booked_total);
    }

    #[test]
    fn a_snipe_earns_its_pro_rata_share() {
        let mut b = HolderBook::new();
        let (mut h, mut s) = (Position::default(), Position::default());
        b.add(&mut h, 100).unwrap();
        b.add(&mut s, 1_000_000).unwrap();
        b.add_reward(10_000, u64::MAX).unwrap();
        b.sub(&mut s, 1_000_000).unwrap();
        assert_eq!(b.take(&mut s), 9_999);
        assert_eq!(b.take(&mut h), 0);
        assert!(b.paid_total + b.carry <= b.booked_total);
    }

    #[test]
    fn dca_and_stranger_dust_earn_by_balance() {
        let mut b = HolderBook::new();
        let (mut p, mut o) = (Position::default(), Position::default());
        for _ in 0..24 {
            b.add(&mut p, 10).unwrap();
        }
        b.add(&mut o, 240).unwrap();
        b.add(&mut o, 1).unwrap();
        b.add_reward(481, u64::MAX).unwrap();
        assert_eq!(b.take(&mut p), 240);
        assert_eq!(b.take(&mut o), 241);
    }

    #[test]
    fn an_opted_out_contract_earns_nothing_until_it_opts_in() {
        let mut b = HolderBook::new();
        let (mut pool, mut h) = (Position::opted_out(), Position::default());
        b.add(&mut pool, 9_000).unwrap();
        b.add(&mut h, 1_000).unwrap();
        assert_eq!(b.w, 1_000);
        b.add_reward(1_000, 10_000).unwrap();
        assert_eq!(b.take(&mut pool), 0);
        assert_eq!(b.take(&mut h), 1_000);
        b.opt_in(&mut pool);
        assert_eq!(b.w, 10_000);
        b.add_reward(1_000, 10_000).unwrap();
        assert_eq!(b.take(&mut pool), 899);
    }

    #[test]
    fn sync_returns_the_missing_tokens_share_to_every_holder() {
        let mut b = HolderBook::new();
        let (mut a, mut c) = (Position::default(), Position::default());
        b.add(&mut a, 1_000).unwrap();
        b.add(&mut c, 1_000).unwrap();
        b.add_reward(2_000, u64::MAX).unwrap();
        b.sync(&mut a, 500).unwrap();
        assert_eq!((a.amount, a.owed, b.w, b.carry), (500, 500, 1_500, 500));
        assert_eq!(b.take(&mut a), 500);
        assert_eq!(b.take(&mut c), 1_000);
        b.add_reward(1_000, u64::MAX).unwrap();
        assert_eq!((b.take(&mut a), b.take(&mut c)), (500, 1_000));
        assert!(b.paid_total + b.carry <= b.booked_total);
    }

    #[test]
    fn sync_to_the_same_balance_is_a_no_op_and_a_rise_is_an_add() {
        let mut b = HolderBook::new();
        let mut a = Position::default();
        b.add(&mut a, 1_000).unwrap();
        b.add_reward(100, u64::MAX).unwrap();
        let (b0, a0) = (b.clone(), a);
        b.sync(&mut a, 1_000).unwrap();
        assert_eq!((&b, a), (&b0, a0));
        b.sync(&mut a, 1_500).unwrap();
        assert_eq!((a.amount, a.owed, b.w), (1_500, 99, 1_500));
    }

    #[test]
    fn sync_stale_window_pays_the_burner_zero_on_burned_tokens() {
        let mut b = HolderBook::new();
        let (mut a, mut c) = (Position::default(), Position::default());
        b.add(&mut a, 1_000).unwrap();
        b.add(&mut c, 1_000).unwrap();
        b.add_reward(1_000, 1_001).unwrap();
        b.sync(&mut a, 1).unwrap();
        assert_eq!(b.take(&mut a), 0, "1 token × 0.5, rounded against the claimer");
        assert_eq!((b.take(&mut c), b.carry), (500, 500));
    }

    #[test]
    fn carry_snipe_after_stall_is_gone() {
        let mut b = HolderBook::new();
        let (mut h, mut s) = (Position::default(), Position::default());
        b.add(&mut h, 10_000).unwrap();
        for _ in 0..200 {
            assert_eq!(b.add_reward(101, 9_999).unwrap(), Payout::Indexed);
        }
        assert!(b.carry <= 1, "nothing waits for a later buyer: carry {}", b.carry);
        b.add(&mut s, 10_000).unwrap();
        b.sync(&mut h, 9_999).unwrap();
        assert!(b.carry <= 4, "carry {}", b.carry);
        b.add_reward(101, 19_999).unwrap();
        let (got_s, got_h) = (b.take(&mut s), b.take(&mut h));
        assert!(got_s <= 53, "the late buyer takes about half of the next payout only: {got_s}");
        assert!(got_h >= 20_200 - 200 - 4, "the holder keeps the 200 payouts: {got_h}");
        assert!(b.paid_total + b.carry <= b.booked_total);
    }

    fn fuzz(seed: u64, holders: usize, ops: usize, contracts: usize) {
        let mut r = Rng(seed);
        let mut b = HolderBook::new();
        let mut ps: Vec<Position> =
            (0..holders).map(|k| if k < contracts { Position::opted_out() } else { Position::default() }).collect();
        let mut supply: u64 = 0;
        let mut events: u64 = 0;
        let mut real = vec![0u64; holders];
        for _ in 0..ops {
            let i = r.below(holders as u64) as usize;
            let op = r.below(12);
            if op != 7 && op != 9 && op != 10 {
                b.sync(&mut ps[i], real[i]).unwrap();
            }
            match op {
                0..=3 => {
                    let big = r.below(5) == 0;
                    let a = 1 + r.below(if big { 1_000_000_000 } else { 1_000 });
                    b.add(&mut ps[i], a).unwrap();
                    supply += a;
                    real[i] += a;
                }
                4..=5 => {
                    let a = r.below(ps[i].amount + 1);
                    b.sub(&mut ps[i], a).unwrap();
                    supply -= a;
                    real[i] -= a;
                }
                6 => {
                    let j = r.below(holders as u64) as usize;
                    let a = r.below(ps[i].amount + 1);
                    if i != j {
                        b.sync(&mut ps[j], real[j]).unwrap();
                        b.sub(&mut ps[i], a).unwrap();
                        b.add(&mut ps[j], a).unwrap();
                        real[i] -= a;
                        real[j] += a;
                    }
                }
                7 => {
                    b.add_reward(r.below(1_000_000), supply).unwrap();
                    events += 1;
                }
                8 => {
                    if r.below(20) == 0 {
                        b.opt_in(&mut ps[i]);
                    } else {
                        b.add_reward(r.below(100), supply).unwrap();
                        events += 1;
                    }
                }
                9 => {
                    let a = r.below(real[i] + 1);
                    real[i] -= a;
                    supply -= a;
                }
                10 => {
                    b.sync(&mut ps[i], real[i]).unwrap();
                }
                _ => {
                    b.take(&mut ps[i]);
                }
            }
            let owed: u64 = ps.iter().map(|p| p.owed).sum();
            assert!(b.paid_total + b.carry + owed <= b.booked_total, "paid + carry + owed > booked");
            let w: u64 = ps.iter().filter(|p| !p.opted_out).map(|p| p.amount).sum();
            assert_eq!(b.w, w);
            assert_eq!(real.iter().sum::<u64>(), supply);
            assert!(ps.iter().zip(&real).map(|(p, &x)| p.amount.min(x)).sum::<u64>() <= supply);
            assert!(ps.iter().zip(&real).all(|(p, &x)| p.amount >= x));
            assert!(ps.iter().filter(|p| p.opted_out).all(|p| p.owed == 0));
        }
        for (p, &x) in ps.iter_mut().zip(&real) {
            b.sync(p, x).unwrap();
            b.take(p);
        }
        assert!(b.paid_total <= b.booked_total);
        let dust = b.booked_total - b.paid_total - b.carry;
        assert!(dust <= events * holders as u64, "dust {} events {}", dust, events);
    }

    #[test]
    fn fuzz_100k_ops_paid_never_exceeds_booked() {
        fuzz(0x9e37_79b9_7f4a_7c15, 40, 100_000, 0);
        fuzz(0x9e37_79b9_7f4a_7c15, 40, 100_000, 6);
    }

    #[test]
    fn fuzz_burns_and_syncs_conserve_money() {
        for seed in 1..=16u64 {
            fuzz(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0x64, 4, 20_000, 0);
        }
    }

    #[test]
    fn fuzz_seeds_with_and_without_opted_out_contracts() {
        for seed in 1..=8u64 {
            fuzz(seed.wrapping_mul(0x2545_f491_4f6c_dd1d), 8, 12_500, 0);
            fuzz(seed.wrapping_mul(0x2545_f491_4f6c_dd1d), 8, 12_500, 3);
        }
    }
}
