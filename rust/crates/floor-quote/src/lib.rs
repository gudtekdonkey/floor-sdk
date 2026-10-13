//! `floor-quote`: FLOOR's quoter for Carpenter pools — `floor_hook`'s swap callbacks replayed
//! off-chain behind `carpenter_quote::HookQuoter`, plus the launch sale, holder-reward and referral
//! quotes.
//!
//! - [`FloorHookQuoter`]: the `HookQuoter` for `floor_hook`. Hand it to
//!   `carpenter_quote::quote_swap` (or `quote_swap_with`, beside other hooks' quoters) to quote a FLOOR
//!   pool; read its `taken`, `replace` and `state` afterwards for FLOOR's breakdown.
//! - [`quote_swap`]: the one-call form — a FLOOR pool's quote with FLOOR's fee breakdown
//!   ([`QuoteFees`]: skim, creator fee, holder tax, platform cut, FLOOR Platform fee, plus the AMM's LP
//!   and Carpenter Platform fees), the bid re-place ([`ReplaceQuote`]) and `FloorPool`'s post-state.
//! - [`floor`]: the three callbacks (`before_swap`, `after_swap`, `after_actions`) and the re-place.
//! - [`sale`]: `sale_buy`, `sale_sell_back`. [`holders`]: `holder_claimable`. [`referral`]:
//!   `referral_tier_dues`.
//! - [`state`], [`price`], [`synth`]: FLOOR's decoders, tick snaps and synthetic pools, over the AMM's
//!   (re-exported from `carpenter_quote`).
//!
//! ⛔ No Solana crate. ⛔ No clock, randomness, float or network: the same bytes in give the same
//! bytes out on every target.
#![forbid(unsafe_code)]

pub mod error;
pub mod floor;
pub mod holders;
pub mod mech;
pub mod price;
pub mod quoter;
pub mod referral;
pub mod sale;
pub mod state;
pub mod synth;

pub use error::{FloorError, QuoteError, QuoteResult};
pub use floor::{bid_holds_tokens, replace_actions, ReplaceBooking};
pub use holders::holder_claimable;
pub use quoter::{quote_swap, quote_swap_for, FloorHookQuoter, QuoteFees, ReplaceQuote, SwapQuote, SwapQuoteParams};
pub use referral::referral_tier_dues;
pub use sale::{sale_buy, sale_sell_back, sale_sell_back_stateless, SaleBuyQuote, SaleSellBackQuote};

pub use carpenter_quote::{amounts_for_liquidity, fees_owed, no_limit, quote_liquidity, PoolAccount, TickArraySet};
/// The AMM's quoter, for callers that compose FLOOR with other hooks.
pub use carpenter_quote as amm;
pub use carpenter_quote::clmm;
/// FLOOR's types (which re-export the AMM's under the same paths).
pub use floor_types as types;
