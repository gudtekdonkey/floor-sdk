//! Each `Amm` method on synthetic `Launch` accounts: the answers against `floor_quote::sale_buy` and
//! `floor_quote::sale_sell_back_stateless` called directly, every refusal by name, `update`'s all-or-nothing, the encoder, and the crate's
//! dependency and source rules.

mod common;

use floor_types::ids::DEVNET;
use common::*;
use floor_sale_amm::accounts::{
    buyer_position_pda, floor_holders_program, holder_book_pda, holder_record_pda, launch_pda, launch_program, sale_vault_pda, spl_token,
    token_2022, token_2022_ata, wsol_ata,
};
use floor_sale_amm::amm::refusal;
use floor_sale_amm::encode::{check_buy, check_sell, side_of, swap_variant, SELL_BACK_WSOL_DISC};
use floor_sale_amm::{buy_instruction_data, instruction_data, sell_instruction_data, FloorSaleAmm, BUY_METAS, LABEL, OP_BUY, OP_SELL, SELL_METAS, SELL_METAS_HOOKED};
use sha2::{Digest, Sha256};
use floor_types::constants::{LAUNCH_STATE_GRADUATED, SALE_TOKENS};
use floor_types::keys::TOKEN_2022_PROGRAM_ID;
use jupiter_amm_interface::{AccountProvider, Amm, AmmContext, AmmError, KeyedAccount, QuoteParams, Swap, SwapMode, SwapParams};
use solana_account::Account;
use solana_pubkey::Pubkey;
use std::collections::HashMap;

fn q(amm: &FloorSaleAmm, input: Pubkey, output: Pubkey, amount: u64, mode: SwapMode) -> Result<jupiter_amm_interface::Quote, AmmError> {
    amm.quote(&QuoteParams { amount, input_mint: input, output_mint: output, swap_mode: mode, fee_mode: Default::default() })
}

fn buy(amm: &FloorSaleAmm, amount: u64) -> Result<jupiter_amm_interface::Quote, AmmError> {
    q(amm, wsol(), amm.mint(), amount, SwapMode::ExactIn)
}

fn sell(amm: &FloorSaleAmm, tokens: u64) -> Result<jupiter_amm_interface::Quote, AmmError> {
    q(amm, amm.mint(), wsol(), tokens, SwapMode::ExactIn)
}

fn refused<T>(r: Result<T, AmmError>, name: &str) {
    let Err(e) = r else { panic!("expected {name}, got Ok") };
    let e = e.to_string();
    assert!(e.starts_with(name), "expected {name}, got {e}");
}

fn swap_params<'a>(mint: Pubkey, user: Pubkey, dest: Pubkey, authority: Pubkey, mode: SwapMode) -> SwapParams<'a, 'a> {
    SwapParams {
        swap_mode: mode,
        in_amount: 1_000_000,
        out_amount: 0,
        source_mint: wsol(),
        destination_mint: mint,
        source_token_account: Pubkey::new_unique(),
        destination_token_account: dest,
        token_transfer_authority: authority,
        user,
        payer: user,
        quote_mint_to_referrer: None,
        jupiter_program_id: &JUP,
        missing_dynamic_accounts_as_default: false,
    }
}

#[test]
fn identity_and_static_answers() {
    let (mint, l) = default_sale(PRICE_ONE, 0, 100);
    let amm = amm_of(&l);
    assert_eq!(amm.label(), LABEL);
    assert_eq!(LABEL, "FLOOR");
    assert_eq!(amm.program_id(), launch_program(&DEVNET));
    assert_eq!(amm.key(), launch_pda(&DEVNET, &mint));
    assert_eq!(amm.get_reserve_mints(), vec![wsol(), mint]);
    assert_eq!(amm.get_accounts_to_update(), vec![launch_pda(&DEVNET, &mint)]);
    assert!(!amm.unidirectional(), "both directions: buys and sell-backs");
    assert!(!amm.supports_exact_out());
    assert!(!amm.has_dynamic_accounts());
    assert!(!amm.requires_update_for_reserve_mints());
    assert_eq!(amm.get_accounts_len(), BUY_METAS.max(SELL_METAS));
    let mut hooked = l;
    hooked.holder_sell = 150;
    assert_eq!(amm_of(&hooked).get_accounts_len(), SELL_METAS_HOOKED);
    assert_eq!(amm.program_dependencies(), vec![(Pubkey::new_from_array(TOKEN_2022_PROGRAM_ID), "spl_token_2022".to_string())]);
    assert!(amm.underlying_liquidities().is_none());
    assert!(amm.is_active());
}

#[test]
fn from_keyed_account_refuses_what_is_not_a_launch() {
    let (_, l) = default_sale(PRICE_ONE, 0, 100);
    let mut k = keyed(&l);
    k.account.owner = Pubkey::new_unique();
    assert!(FloorSaleAmm::from_keyed_account(&k, &AmmContext::default()).is_err(), "wrong owner");
    let mut k = keyed(&l);
    k.key = Pubkey::new_unique();
    assert!(FloorSaleAmm::from_keyed_account(&k, &AmmContext::default()).is_err(), "not the launch PDA of its mint");
    let mut k = keyed(&l);
    k.account.data[0] ^= 1;
    assert!(FloorSaleAmm::from_keyed_account(&k, &AmmContext::default()).is_err(), "discriminator");
    let mut k = keyed(&l);
    k.account.data.truncate(100);
    assert!(FloorSaleAmm::from_keyed_account(&k, &AmmContext::default()).is_err(), "short");
}

#[test]
fn quotes_are_floor_quotes_sale_buy() {
    for (price, sold, bps) in [(PRICE_ONE, 0u64, 0u16), (PRICE_ONE, 0, 100), (92_233_720_368_547, 59_405_940_400_000, 100), (7_682_302_212_939_176, 1, 40), (3 * PRICE_ONE + 7, SALE_TOKENS / 2, 1)] {
        let (mint, l) = default_sale(price, sold, bps);
        let amm = amm_of(&l);
        for amount in [1u64, 1_000, 999_983, 1_000_000_000, 31_415_926_535] {
            match floor_quote::sale_buy(&l, amount) {
                Ok(s) if !s.graduates => {
                    let got = buy(&amm, amount).unwrap();
                    assert_eq!((got.in_amount, got.out_amount, got.fee_amount, got.fee_mint), (s.spent, s.tokens, s.team_fee, wsol()));
                    assert!(got.in_amount <= amount);
                    let pct = rust_decimal::Decimal::from(s.team_fee) / rust_decimal::Decimal::from(s.spent);
                    assert_eq!(got.fee_pct, pct);
                }
                Ok(_) => refused(buy(&amm, amount), refusal::SALE_WOULD_GRADUATE),
                Err(_) => refused(buy(&amm, amount), refusal::NOTHING),
            }
        }
        let _ = mint;
    }
}

#[test]
fn quote_refusals_by_name() {
    let (mint, l) = default_sale(PRICE_ONE, 0, 100);
    let amm = amm_of(&l);
    // nothing sold yet: no position can cover a sell-back
    refused(sell(&amm, 1_000), refusal::EXCEEDS_SOLD);
    refused(q(&amm, wsol(), mint, 1_000, SwapMode::ExactOut), refusal::EXACT_OUT_NOT_SUPPORTED);
    refused(q(&amm, mint, wsol(), 1_000, SwapMode::ExactOut), refusal::EXACT_OUT_NOT_SUPPORTED);
    assert!(q(&amm, Pubkey::new_unique(), mint, 1_000, SwapMode::ExactIn).is_err(), "a foreign mint");
    refused(buy(&amm, 0), refusal::NOTHING);
    refused(buy(&amm, u64::MAX), refusal::SALE_WOULD_GRADUATE);

    let mut g = l;
    g.state = LAUNCH_STATE_GRADUATED;
    let amm_g = amm_of(&g);
    assert!(!amm_g.is_active());
    refused(buy(&amm_g, 1_000), refusal::SALE_OVER);
    refused(sell(&amm_g, 0), refusal::SALE_OVER);

    let mut t = l;
    t.quote_mint = Pubkey::new_unique().to_bytes();
    let amm_t = amm_of(&t);
    assert!(!amm_t.is_active());
    refused(q(&amm_t, amm_t.quote_mint(), mint, 1_000, SwapMode::ExactIn), refusal::TOKEN_QUOTE_NOT_ROUTED);
    refused(q(&amm_t, mint, amm_t.quote_mint(), 1_000, SwapMode::ExactIn), refusal::TOKEN_QUOTE_NOT_ROUTED);

    // One unit left: nothing can be bought without selling the sale out.
    let (_, last) = default_sale(PRICE_ONE, SALE_TOKENS - 1, 100);
    let amm_last = amm_of(&last);
    assert!(!amm_last.is_active());
    assert_eq!(amm_last.max_buy_in(), None);
    refused(buy(&amm_last, 1_000), refusal::SALE_WOULD_GRADUATE);
}

#[test]
fn max_buy_in_is_the_last_amount_that_leaves_a_token_unsold() {
    for (price, sold, bps) in [(PRICE_ONE, 0u64, 100u16), (PRICE_ONE, SALE_TOKENS - 2, 0), (PRICE_ONE, SALE_TOKENS - 3, 100), (92_233_720_368_547, 59_405_940_400_000, 100), (7_682_302_212_939_176, 0, 100), (3 * PRICE_ONE + 7, SALE_TOKENS - 10, 40)] {
        let (_, l) = default_sale(price, sold, bps);
        let amm = amm_of(&l);
        let max = amm.max_buy_in().expect("routable");
        let at = buy(&amm, max).unwrap();
        assert!(l.sold + at.out_amount < l.sale_tokens);
        refused(buy(&amm, max + 1), refusal::SALE_WOULD_GRADUATE);
    }
}

struct Map(HashMap<Pubkey, Account>);
impl AccountProvider for Map {
    fn get(&self, k: &Pubkey) -> Option<impl solana_account::ReadableAccount + use<'_>> {
        self.0.get(k).cloned()
    }
}

#[test]
fn update_reads_the_launch_and_fails_closed() {
    let (mint, l) = default_sale(PRICE_ONE, 0, 100);
    let mut amm = amm_of(&l);
    let key = amm.key();
    let before = buy(&amm, 1_000_000).unwrap();

    let mut moved = l;
    moved.sold = SALE_TOKENS - 10;
    amm.update(Map(HashMap::from([(key, launch_account(&moved))]))).unwrap();
    assert_eq!(amm.remaining(), 10);
    assert_ne!(buy(&amm, 1).ok().map(|q| q.out_amount), Some(before.out_amount));

    // Missing account, wrong owner, another mint: each leaves the market inactive.
    assert!(amm.update(Map(HashMap::new())).is_err());
    let mut wrong_owner = launch_account(&l);
    wrong_owner.owner = token_2022();
    assert!(amm.update(Map(HashMap::from([(key, wrong_owner)]))).is_err());
    assert!(!amm.is_active());
    assert!(buy(&amm, 1_000).is_err());
    let mut other = l;
    other.mint = Pubkey::new_unique().to_bytes();
    assert!(amm.update(Map(HashMap::from([(key, launch_account(&other))]))).is_err());
    assert!(!amm.is_active());
    amm.update(Map(HashMap::from([(key, launch_account(&l))]))).unwrap();
    assert!(amm.is_active());
    assert_eq!(buy(&amm, 1_000_000).unwrap().out_amount, before.out_amount);
    let _ = mint;
}

#[test]
fn metas_name_the_users_accounts_and_refuse_anything_else() {
    let (mint, l) = default_sale(PRICE_ONE, 0, 100);
    let amm = amm_of(&l);
    let user = Pubkey::new_unique();
    let ata = token_2022_ata(&user, &mint);
    let sam = amm.get_swap_and_account_metas(&swap_params(mint, user, ata, user, SwapMode::ExactIn)).unwrap();
    assert_eq!(sam.swap, swap_variant(true));
    let m = &sam.account_metas;
    assert_eq!(m.len(), BUY_METAS);
    assert!(m[0].is_signer && m[0].is_writable && m[0].pubkey == user);
    assert_eq!(m.iter().filter(|a| a.is_signer).count(), 1, "the user is the only signer");
    assert_eq!(m[1].pubkey, amm.key());
    assert_eq!(m[2].pubkey, sale_vault_pda(&DEVNET, &amm.key()));
    assert_eq!(m[4].pubkey, buyer_position_pda(&DEVNET, &amm.key(), &user));
    assert_eq!(m[5].pubkey, ata);
    assert_eq!(m[6].pubkey, mint);

    refused(amm.get_swap_and_account_metas(&swap_params(mint, user, Pubkey::new_unique(), user, SwapMode::ExactIn)), refusal::DESTINATION_NOT_USER_ATA);
    let program_authority = Pubkey::new_unique();
    refused(amm.get_swap_and_account_metas(&swap_params(mint, user, ata, program_authority, SwapMode::ExactIn)), refusal::AUTHORITY_NOT_USER);
    refused(amm.get_swap_and_account_metas(&swap_params(mint, user, ata, user, SwapMode::ExactOut)), refusal::EXACT_OUT_NOT_SUPPORTED);
}

/// A sell-back's quote is `floor_quote::sale_sell_back_stateless` exactly: `in_amount` every token,
/// `out_amount` `payout − fee`, `fee_amount` the fee in wSOL; a payout that floors to nothing and a sell
/// larger than everything sold are refused by name.
#[test]
fn sell_quotes_are_floor_quotes_stateless_sell_back() {
    for (price, sold, bps) in [(PRICE_ONE, SALE_TOKENS / 3, 0u16), (PRICE_ONE, 1_000_000, 100), (92_233_720_368_547, 59_405_940_400_000, 100), (7_682_302_212_939_176, 5, 40), (3 * PRICE_ONE + 7, SALE_TOKENS / 2, 1)] {
        let (_, l) = default_sale(price, sold, bps);
        let amm = amm_of(&l);
        for tokens in [1u64, 2, 1_000, 999_983, 1_000_000_000, 31_415_926_535, sold] {
            if tokens > sold {
                refused(sell(&amm, tokens), refusal::EXCEEDS_SOLD);
                continue;
            }
            let s = floor_quote::sale_sell_back_stateless(&l, tokens).unwrap();
            assert_eq!(s.payout as u128, (tokens as u128 * price) >> 64, "floor(tokens · price / 2^64)");
            if s.received == 0 {
                refused(sell(&amm, tokens), refusal::NOTHING);
                continue;
            }
            let got = sell(&amm, tokens).unwrap();
            assert_eq!((got.in_amount, got.out_amount, got.fee_amount, got.fee_mint), (tokens, s.received, s.team_fee, wsol()));
            assert_eq!(got.fee_pct, rust_decimal::Decimal::from(s.team_fee) / rust_decimal::Decimal::from(s.payout));
        }
    }
}

/// A sell-back's accounts: the 11 named `sell_back` accounts for the user, the user's wSOL ATA and the
/// SPL Token program, and on a hooked launch the 4-account holder tail; refused unless the source is the
/// user's Token-2022 ATA, the destination the user's wSOL ATA and the authority the user.
#[test]
fn sell_metas_name_the_users_accounts_and_refuse_anything_else() {
    let (mint, l) = default_sale(PRICE_ONE, SALE_TOKENS / 3, 100);
    let user = Pubkey::new_unique();
    let ata = token_2022_ata(&user, &mint);
    let wsol_dst = wsol_ata(&user);
    let params = |src: Pubkey, dst: Pubkey, auth: Pubkey| {
        let mut p = swap_params(mint, user, dst, auth, SwapMode::ExactIn);
        (p.source_mint, p.destination_mint, p.source_token_account) = (mint, wsol(), src);
        p
    };
    for hooked in [false, true] {
        let mut l = l;
        l.floor_pool = Pubkey::new_unique().to_bytes();
        if hooked {
            l.holder_buy = 20;
        }
        let amm = amm_of(&l);
        let sam = amm.get_swap_and_account_metas(&params(ata, wsol_dst, user)).unwrap();
        assert_eq!(sam.swap, swap_variant(false));
        let m = &sam.account_metas;
        assert_eq!(m.len(), if hooked { SELL_METAS_HOOKED } else { SELL_METAS });
        assert!(m[0].is_signer && m[0].is_writable && m[0].pubkey == user);
        assert_eq!(m.iter().filter(|a| a.is_signer).count(), 1, "the user is the only signer");
        assert_eq!(
            m.iter().map(|a| a.pubkey).take(13).collect::<Vec<_>>(),
            vec![
                user,
                amm.key(),
                sale_vault_pda(&DEVNET, &amm.key()),
                buyer_position_pda(&DEVNET, &amm.key(), &user),
                ata,
                Pubkey::new_from_array(l.token_vault),
                mint,
                floor_sale_amm::accounts::lock_authority_pda(&DEVNET),
                token_2022(),
                floor_sale_amm::accounts::event_authority_pda(&DEVNET),
                launch_program(&DEVNET),
                wsol_dst,
                spl_token(),
            ]
        );
        assert!(m[11].is_writable && !m[12].is_writable);
        if hooked {
            assert_eq!(
                m[13..].iter().map(|a| (a.pubkey, a.is_writable)).collect::<Vec<_>>(),
                vec![
                    (floor_holders_program(&DEVNET), false),
                    (holder_book_pda(&DEVNET, &mint), true),
                    (Pubkey::new_from_array(l.floor_pool), false),
                    (holder_record_pda(&DEVNET, &mint, &ata), true),
                ]
            );
        }
        refused(amm.get_swap_and_account_metas(&params(Pubkey::new_unique(), wsol_dst, user)), refusal::SOURCE_NOT_USER_ATA);
        refused(amm.get_swap_and_account_metas(&params(ata, Pubkey::new_unique(), user)), refusal::DESTINATION_NOT_USER_WSOL_ATA);
        refused(amm.get_swap_and_account_metas(&params(ata, wsol_dst, Pubkey::new_unique())), refusal::AUTHORITY_NOT_USER);
    }
}

#[test]
fn the_encoder_builds_buy_data_from_its_own_swap_only() {
    #[cfg(not(feature = "upstream-variant"))]
    assert_eq!(swap_variant(true), Swap::Placeholder { data: Some(vec![OP_BUY]) });
    #[cfg(feature = "upstream-variant")]
    assert_eq!(swap_variant(true), Swap::FloorSale { is_buy: true });
    // Both encode to the same data: the Placeholder's byte is the operation, not instruction data.
    let placeholder = Swap::Placeholder { data: Some(vec![OP_BUY]) };
    assert_eq!(buy_instruction_data(&placeholder, 9, 3).unwrap(), buy_instruction_data(&swap_variant(true), 9, 3).unwrap());
    check_buy(&swap_variant(true)).unwrap();
    assert!(check_buy(&swap_variant(false)).is_err(), "a sell swap is not a buy");
    assert!(check_buy(&Swap::Placeholder { data: Some(vec![2]) }).is_err());
    assert!(check_buy(&Swap::Placeholder { data: None }).is_err());
    assert!(check_buy(&Swap::Saber).is_err());
    let d = buy_instruction_data(&swap_variant(true), 5, 7).unwrap();
    assert_eq!(&d[..8], &floor_types::disc::launch::BUY);
    assert_eq!(u64::from_le_bytes(d[8..16].try_into().unwrap()), 5);
    assert_eq!(u64::from_le_bytes(d[16..24].try_into().unwrap()), 7);
    assert_eq!(u64::from_le_bytes(d[24..32].try_into().unwrap()), 0);
}

/// The adapter takes every amount from `floor-quote`: `floor-math` may be a dev-dependency (the
/// golden vectors), never a normal one. And the source reads no clock, network, file or random state.
#[test]
fn dependency_and_source_rules() {
    let manifest = std::fs::read_to_string("Cargo.toml").unwrap();
    let deps: String = manifest
        .split("\n[dependencies]")
        .nth(1)
        .unwrap()
        .split("\n[")
        .next()
        .unwrap()
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!deps.contains("floor-math"), "floor-math is not a normal dependency");
    assert!(!deps.contains("carpenter"), "nothing from Carpenter is a direct dependency");
    for entry in std::fs::read_dir("src").unwrap() {
        let path = entry.unwrap().path();
        let src = std::fs::read_to_string(&path).unwrap();
        for banned in ["std::net", "std::fs", "std::env", "SystemTime", "Instant::", "rand::", "clock_ref", "unix_timestamp"] {
            assert!(!src.contains(banned), "{path:?} uses {banned}");
        }
    }
    let _ = KeyedAccount { key: Pubkey::default(), account: Account::default(), params: None };
}

/// The sell-back encoder: `sell_back_wsol`'s Anchor discriminator (re-derived here from its name) and
/// the token count; refused for a buy swap; `instruction_data` dispatches on the side.
#[test]
fn the_encoder_builds_sell_data_from_its_own_swap_only() {
    let want: [u8; 8] = Sha256::digest(b"global:sell_back_wsol")[..8].try_into().unwrap();
    assert_eq!(SELL_BACK_WSOL_DISC, want);
    #[cfg(not(feature = "upstream-variant"))]
    assert_eq!(swap_variant(false), Swap::Placeholder { data: Some(vec![OP_SELL]) });
    #[cfg(feature = "upstream-variant")]
    assert_eq!(swap_variant(false), Swap::FloorSale { is_buy: false });
    assert_eq!(side_of(&swap_variant(false)).unwrap(), false);
    check_sell(&swap_variant(false)).unwrap();
    assert!(check_sell(&swap_variant(true)).is_err());
    assert!(sell_instruction_data(&swap_variant(true), 5).is_err());
    let placeholder = Swap::Placeholder { data: Some(vec![OP_SELL]) };
    let d = sell_instruction_data(&placeholder, 123_456_789).unwrap();
    assert_eq!(d, sell_instruction_data(&swap_variant(false), 123_456_789).unwrap());
    assert_eq!(d.len(), 16);
    assert_eq!(&d[..8], &want);
    assert_eq!(u64::from_le_bytes(d[8..16].try_into().unwrap()), 123_456_789);
    assert_eq!(instruction_data(&swap_variant(false), 123_456_789).unwrap(), d);
    assert_eq!(instruction_data(&swap_variant(true), 9).unwrap(), buy_instruction_data(&swap_variant(true), 9, 0).unwrap());
}
