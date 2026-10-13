//! [`FloorSaleAmm`]: Jupiter's `Amm` over one FLOOR launch's bonding sale.

use floor_quote::state::decode_launch;
use floor_quote::{sale_buy, sale_sell_back_stateless, QuoteError, SaleBuyQuote, SaleSellBackQuote};
use floor_types::constants::LAUNCH_STATE_BONDING;
use floor_types::ids::{cluster_by_launch_in, ClusterIds, CLUSTERS};
use floor_types::keys::WSOL_MINT;
use floor_types::layout::Launch;
use jupiter_amm_interface::{
    AccountProvider, Amm, AmmContext, AmmError, AmmLabel, KeyedAccount, Quote, QuoteParams, SwapAndAccountMetas, SwapMode, SwapParams,
};
use rust_decimal::Decimal;
use solana_account::ReadableAccount;
use solana_pubkey::Pubkey;

use crate::accounts::{buy_metas, launch_pda, launch_program, sell_back_wsol_metas, token_2022, token_2022_ata, wsol_ata, LaunchKeys};
use crate::encode::swap_variant;
use crate::{BUY_METAS, LABEL, SELL_METAS, SELL_METAS_HOOKED};

/// Refusal names. Each is the program's own error name where the program refuses the same thing,
/// otherwise a name for what this adapter does not route.
pub mod refusal {
    /// The launch has graduated (or is not a sale): the program refuses `buy` with `SaleOver`.
    pub const SALE_OVER: &str = "SaleOver";
    /// The amount buys no token: the program refuses with `Nothing`.
    pub const NOTHING: &str = "Nothing";
    /// The buy would sell the sale out, and the selling-out buy graduates the launch inside the same
    /// instruction; that needs the graduation accounts, which this adapter does not carry.
    pub const SALE_WOULD_GRADUATE: &str = "SaleWouldGraduate";
    /// No longer returned: the adapter quotes sell-backs at the stateless amount. Kept for callers
    /// that matched on it.
    pub const SELL_BACK_NOT_ROUTED: &str = "SellBackNotRouted";
    /// A sell of more tokens than the whole sale has sold: no position can cover it, so the program
    /// would refuse it (`InsufficientAllocation`).
    pub const EXCEEDS_SOLD: &str = "ExceedsSold";
    /// The source of a sell-back is not the user's Token-2022 associated token account for the mint:
    /// the program takes the tokens from the signer's ATA only.
    pub const SOURCE_NOT_USER_ATA: &str = "SourceNotUserAta";
    /// The destination of a sell-back is not the user's SPL Token wSOL associated token account: the
    /// program pays only into a native wSOL account the signer owns, and this adapter names its ATA.
    pub const DESTINATION_NOT_USER_WSOL_ATA: &str = "DestinationNotUserWsolAta";
    /// A sale quoted in a token other than wSOL: its buy also needs the buyer's quote token account
    /// and the whole graduation account list, and is not routed by this adapter.
    pub const TOKEN_QUOTE_NOT_ROUTED: &str = "TokenQuoteNotRouted";
    /// Exact-out: the program's `buy` takes a gross input cap only.
    pub const EXACT_OUT_NOT_SUPPORTED: &str = "ExactOutNotSupported";
    /// The destination is not the user's Token-2022 associated token account for the mint. The
    /// program delivers into the signer's ATA only; the mint is paused, so the tokens cannot be
    /// moved on to another account afterwards.
    pub const DESTINATION_NOT_USER_ATA: &str = "DestinationNotUserAta";
    /// The input's transfer authority is not the user. The buy is signed by the wallet that pays and
    /// receives; a program authority signing it would own the position and the tokens.
    pub const AUTHORITY_NOT_USER: &str = "AuthorityNotUser";
}

fn refuse(name: &str, detail: impl std::fmt::Display) -> AmmError {
    AmmError::Custom(format!("{name}: {detail}"))
}

/// One FLOOR launch's sale as a Jupiter market. The market key is the `Launch` account
/// (`["launch", mint]` under `floor_launch`). The cluster is the one whose `floor_launch` owns that
/// account; every program id and PDA a swap names comes from it.
#[derive(Clone)]
pub struct FloorSaleAmm {
    key: Pubkey,
    cluster: ClusterIds,
    launch: Launch,
    /// `true` once `update` has decoded the account at least once; `from_keyed_account` counts.
    loaded: bool,
}

impl FloorSaleAmm {
    /// The cluster this launch belongs to (looked up by the launch account's owner).
    pub fn cluster(&self) -> &ClusterIds {
        &self.cluster
    }

    /// [`Amm::from_keyed_account`] over a caller's cluster table instead of the shipped
    /// `floor_types::ids::CLUSTERS`. The launch account's owner selects the cluster; an owner that is
    /// `floor_launch` on no entry is refused.
    pub fn from_keyed_account_in(keyed_account: &KeyedAccount, clusters: &[ClusterIds]) -> Result<Self, AmmError> {
        let owner = keyed_account.account.owner;
        let cluster = *cluster_by_launch_in(clusters, &owner.to_bytes())
            .ok_or_else(|| AmmError::Custom(format!("{} is not owned by floor_launch on any known cluster (owner {owner})", keyed_account.key)))?;
        let launch = decode_launch(&keyed_account.account.data).map_err(|e| AmmError::Custom(e.to_string()))?;
        let expected = launch_pda(&cluster, &Pubkey::new_from_array(launch.mint));
        if expected != keyed_account.key {
            return Err(AmmError::Custom(format!("{} is not the launch PDA of its mint (expected {expected})", keyed_account.key)));
        }
        Ok(Self { key: keyed_account.key, cluster, launch, loaded: true })
    }

    /// The decoded `Launch`.
    pub fn launch(&self) -> &Launch {
        &self.launch
    }

    /// The launch token's mint.
    pub fn mint(&self) -> Pubkey {
        Pubkey::new_from_array(self.launch.mint)
    }

    /// The sale's quote mint (wSOL for every routed sale).
    pub fn quote_mint(&self) -> Pubkey {
        Pubkey::new_from_array(self.launch.quote_mint())
    }

    fn native_quote(&self) -> bool {
        self.launch.quote_mint() == WSOL_MINT
    }

    fn bonding(&self) -> bool {
        self.launch.state == LAUNCH_STATE_BONDING
    }

    /// Tokens still for sale (`sale_tokens − sold`).
    pub fn remaining(&self) -> u64 {
        self.launch.sale_tokens.saturating_sub(self.launch.sold)
    }

    /// The mint carries the holder-rewards transfer-hook slot: a holder rate on either side (the rule
    /// `create_launch` applies), so a sell-back carries the 4-account holder tail.
    pub fn hooked(&self) -> bool {
        self.launch.holder_buy > 0 || self.launch.holder_sell > 0
    }

    fn keys(&self) -> LaunchKeys {
        LaunchKeys {
            launch: self.key,
            mint: self.mint(),
            token_vault: Pubkey::new_from_array(self.launch.token_vault),
            floor_pool: Pubkey::new_from_array(self.launch.floor_pool),
            hooked: self.hooked(),
        }
    }

    /// The sale's answer for a gross input of `amount`, with this adapter's refusals applied.
    pub fn quote_buy(&self, amount: u64) -> Result<SaleBuyQuote, AmmError> {
        if !self.native_quote() {
            return Err(refuse(refusal::TOKEN_QUOTE_NOT_ROUTED, format!("the sale is quoted in {}", self.quote_mint())));
        }
        if !self.bonding() {
            return Err(refuse(refusal::SALE_OVER, format!("launch state {}", self.launch.state)));
        }
        let q = sale_buy(&self.launch, amount).map_err(|e: QuoteError| AmmError::Custom(e.to_string()))?;
        if q.graduates {
            return Err(refuse(
                refusal::SALE_WOULD_GRADUATE,
                format!("{amount} buys the last {} tokens of the sale; the graduating buy is not routed", q.tokens),
            ));
        }
        Ok(q)
    }

    /// The largest gross input this adapter quotes: the most that leaves at least one token unsold.
    /// `None` when not even that much can be bought (or the sale is not routed at all).
    pub fn max_buy_in(&self) -> Option<u64> {
        // `sale_buy` is monotone in the input; find the last amount that does not graduate.
        let ok = |a: u64| self.quote_buy(a).is_ok();
        let (mut lo, mut hi) = (1u64, u64::MAX);
        if !ok(lo) {
            // The smallest amounts buy nothing; walk up to the first that buys.
            let mut a = 1u64;
            loop {
                match self.quote_buy(a) {
                    Ok(_) => break,
                    Err(e) if e.to_string().starts_with(refusal::NOTHING) => a = a.checked_mul(2)?,
                    Err(_) => return None,
                }
            }
            lo = a;
        }
        if ok(hi) {
            return Some(hi);
        }
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2;
            if ok(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Some(lo)
    }

    /// The sale's answer for selling `tokens` back (the stateless rule), with this adapter's refusals
    /// applied. Exact for ANY seller whose position covers `tokens`; the quote cannot see a position,
    /// so a seller whose position does not cover `tokens` is refused by the program, never mispaid.
    pub fn quote_sell(&self, tokens: u64) -> Result<SaleSellBackQuote, AmmError> {
        if !self.native_quote() {
            return Err(refuse(refusal::TOKEN_QUOTE_NOT_ROUTED, format!("the sale is quoted in {}", self.quote_mint())));
        }
        if !self.bonding() {
            return Err(refuse(refusal::SALE_OVER, format!("launch state {}", self.launch.state)));
        }
        if tokens > self.launch.sold {
            return Err(refuse(refusal::EXCEEDS_SOLD, format!("{tokens} tokens, but the sale has sold {}", self.launch.sold)));
        }
        let q = sale_sell_back_stateless(&self.launch, tokens).map_err(|e: QuoteError| AmmError::Custom(e.to_string()))?;
        if q.received == 0 {
            return Err(refuse(refusal::NOTHING, format!("{tokens} tokens pay nothing at the sale price")));
        }
        Ok(q)
    }

    /// `true` for a buy (quote mint -> launch token), `false` for a sell-back.
    fn check_pair(&self, input: &Pubkey, output: &Pubkey) -> Result<bool, AmmError> {
        let (q, m) = (self.quote_mint(), self.mint());
        if *input == q && *output == m {
            Ok(true)
        } else if *input == m && *output == q {
            Ok(false)
        } else {
            Err(AmmError::Custom(format!("mints {input} -> {output} are not this sale's pair ({q} <-> {m})")))
        }
    }
}

impl Amm for FloorSaleAmm {
    /// The cluster is the one whose `floor_launch` owns the account (`floor_types::ids::CLUSTERS`);
    /// any other owner is refused.
    fn from_keyed_account(keyed_account: &KeyedAccount, _amm_context: &AmmContext) -> Result<Self, AmmError> {
        Self::from_keyed_account_in(keyed_account, CLUSTERS)
    }

    fn label(&self) -> AmmLabel {
        LABEL
    }

    fn program_id(&self) -> Pubkey {
        launch_program(&self.cluster)
    }

    fn key(&self) -> Pubkey {
        self.key
    }

    /// `[quote mint, launch token]`; both directions are quoted.
    fn get_reserve_mints(&self) -> Vec<Pubkey> {
        vec![self.quote_mint(), self.mint()]
    }

    /// The `Launch` account alone: the price, the sale size, what is sold and the fee rate are all in
    /// it.
    fn get_accounts_to_update(&self) -> Vec<Pubkey> {
        vec![self.key]
    }

    fn update(&mut self, account_provider: impl AccountProvider) -> Result<(), AmmError> {
        let account = account_provider.try_get(&self.key)?;
        if *account.owner() != launch_program(&self.cluster) {
            self.loaded = false;
            return Err(AmmError::Custom(format!("{} is no longer owned by floor_launch", self.key)));
        }
        match decode_launch(account.data()) {
            Ok(l) if l.mint == self.launch.mint => {
                self.launch = l;
                self.loaded = true;
                Ok(())
            }
            Ok(_) => {
                self.loaded = false;
                Err(AmmError::Custom("the launch account's mint changed".into()))
            }
            Err(e) => {
                self.loaded = false;
                Err(AmmError::Custom(e.to_string()))
            }
        }
    }

    /// Exact-in only. A buy: `in_amount` is what the program takes (`cost + fee`, at most `amount`;
    /// the difference is rounding dust under one token's price that stays with the wallet),
    /// `out_amount` the tokens delivered, `fee_amount` the sale's platform fee in the input mint. A
    /// sell-back: `in_amount` is every token offered, `out_amount` the wSOL paid
    /// (`payout − fee`), `fee_amount` the platform fee in the output mint (wSOL).
    fn quote(&self, quote_params: &QuoteParams) -> Result<Quote, AmmError> {
        if !self.loaded {
            return Err(AmmError::Custom("not updated".into()));
        }
        if quote_params.swap_mode != SwapMode::ExactIn {
            return Err(refuse(refusal::EXACT_OUT_NOT_SUPPORTED, "a sale buy takes a gross input cap; a sell-back takes its tokens"));
        }
        if self.check_pair(&quote_params.input_mint, &quote_params.output_mint)? {
            let q = self.quote_buy(quote_params.amount)?;
            let fee_pct = if q.spent == 0 { Decimal::ZERO } else { Decimal::from(q.team_fee) / Decimal::from(q.spent) };
            Ok(Quote { in_amount: q.spent, out_amount: q.tokens, fee_amount: q.team_fee, fee_mint: self.quote_mint(), fee_pct })
        } else {
            let q = self.quote_sell(quote_params.amount)?;
            let fee_pct = if q.payout == 0 { Decimal::ZERO } else { Decimal::from(q.team_fee) / Decimal::from(q.payout) };
            Ok(Quote { in_amount: quote_params.amount, out_amount: q.received, fee_amount: q.team_fee, fee_mint: self.quote_mint(), fee_pct })
        }
    }

    /// The accounts for `swap_params.user`. A buy: the `buy` accounts, refused unless the transfer
    /// authority is the user and the destination is the user's Token-2022 ATA for the mint (the program
    /// signs nothing else and delivers nowhere else). A sell-back: the `sell_back_wsol` accounts,
    /// refused unless the authority is the user, the source is the user's Token-2022 ATA (the program
    /// takes the tokens from the signer's ATA only) and the destination is the user's SPL Token wSOL
    /// ATA (the program pays only into a native wSOL account the signer owns).
    fn get_swap_and_account_metas(&self, swap_params: &SwapParams) -> Result<SwapAndAccountMetas, AmmError> {
        if swap_params.swap_mode != SwapMode::ExactIn {
            return Err(refuse(refusal::EXACT_OUT_NOT_SUPPORTED, "a sale buy takes a gross input cap; a sell-back takes its tokens"));
        }
        let is_buy = self.check_pair(&swap_params.source_mint, &swap_params.destination_mint)?;
        if !self.native_quote() {
            return Err(refuse(refusal::TOKEN_QUOTE_NOT_ROUTED, format!("the sale is quoted in {}", self.quote_mint())));
        }
        let user = swap_params.user;
        if swap_params.token_transfer_authority != user {
            return Err(refuse(refusal::AUTHORITY_NOT_USER, format!("authority {} is not the user {user}", swap_params.token_transfer_authority)));
        }
        let ata = token_2022_ata(&user, &self.mint());
        if is_buy {
            if swap_params.destination_token_account != ata {
                return Err(refuse(
                    refusal::DESTINATION_NOT_USER_ATA,
                    format!("destination {} is not {ata}, the user's Token-2022 ATA", swap_params.destination_token_account),
                ));
            }
            return Ok(SwapAndAccountMetas { swap: swap_variant(true), account_metas: buy_metas(&self.cluster, &self.keys(), &user) });
        }
        if swap_params.source_token_account != ata {
            return Err(refuse(
                refusal::SOURCE_NOT_USER_ATA,
                format!("source {} is not {ata}, the user's Token-2022 ATA", swap_params.source_token_account),
            ));
        }
        let wsol = wsol_ata(&user);
        if swap_params.destination_token_account != wsol {
            return Err(refuse(
                refusal::DESTINATION_NOT_USER_WSOL_ATA,
                format!("destination {} is not {wsol}, the user's wSOL ATA", swap_params.destination_token_account),
            ));
        }
        Ok(SwapAndAccountMetas { swap: swap_variant(false), account_metas: sell_back_wsol_metas(&self.cluster, &self.keys(), &user, &wsol) })
    }

    fn supports_exact_out(&self) -> bool {
        false
    }

    /// Both directions: quote mint → launch token (a buy) and back (a sell-back).
    fn unidirectional(&self) -> bool {
        false
    }

    /// The program's Token-2022 dependency: the deployed Token-2022 carries the `Pausable` extension
    /// every sale mint uses. The test kit loads it as `tests/fixtures/spl_token_2022.so`.
    fn program_dependencies(&self) -> Vec<(Pubkey, String)> {
        vec![(token_2022(), "spl_token_2022".to_string())]
    }

    /// The larger of the two sides: 13 for a buy, 13 (17 on a hooked launch) for a sell-back.
    fn get_accounts_len(&self) -> usize {
        BUY_METAS.max(if self.hooked() { SELL_METAS_HOOKED } else { SELL_METAS })
    }

    /// Active while the account decodes, the sale is bonding, it is quoted in wSOL, and at least one
    /// token can be bought without selling the sale out.
    fn is_active(&self) -> bool {
        self.loaded && self.bonding() && self.native_quote() && self.remaining() >= 2
    }
}
