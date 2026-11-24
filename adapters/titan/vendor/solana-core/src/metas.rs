// SPDX-License-Identifier: Apache-2.0
use solana_instruction::AccountMeta;
use solana_pubkey::Pubkey;

use crate::hook::{holder_position_pda, hook_metas, trade_hook_metas};
use crate::keys::{self, PROGRAM_ID, TOKEN_2022_PROGRAM_ID};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwapAccounts {
    pub user: Pubkey,
    pub token_transfer_authority: Pubkey,
    pub source_token_account: Pubkey,
    pub destination_token_account: Pubkey,
}

impl SwapAccounts {
    pub fn trader_accounts(&self, is_buy: bool) -> (Pubkey, Pubkey) {
        if is_buy {
            (self.destination_token_account, self.source_token_account)
        } else {
            (self.source_token_account, self.destination_token_account)
        }
    }
}

pub fn trader_party(account: &Pubkey, accounts: &SwapAccounts, mint: &Pubkey, token_program: &Pubkey) -> Pubkey {
    if *account == keys::ata(&accounts.user, mint, token_program) {
        accounts.user
    } else {
        accounts.token_transfer_authority
    }
}

pub fn swap_metas(key: &Pubkey, base_mint: &Pubkey, quote_mint: &Pubkey, quote_token_program: &Pubkey, a: &SwapAccounts, is_buy: bool) -> Vec<AccountMeta> {
    let (trader_base, trader_quote) = a.trader_accounts(is_buy);
    let recipient = if is_buy { trader_party(&trader_base, a, base_mint, &TOKEN_2022_PROGRAM_ID) } else { trader_party(&trader_quote, a, quote_mint, quote_token_program) };
    let trader_ref = keys::trader_ref_pda(&a.user, quote_mint);
    vec![
        AccountMeta::new(a.token_transfer_authority, true),
        AccountMeta::new(*key, false),
        AccountMeta::new_readonly(*base_mint, false),
        AccountMeta::new_readonly(*quote_mint, false),
        AccountMeta::new(keys::ata(key, base_mint, &TOKEN_2022_PROGRAM_ID), false),
        AccountMeta::new(keys::ata(key, quote_mint, quote_token_program), false),
        AccountMeta::new(trader_base, false),
        AccountMeta::new(trader_quote, false),
        AccountMeta::new_readonly(TOKEN_2022_PROGRAM_ID, false),
        AccountMeta::new_readonly(*quote_token_program, false),
        AccountMeta::new(trader_ref, false),
        AccountMeta::new(keys::ata(&trader_ref, quote_mint, quote_token_program), false),
        AccountMeta::new_readonly(keys::event_authority_pda(), false),
        AccountMeta::new_readonly(PROGRAM_ID, false),
        AccountMeta::new(a.user, true),
        AccountMeta::new_readonly(keys::trader_link_pda(&a.user), false),
        AccountMeta::new_readonly(keys::SYSTEM_PROGRAM_ID, false),
        AccountMeta::new_readonly(keys::ASSOCIATED_TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(keys::trader_link_pda(&recipient), false),
    ]
}

pub fn full_swap_metas(key: &Pubkey, base_mint: &Pubkey, quote_mint: &Pubkey, quote_token_program: &Pubkey, quote_hooked: bool, a: &SwapAccounts, is_buy: bool) -> Vec<AccountMeta> {
    let (trader_base, trader_quote) = a.trader_accounts(is_buy);
    let mut m = swap_metas(key, base_mint, quote_mint, quote_token_program, a, is_buy);
    m.extend(trade_hook_metas(base_mint, is_buy, &trader_party(&trader_base, a, base_mint, &TOKEN_2022_PROGRAM_ID)));
    if quote_hooked {
        let holder = trader_party(&trader_quote, a, quote_mint, quote_token_program);
        let (src, dst) = if is_buy { (holder, *key) } else { (*key, holder) };
        m.extend(hook_metas(quote_mint, &src, &dst));
        let trader_ref = keys::trader_ref_pda(&a.user, quote_mint);
        m.push(AccountMeta::new(holder_position_pda(&keys::launch_pda(quote_mint), &trader_ref), false));
    }
    m
}
