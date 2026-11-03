// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::collections::HashMap;

use async_trait::async_trait;
use solana_account::Account;
use solana_program_pack::Pack;
use solana_pubkey::Pubkey;

use floor_amm_solana::keys;
use floor_amm_solana::layout::{LaunchV2Account, VERSION_V2};
use titan_integration_template::your_venue::FloorVenue;
use titan_integration_template::your_venue::{CLOCK_SYSVAR, to_core, to_titan};
use floor_math::v2::create::{Clock, CreateCtx, Launch, create};
use floor_math::v2::preset::from_design;
use floor_math::v2::rows::{self, Row};
use floor_math::v2::trade;
use titan_integration_template::account_caching::{AccountCacheError, AccountsCache};
use titan_integration_template::trading_venue::FromAccount;

#[derive(Default, Clone)]
pub struct FixtureCache(pub HashMap<Pubkey, Account>);

#[async_trait]
impl AccountsCache for FixtureCache {
    async fn get_account(&self, pubkey: &Pubkey) -> Result<Option<Account>, AccountCacheError> {
        Ok(self.0.get(pubkey).cloned())
    }

    async fn get_accounts(&self, pubkeys: &[Pubkey]) -> Result<Vec<Option<Account>>, AccountCacheError> {
        Ok(pubkeys.iter().map(|k| self.0.get(k).cloned()).collect())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Quote {
    pub name: &'static str,
    pub decimals: u8,
    pub usd_cents: u128,
}

pub const USDC: Quote = Quote { name: "USDC", decimals: 6, usd_cents: 100 };
pub const SOL: Quote = Quote { name: "SOL@$150", decimals: 9, usd_cents: 15_000 };

impl Quote {
    pub fn ctx(&self) -> CreateCtx {
        CreateCtx { quote_decimals: self.decimals, quote_usd_cents_x64: self.usd_cents << 64, token_decimals: 6, clock: Clock { now: 1_700_000_000, slot: 1 } }
    }
    pub fn units(&self, usd: f64) -> u128 {
        (usd * 100.0 / self.usd_cents as f64 * 10f64.powi(self.decimals as i32)) as u128
    }
}

pub struct State {
    pub name: String,
    pub preset: u8,
    pub row: Row,
    pub quote: Quote,
    pub l: Launch,
}

fn buy(l: &mut Launch, gross: u128) -> u64 {
    let (mut tokens, mut left) = (0u64, gross);
    for _ in 0..1_000 {
        if left == 0 {
            break;
        }
        let Ok(r) = trade::buy(&mut l.book, &l.split, l.supply, left, 0) else { break };
        tokens += r.tokens;
        left = r.refund;
        if !r.budget_hit {
            break;
        }
    }
    tokens
}

fn sell(l: &mut Launch, tokens: u64) -> u64 {
    trade::sell(&mut l.book, &l.split, l.supply, tokens, 0).map(|r| r.tokens_in - r.unfilled).unwrap_or(0)
}

pub fn states() -> Vec<State> {
    let mut out = vec![];
    for (preset, row) in [(1u8, rows::wagmi()), (3, rows::moonshot())] {
        for q in [USDC, SOL] {
            let rv = from_design(&row).expect("row");
            let mut l = create(&rv, &q.ctx()).expect("create");
            let mut outside = 0u64;
            let mut push = |name: &str, l: &Launch| out.push(State { name: format!("P{preset}/{}/{name}", q.name), preset, row: row.clone(), quote: q, l: *l });
            push("open", &l);
            for _ in 0..10 {
                outside += { let g = l.raise / 20; buy(&mut l, g) };
            }
            push("window-half", &l);
            outside += { let g = l.raise * 6 / 10; buy(&mut l, g) };
            assert!(l.book.graduated, "graduated");
            push("graduated", &l);
            for i in 0..40 {
                outside += { let g = l.raise / 10 * (1 + i % 3); buy(&mut l, g) };
            }
            push("stacks", &l);
            for _ in 0..12 {
                outside -= sell(&mut l, outside / 20);
            }
            push("bought-back", &l);
            for _ in 0..4 {
                let g = l.raise / 20; buy(&mut l, g);
            }
            push("rebuy", &l);
        }
    }
    out
}

pub fn base_mint(s: &State) -> Pubkey {
    let mut b = [0u8; 32];
    b[0] = 0xF1;
    b[1] = s.preset;
    b[2] = s.quote.decimals;
    b[3] = s.name.len() as u8;
    b[4..12].copy_from_slice(&s.l.book.sqrt_p.to_le_bytes()[..8]);
    Pubkey::new_from_array(b)
}

pub fn quote_mint(s: &State) -> Pubkey {
    Pubkey::new_from_array([0xC0, s.quote.decimals, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7])
}

pub fn launch_account(s: &State) -> LaunchV2Account {
    let r = from_design(&s.row).expect("row");
    let mut l = LaunchV2Account {
        version: VERSION_V2,
        base_mint: to_core(&base_mint(s)),
        quote_mint: to_core(&quote_mint(s)),
        quote_token_program: keys::TOKEN_PROGRAM_ID,
        decimals: 6,
        quote_decimals: s.quote.decimals,
        preset_id: s.preset,
        supply: s.l.supply.live,
        dead_tokens: s.l.supply.dead,
        phi_bps: r.phi_bps,
        ck_bps: r.ck_target_bps,
        wc_bps: r.wc_bps,
        ms_class: r.ms_class,
        flags: r.flags,
        platform_ms_bps: r.platform_ms_bps,
        floor_ms_bps: r.floor_ms_bps,
        listed: 1,
        ..Default::default()
    };
    l.set_book(&s.l.book).expect("set_book");
    assert_eq!(l.book(), s.l.book, "{}: the book round-trips through the account", s.name);
    assert_eq!(l.split_row(), s.l.split, "{}: the split row round-trips through the account", s.name);
    l
}

pub fn mint_account(program: Pubkey, supply: u64, decimals: u8) -> Account {
    let mut data = vec![0u8; spl_token::state::Mint::LEN];
    spl_token::state::Mint { mint_authority: None.into(), supply, decimals, is_initialized: true, freeze_authority: None.into() }.pack_into_slice(&mut data);
    Account { lamports: 1_000_000, data, owner: program, executable: false, rent_epoch: 0 }
}

pub fn clock_account(epoch: u64) -> Account {
    let c = solana_sdk::clock::Clock { slot: 1, epoch_start_timestamp: 0, epoch, leader_schedule_epoch: epoch, unix_timestamp: 1_700_000_000 };
    Account { lamports: 1, data: bincode::serialize(&c).unwrap(), owner: Pubkey::from_str_const("Sysvar1111111111111111111111111111111111111"), executable: false, rent_epoch: 0 }
}

pub fn cache_for(l: &LaunchV2Account, quote_mint_acc: Account) -> (Pubkey, FixtureCache) {
    let key = to_titan(&keys::launch_pda(&l.base_mint));
    let mut m = HashMap::new();
    m.insert(key, Account { lamports: 1_000_000, data: l.encode(), owner: to_titan(&keys::PROGRAM_ID), executable: false, rent_epoch: 0 });
    m.insert(to_titan(&l.base_mint), mint_account(to_titan(&keys::TOKEN_2022_PROGRAM_ID), l.supply, 6));
    m.insert(to_titan(&l.quote_mint), quote_mint_acc);
    m.insert(CLOCK_SYSVAR, clock_account(800));
    (key, FixtureCache(m))
}

pub async fn venue(s: &State) -> (FloorVenue, LaunchV2Account) {
    let l = launch_account(s);
    let (key, cache) = cache_for(&l, mint_account(to_titan(&keys::TOKEN_PROGRAM_ID), 1 << 60, s.quote.decimals));
    let mut v = FloorVenue::from_account(&key, &cache.0[&key]).expect("from_account");
    titan_integration_template::trading_venue::TradingVenue::update_state(&mut v, &cache).await.expect("update_state");
    (v, l)
}
