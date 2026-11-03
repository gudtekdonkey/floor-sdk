// SPDX-License-Identifier: Apache-2.0
use solana_pubkey::{pubkey, Pubkey};

pub const PROGRAM_ID: Pubkey = pubkey!("GUhcaRccPyjdXtXmRWWopvsERfTEweqSS1Sa1FsFLooR");
pub const TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const TOKEN_2022_PROGRAM_ID: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
pub const ASSOCIATED_TOKEN_PROGRAM_ID: Pubkey = pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
pub const SYSTEM_PROGRAM_ID: Pubkey = pubkey!("11111111111111111111111111111111");

pub fn launch_pda(base_mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"launch", base_mint.as_ref()], &PROGRAM_ID).0
}

pub fn event_authority_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"__event_authority"], &PROGRAM_ID).0
}

pub fn ata(holder: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[holder.as_ref(), token_program.as_ref(), mint.as_ref()], &ASSOCIATED_TOKEN_PROGRAM_ID).0
}

pub fn trader_ref_pda(wallet: &Pubkey, quote_mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"trader_ref", wallet.as_ref(), quote_mint.as_ref()], &PROGRAM_ID).0
}

pub fn trader_link_pda(wallet: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"trader_link", wallet.as_ref()], &PROGRAM_ID).0
}
