// SPDX-License-Identifier: Apache-2.0
pub const BUY_DISCRIMINATOR: [u8; 8] = [102, 6, 61, 18, 1, 218, 235, 234];
pub const SELL_DISCRIMINATOR: [u8; 8] = [51, 230, 133, 164, 1, 127, 131, 173];

pub fn instruction_data(is_buy: bool, amount: u64, min_out: u64) -> [u8; 24] {
    let mut d = [0u8; 24];
    d[..8].copy_from_slice(if is_buy { &BUY_DISCRIMINATOR } else { &SELL_DISCRIMINATOR });
    d[8..16].copy_from_slice(&amount.to_le_bytes());
    d[16..].copy_from_slice(&min_out.to_le_bytes());
    d
}
