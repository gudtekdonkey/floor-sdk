//! The bonding sale: `buy` and `sell_back` quotes against a `Launch`.
//!
//! ⛔ Every sale buy and sell-back pays the platform fee. `lamports_in` is the GROSS cap; the arithmetic is
//! floor-math's (`mech::sale_buy_with_team` / `sell_back_with_team`). The fee is on top: it never counts toward
//! `raised` / `contributed`, and a zero-rate launch (`team_bps == 0`) quotes no fee.

use floor_types::constants::LAUNCH_STATE_BONDING;
use floor_types::layout::{BuyerPosition, Launch};

use crate::error::{FloorError, QuoteError, QuoteResult};
use crate::mech;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaleBuyQuote {
    pub tokens: u64,
    /// The sale-price part, ceil(tokens·price_x64/2^64): what `raised`/`contributed` grow by.
    pub cost: u64,
    /// floor(cost·team_bps/1e4), on top.
    pub team_fee: u64,
    /// What leaves the buyer: cost + team_fee <= lamports_in.
    pub spent: u64,
    /// This buy sells the sale out, so it graduates inline and the buyer pays graduation rent,
    /// bounded by `max_graduation_lamports`.
    pub graduates: bool,
}

/// `buy` steps 1–2 under the platform fee: the walk runs on the NET of the gross cap,
/// `tokens = sale_tokens_for(team_net_of(lamports_in), price_x64, sale_tokens − sold)`, and the fee
/// is floored on the cost. `min_tokens` is the caller's slippage check and is not applied here.
pub fn sale_buy(launch: &Launch, lamports_in: u64) -> QuoteResult<SaleBuyQuote> {
    if launch.state != LAUNCH_STATE_BONDING {
        return Err(FloorError::SaleOver.into());
    }
    let remaining = launch.sale_tokens.checked_sub(launch.sold).ok_or(QuoteError::Overflow)?;
    let (tokens, cost, team_fee) = mech::sale_buy_with_team(lamports_in, launch.price_x64.get(), remaining, launch.team_bps)?;
    if tokens == 0 {
        return Err(FloorError::Nothing.into());
    }
    let spent = cost.checked_add(team_fee).ok_or(QuoteError::Overflow)?;
    Ok(SaleBuyQuote { tokens, cost, team_fee, spent, graduates: launch.sold + tokens == launch.sale_tokens })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaleSellBackQuote {
    /// The GROSS payout, `floor(tokens · price_x64 / 2^64)` (stateless); `raised` / `contributed` fall by all of it.
    pub payout: u64,
    /// floor(payout·team_bps/1e4), kept for the team.
    pub team_fee: u64,
    /// What the seller receives: payout - team_fee.
    pub received: u64,
}

/// The STATELESS sell-back: `payout = floor(tokens · price_x64 / 2^64)` at the launch's one fixed price, less the
/// platform fee — the program's `sell_back` / `sell_back_wsol`. It reads no position, so [`sale_sell_back_stateless`]
/// quotes it from the `Launch` alone (what a router needs); this form also checks that the position covers `tokens`,
/// as the program does (`InsufficientAllocation`).
pub fn sale_sell_back(launch: &Launch, position: &BuyerPosition, tokens: u64) -> QuoteResult<SaleSellBackQuote> {
    if tokens > position.allocation && launch.state == LAUNCH_STATE_BONDING && tokens > 0 {
        return Err(FloorError::InsufficientAllocation.into());
    }
    sale_sell_back_stateless(launch, tokens)
}

/// The stateless sell-back quoted from the `Launch` alone: `(payout, team_fee, received)` for `tokens`, exact to the
/// unit for ANY seller whose position covers `tokens`. `SaleOver` once the sale is not bonding, `Nothing` for zero.
pub fn sale_sell_back_stateless(launch: &Launch, tokens: u64) -> QuoteResult<SaleSellBackQuote> {
    if launch.state != LAUNCH_STATE_BONDING {
        return Err(FloorError::SaleOver.into());
    }
    if tokens == 0 {
        return Err(FloorError::Nothing.into());
    }
    let payout = mech::sell_back_payout_at(tokens, launch.price_x64.get())?;
    let (team_fee, received) = mech::sell_back_with_team(payout, launch.team_bps)?;
    Ok(SaleSellBackQuote { payout, team_fee, received })
}

#[cfg(test)]
mod tests {
    use super::*;
    use floor_types::U128;
    fn launch(team_bps: u16) -> Launch {
        let mut l: Launch = bytemuck::Zeroable::zeroed();
        (l.version, l.state, l.sale_tokens, l.team_bps) = (3, LAUNCH_STATE_BONDING, floor_types::constants::SALE_TOKENS, team_bps);
        l.price_x64 = U128::new(1u128 << 64); // tokens == net
        l
    }
    #[test]
    fn a_sale_buy_quote_spends_the_gross_and_reports_the_fee() {
        assert_eq!(sale_buy(&launch(25), 1_002_500_000).unwrap(), SaleBuyQuote { tokens: 1_000_000_000, cost: 1_000_000_000, team_fee: 2_500_000, spent: 1_002_500_000, graduates: false });
        let tiny = sale_buy(&launch(25), 400).unwrap();
        assert_eq!((tiny.tokens, tiny.cost, tiny.team_fee, tiny.spent), (399, 399, 0, 399));
        let old = sale_buy(&launch(0), 1_002_500_000).unwrap();
        assert_eq!((old.cost, old.team_fee), (1_002_500_000, 0));
    }
    #[test]
    fn a_sell_back_quote_is_stateless_and_pays_the_share_less_the_fee() {
        let mut l = launch(25);
        l.price_x64 = U128::new(3u128 << 63); // 1.5 lamports a unit
        let mut p: BuyerPosition = bytemuck::Zeroable::zeroed();
        (p.allocation, p.contributed) = (3_000, 4_507); // a position carrying 7 lamports of its own rounding
        assert_eq!(sale_sell_back(&l, &p, 1_000).unwrap(), SaleSellBackQuote { payout: 1_500, team_fee: 3, received: 1_497 });
        assert_eq!(sale_sell_back(&l, &p, 3_000).unwrap(), SaleSellBackQuote { payout: 4_500, team_fee: 11, received: 4_489 });
        assert_eq!(sale_sell_back(&l, &p, 1_001).unwrap().payout, 1_501, "floor(1_001 · 1.5)");
        // the position is read only for coverage: any covering position quotes the same
        for t in [1u64, 2, 999, 1_000, 3_000] {
            assert_eq!(sale_sell_back(&l, &p, t).unwrap(), sale_sell_back_stateless(&l, t).unwrap());
        }
        assert!(sale_sell_back(&l, &p, 3_001).is_err(), "more than the allocation");
        assert!(sale_sell_back_stateless(&l, 0).is_err(), "nothing");
        l.state = LAUNCH_STATE_BONDING + 1;
        assert!(sale_sell_back_stateless(&l, 1).is_err(), "not bonding");
    }

    /// A quote follows EACH LAUNCH'S OWN rate — the 100 bps default, a lowered 40, and a zero-fee launch created after
    /// `set_team_fees(0, _)`, which quotes no fee at all (`team_fee == 0`, `spent == cost`, `received == payout`). The
    /// same gross, three answers.
    /// Mutations: a constant read instead of `Launch.team_bps`; a zero rate that still floors a fee.
    #[test]
    fn a_quote_follows_each_launchs_own_rate_including_zero() {
        let gross = 1_010_000_000;
        assert_eq!(sale_buy(&launch(100), gross).unwrap(), SaleBuyQuote { tokens: 1_000_000_000, cost: 1_000_000_000, team_fee: 10_000_000, spent: 1_010_000_000, graduates: false });
        assert_eq!(sale_buy(&launch(40), gross).unwrap(), SaleBuyQuote { tokens: 1_005_976_095, cost: 1_005_976_095, team_fee: 4_023_904, spent: 1_009_999_999, graduates: false });
        assert_eq!(sale_buy(&launch(0), gross).unwrap(), SaleBuyQuote { tokens: gross, cost: gross, team_fee: 0, spent: gross, graduates: false });
        let mut p: BuyerPosition = bytemuck::Zeroable::zeroed();
        (p.allocation, p.contributed) = (1_000_000_000, 1_000_000_000);
        assert_eq!(sale_sell_back(&launch(100), &p, 1_000_000_000).unwrap(), SaleSellBackQuote { payout: 1_000_000_000, team_fee: 10_000_000, received: 990_000_000 });
        assert_eq!(sale_sell_back(&launch(40), &p, 1_000_000_000).unwrap(), SaleSellBackQuote { payout: 1_000_000_000, team_fee: 4_000_000, received: 996_000_000 });
        assert_eq!(sale_sell_back(&launch(0), &p, 1_000_000_000).unwrap(), SaleSellBackQuote { payout: 1_000_000_000, team_fee: 0, received: 1_000_000_000 });
    }
}
