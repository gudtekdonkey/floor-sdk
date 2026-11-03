// SPDX-License-Identifier: Apache-2.0
use floor_amm_solana::metas::{self, SwapAccounts};
use jupiter_amm_interface::SwapParams;
use solana_instruction::AccountMeta;
use solana_pubkey::Pubkey;

pub fn swap_accounts(p: &SwapParams) -> SwapAccounts {
    SwapAccounts { user: p.user, token_transfer_authority: p.token_transfer_authority, source_token_account: p.source_token_account, destination_token_account: p.destination_token_account }
}

pub fn swap_metas(key: &Pubkey, base_mint: &Pubkey, quote_mint: &Pubkey, quote_token_program: &Pubkey, swap_params: &SwapParams, is_buy: bool) -> Vec<AccountMeta> {
    metas::swap_metas(key, base_mint, quote_mint, quote_token_program, &swap_accounts(swap_params), is_buy)
}
