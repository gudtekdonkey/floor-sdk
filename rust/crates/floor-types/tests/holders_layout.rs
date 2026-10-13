//! The `floor_holders` accounts, the `FloorPool` tail carve and the interface constants, field by
//! field, from OUTSIDE the crate — so the published surface is pinned by written numbers, not by the
//! struct that happens to compile.
//!
//! ⛔ Every row is `(name, offset, size)` as written out here. A field moved, resized,
//! renamed or reordered fails here by name; `layout!`'s own contiguity check then says nothing was
//! lost in between.

use floor_types::holders::*;
use floor_types::layout::{FloorPool, Layout};
use floor_types::register::{RegisterPoolArgs, REFERRAL_TIERS_ARG_LEN, REGISTER_POOL_ARGS_LEN, TEAM_FEE_ARG_LEN};

fn fields<L: Layout>() -> Vec<(&'static str, usize, usize)> {
    L::FIELDS.iter().map(|f| (f.name, f.offset, f.size)).collect()
}

#[test]
fn holder_book_is_the_spec_table() {
    assert_eq!(
        fields::<HolderBook>(),
        vec![
            ("version", 0, 1),
            ("bump", 1, 1),
            ("metas_bump", 2, 1),
            ("pad0", 3, 5),
            ("mint", 8, 32),
            ("floor_pool", 40, 32),
            ("quote_mint", 72, 32),
            ("acc", 104, 16),
            ("eligible", 120, 8),
            ("indexed", 128, 8),
            ("unallocated", 136, 8),
            ("paid", 144, 8),
            ("records", 152, 8),
            ("created_slot", 160, 8),
            ("pending", 168, 8),
            ("last_slot", 176, 8),
            ("reserved", 184, 16),
        ]
    );
    assert_eq!((HolderBook::SIZE, HolderBook::ACCOUNT_LEN), (200, 208));
    assert_eq!(HolderBook::NAME, "HolderBook");
}

#[test]
fn holder_record_is_the_spec_table() {
    assert_eq!(
        fields::<HolderRecord>(),
        vec![
            ("version", 0, 1),
            ("bump", 1, 1),
            ("pad0", 2, 6),
            ("mint", 8, 32),
            ("token_account", 40, 32),
            ("owner", 72, 32),
            ("shares", 104, 8),
            ("snapshot", 112, 16),
            ("owed", 128, 8),
            ("paid", 136, 8),
            ("opened_slot", 144, 8),
            ("reserved", 152, 8),
        ]
    );
    assert_eq!((HolderRecord::SIZE, HolderRecord::ACCOUNT_LEN), (160, 168));
    assert_eq!(HOLDER_RECORD_ACCT_OWNER, 80, "owner's account offset, the getProgramAccounts memcmp");
}

#[test]
fn claims_registry_is_the_spec_table() {
    assert_eq!(
        fields::<ClaimsRegistry>(),
        vec![
            ("version", 0, 1),
            ("bump", 1, 1),
            ("authority_bump", 2, 1),
            ("pad0", 3, 5),
            ("current_table", 8, 32),
            ("current_len", 40, 2),
            ("pad1", 42, 6),
            ("tables", 48, 8),
            ("reserved", 56, 32),
        ]
    );
    assert_eq!((ClaimsRegistry::SIZE, ClaimsRegistry::ACCOUNT_LEN), (88, 96));
}

/// The tail of `FloorPool` from the referral carve on, and the account offsets the crate exports.
#[test]
fn the_floor_pool_tail_is_the_spec_carve() {
    let all = fields::<FloorPool>();
    let from_584: Vec<_> = all.iter().copied().filter(|f| f.1 >= 584).collect();
    assert_eq!(
        from_584,
        vec![
            ("referral_owed", 584, 8),
            ("referral_fees_paid", 592, 8),
            ("trader_ref_owed", 600, 8), // trader referrals, in the retired `referral_distributor`'s bytes
            ("trader_ref_booked", 608, 8),
            ("pad7", 616, 16), // reserved for a per-pool rate copy
            ("referral_share_bps", 632, 2),
            // the staircase: carved from `pad3` [u8; 6], nothing else moved.
            ("stair_steps", 634, 1),
            ("stair_ready", 635, 1),
            ("stair_growth_bps", 636, 2),
            // the staircase parameter set's kind, from `pad3` [u8; 2].
            ("stair_kind", 638, 1),
            ("stair_owner", 639, 1),
            ("referrer", 640, 32),
            ("pad8", 672, 1), // the retired version byte, padding
            ("holders_hooked", 673, 1),
            ("pad4", 674, 6),
            // three-tier referrals, carved from the front of `reserved`.
            ("referrer_t2", 680, 32),
            ("referrer_t3", 712, 32),
            ("referral_paid_t2", 744, 8),
            ("referral_paid_t3", 752, 8),
            // the team fee, on the tiers' last byte: the rest of `reserved` (24 -> 0).
            // `layout.rs::floor_pool_team_carve_out_at_760` is the carve's own guard.
            ("team_owed", 760, 8),
            ("team_fees_paid", 768, 8),
            ("team_fee_bps", 776, 2),
            ("pad5", 778, 6),
        ]
    );
    assert_eq!((FloorPool::SIZE, FloorPool::ACCOUNT_LEN), (784, 792));
    let at = |n: &str| all.iter().find(|f| f.0 == n).unwrap().1 + 8;
    assert_eq!(at("holder_owed"), FLOOR_POOL_ACCT_HOLDER_OWED);
    assert_eq!(at("holder_paid_eth"), FLOOR_POOL_ACCT_HOLDER_PAID);
    assert_eq!(at("referrer"), FLOOR_POOL_ACCT_REFERRER);
    assert_eq!(
        (FLOOR_POOL_ACCT_HOLDER_OWED, FLOOR_POOL_ACCT_HOLDER_PAID, FLOOR_POOL_ACCT_REFERRER),
        (440, 520, 648),
        "the published numbers"
    );
    // The tiers and the team fee's pool rate, each at its account offset.
    assert_eq!(at("referrer_t2"), FLOOR_POOL_ACCT_REFERRER_T2);
    assert_eq!(at("team_fee_bps"), FLOOR_POOL_ACCT_TEAM_FEE_BPS);
    assert_eq!((FLOOR_POOL_ACCT_REFERRER_T2, FLOOR_POOL_ACCT_TEAM_FEE_BPS), (688, 784), "struct 680 / 776, + 8");
}

/// The interface constants and the `RegisterPoolArgs` wire.
#[test]
fn the_interface_constants_are_the_specs() {
    assert_eq!((CLAIMS_TABLE_MAX, CLAIMS_ADDRS_HOOKED, CLAIMS_ADDRS_REFERRED), (256, 5, 3));
    // the pool's platform-fee rate and the tiers ride trailing arguments, outside these 265 bytes.
    assert_eq!(REGISTER_POOL_ARGS_LEN, 265);
    let a = RegisterPoolArgs { referrer: [9; 32], ..Default::default() };
    let wire = borsh::to_vec(&a).unwrap();
    assert_eq!(wire.len(), 265);
    assert_eq!(&wire[233..265], &[9u8; 32], "the referrer is the LAST 32 bytes, in fund_referrals' place");
    assert_eq!((REFERRAL_TIERS_ARG_LEN, TEAM_FEE_ARG_LEN), (64, 2), "then the tiers, then the pool rate");
    // the discriminators are the Anchor `account:<Name>` hashes (recomputed in `tests/tests/layouts.rs`
    // against the built IDL; here, that they are three distinct values)
    use floor_types::disc::account as d;
    assert_ne!(d::HOLDER_BOOK, d::HOLDER_RECORD);
    assert_ne!(d::HOLDER_RECORD, d::CLAIMS_REGISTRY);
    assert_eq!(HolderBook::DISCRIMINATOR, d::HOLDER_BOOK);
}
