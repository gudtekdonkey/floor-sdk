//! FLOOR's refusals. They travel inside `carpenter_quote::QuoteError` as `QuoteError::Hook(name)`,
//! under the on-chain error name of the program that raises them (`floor_hook`, `floor_launch`,
//! `floor_holders`), so a parity harness compares names exactly as it does for the AMM's own. This
//! enum is the typed list; `QuoteError` compares equal to it.

use floor_math::FloorMathError;

pub use carpenter_quote::{QuoteError, QuoteResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloorError {
    // ---- quoter-only --------------------------------------------------------------------------
    /// A `FloorPool` with `quote_is_token1 == 1`. The quoter's tick-space math (the dear edge, the
    /// band `deploy_bid` places, the nudge's limit) is written for quote = token0 and does not do the
    /// tick negation the EVM pairs with its `quoteIsCurrency1` branches. On chain the state cannot
    /// exist (`before_initialize` pins `mint0 == quote_mint`), so this refuses hand-built bytes.
    QuoteReverseOrientation,

    // ---- floor_hook ---------------------------------------------------------------------------
    /// An exact-out swap on a FLOOR pool.
    ExactOutNotSupported,
    /// `floor_pool.pool_bound != 1`.
    NotBound,
    /// `floor_pool.amm_pool != pool`, or its mints / spacing disagree with the pool's.
    WrongPool,
    /// `floor_pool.ladder_seeded != 1`: the launch has not graduated, so its pool has not been seeded.
    /// `floor_hook`'s `before_swap` refuses every swap with it (error 7021), after `NotBound` and
    /// before any other check.
    NotSeeded,
    /// A FLOOR pool whose key fee is not the dynamic flag.
    NotDynamicFee,
    /// An epilogue (I-3: claims cover the idle liabilities) the hook asserts on every callback would
    /// fail, or a state the hook refuses (a pool that is not a staircase).
    InvariantViolated,

    // ---- floor_launch -------------------------------------------------------------------------
    /// The launch is not bonding.
    SaleOver,
    /// A buy that buys no token, or a sell-back of zero.
    Nothing,
    /// `tokens > allocation`.
    InsufficientAllocation,

    // ---- floor_holders ------------------------------------------------------------------------
    /// The `HolderRecord` / `HolderBook` / `FloorPool` handed in are not one launch's.
    WrongMint,
    /// A `HolderBook` read against a `FloorPool` whose `holders_hooked` is 0.
    NotHooked,
}

impl FloorError {
    /// The on-chain error name (`QuoteReverseOrientation` has no on-chain twin).
    pub fn name(&self) -> &'static str {
        use FloorError::*;
        match self {
            QuoteReverseOrientation => "QuoteReverseOrientation",
            ExactOutNotSupported => "ExactOutNotSupported",
            NotBound => "NotBound",
            WrongPool => "WrongPool",
            NotSeeded => "NotSeeded",
            NotDynamicFee => "NotDynamicFee",
            InvariantViolated => "InvariantViolated",
            SaleOver => "SaleOver",
            Nothing => "Nothing",
            InsufficientAllocation => "InsufficientAllocation",
            WrongMint => "WrongMint",
            NotHooked => "NotHooked",
        }
    }
}

impl From<FloorError> for QuoteError {
    fn from(e: FloorError) -> Self {
        QuoteError::Hook(e.name())
    }
}

impl PartialEq<FloorError> for QuoteError {
    fn eq(&self, other: &FloorError) -> bool {
        *self == QuoteError::from(*other)
    }
}

impl PartialEq<QuoteError> for FloorError {
    fn eq(&self, other: &QuoteError) -> bool {
        QuoteError::from(*self) == *other
    }
}

/// A `floor-math` refusal, by the name the hook program reports it under. `Math(e)` keeps the
/// clmm-math name (`TickOutOfRange`, …); every other variant is checked arithmetic the program
/// surfaces as `MathOverflow`. The match is exhaustive on purpose: a new refusal in `floor-math` is a
/// compile error here.
pub fn fm(e: FloorMathError) -> QuoteError {
    match e {
        FloorMathError::Math(m) => QuoteError::from(m),
        FloorMathError::ZeroDenominator
        | FloorMathError::Overflow
        | FloorMathError::SupplyUnderflow
        | FloorMathError::InvalidTickSpacing
        | FloorMathError::InvalidRange
        | FloorMathError::TranchePriceOutOfRange
        | FloorMathError::UnknownMode
        | FloorMathError::ReferralShareOutOfRange
        // create-time functions the quoter never calls; listed, not wildcarded
        | FloorMathError::QuoteDecimalsOutOfRange
        | FloorMathError::BadSplit => QuoteError::Overflow,
    }
}
