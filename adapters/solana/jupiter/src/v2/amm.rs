// SPDX-License-Identifier: Apache-2.0
use std::collections::HashSet;

use floor_amm_solana::metas::full_swap_metas;
use floor_amm_solana::quote::{QuoteError, QuoteState};
use floor_math::v2::trade::{BookBuy, BookSell};
use jupiter_amm_interface::{
    AccountProvider, Amm, AmmContext, AmmError, AmmLabel, ClockRef, KeyedAccount, Quote, QuoteParams, SwapAndAccountMetas, SwapMode, SwapParams,
};
use rust_decimal::Decimal;
use solana_account::ReadableAccount;
use solana_pubkey::Pubkey;

use super::hook::{SWAP_METAS_V2, SWAP_METAS_V2_QUOTE_HOOKED};
use super::layout::{LaunchV2Account, STATUS_LIVE_V2};
use super::quote_mint::QuoteMint;
use crate::amm::swap_accounts;
use crate::encode::placeholder;
use crate::keys::{self, PROGRAM_ID, TOKEN_2022_PROGRAM_ID};
use crate::LABEL;

fn account_program<A: ReadableAccount + ?Sized>(a: &A) -> &Pubkey {
    a.owner()
}

fn err(s: impl Into<String>) -> AmmError {
    AmmError::Custom(s.into())
}

fn qerr(e: QuoteError) -> AmmError {
    err(format!("floor-amm: {e}"))
}

pub fn fee_pct(fee: u128, of: u128) -> Decimal {
    if of == 0 {
        return Decimal::ZERO;
    }
    Decimal::from_i128_with_scale(fee as i128, 0) / Decimal::from_i128_with_scale(of as i128, 0)
}

#[derive(Clone)]
pub struct FloorAmmV2 {
    key: Pubkey,
    launch: LaunchV2Account,
    state: QuoteState,
    clock: ClockRef,
    updated: bool,
}

impl std::fmt::Debug for FloorAmmV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FloorAmmV2").field("key", &self.key).field("base_mint", &self.launch.base_mint).field("updated", &self.updated).finish()
    }
}

impl FloorAmmV2 {
    pub fn launch(&self) -> &LaunchV2Account {
        &self.launch
    }

    fn live(&self) -> Result<&LaunchV2Account, AmmError> {
        if !self.updated {
            return Err(err("floor-amm: no successful update() — the last pass failed or none has run"));
        }
        if self.launch.status != STATUS_LIVE_V2 {
            return Err(err(format!("floor-amm: launch status {} is not live", self.launch.status)));
        }
        Ok(&self.launch)
    }

    pub fn clock(&self) -> (i64, u64) {
        use std::sync::atomic::Ordering::Relaxed;
        (self.clock.unix_timestamp.load(Relaxed), self.clock.slot.load(Relaxed))
    }

    fn epoch(&self) -> u64 {
        self.clock.epoch.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn quote_mint(&self) -> &QuoteMint {
        self.state.quote_mint()
    }

    pub fn simulate_buy(&self, gross: u64) -> Result<BookBuy, AmmError> {
        self.live()?;
        self.state.simulate_buy(gross).map_err(qerr)
    }

    pub fn simulate_sell(&self, tokens: u64) -> Result<BookSell, AmmError> {
        self.live()?;
        self.state.simulate_sell(tokens).map_err(qerr)
    }

    fn is_buy(&self, input: &Pubkey, output: &Pubkey) -> Result<bool, AmmError> {
        let (q, b) = (self.launch.quote_mint, self.launch.base_mint);
        if *input == q && *output == b {
            Ok(true)
        } else if *input == b && *output == q {
            Ok(false)
        } else {
            Err(err(format!("mints {input} → {output} are not this launch's pair ({q}, {b})")))
        }
    }

    fn decode(key: &Pubkey, program: &Pubkey, data: &[u8]) -> Result<LaunchV2Account, AmmError> {
        if *program != PROGRAM_ID {
            return Err(err(format!("account {key} is owned by {program}, not floor_amm")));
        }
        LaunchV2Account::decode(data).map_err(|e| err(e.to_string()))
    }
}

impl Amm for FloorAmmV2 {
    fn from_keyed_account(keyed_account: &KeyedAccount, amm_context: &AmmContext) -> Result<Self, AmmError> {
        let launch = Self::decode(&keyed_account.key, account_program(&keyed_account.account), &keyed_account.account.data)?;
        if keys::launch_pda(&launch.base_mint) != keyed_account.key {
            return Err(err(format!("account {} is not the launch PDA of {}", keyed_account.key, launch.base_mint)));
        }
        let state = QuoteState::new(&launch, QuoteMint::default());
        Ok(FloorAmmV2 { key: keyed_account.key, launch, state, clock: amm_context.clock_ref.clone(), updated: false })
    }

    fn label(&self) -> AmmLabel {
        LABEL
    }

    fn program_id(&self) -> Pubkey {
        PROGRAM_ID
    }

    fn key(&self) -> Pubkey {
        self.key
    }

    fn get_reserve_mints(&self) -> Vec<Pubkey> {
        vec![self.launch.quote_mint, self.launch.base_mint]
    }

    fn get_accounts_to_update(&self) -> Vec<Pubkey> {
        vec![self.key, self.launch.quote_mint, self.launch.base_mint]
    }

    fn update(&mut self, account_provider: impl AccountProvider) -> Result<(), AmmError> {
        let staged = (|| {
            let acc = account_provider.try_get(&self.key)?;
            let launch = Self::decode(&self.key, account_program(&acc), acc.data())?;
            if launch.base_mint != self.launch.base_mint {
                return Err(err("the launch account names another mint"));
            }
            let mut launch = launch;
            let bm = account_provider.try_get(&launch.base_mint)?;
            if *account_program(&bm) != TOKEN_2022_PROGRAM_ID {
                return Err(err(format!("base mint {} is owned by {}, not Token-2022", launch.base_mint, account_program(&bm))));
            }
            let supply: Option<[u8; 8]> = bm.data().get(36..44).and_then(|b| b.try_into().ok());
            let supply = u64::from_le_bytes(supply.ok_or_else(|| err("base mint: short data"))?);
            drop(bm);
            launch.sync_supply(supply).ok_or_else(|| err("floor-amm: incinerated overflows (the program refuses Overflow)"))?;
            let qm = account_provider.try_get(&launch.quote_mint)?;
            if *account_program(&qm) != launch.quote_token_program {
                return Err(err(format!("quote mint {} is owned by {}, not the launch's quote token program", launch.quote_mint, account_program(&qm))));
            }
            let quote = QuoteMint::decode(account_program(&qm), qm.data());
            Ok((launch, quote))
        })();
        match staged {
            Ok((launch, quote)) => {
                self.state = QuoteState::new(&launch, quote);
                self.launch = launch;
                self.updated = true;
                Ok(())
            }
            Err(e) => {
                self.updated = false;
                Err(e)
            }
        }
    }

    fn quote(&self, quote_params: &QuoteParams) -> Result<Quote, AmmError> {
        if quote_params.swap_mode == SwapMode::ExactOut {
            return Err(err("floor-amm: exact-out is not supported"));
        }
        let is_buy = self.is_buy(&quote_params.input_mint, &quote_params.output_mint)?;
        self.live()?;
        let q = self.state.quote(is_buy, quote_params.amount, self.epoch()).map_err(qerr)?;
        Ok(Quote { in_amount: q.in_amount, out_amount: q.out_amount, fee_amount: q.fee_amount, fee_mint: self.launch.quote_mint, fee_pct: fee_pct(q.fee_amount as u128, q.fee_base) })
    }

    fn get_swap_and_account_metas(&self, swap_params: &SwapParams) -> Result<SwapAndAccountMetas, AmmError> {
        if swap_params.swap_mode == SwapMode::ExactOut {
            return Err(err("floor-amm: exact-out is not supported"));
        }
        let is_buy = self.is_buy(&swap_params.source_mint, &swap_params.destination_mint)?;
        let l = &self.launch;
        let account_metas = full_swap_metas(&self.key, &l.base_mint, &l.quote_mint, &l.quote_token_program, self.quote_mint().hooked, &swap_accounts(swap_params), is_buy);
        debug_assert_eq!(account_metas.len(), self.get_accounts_len());
        Ok(SwapAndAccountMetas { swap: placeholder(is_buy), account_metas })
    }

    fn has_dynamic_accounts(&self) -> bool {
        false
    }

    fn requires_update_for_reserve_mints(&self) -> bool {
        false
    }

    fn supports_exact_out(&self) -> bool {
        false
    }

    fn unidirectional(&self) -> bool {
        false
    }

    fn program_dependencies(&self) -> Vec<(Pubkey, String)> {
        vec![(TOKEN_2022_PROGRAM_ID, "spl_token_2022".to_string()), (super::hook::FLOOR_HOOK_ID, "floor_hook".to_string())]
    }

    fn get_accounts_len(&self) -> usize {
        if self.quote_mint().hooked {
            SWAP_METAS_V2_QUOTE_HOOKED
        } else {
            SWAP_METAS_V2
        }
    }

    fn underlying_liquidities(&self) -> Option<HashSet<Pubkey>> {
        None
    }

    fn is_active(&self) -> bool {
        self.updated && self.launch.status == STATUS_LIVE_V2
    }
}
