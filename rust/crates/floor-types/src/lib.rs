//! `floor-types`: the bytes every FLOOR program, harness and client agrees on — the MECHANISM's
//! half. The AMM's half is [`carpenter_types`], which this crate depends on and re-exports under
//! the same module paths, so a FLOOR-side consumer spells one crate.
//!
//! - [`ids`]: `floor_hook`, `floor_launch`, `floor_vesting` and `floor_holders`' program IDs, beside the AMM's and the mock hook's re-exported from `carpenter_types::ids`.
//! - [`keys`]: well-known foreign program and mint keys — `carpenter_types::keys`, re-exported whole.
//! - [`seeds`]: FLOOR's PDA seed prefixes (`floor_hook`, `floor_launch`), beside the AMM's and the hook
//!   interface's re-exported from `carpenter_types::seeds`, plus the seed-byte helpers.
//! - [`constants`]: FLOOR's mechanism table, the launch enums and the create bounds, beside
//!   the AMM constants and the hook flag bits re-exported from `carpenter_types::constants`.
//! - [`pod`]: `U128` / `I128` — `carpenter_types::pod`, re-exported whole.
//! - [`layout`]: the account structs (`HookConfig`, `FloorPool`, `LaunchConfig`,
//!   `RegistryEntry`, `Launch`, `BuyerPosition`), built with `carpenter_types::layout!` on the same
//!   `Layout` trait, beside the AMM's accounts and `HookInterfaceHeader` re-exported from there.
//! - [`hook`]: the FROZEN hook interface and the pure record rules —
//!   `carpenter_types::hook`, re-exported whole: it is the AMM's interface, which FLOOR implements.
//! - [`register`]: `RegisterPoolArgs`, the launch → hook interface. Its last field is the
//!   `referrer` wallet.
//! - [`holders`]: `floor_holders` (holder rewards) — its
//!   id (re-exported from `ids`), seeds, `HolderBook` / `HolderRecord` / `ClaimsRegistry`, and the `FloorPool`
//!   account offsets the tally reads raw. The arithmetic is `floor_math::holders`.
//! - [`metadata`]: the mint metadata URI template — the base validator and the join.
//! - [`disc`]: `floor_hook`'s and `floor_launch`'s instruction and account discriminators as
//!   bytes, beside the AMM's and the callbacks' re-exported from `carpenter_types::disc`.
//!
//! ⛔ No Solana crate, ever. Keys are `[u8; 32]`.
//! ⛔ The arrow is `floor-types → carpenter-types` and never back: nothing here is reachable from
//! the AMM program, which links `carpenter-types` alone (`tests/boundary.rs`).
#![forbid(unsafe_code)]

pub mod constants;
pub mod disc;
pub mod holders;
pub mod ids;
pub mod layout;
pub mod metadata;
pub mod owners;
pub mod register;
pub mod seeds;
/// The payee slots in a `FloorPool` account's tail.
pub mod slots;
/// The pool-price TWAP: the oracle in a `FloorPool` account's tail.
pub mod twap;

pub use carpenter_types::{hook, keys, pod, protocol_fee, I128, U128};

pub mod payout;
