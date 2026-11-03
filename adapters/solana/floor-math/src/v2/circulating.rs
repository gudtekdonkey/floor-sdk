// SPDX-License-Identifier: Apache-2.0
use super::V2Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BidHoldings {
    pub c: u128,
    pub k: u128,
    pub f: u128,
}

pub fn circulating(n_live: u128, for_sale: u128, held: BidHoldings, dead: u128) -> Result<u128, V2Error> {
    let out = for_sale
        .checked_add(held.c)
        .and_then(|x| x.checked_add(held.k))
        .and_then(|x| x.checked_add(held.f))
        .and_then(|x| x.checked_add(dead))
        .ok_or(V2Error::Overflow)?;
    n_live.checked_sub(out).ok_or(V2Error::Overflow)
}

pub fn sweep_if_empty(s: u128, f: u128, c: u128, k: u128) -> Result<(u128, (u128, u128, u128)), V2Error> {
    if s > 0 {
        return Ok((0, (f, c, k)));
    }
    let rest = f.checked_add(c).and_then(|x| x.checked_add(k)).ok_or(V2Error::Overflow)?;
    Ok((rest, (0, 0, 0)))
}

pub fn sweep_unsettled(b: &mut super::book::Book) -> Result<u128, V2Error> {
    let n = b.n_ranges as usize;
    let mut moved: u128 = 0;
    for r in b.ranges[..n].iter() {
        moved = moved.checked_add(r.place).ok_or(V2Error::Overflow)?;
    }
    if moved == 0 {
        return Ok(0);
    }
    for r in b.ranges[..n].iter_mut() {
        r.place = 0;
    }
    b.sqrt_l_max = b.sqrt_l_max.max(b.sqrt_p);
    Ok(moved)
}

pub const EVM_DEAD: [[u8; 20]; 4] = [
    [0; 20],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xde, 0xad],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    [0xde, 0xad, 0, 0, 0, 0, 0, 0, 0, 0, 0x42, 0x06, 0x94, 0x20, 0x69, 0x42, 0x06, 0x94, 0x20, 0x69],
];

pub fn is_evm_dead(a: &[u8; 20]) -> bool {
    EVM_DEAD.iter().any(|d| d == a)
}

pub const SOLANA_INCINERATOR: [u8; 32] = [
    0x00, 0x33, 0x90, 0x72, 0x8d, 0x34, 0x11, 0x60, 0x79, 0xbd, 0xc9, 0x11, 0xbf, 0xff, 0x00, 0xdb, 0xd4, 0x4d, 0x2e, 0xcd, 0xcc, 0xf7, 0x9c, 0xa6, 0xe1, 0x00, 0x38,
    0xe1, 0x00, 0x00, 0x00, 0x00,
];
pub const SOLANA_SYSTEM_PROGRAM: [u8; 32] = [0; 32];

pub fn is_solana_dead_holder(holder: &[u8; 32]) -> bool {
    *holder == SOLANA_INCINERATOR || *holder == SOLANA_SYSTEM_PROGRAM
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s_is_what_is_left_after_the_program_and_the_dead() {
        let h = BidHoldings { c: 10, k: 20, f: 30 };
        assert_eq!(circulating(1_000, 400, h, 5), Ok(535));
        assert_eq!(circulating(900, 400, h, 5), Ok(435));
        assert_eq!(circulating(10, 11, BidHoldings::default(), 0), Err(V2Error::Overflow));
        assert_eq!(circulating(u128::MAX, u128::MAX, h, 0), Err(V2Error::Overflow));
    }

    #[test]
    fn all_burned_sends_the_pots_to_the_platform() {
        assert_eq!(sweep_if_empty(1, 5, 6, 7), Ok((0, (5, 6, 7))));
        assert_eq!(sweep_if_empty(0, 5, 6, 7), Ok((18, (0, 0, 0))));
    }

    #[test]
    fn all_burned_sends_the_unsettled_money_to_the_platform() {
        use crate::v2::book::{Book, KIND_BONDING, KIND_CLIMB, KIND_TAIL};
        let mut b = Book::empty(1 << 72, [0; 5]);
        b.n_ranges = 3;
        let e = [1u128 << 72, 1 << 72, 3 << 71, crate::v2::sqrt::SQRT_TAIL_END];
        let kinds = [KIND_BONDING, KIND_CLIMB, KIND_TAIL];
        for i in 0..3 {
            let r = &mut b.ranges[i];
            (r.sqrt_a, r.sqrt_b, r.tokens, r.ask_left, r.kind) = (e[i], e[i + 1], 1_000, 1_000, kinds[i]);
        }
        (b.ranges[0].ask_left, b.ranges[0].place) = (500, 500);
        (b.sqrt_p, b.sqrt_l_max) = (e[0], e[0]);
        b.check().unwrap();
        assert_eq!(sweep_unsettled(&mut b), Ok(500));
        assert!(b.ranges[..3].iter().all(|r| r.place == 0));
        assert_eq!((b.sqrt_l_max, b.sqrt_p), (1 << 72, 1 << 72));
        assert_eq!((b.ranges[0].ask_left, b.ranges[1].ask_left), (500, 1_000));
        b.check().unwrap();
        assert_eq!(sweep_unsettled(&mut b), Ok(0));
        assert_eq!(b.sqrt_l_max, 1 << 72);
        let mut u = crate::v2::book::Undo::EMPTY;
        u.start(&b);
        let up = b.walk_up(300, &mut u).unwrap();
        assert!(up.tokens > 0 && up.money <= 300);
        let down = b.walk_down(up.tokens, &mut u).unwrap();
        assert!(down.money <= up.money, "{down:?} {up:?}");
        assert_eq!(b.ranges[..3].iter().map(|r| r.place).sum::<u128>(), up.money - down.money);
    }

    #[test]
    fn the_dead_lists_are_the_spec_lists() {
        let mut a = [0u8; 20];
        a[18] = 0xde;
        a[19] = 0xad;
        assert!(is_evm_dead(&[0; 20]) && is_evm_dead(&a));
        let mut one = [0u8; 20];
        one[19] = 1;
        assert!(is_evm_dead(&one));
        let hex = "dead000000000000000042069420694206942069";
        let mut z = [0u8; 20];
        for i in 0..20 {
            z[i] = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
        }
        assert!(is_evm_dead(&z));
        let mut base = [0xde, 0xad, 0xde, 0xad, 0xde, 0xad, 0xde, 0xad, 0xde, 0xad, 0xde, 0xad, 0xde, 0xad, 0xde, 0xad, 0xde, 0xad, 0, 1];
        assert!(!is_evm_dead(&base));
        base[0] = 0x36;
        assert!(!is_evm_dead(&base));
        assert!(is_solana_dead_holder(&SOLANA_SYSTEM_PROGRAM) && is_solana_dead_holder(&SOLANA_INCINERATOR));
        assert!(!is_solana_dead_holder(&[1; 32]));
    }
}
