//! FLOOR's account bytes → layouts. The AMM's (pool, tick arrays, bitmap) are
//! `carpenter_quote::state`, re-exported here.
//!
//! ⛔ Decoding copies (`pod_read_unaligned`): account bytes handed in from JS or read off an RPC are
//! not always at an 8-byte alignment.

use floor_types::layout::{FloorPool, Launch, Layout};

pub use carpenter_quote::state::*;

use crate::error::{QuoteError, QuoteResult};

/// A `FloorPool` ACCOUNT may be LONGER than the struct — 1,008 B with the pool-price TWAP tail
/// (`floor_types::twap`), 792 B without it. The struct is the first 792 bytes either way; anything
/// shorter is refused.
pub fn decode_floor_pool(data: &[u8]) -> QuoteResult<FloorPool> {
    if data.len() < FloorPool::ACCOUNT_LEN || data[..8] != FloorPool::DISCRIMINATOR {
        return Err(QuoteError::QuoteBadAccount(FloorPool::NAME));
    }
    Ok(bytemuck::pod_read_unaligned(&data[8..FloorPool::ACCOUNT_LEN]))
}

/// The cushion top a FLOOR pool's last ratchet cached in its `FloorPool` ACCOUNT's TWAP tail, off the
/// SAME bytes [`decode_floor_pool`] reads — `floor_types::twap::TwapOracle::cushion_top_of_account`,
/// the one spelling the program's `before_swap` reads too. `None` without a tail or with no cushion
/// cached. Feed it to `SwapQuoteParams::cushion_top` (the gap buy skim; its switch ships off, and
/// then the program never writes it, so it decodes `None` and the quote is unchanged).
pub fn decode_cushion_top(data: &[u8]) -> Option<i32> {
    floor_types::twap::TwapOracle::cushion_top_of_account(data)
}

pub fn decode_launch(data: &[u8]) -> QuoteResult<Launch> {
    decode_account::<Launch>(data)
}

#[cfg(test)]
mod twap_tail_tests {
    use super::*;
    use bytemuck::Zeroable;

    /// A `FloorPool` account with the TWAP tail (1,008 B) decodes to the same struct as the bare 792 B
    /// one; a short account is still refused, and the generic decoder stays exact.
    #[test]
    fn a_floor_pool_with_the_twap_tail_decodes_and_a_short_one_does_not() {
        let mut fp = FloorPool::zeroed();
        fp.floor_tick = 4_242;
        let bare = encode_account(&fp);
        let mut long = bare.clone();
        long.resize(floor_types::twap::FLOOR_POOL_ACCOUNT_LEN_WITH_TWAP, 0xAB);
        assert_eq!(decode_floor_pool(&bare).unwrap().floor_tick, 4_242);
        assert_eq!(decode_floor_pool(&long).unwrap().floor_tick, 4_242);
        assert!(matches!(decode_floor_pool(&bare[..bare.len() - 1]), Err(QuoteError::QuoteBadAccount("FloorPool"))));
        assert!(decode_account::<FloorPool>(&long).is_err(), "the generic decoder stays exact");
    }
}
