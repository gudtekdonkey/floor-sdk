// SPDX-License-Identifier: Apache-2.0
use jupiter_amm_interface::{AmmError, Swap};

pub use floor_amm_solana::encode::{BUY_DISCRIMINATOR, SELL_DISCRIMINATOR};

pub fn placeholder(is_buy: bool) -> Swap {
    Swap::Placeholder { data: Some(vec![is_buy as u8]) }
}

pub fn direction_of(swap: &Swap) -> Result<bool, AmmError> {
    match swap {
        Swap::Placeholder { data: Some(d) } if d.len() == 1 && d[0] <= 1 => Ok(d[0] == 1),
        Swap::Placeholder { data } => Err(AmmError::Custom(format!("floor-amm Placeholder data must be one byte (is_buy), got {data:?}"))),
        other => Err(AmmError::Custom(format!("floor-amm encodes Swap::Placeholder only, got {other:?}"))),
    }
}

pub fn swap_instruction_data(swap: &Swap, in_amount: u64) -> Result<Vec<u8>, AmmError> {
    let is_buy = direction_of(swap)?;
    Ok(floor_amm_solana::encode::instruction_data(is_buy, in_amount, 0).to_vec())
}
