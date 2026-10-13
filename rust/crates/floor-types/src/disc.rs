//! Anchor discriminators, as bytes, so a CPI never needs the callee's crate.
//! GENERATED once from the instruction and account names (sha256 prefix, the Anchor default);
//! each is checked against the built IDLs, so a rename cannot drift.
//!
//! The AMM's instructions ([`amm`]), the hook CALLBACKS ([`callback`]) and the AMM's accounts are
//! `carpenter_types::disc`, re-exported here; what is DEFINED here is `floor_hook`'s and
//! `floor_launch`'s own instructions and accounts.

pub use carpenter_types::disc::{amm, callback};

/// `floor_hook` instructions other than the callbacks.
pub mod hook {
    /// `sha256("global:initialize_hook_config")[..8]`
    pub const INITIALIZE_HOOK_CONFIG: [u8; 8] = [0x90, 0xef, 0x11, 0x55, 0xe4, 0x30, 0x36, 0x2b];
    /// `sha256("global:register_pool")[..8]`
    pub const REGISTER_POOL: [u8; 8] = [0x55, 0xe5, 0x72, 0x2f, 0x4b, 0x91, 0xa6, 0x64];
    /// `sha256("global:seed_floor")[..8]`
    pub const SEED_FLOOR: [u8; 8] = [0x65, 0x7e, 0xc0, 0xc3, 0xf8, 0x09, 0x8e, 0x4a];
    /// `sha256("global:seed_liquidity")[..8]`
    pub const SEED_LIQUIDITY: [u8; 8] = [0xb4, 0x39, 0x5e, 0x23, 0x49, 0x30, 0x0d, 0x0b];
    /// `sha256("global:seed_inventory")[..8]`
    pub const SEED_INVENTORY: [u8; 8] = [0x57, 0x47, 0xcc, 0x23, 0x51, 0x28, 0x0c, 0x6a];
    /// `sha256("global:ratchet")[..8]`
    pub const RATCHET: [u8; 8] = [0xdd, 0xd6, 0xcd, 0x6e, 0x0c, 0xaa, 0x18, 0xad];
    /// `sha256("global:sweep_stair_fees")[..8]` — the steps' fee sweep.
    pub const SWEEP_STAIR_FEES: [u8; 8] = [0x26, 0x17, 0xbc, 0xc2, 0x2f, 0x25, 0x71, 0xf8];
    /// `sha256("global:reclaim_stair_array")[..8]` — the staircase rent reserve.
    pub const RECLAIM_STAIR_ARRAY: [u8; 8] = [0x1f, 0x29, 0x02, 0xa6, 0x5f, 0x63, 0xa2, 0xce];
    /// `sha256("global:claim_creator_fees")[..8]`
    pub const CLAIM_CREATOR_FEES: [u8; 8] = [0x00, 0x17, 0x7d, 0xea, 0x9c, 0x76, 0x86, 0x59];
    /// `sha256("global:sweep_platform_cut")[..8]`
    pub const SWEEP_PLATFORM_CUT: [u8; 8] = [0x03, 0xdf, 0x23, 0xc9, 0x4a, 0xf3, 0x13, 0x84];
    /// `sha256("global:resolve_recipient")[..8]`
    pub const RESOLVE_RECIPIENT: [u8; 8] = [0x6e, 0x91, 0x49, 0x86, 0xbc, 0xdc, 0xa8, 0x63];
    /// `sha256("global:prepare_positions")[..8]`
    pub const PREPARE_POSITIONS: [u8; 8] = [0x4b, 0x1b, 0xbc, 0x6d, 0x9c, 0x2e, 0x20, 0x11];
    /// `sha256("global:claim_referral_fees")[..8]` (moves a pool's accrued `referral_owed` to the pool's own
    /// `referrer` WALLET, permissionless, the `claim_creator_fees` shape)
    pub const CLAIM_REFERRAL_FEES: [u8; 8] = [0xd0, 0xd8, 0x89, 0x4e, 0x24, 0x67, 0xa2, 0x31];
    /// `sha256("global:list_quote")[..8]` (governance admits a quote mint to the FLOOR roster, with that
    /// quote’s own economics)
    pub const LIST_QUOTE: [u8; 8] = [0x71, 0x05, 0xb8, 0xc4, 0xcc, 0xb9, 0x69, 0x89];
    /// `sha256("global:set_quote_economics")[..8]` — retunes `QuotePolicy.min_bid` / `.max_bounty`, which
    /// `floor_launch::create_launch` reads for a launch on a listed quote. The per-quote twin of
    /// `set_launch_economics`; see `floor_types::layout::QuotePolicy`.
    pub const SET_QUOTE_ECONOMICS: [u8; 8] = [0x54, 0x54, 0x8d, 0x84, 0x7b, 0xb4, 0x20, 0xcf];
    /// `sha256("global:retire_quote")[..8]`
    pub const RETIRE_QUOTE: [u8; 8] = [0xae, 0x9e, 0xa8, 0x70, 0x8e, 0xdc, 0x60, 0x64];
    /// `sha256("global:pay_holder")[..8]` (holder rewards): pays
    /// `amount` of the pool's `holder_owed` to a quote account, signed by `floor_holders`'
    /// `["holders_authority"]` PDA — the ONE exit holder tax has. `floor_holders::claim` CPIs it.
    pub const PAY_HOLDER: [u8; 8] = [0xc8, 0x0a, 0x5d, 0xc0, 0x50, 0x94, 0x08, 0x4b];
    /// `sha256("global:observe_quote")[..8]` (permissionless; reads the pool's quote vault and mint and records
    /// `FloorPool.quote_suspended`)
    pub const OBSERVE_QUOTE: [u8; 8] = [0x47, 0xf1, 0x56, 0x74, 0xbc, 0x47, 0x86, 0x1e];
    // ---- the PROGRAM-OWNED PLATFORM TREASURY
    /// `sha256("global:withdraw_platform")[..8]` — the treasury's governance moves ANY amount of any
    /// token account the treasury PDA owns, up to its real balance (uncapped)
    pub const WITHDRAW_PLATFORM: [u8; 8] = [0x57, 0xed, 0x1f, 0xd4, 0xf1, 0xac, 0x63, 0x21];
    /// `sha256("global:set_treasury_governance")[..8]` — step one of the two-step handover
    pub const SET_TREASURY_GOVERNANCE: [u8; 8] = [0x8e, 0x90, 0xa1, 0xe6, 0x6b, 0x0d, 0x86, 0x32];
    /// `sha256("global:accept_treasury_governance")[..8]` — step two: the pending key signs
    pub const ACCEPT_TREASURY_GOVERNANCE: [u8; 8] = [0x6c, 0xf8, 0xa8, 0x36, 0x28, 0xfb, 0x99, 0xe6];
    // ---- the program-owned TEAM treasury (owed -> swept -> withdrawn, uncapped) — `PlatformTreasury`'s twin under
    //      ["team_treasury"]
    /// `sha256("global:initialize_team_treasury")[..8]` — ONCE, signed by `HookConfig.governance`,
    /// which NAMES the team governance
    pub const INITIALIZE_TEAM_TREASURY: [u8; 8] = [0xb5, 0x26, 0x6b, 0x95, 0xac, 0x7b, 0xf6, 0xa1];
    /// `sha256("global:withdraw_team")[..8]` — the team governance moves any amount of any token
    /// account the team PDA owns, up to its real balance
    pub const WITHDRAW_TEAM: [u8; 8] = [0x84, 0x9c, 0x8a, 0xfa, 0x20, 0x0f, 0x65, 0x23];
    /// `sha256("global:set_team_treasury_governance")[..8]` — step one of the team handover
    pub const SET_TEAM_TREASURY_GOVERNANCE: [u8; 8] = [0x04, 0x39, 0x12, 0x1d, 0x42, 0x31, 0xec, 0xc4];
    /// `sha256("global:accept_team_treasury_governance")[..8]` — step two: the pending key signs
    pub const ACCEPT_TEAM_TREASURY_GOVERNANCE: [u8; 8] = [0x33, 0x41, 0xaf, 0xb5, 0xb8, 0x3b, 0xd3, 0xa0];
    // ---- `HookConfig.governance`'s two-step handover ----
    /// `sha256("global:set_hook_governance")[..8]` — step one: the current governance names a candidate
    pub const SET_HOOK_GOVERNANCE: [u8; 8] = [0x24, 0xb4, 0xe0, 0xc0, 0x8c, 0xde, 0x3e, 0x29];
    /// `sha256("global:accept_hook_governance")[..8]` — step two: the candidate signs
    pub const ACCEPT_HOOK_GOVERNANCE: [u8; 8] = [0x44, 0x27, 0x63, 0x6a, 0x6d, 0xa6, 0x43, 0x90];
    // ---- the pool-price TWAP ----
    /// `sha256("global:twap_tick")[..8]` — the READ: `twap_tick(window: u32) -> i32` as return data
    pub const TWAP_TICK: [u8; 8] = [0x6c, 0x53, 0xf6, 0x14, 0x0e, 0x51, 0xf1, 0xbd];
    /// Team steps and the team buy: the owner books' three instructions.
    pub const SWEEP_STAIR_PAYEE: [u8; 8] = [0xa3, 0x0b, 0x7e, 0x2b, 0x8c, 0xad, 0xd9, 0x50];
    pub const WITHDRAW_STAIR_TOKENS: [u8; 8] = [0x8a, 0xd5, 0x50, 0xd0, 0xc1, 0x62, 0xca, 0x29];
    pub const RESOLVE_STAIR_PAYEE: [u8; 8] = [0x31, 0x5b, 0xb8, 0xdc, 0xbf, 0x6f, 0x05, 0x1d];
    /// `sha256("global:clear_team_buy_books")[..8]` — the team buy's sell-back: CPI-only, floor_launch's
    /// `team_buy_sell_back` (signed by its `launch_authority`) turns an UNPLACED team-buy pool back into a plain
    /// staircase pool.
    pub const CLEAR_TEAM_BUY_BOOKS: [u8; 8] = [0xb2, 0xe8, 0x40, 0xcd, 0x4d, 0x2a, 0x4f, 0xc2];
    // ---- `$FLOOR`'s pool: other wSOL pools' cut feeds it ----
    /// `sha256("global:designate_floor_pool")[..8]` — ONCE: floor_launch's genesis path (signed by
    /// its `launch_authority` PDA) records `$FLOOR`'s pool and zeroes that pool's own cut
    pub const DESIGNATE_FLOOR_POOL: [u8; 8] = [0x8a, 0x52, 0x72, 0xb7, 0x44, 0xae, 0xa5, 0xfc];
    /// `sha256("global:credit_floor_pool")[..8]` — permissionless: a wSOL pool's `platform_owed`
    /// into `$FLOOR`'s backing (the EVM's in-intake `PlatformPoolCredit`, delivered by sweep)
    pub const CREDIT_FLOOR_POOL: [u8; 8] = [0xa1, 0x3a, 0x0a, 0x82, 0x10, 0x74, 0xc9, 0xac];
    // ---- $FLOOR's Solana genesis ----
    /// `sha256("global:donate_inventory")[..8]` — the EVM's `donateInventory`: the pool's own
    /// initializer (at genesis, the `$FLOOR` Launch PDA) adds base tokens to `inventory_tokens`
    /// without touching the ladder or its cost basis. CPI'd by `floor_launch::flush_allocation_dust`.
    pub const DONATE_INVENTORY: [u8; 8] = [0x01, 0xa3, 0xb6, 0x51, 0x71, 0x01, 0xb5, 0x34];
    /// `sha256("global:set_genesis_stair")[..8]` — ONCE, before graduation, `floor_launch::end_genesis_sale` (signed
    /// by its `launch_authority` PDA) turns `$FLOOR`'s designated pool into a staircase pool — its step count, growth
    /// and parameter set (`stair_kind`)
    pub const SET_GENESIS_STAIR: [u8; 8] = [0xc2, 0x07, 0xdb, 0x84, 0xf5, 0x65, 0xfd, 0x97];
    /// `sha256("global:set_allow_all_quotes")[..8]` — the platform treasury's governance flips
    /// `PlatformTreasury.allow_all_quotes`.
    pub const SET_ALLOW_ALL_QUOTES: [u8; 8] = [0x66, 0x7f, 0x80, 0x48, 0x18, 0xb0, 0xd2, 0x21];
    // ---- trader referrals ----
    /// `sha256("global:open_trader_bucket")[..8]` — permissionless; opens `["trader_ref", trader, quote_mint]` from the
    /// trader's `TraderLink` (`floor_launch::link_trader` CPIs it for wSOL, D4).
    pub const OPEN_TRADER_BUCKET: [u8; 8] = [0x2a, 0x6c, 0xb2, 0xe5, 0x6e, 0x99, 0x12, 0xa9];
    /// `sha256("global:credit_trader_bucket")[..8]` — CPI-only, signed by `floor_launch`'s `launch_authority`.
    pub const CREDIT_TRADER_BUCKET: [u8; 8] = [0xd9, 0x5a, 0x14, 0x03, 0x64, 0x8f, 0xff, 0xa0];
    /// `sha256("global:open_trader_ref_vault")[..8]` — permissionless; the vault ATA of `["trader_ref_vault"]` per quote.
    pub const OPEN_TRADER_REF_VAULT: [u8; 8] = [0xf3, 0x9b, 0x65, 0xd2, 0x74, 0x3c, 0xfb, 0x2a];
    /// `sha256("global:sweep_trader_referral")[..8]` — permissionless, fail-soft: a pool's `trader_ref_owed` into the vault.
    pub const SWEEP_TRADER_REFERRAL: [u8; 8] = [0x33, 0x70, 0x18, 0xfd, 0xc4, 0x7a, 0xa4, 0xa7];
    /// `sha256("global:claim_trader_referral")[..8]` — permissionless; sweeps, then pays a referrer from its buckets.
    pub const CLAIM_TRADER_REFERRAL: [u8; 8] = [0x5e, 0x42, 0x25, 0xf9, 0x05, 0xb7, 0xb6, 0x9f];
}

/// `floor_vesting` instructions another FLOOR program CALLS (bytes, never the crate):
/// `floor_launch::lock_genesis` CPIs [`CREATE_SCHEDULE_AT`] — the EVM's `FloorVesting.lockAt` — signing as the
/// `$FLOOR` Launch PDA. It is the ONE caller of `floor_vesting` from another program.
pub mod vesting {
    /// `sha256("global:create_schedule_at")[..8]`
    pub const CREATE_SCHEDULE_AT: [u8; 8] = [0xa2, 0x16, 0xe3, 0x6b, 0x4a, 0x90, 0x07, 0x05];
}

/// `floor_holders` instructions (holder rewards), the ones another FLOOR program or a client CALLS (a
/// discriminator, never the crate). `floor_launch::create_launch` CPIs [`INIT_LAUNCH`] and `sell_back`
/// CPIs [`SYNC`]; every other entry is a client's. Each is pinned to the built `floor_holders` IDL.
pub mod holders {
    /// `sha256("global:initialize_claims_registry")[..8]`
    pub const INITIALIZE_CLAIMS_REGISTRY: [u8; 8] = [0x64, 0x95, 0x78, 0x79, 0xae, 0xc4, 0x1f, 0x1f];
    /// `sha256("global:rotate_claims_table")[..8]`
    pub const ROTATE_CLAIMS_TABLE: [u8; 8] = [0x4e, 0xfe, 0x77, 0xce, 0x26, 0x15, 0x3f, 0xde];
    /// `sha256("global:init_launch")[..8]` — CPI'd by `floor_launch::create_launch`, signed by its
    /// `["lock_authority"]` PDA.
    pub const INIT_LAUNCH: [u8; 8] = [0x4b, 0xa2, 0x1f, 0xc6, 0xc0, 0x6d, 0x24, 0xa9];
    /// `sha256("global:open_record")[..8]`
    pub const OPEN_RECORD: [u8; 8] = [0x09, 0xf5, 0xf4, 0x74, 0x6e, 0xd8, 0x36, 0x9e];
    /// `sha256("global:touch")[..8]` — open if missing, else sync.
    pub const TOUCH: [u8; 8] = [0x50, 0x9d, 0x6a, 0xdc, 0xb5, 0x48, 0x65, 0x51];
    /// `sha256("global:sync")[..8]` — permissionless; CPI'd by `floor_launch::sell_back`.
    pub const SYNC: [u8; 8] = [0x04, 0xdb, 0x28, 0xa4, 0x15, 0x9d, 0xbd, 0x58];
    /// `sha256("global:claim")[..8]`
    pub const CLAIM: [u8; 8] = [0x3e, 0xc6, 0xd6, 0xc1, 0xd5, 0x9f, 0x6c, 0xd2];
    /// `sha256("global:close_record")[..8]`
    pub const CLOSE_RECORD: [u8; 8] = [0x6f, 0xc0, 0x7a, 0xbc, 0x26, 0xea, 0xf2, 0xf9];
    /// `sha256("spl-transfer-hook-interface:execute")[..8]` — the Token-2022 transfer-hook
    /// interface's `Execute`. ⛔ NOT an Anchor `global:` hash: Token-2022 sends this exact prefix.
    pub const EXECUTE: [u8; 8] = [0x69, 0x25, 0x65, 0xc5, 0x4b, 0xfb, 0x66, 0x1a];
    /// `sha256("spl-transfer-hook-interface:initialize-extra-account-metas")[..8]` — the interface's
    /// own OPTIONAL `InitializeExtraAccountMetaList` instruction, recorded so nobody spells it by
    /// hand. ⛔ `floor_holders` does not expose it (`init_launch` writes the list), and the list's TLV
    /// entry is tagged with [`EXECUTE`]'s bytes, not these.
    pub const INITIALIZE_EXTRA_ACCOUNT_METAS: [u8; 8] = [0x2b, 0x22, 0x0d, 0x31, 0xa7, 0x58, 0xeb, 0xeb];
}

/// `floor_launch` instructions.
pub mod launch {
    /// `sha256("global:initialize_launch_config")[..8]`
    pub const INITIALIZE_LAUNCH_CONFIG: [u8; 8] = [0x6a, 0xe7, 0x23, 0x0f, 0x19, 0x68, 0xa0, 0x99];
    /// `sha256("global:set_attestor")[..8]`
    pub const SET_ATTESTOR: [u8; 8] = [0x5f, 0x0b, 0xec, 0x9d, 0xea, 0x92, 0xa3, 0xed];
    /// `sha256("global:set_governance")[..8]`
    pub const SET_GOVERNANCE: [u8; 8] = [0x22, 0x47, 0x80, 0xf5, 0xb3, 0x2a, 0x8c, 0x89];
    /// `sha256("global:accept_governance")[..8]`
    pub const ACCEPT_GOVERNANCE: [u8; 8] = [0xf8, 0x7c, 0x24, 0xd3, 0xe0, 0xbf, 0x4f, 0xe3];
    /// `sha256("global:create_launch")[..8]`
    pub const CREATE_LAUNCH: [u8; 8] = [0xef, 0xdf, 0xff, 0x86, 0x27, 0x79, 0x7f, 0x3e];
    /// `sha256("global:buy")[..8]`
    pub const BUY: [u8; 8] = [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea];
    /// `sha256("global:sell_back")[..8]`
    pub const SELL_BACK: [u8; 8] = [0xe6, 0x32, 0x08, 0x06, 0xd2, 0x0f, 0x98, 0x85];
    /// `sha256("global:sell_back_wsol")[..8]` — `sell_back` with the
    /// payout as wSOL into the seller's native SPL Token account (the Jupiter-routable sell-back).
    pub const SELL_BACK_WSOL: [u8; 8] = [0x8c, 0xc1, 0x20, 0x8a, 0x4c, 0x2d, 0xdc, 0xbe];
    /// `sha256("global:close_buyer_position")[..8]`
    pub const CLOSE_BUYER_POSITION: [u8; 8] = [0xef, 0x79, 0xdd, 0xe1, 0xcc, 0x60, 0xab, 0xd4];
    /// `sha256("global:claim_recipient")[..8]`
    pub const CLAIM_RECIPIENT: [u8; 8] = [0xdb, 0xb5, 0xf3, 0xf6, 0x36, 0x55, 0x39, 0xc3];
    /// `sha256("global:graduate")[..8]`
    pub const GRADUATE: [u8; 8] = [0x2d, 0xeb, 0xe1, 0xb5, 0x11, 0xda, 0x40, 0x82];
    /// `sha256("global:set_metadata_base")[..8]`
    pub const SET_METADATA_BASE: [u8; 8] = [0x22, 0x8d, 0x18, 0xbf, 0xfa, 0x42, 0x37, 0x9a];
    /// `sha256("global:fill_metadata_uri")[..8]`
    pub const FILL_METADATA_URI: [u8; 8] = [0x03, 0x16, 0x38, 0x62, 0x5e, 0xc0, 0x27, 0xb0];
    /// `sha256("global:set_launch_economics")[..8]`
    pub const SET_LAUNCH_ECONOMICS: [u8; 8] = [0xcb, 0x26, 0x0c, 0xea, 0x7a, 0x36, 0xf9, 0x09];
    /// `sha256("global:close_graduation_rent")[..8]`
    pub const CLOSE_GRADUATION_RENT: [u8; 8] = [0x01, 0x5a, 0x0c, 0x7b, 0x94, 0xdf, 0x84, 0x3f];
    /// `sha256("global:fund_stair_rent")[..8]` — the staircase rent reserve.
    pub const FUND_STAIR_RENT: [u8; 8] = [0x9f, 0x87, 0xc9, 0xd7, 0x7a, 0xc4, 0xd9, 0x34];
    /// `sha256("global:close_prepared_tick_array")[..8]` — the reserve owns the prepared tick arrays until
    /// graduation; after it the creator closes an empty one through this.
    pub const CLOSE_PREPARED_TICK_ARRAY: [u8; 8] = [0x73, 0x65, 0xd7, 0x5b, 0x35, 0x3e, 0x00, 0x24];
    /// `sha256("global:prepare_graduation")[..8]`
    pub const PREPARE_GRADUATION: [u8; 8] = [0x45, 0xbc, 0xed, 0x6d, 0x58, 0xda, 0x9f, 0x73];
    /// `sha256("global:set_team_fees")[..8]`: the FLOOR Platform fee's current setting, sale and pool, signed by the
    /// TEAM treasury's governance only
    pub const SET_TEAM_FEES: [u8; 8] = [0xb4, 0xf9, 0x40, 0x08, 0xc3, 0x06, 0xde, 0xb3];
    // ---- $FLOOR's Solana genesis — the timed pro-rata raise, `FLOOR_TIMED_V1` ----
    /// `sha256("global:init_genesis")[..8]`
    pub const INIT_GENESIS: [u8; 8] = [0x35, 0x99, 0x1b, 0x88, 0x26, 0xa7, 0x49, 0x19];
    /// `sha256("global:set_genesis_schedule")[..8]`
    pub const SET_GENESIS_SCHEDULE: [u8; 8] = [0x21, 0x03, 0xc1, 0xb8, 0x97, 0xe4, 0xe2, 0xbd];
    /// `sha256("global:set_genesis_floor_share")[..8]`
    pub const SET_GENESIS_FLOOR_SHARE: [u8; 8] = [0x8c, 0x8c, 0x5f, 0xdc, 0x04, 0x98, 0x13, 0xc4];
    /// `sha256("global:contribute")[..8]`
    pub const CONTRIBUTE: [u8; 8] = [0x52, 0x21, 0x44, 0x83, 0x20, 0x00, 0xcd, 0x5f];
    /// `sha256("global:end_genesis_sale")[..8]`
    pub const END_GENESIS_SALE: [u8; 8] = [0x23, 0x45, 0xe6, 0x3f, 0xd1, 0xc7, 0xf2, 0xc9];
    /// `sha256("global:graduate_genesis")[..8]`
    pub const GRADUATE_GENESIS: [u8; 8] = [0x1a, 0x9b, 0x54, 0xf9, 0x5f, 0xbd, 0x49, 0xd8];
    /// `sha256("global:lock_genesis")[..8]`
    pub const LOCK_GENESIS: [u8; 8] = [0x24, 0x89, 0x4e, 0xae, 0x8e, 0x29, 0xf6, 0xc4];
    /// `sha256("global:claim_genesis")[..8]`
    pub const CLAIM_GENESIS: [u8; 8] = [0x28, 0x51, 0xb2, 0x63, 0xbe, 0x9a, 0xd0, 0xfe];
    /// `sha256("global:flush_floor_fees")[..8]`
    pub const FLUSH_FLOOR_FEES: [u8; 8] = [0x99, 0xb8, 0xcb, 0x3a, 0x7c, 0x36, 0xa0, 0x23];
    /// `sha256("global:flush_allocation_dust")[..8]`
    pub const FLUSH_ALLOCATION_DUST: [u8; 8] = [0x3d, 0xb9, 0xe5, 0x2e, 0x93, 0xbc, 0x95, 0x20];
    /// `sha256("global:transfer_genesis_authority")[..8]` — the EVM's `Ownable2Step.transferOwnership` for
    /// `GenesisSale.authority`.
    pub const TRANSFER_GENESIS_AUTHORITY: [u8; 8] = [0x91, 0xa7, 0xa0, 0x3f, 0x42, 0x6c, 0xeb, 0x5e];
    /// `sha256("global:accept_genesis_authority")[..8]` — the EVM's `acceptOwnership`.
    pub const ACCEPT_GENESIS_AUTHORITY: [u8; 8] = [0xcc, 0xa0, 0x75, 0xfe, 0x7b, 0xf1, 0xec, 0xb4];
    /// `sha256("global:set_genesis_stair_top")[..8]` — the staircase's top as a market cap in quote units, by the
    /// genesis authority, frozen once opened.
    pub const SET_GENESIS_STAIR_TOP: [u8; 8] = [0xe7, 0x25, 0x81, 0x07, 0xdb, 0x13, 0x6a, 0xf5];
    /// The team buy: the wallet part's cliffs, and the steps' payee claim.
    pub const CLAIM_TEAM_VESTED: [u8; 8] = [0xe4, 0x1b, 0xf0, 0x7e, 0x26, 0x49, 0x0e, 0x67];
    pub const CLAIM_TEAM_PAYEE: [u8; 8] = [0x6d, 0xeb, 0x4b, 0x84, 0x35, 0x3f, 0x7e, 0x37];
    /// Team steps: `$FLOOR`'s team steps' payee claim.
    pub const CLAIM_GENESIS_PAYEE: [u8; 8] = [0xb0, 0x04, 0x57, 0xe7, 0xa1, 0xd8, 0x58, 0xaa];
    /// `sha256("global:team_buy_sell_back")[..8]` — the team buy's sell-back: the deployer undoes the WHOLE team buy
    /// before graduation, as on EVM (`FloorLaunch.teamBuySellBack`).
    pub const TEAM_BUY_SELL_BACK: [u8; 8] = [0x09, 0xf1, 0x5b, 0x33, 0xd1, 0x93, 0x28, 0x3d];
    /// `sha256("global:set_genesis_whitelist")[..8]` — the genesis whitelist's Merkle root, by the genesis authority,
    /// frozen once opened.
    pub const SET_GENESIS_WHITELIST: [u8; 8] = [0xab, 0xba, 0x7f, 0xa2, 0x9f, 0xd8, 0x80, 0xba];
    /// `sha256("global:refund_genesis")[..8]` — the refund: a contributor's SOL back, 14 days after the authority
    /// closed a raise that never graduated.
    pub const REFUND_GENESIS: [u8; 8] = [0xaf, 0xfb, 0xb1, 0x91, 0xdc, 0x0b, 0xde, 0x43];
    /// `sha256("global:walk_quote_chain")[..8]` — the chained quote: advances a `QuoteWalk` by the hops its accounts
    /// carry.
    pub const WALK_QUOTE_CHAIN: [u8; 8] = [0x94, 0x30, 0xf0, 0x33, 0xeb, 0x2b, 0x52, 0x12];
    /// `sha256("global:close_quote_walk")[..8]` — the payer abandons its `QuoteWalk` and takes the rent back.
    pub const CLOSE_QUOTE_WALK: [u8; 8] = [0x90, 0xda, 0xe2, 0xa5, 0x86, 0xc4, 0x75, 0x73];
    // ---- the preset registry ----
    /// `sha256("global:init_preset_registry")[..8]` — once, by `LaunchConfig.governance`; seeds ids 1–3.
    pub const INIT_PRESET_REGISTRY: [u8; 8] = [0x41, 0x17, 0xf7, 0x6f, 0x72, 0x31, 0x86, 0x6b];
    /// `sha256("global:add_preset")[..8]` — the registry's authority appends one row.
    pub const ADD_PRESET: [u8; 8] = [0x97, 0x01, 0x51, 0xbb, 0x50, 0x24, 0x04, 0xcb];
    /// `sha256("global:set_preset_enabled")[..8]`
    pub const SET_PRESET_ENABLED: [u8; 8] = [0xa7, 0x7a, 0xee, 0xb9, 0x5c, 0x3b, 0x55, 0x68];
    /// `sha256("global:set_preset_authority")[..8]` — the two-step handover's first step.
    pub const SET_PRESET_AUTHORITY: [u8; 8] = [0x55, 0xe4, 0x47, 0xf0, 0xe2, 0xc0, 0x8f, 0x76];
    /// `sha256("global:accept_preset_authority")[..8]`
    pub const ACCEPT_PRESET_AUTHORITY: [u8; 8] = [0x66, 0xfe, 0xbc, 0x55, 0x1e, 0xbd, 0xc0, 0xcf];
    /// `sha256("global:link_trader")[..8]` — trader referrals: the attestor-signed link, written once per trader.
    pub const LINK_TRADER: [u8; 8] = [0xfb, 0x3f, 0x3d, 0x38, 0xa7, 0xc7, 0x3a, 0xc1];
    /// `sha256("global:sweep_sale_trader_referral")[..8]` — a launch's sale referral account into the vault.
    pub const SWEEP_SALE_TRADER_REFERRAL: [u8; 8] = [0x42, 0x23, 0x40, 0x83, 0x0f, 0x21, 0x8a, 0x5c];
    /// `sha256("global:open_sale_trader_referral")[..8]` — opens a launch's sale referral account after the create.
    pub const OPEN_SALE_TRADER_REFERRAL: [u8; 8] = [0x40, 0xb4, 0x6b, 0x37, 0xe9, 0x6c, 0x6a, 0x4b];
}

/// Account discriminators, `sha256("account:<Name>")[..8]` — the AMM's through the re-export, FLOOR's here.
pub mod account {
    pub use carpenter_types::disc::account::*;

    /// `HookConfig` (`floor_hook`)
    pub const HOOK_CONFIG: [u8; 8] = [0x89, 0x9b, 0x65, 0x5f, 0x8a, 0x48, 0x08, 0xb6];
    /// `FloorPool` (`floor_hook`)
    pub const FLOOR_POOL: [u8; 8] = [0x73, 0xeb, 0xd8, 0x17, 0xd1, 0x71, 0x95, 0xc4];
    /// `QuotePolicy` (`floor_hook`) — the quote roster
    pub const QUOTE_POLICY: [u8; 8] = [0x01, 0x2b, 0x1a, 0x4f, 0x56, 0x27, 0xab, 0x43];
    /// `HolderBook` (`floor_holders`) — holder rewards
    pub const HOLDER_BOOK: [u8; 8] = [0x1d, 0x5d, 0xd0, 0x72, 0x4a, 0xee, 0x06, 0xf6];
    /// `HolderRecord` (`floor_holders`)
    pub const HOLDER_RECORD: [u8; 8] = [0xd7, 0x43, 0x1f, 0x29, 0x9b, 0xfb, 0xb2, 0x80];
    /// `ClaimsRegistry` (`floor_holders`)
    pub const CLAIMS_REGISTRY: [u8; 8] = [0x01, 0x31, 0xcd, 0x88, 0xed, 0xee, 0xe1, 0x79];
    /// `PlatformTreasury` (`floor_hook`) — the program-owned platform treasury
    pub const PLATFORM_TREASURY: [u8; 8] = [0x23, 0xe5, 0x0f, 0xdd, 0xd7, 0xae, 0xa7, 0x5c];
    /// `FloorPlatformPool` (`floor_hook`) — `$FLOOR`'s pool record
    pub const FLOOR_PLATFORM_POOL: [u8; 8] = [0xa7, 0xd0, 0xa8, 0x21, 0x89, 0x23, 0x05, 0x1f];
    /// `StairState` (`floor_hook`) — a staircase pool's steps, tail, cushion and re-list
    pub const STAIR_STATE: [u8; 8] = [0xb8, 0x99, 0xc1, 0xde, 0x1a, 0xed, 0xf9, 0x72];
    /// `LaunchConfig` (`floor_launch`)
    pub const LAUNCH_CONFIG: [u8; 8] = [0x12, 0xa1, 0x09, 0xe0, 0x66, 0x91, 0x1d, 0x5e];
    /// `RegistryEntry` (`floor_launch`)
    pub const REGISTRY_ENTRY: [u8; 8] = [0x30, 0xc6, 0xf0, 0xfc, 0x9b, 0xba, 0x48, 0x10];
    /// `Launch` (`floor_launch`)
    pub const LAUNCH: [u8; 8] = [0x90, 0x33, 0x33, 0xa3, 0xce, 0x55, 0xd5, 0x26];
    /// `BuyerPosition` (`floor_launch`)
    pub const BUYER_POSITION: [u8; 8] = [0xe8, 0xa3, 0xa7, 0x5f, 0xaa, 0xd2, 0xd6, 0x53];
    /// `GenesisSale` (`floor_launch`) — `$FLOOR`'s Solana genesis
    pub const GENESIS_SALE: [u8; 8] = [0xf2, 0x7d, 0x74, 0xf0, 0xbe, 0x62, 0x28, 0xde];
    /// `GenesisContribution` (`floor_launch`)
    pub const GENESIS_CONTRIBUTION: [u8; 8] = [0x69, 0x1a, 0xb8, 0xf6, 0xbc, 0xfc, 0x03, 0xc9];
    /// `QuoteWalk` (`floor_launch`) — the chained-quote walk
    pub const QUOTE_WALK: [u8; 8] = [0x10, 0x28, 0xed, 0x09, 0xa2, 0xbd, 0x53, 0x9f];
    /// `PresetRegistry` (`floor_launch`) — the preset registry
    pub const PRESET_REGISTRY: [u8; 8] = [0x5a, 0x19, 0x0a, 0x47, 0x52, 0xd0, 0xf6, 0xd3];
    /// `TraderLink` (`floor_launch`) — trader referrals
    pub const TRADER_LINK: [u8; 8] = [0x47, 0xaa, 0x70, 0x4b, 0xf3, 0x50, 0xc0, 0xad];
    /// `TraderBucket` (`floor_hook`) — trader referrals
    pub const TRADER_BUCKET: [u8; 8] = [0xc4, 0xae, 0x45, 0x6d, 0xad, 0xb3, 0x77, 0xbb];
}
