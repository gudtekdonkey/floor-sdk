// SPDX-License-Identifier: Apache-2.0
use anchor_lang::{prelude::*, solana_program::instruction::Instruction};

pub const FLOOR_PROGRAM_ID: Pubkey = pubkey!("GUhcaRccPyjdXtXmRWWopvsERfTEweqSS1Sa1FsFLooR");

const BUY_DISCRIMINATOR: [u8; 8] = [102, 6, 61, 18, 1, 218, 235, 234];
const SELL_DISCRIMINATOR: [u8; 8] = [51, 230, 133, 164, 1, 127, 131, 173];

pub fn swap(is_buy: bool, amount_in: u64, account_metas: &[AccountMeta]) -> Result<Vec<Instruction>> {
    let mut data = Vec::with_capacity(24);
    data.extend_from_slice(if is_buy { &BUY_DISCRIMINATOR } else { &SELL_DISCRIMINATOR });
    data.extend_from_slice(&amount_in.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    Ok(vec![Instruction { program_id: FLOOR_PROGRAM_ID, accounts: account_metas.to_vec(), data }])
}
