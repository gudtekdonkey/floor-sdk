//! The program-derived addresses a sale buy or sell-back names, and the `buy` and `sell_back_wsol`
//! instructions' accounts in program order.
//!
//! Every program id comes from a [`ClusterIds`] (`floor_types::ids`): the cluster whose
//! `floor_launch` owns the launch account. Nothing here assumes one cluster.

use floor_types::ids::ClusterIds;
use floor_types::keys::{ASSOCIATED_TOKEN_PROGRAM_ID, SPL_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_2022_PROGRAM_ID, WSOL_MINT};
use floor_types::seeds;
use solana_instruction::AccountMeta;
use solana_pubkey::Pubkey;

/// `floor_launch`'s program id on `cluster`.
pub fn launch_program(cluster: &ClusterIds) -> Pubkey {
    Pubkey::new_from_array(cluster.floor_launch)
}

/// Token-2022's program id. Every launch mint is a Token-2022 mint.
pub fn token_2022() -> Pubkey {
    Pubkey::new_from_array(TOKEN_2022_PROGRAM_ID)
}

/// The SPL Token program (the wSOL mint's program).
pub fn spl_token() -> Pubkey {
    Pubkey::new_from_array(SPL_TOKEN_PROGRAM_ID)
}

/// The wSOL (native) mint.
pub fn wsol_mint() -> Pubkey {
    Pubkey::new_from_array(WSOL_MINT)
}

/// The holder-rewards program a hooked launch's sell-back syncs a holder record through, on `cluster`.
pub fn floor_holders_program(cluster: &ClusterIds) -> Pubkey {
    Pubkey::new_from_array(cluster.floor_holders)
}

/// The associated token account program.
pub fn ata_program() -> Pubkey {
    Pubkey::new_from_array(ASSOCIATED_TOKEN_PROGRAM_ID)
}

/// The system program.
pub fn system_program() -> Pubkey {
    Pubkey::new_from_array(SYSTEM_PROGRAM_ID)
}

/// `["launch", mint]` under `floor_launch`: the market's key.
pub fn launch_pda(cluster: &ClusterIds, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[seeds::LAUNCH, mint.as_ref()], &launch_program(cluster)).0
}

/// `["sale_vault", launch]` under `floor_launch`: the native-SOL escrow of a wSOL-quoted sale.
pub fn sale_vault_pda(cluster: &ClusterIds, launch: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[seeds::SALE_VAULT, launch.as_ref()], &launch_program(cluster)).0
}

/// `["buyer", launch, buyer]` under `floor_launch`: the buyer's position (allocation and recorded
/// cost). Created by the first buy, paid by the buyer.
pub fn buyer_position_pda(cluster: &ClusterIds, launch: &Pubkey, buyer: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[seeds::BUYER, launch.as_ref(), buyer.as_ref()], &launch_program(cluster)).0
}

/// `["lock_authority"]` under `floor_launch`: the pause authority of every sale mint.
pub fn lock_authority_pda(cluster: &ClusterIds) -> Pubkey {
    Pubkey::find_program_address(&[seeds::LOCK_AUTHORITY], &launch_program(cluster)).0
}

/// `["__event_authority"]` under `floor_launch` (Anchor's event-CPI signer).
pub fn event_authority_pda(cluster: &ClusterIds) -> Pubkey {
    Pubkey::find_program_address(&[seeds::EVENT_AUTHORITY], &launch_program(cluster)).0
}

/// The Token-2022 associated token account of `wallet` for `mint`: the only account a buy delivers
/// into.
pub fn token_2022_ata(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[wallet.as_ref(), token_2022().as_ref(), mint.as_ref()], &ata_program()).0
}

/// The SPL Token associated token account of `wallet` for the wSOL mint: the account a sell-back's
/// wSOL payout is delivered into.
pub fn wsol_ata(wallet: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[wallet.as_ref(), spl_token().as_ref(), wsol_mint().as_ref()], &ata_program()).0
}

/// `["holder_book", mint]` under the holder-rewards program (a hooked launch only).
pub fn holder_book_pda(cluster: &ClusterIds, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[seeds::HOLDER_BOOK, mint.as_ref()], &floor_holders_program(cluster)).0
}

/// `["holder", mint, token_account]` under the holder-rewards program: a holder's record, keyed on its
/// token account (a hooked launch only; it need not exist).
pub fn holder_record_pda(cluster: &ClusterIds, mint: &Pubkey, token_account: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[seeds::HOLDER_RECORD, mint.as_ref(), token_account.as_ref()], &floor_holders_program(cluster)).0
}

/// The keys of one buy or sell-back that come from the `Launch` account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaunchKeys {
    pub launch: Pubkey,
    pub mint: Pubkey,
    pub token_vault: Pubkey,
    /// `Launch.floor_pool`: named by a hooked launch's sell-back tail.
    pub floor_pool: Pubkey,
    /// The mint carries the holder-rewards transfer-hook slot (a holder rate on either side).
    pub hooked: bool,
}

/// `buy`'s named accounts in program order, for `buyer`:
///
/// ```text
///  0 buyer           (w, s)  the wallet: pays the input in native SOL, the position's rent and
///                            (if absent) its token account's rent
///  1 launch          (w)
///  2 sale_vault      (w)     ["sale_vault", launch]
///  3 token_vault     (w)     Launch.token_vault
///  4 buyer_position  (w)     ["buyer", launch, buyer]
///  5 buyer_ata       (w)     the buyer's Token-2022 ATA for the mint
///  6 mint            (w)
///  7 lock_authority          ["lock_authority"]
///  8 ata_program
///  9 token_2022
/// 10 system_program
/// 11 event_authority         ["__event_authority"]
/// 12 program                 floor_launch
/// ```
pub fn buy_metas(cluster: &ClusterIds, keys: &LaunchKeys, buyer: &Pubkey) -> Vec<AccountMeta> {
    vec![
        AccountMeta::new(*buyer, true),
        AccountMeta::new(keys.launch, false),
        AccountMeta::new(sale_vault_pda(cluster, &keys.launch), false),
        AccountMeta::new(keys.token_vault, false),
        AccountMeta::new(buyer_position_pda(cluster, &keys.launch, buyer), false),
        AccountMeta::new(token_2022_ata(buyer, &keys.mint), false),
        AccountMeta::new(keys.mint, false),
        AccountMeta::new_readonly(lock_authority_pda(cluster), false),
        AccountMeta::new_readonly(ata_program(), false),
        AccountMeta::new_readonly(token_2022(), false),
        AccountMeta::new_readonly(system_program(), false),
        AccountMeta::new_readonly(event_authority_pda(cluster), false),
        AccountMeta::new_readonly(launch_program(cluster), false),
    ]
}

/// `sell_back_wsol`'s named accounts (`sell_back`'s, in the program's order), then the wSOL pair, then
/// a hooked launch's 4-account holder tail, for `seller` paid into `seller_wsol`:
///
/// ```text
///  0 buyer           (w, s)  the seller: signs, and its position and its token account are debited
///  1 launch          (w)
///  2 sale_vault      (w)     ["sale_vault", launch]: the native escrow the payout leaves
///  3 buyer_position  (w)     ["buyer", launch, seller]: must cover the tokens
///  4 buyer_ata       (w)     the seller's Token-2022 ATA for the mint: the tokens' source
///  5 token_vault     (w)     Launch.token_vault: where the tokens return
///  6 mint            (w)
///  7 lock_authority          ["lock_authority"]
///  8 token_2022
///  9 event_authority         ["__event_authority"]
/// 10 program                 floor_launch
/// 11 seller_wsol     (w)     the seller's SPL Token wSOL account: credited, then `SyncNative`
/// 12 spl_token
/// 13 floor_holders           (hooked only, 13..16)
/// 14 holder_book     (w)     ["holder_book", mint]
/// 15 floor_pool              Launch.floor_pool
/// 16 holder_record   (w)     ["holder", mint, buyer_ata] (synced when it exists)
/// ```
pub fn sell_back_wsol_metas(cluster: &ClusterIds, keys: &LaunchKeys, seller: &Pubkey, seller_wsol: &Pubkey) -> Vec<AccountMeta> {
    let seller_ata = token_2022_ata(seller, &keys.mint);
    let mut metas = vec![
        AccountMeta::new(*seller, true),
        AccountMeta::new(keys.launch, false),
        AccountMeta::new(sale_vault_pda(cluster, &keys.launch), false),
        AccountMeta::new(buyer_position_pda(cluster, &keys.launch, seller), false),
        AccountMeta::new(seller_ata, false),
        AccountMeta::new(keys.token_vault, false),
        AccountMeta::new(keys.mint, false),
        AccountMeta::new_readonly(lock_authority_pda(cluster), false),
        AccountMeta::new_readonly(token_2022(), false),
        AccountMeta::new_readonly(event_authority_pda(cluster), false),
        AccountMeta::new_readonly(launch_program(cluster), false),
        AccountMeta::new(*seller_wsol, false),
        AccountMeta::new_readonly(spl_token(), false),
    ];
    if keys.hooked {
        metas.extend([
            AccountMeta::new_readonly(floor_holders_program(cluster), false),
            AccountMeta::new(holder_book_pda(cluster, &keys.mint), false),
            AccountMeta::new_readonly(keys.floor_pool, false),
            AccountMeta::new(holder_record_pda(cluster, &keys.mint, &seller_ata), false),
        ]);
    }
    metas
}
