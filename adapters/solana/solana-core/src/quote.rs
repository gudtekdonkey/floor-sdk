// SPDX-License-Identifier: Apache-2.0
use floor_math::v2::book::{Book, Undo, SOL_BUY_WALK_STEPS};
use floor_math::v2::fees::{fee_total, LgCache};
use floor_math::v2::split::SplitRow;
use floor_math::v2::supply::Supply;
use floor_math::v2::trade::{self, BookBuy, BookSell};
use floor_math::MathError;

use crate::layout::{LaunchV2Account, STATUS_LIVE_V2};
use crate::quote_mint::QuoteMint;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuoteError {
    NotLive(u8),
    TransferFeeTakesAll,
    Overflow,
    Math(MathError),
}

impl core::fmt::Display for QuoteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            QuoteError::NotLive(s) => write!(f, "launch status {s} is not live"),
            QuoteError::TransferFeeTakesAll => f.write_str("the quote mint's transfer fee takes the whole amount (QuoteTransferShort)"),
            QuoteError::Overflow => f.write_str("an amount does not fit u64"),
            QuoteError::Math(e) => write!(f, "floor-math refused: {e:?}"),
        }
    }
}

impl std::error::Error for QuoteError {}

impl From<MathError> for QuoteError {
    fn from(e: MathError) -> Self {
        QuoteError::Math(e)
    }
}

fn to64(v: u128) -> Result<u64, QuoteError> {
    u64::try_from(v).map_err(|_| QuoteError::Overflow)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwapQuote {
    pub in_amount: u64,
    pub out_amount: u64,
    pub fee_amount: u64,
    pub fee_base: u128,
    pub not_enough_liquidity: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct QuoteState {
    status: u8,
    book: Book,
    split: SplitRow,
    supply: Supply,
    quote_mint: QuoteMint,
    lg: LgCache,
}

impl QuoteState {
    pub fn new(l: &LaunchV2Account, quote_mint: QuoteMint) -> QuoteState {
        let book = l.book();
        let mut lg = LgCache::new();
        book.warm_lg(&mut lg);
        QuoteState { status: l.status, book, split: l.split_row(), supply: l.supply(), quote_mint, lg }
    }

    pub fn book(&self) -> &Book {
        &self.book
    }

    pub fn quote_mint(&self) -> &QuoteMint {
        &self.quote_mint
    }

    fn live(&self) -> Result<(), QuoteError> {
        if self.status == STATUS_LIVE_V2 {
            Ok(())
        } else {
            Err(QuoteError::NotLive(self.status))
        }
    }

    pub fn simulate_buy(&self, gross: u64) -> Result<BookBuy, QuoteError> {
        self.live()?;
        let (mut b, mut u) = (self.book, Undo::EMPTY);
        Ok(trade::buy_steps_with_memo(&mut b, &self.split, self.supply, gross as u128, 0, SOL_BUY_WALK_STEPS, &mut u, &mut self.lg.peek())?)
    }

    pub fn simulate_sell(&self, tokens: u64) -> Result<BookSell, QuoteError> {
        self.live()?;
        Ok(trade::quote_sell_with_memo(&self.book, &self.split, self.supply, tokens, &mut self.lg.peek())?)
    }

    pub fn quote(&self, is_buy: bool, amount: u64, epoch: u64) -> Result<SwapQuote, QuoteError> {
        if is_buy {
            let withheld = self.quote_mint.transfer_fee(epoch, amount);
            if withheld >= amount {
                return Err(QuoteError::TransferFeeTakesAll);
            }
            self.live()?;
            let r = trade::quote_buy_lite(&self.book, &self.split, self.supply, (amount - withheld) as u128, SOL_BUY_WALK_STEPS, false, &mut self.lg.peek())?;
            let fees = fee_total(&r.fees);
            let spent = amount - self.quote_mint.received(epoch, to64(r.refund)?);
            Ok(SwapQuote { in_amount: spent, out_amount: r.tokens, fee_amount: to64(fees)?, fee_base: spent as u128, not_enough_liquidity: r.refund > 0 })
        } else {
            self.live()?;
            let r = trade::quote_sell_lite(&self.book, &self.split, self.supply, amount, false, &mut self.lg.peek())?;
            let fees = fee_total(&r.fees);
            Ok(SwapQuote {
                in_amount: r.tokens_in - r.unfilled,
                out_amount: self.quote_mint.received(epoch, to64(r.out)?),
                fee_amount: to64(fees)?,
                fee_base: r.gross,
                not_enough_liquidity: r.unfilled > 0,
            })
        }
    }
}
