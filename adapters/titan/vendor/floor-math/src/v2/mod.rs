// SPDX-License-Identifier: Apache-2.0
pub mod ask;
pub mod book;
pub mod circulating;
pub mod create;
pub mod feasibility;
pub mod fees;
pub mod genesis;
pub mod holders;
pub mod marginal;
pub mod milestones;
pub mod preset;
pub mod quote;
pub mod relay;
pub mod rows;
pub mod split;
pub mod sqrt;
pub mod team;
pub mod supply;
pub mod trade;
pub mod wide;
pub mod work;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum V2Error {
    RowInvalid,
    Zero,
    Overflow,
    SupplyShort,
    TargetUnreachable(u8),
    NotHolder,
    WExceedsSupply,
    NotHeld,
}
