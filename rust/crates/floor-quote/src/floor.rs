//! FLOOR's three swap callbacks, replayed off-chain against a copy of `FloorPool`.
//!
//! Each function takes the state BEFORE the callback, leaves it as `floor_hook` writes it, and
//! returns the record the hook returns. [`crate::FloorHookQuoter`] hands them to
//! `carpenter_quote::quote_swap`, which runs them in the AMM's round order and applies the records
//! exactly as the AMM does, so the trader's amounts, the pool's post-state and `FloorPool`'s
//! post-state all come out of one replay.
//!
//! ⛔ The fee booking (`take_fees`, `intake`) is `floor-math`'s, through `mech.rs`, so the quoter and
//! the hook program book through ONE function. The re-place's booking and the I-3 epilogue are
//! written out here; the mechanism MATH (the ramp, the bid's amounts, the band, the solve) is
//! floor-math's too. Each step names its EVM twin.
//!
//! **The in-swap bid re-place** (the EVM's `replaceBidInSwap`): when a sell leaves tokens inside the bid
//! (`bid_holds_tokens`), round 1 closes the bid and nudges the price back up to the
//! band's dear edge, round 2 re-opens the bid over the part of the band still above the market. The
//! skim distance is to the dear edge, through floor-math's `take_fees`.
//! ⚠ The nudge's budget is sized in round 1, before the close executes, so it excludes the LP fees
//! the close returns — the EVM's budget includes them (`replace_actions`).
//! ⛔ **Orientation**: `quote_is_token1 == 1` is refused in `bind` (`QuoteReverseOrientation`); the
//! `quote_is_token1` branches below are the EVM's lines transcribed and every one runs its token0 arm.
//!
//! **Rounds** (`FLOOR_MAX_ROUNDS` = 3):
//! - `after_swap` (round 1): `[ModifyPosition BID −L, Swap nudge]`, `want_callback`.
//! - `after_actions` (round 2): books the close (`_teardownBid`) and the nudge
//!   (`_buyUpTo`), then `_deployBid` sizes the new band at the
//!   post-nudge price and pushes `[ModifyPosition BID +L']`, computing the owed itself
//!   (`amounts_for_liquidity`, rounded UP — the same function the AMM's add charges by), no further
//!   callback. ⚠ The hook books an owed it COMPUTED, where the EVM books what `_open` RETURNED; the
//!   two agree while both use the same rounding, and a ratchet's surplus booking absorbs any dust.

use carpenter_quote::clmm::tick_math;
use floor_math::FeeTake;
use floor_types::constants::{DYNAMIC_FEE_FLAG, LP_FEE_PIPS, MAX_TICK, SALT_BID};
use floor_types::hook::{
    ActionResult, BalanceDelta, FeeSplit, HookAction, HookDeltaAssembly, HookRecordV1, HOOK_INTERFACE_VERSION,
    PHASE_AFTER_ACTIONS, PHASE_AFTER_SWAP, PHASE_BEFORE_SWAP,
};
use floor_types::layout::FloorPool;

use carpenter_quote::amounts_for_liquidity;
pub use carpenter_quote::AfterSwapIn;

use crate::error::{FloorError, QuoteError, QuoteResult};
use crate::mech;

/// What the `take_fees` calls of one quote charged, for the quote's fee breakdown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FeesTaken {
    /// The skim (`fee`), before the platform cut.
    pub skim: u64,
    pub creator: u64,
    /// The holder tax, booked where I-3 counts it (`holder_owed`); up to 10 % a side.
    pub holder: u64,
    /// The cut `intake` took out of the skim.
    pub platform_cut: u64,
    /// The part of `creator` split off to the pool's referrer, 0 unless the launch was referred. ⛔ CARVED OUT OF
    /// `creator`, exactly as `platform_cut` is carved out of `skim` — so it is NOT added into [`FeesTaken::total`], and
    /// a quote is the same number on a referred and an unreferred pool.
    pub referral: u64,
    /// The platform fee, on top: `floor(leg · team_fee_bps / 1e4)`, booked to `team_owed`; 0 at a zero rate.
    /// ⛔ ADDED into [`FeesTaken::total`], unlike `platform_cut` and `referral`.
    pub team: u64,
}

impl FeesTaken {
    /// The hook delta on the quote leg: skim + creator + holder + team (the platform cut stays inside the skim).
    pub fn total(&self) -> u64 {
        self.skim + self.creator + self.holder + self.team
    }

    fn add(&mut self, t: FeeTake) {
        self.skim += t.skim_fee;
        self.creator += t.creator_fee;
        self.holder += t.holder_fee;
        self.platform_cut += t.cut;
        self.referral += t.referral_fee;
        self.team += t.team_fee;
    }
}

/// The inputs the AMM hands `before_swap` / `after_swap` that FLOOR reads.
#[derive(Clone, Copy, Debug)]
pub struct SwapCtx {
    pub pool_key: [u8; 32],
    pub zero_for_one: bool,
    pub amount_specified: i128,
    pub pre_tick: i32,
    pub supply1: u64,
    /// Pre-swap claims (`BeforeSwapArgs` / `AfterSwapArgs`).
    pub claims0: u64,
    pub claims1: u64,
    /// The pool's own `mint0` / `mint1` / `tick_spacing`, for the binding's second clause (see [`bind`]).
    pub mint0: [u8; 32],
    pub mint1: [u8; 32],
    pub tick_spacing: u16,
    /// The pool's key fee, for the third clause: a FLOOR pool's is `DYNAMIC_FEE_FLAG` (see [`bind`]).
    pub fee: u32,
    /// THE GAP BUY SKIM: the cushion top cached in the `FloorPool` account's TWAP tail
    /// ([`crate::state::decode_cushion_top`]), as the chain's `before_swap` reads it (`SwapQuoteParams::cushion_top`).
    pub cushion_top: Option<i32>,
}

/// The pool binding, BOTH clauses.
///
/// 1. `require_keys_eq!(pool.key(), floor_pool.amm_pool, WrongPool)` and `pool_bound == 1`.
///    (The hook's next check, `ladder_seeded == 1` / `NotSeeded`, is made in [`before_swap`] only.)
/// 2. ⛔ "every vault, mint and token program is checked against the bound pool's fields". Without this clause a
///    `FloorPool` whose `quote_mint` / `base_mint` / `quote_is_token1` disagreed with the `Pool` would be ACCEPTED —
///    and `quote_is_token1` is what `is_buy` and `quote_specified` key off, so a mis-stamped state would decide which
///    leg pays the sell skim and fire the re-place on the wrong side, with no refusal. On chain this is unreachable
///    (`before_initialize` pins `mint0 == quote_mint`, `mint1 == base_mint`), which is exactly why a quoter fed
///    hand-built bytes must check it: app mocks and fixtures are where it comes from. The binding's error is
///    `WrongPool`, so that is the name.
/// 3. ⛔ **`quote_is_token1 == 1` is refused by name (`QuoteReverseOrientation`)**, whatever the mints
///    say. The `quote_is_token1` branches below — the limit comparison in `replace_actions`, the fee and owed legs in
///    `after_actions` / `deploy_bid`, `legs`, `is_buy` — are transcriptions of the EVM's lines and are kept as such,
///    but the EVM pairs them with a TICK negation this file does not do (`_sqrtPriceAtTick`, `_modify`, `_market`):
///    `dear_edge`, `deploy_bid`'s ticks and `sqrt_at(edge)` are all computed in the quote = token0 orientation. On
///    chain the reverse state cannot exist (`before_initialize` pins `mint0 == quote_mint`), so rather than carry half
///    of the EVM's reverse-orientation handling and answer wrongly on a fixture that carries the other half, the
///    quoter refuses it. Every branch below therefore runs its token0 arm.
fn bind(fp: &FloorPool, ctx: &SwapCtx) -> QuoteResult<()> {
    if fp.amm_pool != ctx.pool_key {
        return Err(FloorError::WrongPool.into());
    }
    if fp.pool_bound != 1 {
        return Err(FloorError::NotBound.into());
    }
    if quote_is_token1(fp) {
        return Err(FloorError::QuoteReverseOrientation.into());
    }
    // The orientation `quote_is_token1` CLAIMS must be the one the pool's mints are in.
    let (quote_mint, base_mint) = if quote_is_token1(fp) { (ctx.mint1, ctx.mint0) } else { (ctx.mint0, ctx.mint1) };
    if fp.quote_mint != quote_mint || fp.base_mint != base_mint {
        return Err(FloorError::WrongPool.into());
    }
    if fp.tick_spacing != i32::from(ctx.tick_spacing) {
        return Err(FloorError::WrongPool.into());
    }
    // 3. `fee == DYNAMIC_FEE_FLAG`, under the hook's own `NotDynamicFee` — the error the interface table lists for
    // every swap callback, and what `before_initialize` requires: a FLOOR-hooked pool whose key fee was static would
    // otherwise quote at `pool.lp_fee`, the 2,500-pip override silently dropped (`effective_fee_override` returns
    // `None` off a non-dynamic pool).
    if ctx.fee != DYNAMIC_FEE_FLAG {
        return Err(FloorError::NotDynamicFee.into());
    }
    Ok(())
}

fn quote_is_token1(fp: &FloorPool) -> bool {
    fp.quote_is_token1 != 0
}

/// Is the specified amount the quote leg?
fn quote_specified(fp: &FloorPool, zfo: bool, amount_specified: i128) -> bool {
    (zfo != quote_is_token1(fp)) == (amount_specified < 0)
}

fn is_buy(fp: &FloorPool, zfo: bool) -> bool {
    zfo != quote_is_token1(fp)
}

fn add(a: u64, b: u64) -> QuoteResult<u64> {
    a.checked_add(b).ok_or(QuoteError::Overflow)
}

fn sub(a: u64, b: u64) -> QuoteResult<u64> {
    a.checked_sub(b).ok_or(QuoteError::Overflow)
}

/// The quote / base legs of a hook-claims delta, by the pool's orientation (quote is token0 on
/// every FLOOR pool, but the orientation is read, never assumed).
/// THE VAULT'S BOUGHT-BACK TOKENS, the chain's `floor_hook::mech::positions::vault_bought`
/// mirrored: inventory WITH its cost. Every pool is a staircase; the chain refuses a non-staircase pool here
/// (`InvariantViolated`), and so does the quoter.
fn vault_bought(fp: &mut FloorPool, tokens: u64, cost: u64) -> QuoteResult<()> {
    if fp.stair_steps == 0 {
        return Err(FloorError::InvariantViolated.into());
    }
    if tokens == 0 {
        return Ok(());
    }
    fp.inventory_tokens = add(fp.inventory_tokens, tokens)?;
    fp.inventory_cost_eth = add(fp.inventory_cost_eth, cost)?;
    Ok(())
}

fn legs(fp: &FloorPool, d: BalanceDelta) -> (i128, i128) {
    if quote_is_token1(fp) {
        (d.amount1, d.amount0)
    } else {
        (d.amount0, d.amount1)
    }
}

fn credited(x: i128) -> QuoteResult<u64> {
    u64::try_from(x).map_err(|_| QuoteError::Overflow)
}

fn debited(x: i128) -> QuoteResult<u64> {
    u64::try_from(x.checked_neg().ok_or(QuoteError::Overflow)?).map_err(|_| QuoteError::Overflow)
}

/// `take_fees(leg, is_buy, tick)`, floor-math's through `mech.rs`: the skim distance
/// is to the band's DEAR edge and the at-floor endpoints are the pool's. `.total` is what the hook
/// delta carries: `fee + cfee + hfee + tfee`; `split_of` is what the record's `fees` carries.
fn take_fees(fp: &mut FloorPool, leg: u64, buy: bool, tick: i32, cushion_top: Option<i32>, taken: &mut FeesTaken) -> QuoteResult<FeeTake> {
    let t = mech::take_fees(fp, leg, buy, tick, cushion_top)?;
    taken.add(t);
    Ok(t)
}

/// `HookRecordV1.fees` for one `take_fees`: `[skim, creator, holder, platform, team]`, the skim GROSS and the platform
/// cut carved out of it, the team fee on top — the same figures `FeesTaken` accumulates. ⚠ The chain's twin is
/// `floor_hook`'s `mech::fees::split_of`; the two must agree.
fn split_of(t: FeeTake) -> FeeSplit {
    [t.skim_fee, t.creator_fee, t.holder_fee, t.cut, t.team_fee]
}

/// I-3 (the epilogue on every callback): claims plus this callback's credit cover the idle
/// liabilities.
fn epilogue_i3(fp: &FloorPool, claims0: u64, claims1: u64, credit: BalanceDelta) -> QuoteResult<()> {
    let have0 = (claims0 as i128) + credit.amount0;
    let have1 = (claims1 as i128) + credit.amount1;
    // ⛔ ONE SPELLING of I-3's quote side, in `floor-math`. The quoter must refuse EXACTLY what the chain refuses, or
    //    it promises a swap the hook reverts; a liability added to the chain's copies and not to a quoter copy would
    //    leave the quoter silently looser. `FloorPool` here IS `FloorState`'s bytes (`mirror_is_the_same_bytes`), so
    //    the cast is the same one `mech/fees.rs` books through.
    let owe0 = floor_math::FloorState::quote_liabilities(bytemuck::cast_ref(fp)) as i128;
    // ⛔ And ONE spelling of the BASE side: the held tokens are idle in claims1 for good, and a staircase pool's
    //    standing tokens are not idle.
    let owe1 = floor_math::FloorState::base_liabilities(bytemuck::cast_ref(fp)) as i128;
    if have0 >= owe0 && have1 >= owe1 {
        Ok(())
    } else {
        Err(FloorError::InvariantViolated.into())
    }
}

/// `before_swap` (the EVM's `beforeSwap`).
pub fn before_swap(fp: &mut FloorPool, ctx: &SwapCtx, taken: &mut FeesTaken) -> QuoteResult<HookRecordV1> {
    // `floor_hook`'s `before_swap` requires `ladder_seeded == 1` (`NotSeeded`) right after the
    // `amm_pool` / `pool_bound` checks and before every other branch, exact-out included: a pool whose
    // launch has not graduated refuses every swap. A wrong or unbound pool still reports `WrongPool` /
    // `NotBound` first (from `bind`). `after_swap` and `after_actions` do not read the flag.
    if fp.amm_pool == ctx.pool_key && fp.pool_bound == 1 && fp.ladder_seeded != 1 {
        return Err(FloorError::NotSeeded.into());
    }
    bind(fp, ctx)?;
    if ctx.amount_specified > 0 {
        return Err(FloorError::ExactOutNotSupported.into()); // FLOOR pools quote exact-in only
    }
    let mut rec = HookRecordV1::pass_through(HOOK_INTERFACE_VERSION, PHASE_BEFORE_SWAP, 0);
    rec.fee_override_pips = Some(LP_FEE_PIPS);
    if quote_specified(fp, ctx.zero_for_one, ctx.amount_specified) {
        let spec = u64::try_from(ctx.amount_specified.unsigned_abs()).map_err(|_| QuoteError::AmountTooLarge)?;
        // ⛔ SATURATING, as the program is (`floor_hook`'s `before_swap`): `spec` is the
        // trader's NOMINAL amount, so one exact-in buy of ~u64::MAX with a one-unit price limit pins the chain's
        // counter at u64::MAX for ~2 units; a checked add here would then refuse EVERY later quote (`Overflow`).
        fp.cumulative_volume_eth = fp.cumulative_volume_eth.saturating_add(spec);
        let buy = is_buy(fp, ctx.zero_for_one);
        let t = take_fees(fp, spec, buy, ctx.pre_tick, ctx.cushion_top, taken)?;
        rec.delta_specified = t.total as i128;
        rec.fees = split_of(t);
    }
    let credit = HookDeltaAssembly::assemble(ctx.amount_specified, ctx.zero_for_one, rec.delta_specified, rec.delta_unspecified);
    epilogue_i3(fp, ctx.claims0, ctx.claims1, credit)?;
    Ok(rec)
}


/// `after_swap`: the unspecified-leg fee, the exhaustion signals, and on a sell the
/// in-swap bid re-place.
pub fn after_swap(fp: &mut FloorPool, ctx: &SwapCtx, a: &AfterSwapIn, taken: &mut FeesTaken) -> QuoteResult<HookRecordV1> {
    bind(fp, ctx)?;
    let zfo = ctx.zero_for_one;
    let mut rec = HookRecordV1::pass_through(HOOK_INTERFACE_VERSION, PHASE_AFTER_SWAP, 1);

    if !quote_specified(fp, zfo, ctx.amount_specified) {
        let quote_leg = if quote_is_token1(fp) { a.swap_delta.amount1 } else { a.swap_delta.amount0 };
        let leg = u64::try_from(quote_leg.unsigned_abs()).map_err(|_| QuoteError::Overflow)?;
        if leg > 0 {
            fp.cumulative_volume_eth = fp.cumulative_volume_eth.saturating_add(leg); // as `after_swap.rs:70`
            let buy = is_buy(fp, zfo);
            // a sell's leg (a buy's quote leg is always the specified one): no cushion top, as the chain's `after_swap`
            let t = take_fees(fp, leg, buy, a.post_tick, None, taken)?;
            rec.delta_unspecified = t.total as i128;
            rec.fees = split_of(t);
        }
    }

    if !is_buy(fp, zfo) {
        if let Some(plan) = replace_actions(fp, a.post_sqrt_price_x64)? {
            rec.actions.extend_from_slice(&plan);
            rec.want_callback = true;
        }
    }

    let unspecified = a.before_delta_unspecified.checked_add(rec.delta_unspecified).ok_or(QuoteError::Overflow)?;
    let credit = HookDeltaAssembly::assemble(ctx.amount_specified, zfo, a.before_delta_specified, unspecified);
    epilogue_i3(fp, ctx.claims0, ctx.claims1, credit)?;
    Ok(rec)
}

/// The EVM's `_bidHoldsTokens`: the bid is deployed and, at the live price, holds tokens. A sell
/// that stopped short of the band leaves it all-quote and this is false; a gap under backing on its
/// own triggers nothing.
pub fn bid_holds_tokens(fp: &FloorPool, post_sqrt: u128) -> QuoteResult<bool> {
    if fp.bid_liquidity.get() == 0 {
        return Ok(false);
    }
    let (_, tokens_held) = mech::bid_amounts(post_sqrt, fp.bid_lower, mech::bid_width(fp), fp.bid_liquidity.get(), fp.bid_eth_deployed)?;
    Ok(tokens_held > 0)
}

/// The round-1 plan of the in-swap bid re-place (the EVM's `replaceBidInSwap`), or `None` when the
/// bid holds no tokens at `post_sqrt`:
///
/// 1. `ModifyPosition { BID, [bid_lower, bid_lower + bid_width], −bid_liquidity }` — `_teardownBid`.
/// 2. `Swap { buy, exact-in −budget, limit sqrt(dear_edge) }` — the nudge, with
///    `budget = min(vault_eth + bid_eth, min_bid)`: the ceiling term is what
///    claims will hold when the Swap runs (`bid_eth` is the quote the close returns, `bid_amounts`,
///    rounded DOWN exactly as the AMM's remove rounds), and the `min` is the CAP — dust, the same
///    bound `ratchet` gives the identical call, because the gap the nudge exists to cross is EMPTY
///    and anything it cannot cross on dust is third-party liquidity. The EVM hands it the whole
///    vault; this does not. Omitted when the market already sits at or under the dear edge, or the budget is 0.
///
/// ⚠ The budget EXCLUDES the LP fees the close returns. The EVM's `_nudge(key, _state().vaultEth)` runs AFTER
/// `_teardownBid`, whose `ethBack` is v4's `callerDelta` = principal + `feesAccrued` — so its budget is
/// larger by the fees the bid earned. Here round 1 must name the budget BEFORE the close executes, and
/// the fees are not knowable then (they need the `Position` account, and `bid_amounts` reports the
/// principal). Direction: the Solana nudge spends up to that much less; the fees stay in `vault_eth`
/// (booked by `after_actions`) and are re-deployed in the bid or spent by the next nudge. Dust. The
/// alternative — a round-3 nudge sized from the close's `ActionResult` — needs a pool whose `hook_max_rounds` is 4.
///
/// The AMM executes both in order and calls `after_actions` (round 2) with their results. The tick is
/// not an input: the trigger and the limit test both compare sqrt prices.
pub fn replace_actions(fp: &FloorPool, post_sqrt: u128) -> QuoteResult<Option<Vec<HookAction>>> {
    if !bid_holds_tokens(fp, post_sqrt)? {
        return Ok(None);
    }
    let width = mech::bid_width(fp);
    let lower = fp.bid_lower;
    let upper = lower.checked_add(width).ok_or(QuoteError::Overflow)?;
    let liq = i128::try_from(fp.bid_liquidity.get()).map_err(|_| QuoteError::Overflow)?;
    let mut plan = vec![HookAction::ModifyPosition {
        salt: SALT_BID,
        tick_lower: lower,
        tick_upper: upper,
        liquidity_delta: -liq,
    }];

    let (bid_eth, _) = mech::bid_amounts(post_sqrt, lower, width, fp.bid_liquidity.get(), fp.bid_eth_deployed)?;
    // the nudge's budget is DUST, capped at `min_bid` — the same bound the ratchet gives the identical call, and the
    // same one the program's re-place planner builds. A quoter that predicted the EVM's whole vault would disagree
    // with the chain on every re-place that crosses third-party liquidity.
    let budget = add(fp.vault_eth, bid_eth)?.min(fp.min_bid);
    let edge = mech::dear_edge(fp)?;
    let limit = tick_math::get_sqrt_price_at_tick(edge)?;
    // nothing to do at or under the edge, or with nothing to spend.
    let at_or_under = if quote_is_token1(fp) { post_sqrt >= limit } else { post_sqrt <= limit };
    if budget > 0 && !at_or_under {
        plan.push(HookAction::Swap {
            zero_for_one: !quote_is_token1(fp),
            amount_specified: -(budget as i128),
            sqrt_price_limit_x64: limit,
        });
    }
    Ok(Some(plan))
}

/// What `after_actions` booked and (re)deployed, for the quote's `replace` breakdown
/// (`BidReplaced(absorbed, ethBack, fromLower, bidLower)`, plus the nudge and the new band).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReplaceBooking {
    /// Tokens the closed bid held; joined inventory at cost.
    pub absorbed: u64,
    /// Quote the close returned to the vault (principal + fees, the fees through `intake`).
    pub eth_back: u64,
    /// What the nudge spent and got (both 0 across an empty gap).
    pub nudge_spent: u64,
    pub nudge_got: u64,
    /// The closed band's lower tick.
    pub from_lower: i32,
    /// The re-deployed band, if any (`redeployed`).
    pub redeployed: bool,
    pub to_lower: i32,
    pub to_upper: i32,
    pub bid_eth_deployed: u64,
    pub bid_liquidity: u128,
    /// True when the close's `fees0/fees1` were NOT the position's real accrued fees — i.e. `results`
    /// were produced by `modify.rs`, which has no `Position` account to read them from. Derived from
    /// the caller's `results_carry_fees`, never assumed: a caller replaying the chain's own
    /// `ActionResult`s gets `false`, and the booking then used the fees it was given.
    pub fees_unknown: bool,
}

/// `after_actions` round 2: book the close and the nudge, then re-deploy the bid. `results` are
/// round 1's `ActionResult`s in order (`[close, nudge]` or `[close]`); `results_carry_fees` says whether
/// the close's `fees0/fees1` are the position's real accrued fees (true for the chain's own results;
/// false for `modify.rs`'s, which are 0 by construction) and is what `ReplaceBooking.fees_unknown`
/// reports; `claims0/1` are post-action. Returns the round-2 record — `[ModifyPosition BID +L']`
/// when a bid can stand, else empty — and the booking.
pub fn after_actions(
    fp: &mut FloorPool,
    ctx: &SwapCtx,
    round: u8,
    results: &[ActionResult],
    results_carry_fees: bool,
    claims0: u64,
    claims1: u64,
) -> QuoteResult<(HookRecordV1, ReplaceBooking)> {
    bind(fp, ctx)?;
    let mut rec = HookRecordV1::pass_through(HOOK_INTERFACE_VERSION, PHASE_AFTER_ACTIONS, round);
    let mut booking = ReplaceBooking { from_lower: fp.bid_lower, ..ReplaceBooking::default() };
    if results.is_empty() {
        epilogue_i3(fp, claims0, claims1, BalanceDelta::default())?;
        return Ok((rec, booking));
    }

    // ---- 1. the close: `_teardownBid`, with the fees through `intake` --------------------------------
    let close = &results[0];
    if close.kind != HookAction::KIND_MODIFY_POSITION {
        return Err(QuoteError::HookFieldNotAllowed);
    }
    let (q, b) = legs(fp, close.delta);
    let quote_back = credited(q)?;
    let tok_back = credited(b)?;
    let (fees0, fees1) = if quote_is_token1(fp) { (close.fees1, close.fees0) } else { (close.fees0, close.fees1) };
    let principal0 = sub(quote_back, fees0)?;
    let kept_fees = mech::intake(fp, fees0)?;
    fp.vault_eth = add(fp.vault_eth, add(principal0, kept_fees)?)?;
    // What the bid bought is the vault's bought-back inventory WITH its cost — the chain's
    // `floor_hook::mech::positions::vault_bought`, mirrored.
    let cost = fp.bid_eth_deployed.saturating_sub(principal0);
    vault_bought(fp, tok_back, cost)?;
    let _ = fees1; // token-side LP fees came back with the tokens: they are in `tok_back` already
    fp.bid_liquidity.set(0);
    fp.bid_eth_deployed = 0;
    booking.absorbed = tok_back;
    booking.eth_back = quote_back;
    booking.fees_unknown = !results_carry_fees;

    // ---- 2. the nudge: `_buyUpTo` ----------------------------------------------------------------------
    let (mut sqrt, mut tick) = (close.post_sqrt_price_x64, close.post_tick);
    if let Some(nudge) = results.get(1) {
        if nudge.kind != HookAction::KIND_SWAP {
            return Err(QuoteError::HookFieldNotAllowed);
        }
        let (q, b) = legs(fp, nudge.delta);
        let spent = if q < 0 { debited(q)? } else { 0 };
        let got = if b > 0 { credited(b)? } else { 0 };
        // `spent == 0 || got == 0` books nothing (the walk moved through an empty gap, or
        // nothing at all). A swap that spent and got nothing back cannot happen on chain: the AMM
        // debits only what the walk took.
        if spent > 0 && got > 0 {
            fp.vault_eth = sub(fp.vault_eth, spent)?;
            vault_bought(fp, got, spent)?; // the staircase pool's inventory, at cost
            booking.nudge_spent = spent;
            booking.nudge_got = got;
        }
        sqrt = nudge.post_sqrt_price_x64;
        tick = nudge.post_tick;
    }

    // ---- 3. `_deployBid` at the post-nudge price --------------------------------------------------------
    if let Some((lower, upper, liq, owed)) = deploy_bid(fp, sqrt, tick)? {
        fp.vault_eth = sub(fp.vault_eth, owed)?;
        fp.bid_eth_deployed = owed;
        fp.bid_liquidity.set(liq);
        fp.bid_lower = lower;
        fp.bid_width = upper - lower;
        rec.actions.push(HookAction::ModifyPosition {
            salt: SALT_BID,
            tick_lower: lower,
            tick_upper: upper,
            liquidity_delta: i128::try_from(liq).map_err(|_| QuoteError::Overflow)?,
        });
        booking.redeployed = true;
        booking.to_lower = lower;
        booking.to_upper = upper;
        booking.bid_eth_deployed = owed;
        booking.bid_liquidity = liq;
    }
    fp.needs_ratchet = 1;

    // I-3 on the post-round-1 claims: the open is booked (`vault_eth −= owed`) before it executes, so
    // the claims still hold the owed and the inequality has that much slack.
    epilogue_i3(fp, claims0, claims1, BalanceDelta::default())?;
    Ok((rec, booking))
}

/// `_deployBid`'s geometry and size: `(lower, upper, liquidity, owed)` or `None` when no
/// bid is placed (vault under `min_bid`, the band above `MAX_TICK`, or zero liquidity).
///
/// The bid is the band `[dear edge, floor_tick]`, all quote, strictly above the current tick. The CHEAP
/// edge is pinned to `floor_tick`; a partial fill leaves the market inside the band, so the re-place
/// keeps only the part above it — narrowing from the dear side. When the whole band is behind the
/// market, one spacing above it (`upper = lower + spacing`), under the published floor.
pub fn deploy_bid(fp: &FloorPool, sqrt: u128, current: i32) -> QuoteResult<Option<(i32, i32, u128, u64)>> {
    if fp.vault_eth < fp.min_bid {
        return Ok(None);
    }
    let spacing = fp.tick_spacing;
    let mut upper = fp.floor_tick;
    let mut lower = upper.checked_sub(mech::band_ticks(fp)?).ok_or(QuoteError::Overflow)?;
    let min_lower = mech::ceil_to_spacing(current.checked_add(1).ok_or(QuoteError::Overflow)?, spacing)?;
    if lower < min_lower {
        lower = min_lower;
    }
    if lower >= upper {
        upper = lower.checked_add(spacing).ok_or(QuoteError::Overflow)?;
    }
    if upper > MAX_TICK {
        return Ok(None);
    }
    let liq = mech::position_liquidity(sqrt, lower, upper, fp.vault_eth, 0)?;
    if liq == 0 {
        return Ok(None);
    }
    // What the AMM's add will charge: rounded UP (`liquidity::amounts_for_liquidity`), the quote leg.
    let (a0, a1) = amounts_for_liquidity(sqrt, current, lower, upper, i128::try_from(liq).map_err(|_| QuoteError::Overflow)?)?;
    let owed = if quote_is_token1(fp) { a1 } else { a0 };
    Ok(Some((lower, upper, liq, owed)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::Zeroable;

    fn pool(vault: u64, floor_tick: i32) -> FloorPool {
        let mut fp = FloorPool::zeroed();
        fp.tick_spacing = 60;
        fp.floor_tick = floor_tick;
        fp.vault_eth = vault;
        fp.min_bid = 50_000_000;
        // every pool is a staircase: `vault_bought` refuses anything else
        fp.stair_steps = 1;
        fp
    }

    // `_deployBid`'s three geometries: the whole band above the market; narrowed from the dear side
    // by a market inside it; and the last resort, one spacing above a market past the cheap edge.
    #[test]
    fn deploy_bid_geometry() {
        let fp = pool(1_000_000_000, 100_320);
        let at = |t: i32| tick_math::get_sqrt_price_at_tick(t).unwrap();
        // market well under the band: [dear, floor]
        let (lo, up, liq, owed) = deploy_bid(&fp, at(100_020), 100_020).unwrap().unwrap();
        assert_eq!((lo, up), (100_200, 100_320));
        assert!(liq > 0 && owed <= 1_000_000_000 && owed > 999_000_000, "{owed}");
        // market one tick under the dear edge (v4's next − 1 after a nudge that reached it): whole band
        assert_eq!(deploy_bid(&fp, at(100_200), 100_199).unwrap().unwrap().0, 100_200);
        // market AT the dear edge: narrowed by one spacing
        let (lo, up, _, _) = deploy_bid(&fp, at(100_200), 100_200).unwrap().unwrap();
        assert_eq!((lo, up), (100_260, 100_320));
        // market inside the band: only the part above it, narrowed from the dear side
        let (lo, up, _, _) = deploy_bid(&fp, at(100_210), 100_210).unwrap().unwrap();
        assert_eq!((lo, up), (100_260, 100_320));
        // market in the band's last spacing: ceil(100_271) is the cheap edge itself, so nothing of
        // the band is above the market and the last resort applies — one spacing above it
        let (lo, up, _, _) = deploy_bid(&fp, at(100_270), 100_270).unwrap().unwrap();
        assert_eq!((lo, up), (100_320, 100_380));
    }

    // `fees_unknown` is DERIVED from `results_carry_fees`: a caller replaying the chain's own `ActionResult`s — which
    // DO carry the close's fees, and which the booking routes through `intake` — must get a flag that agrees with its
    // own booking. Both provenances are exercised on the same close: the fees are booked either way
    // (`principal0 + intake(fees0)`), and only the flag differs.
    #[test]
    fn fees_unknown_is_derived_from_the_results_provenance() {
        let at = |t: i32| tick_math::get_sqrt_price_at_tick(t).unwrap();
        let mut fp = pool(1_000_000_000, 100_320);
        fp.amm_pool = [7; 32];
        fp.pool_bound = 1;
        fp.quote_mint = [1; 32];
        fp.base_mint = [2; 32];
        fp.platform_bps = 250;
        fp.bid_lower = 100_200;
        fp.bid_width = 120;
        fp.bid_liquidity.set(1_000_000_000_000);
        fp.bid_eth_deployed = 500_000_000;
        let ctx = SwapCtx {
            pool_key: [7; 32],
            zero_for_one: false,
            amount_specified: -1,
            pre_tick: 100_020,
            supply1: 0,
            claims0: 0,
            claims1: 0,
            mint0: [1; 32],
            mint1: [2; 32],
            tick_spacing: 60,
            // the pool binding's third clause (a dynamic-fee key), so `bind` reaches the body instead of refusing
            // `NotDynamicFee`.
            fee: DYNAMIC_FEE_FLAG,
            cushion_top: None,
        };
        let close = ActionResult {
            index: 0,
            kind: HookAction::KIND_MODIFY_POSITION,
            delta: BalanceDelta { amount0: 400_000_000 + 1_234, amount1: 5_000 },
            fees0: 1_234,
            fees1: 0,
            post_sqrt_price_x64: at(100_020),
            post_tick: 100_020,
        };
        let claims = u64::MAX / 4;
        let mut a = fp;
        let (_, quoter) = after_actions(&mut a, &ctx, 2, &[close], false, claims, claims).unwrap();
        let mut b = fp;
        let (_, chain) = after_actions(&mut b, &ctx, 2, &[close], true, claims, claims).unwrap();
        assert!(quoter.fees_unknown, "modify.rs produced the result: the fees are not the position's");
        assert!(!chain.fees_unknown, "the chain's result carries the position's fees");
        // The BOOKING is identical: the fees it was given were booked in both cases (the fees through `intake`).
        assert_eq!(ReplaceBooking { fees_unknown: true, ..chain }, quoter);
        assert_eq!(bytemuck::bytes_of(&a), bytemuck::bytes_of(&b));
        let kept = 1_234 - 1_234 * 250 / 10_000;
        assert_eq!(quoter.eth_back, 400_000_000 + 1_234);
        assert_eq!(a.platform_owed, 1_234 - kept);
        // vault: 1e9 + principal + kept fees − the re-deployed bid's owed
        assert_eq!(a.vault_eth + a.bid_eth_deployed, 1_000_000_000 + 400_000_000 + kept);
        assert!(quoter.redeployed);
    }

    #[test]
    fn deploy_bid_last_resort_and_refusals() {
        let fp = pool(1_000_000_000, 100_320);
        let at = |t: i32| tick_math::get_sqrt_price_at_tick(t).unwrap();
        // market past the cheap edge: one spacing above the market, under the published floor
        let (lo, up, liq, _) = deploy_bid(&fp, at(100_500), 100_500).unwrap().unwrap();
        assert_eq!((lo, up), (100_560, 100_620));
        assert!(liq > 0);
        // market exactly on the cheap edge
        let (lo, up, _, _) = deploy_bid(&fp, at(100_320), 100_320).unwrap().unwrap();
        assert_eq!((lo, up), (100_380, 100_440));
        // under min_bid: nothing
        assert_eq!(deploy_bid(&pool(49_999_999, 100_320), at(100_020), 100_020).unwrap(), None);
        // at the top of the grid `ceil_to_spacing`'s clamp (`MAX_TICK − s`) keeps
        // `lower` at 443,520 and the one-spacing band lands at [443,520, 443,580] ≤ MAX_TICK: the
        // `upper > MAX_TICK` guard is defensive under that clamp, on every spacing
        let top = pool(1_000_000_000, 443_520);
        let (lo, up, _, _) = deploy_bid(&top, at(443_520), 443_520).unwrap().unwrap();
        assert_eq!((lo, up), (443_520, 443_580));
    }

    /// The quoter's I-3 counts the held tokens — idle in claims1 — always. Every pool is a staircase, whose
    /// `ladder_tokens_remaining` stand in its positions and are never a claims liability (`is_stair`).
    /// Mutations this kills: held left out of the quoter's base side; a staircase's standing tokens counted idle.
    #[test]
    fn i3_counts_the_held_tokens_and_never_the_staircases_standing_ones() {
        let mut fp = pool(0, 100_320);
        fp.inventory_tokens = 10;
        fp.ladder_tokens_remaining = 450;
        fp.wall_held_tokens = 150;
        assert_eq!(epilogue_i3(&fp, 0, 160, BalanceDelta::default()), Ok(()));
        assert_eq!(epilogue_i3(&fp, 0, 159, BalanceDelta::default()), Err(FloorError::InvariantViolated.into()), "held is owed");
        fp.stair_steps = 0;
        assert_eq!(epilogue_i3(&fp, 0, 609, BalanceDelta::default()), Err(FloorError::InvariantViolated.into()), "a non-staircase state counts its ladder idle");
        assert_eq!(epilogue_i3(&fp, 0, 610, BalanceDelta::default()), Ok(()));
    }
}
