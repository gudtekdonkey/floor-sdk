// SPDX-License-Identifier: Apache-2.0
#![forbid(unsafe_code)]

pub mod amm;
pub mod encode;
pub mod keys;
pub mod v2;

pub use encode::swap_instruction_data;
pub use v2::FloorAmmV2;

pub const LABEL: jupiter_amm_interface::AmmLabel = "FLOOR";

pub use floor_amm_solana::SWAP_METAS;

pub const SWAP_COMPUTE_UNIT_LIMIT: u32 = 900_000;
pub const SWAP_COMPUTE_UNITS_MEASURED_MAX: u32 = 266_657;
const _: () = assert!(SWAP_COMPUTE_UNIT_LIMIT as u64 * 100 >= SWAP_COMPUTE_UNITS_MEASURED_MAX as u64 * 115 && SWAP_COMPUTE_UNIT_LIMIT <= 1_400_000);
