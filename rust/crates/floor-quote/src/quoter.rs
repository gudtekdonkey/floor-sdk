//! [`FloorHookQuoter`]: `floor_hook`'s swap callbacks as a `carpenter_quote::HookQuoter`, and
//! [`quote_swap`], the one-call FLOOR-pool quote built on it.
//!
//! `carpenter_quote::quote_swap` runs the AMM's round protocol and calls this quoter at each point the
//! AMM calls `floor_hook`; the callbacks themselves are `floor.rs`. What FLOOR adds to the AMM's
//! answer — the fee breakdown in FLOOR's terms, the bid re-place's booking, `FloorPool`'s post-state —
//! the quoter accumulates as it runs, and [`quote_swap`] assembles into [`SwapQuote`].

use carpenter_quote::hook::{AfterSwapIn, HookQuoter, HookSwapCtx};
use carpenter_quote::types::hook::{ActionResult, BalanceDelta, FeeSplit, HookAction, HookRecordV1};
use carpenter_quote::types::layout::{Pool, TickArray};
use carpenter_quote::{is_swap_hooked, PoolAccount, TickArraySet};
use floor_types::ids::{is_floor_hook, ClusterIds, FLOOR_HOOK_ID};
use floor_types::layout::FloorPool;

use crate::error::{QuoteError, QuoteResult};
use crate::floor::{self, FeesTaken, ReplaceBooking, SwapCtx};

/// FLOOR's hook quoter for one quote. Build it with the pool's decoded `FloorPool` (or `None` when it
/// was not fetched: the quote is then refused with `QuoteHookStateMissing`) and the cushion top read
/// from the same account ([`crate::state::decode_cushion_top`]). Use a fresh value per quote.
///
/// Which hook program it replays: [`FloorHookQuoter::new`] accepts `floor_hook` on every cluster in
/// `floor_types::ids::CLUSTERS`; [`FloorHookQuoter::for_cluster`] accepts that one cluster's only (use
/// it when the pool's owner, and so its cluster, is known).
#[derive(Clone, Debug)]
pub struct FloorHookQuoter {
    /// `FloorPool` before the swap; after a successful quote, as the hook leaves it.
    pub state: Option<FloorPool>,
    /// The gap buy skim's input, as `before_swap` reads it (`None`: no cushion cached).
    pub cushion_top: Option<i32>,
    /// What the `take_fees` calls charged.
    pub taken: FeesTaken,
    /// The in-swap bid re-place, when the sell triggered one.
    pub replace: Option<ReplaceQuote>,
    from_tick: i32,
    /// `Some`: the one `floor_hook` id accepted. `None`: any cluster's.
    hook: Option<[u8; 32]>,
}

impl FloorHookQuoter {
    /// A quoter for a pool hooked by `floor_hook` on any known cluster.
    pub fn new(state: Option<FloorPool>, cushion_top: Option<i32>) -> Self {
        Self { state, cushion_top, taken: FeesTaken::default(), replace: None, from_tick: 0, hook: None }
    }

    /// A quoter for a pool of `cluster` only: it accepts `cluster.floor_hook` and no other hook id.
    pub fn for_cluster(state: Option<FloorPool>, cushion_top: Option<i32>, cluster: &ClusterIds) -> Self {
        Self { hook: Some(cluster.floor_hook), ..Self::new(state, cushion_top) }
    }

    fn ctx(&self, c: &HookSwapCtx) -> SwapCtx {
        SwapCtx {
            pool_key: c.pool_key,
            zero_for_one: c.zero_for_one,
            amount_specified: c.amount_specified,
            pre_tick: c.pre_tick,
            supply1: c.supply1,
            claims0: c.claims0,
            claims1: c.claims1,
            mint0: c.mint0,
            mint1: c.mint1,
            tick_spacing: c.tick_spacing,
            fee: c.fee,
            cushion_top: self.cushion_top,
        }
    }

    fn state_mut(&mut self) -> QuoteResult<&mut FloorPool> {
        self.state.as_mut().ok_or(QuoteError::QuoteHookStateMissing)
    }
}

impl HookQuoter for FloorHookQuoter {
    fn program_id(&self) -> [u8; 32] {
        self.hook.unwrap_or(FLOOR_HOOK_ID)
    }

    fn accepts(&self, hook: &[u8; 32]) -> bool {
        match &self.hook {
            Some(id) => id == hook,
            None => is_floor_hook(hook),
        }
    }

    fn prepare(&mut self, _pool: &PoolAccount) -> QuoteResult<()> {
        self.state.map(|_| ()).ok_or(QuoteError::QuoteHookStateMissing)
    }

    fn before_swap(&mut self, c: &HookSwapCtx) -> QuoteResult<HookRecordV1> {
        let ctx = self.ctx(c);
        let mut taken = self.taken;
        let rec = floor::before_swap(self.state_mut()?, &ctx, &mut taken);
        self.taken = taken;
        rec
    }

    fn after_swap(&mut self, c: &HookSwapCtx, after: &AfterSwapIn) -> QuoteResult<HookRecordV1> {
        let ctx = self.ctx(c);
        self.from_tick = after.post_tick;
        let mut taken = self.taken;
        let rec = floor::after_swap(self.state_mut()?, &ctx, after, &mut taken);
        self.taken = taken;
        rec
    }

    fn after_actions(
        &mut self,
        c: &HookSwapCtx,
        round: u8,
        results: &[ActionResult],
        results_carry_fees: bool,
        claims0: u64,
        claims1: u64,
    ) -> QuoteResult<HookRecordV1> {
        let ctx = self.ctx(c);
        let (rec, booking) = floor::after_actions(self.state_mut()?, &ctx, round, results, results_carry_fees, claims0, claims1)?;
        // Round 2 books the re-place that round 1's close and nudge executed. Every action result
        // carries the pool's tick after it, so the last one is where the nudge left the market.
        if round == 2 {
            if let Some(last) = results.last() {
                self.replace = Some(ReplaceQuote { booking, from_tick: self.from_tick, to_tick: last.post_tick });
            }
        }
        Ok(rec)
    }
}

/// [`quote_swap`]'s parameters: the AMM's, plus FLOOR's cushion top.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwapQuoteParams {
    pub zero_for_one: bool,
    /// v4 sign: `< 0` exact-in, `> 0` exact-out (a FLOOR pool refuses exact-out).
    pub amount_specified: i128,
    pub sqrt_price_limit_x64: u128,
    /// `mint0.supply`, as the AMM reads it into the callbacks' args.
    pub supply0: u64,
    /// `mint1.supply`, as the AMM reads it into the callbacks' args. Unused on a plain pool.
    pub supply1: u64,
    /// THE GAP BUY SKIM's input: a FLOOR pool's cushion top as its last ratchet cached it in the
    /// `FloorPool` ACCOUNT's TWAP tail — pass [`crate::state::decode_cushion_top`] of the SAME account
    /// bytes the `FloorPool` was decoded from. `None` on a plain pool. With the switch off (the
    /// shipped setting) the program never writes it, so it decodes `None` and the quote is unchanged.
    pub cushion_top: Option<i32>,
}

impl SwapQuoteParams {
    /// The AMM's half.
    pub fn amm(&self) -> carpenter_quote::SwapQuoteParams {
        carpenter_quote::SwapQuoteParams {
            zero_for_one: self.zero_for_one,
            amount_specified: self.amount_specified,
            sqrt_price_limit_x64: self.sqrt_price_limit_x64,
            supply0: self.supply0,
            supply1: self.supply1,
        }
    }
}

/// The fees one swap pays, each in the currency it is charged in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QuoteFees {
    /// LP fee, Σ of the trader walk's step fees, in the INPUT currency — the LPs' part only; the
    /// Carpenter Platform fee is [`Self::protocol`].
    pub lp: u64,
    /// FLOOR skim before the platform cut, in the quote currency. 0 on a plain pool.
    pub skim: u64,
    /// FLOOR creator fee, in the quote currency.
    pub creator: u64,
    /// FLOOR holder tax, in the quote currency (0 on a pool with no holder rate). Reported rather than
    /// dropped, so `skim + creator + holder + team == hook_delta` on every pool.
    pub holder: u64,
    /// The part of `skim` accrued to the platform.
    pub platform_cut: u64,
    /// The FLOOR Platform fee (quote units, on top; 0 on plain and older pools). Booked to
    /// `team_owed`; never inside the skim, so it is added into [`QuoteFees::hook_total`].
    pub team: u64,
    /// The Carpenter Platform fee the TRADER's walk pays, in [`Self::protocol_currency`] — the quote on
    /// a FLOOR pool, off the input on a buy and off the output on a sell (so `amount_out` is already
    /// net of it). ⚠ The trader's walk only: the in-swap nudge pays its own, visible in
    /// `pool_after.protocol_fees_owed0/1`.
    pub protocol: u64,
    /// The currency (0 / 1) `protocol` is in.
    pub protocol_currency: u8,
}

impl QuoteFees {
    /// The hook delta on the quote leg: `skim + creator + holder + team == hook_delta` (the LP fee is
    /// the pool's, not the hook's, and the platform cut is inside the skim).
    pub fn hook_total(&self) -> u64 {
        self.skim + self.creator + self.holder + self.team
    }
}

/// The in-swap bid re-place, when the sell triggers one: what the hook booked in
/// `after_actions` plus where the price stood before and after the nudge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplaceQuote {
    pub booking: ReplaceBooking,
    /// The tick the sell left the market at (before the close and the nudge).
    pub from_tick: i32,
    /// The tick after the nudge (where the new band was sized).
    pub to_tick: i32,
}

/// A FLOOR-pool (or plain-pool) quote: the AMM's answer and FLOOR's breakdown of it.
#[derive(Clone, Debug)]
pub struct SwapQuote {
    /// What the trader pays (`-in`).
    pub amount_in: u64,
    /// What the trader receives.
    pub amount_out: u64,
    pub fees: QuoteFees,
    /// The pool price after the whole swap, actions included.
    pub sqrt_after: u128,
    pub tick_after: i32,
    /// The price after the trader's walk, before any action.
    pub sqrt_after_trade: u128,
    pub tick_after_trade: i32,
    /// Arrays the trader's walk read (start indices).
    pub arrays_touched: Vec<i32>,
    /// The actions the hook requested and the AMM executed, every round in order.
    pub replace_actions: Vec<HookAction>,
    /// Arrays the actions read: the close's and the open's ticks, the nudge's walk.
    pub action_arrays: Vec<i32>,
    pub replace: Option<ReplaceQuote>,
    pub signals: u8,
    /// Caller perspective, the walk's raw delta.
    pub swap_delta: BalanceDelta,
    /// The one hook delta credited to claims.
    pub hook_delta: BalanceDelta,
    /// `Swapped.fee_split0 / fee_split1`: `[skim, creator, holder, platform, team]` per currency.
    pub fee_split0: FeeSplit,
    pub fee_split1: FeeSplit,
    /// Caller perspective, what settles.
    pub trader_delta: BalanceDelta,
    pub pool_after: Pool,
    /// Post-state of every array a walk crossed a tick in, or read.
    pub arrays_after: Vec<TickArray>,
    /// `FloorPool` as the hook leaves it; `None` on a pool whose hook does not take part in swaps.
    pub floor_after: Option<FloorPool>,
}

impl SwapQuote {
    /// Every array the transaction must carry: the trader's walk and every action's, deduplicated.
    pub fn all_arrays(&self) -> Vec<i32> {
        let mut v = self.arrays_touched.clone();
        for s in &self.action_arrays {
            if !v.contains(s) {
                v.push(*s);
            }
        }
        v
    }
}

/// Quote one swap on a FLOOR pool (or a plain one), from decoded account state.
///
/// `hook` is the pool's `FloorPool`; it is required exactly when the pool's hook is `floor_hook` (on
/// any cluster in `floor_types::ids::CLUSTERS`; [`quote_swap_for`] names one) and
/// sets a swap flag (`QuoteHookStateMissing` otherwise). A pool whose hook sets no swap flag is quoted
/// as plain and `hook` is ignored; any other hook with swap flags is refused
/// (`QuoteHookNotQuotable`). The swap itself is `carpenter_quote::quote_swap` with a
/// [`FloorHookQuoter`].
pub fn quote_swap(pool: &PoolAccount, arrays: &TickArraySet, hook: Option<&FloorPool>, params: &SwapQuoteParams) -> QuoteResult<SwapQuote> {
    quote_with(pool, arrays, FloorHookQuoter::new(hook.copied(), params.cushion_top), params)
}

/// [`quote_swap`] for a pool of `cluster`: a pool hooked by another cluster's `floor_hook` is refused
/// (`QuoteHookNotQuotable`), as on a cluster where that program is not an approved hook.
pub fn quote_swap_for(
    cluster: &ClusterIds,
    pool: &PoolAccount,
    arrays: &TickArraySet,
    hook: Option<&FloorPool>,
    params: &SwapQuoteParams,
) -> QuoteResult<SwapQuote> {
    quote_with(pool, arrays, FloorHookQuoter::for_cluster(hook.copied(), params.cushion_top, cluster), params)
}

fn quote_with(pool: &PoolAccount, arrays: &TickArraySet, mut q: FloorHookQuoter, params: &SwapQuoteParams) -> QuoteResult<SwapQuote> {
    let base = carpenter_quote::quote_swap(pool, arrays, Some(&mut q), &params.amm())?;
    let floor_after = if is_swap_hooked(&pool.pool) { q.state } else { None };
    Ok(SwapQuote {
        amount_in: base.amount_in,
        amount_out: base.amount_out,
        fees: QuoteFees {
            lp: base.fees.lp,
            skim: q.taken.skim,
            creator: q.taken.creator,
            holder: q.taken.holder,
            platform_cut: q.taken.platform_cut,
            team: q.taken.team,
            protocol: base.fees.protocol,
            protocol_currency: base.fees.protocol_currency,
        },
        sqrt_after: base.sqrt_after,
        tick_after: base.tick_after,
        sqrt_after_trade: base.sqrt_after_trade,
        tick_after_trade: base.tick_after_trade,
        arrays_touched: base.arrays_touched,
        replace_actions: base.hook_actions,
        action_arrays: base.action_arrays,
        replace: q.replace,
        signals: base.signals,
        swap_delta: base.swap_delta,
        hook_delta: base.hook_delta,
        fee_split0: base.fee_split0,
        fee_split1: base.fee_split1,
        trader_delta: base.trader_delta,
        pool_after: base.pool_after,
        arrays_after: base.arrays_after,
        floor_after,
    })
}

#[cfg(test)]
mod cluster_tests {
    use super::*;
    use crate::clmm::tick_math;
    use crate::synth::{build_pool, floor_pool_defaults, PoolKeys, SynthPool, SynthPosition, FLOOR};
    use floor_types::ids::DEVNET;

    /// A synthetic second cluster, test-only.
    const OTHER: ClusterIds = ClusterIds {
        name: "other",
        carpenter_amm: [0xC1; 32],
        floor_hook: [0xC2; 32],
        floor_launch: [0xC3; 32],
        floor_holders: [0xC4; 32],
        floor_vesting: [0xC5; 32],
    };

    fn floor_pool(hook: [u8; 32]) -> (SynthPool, FloorPool) {
        let k = PoolKeys {
            pool: [7; 32],
            mint0: [1; 32],
            mint1: [2; 32],
            vault0: [3; 32],
            vault1: [4; 32],
            token_program0: [5; 32],
            token_program1: [6; 32],
            hook,
            hook_authority: [9; 32],
        };
        let tick = 100_020;
        let sqrt = tick_math::get_sqrt_price_at_tick(tick).unwrap();
        let positions = [SynthPosition { tick_lower: tick - 12_000, tick_upper: tick + 12_000, liquidity: 50_000_000_000_000 }];
        let sp = build_pool(&k, FLOOR, 60, sqrt, 9, 6, &positions).unwrap();
        let mut fp = floor_pool_defaults(k.pool, k.mint0, k.mint1, [8; 32]);
        fp.ladder_seeded = 1;
        fp.floor_tick = tick + 30_000;
        (sp, fp)
    }

    fn params() -> SwapQuoteParams {
        SwapQuoteParams { zero_for_one: true, amount_specified: -1_000_000_000, sqrt_price_limit_x64: carpenter_quote::no_limit(true), supply0: 0, supply1: 1_000_000_000_000_000, cushion_top: None }
    }

    #[test]
    fn new_accepts_every_shipped_clusters_hook_and_no_other() {
        let q = FloorHookQuoter::new(None, None);
        for c in floor_types::ids::CLUSTERS {
            assert!(q.accepts(&c.floor_hook), "{}", c.name);
        }
        assert!(!q.accepts(&OTHER.floor_hook), "the synthetic cluster is not shipped");
        assert!(!q.accepts(&DEVNET.carpenter_amm));
    }

    #[test]
    fn for_cluster_accepts_that_clusters_hook_only() {
        let other = FloorHookQuoter::for_cluster(None, None, &OTHER);
        assert!(other.accepts(&OTHER.floor_hook));
        assert!(!other.accepts(&DEVNET.floor_hook));
        assert_eq!(other.program_id(), OTHER.floor_hook);
        let devnet = FloorHookQuoter::for_cluster(None, None, &DEVNET);
        assert!(devnet.accepts(&DEVNET.floor_hook));
        assert!(!devnet.accepts(&OTHER.floor_hook));
    }

    /// The same pool under the synthetic cluster's hook id quotes exactly as under devnet's, through
    /// that cluster's quoter; the other cluster's quoter refuses it by name.
    #[test]
    fn a_pool_of_another_cluster_quotes_through_its_own_cluster_and_is_refused_by_a_mismatched_one() {
        let (dev_sp, dev_fp) = floor_pool(DEVNET.floor_hook);
        let (oth_sp, oth_fp) = floor_pool(OTHER.floor_hook);
        let dev = quote_swap_for(&DEVNET, &dev_sp.account, &dev_sp.tick_array_set().unwrap(), Some(&dev_fp), &params()).unwrap();
        let oth = quote_swap_for(&OTHER, &oth_sp.account, &oth_sp.tick_array_set().unwrap(), Some(&oth_fp), &params()).unwrap();
        assert_eq!((dev.amount_in, dev.amount_out, dev.fees), (oth.amount_in, oth.amount_out, oth.fees));
        assert!(oth.fees.skim > 0, "the hook ran");
        assert_eq!(
            quote_swap_for(&DEVNET, &oth_sp.account, &oth_sp.tick_array_set().unwrap(), Some(&oth_fp), &params()).unwrap_err(),
            QuoteError::QuoteHookNotQuotable
        );
        assert_eq!(
            quote_swap_for(&OTHER, &dev_sp.account, &dev_sp.tick_array_set().unwrap(), Some(&dev_fp), &params()).unwrap_err(),
            QuoteError::QuoteHookNotQuotable
        );
        // `quote_swap` knows the shipped clusters only
        assert_eq!(
            quote_swap(&oth_sp.account, &oth_sp.tick_array_set().unwrap(), Some(&oth_fp), &params()).unwrap_err(),
            QuoteError::QuoteHookNotQuotable
        );
    }
}
