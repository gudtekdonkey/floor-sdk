//! The `Swap` value this adapter emits, and the native `floor_launch` instruction data built from it:
//! `buy` for a buy, `sell_back_wsol` for a sell-back.
//!
//! Jupiter's `Swap` enum has no variant for a FLOOR sale at the pinned interface commit, so by
//! default the adapter returns `Swap::Placeholder { data: [OP_BUY] }` or `[OP_SELL]`, the
//! interface's variant for venues not yet integrated into the router. A `Placeholder` swap is not
//! executed by Jupiter's on-chain router; it serves the test kit and any caller that encodes the
//! instruction itself.
//!
//! With the `upstream-variant` feature, built against an interface carrying
//! `upstream/swap-variant.patch`, the adapter emits `Swap::FloorSale { is_buy }` instead (see
//! `upstream/PR.md`). Both encode to the same instruction data.
//!
//! `sell_back_wsol` is `floor_launch`'s stateless sell-back paid as wSOL. Its discriminator is Anchor's
//! default, `sha256("global:sell_back_wsol")[..8]` (`floor_types::disc::launch::SELL_BACK_WSOL`).

use floor_types::disc;
use jupiter_amm_interface::{AmmError, Swap};

/// A sale buy (quote mint -> launch token).
pub const OP_BUY: u8 = 0;
/// A sale sell-back paid as wSOL (launch token -> quote mint).
pub const OP_SELL: u8 = 1;

/// `sha256("global:sell_back_wsol")[..8]`: the instruction's Anchor discriminator.
pub const SELL_BACK_WSOL_DISC: [u8; 8] = disc::launch::SELL_BACK_WSOL;

/// The `Swap` this build emits: `Swap::Placeholder { data: [OP_BUY | OP_SELL] }` by default.
#[cfg(not(feature = "upstream-variant"))]
pub fn swap_variant(is_buy: bool) -> Swap {
    Swap::Placeholder { data: Some(vec![if is_buy { OP_BUY } else { OP_SELL }]) }
}

/// The `Swap` this build emits: `Swap::FloorSale { is_buy }`.
#[cfg(feature = "upstream-variant")]
pub fn swap_variant(is_buy: bool) -> Swap {
    Swap::FloorSale { is_buy }
}

/// The side of a `Swap` this crate emitted: `true` for a buy, `false` for a sell-back.
pub fn side_of(swap: &Swap) -> Result<bool, AmmError> {
    match swap {
        Swap::Placeholder { data: Some(d) } if d.as_slice() == [OP_BUY] => Ok(true),
        Swap::Placeholder { data: Some(d) } if d.as_slice() == [OP_SELL] => Ok(false),
        Swap::Placeholder { data } => {
            Err(AmmError::Custom(format!("floor-sale-amm: Placeholder data must be [{OP_BUY}] (buy) or [{OP_SELL}] (sell), got {data:?}")))
        }
        #[cfg(feature = "upstream-variant")]
        Swap::FloorSale { is_buy } => Ok(*is_buy),
        other => Err(AmmError::Custom(format!("floor-sale-amm: not a FLOOR sale swap: {other:?}"))),
    }
}

/// Check that `swap` is a buy this crate emitted.
pub fn check_buy(swap: &Swap) -> Result<(), AmmError> {
    match side_of(swap)? {
        true => Ok(()),
        false => Err(AmmError::Custom("floor-sale-amm: a sell-back swap, not a buy".into())),
    }
}

/// Check that `swap` is a sell-back this crate emitted.
pub fn check_sell(swap: &Swap) -> Result<(), AmmError> {
    match side_of(swap)? {
        false => Ok(()),
        true => Err(AmmError::Custom("floor-sale-amm: a buy swap, not a sell-back".into())),
    }
}

/// The Anchor / Borsh data of `buy(lamports_in, min_tokens, max_graduation_lamports)`:
///
/// ```text
/// discriminator [8] ‖ lamports_in u64 LE ‖ min_tokens u64 LE ‖ max_graduation_lamports u64 LE
/// ```
///
/// `lamports_in` is the input (the gross amount: the sale price plus the platform fee are taken out
/// of it, never more). `min_tokens` is the minimum out. `max_graduation_lamports` is 0: this adapter
/// never quotes a buy that sells the sale out, and a buy that turns out to (because other buys landed
/// first) fails, as it carries no graduation accounts.
pub fn buy_instruction_data(swap: &Swap, in_amount: u64, min_tokens: u64) -> Result<Vec<u8>, AmmError> {
    check_buy(swap)?;
    let mut data = Vec::with_capacity(8 + 8 + 8 + 8);
    data.extend_from_slice(&disc::launch::BUY);
    data.extend_from_slice(&in_amount.to_le_bytes());
    data.extend_from_slice(&min_tokens.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    Ok(data)
}

/// The Anchor / Borsh data of `sell_back_wsol(tokens)`:
///
/// ```text
/// discriminator [8] ‖ tokens u64 LE
/// ```
///
/// `tokens` is the input (every token sold is taken). There is no minimum out: the payout is
/// `floor(tokens · price_x64 / 2^64)` less `floor(payout · team_bps / 10_000)`, a function of the input
/// and two numbers fixed for the life of the sale, so it is the quoted amount or the instruction fails
/// (the sale ended, or the seller's position does not cover `tokens`).
pub fn sell_instruction_data(swap: &Swap, tokens: u64) -> Result<Vec<u8>, AmmError> {
    check_sell(swap)?;
    let mut data = Vec::with_capacity(8 + 8);
    data.extend_from_slice(&SELL_BACK_WSOL_DISC);
    data.extend_from_slice(&tokens.to_le_bytes());
    Ok(data)
}

/// The instruction data for either side, from the `Swap` and the input amount: `buy` with no minimum
/// out, or `sell_back_wsol`. The test kit's encoder.
///
/// A buy is built with `min_tokens = 0`. That is safe on a bonding sale because the price is fixed for
/// the sale's whole life (`Launch.price_x64` never changes): `tokens` is a function of `in_amount` and
/// that price alone, so no earlier transaction can move the price against this one. The only way to
/// receive fewer tokens than quoted is a smaller remaining supply, and then the buy takes only the
/// cost of what it receives at the same price (`spent = cost + fee` of the tokens delivered, never
/// `in_amount`); a buy that would sell the sale out fails (it carries no graduation accounts), and one
/// that would buy nothing fails `Nothing`. A caller that wants a floor on the fill size uses
/// [`buy_instruction_data`] with its own `min_tokens`.
pub fn instruction_data(swap: &Swap, in_amount: u64) -> Result<Vec<u8>, AmmError> {
    if side_of(swap)? {
        buy_instruction_data(swap, in_amount, 0)
    } else {
        sell_instruction_data(swap, in_amount)
    }
}
