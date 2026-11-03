// SPDX-License-Identifier: Apache-2.0
use solana_instruction::AccountMeta;
use solana_pubkey::{pubkey, Pubkey};

use crate::keys::launch_pda;
use crate::SWAP_METAS;

pub const FLOOR_HOOK_ID: Pubkey = pubkey!("HVo1eRwj5SHtjkmHa4cVSV5KeL2BenN2d5qM2bqFLooR");

pub const HOOK_METAS: usize = 6;
pub const SWAP_METAS_V2: usize = SWAP_METAS + HOOK_METAS;
pub const SWAP_METAS_V2_QUOTE_HOOKED: usize = SWAP_METAS_V2 + HOOK_METAS + 1;

pub fn extra_metas_pda(base_mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"extra-account-metas", base_mint.as_ref()], &FLOOR_HOOK_ID).0
}

pub fn holder_book_pda(launch: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"holder_book", launch.as_ref()], &FLOOR_HOOK_ID).0
}

pub fn holder_position_pda(launch: &Pubkey, holder: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"holder", launch.as_ref(), holder.as_ref()], &FLOOR_HOOK_ID).0
}

pub fn hook_metas(base_mint: &Pubkey, src_holder: &Pubkey, dst_holder: &Pubkey) -> [AccountMeta; HOOK_METAS] {
    let launch = launch_pda(base_mint);
    [
        AccountMeta::new_readonly(extra_metas_pda(base_mint), false),
        AccountMeta::new_readonly(launch, false),
        AccountMeta::new(holder_book_pda(&launch), false),
        AccountMeta::new(holder_position_pda(&launch, src_holder), false),
        AccountMeta::new(holder_position_pda(&launch, dst_holder), false),
        AccountMeta::new_readonly(FLOOR_HOOK_ID, false),
    ]
}

pub fn trade_hook_metas(base_mint: &Pubkey, is_buy: bool, token_transfer_authority: &Pubkey) -> [AccountMeta; HOOK_METAS] {
    let launch = launch_pda(base_mint);
    if is_buy {
        hook_metas(base_mint, &launch, token_transfer_authority)
    } else {
        hook_metas(base_mint, token_transfer_authority, &launch)
    }
}
