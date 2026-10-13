//! `floor_holders` — the holder-rewards tally: its program id, its PDA seeds, its three account
//! layouts, and the `FloorPool` ACCOUNT offsets its `execute` / `claim` read without deserialising
//! the pool.
//!
//! The tally is a Token-2022 TRANSFER HOOK on hooked launches, in its own small program,
//! `floor_holders`, not in `floor_hook`: no rounds, no proofs, no publisher key, no scheduler.
//!
//! ⛔ **`FLOOR_HOLDERS_ID` lives in `crate::ids`** with the other FLOOR programs. This module
//! RE-EXPORTS it; never define it here again (two `FLOOR_HOLDERS_ID`s would compile and disagree).
//!
//! ⛔ No Solana crate. Keys are `[u8; 32]`; seeds are byte strings spelled ONCE, here.
//! ⛔ Offsets are STRUCT offsets; account offset = struct offset + 8 (the Anchor discriminator).
//!
//! What this module does NOT carry, and where it is:
//! - the tally ARITHMETIC — `floor_math::holders` (one implementation: the program, the quoter and
//!   the wasm all run it);
//! - the extra-account-meta TLV itself — built by the program with `spl-tlv-account-resolution`
//!   (kept out of here); only its LENGTH and entry count are pinned below.

use crate::disc::account as disc;
use crate::layout::FloorPool;
use crate::pod::U128;
use bytemuck::{Pod, Zeroable};
use carpenter_types::layout;

// =========================================================================================
// program id
// =========================================================================================

/// `floor_holders`: the Token-2022 transfer-hook tally — base58
/// `58nLaD9bnCV7HirnwJa9kxNWDYFFLeuPRDPV5zSM7oE9`, defined in [`crate::ids`] and re-exported here so the module's
/// consumers keep one path.
pub use crate::ids::FLOOR_HOLDERS_ID;

/// `AddressLookupTab1e1111111111111111111111111` — the native address-lookup-table program, which
/// `rotate_claims_table` / `init_launch` CPI to create and extend the SHARED claims table.
pub const ADDRESS_LOOKUP_TABLE_PROGRAM_ID: [u8; 32] = [
    2, 119, 166, 175, 151, 51, 155, 122, 200, 141, 24, 146, 201, 4, 70, 245, 0, 2, 48, 146, 102, 246, 46, 83, 193, 24,
    36, 73, 130, 0, 0, 0,
];

// =========================================================================================
// seeds (all under FLOOR_HOLDERS_ID)
// =========================================================================================

/// `["holders_authority"]` — the ONE signer `floor_hook::pay_holder` accepts (it re-derives this
/// PDA under `FLOOR_HOLDERS_ID` and compares, never trusts an argument).
pub const HOLDERS_AUTHORITY: &[u8] = b"holders_authority";
/// `["claims_alt_authority"]` — the authority of every SHARED claims lookup table.
pub const CLAIMS_ALT_AUTHORITY: &[u8] = b"claims_alt_authority";
/// `["claims_registry"]` — the singleton naming the current shared claims table.
pub const CLAIMS_REGISTRY: &[u8] = b"claims_registry";
/// `["holder_book", mint]` — one per HOOKED launch.
pub const HOLDER_BOOK: &[u8] = b"holder_book";
/// `["extra-account-metas", mint]` — the transfer-hook interface's OWN seed (the Token-2022
/// interface derives it under the armed program; a different spelling is a hook Token-2022 cannot
/// find). ⛔ The hyphens are the interface's, not a typo for underscores.
pub const EXTRA_ACCOUNT_METAS: &[u8] = b"extra-account-metas";
/// `["holder", mint, token_account]` — one per (mint, ASSOCIATED token account) that has started
/// earning. ⛔ Keyed on the TOKEN ACCOUNT, not the wallet: a transfer hook sees accounts, and a
/// Token-2022 ATA is mint-specific and `ImmutableOwner`, so the account names the holder for life.
pub const HOLDER_RECORD: &[u8] = b"holder";

// =========================================================================================
// the FloorPool fields the tally reads, as ACCOUNT offsets (struct offset + 8)
// =========================================================================================

/// `FloorPool.holder_owed` (struct 432): the holder tax accrued and not yet paid.
/// ⛔ Written ONLY by `floor_math::take_fees` (+) and `floor_hook::pay_holder` (−).
pub const FLOOR_POOL_ACCT_HOLDER_OWED: usize = 440;
/// `FloorPool.holder_paid_eth` (struct 512): lifetime quote paid to holders. Written ONLY by
/// `floor_hook::pay_holder` (+). `holder_owed + holder_paid_eth` is the tally's LIFETIME counter.
pub const FLOOR_POOL_ACCT_HOLDER_PAID: usize = 520;
/// `FloorPool.referrer` (struct 640): the referrer's WALLET; zero = unreferred.
pub const FLOOR_POOL_ACCT_REFERRER: usize = 648;
/// `FloorPool.holders_hooked` (struct 673): 1 iff the base mint carries FLOOR's hook slot.
pub const FLOOR_POOL_ACCT_HOLDERS_HOOKED: usize = 681;
/// `FloorPool.referrer_t2` (struct 680, three-tier referrals); `referrer_t3` follows at +32.
pub const FLOOR_POOL_ACCT_REFERRER_T2: usize = 688;
/// `FloorPool.team_fee_bps` (struct 776): the pool's FLOOR Platform fee rate, a u16 written ONCE by `register_pool`
/// from its trailing `TeamFeeArg` (there is no setter).
pub const FLOOR_POOL_ACCT_TEAM_FEE_BPS: usize = 784;
/// `FloorPool.skim_sell_at_floor_bps` (struct 534): the at-floor SELL skim, a u16 written ONCE by
/// `register_pool` as `far · multiple / 10,000`. The multiple is PER LAUNCH, carried in `register_pool`'s trailing
/// `SkimSellMultipleArg`.
pub const FLOOR_POOL_ACCT_SKIM_SELL_AT_FLOOR_BPS: usize = 542;
/// `FloorPool.stair_steps` / `stair_ready` / `stair_growth_bps` as ACCOUNT offsets (struct + 8) — what graduation and
/// `prepare_graduation` read to know a pool places a staircase (`graduate::pool_stair_steps`). `stair_ready` is 1 from
/// the graduating buy on (the staircase is placed inside it).
pub const FLOOR_POOL_ACCT_STAIR_STEPS: usize = 642;
pub const FLOOR_POOL_ACCT_STAIR_READY: usize = 643;
pub const FLOOR_POOL_ACCT_STAIR_GROWTH_BPS: usize = 644;
/// `FloorPool.stair_kind` as an ACCOUNT offset.
pub const FLOOR_POOL_ACCT_STAIR_KIND: usize = 646;
/// Team steps and the team buy: `FloorPool.stair_owner`'s ACCOUNT offset (8 + 639) —
/// `floor_launch::create_launch` and `end_genesis_sale` read it back after the hook writes it.
pub const FLOOR_POOL_ACCT_STAIR_OWNER: usize = 8 + core::mem::offset_of!(crate::layout::FloorPool, stair_owner);
const _: () = assert!(FLOOR_POOL_ACCT_STAIR_OWNER == 647);


// ⛔ The offset constants above are the pool's OWN offsets, asserted at compile time so a carve that
// moves a field cannot leave a reader of raw account bytes pointed at the wrong eight.
const _: () = {
    assert!(core::mem::offset_of!(FloorPool, holder_owed) + 8 == FLOOR_POOL_ACCT_HOLDER_OWED);
    assert!(core::mem::offset_of!(FloorPool, holder_paid_eth) + 8 == FLOOR_POOL_ACCT_HOLDER_PAID);
    assert!(core::mem::offset_of!(FloorPool, referrer) + 8 == FLOOR_POOL_ACCT_REFERRER);
    assert!(core::mem::offset_of!(FloorPool, referrer_t2) + 8 == FLOOR_POOL_ACCT_REFERRER_T2);
    assert!(core::mem::offset_of!(FloorPool, referrer_t3) + 8 == FLOOR_POOL_ACCT_REFERRER_T2 + 32);
    assert!(core::mem::offset_of!(FloorPool, team_fee_bps) + 8 == FLOOR_POOL_ACCT_TEAM_FEE_BPS);
    assert!(core::mem::offset_of!(FloorPool, holders_hooked) + 8 == FLOOR_POOL_ACCT_HOLDERS_HOOKED);
    assert!(core::mem::offset_of!(FloorPool, skim_sell_at_floor_bps) + 8 == FLOOR_POOL_ACCT_SKIM_SELL_AT_FLOOR_BPS);
    assert!(core::mem::offset_of!(FloorPool, stair_steps) + 8 == FLOOR_POOL_ACCT_STAIR_STEPS);
    assert!(core::mem::offset_of!(FloorPool, stair_ready) + 8 == FLOOR_POOL_ACCT_STAIR_READY);
    assert!(core::mem::offset_of!(FloorPool, stair_growth_bps) + 8 == FLOOR_POOL_ACCT_STAIR_GROWTH_BPS);
    assert!(core::mem::offset_of!(FloorPool, stair_kind) + 8 == FLOOR_POOL_ACCT_STAIR_KIND);
};

/// Read the tally's LIFETIME counter (`holder_owed + holder_paid_eth`, saturating) straight off a
/// `FloorPool` account's raw data, discriminator included. `None` on a short buffer.
/// ⛔ This is the read `floor_holders::execute` makes on every hooked transfer; it is here so the
/// program, the quoter and the wasm cannot disagree about which sixteen bytes it is.
pub fn lifetime_from_floor_pool_account(data: &[u8]) -> Option<u64> {
    let owed = u64::from_le_bytes(data.get(FLOOR_POOL_ACCT_HOLDER_OWED..FLOOR_POOL_ACCT_HOLDER_OWED + 8)?.try_into().ok()?);
    let paid = u64::from_le_bytes(data.get(FLOOR_POOL_ACCT_HOLDER_PAID..FLOOR_POOL_ACCT_HOLDER_PAID + 8)?.try_into().ok()?);
    Some(owed.saturating_add(paid))
}

// =========================================================================================
// the shared claims lookup table
// =========================================================================================

/// An address lookup table holds at most 256 addresses.
pub const CLAIMS_TABLE_MAX: u16 = 256;
/// Addresses `init_launch` appends for a HOOKED launch: `[mint, holder_book, floor_pool, amm_pool, vault0]`.
pub const CLAIMS_ADDRS_HOOKED: u8 = 5;
/// Addresses `init_launch` appends for an UNHOOKED but REFERRED launch: `[floor_pool, amm_pool, vault0]`.
pub const CLAIMS_ADDRS_REFERRED: u8 = 3;
/// `rotate_claims_table` refuses `ClaimsTableNotFull` unless the current table is unset or has fewer
/// than this many free slots (`current_len > CLAIMS_TABLE_MAX − CLAIMS_ADDRS_HOOKED`): a table is
/// rotated only when the largest append would no longer fit.
pub const CLAIMS_TABLE_ROTATE_AT: u16 = CLAIMS_TABLE_MAX - CLAIMS_ADDRS_HOOKED as u16;

// =========================================================================================
// the extra-account-meta list and the token-account sizes a hooked mint implies
// =========================================================================================

/// Entries in a hooked mint's `ExtraAccountMetaList`: `[holder_book, floor_pool, sender record,
/// receiver record]`. The account list `execute` sees is therefore 5 + 4 = 9.
pub const EXTRA_ACCOUNT_METAS_ENTRIES: usize = 4;
/// The TLV account's length for those four entries (8 disc + 4 len + 4 count + 4 × 35).
pub const EXTRA_ACCOUNT_METAS_LEN: usize = 8 + 4 + 4 + 35 * EXTRA_ACCOUNT_METAS_ENTRIES;
/// Accounts `execute` receives: source, mint, destination, authority, the metas list, then the four.
pub const EXECUTE_ACCOUNTS: usize = 5 + EXTRA_ACCOUNT_METAS_ENTRIES;

/// A Token-2022 ASSOCIATED token account of an UNHOOKED FLOOR mint: 165 + 1 account type +
/// `ImmutableOwner` (4) + `PausableAccount` (4).
pub const TOKEN_ACCOUNT_LEN_UNHOOKED: usize = 174;
/// …of a HOOKED mint: + `TransferHookAccount` (4 + 1).
pub const TOKEN_ACCOUNT_LEN_HOOKED: usize = 179;
/// The AMM's base vault (not an ATA, so no `ImmutableOwner`) of an unhooked mint.
pub const AMM_BASE_VAULT_LEN_UNHOOKED: usize = 170;
/// …of a hooked mint.
pub const AMM_BASE_VAULT_LEN_HOOKED: usize = 175;

// =========================================================================================
// layouts
// =========================================================================================

/// `["holder_book", mint]` under `floor_holders`. Struct 200 B, account 208 B. One per HOOKED launch,
/// created by `init_launch` (the creator pays, at create).
///
/// ⛔ `acc` WRAPS (it is a Q64.64 reward-per-share accumulator read only as differences); every other
/// counter saturates. The arithmetic is `floor_math::holders`, and nowhere else.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct HolderBook {
    /// 1.
    pub version: u8,
    pub bump: u8,
    /// The `["extra-account-metas", mint]` PDA's bump.
    pub metas_bump: u8,
    pub pad0: [u8; 5],
    pub mint: [u8; 32],
    /// The launch's `FloorPool` — DERIVED by `init_launch` from the mint, never an argument.
    pub floor_pool: [u8; 32],
    pub quote_mint: [u8; 32],
    /// Wrapping Q64.64 reward-per-share.
    pub acc: U128,
    /// Σ `shares` of open records.
    pub eligible: u64,
    /// The lifetime (`holder_owed + holder_paid_eth`) already folded into `acc` / `unallocated`.
    pub indexed: u64,
    /// Quote base units not yet distributed: tax that arrived while `eligible == 0` (it PARKS), and
    /// the burn clip.
    pub unallocated: u64,
    /// Lifetime quote paid to holders through `claim`.
    pub paid: u64,
    /// Open record count.
    pub records: u64,
    pub created_slot: u64,
    /// Holder tax indexed in `last_slot` and not yet shared (each slot's tax is shared at the slot's end,
    /// `floor_math::holders::catch_up_at`).
    pub pending: u64,
    /// The slot of the book's last catch-up.
    pub last_slot: u64,
    pub reserved: [u8; 16],
}
layout!(HolderBook, name = "HolderBook", size = 200, disc = disc::HOLDER_BOOK, {
    version: u8 = 0, bump: u8 = 1, metas_bump: u8 = 2, pad0: [u8; 5] = 3, mint: [u8; 32] = 8,
    floor_pool: [u8; 32] = 40, quote_mint: [u8; 32] = 72, acc: U128 = 104, eligible: u64 = 120,
    indexed: u64 = 128, unallocated: u64 = 136, paid: u64 = 144, records: u64 = 152,
    created_slot: u64 = 160, pending: u64 = 168, last_slot: u64 = 176, reserved: [u8; 16] = 184,
});


/// `["holder", mint, token_account]` under `floor_holders`. Struct 160 B, account 168 B. The HOLDER
/// pays its deposit (≈ 0.0015 SOL) inside their own buy, or opens it once; the record
/// closes and REFUNDS that deposit to the holder once they have sold out and claimed.
///
/// ⛔ Exists ONLY for a Token-2022 ASSOCIATED token account of an ON-CURVE owner: a program-owned
/// (off-curve) account — a pool vault, an escrow, a vesting PDA — can never open one, so it earns
/// nothing and its share goes to recorded holders ("records-only" counting).
/// ⛔ Every instruction checks `mint` and `token_account` against the accounts it was handed.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct HolderRecord {
    /// 1.
    pub version: u8,
    pub bump: u8,
    pub pad0: [u8; 6],
    pub mint: [u8; 32],
    pub token_account: [u8; 32],
    /// The token account's owner at `open_record` (an ATA's owner never changes). ACCOUNT offset 80
    /// — the `getProgramAccounts` memcmp a wallet's records are listed by.
    pub owner: [u8; 32],
    pub shares: u64,
    /// `HolderBook.acc` at this record's last settle.
    pub snapshot: U128,
    pub owed: u64,
    pub paid: u64,
    pub opened_slot: u64,
    pub reserved: [u8; 8],
}
layout!(HolderRecord, name = "HolderRecord", size = 160, disc = disc::HOLDER_RECORD, {
    version: u8 = 0, bump: u8 = 1, pad0: [u8; 6] = 2, mint: [u8; 32] = 8, token_account: [u8; 32] = 40,
    owner: [u8; 32] = 72, shares: u64 = 104, snapshot: U128 = 112, owed: u64 = 128, paid: u64 = 136,
    opened_slot: u64 = 144, reserved: [u8; 8] = 152,
});

/// `HolderRecord.owner`'s ACCOUNT offset, for a `getProgramAccounts` memcmp filter.
pub const HOLDER_RECORD_ACCT_OWNER: usize = 80;
const _: () = assert!(core::mem::offset_of!(HolderRecord, owner) + 8 == HOLDER_RECORD_ACCT_OWNER);


/// `["claims_registry"]` under `floor_holders`. Struct 88 B, account 96 B. The singleton naming the
/// CURRENT shared claims lookup table (created permissionlessly once; rotated when full).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ClaimsRegistry {
    pub version: u8,
    pub bump: u8,
    /// The `["claims_alt_authority"]` PDA's bump.
    pub authority_bump: u8,
    pub pad0: [u8; 5],
    /// Zero until the first `rotate_claims_table`.
    pub current_table: [u8; 32],
    pub current_len: u16,
    pub pad1: [u8; 6],
    /// Tables ever created.
    pub tables: u64,
    pub reserved: [u8; 32],
}
layout!(ClaimsRegistry, name = "ClaimsRegistry", size = 88, disc = disc::CLAIMS_REGISTRY, {
    version: u8 = 0, bump: u8 = 1, authority_bump: u8 = 2, pad0: [u8; 5] = 3,
    current_table: [u8; 32] = 8, current_len: u16 = 40, pad1: [u8; 6] = 42, tables: u64 = 48,
    reserved: [u8; 32] = 56,
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;

    /// The three account lengths and their rent at a reference 5,080 lamports per byte-row.
    #[test]
    fn account_lengths_and_rent_match_the_spec() {
        assert_eq!(HolderBook::ACCOUNT_LEN, 208);
        assert_eq!(HolderRecord::ACCOUNT_LEN, 168);
        assert_eq!(ClaimsRegistry::ACCOUNT_LEN, 96);
        let rent = |len: usize| (128 + len as u64) * 5_080;
        assert_eq!(rent(HolderBook::ACCOUNT_LEN), 1_706_880);
        assert_eq!(rent(HolderRecord::ACCOUNT_LEN), 1_503_680);
        assert_eq!(EXTRA_ACCOUNT_METAS_LEN, 156);
        assert_eq!(rent(EXTRA_ACCOUNT_METAS_LEN), 1_442_720);
        assert_eq!(EXECUTE_ACCOUNTS, 9);
    }

    /// The id is the generated program key (never a placeholder `[24, 50, …, 41, 216]`), and it is distinct from
    /// every other FLOOR id.
    #[test]
    fn the_generated_id_is_not_the_placeholder_and_collides_with_nothing() {
        assert_eq!(FLOOR_HOLDERS_ID[0], 18);
        assert_eq!(FLOOR_HOLDERS_ID[31], 3);
        assert!(!(FLOOR_HOLDERS_ID[0] == 24 && FLOOR_HOLDERS_ID[31] == 216), "still a placeholder");
        for other in [crate::ids::FLOOR_HOOK_ID, crate::ids::FLOOR_LAUNCH_ID, crate::ids::FLOOR_VESTING_ID] {
            assert_ne!(FLOOR_HOLDERS_ID, other);
        }
        assert_eq!(crate::ids::FLOOR_HOLDERS_ID, FLOOR_HOLDERS_ID, "re-exported from ids");
    }

    /// The seeds are the expected strings (and the interface's hyphenated one is hyphenated).
    #[test]
    fn seeds_are_the_spec_strings() {
        assert_eq!(HOLDERS_AUTHORITY, b"holders_authority");
        assert_eq!(CLAIMS_ALT_AUTHORITY, b"claims_alt_authority");
        assert_eq!(CLAIMS_REGISTRY, b"claims_registry");
        assert_eq!(HOLDER_BOOK, b"holder_book");
        assert_eq!(EXTRA_ACCOUNT_METAS, b"extra-account-metas");
        assert_eq!(HOLDER_RECORD, b"holder");
    }

    /// The pool offsets the tally reads raw, by a WRITE through the struct and a READ
    /// through the constant — the mutation this kills is a carve that moves `holder_owed` /
    /// `holder_paid_eth` / `referrer` and leaves `execute` reading the neighbouring eight bytes.
    #[test]
    fn the_raw_pool_offsets_read_what_the_struct_wrote() {
        let mut p = FloorPool::zeroed();
        p.holder_owed = 0x1111;
        p.holder_paid_eth = 0x2222;
        p.referrer = [0x33; 32];
        p.holders_hooked = 1;
        let mut data = FloorPool::DISCRIMINATOR.to_vec();
        data.extend_from_slice(bytemuck::bytes_of(&p));
        assert_eq!(data.len(), FloorPool::ACCOUNT_LEN);
        let r = |o: usize| u64::from_le_bytes(data[o..o + 8].try_into().unwrap());
        assert_eq!(r(FLOOR_POOL_ACCT_HOLDER_OWED), 0x1111);
        assert_eq!(r(FLOOR_POOL_ACCT_HOLDER_PAID), 0x2222);
        assert_eq!(&data[FLOOR_POOL_ACCT_REFERRER..FLOOR_POOL_ACCT_REFERRER + 32], &[0x33; 32]);
        assert_eq!(data[FLOOR_POOL_ACCT_HOLDERS_HOOKED], 1);
        assert_eq!(lifetime_from_floor_pool_account(&data), Some(0x3333));
        // saturating, never wrapping
        let mut big = p;
        big.holder_owed = u64::MAX;
        let mut d2 = FloorPool::DISCRIMINATOR.to_vec();
        d2.extend_from_slice(bytemuck::bytes_of(&big));
        assert_eq!(lifetime_from_floor_pool_account(&d2), Some(u64::MAX));
        assert_eq!(lifetime_from_floor_pool_account(&data[..FLOOR_POOL_ACCT_HOLDER_PAID]), None);
    }

    /// The record's owner sits where a `getProgramAccounts` memcmp looks for it.
    #[test]
    fn the_record_owner_is_at_account_offset_80() {
        let mut r = HolderRecord::zeroed();
        r.owner = [0x44; 32];
        let mut data = HolderRecord::DISCRIMINATOR.to_vec();
        data.extend_from_slice(bytemuck::bytes_of(&r));
        assert_eq!(&data[HOLDER_RECORD_ACCT_OWNER..HOLDER_RECORD_ACCT_OWNER + 32], &[0x44; 32]);
    }


    /// The claims-table arithmetic the registry's rotation rule rests on.
    #[test]
    fn the_claims_table_constants_agree() {
        assert_eq!(CLAIMS_TABLE_MAX, 256);
        assert_eq!(CLAIMS_TABLE_ROTATE_AT, 251);
        assert!(CLAIMS_ADDRS_REFERRED < CLAIMS_ADDRS_HOOKED);
    }
}
