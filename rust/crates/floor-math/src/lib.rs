//! FLOOR's mechanism math for Solana.
//!
//! The Solana twin of the EVM's `FloorMath`, `FloorHookMath`, `FloorPositionMath`, `FloorModes`, `FloorPolicy` and
//! the pure parts of `SingletonFloorEngine` / `SingletonFloorProceeds`, in Q64.64 on top of `carpenter-clmm-math`. No
//! Solana dependency: one implementation for the hook program, the quoter and its wasm.
//!
//! The core functions are re-exported at the root:
//! [`skim_bps`], [`ceil_to_spacing`], [`floor_to_spacing`], [`sqrt_price_from_ratio`], [`ask_tick`],
//! [`position_liquidity`], [`position_amounts`], [`bid_amounts`], [`pol_base`], [`quote_cost`],
//! [`solve_floor_tick_with`], [`sale_tokens_for`], [`tranche_price_x64`], [`open_tick`]; the bid band's
//! [`band_ticks`], [`dear_edge`], [`publish`], [`bid_width_or_spacing`]; plus [`FloorState`],
//! [`take_fees`], [`intake`], [`modes::preset`]; the holder tally, [`holders`] — `catch_up`, `open`, `settle`,
//! `claimable`, the ONE implementation `floor_holders`, the quoter and the wasm all run; and three-tier referrals,
//! [`referral_tier_dues`], the ONE spelling of the 90 / 9 / 1 cascade every claim, quoter, SDK and indexer runs.
//!
//! The bid is a band and `floor_tick` its cheap edge (`band.rs`); `skim_bps` takes the per-pool at-floor endpoints.

#![deny(unsafe_code)]

pub mod band;
pub mod constants;
pub mod error;
pub mod fees;
pub mod genesis;
pub mod genesis_layout;
pub mod holders;
pub mod lineage;
pub mod modes;
pub mod netflow;
pub mod owners;
pub mod policy;
pub mod position;
/// The preset registry's one rule table and seed rows.
pub mod presets;
pub mod price;
pub mod ramp;
pub mod referral;
pub mod slots;
pub mod sale;
pub mod solve;
pub mod spacing;
pub mod stair;
pub mod state;
pub mod trader_ref;

pub use error::{FloorMathError, FloorMathResult};
pub use band::{band_ticks, bid_width_or_spacing, dear_edge, live_floor_tick, publish};
pub use fees::{intake, take_fees, FeeTake};
pub use genesis::{genesis_allocation, genesis_open_tick, genesis_split};
pub use modes::{preset, tranche_price_x64, Mode, Preset};
pub use position::{base_at_sqrt, bid_amounts, pol_base, position_amounts, position_liquidity, quote_cost};
pub use price::{ask_tick, open_tick, sqrt_price_from_ratio};
pub use lineage::{chain_hop_tick, chained_price_x64, min_bid_for, quote_amount_at_sqrt, quote_amount_at_tick};
pub use ramp::skim_bps;
pub use referral::{referral_tier_due, referral_tier_dues, referral_tier_earned, referral_tier_wallet};
pub use slots::{ask_paid, cascade, check_slots, in_weight, part, slot_cliff_part, slot_due, slot_due_with_credit, slot_earned, Slot};
pub use sale::{sale_buy_with_team, sale_tokens_for, sell_back_payout_at, sell_back_settle, sell_back_with_team, team_fee_of, team_gross_for, team_net_of};
pub use solve::{circulating_supply, sellable_at, sellable_at_with, solve_floor_tick_with, solve_floor_tick_with_more, solve_floor_tick_within, MorePosition};
pub use stair::{custom_stair_steps, stair_split, step_tokens, StairSplit};
pub use spacing::{ceil_to_spacing, floor_to_spacing};
pub use state::{FloorState, PodU128, FLOOR_STATE_SIZE};
pub use trader_ref::{trader_link_tiers_well_formed, trader_ref_floor_keeps, trader_ref_slices};
