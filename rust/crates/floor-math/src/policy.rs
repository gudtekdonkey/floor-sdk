//! The EVM's `FloorPolicy` restated for Solana (the min bid in lamports). The caps are the contract's;
//! `register_pool` enforces them.

/// 0.05 SOL. ⚠ A PLANNING value, not a conversion of the EVM's `MIN_BID_ETH = 0.001 ether`.
pub const MIN_BID_LAMPORTS: u64 = 50_000_000;
/// 2.5 % of every quote inflow (`FloorPolicy`'s platform cut).
pub const PLATFORM_BPS: u16 = 250;
// ---- THE FLOOR PLATFORM FEE ("team fee" in code) ----------------------------------------------
// A sale rate and a pool rate, each a governance-settable CURRENT SETTING in `LaunchConfig` (defaults below; all-zero
// bytes read as them) under a CAP in code, copied into each launch / pool at its CREATION and kept for life. Every
// arithmetic in this crate takes the ACCOUNT's own rate (`FloorState.team_fee_bps`, `Launch.team_bps`), never these;
// they are here for the parity table and the figures. Each is pinned to its floor-types twin (`floor_types_pin`).
/// The sale's default rate, on every bonding-sale buy and sell-back — 1 %.
pub const TEAM_SALE_FEE_BPS_DEFAULT: u16 = 100;
/// The most governance may set for the sale — 1 %.
pub const TEAM_SALE_FEE_BPS_MAX: u16 = 100;
/// The pool's default rate, on every swap after graduation — 0.25 %.
pub const TEAM_POOL_FEE_BPS_DEFAULT: u16 = 25;
/// The most governance may set for a pool — 0.5 %.
pub const TEAM_POOL_FEE_BPS_MAX: u16 = 50;
/// Hard cap on a launch's FAR floor-skim rate, each side (`FloorPolicy.MAX_SKIM_FAR_BPS`). No skim setter exists on
/// either chain, and the at-floor SELL endpoint is DERIVED from the far rate
/// (`constants::SKIM_SELL_AT_FLOOR_MULTIPLE_BPS`) and NOT capped by this: 10 % far → 15 %.
pub const MAX_SKIM_FAR_BPS: u16 = 1000;
/// Creator fee cap per side (`FloorPolicy.MAX_CREATOR_BPS`): 5 %.
pub const MAX_CREATOR_BPS: u16 = 500;
/// The EVM holder-tax cap (`FloorPolicy.MAX_HOLDER_BPS`), 1000 (10 %). The two caps are EQUAL (below); this name is
/// kept for the parity table and `fees_check`'s f16.
pub const MAX_HOLDER_BPS_EVM: u16 = 1000;
/// The Solana holder cap: the EVM's, 10 % a side. `floor_types::constants::MAX_HOLDER_BPS` carries the same value;
/// `tests/floor_types_pin.rs` holds this equal to it.
pub const MAX_HOLDER_BPS: u16 = 1000;
const _: () = assert!(MAX_HOLDER_BPS == MAX_HOLDER_BPS_EVM, "the Solana holder cap IS the EVM's");
// Custom's staircase constants are `crate::stair::{CUSTOM_STAIR_TOP_MCAP_USD, CUSTOM_STAIR_GROWTH_BPS}`.
