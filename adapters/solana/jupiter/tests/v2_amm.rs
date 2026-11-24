// SPDX-License-Identifier: Apache-2.0
mod common;

use common::{account, accounts, Accounts, BASE_MINT, QUOTE_MINT};
use floor_amm_jupiter::amm::swap_metas;
use floor_amm_jupiter::keys::{self, PROGRAM_ID, TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID};
use floor_amm_jupiter::v2::layout::VERSION_V2;
use floor_amm_jupiter::v2::{trade_hook_metas, FloorAmmV2, LaunchV2Account, FLOOR_HOOK_ID, SWAP_METAS_V2};
use floor_amm_jupiter::{swap_instruction_data, LABEL, SWAP_METAS};
use jupiter_amm_interface::{Amm, AmmContext, KeyedAccount, QuoteParams, Swap, SwapMode, SwapParams};
use solana_pubkey::Pubkey;

fn launch() -> LaunchV2Account {
    LaunchV2Account {
        version: VERSION_V2,
        base_mint: BASE_MINT,
        quote_mint: QUOTE_MINT,
        quote_token_program: TOKEN_PROGRAM_ID,
        decimals: 6,
        quote_decimals: 6,
        ..Default::default()
    }
}

fn keyed(l: &LaunchV2Account) -> KeyedAccount {
    KeyedAccount { key: keys::launch_pda(&l.base_mint), account: account(PROGRAM_ID, l.encode()), params: None }
}

fn amm(l: &LaunchV2Account) -> FloorAmmV2 {
    let k = keyed(l);
    let mut a = FloorAmmV2::from_keyed_account(&k, &AmmContext::default()).unwrap();
    a.update(accounts([(k.key, k.account.clone())])).unwrap();
    a
}

fn sp<'a>(jup: &'a Pubkey, input: Pubkey, output: Pubkey, src: Pubkey, dst: Pubkey, auth: Pubkey) -> SwapParams<'a, 'a> {
    SwapParams {
        swap_mode: SwapMode::ExactIn,
        in_amount: 1_000_000,
        out_amount: 0,
        source_mint: input,
        destination_mint: output,
        source_token_account: src,
        destination_token_account: dst,
        token_transfer_authority: auth,
        user: auth,
        payer: auth,
        quote_mint_to_referrer: None,
        jupiter_program_id: jup,
        missing_dynamic_accounts_as_default: false,
    }
}

#[test]
fn trait_basics() {
    let a = amm(&launch());
    assert_eq!(a.label(), LABEL);
    assert_eq!(a.program_id(), PROGRAM_ID);
    assert_eq!(a.key(), keys::launch_pda(&BASE_MINT));
    assert_eq!(a.get_reserve_mints(), vec![QUOTE_MINT, BASE_MINT]);
    assert_eq!(a.get_accounts_to_update(), vec![a.key(), QUOTE_MINT, BASE_MINT]);
    assert!(!a.has_dynamic_accounts() && !a.requires_update_for_reserve_mints() && !a.supports_exact_out() && !a.unidirectional());
    assert_eq!(a.get_accounts_len(), SWAP_METAS_V2);
    assert_eq!(SWAP_METAS_V2, 25);
    assert!(a.underlying_liquidities().is_none());
    assert!(a.is_active());
    assert_eq!(
        a.program_dependencies(),
        vec![(TOKEN_2022_PROGRAM_ID, "spl_token_2022".to_string()), (FLOOR_HOOK_ID, "floor_hook".to_string())]
    );
}

#[test]
fn swap_metas_are_the_nineteen_then_the_hooks_six() {
    let a = amm(&launch());
    let jup = Pubkey::new_unique();
    let (src, dst, auth) = (Pubkey::new_unique(), Pubkey::new_unique(), Pubkey::new_unique());
    for is_buy in [true, false] {
        let (i, o) = if is_buy { (QUOTE_MINT, BASE_MINT) } else { (BASE_MINT, QUOTE_MINT) };
        let p = sp(&jup, i, o, src, dst, auth);
        let s = a.get_swap_and_account_metas(&p).unwrap();
        assert_eq!(s.account_metas.len(), SWAP_METAS_V2);
        assert_eq!(s.account_metas[..SWAP_METAS], swap_metas(&a.key(), &BASE_MINT, &QUOTE_MINT, &TOKEN_PROGRAM_ID, &p, is_buy)[..]);
        assert_eq!(s.account_metas[SWAP_METAS..], trade_hook_metas(&BASE_MINT, is_buy, &auth)[..]);
        assert!(matches!(s.swap, Swap::Placeholder { .. }));
        assert_eq!(
            swap_instruction_data(&s.swap, 7).unwrap(),
            swap_instruction_data(&floor_amm_jupiter::encode::placeholder(is_buy), 7).unwrap()
        );
        assert_eq!(s.account_metas[6].pubkey, if is_buy { dst } else { src });
        assert_eq!(s.account_metas[SWAP_METAS + 1].pubkey, a.key());
    }
    let p = sp(&jup, QUOTE_MINT, Pubkey::new_unique(), src, dst, auth);
    assert!(a.get_swap_and_account_metas(&p).is_err());
}

#[test]
fn from_keyed_account_refuses_what_is_not_a_v2_launch() {
    let l = launch();
    let k = keyed(&l);
    let ctx = AmmContext::default();
    let try_ = |key: Pubkey, a: solana_account::Account| FloorAmmV2::from_keyed_account(&KeyedAccount { key, account: a, params: None }, &ctx).map(|_| ());
    assert!(try_(k.key, k.account.clone()).is_ok());
    assert!(try_(k.key, account(TOKEN_PROGRAM_ID, k.account.data.clone())).unwrap_err().to_string().contains("not floor_amm"));
    let mut d = k.account.data.clone();
    d[0] ^= 1;
    assert!(try_(k.key, account(PROGRAM_ID, d)).unwrap_err().to_string().contains("discriminator"));
    let mut d = k.account.data.clone();
    d[8] = 1;
    assert!(try_(k.key, account(PROGRAM_ID, d)).unwrap_err().to_string().contains("version"));
    assert!(try_(Pubkey::new_unique(), k.account.clone()).unwrap_err().to_string().contains("not the launch PDA"));
    assert!(try_(k.key, account(PROGRAM_ID, vec![0; 8 + 5_112])).is_err());
}

#[test]
fn update_is_all_or_nothing_and_status_gates_activity() {
    let mut l = launch();
    let k = keyed(&l);
    let mut a = FloorAmmV2::from_keyed_account(&k, &AmmContext::default()).unwrap();
    assert!(!a.is_active());
    a.update(Accounts(Default::default())).unwrap_err();
    assert!(!a.is_active());
    l.status = 1;
    a.update(accounts([(k.key, account(PROGRAM_ID, l.encode()))])).unwrap();
    assert!(!a.is_active());
    let mut other = l;
    other.base_mint = Pubkey::new_unique();
    assert!(a.update(accounts([(k.key, account(PROGRAM_ID, other.encode()))])).is_err());
    assert!(!a.is_active());
}

#[test]
fn exact_out_and_foreign_mints_are_refused() {
    let a = amm(&launch());
    let q = |amount, i, o, m| QuoteParams { amount, input_mint: i, output_mint: o, swap_mode: m, fee_mode: Default::default() };
    assert!(a.quote(&q(1, QUOTE_MINT, BASE_MINT, SwapMode::ExactOut)).is_err());
    assert!(a.quote(&q(1, QUOTE_MINT, Pubkey::new_unique(), SwapMode::ExactIn)).is_err());
}
