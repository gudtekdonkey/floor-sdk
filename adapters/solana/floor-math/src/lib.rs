// SPDX-License-Identifier: Apache-2.0
#[cfg(test)]
extern crate self as floor_math;

pub mod fixed;
pub mod trace;
pub mod u256;
pub mod v2;

pub const BPS: u64 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathError {
    VaultShareOutOfRange,
    Zero,
    Overflow,
    NotDue,
    NothingToDo,
    BookEmpty,
    StepBudget,
    SqrtRange,
    RowTooManyRanges,
    RowBands,
    RowShares,
    RowNotFeasible,
    TeamBuyOverShare,
    LotsFull,
    IndexGuard,
    NothingToSettle,
    SlippageExceeded,
    QuoteRowCannotOpen,
    RangeCover,
    RowLength,
    RowVersion,
    RowEdges,
    RowTail,
    RowBounds,
    RowFees,
    RowTeam,
    RowFlags,
    RowMsClass,
    RowNonCanonical,
}

