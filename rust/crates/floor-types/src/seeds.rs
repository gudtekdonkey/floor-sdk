//! Every PDA seed prefix, and the pure helpers that build seed bytes.
//!
//! ⛔ A seed is a byte string, not a `str`: `b"pool"`, never `"pool".as_bytes()` in one place
//! and a differently spelled literal in another. Every program and client takes them from here.
//! ⛔ Integers in seeds are LITTLE-endian (`fee u32 LE`, `tick_spacing u16 LE`, `salt u64 LE`,
//! `start_index i32 LE`, `index u64 LE`, `nonce u64 LE`).
//! ⛔ The AMM's seeds (`pool`, `position`, `tick_array`, …), the two seeds of the hook INTERFACE
//! (`hook_config`, `hook_authority`) and the seed-byte helpers are `carpenter_types::seeds`,
//! re-exported here whole; what is DEFINED here is every seed only a FLOOR program derives — none
//! of which the AMM program can spell, since it links `carpenter-types` alone.

pub use carpenter_types::seeds::*;

// ---- floor_hook -------------------------------------------------------------------------
// ⚠ `HOOK_CONFIG` and `HOOK_AUTHORITY` are the INTERFACE's seeds — the AMM derives both under
// any approved hook — and live in `carpenter_types::seeds`, re-exported above.
/// `["floor_pool", amm_pool]`
pub const FLOOR_POOL: &[u8] = b"floor_pool";
/// `["stair", floor_pool]` — a staircase pool's `StairState` under `floor_hook`, created by `prepare_positions` (or,
/// unprepared, by `seed_inventory` inside the graduating buy) and filled by graduation's `seed_inventory`.
pub const STAIR: &[u8] = b"stair";
/// `["stair_rent", amm_pool]` under `floor_hook` — the staircase rent reserve. A SYSTEM-owned, zero-data PDA holding
/// lamports only: the creator's prepaid budget, moved here by `floor_launch::fund_stair_rent` after graduation. The
/// ratchet pays the cranker back the rent it spent out of it (a system transfer signed by these seeds), records it as
/// the rent owner of the tick arrays it pays for, and `floor_hook::reclaim_stair_array` closes those arrays back into
/// it. Anyone may top it up.
pub const STAIR_RENT: &[u8] = b"stair_rent";
/// `["quote", quote_mint]` — the per-quote roster entry (`QuotePolicy`).
/// ⛔ `b"quote"`, not `b"quote_policy"`: a seed is a byte string and this one is spelled ONCE,
/// here, so the hook and every client derive the same address.
pub const QUOTE_POLICY: &[u8] = b"quote";
/// `["platform_treasury"]` — the program-owned PLATFORM TREASURY. ONE address that is BOTH the `PlatformTreasury`
/// state account (its governance and the two-step handover) AND the owner of every token account the 2.5 %
/// platform cut is swept into — `ATA(this PDA, quote_mint, quote token program)`, one per quote.
/// ⛔ No private key exists for it: only `floor_hook` signs for it, and only inside
/// `withdraw_platform`, which its governance signs. ⛔ NOT `hook_authority`: that PDA's token
/// accounts are TRANSIT accounts for `pay_holder` / `claim_referral_fees` / `claim_creator_fees`,
/// and a treasury that may withdraw "whatever it holds" must never share an owner with money in flight.
pub const PLATFORM_TREASURY: &[u8] = b"platform_treasury";
/// `["team_treasury"]` — the team's program-owned treasury under `floor_hook`. `PlatformTreasury`'s layout (its
/// governance and the two-step handover), and the owner of every token account the FLOOR PLATFORM FEE ("team fee" in
/// code; each launch and pool keeps the rate it was created with) is swept into — `ATA(this PDA, quote_mint, quote
/// token program)`, one per quote, opened by the first sweep that moves a team fee in it. ⛔ No private key exists
/// for it, and it is NEVER the platform treasury: the two never cross (`withdraw_team` checks this PDA's governance
/// and ownership only).
pub const TEAM_TREASURY: &[u8] = b"team_treasury";
/// `["floor_platform_pool"]` — the ONE record of `$FLOOR`'s own pool under `floor_hook` (`FloorPlatformPool`), the
/// twin of the EVM's `Layout.floorPool` + `PoolState.platformReceivedEth`. Set once by `designate_floor_pool`
/// (floor_launch's genesis path). Its ADDRESS also owns the TRANSIT token account `credit_floor_pool` stages a wSOL
/// cut through (`ATA(this PDA, wSOL)`, zero at rest) — never the platform or team treasury, which may withdraw
/// "whatever they hold".
pub const FLOOR_PLATFORM_POOL: &[u8] = b"floor_platform_pool";

// ---- trader referrals -------------------------------------------------------------------------
/// `["trader_link", trader]` under `floor_launch` — a trader's `TraderLink`: the three referrer wallets the attestor
/// signed for this trader, written ONCE by `link_trader` and never changed.
pub const TRADER_LINK: &[u8] = b"trader_link";
/// `["trader_ref", trader, quote_mint]` under `floor_hook` — a trader's `TraderBucket` in one quote: what that trader's
/// trades owe each of its tiers. ⛔ Only that trader's own trades (and claims of its tiers) ever write it, so it never
/// serialises trading across pools or traders.
pub const TRADER_REF: &[u8] = b"trader_ref";
/// `["trader_ref_vault"]` under `floor_hook` — the ONE authority whose ATA per quote mint is the shared trader-referral
/// vault. ⛔ Never written during a trade (sweeps and claims only).
pub const TRADER_REF_VAULT: &[u8] = b"trader_ref_vault";
/// `["trader_ref_sale", launch]` under `floor_launch` — a launch's sale referral TOKEN ACCOUNT, authority
/// `["launch_authority"]`; sale trades put their slice here (they already serialise on the `Launch`), swept to the vault later.
pub const TRADER_REF_SALE: &[u8] = b"trader_ref_sale";

// ---- floor_holders, spelled ONCE in `crate::holders` and re-exported here ---------------------
pub use crate::holders::{
    CLAIMS_ALT_AUTHORITY, CLAIMS_REGISTRY, EXTRA_ACCOUNT_METAS, HOLDERS_AUTHORITY, HOLDER_BOOK, HOLDER_RECORD,
};

// ---- floor_launch -----------------------------------------------------------------------
/// `["launch_config"]`
pub const LAUNCH_CONFIG: &[u8] = b"launch_config";
/// `["entry", index u64 LE]`
pub const REGISTRY_ENTRY: &[u8] = b"entry";
/// `["launch", mint]`
pub const LAUNCH: &[u8] = b"launch";
/// `["sale_vault", launch]`
pub const SALE_VAULT: &[u8] = b"sale_vault";
/// `["grad_rent", launch]` — the graduation rent reserve.
/// Zero data, owned by `floor_launch`, funded by the creator at `create_launch`.
pub const GRAD_RENT: &[u8] = b"grad_rent";
/// `["buyer", launch, buyer]`
pub const BUYER: &[u8] = b"buyer";
/// `["mint", creator, nonce u64 LE]` (the B1D vanity mint)
pub const MINT: &[u8] = b"mint";
/// `["mint_authority"]`
pub const MINT_AUTHORITY: &[u8] = b"mint_authority";
/// `["lock_authority"]`. ⛔ Its seed appears only in `floor_launch`'s lock module and here.
pub const LOCK_AUTHORITY: &[u8] = b"lock_authority";
/// `["launch_authority"]`, the signer `floor_hook` trusts for `register_pool` and
/// `resolve_recipient`.
pub const LAUNCH_AUTHORITY: &[u8] = b"launch_authority";
/// `["genesis_sale"]` — `$FLOOR`'s Solana genesis: the ONE timed pro-rata raise, the twin of the EVM's
/// `FloorProtocolSetup`
/// (`FLOOR_TIMED_V1`). ⛔ A SINGLETON, with no launch in its seeds, ON PURPOSE: `init_genesis` can
/// succeed once per program, ever — the EVM's `protocolSetupConsumed` — so no second "$FLOOR" can
/// be created by anyone, the genesis authority included.
pub const GENESIS_SALE: &[u8] = b"genesis_sale";
/// `["genesis_contrib", wallet]` — one contributor's `GenesisContribution` (the EVM's
/// `contributed` / `claimed` / `claimSettled` mappings, one account per wallet). Genesis is a
/// singleton, so the wallet alone keys it.
pub const GENESIS_CONTRIBUTION: &[u8] = b"genesis_contrib";
/// `["quote_walk", payer, quote_mint]` — a `QuoteWalk` that `floor_launch::walk_quote_chain` advances across
/// transactions and `create_launch` consumes (and closes). One per payer and quote; the payer funds it and takes its
/// rent back.
pub const QUOTE_WALK: &[u8] = b"quote_walk";
/// `["preset_registry"]` under `floor_launch` — the preset registry, a singleton; the same bytes as
/// `constants::PRESET_REGISTRY_SEED`.
pub const PRESET_REGISTRY: &[u8] = crate::constants::PRESET_REGISTRY_SEED;

// ---- floor_vesting, spelled HERE for its ONE cross-program caller ------------------------------
// ⛔ `floor_vesting` is standalone and keeps its own copies; these are what `floor_launch::lock_genesis`
// derives the `$FLOOR` locks with, pinned against the program's own bytes.
/// `["schedule", funder, nonce u64 LE]` under `floor_vesting`.
pub const VESTING_SCHEDULE: &[u8] = b"schedule";
/// `["escrow", schedule]` under `floor_vesting`.
pub const VESTING_ESCROW: &[u8] = b"escrow";
/// `["vesting_authority"]` under `floor_vesting`.
pub const VESTING_AUTHORITY: &[u8] = b"vesting_authority";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_are_the_spec_strings() {
        assert_eq!(FLOOR_POOL, b"floor_pool");
        assert_eq!(QUOTE_POLICY, b"quote");
        assert_eq!(QUOTE_WALK, b"quote_walk");
        assert_eq!(PLATFORM_TREASURY, b"platform_treasury");
        // the team treasury, never the platform's
        assert_eq!(TEAM_TREASURY, b"team_treasury");
        assert_ne!(TEAM_TREASURY, PLATFORM_TREASURY);
        // `$FLOOR`'s pool record, never either treasury
        assert_eq!(FLOOR_PLATFORM_POOL, b"floor_platform_pool");
        assert_ne!(FLOOR_PLATFORM_POOL, PLATFORM_TREASURY);
        assert_ne!(FLOOR_PLATFORM_POOL, TEAM_TREASURY);
        assert_eq!(REGISTRY_ENTRY, b"entry");
        assert_eq!(GRAD_RENT, b"grad_rent");
        assert_eq!(PRESET_REGISTRY, b"preset_registry");
        // floor_holders' seeds, through the re-export
        assert_eq!(HOLDER_BOOK, b"holder_book");
        assert_eq!(HOLDER_RECORD, b"holder");
        assert_eq!(EXTRA_ACCOUNT_METAS, b"extra-account-metas");
        assert_eq!(HOLDERS_AUTHORITY, b"holders_authority");
        // the AMM's and the interface's, through the re-export, so one `use` still finds every seed
        assert_eq!(AMM_CONFIG, b"amm_config");
        assert_eq!(HOOK_CONFIG, b"hook_config");
        assert_eq!(EVENT_AUTHORITY, b"__event_authority");
        // trader referrals
        assert_eq!(TRADER_LINK, b"trader_link");
        assert_eq!(TRADER_REF, b"trader_ref");
        assert_eq!(TRADER_REF_VAULT, b"trader_ref_vault");
        assert_eq!(TRADER_REF_SALE, b"trader_ref_sale");
        // ⛔ never the creator stream's names
        assert_ne!(TRADER_REF_VAULT, PLATFORM_TREASURY);
        assert_ne!(TRADER_REF_VAULT, TEAM_TREASURY);
    }

    #[test]
    fn floor_mints_sort_above_wsol() {
        let mut above = crate::keys::WSOL_MINT;
        above[31] = 2;
        assert!(mints_sorted(&crate::keys::WSOL_MINT, &above));
        assert!(!mints_sorted(&above, &crate::keys::WSOL_MINT));
    }
}
