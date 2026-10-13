//! `floor-sale-amm`: a [`jupiter_amm_interface::Amm`] implementation for FLOOR launch sales.
//!
//! A FLOOR launch on Solana sells a fixed share of its supply at one fixed price through the
//! `floor_launch` program before its Carpenter pool exists. The buy that sells the sale out
//! graduates the launch inside the same instruction: it creates the pool, seeds its positions, and
//! from then on the token trades on Carpenter (`carpenter-amm`, label `"Carpenter"`). This crate is
//! the other venue, the sale itself.
//!
//! - [`amm::FloorSaleAmm`]: the `Amm` impl. One market per `Launch` account. It quotes buys through
//!   [`floor_quote::sale_buy`] (the program's arithmetic) and lists the `buy` instruction's accounts.
//!   It also quotes sell-backs (launch token → wSOL) through [`floor_quote::sale_sell_back_stateless`]
//!   and lists the accounts of `floor_launch`'s `sell_back_wsol`: a sell-back that pays
//!   `floor(tokens · price_x64 / 2^64)` less the platform fee, independent of the seller's history,
//!   into the seller's wSOL account.
//! - [`accounts`]: the program-derived addresses a buy or a sell-back names.
//! - [`encode`]: the `Swap` value this adapter emits and the native instruction data.
//!
//! What a buy requires of the transaction around it (all enforced by the program, and checked by
//! [`amm::FloorSaleAmm::get_swap_and_account_metas`] where the adapter can see it):
//!
//! - the input is native SOL taken from the signing wallet by a System transfer, not a token
//!   account;
//! - the tokens are delivered into the signing wallet's own Token-2022 associated token account for
//!   the mint, and nowhere else. The mint stays paused (Token-2022 `Pausable`) until graduation; the
//!   program resumes it only around its own transfer. No other transfer of the token succeeds while
//!   the sale runs, so the token cannot be an intermediate of a route;
//! - the buy must not sell the sale out: the graduating buy needs the graduation accounts, which this
//!   adapter does not carry, so such an amount is refused.
//!
//! What a sell-back requires (`sell_back_wsol`):
//!
//! - the tokens are taken from the signing wallet's own Token-2022 associated token account (the only
//!   account the paused token can leave, and only through the program), so a sell-back is the first
//!   hop of a route;
//! - the signer holds a sale position covering the tokens (every holder does: the token cannot be
//!   transferred during the sale, so a holder bought it);
//! - the payout is delivered as synced wSOL into the signing wallet's SPL Token wSOL account, from
//!   which the route continues.
//!
//! Nothing here reads a clock, the network, a file or a random source.
#![forbid(unsafe_code)]

pub mod accounts;
pub mod amm;
pub mod encode;

pub use amm::FloorSaleAmm;
pub use encode::{buy_instruction_data, instruction_data, sell_instruction_data, OP_BUY, OP_SELL};

/// `Amm::label()`. Jupiter's test kit derives the program fixture path from the label (lowercased,
/// spaces to underscores), so this one expects `tests/fixtures/floor.so`. Keep it stable.
pub const LABEL: jupiter_amm_interface::AmmLabel = "FLOOR";

/// The `buy` instruction's named accounts, `buyer` through `program`. A buy that does not sell the
/// sale out on a launch quoted in wSOL carries no remaining accounts.
pub const BUY_METAS: usize = 13;

/// `sell_back_wsol`'s accounts on an unhooked launch: 11 named, then `[seller_wsol, spl_token]`.
pub const SELL_METAS: usize = 13;

/// ... and on a hooked launch, with the 4-account holder tail.
pub const SELL_METAS_HOOKED: usize = 17;
