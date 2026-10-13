//! Several clusters in one build: the launch account's owner selects the cluster, and every program
//! id and PDA a buy or sell-back names comes from that cluster's entry. The second cluster here is a
//! synthetic one that exists only in this test (the shipped table lists mainnet-beta and devnet).

mod common;

use common::*;
use floor_sale_amm::accounts::{
    buyer_position_pda, event_authority_pda, floor_holders_program, holder_book_pda, launch_pda, launch_program, lock_authority_pda,
    sale_vault_pda, token_2022_ata, wsol_ata,
};
use floor_sale_amm::FloorSaleAmm;
use floor_types::ids::{ClusterIds, DEVNET};
use jupiter_amm_interface::{AccountProvider, Amm, AmmContext, KeyedAccount, SwapMode, SwapParams};
use solana_account::{Account, ReadableAccount};
use solana_pubkey::Pubkey;

const SYNTH: ClusterIds = ClusterIds {
    name: "synthetic",
    carpenter_amm: [0xE1; 32],
    floor_hook: [0xE2; 32],
    floor_launch: [0xE3; 32],
    floor_holders: [0xE4; 32],
    floor_vesting: [0xE5; 32],
};
const TABLE: &[ClusterIds] = &[DEVNET, SYNTH];

/// The launch account as `cluster`'s `floor_launch` stores it, at its PDA there.
fn keyed_on(cluster: &ClusterIds, l: &floor_types::layout::Launch) -> KeyedAccount {
    let mut account = launch_account(l);
    account.owner = launch_program(cluster);
    KeyedAccount { key: launch_pda(cluster, &Pubkey::new_from_array(l.mint)), account, params: None }
}

fn devnet_keys() -> Vec<Pubkey> {
    let mut v: Vec<Pubkey> =
        [DEVNET.carpenter_amm, DEVNET.floor_hook, DEVNET.floor_launch, DEVNET.floor_holders, DEVNET.floor_vesting].map(Pubkey::new_from_array).to_vec();
    v.push(event_authority_pda(&DEVNET));
    v.push(lock_authority_pda(&DEVNET));
    v
}

fn params<'a>(source_mint: Pubkey, destination_mint: Pubkey, source: Pubkey, destination: Pubkey, user: Pubkey) -> SwapParams<'a, 'a> {
    SwapParams {
        swap_mode: SwapMode::ExactIn,
        in_amount: 1_000_000,
        out_amount: 0,
        source_mint,
        destination_mint,
        source_token_account: source,
        destination_token_account: destination,
        token_transfer_authority: user,
        user,
        payer: user,
        quote_mint_to_referrer: None,
        jupiter_program_id: &JUP,
        missing_dynamic_accounts_as_default: false,
    }
}

struct One(Pubkey, Account);
impl AccountProvider for One {
    fn get(&self, pubkey: &Pubkey) -> Option<impl ReadableAccount + use<'_>> {
        (*pubkey == self.0).then(|| self.1.clone())
    }
}

#[test]
fn the_owner_selects_the_cluster_and_no_devnet_id_reaches_a_buy_or_a_sell_back() {
    let (mint, mut l) = default_sale(PRICE_ONE, 1_000_000_000, 100);
    l.floor_pool = Pubkey::new_unique().to_bytes();
    l.holder_buy = 20; // hooked: the sell-back carries the holder tail
    let ka = keyed_on(&SYNTH, &l);
    let mut amm = FloorSaleAmm::from_keyed_account_in(&ka, TABLE).unwrap();
    assert_eq!(amm.cluster().name, "synthetic");
    assert_eq!(amm.program_id(), launch_program(&SYNTH));
    assert_eq!(amm.key(), launch_pda(&SYNTH, &mint));
    amm.update(One(ka.key, ka.account.clone())).unwrap();

    let user = Pubkey::new_unique();
    let ata = token_2022_ata(&user, &mint);
    let buy = amm.get_swap_and_account_metas(&params(wsol(), mint, Pubkey::new_unique(), ata, user)).unwrap().account_metas;
    assert_eq!(buy[2].pubkey, sale_vault_pda(&SYNTH, &amm.key()));
    assert_eq!(buy[4].pubkey, buyer_position_pda(&SYNTH, &amm.key(), &user));
    assert_eq!(buy[7].pubkey, lock_authority_pda(&SYNTH));
    assert_eq!(buy[11].pubkey, event_authority_pda(&SYNTH));
    assert_eq!(buy[12].pubkey, launch_program(&SYNTH));
    let sell = amm.get_swap_and_account_metas(&params(mint, wsol(), ata, wsol_ata(&user), user)).unwrap().account_metas;
    assert_eq!(sell[13].pubkey, floor_holders_program(&SYNTH));
    assert_eq!(sell[14].pubkey, holder_book_pda(&SYNTH, &mint));
    let devnet = devnet_keys();
    for m in buy.iter().chain(&sell) {
        assert!(!devnet.contains(&m.pubkey), "a devnet key {} reached the synthetic cluster's swap", m.pubkey);
    }

    // `update` keeps refusing an account that changes owner, even to another known cluster's program.
    let mut moved = ka.account.clone();
    moved.owner = launch_program(&DEVNET);
    assert!(amm.update(One(ka.key, moved)).is_err());
}

#[test]
fn an_unknown_owner_or_a_pda_of_another_cluster_is_refused() {
    let (_, l) = default_sale(PRICE_ONE, 0, 100);
    // The shipped table does not know the synthetic cluster.
    let err = FloorSaleAmm::from_keyed_account(&keyed_on(&SYNTH, &l), &AmmContext::default()).err().unwrap();
    assert!(err.to_string().contains("not owned by floor_launch"), "{err}");
    // Owned by the synthetic floor_launch but keyed at devnet's launch PDA.
    let mut ka = keyed_on(&SYNTH, &l);
    ka.key = launch_pda(&DEVNET, &Pubkey::new_from_array(l.mint));
    let err = FloorSaleAmm::from_keyed_account_in(&ka, TABLE).err().unwrap();
    assert!(err.to_string().contains("not the launch PDA"), "{err}");
    // Devnet's launches are unchanged through the extended table.
    let dev = FloorSaleAmm::from_keyed_account_in(&keyed(&l), TABLE).unwrap();
    assert_eq!(dev.program_id(), launch_program(&DEVNET));
    assert_eq!(dev.cluster(), &DEVNET);
}
