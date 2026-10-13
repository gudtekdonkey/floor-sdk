//! ⛔ Trader referrals: the 193-byte link message, pinned for the off-chain signers. The TypeScript SDK's
//! `traderLinkMessage` (`packages/sdk-solana/test/trader-ref.test.ts`) holds the SAME hex: floor_launch id `[3; 32]`, cluster 2, trader `[4; 32]`,
//! tiers `[5; 32]`, `[6; 32]`, absent (zeros), deadline 1,900,000,000.

const VECTOR: &str = "464c4f4f525f5452414445525f5245465f4c494e4b5f5631\
0303030303030303030303030303030303030303030303030303030303030303\
02\
0404040404040404040404040404040404040404040404040404040404040404\
0505050505050505050505050505050505050505050505050505050505050505\
0606060606060606060606060606060606060606060606060606060606060606\
0000000000000000000000000000000000000000000000000000000000000000\
00b33f7100000000";

#[test]
fn the_trader_link_message_vector_is_pinned_for_the_signers() {
    let m = floor_types::constants::trader_link_message(&[3; 32], 2, &[4; 32], [&[5; 32], &[6; 32], &[0; 32]], 1_900_000_000);
    let hex: String = m.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(m.len(), floor_types::constants::TRADER_LINK_MSG_LEN);
    assert_eq!(hex, VECTOR);
}
