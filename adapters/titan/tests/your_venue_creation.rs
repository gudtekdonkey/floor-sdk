// SPDX-License-Identifier: Apache-2.0
use floor_amm_solana::creation::LaunchCreatedV2;
use titan_integration_template::your_venue::parse_pool_creations;
use titan_integration_template::your_venue::{PROTOCOL, program_id, to_core};
use solana_pubkey::Pubkey;
use titan_integration_template::trading_venue::venue_creation::{ParsedInstruction, PoolCreation};

const LAUNCH: Pubkey = Pubkey::from_str_const("7Np41oeYqPefeNQEHSv1UDhYrehxin3NStELsSKCT4K2");
const MINT: Pubkey = Pubkey::from_str_const("So11111111111111111111111111111111111111112");
const QUOTE: Pubkey = Pubkey::from_str_const("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

fn event() -> LaunchCreatedV2 {
    LaunchCreatedV2 {
        launch: to_core(&LAUNCH),
        mint: to_core(&MINT),
        quote_mint: to_core(&QUOTE),
        creator: to_core(&Pubkey::new_from_array([9; 32])),
        id: 42,
        preset_id: 1,
        preset_version: 1,
        preset_hash: [3; 32],
        p0_x64: 1 << 40,
        raise: 1_000_000,
        window_tokens: 5,
        creator_buy_tokens: 0,
        create_fee: 10_000_000,
        name: [b'a'; 32],
        symbol: [b'b'; 10],
        slot: 7,
        ts: 1_700_000_000,
        listed: true,
        quote_usd_cents_x64: 100 << 64,
        native_create_fee: 0,
        row_version: 1,
        sqrt_p0: 1 << 72,
    }
}

fn ix(program_id: Pubkey, data: Vec<u8>) -> ParsedInstruction {
    ParsedInstruction { program_id, accounts: vec![Pubkey::new_from_array([1; 32])], data }
}

#[test]
fn parses_the_launch_created_event() {
    let c = parse_pool_creations(&[ix(Pubkey::new_from_array([5; 32]), vec![1, 2, 3]), ix(program_id(), event().to_data())]);
    assert_eq!(c, vec![PoolCreation { protocol: PROTOCOL, pool: LAUNCH, mints: vec![MINT, QUOTE] }]);
}

#[test]
fn ignores_everything_else() {
    let d = event().to_data();
    assert!(parse_pool_creations(&[ix(program_id(), vec![])]).is_empty());
    assert!(parse_pool_creations(&[ix(program_id(), d[..d.len() - 1].to_vec())]).is_empty(), "a truncated event");
    assert!(parse_pool_creations(&[ix(Pubkey::new_from_array([5; 32]), d)]).is_empty(), "another program's instruction");
}
