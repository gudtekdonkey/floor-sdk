//! The launch → hook interface: `floor_hook::register_pool`'s arguments.
//!
//! `floor_launch::create_launch` builds them and CPIs `register_pool` signed by `launch_authority`; `floor_hook`
//! mirrors each field for field (Anchor needs the IDL type in its own crate), and a layout test pins the mirror's
//! field list to this one.
//!
//! Every trailing argument but the owner kind is REQUIRED (plain borsh): the tiers (64), the team fee (2), the at-floor
//! sell multiple (4) and the staircase (3).
//!
//! `disc(8) ‖ RegisterPoolArgs(265) ‖ ReferralTiersArg(64) ‖ TeamFeeArg(2) ‖ SkimSellMultipleArg(4) ‖ StairArg(3) ‖
//! [StairOwnerKindArg(1), optional]` = 346 (or 347) bytes.
//! ⛔ Field order is the wire format.

use borsh::{BorshDeserialize, BorshSerialize};

/// `floor_hook::register_pool`'s main argument. ⛔ Field order is the wire format.
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegisterPoolArgs {
    /// The predicted AMM pool (the quote, mint, `0x800000`, 60, `floor_hook`).
    pub amm_pool: [u8; 32],
    pub base_mint: [u8; 32],
    /// The pool's quote mint (wSOL, or a listed quote).
    pub quote_mint: [u8; 32],
    /// The Launch PDA.
    pub pool_initializer: [u8; 32],
    pub mode: u8,
    pub skim_buy_far_bps: u16,
    pub skim_sell_far_bps: u16,
    pub creator_buy_bps: u16,
    pub creator_sell_bps: u16,
    /// `<= MAX_HOLDER_BPS` (`constants::MAX_HOLDER_BPS`); a CAP, the creator fee's shape.
    pub holder_buy_bps: u16,
    /// `<= MAX_HOLDER_BPS`.
    pub holder_sell_bps: u16,
    /// The launch's DEPLOYER (`Launch.creator`, the signer of `create_launch`) — not the fee recipient (the creator
    /// slots ride the trailing `slots::SlotsArg`). Read only by the referral shape checks (no referrer or tier is the
    /// deployer).
    pub deployer: [u8; 32],
    /// Unused, written zero (a slot's pending id is in `SlotsArg`).
    pub pad: [u8; 32],
    pub min_bid: u64,
    pub min_sellable: u64,
    pub platform_bps: u16,
    /// The pool's ratchet bounty CEILING, in lamports. Copied from
    /// `LaunchConfig.max_bounty_lamports` (or the quote's policy) at `create_launch` and fixed for the pool's life.
    pub max_bounty: u64,
    /// The pool's ratchet bounty RATE, in bips of the skim collected since the last ratchet.
    pub bounty_bps: u16,
    /// **THE REFERRER'S WALLET**, or the zero key when the launch was not referred. A referrer exists only when the
    /// deployer is the fee recipient. `register_pool` copies it into `FloorPool.referrer` with
    /// `referral_share_bps = 1000` (fixed 10 %), and `claim_referral_fees` pays that wallet and nobody else. ⛔ The
    /// referrer rule IS ENFORCED ON CHAIN, TWICE (`floor_launch::create_launch`, then `register_pool`'s
    /// `ReferrerNotAllowed`). ⛔ ZERO IS THE SAFE DEFAULT: a zero referrer splits nothing.
    pub referrer: [u8; 32],
}

/// Borsh length: 32×4 + 1 + 2×6 + 32×2 + 8×2 + 2 + 8 + 2 + 32 = **265**.
pub const REGISTER_POOL_ARGS_LEN: usize = 128 + 1 + 12 + 64 + 16 + 2 + 8 + 2 + 32;

/// Three-tier referrals, a separate argument, never inside [`RegisterPoolArgs`]: REQUIRED on `floor_hook::register_pool`
/// (after the args) and on `floor_launch::create_launch` (its last argument) — the bare 64 bytes `t2 ‖ t3`, either may
/// be the zero key (`t3` set needs `t2` set). Each program carries its own Anchor copy; this is the borsh twin both
/// encode through.
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReferralTiersArg {
    pub referrer_t2: [u8; 32],
    pub referrer_t3: [u8; 32],
}

/// The encoded length of [`ReferralTiersArg`].
pub const REFERRAL_TIERS_ARG_LEN: usize = 64;

/// The pool's FLOOR Platform fee rate, `LaunchConfig::team_pool_bps()` at `create_launch`, fixed for the pool's life —
/// REQUIRED (2 bytes LE) after [`ReferralTiersArg`]; `register_pool` re-bounds it (`TeamFeeOutOfRange`) and writes
/// `FloorPool.team_fee_bps` once. `#[derive(Default)]` gives 0, a zero-fee pool.
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TeamFeeArg {
    pub team_fee_bps: u16,
}

/// The encoded length of [`TeamFeeArg`].
pub const TEAM_FEE_ARG_LEN: usize = 2;

/// The at-floor sell multiple, per launch — REQUIRED (u32 LE)
/// after [`TeamFeeArg`]. The multiple of the far SELL skim a sell INTO the bid pays, in bps of a multiple (u32: 5× =
/// 50,000 does not fit a u16). `floor_launch` sends 50,000 for the three presets and Custom's own choice (1.5× for 0)
/// / `$FLOOR`'s 1.5×. `register_pool` bounds it to `MIN..=MAX_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS` (10,000..=50,000;
/// `SkimSellMultipleOutOfRange` otherwise, an explicit 0 included) and writes `FloorPool.skim_sell_at_floor_bps =
/// far · multiple / 10,000` ONCE — no setter, ever. `#[derive(Default)]` is 0, which `register_pool` REFUSES.
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkimSellMultipleArg {
    pub skim_sell_at_floor_multiple_bps: u32,
}

/// The encoded length of [`SkimSellMultipleArg`].
pub const SKIM_SELL_MULTIPLE_ARG_LEN: usize = 4;

/// The staircase: REQUIRED after [`SkimSellMultipleArg`] — `steps: u8 ‖ growth_bps: u16 LE`;
/// `register_pool` bounds them (`floor_math::stair::stair_shape_ok`, `StairShapeOutOfRange`; `steps` 0 refused — every
/// pool is a staircase) and writes `FloorPool.stair_steps / stair_growth_bps` ONCE.
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StairArg {
    pub steps: u8,
    pub growth_bps: u16,
}

/// The encoded length of [`StairArg`].
pub const STAIR_ARG_LEN: usize = 3;

/// The chain's structural checks on a referral chain, the ONE spelling both programs run:
/// `floor_launch::sale::creator_upline` (with the signer as `creator`, a defensive check of the creator's `TraderLink`)
/// and `floor_hook::register_pool::validate` (with the args' creator, defence in depth).
/// - no gaps: tier 2 set ⇒ tier 1 set; tier 3 set ⇒ tier 2 set;
/// - no set tier equals the creator;
/// - the set tiers are pairwise distinct (an off-chain referral tree may contain cycles).
///
/// The rule that the fee recipient is the deployer's own resolved wallet is tier 1's check and is NOT
/// repeated here: by "no gaps", any tier set ⇒ tier 1 set ⇒ that check already ran. The zero key is "not set".
pub fn referral_tiers_well_formed(creator: &[u8; 32], t1: &[u8; 32], t2: &[u8; 32], t3: &[u8; 32]) -> bool {
    const Z: [u8; 32] = [0u8; 32];
    let (s1, s2, s3) = (*t1 != Z, *t2 != Z, *t3 != Z);
    if (s2 && !s1) || (s3 && !s2) {
        return false;
    }
    if (s1 && t1 == creator) || (s2 && t2 == creator) || (s3 && t3 == creator) {
        return false;
    }
    if (s2 && t2 == t1) || (s3 && (t3 == t1 || t3 == t2)) {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_and_round_trip() {
        let a = RegisterPoolArgs {
            amm_pool: [1; 32],
            base_mint: [2; 32],
            quote_mint: crate::keys::WSOL_MINT,
            pool_initializer: [3; 32],
            mode: crate::constants::MODE_CUSTOM,
            skim_buy_far_bps: 250,
            skim_sell_far_bps: 250,
            creator_buy_bps: 25,
            creator_sell_bps: 25,
            holder_buy_bps: 0,
            holder_sell_bps: 0,
            deployer: [4; 32],
            pad: [0; 32],
            min_bid: crate::constants::MIN_BID_LAMPORTS,
            min_sellable: crate::constants::MIN_SELLABLE,
            platform_bps: crate::constants::PLATFORM_BPS,
            max_bounty: crate::constants::MAX_BOUNTY_LAMPORTS,
            bounty_bps: crate::constants::BOUNTY_BPS,
            referrer: [7; 32],
        };
        let bytes = borsh::to_vec(&a).unwrap();
        assert_eq!(bytes.len(), REGISTER_POOL_ARGS_LEN);
        assert_eq!(REGISTER_POOL_ARGS_LEN, 265);
        // The spans after `min_bid` are written out so a later field cannot leave a range silently reading the wrong bytes.
        assert_eq!(&bytes[213..221], &crate::constants::MIN_SELLABLE.to_le_bytes());
        assert_eq!(&bytes[221..223], &crate::constants::PLATFORM_BPS.to_le_bytes());
        assert_eq!(&bytes[223..231], &crate::constants::MAX_BOUNTY_LAMPORTS.to_le_bytes());
        assert_eq!(&bytes[231..233], &crate::constants::BOUNTY_BPS.to_le_bytes());
        assert_eq!(&bytes[233..265], &[7u8; 32]);
        assert_eq!(borsh::from_slice::<RegisterPoolArgs>(&bytes).unwrap(), a);
    }

    /// The whole `register_pool` data a `floor_launch` sends, in order: every trailing argument is required.
    /// Mutations: a field put back into the args (the length moves); a trailing argument made optional again
    /// (a short wire then decodes).
    #[test]
    fn the_trailing_arguments_are_required_and_in_order() {
        let tiers = ReferralTiersArg { referrer_t2: [2; 32], referrer_t3: [3; 32] };
        let team = TeamFeeArg { team_fee_bps: crate::constants::TEAM_POOL_FEE_BPS_MAX };
        let skim = SkimSellMultipleArg { skim_sell_at_floor_multiple_bps: crate::constants::PRESET_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS };
        let stair = StairArg { steps: 9, growth_bps: 9_500 };
        assert_eq!(borsh::to_vec(&tiers).unwrap().len(), REFERRAL_TIERS_ARG_LEN);
        assert_eq!(&borsh::to_vec(&tiers).unwrap()[..32], &[2u8; 32], "t2 first");
        assert_eq!(borsh::to_vec(&team).unwrap(), 50u16.to_le_bytes().to_vec());
        assert_eq!(borsh::to_vec(&skim).unwrap(), vec![0x98, 0x3A, 0, 0], "15,000 LE (the presets' 1.5x)");
        assert_eq!(borsh::to_vec(&stair).unwrap(), vec![9, 0x1c, 0x25]);
        let mut data = borsh::to_vec(&RegisterPoolArgs::default()).unwrap();
        for part in [borsh::to_vec(&tiers).unwrap(), borsh::to_vec(&team).unwrap(), borsh::to_vec(&skim).unwrap(), borsh::to_vec(&stair).unwrap()] {
            data.extend_from_slice(&part);
        }
        assert_eq!(data.len(), REGISTER_POOL_ARGS_LEN + REFERRAL_TIERS_ARG_LEN + TEAM_FEE_ARG_LEN + SKIM_SELL_MULTIPLE_ARG_LEN + STAIR_ARG_LEN);
        assert_eq!(data.len(), 338, "+ the 8-byte discriminator = 346 (347 with the optional owner kind)");
        let mut r = &data[..];
        assert_eq!(<RegisterPoolArgs as BorshDeserialize>::deserialize(&mut r).unwrap(), RegisterPoolArgs::default());
        assert_eq!(<ReferralTiersArg as BorshDeserialize>::deserialize(&mut r).unwrap(), tiers);
        assert_eq!(<TeamFeeArg as BorshDeserialize>::deserialize(&mut r).unwrap(), team);
        assert_eq!(<SkimSellMultipleArg as BorshDeserialize>::deserialize(&mut r).unwrap(), skim);
        assert_eq!(<StairArg as BorshDeserialize>::deserialize(&mut r).unwrap(), stair);
        assert!(r.is_empty());
        // a short tail is refused, never padded
        let mut short = &data[REGISTER_POOL_ARGS_LEN..REGISTER_POOL_ARGS_LEN + 63];
        assert!(<ReferralTiersArg as BorshDeserialize>::deserialize(&mut short).is_err());
        let mut none: &[u8] = &[];
        assert!(<StairArg as BorshDeserialize>::deserialize(&mut none).is_err(), "0 bytes is no longer 'not given'");
    }

    /// The safe default is the ZERO referrer — unreferred.
    #[test]
    fn the_default_referrer_is_zero_so_an_unsaid_launch_is_unreferred() {
        let d = RegisterPoolArgs::default();
        assert_eq!(d.referrer, [0u8; 32], "ZERO is the safe default");
        let bytes = borsh::to_vec(&d).unwrap();
        assert_eq!(bytes.len(), 265);
        assert!(bytes[233..].iter().all(|&b| b == 0));
        assert!(borsh::from_slice::<RegisterPoolArgs>(&bytes[..264]).is_err(), "short: refused, never padded");
    }

    /// The referral-chain rules, every refusal by itself. Mutations this kills: any one check dropped.
    #[test]
    fn the_tier_checks_are_the_specs() {
        let (c, a, b, d, z) = ([9u8; 32], [1u8; 32], [2u8; 32], [3u8; 32], [0u8; 32]);
        assert!(referral_tiers_well_formed(&c, &z, &z, &z), "unreferred");
        assert!(referral_tiers_well_formed(&c, &a, &z, &z), "one tier");
        assert!(referral_tiers_well_formed(&c, &a, &b, &z), "two tiers");
        assert!(referral_tiers_well_formed(&c, &a, &b, &d), "three tiers");
        assert!(!referral_tiers_well_formed(&c, &z, &b, &z), "gap: t2 without t1");
        assert!(!referral_tiers_well_formed(&c, &a, &z, &d), "gap: t3 without t2");
        assert!(!referral_tiers_well_formed(&c, &z, &z, &d), "gap: t3 alone");
        assert!(!referral_tiers_well_formed(&c, &c, &z, &z), "t1 is the creator");
        assert!(!referral_tiers_well_formed(&c, &a, &c, &z), "t2 is the creator");
        assert!(!referral_tiers_well_formed(&c, &a, &b, &c), "t3 is the creator");
        assert!(!referral_tiers_well_formed(&c, &a, &a, &z), "t2 == t1");
        assert!(!referral_tiers_well_formed(&c, &a, &b, &a), "t3 == t1");
        assert!(!referral_tiers_well_formed(&c, &a, &b, &b), "t3 == t2");
    }
}
