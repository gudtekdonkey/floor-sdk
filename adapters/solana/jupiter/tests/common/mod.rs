// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

pub mod v2;

use std::collections::HashMap;

use jupiter_amm_interface::AccountProvider;
use solana_account::Account;
use solana_pubkey::{pubkey, Pubkey};

pub const BASE_MINT: Pubkey = Pubkey::new_from_array([0xF1; 32]);
pub const QUOTE_MINT: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

pub struct Accounts(pub HashMap<Pubkey, Account>);

impl AccountProvider for Accounts {
    fn get(&self, pubkey: &Pubkey) -> Option<impl solana_account::ReadableAccount + use<'_>> {
        self.0.get(pubkey).cloned()
    }
}

pub fn mint_data(supply: u64, decimals: u8) -> Vec<u8> {
    let mut d = vec![0u8; 82];
    d[36..44].copy_from_slice(&supply.to_le_bytes());
    d[44] = decimals;
    d[45] = 1;
    d
}

pub fn account(program: Pubkey, data: Vec<u8>) -> Account {
    let mut a = Account::new(1_000_000_000, 0, &program);
    a.data = data;
    a
}

pub fn quote_mint_account() -> Account {
    account(pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"), mint_data(0, 6))
}

pub fn base_mint_account(supply: u64) -> Account {
    account(pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"), mint_data(supply, 6))
}

pub fn accounts(entries: impl IntoIterator<Item = (Pubkey, Account)>) -> Accounts {
    let mut m: HashMap<Pubkey, Account> = entries.into_iter().collect();
    m.entry(QUOTE_MINT).or_insert_with(quote_mint_account);
    m.entry(BASE_MINT).or_insert_with(|| base_mint_account(u64::MAX));
    Accounts(m)
}
