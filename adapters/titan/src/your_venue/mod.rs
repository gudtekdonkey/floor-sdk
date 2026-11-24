// SPDX-License-Identifier: Apache-2.0
use async_trait::async_trait;
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;

use floor_amm_solana::creation::LaunchCreatedV2;
use floor_amm_solana::encode::instruction_data;
use floor_amm_solana::hook::{FLOOR_HOOK_ID, extra_metas_pda, holder_book_pda, holder_position_pda};
use floor_amm_solana::keys;
use floor_amm_solana::layout::{LaunchV2Account, STATUS_LIVE_V2};
use floor_amm_solana::metas::{SwapAccounts, full_swap_metas};
use floor_amm_solana::quote_mint::QuoteMint;
use floor_math::v2::book::{Book, WalkCheckpoints, SOL_BUY_WALK_STEPS};
use floor_math::v2::fees::{FEE_LEGS, Lg, LgCache, cheaper};
use floor_math::v2::sqrt::price_x64;
use floor_math::v2::marginal;
use floor_math::v2::split::SplitRow;
use floor_math::v2::supply::Supply;
use floor_math::v2::trade;

use crate::account_caching::AccountsCache;
use crate::trading_venue::error::{ErrorInfo, TradingVenueError};
use crate::trading_venue::protocol::PoolProtocol;
use crate::trading_venue::token_info::TokenInfo;
use crate::trading_venue::venue_creation::{ParsedInstruction, PoolCreation};
use crate::trading_venue::bounds::find_boundaries;
use crate::trading_venue::{
    AddressLookupTableTrait, FromAccount, QuoteRequest, QuoteResult, SwapType, TradingVenue,
};

pub const PROTOCOL: PoolProtocol = PoolProtocol::FloorAmm;
pub const LABEL: &str = "FLOOR";

pub const CLOCK_SYSVAR: Pubkey = Pubkey::from_str_const("SysvarC1ock11111111111111111111111111111111");
pub const LAUNCH_DECIMALS: u8 = 6;

pub fn to_titan(k: &solana_pubkey_v4::Pubkey) -> Pubkey {
    Pubkey::new_from_array(k.to_bytes())
}

pub fn to_core(k: &Pubkey) -> solana_pubkey_v4::Pubkey {
    solana_pubkey_v4::Pubkey::new_from_array(k.to_bytes())
}

pub fn program_id() -> Pubkey {
    to_titan(&keys::PROGRAM_ID)
}

fn err(s: &'static str) -> ErrorInfo {
    ErrorInfo::StaticStr(s)
}

pub fn parse_pool_creations(instructions: &[ParsedInstruction]) -> Vec<PoolCreation> {
    let id = program_id();
    instructions
        .iter()
        .filter(|ix| ix.program_id == id)
        .filter_map(|ix| LaunchCreatedV2::parse(&ix.data))
        .map(|e| PoolCreation { protocol: PROTOCOL, pool: to_titan(&e.launch), mints: vec![to_titan(&e.mint), to_titan(&e.quote_mint)] })
        .collect()
}

#[derive(Clone, Debug)]
pub struct Cached {
    pub book: Book,
    pub split: SplitRow,
    pub supply: Supply,
    pub quote_mint: QuoteMint,
    pub epoch: u64,
    pub memo: LgCache,
    pub pre: [u32; FEE_LEGS],
    pub cps: WalkCheckpoints,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fill {
    pub amount: u64,
    pub out: u64,
    pub not_enough_liquidity: bool,
    pub price: f64,
    pub graduates: bool,
}

const Q64: f64 = 18_446_744_073_709_551_616.0;
const PPM: f64 = 1_000_000.0;

impl Cached {
    pub fn new(l: &LaunchV2Account, quote_mint: QuoteMint, epoch: u64) -> Cached {
        let book = l.book();
        let mut memo = LgCache::new();
        book.warm_lg(&mut memo);
        let pre = book.fees_at_memo(book.sqrt_p, &mut memo);
        let _ = marginal::buy_point_with(&book, &mut memo);
        let _ = marginal::sell_point_with(&book, &mut memo);
        let cps = WalkCheckpoints::new(&book, &mut memo);
        Cached { book, split: l.split_row(), supply: l.supply(), quote_mint, epoch, memo, pre, cps }
    }

    pub fn rate<M: Lg + ?Sized>(&self, is_buy: bool, post: &Book, memo: &mut M) -> f64 {
        let pt = if is_buy { marginal::buy_point_with(post, memo) } else { marginal::sell_point_with(post, memo) };
        self.rate_at(is_buy, pt)
    }

    pub fn rate_at(&self, is_buy: bool, pt: Option<marginal::Point>) -> f64 {
        let Some(pt) = pt else { return 0.0 };
        let legs: u32 = cheaper(&pt.ppm, &self.pre).iter().sum();
        let p = price_x64(pt.sqrt) as f64 / Q64;
        if is_buy {
            if p > 0.0 { PPM / (p * (PPM + legs as f64)) } else { 0.0 }
        } else {
            p * (PPM - legs as f64) / PPM
        }
    }

    fn empty(price: f64) -> Fill {
        Fill { amount: 0, out: 0, not_enough_liquidity: price <= 0.0, price, graduates: false }
    }

    pub fn fill(&self, is_buy: bool, amount: u64) -> Fill {
        let mut memo = self.memo.peek();
        let b = &self.book;
        let qm = &self.quote_mint;
        if is_buy {
            if amount == 0 {
                return Cached::empty(self.rate(true, b, &mut memo));
            }
            let withheld = qm.transfer_fee(self.epoch, amount);
            if withheld >= amount {
                return Fill { not_enough_liquidity: true, ..Cached::empty(0.0) };
            }
            let Ok(r) = trade::quote_buy_cp(&self.cps, &self.split, self.supply, (amount - withheld) as u128, SOL_BUY_WALK_STEPS, true, &mut memo) else {
                return Fill { not_enough_liquidity: true, ..Cached::empty(0.0) };
            };
            let Ok(refund) = u64::try_from(r.refund) else { return Fill { not_enough_liquidity: true, ..Cached::empty(0.0) } };
            let price = self.rate_at(true, r.next);
            Fill {
                amount: amount - qm.received(self.epoch, refund),
                out: r.tokens,
                not_enough_liquidity: r.refund > 0 && (r.budget_hit || price <= 0.0),
                price,
                graduates: r.graduated_now && !self.book.graduated,
            }
        } else {
            if amount == 0 {
                return Cached::empty(self.rate(false, b, &mut memo));
            }
            let Ok(r) = trade::quote_sell_cp_end(&self.cps, &self.split, self.supply, amount, &mut memo) else {
                return Fill { not_enough_liquidity: true, ..Cached::empty(0.0) };
            };
            let Ok(out) = u64::try_from(r.out) else { return Fill { not_enough_liquidity: true, ..Cached::empty(0.0) } };
            let price = self.rate_at(false, r.next);
            Fill { amount: r.tokens_in - r.unfilled, out: qm.received(self.epoch, out), not_enough_liquidity: r.unfilled > 0 || price <= 0.0, price, graduates: false }
        }
    }
}

const MVT_REL_TOL: f64 = 1e-5;
const MVT_POINTS: f64 = 64.0;

pub fn min_size(c: &Cached, is_buy: bool, upper: u64) -> u64 {
    let k = (FEE_LEGS as f64 + 1.0) / MVT_REL_TOL;
    let n = MVT_POINTS - 1.0;
    let u = upper as f64;
    let clears = |a: u64| {
        let af = a as f64;
        let step = af * ((u / af).powf(1.0 / n) - 1.0);
        let per_in = if is_buy { 1.0 } else { c.fill(false, a).price };
        step * per_in >= k
    };
    let peak = ((u * ((n - 1.0) / n).powf(n)) as u64).max(1);
    if !clears(peak) {
        return upper;
    }
    let (mut lo, mut hi) = (0u64, peak);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if mid > 0 && clears(mid) { hi = mid } else { lo = mid }
    }
    hi
}

#[derive(Clone)]
pub struct FloorVenue {
    pub key: Pubkey,
    pub launch: LaunchV2Account,
    token_info: Vec<TokenInfo>,
    state: Option<Cached>,
    initialized: bool,
    directions: Vec<(u8, u8)>,
}

impl FloorVenue {
    pub fn base_mint(&self) -> Pubkey {
        to_titan(&self.launch.base_mint)
    }

    pub fn quote_mint(&self) -> Pubkey {
        to_titan(&self.launch.quote_mint)
    }

    pub fn quote_token_program(&self) -> Pubkey {
        to_titan(&self.launch.quote_token_program)
    }

    pub fn cached(&self) -> Option<&Cached> {
        self.state.as_ref()
    }

    pub fn is_buy(&self, input: &Pubkey, output: &Pubkey) -> Result<bool, TradingVenueError> {
        let (base, quote) = (self.base_mint(), self.quote_mint());
        if *input == quote && *output == base {
            Ok(true)
        } else if *input == base && *output == quote {
            Ok(false)
        } else {
            Err(TradingVenueError::InvalidMint(err("not this launch's pair")))
        }
    }

    fn decode(pubkey: &Pubkey, account: &Account) -> Result<LaunchV2Account, TradingVenueError> {
        if account.owner != program_id() {
            return Err(TradingVenueError::FromAccountError(err("the account is not the FLOOR Program's")));
        }
        let l = LaunchV2Account::decode(&account.data).map_err(|_| TradingVenueError::DeserializationFailed(err("not a LaunchV2 account")))?;
        if to_titan(&keys::launch_pda(&l.base_mint)) != *pubkey {
            return Err(TradingVenueError::FromAccountError(err("the account is not its mint's launch PDA")));
        }
        Ok(l)
    }

    fn get(accounts: &[Option<Account>], i: usize, what: &'static str) -> Result<Account, TradingVenueError> {
        accounts.get(i).cloned().flatten().ok_or(TradingVenueError::NoAccountFound(err(what)))
    }
}

impl FromAccount for FloorVenue {
    fn from_account(pubkey: &Pubkey, account: &Account) -> Result<Self, TradingVenueError> {
        let launch = FloorVenue::decode(pubkey, account)?;
        Ok(FloorVenue { key: *pubkey, launch, token_info: vec![], state: None, initialized: false, directions: vec![] })
    }
}

#[async_trait]
impl TradingVenue for FloorVenue {
    fn initialized(&self) -> bool {
        self.initialized
    }

    fn program_id(&self) -> Pubkey {
        program_id()
    }

    fn directions_num(&self) -> Vec<(u8, u8)> {
        self.directions.clone()
    }

    fn program_dependencies(&self) -> Vec<Pubkey> {
        let mut v = vec![program_id(), to_titan(&keys::TOKEN_2022_PROGRAM_ID), to_titan(&FLOOR_HOOK_ID)];
        let q = self.quote_token_program();
        if !v.contains(&q) {
            v.push(q);
        }
        v
    }

    fn market_id(&self) -> Pubkey {
        self.key
    }

    fn get_token_info(&self) -> &[TokenInfo] {
        &self.token_info
    }

    fn protocol(&self) -> PoolProtocol {
        PROTOCOL
    }

    fn label(&self) -> String {
        LABEL.to_string()
    }

    fn get_required_pubkeys_for_update(&self) -> Result<Vec<Pubkey>, TradingVenueError> {
        Ok(vec![self.key, self.base_mint(), self.quote_mint(), CLOCK_SYSVAR])
    }

    async fn update_state(&mut self, cache: &dyn AccountsCache) -> Result<(), TradingVenueError> {
        self.initialized = false;
        self.state = None;
        self.directions.clear();
        let keys = self.get_required_pubkeys_for_update()?;
        let accounts = cache.get_accounts(&keys).await?;
        let la = FloorVenue::get(&accounts, 0, "the launch account")?;
        let mut launch = FloorVenue::decode(&self.key, &la)?;
        if launch.base_mint != self.launch.base_mint {
            return Err(TradingVenueError::FromAccountError(err("the launch account names another mint")));
        }
        let bm = FloorVenue::get(&accounts, 1, "the launch token's mint")?;
        if bm.owner != to_titan(&keys::TOKEN_2022_PROGRAM_ID) {
            return Err(TradingVenueError::InvalidMint(err("the launch token is not Token-2022")));
        }
        let supply = bm.data.get(36..44).and_then(|b| <[u8; 8]>::try_from(b).ok()).map(u64::from_le_bytes);
        let supply = supply.ok_or(TradingVenueError::DeserializationFailed(err("the launch token's mint is short")))?;
        launch.sync_supply(supply).ok_or(TradingVenueError::CheckedMathError(err("incinerated overflows")))?;
        let qm = FloorVenue::get(&accounts, 2, "the quote mint")?;
        if qm.owner != to_titan(&launch.quote_token_program) {
            return Err(TradingVenueError::InvalidMint(err("the quote mint is not under the launch's quote token program")));
        }
        let quote = QuoteMint::decode(&launch.quote_token_program, &qm.data);
        let clock = FloorVenue::get(&accounts, 3, "the clock sysvar")?;
        let epoch = clock.data.get(16..24).and_then(|b| <[u8; 8]>::try_from(b).ok()).map(u64::from_le_bytes);
        let epoch = epoch.ok_or(TradingVenueError::DeserializationFailed(err("the clock sysvar is short")))?;
        let base_info = TokenInfo::new(&to_titan(&launch.base_mint), &bm, epoch)?;
        let quote_info = TokenInfo::new(&to_titan(&launch.quote_mint), &qm, epoch)?;
        let routable = launch.status == STATUS_LIVE_V2 && !quote.hooked && quote.fee.is_none() && launch.decimals == LAUNCH_DECIMALS && base_info.decimals == LAUNCH_DECIMALS as i32;
        self.state = Some(Cached::new(&launch, quote, epoch));
        self.launch = launch;
        self.token_info = vec![base_info, quote_info];
        self.initialized = routable;
        if routable {
            self.directions = [(1u8, 0u8), (0, 1)].into_iter().filter(|&(i, o)| self.bounds(i, o).is_ok()).collect();
        }
        Ok(())
    }

    fn quote(&self, request: QuoteRequest) -> Result<QuoteResult, TradingVenueError> {
        if request.swap_type == SwapType::ExactOut {
            return Err(TradingVenueError::ExactOutNotSupported);
        }
        let is_buy = self.is_buy(&request.input_mint, &request.output_mint)?;
        let Some(c) = self.state.as_ref().filter(|_| self.initialized) else {
            return Err(TradingVenueError::NotInitialized(err("the launch is not loaded or not routable")));
        };
        let f = c.fill(is_buy, request.amount);
        Ok(QuoteResult {
            input_mint: request.input_mint,
            output_mint: request.output_mint,
            amount: f.amount,
            expected_output: f.out,
            not_enough_liquidity: f.not_enough_liquidity,
            price: f.price,
        })
    }

    fn bounds(&self, tkn_in_ind: u8, tkn_out_ind: u8) -> Result<(u64, u64), TradingVenueError> {
        let input_mint = self.get_token(tkn_in_ind as usize)?.pubkey;
        let output_mint = self.get_token(tkn_out_ind as usize)?.pubkey;
        let is_buy = self.is_buy(&input_mint, &output_mint)?;
        let c = self.state.as_ref().filter(|_| self.initialized).ok_or(TradingVenueError::NotInitialized(err("the launch is not loaded or not routable")))?;
        let f = |x: u64| self.quote(QuoteRequest { amount: x, swap_type: SwapType::ExactIn, input_mint, output_mint });
        let (lower, mut upper) = find_boundaries(&f)?;
        if is_buy && !c.book.graduated && c.fill(true, upper).graduates {
            let (mut lo, mut hi) = (lower, upper);
            if c.fill(true, lo).graduates {
                return Err(TradingVenueError::NoQuotableValue(err("every buy graduates the launch")));
            }
            while hi - lo > 1 {
                let mid = lo + (hi - lo) / 2;
                if c.fill(true, mid).graduates { hi = mid } else { lo = mid }
            }
            upper = lo;
        }
        let lower = lower.max(min_size(c, is_buy, upper));
        if lower >= upper {
            return Err(TradingVenueError::NoQuotableValue(err("no size clears the rounding floor")));
        }
        Ok((lower, upper))
    }

    fn generate_swap_instruction(&self, request: QuoteRequest, user: Pubkey) -> Result<Instruction, TradingVenueError> {
        if request.swap_type == SwapType::ExactOut {
            return Err(TradingVenueError::ExactOutNotSupported);
        }
        let is_buy = self.is_buy(&request.input_mint, &request.output_mint)?;
        let l = &self.launch;
        let quote_hooked = self.state.as_ref().map(|c| c.quote_mint.hooked).unwrap_or(false);
        let u = to_core(&user);
        let base_ata = keys::ata(&u, &l.base_mint, &keys::TOKEN_2022_PROGRAM_ID);
        let quote_ata = keys::ata(&u, &l.quote_mint, &l.quote_token_program);
        let (source, destination) = if is_buy { (quote_ata, base_ata) } else { (base_ata, quote_ata) };
        let a = SwapAccounts { user: u, token_transfer_authority: u, source_token_account: source, destination_token_account: destination };
        let metas = full_swap_metas(&to_core(&self.key), &l.base_mint, &l.quote_mint, &l.quote_token_program, quote_hooked, &a, is_buy);
        Ok(Instruction {
            program_id: program_id(),
            accounts: metas.iter().map(|m| AccountMeta { pubkey: to_titan(&m.pubkey), is_signer: m.is_signer, is_writable: m.is_writable }).collect(),
            data: instruction_data(is_buy, request.amount, 0).to_vec(),
        })
    }
}

#[async_trait]
impl AddressLookupTableTrait for FloorVenue {
    async fn get_lookup_table_keys(&self, _accounts_cache: Option<&dyn AccountsCache>) -> Result<Vec<Pubkey>, TradingVenueError> {
        let l = &self.launch;
        let key = to_core(&self.key);
        let mut v = vec![
            keys::PROGRAM_ID,
            keys::event_authority_pda(),
            key,
            l.base_mint,
            l.quote_mint,
            keys::ata(&key, &l.base_mint, &keys::TOKEN_2022_PROGRAM_ID),
            keys::ata(&key, &l.quote_mint, &l.quote_token_program),
            keys::TOKEN_2022_PROGRAM_ID,
            l.quote_token_program,
            keys::SYSTEM_PROGRAM_ID,
            keys::ASSOCIATED_TOKEN_PROGRAM_ID,
            FLOOR_HOOK_ID,
            extra_metas_pda(&l.base_mint),
            holder_book_pda(&key),
            holder_position_pda(&key, &key),
        ];
        v.dedup();
        Ok(v.iter().map(to_titan).collect())
    }
}

pub type YourVenue = FloorVenue;
