//! Independent byte checks of the multi-recipient layout against its written description, asserted on raw bytes
//! written through the public types (never `offset_of!` alone, which is what the crate's own const asserts use):
//!
//! > `FloorPool` gains a `PayeeSlots` tail at account offset **1,008** (after the TWAP);
//! > `FLOOR_POOL_ACCOUNT_LEN_WITH_SLOTS = 1,008 + 568 = 1,576`.
//! > `PayeeSlots (568 B, repr(C), Pod): version u8 · creator_count u8 · team_count u8 · creator_pending_mask u8 ·
//! > team_pending_mask u8 · pad [u8;3] · creator [PayeeSlot;5] · team [PayeeSlot;5]`
//! > `PayeeSlot (56 B): key [u8;32] · bps u16 · platform u8 · state u8 (0 wallet · 1 pending · 2 resolved) · pad [u8;4] ·
//! > paid u64 · tokens_paid u64`
//! > `FloorPool.creator @144 / pending_recipient_id @176 become padding (zero)`; `StairOwners.payee / payee_pending_id`
//! > become padding.
//!
//! Plus the V2 claim message (prefix 24 ‖ program 32 ‖ cluster 1 ‖ launch 32 ‖ slot 1 ‖ pending 32 ‖ recipient 32 ‖
//! deadline i64 LE 8 = 162) and the `SlotArg` wire (35 B: `bps u16 · platform u8 · key [u8;32]`).

use floor_types::layout::FloorPool;
use floor_types::owners::StairOwners;
use floor_types::slots::*;
use floor_types::layout::Layout;

fn field(name: &str) -> (usize, usize) {
    let f = FloorPool::FIELDS.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("FloorPool has no field {name}"));
    (f.offset, f.size)
}

#[test]
fn mr_review_the_tail_is_at_1008_and_the_account_is_1576() {
    assert_eq!(PAYEE_SLOTS_OFFSET, 1_008);
    assert_eq!(PAYEE_SLOTS_SIZE, 568);
    assert_eq!(PAYEE_SLOT_SIZE, 56);
    assert_eq!(FLOOR_POOL_ACCOUNT_LEN_WITH_SLOTS, 1_576);
    assert_eq!(core::mem::size_of::<PayeeSlots>(), 568);
    assert_eq!(core::mem::size_of::<PayeeSlot>(), 56);
    // the tail follows the struct (784) + discriminator (8) + TWAP (216) exactly
    assert_eq!(FloorPool::ACCOUNT_LEN + 216, PAYEE_SLOTS_OFFSET);
    assert_eq!(8 + 5 * 56 + 5 * 56, 568);
}

/// Every field of the tail written to a distinct value through the public type, then read back as RAW account bytes at the
/// documented offsets.
#[test]
fn mr_review_the_tail_bytes_are_the_specs_order() {
    let slot = |b: u8, bps: u16, platform: u8, state: u8, paid: u64, tp: u64| PayeeSlot {
        key: [b; 32],
        bps,
        platform,
        state,
        pad: [0; 4],
        paid,
        tokens_paid: tp,
    };
    let mut t = PayeeSlots::default();
    t.version = 0xA1;
    t.creator_count = 0xA2;
    t.team_count = 0xA3;
    t.creator_pending_mask = 0xA4;
    t.team_pending_mask = 0xA5;
    for i in 0..5u8 {
        t.creator[i as usize] =
            slot(0x10 + i, 0x0100 + i as u16, i, 2, 0x1111_0000_0000_0000 + i as u64, 0x2222_0000_0000_0000 + i as u64);
        t.team[i as usize] =
            slot(0x20 + i, 0x0200 + i as u16, 4 - i, 1, 0x3333_0000_0000_0000 + i as u64, 0x4444_0000_0000_0000 + i as u64);
    }
    let mut acct = vec![0xEEu8; FLOOR_POOL_ACCOUNT_LEN_WITH_SLOTS];
    assert!(t.store(&mut acct));
    // the 1,008 bytes before the tail are untouched
    assert!(acct[..1_008].iter().all(|&b| b == 0xEE), "store wrote before the tail");
    let a = &acct[1_008..];
    assert_eq!(&a[0..5], &[0xA1, 0xA2, 0xA3, 0xA4, 0xA5], "version · counts · masks");
    assert_eq!(&a[5..8], &[0, 0, 0], "pad [u8;3]");
    for (list, base) in [(0usize, 8usize), (1, 8 + 5 * 56)] {
        for i in 0..5usize {
            let s = &a[base + i * 56..base + (i + 1) * 56];
            let (kb, bps, plat, state, paid, tp) = if list == 0 {
                (0x10 + i as u8, 0x0100 + i as u16, i as u8, 2u8, 0x1111_0000_0000_0000 + i as u64, 0x2222_0000_0000_0000 + i as u64)
            } else {
                (0x20 + i as u8, 0x0200 + i as u16, 4 - i as u8, 1u8, 0x3333_0000_0000_0000 + i as u64, 0x4444_0000_0000_0000 + i as u64)
            };
            assert_eq!(&s[0..32], &[kb; 32], "key @0");
            assert_eq!(u16::from_le_bytes([s[32], s[33]]), bps, "bps @32 LE");
            assert_eq!(s[34], plat, "platform @34");
            assert_eq!(s[35], state, "state @35");
            assert_eq!(&s[36..40], &[0; 4], "pad @36");
            assert_eq!(u64::from_le_bytes(s[40..48].try_into().unwrap()), paid, "paid @40 LE");
            assert_eq!(u64::from_le_bytes(s[48..56].try_into().unwrap()), tp, "tokens_paid @48 LE");
        }
    }
    assert_eq!(PayeeSlots::from_account(&acct), Some(t));
}

/// A reader that pinned 1,008 B must accept ≥ (the TWAP rule, one tail later): a longer account reads the same tail; a
/// shorter one has none, and `store` refuses rather than writing a partial tail.
#[test]
fn mr_review_from_account_and_store_lengths() {
    let mut t = PayeeSlots::default();
    t.fill(SLOT_KIND_CREATOR, &[SlotArg { bps: 10_000, platform: 0, key: [7; 32] }]);
    let mut long = vec![0u8; FLOOR_POOL_ACCOUNT_LEN_WITH_SLOTS + 333];
    assert!(t.store(&mut long));
    assert!(long[FLOOR_POOL_ACCOUNT_LEN_WITH_SLOTS..].iter().all(|&b| b == 0), "nothing past the tail");
    assert_eq!(PayeeSlots::from_account(&long), Some(t));
    for len in [0usize, 792, 1_008, 1_575] {
        let mut short = vec![0u8; len];
        assert!(PayeeSlots::from_account(&short).is_none(), "len {len} carries no tail");
        assert!(!t.store(&mut short), "len {len}: store refuses");
        assert!(short.iter().all(|&b| b == 0), "len {len}: a refused store writes nothing");
    }
}

/// `FloorPool.creator @144 / pending_recipient_id @176 become padding` — the account still has 32-byte slots there
/// and NO field of those names anywhere in the layout.
#[test]
fn mr_review_the_dead_single_recipient_fields_are_padding() {
    assert!(FloorPool::FIELDS.iter().all(|f| f.name != "creator" && f.name != "pending_recipient_id"));
    assert_eq!(field("pad9"), (144, 32));
    assert_eq!(field("pad10"), (176, 32));
    // StairOwners: `payee` / `payee_pending_id` are pads at the same offsets the single payee held
    assert_eq!(core::mem::offset_of!(StairOwners, pad1), 24);
    assert_eq!(core::mem::offset_of!(StairOwners, pad2), 56);
    assert_eq!(core::mem::size_of::<StairOwners>(), 440, "the owner tail did not grow");
}

#[test]
fn mr_review_slot_states_kinds_and_wire() {
    assert_eq!((SLOT_STATE_WALLET, SLOT_STATE_PENDING, SLOT_STATE_RESOLVED), (0, 1, 2));
    assert_eq!((SLOT_KIND_CREATOR, SLOT_KIND_TEAM), (0, 1));
    assert_eq!((MAX_SLOTS, SLOTS_TOTAL_BPS, MAX_SLOT_PLATFORM), (5, 10_000, 4));
    // SlotArg: bps u16 LE · platform · key
    let a = SlotArg { bps: 0xBEEF, platform: 2, key: core::array::from_fn(|i| i as u8) };
    let b = borsh::to_vec(&a).unwrap();
    assert_eq!(b.len(), 35);
    assert_eq!(&b[..3], &[0xEF, 0xBE, 2]);
    assert_eq!(&b[3..], &a.key);
    // SlotsArg: Vec ‖ Vec (u32 LE length prefixes); the empty team list is 4 zero bytes
    let s = SlotsArg { creator: vec![a], team: vec![] };
    let sb = borsh::to_vec(&s).unwrap();
    assert_eq!(sb.len(), 4 + 35 + 4);
    assert_eq!(&sb[..4], &1u32.to_le_bytes());
    assert_eq!(&sb[39..], &0u32.to_le_bytes());
    assert_eq!(borsh::from_slice::<SlotsArg>(&sb).unwrap(), s);
    // the wire itself does not cap the list at 5 (the check does): six slots round-trip
    let six = SlotsArg { creator: vec![a; 6], team: vec![a; 6] };
    assert_eq!(borsh::from_slice::<SlotsArg>(&borsh::to_vec(&six).unwrap()).unwrap(), six);
}

/// `fill` past five takes the first five (the caller has already refused a sixth by `check_slots`); every state, the
/// masks, the version; `resolve` keeps the platform; an unknown kind byte resolves nothing.
#[test]
fn mr_review_fill_and_resolve_edges() {
    let args: Vec<SlotArg> = (0..5u8)
        .map(|i| SlotArg { bps: 2_000, platform: if i % 2 == 1 { 1 + i % 4 } else { 0 }, key: [i + 1; 32] })
        .collect();
    let mut t = PayeeSlots::default();
    t.fill(SLOT_KIND_TEAM, &args);
    assert_eq!((t.version, t.team_count, t.team_pending_mask, t.creator_count), (1, 5, 0b01010, 0));
    for i in 0..5 {
        let s = t.team[i];
        assert_eq!(s.state, if i % 2 == 1 { SLOT_STATE_PENDING } else { SLOT_STATE_WALLET });
        assert_eq!(s.wallet().is_some(), i % 2 == 0, "a pending slot has no wallet");
        assert_eq!((s.paid, s.tokens_paid), (0, 0));
    }
    assert!(t.slot(SLOT_KIND_TEAM, 5).is_none());
    assert!(t.slot(SLOT_KIND_CREATOR, 0).is_none(), "the other list is empty");
    assert!(t.slot(7, 0).is_none(), "an unknown kind byte names no list");
    assert!(!t.resolve(7, 1, [9; 32]), "an unknown kind resolves nothing");
    assert!(t.resolve(SLOT_KIND_TEAM, 3, [9; 32]));
    assert_eq!(t.team_pending_mask, 0b00010);
    assert_eq!((t.team[3].state, t.team[3].platform, t.team[3].key), (SLOT_STATE_RESOLVED, 4, [9; 32]));
    assert!(t.holds_wallet(SLOT_KIND_TEAM, &[9; 32]));
    assert!(!t.holds_wallet(SLOT_KIND_CREATOR, &[9; 32]), "the creator list never holds a team wallet");
    // a sixth arg is ignored by fill (the check refuses it before fill is ever reached)
    let mut seven = args.clone();
    seven.push(SlotArg { bps: 1, platform: 0, key: [0xFF; 32] });
    let mut u = PayeeSlots::default();
    u.fill(SLOT_KIND_CREATOR, &seven);
    assert_eq!(u.creator_count, 5);
    assert!(!u.holds_wallet(SLOT_KIND_CREATOR, &[0xFF; 32]));
}

#[test]
fn mr_review_the_v2_claim_message_is_162_bytes_at_the_specs_offsets() {
    assert_eq!(SLOT_CLAIM_MSG_LEN, 162);
    assert_eq!(RECIPIENT_CLAIM_PREFIX_V2, b"FLOOR_RECIPIENT_CLAIM_V2");
    assert_eq!(STAIR_PAYEE_CLAIM_PREFIX_V2, b"FLOOR_STAIR_PAYEE_CLM_V2");
    let prog = [0x01u8; 32];
    let launch = [0x03u8; 32];
    let pend = [0x05u8; 32];
    let rec = [0x06u8; 32];
    let deadline: i64 = -0x0102_0304_0506_0708;
    let m = slot_claim_message(STAIR_PAYEE_CLAIM_PREFIX_V2, &prog, 0x02, &launch, 0x04, &pend, &rec, deadline);
    let mut want = Vec::new();
    want.extend_from_slice(b"FLOOR_STAIR_PAYEE_CLM_V2");
    want.extend_from_slice(&prog);
    want.push(0x02);
    want.extend_from_slice(&launch);
    want.push(0x04);
    want.extend_from_slice(&pend);
    want.extend_from_slice(&rec);
    want.extend_from_slice(&deadline.to_le_bytes());
    assert_eq!(want.len(), 162);
    assert_eq!(m.to_vec(), want);
    // the slot byte distinguishes two otherwise identical messages (one slot's attestation never verifies for another)
    let m0 = slot_claim_message(RECIPIENT_CLAIM_PREFIX_V2, &prog, 2, &launch, 0, &pend, &rec, 1);
    let m1 = slot_claim_message(RECIPIENT_CLAIM_PREFIX_V2, &prog, 2, &launch, 1, &pend, &rec, 1);
    assert_ne!(m0, m1);
    assert_eq!(m0.iter().zip(m1.iter()).filter(|(a, b)| a != b).count(), 1);
}
