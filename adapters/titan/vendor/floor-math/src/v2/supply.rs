// SPDX-License-Identifier: Apache-2.0
use super::book::Book;
use crate::MathError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Supply {
    pub live: u64,
    pub dead: u64,
}

pub fn s_circ(b: &Book, sup: Supply) -> Result<u64, MathError> {
    circulating(sup.live, 0, b.for_sale(), b.bought_back(), sup.dead)
}

pub fn circulating(minted: u64, burned: u64, for_sale: u64, bought_back: u64, dead: u64) -> Result<u64, MathError> {
    minted
        .checked_sub(burned)
        .and_then(|x| x.checked_sub(for_sale))
        .and_then(|x| x.checked_sub(bought_back))
        .and_then(|x| x.checked_sub(dead))
        .ok_or(MathError::Overflow)
}

pub fn s_below(s: u64, x_c: u64, x_k: u64) -> u64 {
    s.saturating_sub(x_c).saturating_sub(x_k).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circulating_and_s_below() {
        assert_eq!(circulating(1_000, 100, 300, 50, 25), Ok(525));
        assert_eq!(circulating(1_000, 100, 900, 1, 0), Err(MathError::Overflow));
        assert_eq!(s_below(10, 3, 4), 3);
        assert_eq!(s_below(10, 30, 4), 1);
    }
}
