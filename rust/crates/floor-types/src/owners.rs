//! Team steps and the team buy.
//!
//! A staircase pool may carry ONE set of OWNER-held step tokens: `$FLOOR`'s TEAM steps (layout "top") or a platform
//! launch's TEAM-BUY steps (an equal share of every step, "blended"). The pool says which in `FloorPool.stair_owner`
//! (@639) and keeps the books here, in a TAIL appended to its `StairState` account (`["stair", floor_pool]` under
//! `floor_hook`) — so no keeper's account set moves: every ratchet already passes the `StairState`.
//!
//! ⛔ APPEND-ONLY: a plain `StairState` account is exactly `8 + 584` bytes. The tail starts at [`STAIR_OWNERS_OFFSET`]
//! and exists ONLY on an account created for an owner pool ([`STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS`]); a reader that
//! finds the account shorter reads "no owner steps". No field of `StairState` moved.

use bytemuck::{Pod, Zeroable};

use crate::layout::{Layout, StairState};

/// The tail's ACCOUNT offset: straight after the `StairState` account (discriminator + struct).
pub const STAIR_OWNERS_OFFSET: usize = 8 + <StairState as Layout>::SIZE;
/// `size_of::<StairOwners>()`.
pub const STAIR_OWNERS_SIZE: usize = 440;
/// A `StairState` account that carries the owner tail: 592 + 440 = 1,032 bytes.
pub const STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS: usize = STAIR_OWNERS_OFFSET + STAIR_OWNERS_SIZE;
/// [`StairOwners::version`] once graduation filled it.
pub const STAIR_OWNERS_VERSION: u8 = 1;

/// The owner books of a staircase pool. Every amount is raw units of the pool's own base / quote.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
pub struct StairOwners {
    /// 0 while the account is only sized (created ahead of graduation); [`STAIR_OWNERS_VERSION`] once graduation's
    /// `seed_inventory` filled it from `floor_launch`'s `StairOwnerArg`.
    pub version: u8,
    /// `STAIR_OWNER_TEAM` 1 / `STAIR_OWNER_TEAM_BUY` 2 (a copy of `FloorPool.stair_owner`).
    pub kind: u8,
    /// 1: the owner's standing tokens COUNT against the floor. BOTH kinds send 1 — `$FLOOR`'s team and a team buy — and
    /// `floor_hook::mech::owners::validate` refuses 0, so every withdrawal is FREE.
    pub counted: u8,
    /// The lowest step that held owner tokens at graduation — the FIRST team step to unlock (the cliffs' order).
    pub first_step: u8,
    /// The payee's share of what the owner's tokens SELL for, after FLOOR FIRST (team 4,000; deployer 8,000).
    pub payee_bps: u16,
    pub pad0: [u8; 2],
    /// Seconds after `anchor` the first owner step unlocks (team: 30 days; deployer: the lock, ≥ 90 days).
    pub first_unlock_secs: u32,
    /// Seconds between later unlocks, one step each, lowest first (team: 30 days); 0 = every step at the first (deployer).
    pub every_secs: u32,
    /// Unix seconds the clock starts: `$FLOOR`'s sale end; a deployer launch's graduation.
    pub anchor: i64,
    /// Unused, written zero. The payees are the TEAM slots of the pool's `FloorPool` account tail
    /// (`crate::slots::PayeeSlots`).
    pub pad1: [u8; 32],
    /// Unused, written zero.
    pub pad2: [u8; 32],
    /// Owner tokens placed at graduation.
    pub owner_total: u64,
    /// Lifetime: owner tokens bought off the steps.
    pub owner_sold: u64,
    /// Lifetime: owner tokens withdrawn to the payee.
    pub owner_withdrawn: u64,
    /// Lifetime: what the owner's sold tokens sold for (gross, quote; token-weighted per close).
    pub owner_proceeds: u64,
    /// Lifetime: booked to the payee (`FloorPool.owner_owed` grew by this).
    pub owed_total: u64,
    /// Lifetime: the payee share FLOOR FIRST forfeited to the split (never carried as a debt).
    pub forfeited: u64,
    /// Lifetime: owner steps' LP fee quote left for the vault (through the cut).
    pub fees_to_floor: u64,
    /// Lifetime: keepers' tips paid OUT OF the payee's owed by `sweep_stair_payee` (keepers earn a reward for paying
    /// teams).
    pub tips_paid: u64,
    /// Owner tokens still standing on step i (unsold and not withdrawn). Updated when the ratchet collects the step (or
    /// splits it off the reserve) and when a withdrawal takes them.
    pub tokens: [u64; 16],
    /// Tokens that LEFT the reserve for step i WHILE IT RODE IT — withdrawn (`withdraw_stair_tokens`) or SOLD at a
    /// reserve close (`floor_math::owners::ReserveClose::left`): the reserve's split and the next close's attribution
    /// read step i's planned amount less these (`floor_math::owners::owner_planned`), so a step part-sold at one close is
    /// never attributed its whole planned amount again.
    pub cut: [u64; 16],
    /// The unix time of the last `sweep_stair_payee` that PAID a tip; 0 = never tipped. A sweep within
    /// `TEAM_TIP_INTERVAL_SECS` of it tips nobody (at most one tipped sweep a day).
    pub last_tip_at: i64,
    /// Three u64 records, read and written through their accessors: `[0..8]` [`Self::held`] (the team's HELD balance),
    /// `[8..16]` [`Self::reserve_fed`], `[16..24]` [`Self::clamped`]. A zeroed account reads 0 in each.
    pub reserved: [u8; 24],
}

const _: () = assert!(core::mem::size_of::<StairOwners>() == STAIR_OWNERS_SIZE);
const _: () = assert!(core::mem::offset_of!(StairOwners, last_tip_at) == 408);
const _: () = assert!(core::mem::offset_of!(StairOwners, reserved) == 416);

impl StairOwners {
    /// The tail of a `StairState` account's bytes, or `None` when the account carries none.
    pub fn from_account(data: &[u8]) -> Option<Self> {
        if data.len() < STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS {
            return None;
        }
        Some(bytemuck::pod_read_unaligned(&data[STAIR_OWNERS_OFFSET..STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS]))
    }

    /// Write `self` into the tail (the account must carry one).
    pub fn store(&self, data: &mut [u8]) -> bool {
        if data.len() < STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS {
            return false;
        }
        data[STAIR_OWNERS_OFFSET..STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS].copy_from_slice(bytemuck::bytes_of(self));
        true
    }

    /// Owner tokens standing on the steps (Σ `tokens`).
    pub fn standing(&self) -> u64 {
        self.tokens.iter().fold(0u64, |a, &t| a.saturating_add(t))
    }

    fn word(&self, i: usize) -> u64 {
        u64::from_le_bytes(self.reserved[i * 8..i * 8 + 8].try_into().unwrap())
    }

    fn set_word(&mut self, i: usize, v: u64) {
        self.reserved[i * 8..i * 8 + 8].copy_from_slice(&v.to_le_bytes());
    }

    /// ⛔ THE TEAM'S HELD TOKENS (the EVM's `TeamSteps.held`): owner tokens whose step left the staircase UNSOLD (the
    /// market passed it, the clear line pushed it past its dear edge, or its re-open rounded to no liquidity; the
    /// reserve's likewise) are SET ASIDE in the pool's claims, still the owner's: never sold, never re-listed, never fed,
    /// COUNTED against the floor and paid to the payee by `withdraw_stair_tokens(STAIR_HELD_STEP)`. Mirrored in
    /// `FloorPool.ask_tokens_deployed`'s bytes (the base liability the claims check covers). Only ROUNDING on a placement
    /// that stood is still the protocol's ([`Self::clamped`]).
    pub fn held(&self) -> u64 {
        self.word(0)
    }

    pub fn set_held(&mut self, v: u64) {
        self.set_word(0, v)
    }

    /// ⛔ THE RESERVE'S PROTOCOL TOKENS: the tokens riding the reserve that are NOT any step's — the vault's
    /// bought-back tokens fed into it (`feed_vault_tokens`). A reserve close sells them FIRST
    /// (`floor_math::owners::reserve_close`) and no step's deficit is ever filled from them: fed tokens stay the protocol's.
    pub fn reserve_fed(&self) -> u64 {
        self.word(1)
    }

    pub fn set_reserve_fed(&mut self, v: u64) {
        self.set_word(1, v)
    }

    /// Lifetime: owner tokens the books dropped because no position held them — rounding at a placement (graduation's
    /// step 0, a step's re-open) or the reserve's cap (`cap_reserve`). Never paid, never sold.
    pub fn clamped(&self) -> u64 {
        self.word(2)
    }

    pub fn add_clamped(&mut self, v: u64) {
        let c = self.word(2).saturating_add(v);
        self.set_word(2, c)
    }

    /// ⛔ CONSERVATION (the EVM's `on + held + withdrawn + soldTokens + released + clamped == tokens`): every owner
    /// token placed at graduation is standing, sold, withdrawn, HELD or clamped —
    /// `owner_total == Σ tokens + owner_sold + owner_withdrawn + held + clamped` (a withdrawal's rounding left in a
    /// position is clamped here; the EVM books it `released`). A book that moves owner tokens anywhere else breaks it.
    pub fn conserved(&self) -> bool {
        let sum = self.standing() as u128
            + self.owner_sold as u128
            + self.owner_withdrawn as u128
            + self.held() as u128
            + self.clamped() as u128;
        sum == self.owner_total as u128
    }
}

// ---- the wire: what `floor_launch` hands `seed_inventory` at graduation ------------------------------------------------

/// ⛔ `seed_inventory`'s trailing, OPTIONAL `owner` argument, AFTER `rent_owner` (sent in full first): **0 bytes** → no
/// owner steps (every launch without a team buy); **[`STAIR_OWNER_ARG_LEN`] bytes** → the books' config; anything
/// between → an error, never a silent truncation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StairOwnerArg {
    pub kind: u8,
    pub counted: u8,
    pub payee_bps: u16,
    pub first_unlock_secs: u32,
    pub every_secs: u32,
    /// 0 = the graduation's own clock (a deployer launch).
    pub anchor: i64,
    /// Team: the top `owner_tokens` of the steps are the team's. Deployer: EXTRA tokens deposited beside the ladder, blended.
    pub owner_tokens: u64,
    /// Read ONLY for `$FLOOR`'s team (`STAIR_OWNER_TEAM`), whose ONE payee (the
    /// beneficiary or the pending handle fixed at `init_genesis`) the hook writes as team slot 0 at 10,000 bps when it fills
    /// the books. A team buy's payees are the slots `create_launch` registered; it sends zeros here.
    pub payee: [u8; 32],
    pub payee_pending_id: [u8; 32],
    /// The pending payee's platform (1..=4, the one meaning of a slot's platform byte), written into team slot 0; 0
    /// with a wallet payee and on a team buy. The last byte of the wire (92 → 93 bytes).
    pub payee_platform: u8,
}

pub const STAIR_OWNER_ARG_LEN: usize = 1 + 1 + 2 + 4 + 4 + 8 + 8 + 32 + 32 + 1;

impl StairOwnerArg {
    pub fn to_bytes(&self) -> [u8; STAIR_OWNER_ARG_LEN] {
        let mut b = [0u8; STAIR_OWNER_ARG_LEN];
        b[0] = self.kind;
        b[1] = self.counted;
        b[2..4].copy_from_slice(&self.payee_bps.to_le_bytes());
        b[4..8].copy_from_slice(&self.first_unlock_secs.to_le_bytes());
        b[8..12].copy_from_slice(&self.every_secs.to_le_bytes());
        b[12..20].copy_from_slice(&self.anchor.to_le_bytes());
        b[20..28].copy_from_slice(&self.owner_tokens.to_le_bytes());
        b[28..60].copy_from_slice(&self.payee);
        b[60..92].copy_from_slice(&self.payee_pending_id);
        b[92] = self.payee_platform;
        b
    }
}

#[allow(clippy::result_unit_err)]
pub fn stair_owner_from_trailing(bytes: &[u8]) -> Result<Option<StairOwnerArg>, ()> {
    match bytes.len() {
        0 => Ok(None),
        n if n < STAIR_OWNER_ARG_LEN => Err(()),
        _ => {
            let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
            let u32_at = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
            let u64_at = |i: usize| u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
            let mut payee = [0u8; 32];
            payee.copy_from_slice(&bytes[28..60]);
            let mut pending = [0u8; 32];
            pending.copy_from_slice(&bytes[60..92]);
            Ok(Some(StairOwnerArg {
                kind: bytes[0],
                counted: bytes[1],
                payee_bps: u16_at(2),
                first_unlock_secs: u32_at(4),
                every_secs: u32_at(8),
                anchor: u64_at(12) as i64,
                owner_tokens: u64_at(20),
                payee,
                payee_pending_id: pending,
                payee_platform: bytes[92],
            }))
        }
    }
}

/// ⛔ `register_pool`'s SEVENTH trailing, OPTIONAL argument, AFTER `StairArg` (sent in full first): **0 bytes** → no owner
/// steps; **1 byte** → `FloorPool.stair_owner` (only `STAIR_OWNER_TEAM_BUY` is admitted there: `$FLOOR`'s team kind is
/// written by `set_genesis_stair`, never at registration).
pub const STAIR_OWNER_KIND_ARG_LEN: usize = 1;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{STAIR_OWNER_TEAM_BUY, STAIR_OWNER_TEAM};

    #[test]
    fn the_tail_sits_after_the_stair_state_and_is_440_bytes() {
        assert_eq!(STAIR_OWNERS_OFFSET, 592);
        assert_eq!(STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS, 1_032);
        assert_eq!(core::mem::offset_of!(StairOwners, anchor), 16);
        assert_eq!(core::mem::offset_of!(StairOwners, pad1), 24);
        assert_eq!(core::mem::offset_of!(StairOwners, owner_total), 88);
        assert_eq!(core::mem::offset_of!(StairOwners, tokens), 152);
        assert_eq!(core::mem::offset_of!(StairOwners, cut), 280);
        assert_eq!(core::mem::offset_of!(StairOwners, last_tip_at), 408);
        assert_eq!(core::mem::offset_of!(StairOwners, reserved), 416);
        assert_eq!(core::mem::size_of::<StairOwners>(), 440, "the tail never grows: last_tip_at came out of reserved");
        assert_eq!((STAIR_OWNER_TEAM, STAIR_OWNER_TEAM_BUY), (1, 2));
    }

    #[test]
    fn a_short_account_has_no_tail_and_the_arg_round_trips() {
        assert!(StairOwners::from_account(&[0u8; 592]).is_none());
        let mut data = vec![0u8; STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS];
        let mut o = StairOwners { kind: 1, payee_bps: 4_000, ..Default::default() };
        o.tokens[3] = 7;
        o.last_tip_at = 1_790_000_000;
        assert!(o.store(&mut data));
        assert_eq!(&data[STAIR_OWNERS_OFFSET + 408..STAIR_OWNERS_OFFSET + 416], &1_790_000_000i64.to_le_bytes());
        assert_eq!(StairOwners::from_account(&data).unwrap(), o);
        assert_eq!(o.standing(), 7);
        let a = StairOwnerArg {
            kind: 2,
            counted: 1,
            payee_bps: 8_000,
            first_unlock_secs: 90 * 86_400,
            every_secs: 0,
            anchor: -5,
            owner_tokens: 40_000_000_000_000,
            payee: [9u8; 32],
            payee_pending_id: [8u8; 32],
            payee_platform: 3,
        };
        // the pending payee's platform is the LAST byte, @92
        assert_eq!(STAIR_OWNER_ARG_LEN, 93);
        assert_eq!(a.to_bytes()[92], 3);
        assert_eq!(stair_owner_from_trailing(&a.to_bytes()), Ok(Some(a)));
        assert_eq!(stair_owner_from_trailing(&a.to_bytes()[..92]), Err(()), "the 92-byte wire is refused");
        assert_eq!(stair_owner_from_trailing(&[]), Ok(None));
        assert_eq!(stair_owner_from_trailing(&[1, 2, 3]), Err(()));
    }
}
