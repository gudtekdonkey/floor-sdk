//! `FloorState`: a Pod mirror of `floor_hook`'s `FloorPool` (784-byte struct, account 792 B). The three-tier referral
//! fields sit @680..760 and the team fee @760..784; no `reserved` is left.
//!
//! Field names match the EVM's `PoolState`; `_eth` suffixes mean lamports. The program crate and `floor-types` own
//! the account; this mirror exists so the mechanism math, the quoter and the wasm build read the same bytes with no
//! Solana crate.
//! ⛔ Every offset is asserted in the tests below; a field moved here and not there (or the reverse) fails
//! `offsets_match_spec_4_2`.
//!
//! The layout is append-only, so unused slots stay in place as padding.
//!
//! ⛔ **The fields carved at 528 are in a DIFFERENT ORDER from the EVM's, deliberately.** The EVM appends
//! `feeAdmin` → `skimBuyAtFloorBps` → `skimSellAtFloorBps` → `bidWidth` (each its own storage slot, so order costs
//! nothing there); here they are `bid_width` (i32) → the two `u16` endpoints → `pad6` (`[u8;32]`), ordered by
//! ALIGNMENT so the carve-out starts on 528 and is padding-free. ⛔ These are two INDEPENDENT account formats that
//! agree on NAMES and VALUES, never a byte mirror of each other (the EVM's is a Solidity struct of 32-byte slots), so
//! do not "correct" this order toward the EVM's: every offset here is a deployed account's, and moving one silently
//! rewrites every `FloorPool` on chain.

#![allow(unsafe_code)]

/// A 128-bit field as `[u64; 2]`, little-endian, low word first (the layout rule). Its alignment is
/// 8, so a `FloorState` has no implicit padding.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct PodU128(pub [u64; 2]);

impl PodU128 {
    pub const fn new(v: u128) -> Self {
        PodU128([v as u64, (v >> 64) as u64])
    }
    pub const fn get(self) -> u128 {
        self.0[0] as u128 | ((self.0[1] as u128) << 64)
    }
    pub fn set(&mut self, v: u128) {
        *self = PodU128::new(v);
    }
}

impl core::fmt::Debug for PodU128 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.get())
    }
}

// SAFETY: repr(transparent) over [u64; 2]: every bit pattern is valid, no padding, all-zero is valid.
unsafe impl bytemuck::Zeroable for PodU128 {}
unsafe impl bytemuck::Pod for PodU128 {}

/// `FloorPool`, struct offsets in the comments (account offset = struct offset + 8).
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct FloorState {
    /* 0 */ pub version: u8,
    /* 1 */ pub bump: u8,
    /* 2 */ pub pool_bound: u8,
    /* 3 */ pub quote_is_token1: u8,
    /* 4 */ pub ladder_seeded: u8,
    /* 5 */ pub needs_ratchet: u8,
    /* 6 */ pub mode: u8,
    /// The hook's `observe_quote` halt flag — READ BY NOTHING in this crate: the mechanism's arithmetic is the same
    /// whether the quote is frozen or not, and the gate that asks it lives on the hook's entry points, not in a
    /// solve. Named here only so the mirror stays field-for-field with `floor_types::FloorPool`.
    /* 7 */ pub quote_suspended: u8,
    /* 8 */ pub tick_spacing: i32,
    /* 12 */ pub floor_tick: i32,
    /* 16 */ pub amm_pool: [u8; 32],
    /* 48 */ pub pool_initializer: [u8; 32],
    /* 80 */ pub base_mint: [u8; 32],
    /* 112 */ pub quote_mint: [u8; 32],
    /* 144 */ pub pad9: [u8; 32], // unused (the creator slots are the account's payee tail)
    /* 176 */ pub pad10: [u8; 32], // unused
    /* 208 */ pub skim_buy_far_bps: u16,
    /* 210 */ pub skim_sell_far_bps: u16,
    /* 212 */ pub creator_buy_bps: u16,
    /* 214 */ pub creator_sell_bps: u16,
    /* 216 */ pub holder_buy_bps: u16,
    /* 218 */ pub holder_sell_bps: u16,
    /// The wall's offset from the floor in ticks, written once by `register_pool` (`crate::wall_ticks_centi`). Nothing
    /// in this crate reads it.
    /* 220 */ pub wall_ticks: u16,
    /* 222 */ pub platform_bps: u16,
    /* 224 */ pub min_bid: u64,
    /* 232 */ pub min_sellable: u64,
    /// Team steps and the team buy. `owner_owed`: the quote the step OWNER's payee is owed (its share of what owner
    /// steps SOLD for, booked after FLOOR FIRST, never backing, never bids) — a QUOTE LIABILITY in
    /// `FloorState::quote_liabilities`; the ONE decrement is `floor_hook::sweep_stair_payee`. `owner_paid`: the
    /// lifetime total paid out of it; only rises. `register_pool` writes both 0.
    /* 240 */ pub owner_owed: u64,
    /* 248 */ pub owner_paid: u64,
    /* 256 */ pub vault_eth: u64,
    /* 264 */ pub bid_eth_deployed: u64,
    /* 272 */ pub unpaid_skim: u64,
    /* 280 */ pub cumulative_volume_eth: u64,
    /* 288 */ pub inventory_tokens: u64,
    /* 296 */ pub inventory_cost_eth: u64,
    /* 304 */ pub ask_tokens_deployed: u64,
    /* 312 */ pub ladder_tokens_remaining: u64,
    /* 320 */ pub bid_lower: i32,
    /* 324 */ pub ask_upper: i32,
    /* 328 */ pub ask_width: i32,
    /* 332 */ pub seed_lower: i32,
    /* 336 */ pub seed_upper: i32,
    /* 340 */ pub ladder_lower: i32,
    /* 344 */ pub ladder_upper: i32,
    /// The wall's split tick: the band's DEAR edge = the tail's CHEAP edge (`band::wall_band_split`). Meaningful ONLY
    /// while `wall_tail_liq != 0` — ⛔ never test `wall_split_tick != 0` for "is there a tail?" (a real split can sit
    /// at tick 0); 0 while the wall is one position. Inert on a staircase pool.
    /* 348 */ pub wall_split_tick: i32,
    /* 352 */ pub bid_liquidity: PodU128,
    /* 368 */ pub ask_liquidity: PodU128,
    /* 384 */ pub seed_liq: PodU128,
    /* 400 */ pub ladder_liq: PodU128,
    /* 416 */ pub creator_fees_owed: u64,
    /* 424 */ pub platform_owed: u64,
    /* 432 */ pub holder_owed: u64,
    /// The wall tail's liquidity (salt 101) on `[ladder_lower, wall_split_tick]`; 0 when the wall is ONE position.
    /// Inert on a staircase pool.
    /* 440 */ pub wall_tail_liq: PodU128,
    /// The wall's held tokens (inert on a staircase pool, where it counts the staircase's tokens in the claims): still
    /// the protocol's, sitting in the pool's claims, OUTSIDE circulating supply ([`crate::circulating_supply`]).
    /* 456 */ pub wall_held_tokens: u64,
    /* 464 */ pub seed_fees_swept_eth: u64,
    /* 472 */ pub seed_fees_swept_tokens: u64,
    /* 480 */ pub ladder_proceeds_eth: u64,
    /* 488 */ pub platform_paid_eth: u64,
    /* 496 */ pub platform_swept: u64,
    /* 504 */ pub creator_fees_paid: u64,
    /* 512 */ pub holder_paid_eth: u64,
    /* 520 */ pub last_ratchet_slot: u64,
    // ---- carved out of `reserved` at the offsets `floor_types::layout::FloorPool` uses ----
    /// Width of the deployed bid in ticks (the EVM's `bidWidth`): the band runs `[bid_lower, bid_lower + bid_width]`
    /// and its cheap edge is `floor_tick`. 0 only on a pool with no bid yet (read as one spacing,
    /// `band::bid_width_or_spacing`).
    /* 528 */ pub bid_width: i32,
    /// The at-floor skim endpoints, written ONCE at `register_pool`: 0 and
    /// `constants::skim_sell_at_floor_bps_at(skim_sell_far_bps, multiple)`. No setter.
    /* 532 */ pub skim_buy_at_floor_bps: u16,
    /* 534 */ pub skim_sell_at_floor_bps: u16,
    /// Unused (the EVM's dead `feeAdmin` word; no skim setter exists on either chain). `[..8]` holds
    /// [`Self::owner_pending`].
    /* 536 */ pub pad6: [u8; 32],
    /* 568 */ pub max_bounty: u64,
    /* 576 */ pub bounty_bps: u16,
    /// The wall shape (inert: every pool is a staircase), written ONCE by `register_pool` from its trailing
    /// `WallShapeArg` and never again (no setter): the band's RAW width `wall_ticks(W)` (0 = the open shape —
    /// ⛔ never `wall_ticks(0)`, which is the DEFAULT 5×), the tail's share of the listed tokens, and the list share
    /// (0 = list everything).
    /* 578 */ pub wall_band_ticks: u16,
    /* 580 */ pub wall_tail_bps: u16,
    /* 582 */ pub wall_list_bps: u16,
    // ---- the CONDITIONAL creator-fee referral split ------------------------------------------
    // Carved out of `reserved` at the offsets `floor_types::layout::FloorPool` uses, ordered by ALIGNMENT so the
    // carve starts on the 8-aligned 584 and is padding-free. `FLOOR_STATE_SIZE` is unchanged at 784.
    // ⛔ None of these has a per-pool setter anywhere. `register_pool` writes them once; afterwards only the meters
    // move — up in `take_fees`, down in `claim_referral_fees`.
    // ⛔ ACCRUAL BY DESIGN: the share sits accrued and moves when someone claims it, so no pool's trading is
    // serialised. The swap callback holds only its fixed hook accounts and so cannot pay out inside the swap; paying
    // it would also write-lock one shared account across every referred pool's every trade — the trap the platform
    // cut (`sweep_platform_cut`) avoids by being pull-shaped too.
    /// Lamports split off a creator fee and not yet claimed by the pool's `referrer` wallet through
    /// `floor_hook::claim_referral_fees` (the ONE decrement). The EVM's twin is `referralFeesOwed`.
    /// ⛔ A QUOTE LIABILITY: it belongs in EVERY I-3 sum — the hook's fee check, the ratchet's claims check and surplus
    /// booking, and the quoter's epilogue — through ONE spelling, [`FloorState::quote_liabilities`]. The split does
    /// not change the SUM — what this gains, `creator_fees_owed` does not. ⛔ Drop it from ANY one of them and the
    /// pool reports more free claims than it holds — and where that one is `book_claims_surplus`, the ratchet BOOKS
    /// the referrer's lamports as a donation and then its own epilogue reverts on the shortfall, for ever.
    /* 584 */ pub referral_owed: u64,
    /// Lifetime lamports split off to the referrer (the EVM's `referralFeesPaid`): `referral_owed` plus everything
    /// already claimed out. Only ever rises.
    /* 592 */ pub referral_fees_paid: u64,
    /// Trader referrals: the trader slices booked out of `team_owed` and not yet swept — a QUOTE LIABILITY
    /// ([`Self::quote_liabilities`]). The retired `referral_distributor` bytes.
    /* 600 */ pub trader_ref_owed: u64,
    /// Lifetime trader slices booked on this pool (owed included).
    /* 608 */ pub trader_ref_booked: u64,
    /* 616 */ pub pad7: [u8; 16], // the rest of the retired distributor bytes; reserved for a per-pool rate copy
    /// Share of a GROSS creator fee that goes to the REFERRER, in bips (1000 = 10 %, the EVM's own default). 0 on an
    /// unreferred pool. ⛔ A STORED COPY, never a live cross-program read: the swap callback holds no account it could
    /// read one from, so the rate MUST live here — a mechanism divergence from the EVM forced by the account model, not
    /// chosen. See `floor_types::constants::REFERRAL_CREATOR_SHARE_BPS`. ⚠ An indexer that stores no rate is not in
    /// tension with this: its ROWS carry amount and gross, so a reader derives the rate that applied; this is the rate
    /// the pool applies.
    /* 632 */ pub referral_share_bps: u16,
    /// ⛔ THE STAIRCASE, carved from `pad3` (zero reads "no staircase"). `stair_steps` != 0 marks a STAIRCASE pool,
    /// written ONCE by `register_pool`; `stair_ready` is 1 once graduation's `seed_inventory` has placed the staircase
    /// INSIDE the graduating buy (no swap gate reads it); `stair_growth_bps` is the step weighting.
    /* 634 */ pub stair_steps: u8,
    /* 635 */ pub stair_ready: u8,
    /* 636 */ pub stair_growth_bps: u16,
    /// The staircase parameter set the pool runs (`stair::STAIR_KIND_PRESET` 0 / `STAIR_KIND_GENESIS` 1), carved from
    /// `pad3`; written once by `floor_hook::set_genesis_stair`.
    /* 638 */ pub stair_kind: u8,
    /// Team steps and the team buy, carved from `pad3`: 0 none / 1 team / 2 deployer. Gates `owner_owed`.
    /* 639 */ pub stair_owner: u8,
    // ---- holder rewards + the on-chain referrer -----------------------------------------------
    // Mirrors `floor_types::layout::FloorPool` (carved from the front of `reserved`).
    // ⛔ The retired `referral_distributor` bytes @600 are never read as a referrer — `take_fees` gates on
    // [`FloorState::referrer`] alone.
    /// The referrer's WALLET, or the zero key when the launch was not referred. A referrer exists only when the
    /// deployer is the fee recipient, enforced on chain.
    /// ⛔ **The zero key IS the gate** in `take_fees` — no flag, the destination is simply absent.
    /* 640 */ pub referrer: [u8; 32],
    /* 672 */ pub pad8: u8, // the retired version byte, padding
    /// 1 iff the base mint carries FLOOR's transfer-hook slot. Read by nothing here.
    /* 673 */ pub holders_hooked: u8,
    /* 674 */ pub pad4: [u8; 6],
    // ---- three-tier referrals: each tier gets 10 % of the tier below, from that tier's cut ---------
    // Carved out of the FRONT of `reserved`, ordered by alignment so the carve is padding-free; the struct is still
    // 784 B. A zeroed pool reads ZERO in all four, which IS "single-tier, nothing paid to tiers 2/3".
    // ⛔ NO SETTER, EVER: `register_pool` writes the two wallets once; the two counters move only UP, only in
    // `claim_referral_fees`.
    // ⛔ There is NO tier-1 counter and there must never be one: tier 1's paid is DERIVED
    // (`referral::referral_tier_dues`).
    /// Tier 2 of the referral chain (the referrer's own referrer), or the zero key. Set ⇒ `referrer`
    /// set. Read through [`crate::referral::referral_tier_dues`].
    /* 680 */ pub referrer_t2: [u8; 32],
    /// Tier 3 (tier 2's referrer), or the zero key. Set ⇒ `referrer_t2` set.
    /* 712 */ pub referrer_t3: [u8; 32],
    /// Lifetime quote paid to tier 2. Only ever rises, only in `claim_referral_fees`.
    /* 744 */ pub referral_paid_t2: u64,
    /// Lifetime quote paid to tier 3. Only ever rises, only in `claim_referral_fees`.
    /* 752 */ pub referral_paid_t3: u64,
    // the team fee: the LAST 24 B of `reserved` (24 -> 0). ⛔ No `reserved` is left: the next field needs a realloc
    // or a new version.
    /// Platform fee owed to the team, not yet swept. ⛔ A QUOTE LIABILITY: counted by
    /// [`Self::quote_liabilities`]. Up: [`crate::take_fees`]; zero: `sweep_platform_cut` only.
    /* 760 */ pub team_owed: u64,
    /// Lifetime swept to the team treasury. Read by nothing in this crate.
    /* 768 */ pub team_fees_paid: u64,
    /// The pool's own FLOOR Platform fee rate, written by `register_pool` from the launch config's setting at CREATION
    /// (`<= TEAM_POOL_FEE_BPS_MAX`). Kept for the pool's life — no per-pool setter.
    /* 776 */ pub team_fee_bps: u16,
    /* 778 */ pub pad5: [u8; 6],
}

/// The struct is 784 B; the layout rule `size % 8 == 0 && align ≤ 8`.
pub const FLOOR_STATE_SIZE: usize = 784;
#[allow(clippy::manual_is_multiple_of)] // spelled as the layout rule is written
const _: () = assert!(
    core::mem::size_of::<FloorState>() == FLOOR_STATE_SIZE
        && core::mem::size_of::<FloorState>() % 8 == 0
        && core::mem::align_of::<FloorState>() <= 8
);
const _: () = assert!(core::mem::size_of::<PodU128>() == 16 && core::mem::align_of::<PodU128>() == 8);

// SAFETY: repr(C); every field is itself Pod (integers, byte arrays, PodU128); the size assert above
// equals the sum of the field sizes (checked field by field in `offsets_match_spec_4_2`), so there is
// no padding byte; all-zero is a valid value.
unsafe impl bytemuck::Zeroable for FloorState {}
unsafe impl bytemuck::Pod for FloorState {}

impl Default for FloorState {
    fn default() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

impl core::fmt::Debug for FloorState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FloorState")
            .field("tick_spacing", &self.tick_spacing)
            .field("floor_tick", &self.floor_tick)
            .field("vault_eth", &self.vault_eth)
            .field("inventory_tokens", &self.inventory_tokens)
            .field("ladder_tokens_remaining", &self.ladder_tokens_remaining)
            .field("wall_held_tokens", &self.wall_held_tokens)
            .field("wall_tail_liq", &self.wall_tail_liq)
            .field("seed_liq", &self.seed_liq)
            .field("platform_owed", &self.platform_owed)
            .field("creator_fees_owed", &self.creator_fees_owed)
            .finish_non_exhaustive()
    }
}

impl FloorState {
    /// Borrow the struct from an account's data AFTER the 8-byte discriminator. `None` on a wrong
    /// length or misalignment (bytemuck's checks).
    pub fn from_bytes(data: &[u8]) -> Option<&FloorState> {
        bytemuck::try_from_bytes(data).ok()
    }

    pub fn from_bytes_mut(data: &mut [u8]) -> Option<&mut FloorState> {
        bytemuck::try_from_bytes_mut(data).ok()
    }

    /// A copy decoded from unaligned bytes (e.g. a `Vec<u8>` read off RPC).
    pub fn read_unaligned(data: &[u8]) -> Option<FloorState> {
        bytemuck::try_pod_read_unaligned(data).ok()
    }

    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }

    /// I-3's QUOTE side: every idle lamport liability this pool carries, summed in u128 so the u64s cannot overflow.
    /// The epilogue on every callback requires `claims_q + credit_q >= quote_liabilities()`.
    ///
    /// ⛔ **THE ONE SPELLING.** The hook's fee check (`assert_claims_cover`), the ratchet's claims check
    /// (`claims_cover_liabilities`), the ratchet's surplus booking (`claims_surplus`) and the quoter's epilogue
    /// (`carpenter_quote::floor::epilogue_i3`) all call this. A liability added to only some of them would leave the
    /// rest quietly LOOSER than the chain: the quoter would promise a swap the hook reverts, an instruction would let
    /// a withdrawal past that leaves a liability unbacked, or the ratchet would BOOK the liability into the vault as
    /// surplus and then revert on its own epilogue. No `vault_eth + creator_fees_owed + platform_owed + holder_owed`
    /// may exist anywhere but here, and test oracles read THIS function too, or they are looser than the chain.
    /// ⛔ Never re-inline the sum at a call site; add a liability HERE and every gate gets it at once.
    /// ⚠ A new idle quote liability that is NOT added here is invisible to every gate.
    pub fn quote_liabilities(&self) -> u128 {
        self.vault_eth as u128
            + self.creator_fees_owed as u128
            + self.platform_owed as u128
            + self.holder_owed as u128
            // lamports split off a creator fee and not yet claimed by the referrer. They sit in the pool's claims
            // exactly as the platform cut does.
            + self.referral_owed as u128
            // the team's fee, owed and unswept. Left out, a ratchet books it as a DONATION and then reverts on its own
            // epilogue.
            + self.team_owed as u128
            // team steps: the step owner's payee share, owed and unswept (`register_pool` zeroes it on every pool).
            + self.owner_owed as u128
            // the payee's share SET ASIDE by a ratchet that DEFERRED part of its collection, undecided until the
            // ratchet that finishes it. It sits in the claims, never backing; left out, `book_claims_surplus` banks it
            // into the vault as a donation.
            + self.owner_pending() as u128
            // trader referrals: the slices moved OUT of `team_owed` into the trader books, still in the pool's claims
            // until `sweep_trader_referral` moves them to the vault. Left out, the move would shrink the liabilities
            // by exactly the slice and a ratchet would book it as a donation.
            + self.trader_ref_owed as u128
    }

    /// ⛔ THE PAYEE'S PENDING SHARE (the EVM twin pays the payee only on the ratchet that finishes the collection):
    /// quote a ratchet that left part of its collection for the next one (`needs_ratchet`) set aside for the payee —
    /// decided by nothing and forfeited by nothing until the ratchet that finishes the collection folds it back. A
    /// QUOTE LIABILITY ([`Self::quote_liabilities`]). Stored in `pad6[..8]` (little-endian), the bytes of the unused
    /// `fee_admin` @536.
    pub fn owner_pending(&self) -> u64 {
        u64::from_le_bytes([
            self.pad6[0], self.pad6[1], self.pad6[2], self.pad6[3], self.pad6[4], self.pad6[5], self.pad6[6], self.pad6[7],
        ])
    }

    /// Writes [`Self::owner_pending`] (`pad6[..8]`; the other 24 bytes untouched).
    pub fn set_owner_pending(&mut self, v: u64) {
        self.pad6[..8].copy_from_slice(&v.to_le_bytes());
    }

    /// Is this a STAIRCASE pool? The ONE spelling: `stair_steps != 0`. On a staircase
    /// pool `ladder_tokens_remaining` counts only tokens standing in the staircase's positions (the steps, the
    /// tail, the POL re-list) — never in the claims — and every staircase token still in the claims is
    /// `wall_held_tokens`, so the placed side is always "placed" for [`FloorState::base_liabilities`].
    pub fn is_stair(&self) -> bool {
        self.stair_steps != 0
    }

    /// I-3's BASE side: every token of the protocol's that must sit IDLE in the pool's claims — the inventory, the
    /// ladder tokens of a NON-staircase state, the held tokens (always idle: they never leave claims), and the team's
    /// held tokens. u128, so the u64s cannot overflow.
    ///
    /// ⛔ THE ONE SPELLING, as [`Self::quote_liabilities`] is for the quote side: the hook's fee check, the ratchet's
    /// claims check and the quoter's epilogue all call this; a held count added to only some of them would leave the
    /// rest looser than the chain.
    /// ⚠ A staircase pool's ladder tokens stand in its positions; only a NON-staircase state (which `register_pool`
    /// refuses) counts them idle, the conservative reading.
    pub fn base_liabilities(&self) -> u128 {
        self.inventory_tokens as u128
            + if self.is_stair() { 0 } else { self.ladder_tokens_remaining as u128 }
            + self.wall_held_tokens as u128
            + self.owner_held() as u128
    }

    /// ⛔ THE TEAM'S HELD TOKENS: owner tokens whose step left the staircase unsold, set aside in claims1 for the payee
    /// (`StairOwners::held`'s mirror). Stored in the bytes of `ask_tokens_deployed` (@304; no ask is ever deployed). A
    /// BASE LIABILITY (the claims check covers it) that is NOT subtracted from circulating supply: held team tokens
    /// count against the floor — the EVM's `custody == inventoryTokens + held`, `held` outside `ladderTokensRemaining`.
    pub fn owner_held(&self) -> u64 {
        self.ask_tokens_deployed
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use core::mem::offset_of;

    // The account layout, offset by offset.
    #[test]
    fn offsets_match_spec_4_2() {
        macro_rules! at {
            ($f:ident, $o:expr) => {
                assert_eq!(offset_of!(FloorState, $f), $o, stringify!($f));
            };
        }
        at!(version, 0); at!(bump, 1); at!(pool_bound, 2); at!(quote_is_token1, 3);
        at!(ladder_seeded, 4); at!(needs_ratchet, 5); at!(mode, 6); at!(quote_suspended, 7);
        at!(tick_spacing, 8); at!(floor_tick, 12);
        at!(amm_pool, 16); at!(pool_initializer, 48); at!(base_mint, 80); at!(quote_mint, 112);
        at!(pad9, 144); at!(pad10, 176);
        at!(skim_buy_far_bps, 208); at!(skim_sell_far_bps, 210); at!(creator_buy_bps, 212); at!(creator_sell_bps, 214);
        at!(holder_buy_bps, 216); at!(holder_sell_bps, 218); at!(wall_ticks, 220); at!(platform_bps, 222);
        at!(min_bid, 224); at!(min_sellable, 232); at!(owner_owed, 240); at!(owner_paid, 248);
        at!(vault_eth, 256); at!(bid_eth_deployed, 264); at!(unpaid_skim, 272); at!(cumulative_volume_eth, 280);
        at!(inventory_tokens, 288); at!(inventory_cost_eth, 296); at!(ask_tokens_deployed, 304); at!(ladder_tokens_remaining, 312);
        at!(bid_lower, 320); at!(ask_upper, 324); at!(ask_width, 328); at!(seed_lower, 332);
        at!(seed_upper, 336); at!(ladder_lower, 340); at!(ladder_upper, 344); at!(wall_split_tick, 348);
        at!(bid_liquidity, 352); at!(ask_liquidity, 368); at!(seed_liq, 384); at!(ladder_liq, 400);
        at!(creator_fees_owed, 416); at!(platform_owed, 424); at!(holder_owed, 432); at!(wall_tail_liq, 440);
        at!(wall_held_tokens, 456); at!(seed_fees_swept_eth, 464); at!(seed_fees_swept_tokens, 472);
        at!(ladder_proceeds_eth, 480); at!(platform_paid_eth, 488); at!(platform_swept, 496); at!(creator_fees_paid, 504);
        at!(holder_paid_eth, 512); at!(last_ratchet_slot, 520); at!(bid_width, 528);
        at!(skim_buy_at_floor_bps, 532); at!(skim_sell_at_floor_bps, 534); at!(pad6, 536);
        at!(max_bounty, 568); at!(bounty_bps, 576);
        // the wall's shape: the three config words in the old `pad2`.
        at!(wall_band_ticks, 578); at!(wall_tail_bps, 580); at!(wall_list_bps, 582);
        // the referral carve. Mutation this kills: any of the four moved, resized or reordered against
        // `floor_types::layout::FloorPool` — which would make `bytemuck::cast_mut(FloorPool) -> FloorState` in
        // `mech/fees.rs` book into the wrong bytes.
        at!(referral_owed, 584); at!(referral_fees_paid, 592); at!(trader_ref_owed, 600); at!(trader_ref_booked, 608); at!(pad7, 616);
        at!(referral_share_bps, 632); at!(stair_steps, 634); at!(stair_ready, 635); at!(stair_growth_bps, 636); at!(stair_kind, 638); at!(stair_owner, 639);
        // the holders / referrer carve. Mutation this kills: the referrer written over the RETIRED distributor bytes
        // @600, or any of the four moved against `floor_types::layout::FloorPool`
        // (`floor_types::holders::FLOOR_POOL_ACCT_*` read the raw bytes at these offsets + 8).
        at!(referrer, 640); at!(pad8, 672); at!(holders_hooked, 673); at!(pad4, 674);
        // the tiers carve. Mutation this kills: a tier wallet or counter moved, resized or transposed against
        // `floor_types::layout::FloorPool`.
        at!(referrer_t2, 680); at!(referrer_t3, 712); at!(referral_paid_t2, 744); at!(referral_paid_t3, 752);
        // the team-fee carve on the tiers' last byte; it spends the rest of `reserved`, so `pad5` ends the struct.
        at!(team_owed, 760); at!(team_fees_paid, 768); at!(team_fee_bps, 776); at!(pad5, 778);
        assert_eq!(778 + 6, FLOOR_STATE_SIZE);
    }

    /// I-3's quote side counts `referral_owed`.
    ///
    /// ⛔ THE MUTATION THIS KILLS is the one that under-collateralises a pool: drop
    /// `referral_owed` from `quote_liabilities` and the pool reports fewer liabilities than it
    /// holds, so a claim or a withdrawal is let past that leaves the referrer's lamports unbacked.
    /// Every one of the five terms is asserted INDEPENDENTLY, so dropping any single one fails.
    #[test]
    fn quote_liabilities_counts_every_idle_lamport_including_the_referral() {
        let mut s = FloorState::default();
        assert_eq!(s.quote_liabilities(), 0);
        s.vault_eth = 1;
        s.creator_fees_owed = 2;
        s.platform_owed = 4;
        s.holder_owed = 8;
        s.referral_owed = 16;
        s.team_owed = 32; // the sixth term
        // powers of two: the total names exactly which terms are counted
        assert_eq!(s.quote_liabilities(), 63, "a missing term shows up as a missing bit");

        // and the split itself does not move the total -- what `referral_owed` gains,
        // `creator_fees_owed` does not, so I-3 is exactly as strict as before referrals existed
        let mut before = FloorState::default();
        before.creator_fees_owed = 100;
        let mut after = FloorState::default();
        after.creator_fees_owed = 90;
        after.referral_owed = 10;
        assert_eq!(before.quote_liabilities(), after.quote_liabilities());

        // five u64::MAX cannot overflow the u128 sum
        let mut big = FloorState::default();
        big.vault_eth = u64::MAX;
        big.creator_fees_owed = u64::MAX;
        big.platform_owed = u64::MAX;
        big.holder_owed = u64::MAX;
        big.referral_owed = u64::MAX;
        big.team_owed = u64::MAX;
        assert_eq!(big.quote_liabilities(), 6 * u64::MAX as u128);
    }

    /// Trader referrals: `trader_ref_owed` is the ninth quote term, and moving a slice out of `team_owed` into it leaves
    /// the total unchanged. Mutations this kills: the term dropped (a ratchet books the referrers' slices as a donation
    /// and reverts on its own epilogue); the term read off `trader_ref_booked` (the lifetime meter, which never falls).
    #[test]
    fn trader_ref_owed_is_a_liability_and_the_move_out_of_team_owed_keeps_the_sum() {
        let mut s = FloorState::default();
        s.trader_ref_owed = 128;
        s.trader_ref_booked = 1 << 40;
        assert_eq!(s.quote_liabilities(), 128, "owed counts; the booked meter does not");
        let mut before = FloorState::default();
        before.team_owed = 1_000;
        let mut after = FloorState::default();
        after.team_owed = 630;
        after.trader_ref_owed = 370;
        assert_eq!(before.quote_liabilities(), after.quote_liabilities());
        let mut big = FloorState::default();
        big.team_owed = u64::MAX;
        big.trader_ref_owed = u64::MAX;
        assert_eq!(big.quote_liabilities(), 2 * u64::MAX as u128);
    }

    /// Team steps: the payee's owed share is the seventh term. Mutation: drop the term (the ratchet books the payee's
    /// quote as a donation).
    #[test]
    fn owner_owed_is_a_liability() {
        let mut s = FloorState::default();
        s.owner_owed = 64;
        assert_eq!(s.quote_liabilities(), 64);
        s.stair_owner = 1;
        assert_eq!(s.quote_liabilities(), 64);
    }

    /// The payee's pending share lives in `pad6[..8]` and is the eighth quote term. Mutations this
    /// kills: the term dropped (the ratchet banks the set-aside share as a donation); the accessor reading other bytes.
    #[test]
    fn owner_pending_is_a_liability_in_pad6() {
        let mut s = FloorState::default();
        s.set_owner_pending(0x0102_0304_0506_0708);
        assert_eq!(&s.pad6[..8], &0x0102_0304_0506_0708u64.to_le_bytes());
        assert_eq!(&s.pad6[8..], &[0u8; 24]);
        assert_eq!(s.owner_pending(), 0x0102_0304_0506_0708);
        assert_eq!(s.quote_liabilities(), 0x0102_0304_0506_0708);
        s.owner_owed = 1;
        s.set_owner_pending(2);
        assert_eq!(s.quote_liabilities(), 3);
    }

    #[test]
    fn u128_fields_are_little_endian_low_word_first() {
        let mut s = FloorState::default();
        s.seed_liq.set(0x0102_0304_0506_0708_1112_1314_1516_1718);
        let bytes = s.as_bytes();
        assert_eq!(&bytes[384..392], &0x1112_1314_1516_1718u64.to_le_bytes());
        assert_eq!(&bytes[392..400], &0x0102_0304_0506_0708u64.to_le_bytes());
        assert_eq!(PodU128::new(u128::MAX).get(), u128::MAX);
    }

    #[test]
    fn round_trips_through_bytes() {
        let mut s = FloorState::default();
        s.floor_tick = -123_456;
        s.vault_eth = 42;
        s.ladder_liq.set(u128::MAX - 5);
        s.bid_width = 120;
        s.skim_sell_at_floor_bps = 1000;
        s.pad6 = [7; 32];
        // the referral carve; the LAST byte is still the last byte of the 784-byte struct, which is what this case is
        // really pinning.
        s.trader_ref_owed = 8;
        s.trader_ref_booked = 8;
        s.pad7 = [8; 16];
        s.referral_owed = 13;
        // the holders carve; its last byte is still byte 783.
        s.referrer = [10; 32];
        s.pad8 = 1;
        s.holders_hooked = 1;
        // the tiers carve.
        s.referrer_t2 = [11; 32];
        s.referrer_t3 = [12; 32];
        s.referral_paid_t2 = 14;
        s.referral_paid_t3 = 15;
        // the team carve: the last 24 B; `pad5`'s last byte is byte 783.
        s.team_owed = 21;
        s.team_fee_bps = 25;
        s.pad5[5] = 9;
        // the wall's shape words in the dead / pad slots.
        s.wall_split_tick = -67_920;
        s.wall_tail_liq.set(u128::MAX - 7);
        s.wall_held_tokens = 150_000_000_000_000;
        s.wall_band_ticks = 10_987;
        s.wall_tail_bps = 1_000;
        s.wall_list_bps = 7_500;
        let copy = FloorState::read_unaligned(s.as_bytes()).unwrap();
        assert_eq!(copy, s);
        assert!(FloorState::read_unaligned(&s.as_bytes()[..783]).is_none());
    }

    /// I-3's base side counts the held tokens always; the inventory is counted whole and the ladder count only on a
    /// NON-staircase state. Each term is a distinct power of two, so a dropped term is a missing bit. Mutations this
    /// kills: held left out; a staircase's standing tokens counted idle.
    #[test]
    fn base_liabilities_count_held_and_never_a_staircases_standing_tokens() {
        let mut s = FloorState::default();
        assert_eq!(s.base_liabilities(), 0);
        s.inventory_tokens = 1;
        s.ladder_tokens_remaining = 2;
        s.wall_held_tokens = 4;
        assert_eq!(s.base_liabilities(), 7, "not a staircase: inventory + ladder + held");
        s.stair_steps = 3;
        assert_eq!(s.base_liabilities(), 5, "a staircase: its ladder count stands in its positions");
        let mut big = FloorState::default();
        big.inventory_tokens = u64::MAX;
        big.ladder_tokens_remaining = u64::MAX;
        big.wall_held_tokens = u64::MAX;
        assert_eq!(big.base_liabilities(), 3 * u64::MAX as u128);
    }
}
