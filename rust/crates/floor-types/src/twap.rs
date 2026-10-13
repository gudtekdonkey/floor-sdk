//! THE POOL-PRICE TWAP: a manipulation-resistant time-weighted average of each FLOOR pool's OWN tick,
//! Uniswap-v3-observation style, the Solana twin of the EVM hook's per-pool oracle.
//!
//! ⛔ **Where it lives: the TAIL of the `FloorPool` ACCOUNT, bytes `[792, 1008)`** — AFTER the 784-byte
//! struct and its discriminator, never inside it. `FloorPool` has no `reserved` left, and the swap
//! callback receives only the hook accounts fixed in the hook's interface header
//! (`HookConfig.callback_accounts`, copied into every AMM `Pool` at `initialize_pool`), so a separate
//! `["twap", pool]` PDA could never be written by the swap callback without approving the hook as a
//! different interface. `register_pool` allocates the longer account.
//!
//! The semantics — ⛔ IDENTICAL to the EVM side's (one spelling here, [`TwapOracle::observe`] / [`TwapOracle::twap_tick`]):
//! - A WRITE at unix second `now` with the pool's CURRENT tick — read BEFORE the caller (a swap, the
//!   ratchet) moves the price. The first write ever initialises (`cum = 0`, `timestamp = now`,
//!   `last_tick = tick`, ring index 0 = `(now, 0)`). A write whose `now` equals the stored timestamp
//!   is a NO-OP: that is what makes a swap-then-swap-back inside one second (one transaction) move
//!   nothing. Otherwise `cum += tick × (now − timestamp)`, `timestamp = now`, `last_tick = tick`, and a
//!   checkpoint `(now, cum)` is appended (the newest index advancing mod 8) when the newest one is at
//!   least [`TWAP_CHECKPOINT_SPACING_SECS`] old.
//! - ⛔ There is NO "accrue the stored `last_tick`": the current pre-move tick IS the price that stood
//!   since the last write, because everything that moves the price writes first. (Accruing `last_tick`
//!   lags one interval and is exploitable: a 1-second excursion would be weighted over the whole gap to
//!   the next write — test `a_one_second_excursion_weighs_one_second`.) `last_tick` is the tick at the
//!   last effective write (the price at the START of that second), kept because the EVM carries it.
//! - A READ `twap_tick(window)`: `window == 0` → the live tick. Never written →
//!   [`TwapError::Uninitialized`]. If `now − timestamp >= window` → the live tick EXACTLY (nothing has
//!   written, so nothing has moved the price, for the whole window). Otherwise `cum_now = cum + live ×
//!   (now − timestamp)`, and the ring is walked newest → oldest for the FIRST checkpoint whose age is
//!   at least `window`: `floor((cum_now − cum_cp) / age)`, rounded toward −∞. None old enough →
//!   [`TwapError::WindowTooLong`]. ⚠ The span is therefore `age`, which may exceed `window` by up to
//!   one checkpoint gap (≥ 300 s; more after a quiet spell).
//! - ORIENTATION: the ring holds RAW pool ticks and the read returns the raw tick. The EVM normalises
//!   a pool whose quote is its SECOND mint (`t → −t − 1`, on the sum as `d → −d − span`); Solana's
//!   rule for that case is that the quote is token0 on EVERY FLOOR pool (`register_pool` refuses
//!   anything else, and the ratchet refuses `quote_is_token1 != 0` as `InvariantViolated`), so
//!   the raw tick IS the normalised one, the EVM's identity case. `twap_tick` refuses a
//!   `quote_is_token1 != 0` pool the same way; there is no flip here and there must never be one.
//! - Arithmetic wraps like Uniswap v3's: the cumulative is `i64` (the EVM's `int56`) with wrapping
//!   adds and subtractions, timestamps are `u32` (`uint32 block.timestamp`) with wrapping subtraction —
//!   a write can never fail the trader's swap, and a read over a wrapped cumulative is still exact.
//! - ⚠ ONE DELIBERATE DIVERGENCE: a write whose `now` is BEHIND the stored timestamp (a wrapping
//!   difference that reads negative as `i32`) is a no-op here, like the same second. EVM
//!   `block.timestamp` never goes backwards, so the two agree on every sequence the EVM can see;
//!   Solana's `Clock.unix_timestamp` is a stake-weighted estimate, and without the guard one backwards
//!   step would accrue ~2^32 seconds.
//! The staircase's cushion reads it through [`TwapOracle::settled_tick`].

use bytemuck::{Pod, Zeroable};

use crate::layout::{FloorPool, Layout};

/// Checkpoints in the ring. ⛔ The EVM side uses the SAME 8 (the same ring size).
pub const TWAP_CHECKPOINTS: usize = 8;
/// A checkpoint is appended when the newest is at least this many seconds old. ⛔ The EVM side
/// uses the SAME 300 s.
pub const TWAP_CHECKPOINT_SPACING_SECS: u32 = 300;
/// ⛔ THE CUSHION'S WINDOW: the staircase's cushion is placed under `min(spot, TWAP)` over this many seconds
/// ([`TwapOracle::settled_tick`]), so a pump + ratchet + dump inside one transaction (or one second) cannot raise it.
/// 900 s = three checkpoint gaps, inside the ring's reach (8 × 300 s); a younger pool reads over its oldest checkpoint
/// instead. The EVM twin is `FloorPolicy.STAIR_CUSHION_TWAP_WINDOW`. A longer read leaves the cushion far under a
/// fast-rising price, and every pump-and-hold probe still loses at 15 minutes.
/// ⚠ One window for every staircase pool: `$FLOOR`'s cushion reads it too.
pub const CUSHION_TWAP_WINDOW_SECS: u32 = 900;

/// [`TwapOracle::version`] once initialised; 0 = never written (lazy init on the first write).
pub const TWAP_VERSION: u8 = 1;
/// The oracle's ACCOUNT offset: straight after the `FloorPool` account (discriminator + struct).
pub const TWAP_ORACLE_OFFSET: usize = FloorPool::ACCOUNT_LEN;
/// `size_of::<TwapOracle>()`.
pub const TWAP_ORACLE_SIZE: usize = 216;
/// A `FloorPool` account that carries the oracle: 792 + 216 = 1,008 bytes.
pub const FLOOR_POOL_ACCOUNT_LEN_WITH_TWAP: usize = TWAP_ORACLE_OFFSET + TWAP_ORACLE_SIZE;

/// The current observation (16 B). Field order is by alignment, not the EVM's slot order.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
pub struct TwapObservation {
    /// Σ tick × seconds since initialisation, up to [`Self::timestamp`].
    pub tick_cumulative: i64,
    /// Unix seconds of the last EFFECTIVE write.
    pub timestamp: u32,
    /// The pool's tick at the last effective write (the start of that second). Informational: the
    /// accrual uses the CURRENT tick (module docs).
    pub last_tick: i32,
}

/// One ring entry (16 B).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
pub struct TwapCheckpoint {
    pub tick_cumulative: i64,
    pub timestamp: u32,
    pub pad: u32,
}

/// The oracle, 216 B, at [`TWAP_ORACLE_OFFSET`] of a `FloorPool` account. ⛔ APPEND-ONLY: carve new
/// fields out of `reserved`, never move one.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct TwapOracle {
    /// 0 = never written; [`TWAP_VERSION`] after the first write.
    pub version: u8,
    /// Ring index of the newest checkpoint.
    pub newest: u8,
    /// Checkpoints written so far, at most [`TWAP_CHECKPOINTS`].
    pub count: u8,
    pub pad0: [u8; 5],
    pub current: TwapObservation,
    pub checkpoints: [TwapCheckpoint; TWAP_CHECKPOINTS],
    /// ⛔ THE OLDER SURPLUS HELD (carved from `reserved`): 1 when the last ratchet
    /// withheld backing the vault held before its new money because the SETTLED price's stand line capped it
    /// (`floor_hook::mech::ratchet::run`); the ratchet's no-op gate then lets a keeper release it once this oracle's settled
    /// tick has caught up -- no trade needed. Written only by the ratchet; every oracle write keeps it.
    pub older_surplus_held: u8,
    /// ⛔ THE CUSHION TOP, CACHED FOR THE SWAP PATH (the gap buy skim; BEHIND A SWITCH, default OFF —
    /// `floor_math::fees::STAIR_GAP_SKIM`; carved from `reserved`): 1 when the last ratchet left a cushion standing,
    /// its top then in [`Self::cushion_top_tick`]. Written only by the ratchet (`floor_hook::mech::ratchet::run`, after
    /// the trim), and only while the switch is ON; every oracle write keeps it. Read by `before_swap` / `after_swap` and the
    /// quoter (`carpenter_quote::state::decode_cushion_top`) — `after_swap` has no `StairState`.
    pub cushion_top_set: u8,
    pub pad1: [u8; 2],
    /// The cushion's TOP in price — its LOWER tick, `StairState.cushion_lower` (a dearer price is a lower tick on every
    /// FLOOR pool, whose quote is token0) — as the last ratchet placed it; read only while
    /// [`Self::cushion_top_set`] is 1.
    pub cushion_top_tick: i32,
    /// ⛔ THE CUSHION'S NET-FLOW METER (BEHIND A SWITCH —
    /// `floor_math::netflow::STAIR_CUSHION_NETFLOW_BPS`; carved from `reserved`, so the account does NOT grow): `a` = Σ quote ×
    /// tick, an `i128` stored little-endian as bytes (an `i128` field would raise this struct's alignment to 16 and its size to
    /// 224); see [`Self::net_flow`]. Written only by `after_swap` (every swap that reaches the hook) while the switch is ON;
    /// read by the ratchet's cushion target. Every oracle write keeps it. All zero while the switch is OFF (never written).
    pub flow_a: [u8; 16],
    /// The meter's weight `b` = Σ quote (raw quote units); 0 = empty.
    pub flow_b: u64,
    pub reserved: [u8; 32],
}

const _: () = assert!(core::mem::size_of::<TwapObservation>() == 16);
const _: () = assert!(core::mem::size_of::<TwapCheckpoint>() == 16);
const _: () = assert!(core::mem::size_of::<TwapOracle>() == TWAP_ORACLE_SIZE);
const _: () = assert!(TWAP_ORACLE_OFFSET == 792 && TWAP_ORACLE_OFFSET % 8 == 0);
const _: () = assert!(FLOOR_POOL_ACCOUNT_LEN_WITH_TWAP == 1_008);

/// What a write did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Observed {
    /// The first write: `cum = 0`, one checkpoint at `now`.
    Initialized,
    /// Accrued the elapsed span; `checkpointed` when a ring entry was appended.
    Accrued { checkpointed: bool },
    /// Same second as the last effective write (or a clock behind it): nothing changed.
    SameSecond,
}

/// Why a read has no answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TwapError {
    /// The oracle was never written (or the account has no tail).
    Uninitialized,
    /// No checkpoint is at least `window` seconds old.
    WindowTooLong,
}

/// `a / b` rounded toward negative infinity (`b > 0`).
pub fn floor_div(a: i64, b: i64) -> i64 {
    let q = a / b;
    if (a % b != 0) && ((a < 0) != (b < 0)) {
        q - 1
    } else {
        q
    }
}

impl TwapOracle {
    /// The oracle a zero-filled tail decodes to: version 0, never written.
    pub fn empty() -> Self {
        Zeroable::zeroed()
    }

    pub fn is_initialized(&self) -> bool {
        self.version != 0 && self.count != 0
    }

    /// The newest checkpoint (meaningless before initialisation).
    pub fn newest_checkpoint(&self) -> TwapCheckpoint {
        self.checkpoints[self.newest as usize % TWAP_CHECKPOINTS]
    }

    /// THE WRITE. `tick` is the pool's current tick before whatever is about to move it.
    pub fn observe(&mut self, now: u32, tick: i32) -> Observed {
        if !self.is_initialized() {
            // the ratchet's flags are not the oracle's: a (re)initialisation keeps them
            let (held, set, top) = (self.older_surplus_held, self.cushion_top_set, self.cushion_top_tick);
            let (fa, fb) = (self.flow_a, self.flow_b);
            *self = Self::empty();
            (self.older_surplus_held, self.cushion_top_set, self.cushion_top_tick) = (held, set, top);
            (self.flow_a, self.flow_b) = (fa, fb);
            self.version = TWAP_VERSION;
            self.current = TwapObservation { tick_cumulative: 0, timestamp: now, last_tick: tick };
            self.checkpoints[0] = TwapCheckpoint { tick_cumulative: 0, timestamp: now, pad: 0 };
            self.newest = 0;
            self.count = 1;
            return Observed::Initialized;
        }
        let elapsed = now.wrapping_sub(self.current.timestamp);
        // ⛔ The same-second no-op (and the one divergence: a clock behind the last write).
        if elapsed == 0 || (elapsed as i32) < 0 {
            return Observed::SameSecond;
        }
        self.current.tick_cumulative = self.current.tick_cumulative.wrapping_add((tick as i64).wrapping_mul(elapsed as i64));
        self.current.timestamp = now;
        self.current.last_tick = tick;
        let checkpointed = now.wrapping_sub(self.newest_checkpoint().timestamp) >= TWAP_CHECKPOINT_SPACING_SECS;
        if checkpointed {
            let next = (self.newest as usize + 1) % TWAP_CHECKPOINTS;
            self.checkpoints[next] =
                TwapCheckpoint { tick_cumulative: self.current.tick_cumulative, timestamp: now, pad: 0 };
            self.newest = next as u8;
            self.count = (self.count as usize + 1).min(TWAP_CHECKPOINTS) as u8;
        }
        Observed::Accrued { checkpointed }
    }

    /// The cumulative at `now`, the span since the last write extrapolated with `live_tick` (wrapping).
    pub fn cumulative_at(&self, now: u32, live_tick: i32) -> i64 {
        let c = self.current;
        let since = now.wrapping_sub(c.timestamp);
        c.tick_cumulative.wrapping_add((live_tick as i64).wrapping_mul(since as i64))
    }

    /// THE READ: the time-weighted average RAW tick over the last `age >= window` seconds (module docs).
    pub fn twap_tick(&self, now: u32, live_tick: i32, window: u32) -> Result<i32, TwapError> {
        if window == 0 {
            return Ok(live_tick);
        }
        if !self.is_initialized() {
            return Err(TwapError::Uninitialized);
        }
        // Nothing has written — so nothing has moved the price — for the whole window.
        if now.wrapping_sub(self.current.timestamp) >= window {
            return Ok(live_tick);
        }
        let cum_now = self.cumulative_at(now, live_tick);
        let count = (self.count as usize).min(TWAP_CHECKPOINTS);
        for back in 0..count {
            let i = (self.newest as usize + TWAP_CHECKPOINTS - back) % TWAP_CHECKPOINTS;
            let cp = self.checkpoints[i];
            let age = now.wrapping_sub(cp.timestamp);
            if age >= window {
                let avg = floor_div(cum_now.wrapping_sub(cp.tick_cumulative), age as i64);
                // An average of ticks is a tick; the clamp only guards a corrupted account.
                return Ok(avg.clamp(i32::MIN as i64, i32::MAX as i64) as i32);
            }
        }
        Err(TwapError::WindowTooLong)
    }

    /// THE SETTLED TICK (the staircase's cushion reads it): [`Self::twap_tick`] over `window`, and when
    /// the ring is younger than `window`, over its OLDEST checkpoint's age instead; when even that is 0 (the oracle was
    /// initialised this very second) the tick at the START of this second (`current.last_tick`, written before
    /// anything moved the price). ⛔ Never the live tick when anything wrote this second: every price move writes
    /// first, and a same-second write is a no-op, so a price moved inside this transaction (or this second) weighs
    /// NOTHING here. The live tick only when nothing has written for the whole window (then nothing moved it).
    pub fn settled_tick(&self, now: u32, live_tick: i32, window: u32) -> Result<i32, TwapError> {
        match self.twap_tick(now, live_tick, window) {
            Err(TwapError::WindowTooLong) => {
                let count = (self.count as usize).clamp(1, TWAP_CHECKPOINTS);
                let oldest = self.checkpoints[(self.newest as usize + TWAP_CHECKPOINTS + 1 - count) % TWAP_CHECKPOINTS];
                let age = now.wrapping_sub(oldest.timestamp);
                if age > 0 && (age as i32) > 0 {
                    self.twap_tick(now, live_tick, age)
                } else {
                    Ok(self.current.last_tick)
                }
            }
            r => r,
        }
    }

    /// The oracle in a `FloorPool` ACCOUNT's bytes, or `None` when the account has no tail (an
    /// account shorter than [`FLOOR_POOL_ACCOUNT_LEN_WITH_TWAP`]). Copies (`pod_read_unaligned`).
    pub fn from_account(data: &[u8]) -> Option<Self> {
        data.get(TWAP_ORACLE_OFFSET..FLOOR_POOL_ACCOUNT_LEN_WITH_TWAP).map(bytemuck::pod_read_unaligned)
    }

    /// THE CUSHION TOP (the gap buy skim's cache) in a `FloorPool` account's bytes: `Some(tick)` when the last ratchet
    /// left a cushion standing, `None` otherwise (or with no tail). The one spelling the program and the quoter read.
    pub fn cushion_top_of_account(data: &[u8]) -> Option<i32> {
        let o = Self::from_account(data)?;
        (o.cushion_top_set != 0).then_some(o.cushion_top_tick)
    }

    /// THE NET-FLOW METER as `(a, b)` (`floor_math::netflow::NetFlow`'s two fields; this crate does not depend on
    /// `floor-math`).
    pub fn net_flow(&self) -> (i128, u64) {
        (i128::from_le_bytes(self.flow_a), self.flow_b)
    }

    /// Store the meter `(a, b)`.
    pub fn set_net_flow(&mut self, a: i128, b: u64) {
        self.flow_a = a.to_le_bytes();
        self.flow_b = b;
    }

    /// Write this oracle into a `FloorPool` account's tail; `false` (nothing written) without one.
    pub fn store(&self, data: &mut [u8]) -> bool {
        match data.get_mut(TWAP_ORACLE_OFFSET..FLOOR_POOL_ACCOUNT_LEN_WITH_TWAP) {
            Some(tail) => {
                tail.copy_from_slice(bytemuck::bytes_of(self));
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh(now: u32, tick: i32) -> TwapOracle {
        let mut o = TwapOracle::empty();
        assert_eq!(o.observe(now, tick), Observed::Initialized);
        o
    }

    #[test]
    fn layout_is_pinned() {
        assert_eq!(core::mem::offset_of!(TwapOracle, current), 8);
        assert_eq!(core::mem::offset_of!(TwapOracle, checkpoints), 24);
        assert_eq!(core::mem::offset_of!(TwapOracle, older_surplus_held), 152);
        // the gap buy skim's cache, carved from `reserved`
        assert_eq!(core::mem::offset_of!(TwapOracle, cushion_top_set), 153);
        assert_eq!(core::mem::offset_of!(TwapOracle, cushion_top_tick), 156);
        // the net-flow meter, carved from `reserved`
        assert_eq!(core::mem::offset_of!(TwapOracle, flow_a), 160);
        assert_eq!(core::mem::offset_of!(TwapOracle, flow_b), 176);
        assert_eq!(core::mem::offset_of!(TwapOracle, reserved), 184);
        assert_eq!(core::mem::offset_of!(TwapObservation, timestamp), 8);
        assert_eq!(core::mem::offset_of!(TwapObservation, last_tick), 12);
        assert_eq!((TWAP_ORACLE_OFFSET, FLOOR_POOL_ACCOUNT_LEN_WITH_TWAP), (792, 1_008));
        assert_eq!((TWAP_CHECKPOINTS, TWAP_CHECKPOINT_SPACING_SECS), (8, 300));
    }

    #[test]
    fn floor_div_rounds_toward_negative_infinity() {
        assert_eq!(floor_div(7, 2), 3);
        assert_eq!(floor_div(-7, 2), -4);
        assert_eq!(floor_div(-8, 2), -4);
        assert_eq!(floor_div(0, 5), 0);
        assert_eq!(floor_div(-1, 300), -1);
    }

    #[test]
    fn the_first_write_initialises() {
        let o = fresh(1_000, -42);
        assert!(o.is_initialized());
        assert_eq!(o.current, TwapObservation { tick_cumulative: 0, timestamp: 1_000, last_tick: -42 });
        assert_eq!((o.count, o.newest), (1, 0));
        assert_eq!(o.checkpoints[0], TwapCheckpoint { tick_cumulative: 0, timestamp: 1_000, pad: 0 });
    }

    /// (1) A swap then a swap back in the SAME second moves nothing: the second write is a no-op.
    #[test]
    fn a_same_second_round_trip_does_not_move_it() {
        let mut o = fresh(1_000, 100);
        // Held at 100 for 600 s; at 1_600 the manipulator swaps up to 50_000 and back down in one tx.
        assert_eq!(o.observe(1_600, 100), Observed::Accrued { checkpointed: true }); // pre-swap tick 100
        let before = o;
        assert_eq!(o.observe(1_600, 50_000), Observed::SameSecond); // the swap back's pre tick
        assert_eq!(o, before);
        // Price is back at 100; any read later sees 100 exactly.
        assert_eq!(o.twap_tick(2_000, 100, 600), Ok(100));
        assert_eq!(o.twap_tick(1_600, 100, 600), Ok(100));
    }

    /// The literal "accrue the stored last_tick" rule would weight a 1-second excursion over the
    /// whole following gap; this spelling weights it one second.
    #[test]
    fn a_one_second_excursion_weighs_one_second() {
        let mut o = fresh(0, 0);
        o.observe(1_000, 0); // swap at 1_000 pushes to 3_000
        o.observe(1_001, 3_000); // swap back at 1_001 (pre tick 3_000) returns to 0
        o.observe(2_000, 0);
        // Over [0, 2_000]: 3_000 stood exactly 1 s.
        assert_eq!(o.current.tick_cumulative, 3_000);
        assert_eq!(o.cumulative_at(2_000, 0), 3_000);
    }

    /// (2) A price held N seconds moves the TWAP by exactly the time-weighted amount.
    #[test]
    fn a_held_price_moves_it_by_the_time_weighted_amount() {
        let mut o = fresh(10_000, 1_000);
        // 10_000..10_300 at 1_000; a swap at 10_300 moves the price to 4_000, held 900 s.
        assert_eq!(o.observe(10_300, 1_000), Observed::Accrued { checkpointed: true });
        assert_eq!(o.observe(11_200, 4_000), Observed::Accrued { checkpointed: true });
        assert_eq!(o.current.tick_cumulative, 1_000 * 300 + 4_000 * 900);
        // Window 1_200 from 11_200: the checkpoint at 10_000 → (300·1000 + 900·4000) / 1200 = 3_250.
        assert_eq!(o.twap_tick(11_200, 4_000, 1_200), Ok(3_250));
        // Window 900: the newest checkpoint ≥ 900 s old is 10_300 → 4_000 exactly.
        assert_eq!(o.twap_tick(11_200, 4_000, 900), Ok(4_000));
        // Extrapolation with the CURRENT tick: 100 s later at 4_000, window 1_000 → cp 10_300 (1_000 s).
        assert_eq!(o.twap_tick(11_300, 4_000, 1_000), Ok(4_000));
        // …and window 1_300 → cp 10_000: (300·1000 + 1000·4000)/1300 = 3307.69 → 3_307.
        assert_eq!(o.twap_tick(11_300, 4_000, 1_300), Ok(3_307));
    }

    #[test]
    fn negative_ticks_round_down() {
        let mut o = fresh(0, -1);
        o.observe(300, -1);
        o.observe(301, -2); // -1·300 then -2·1
        // (−300 − 2) / 301 = −1.0066 → −2 (toward −∞), over the checkpoint at 0.
        assert_eq!(o.twap_tick(301, -2, 301), Ok(-2));
        // Over the checkpoint at 300 alone: (−302 + 300) / 1 = −2.
        assert_eq!(o.twap_tick(301, -2, 1), Ok(-2));
        // A read 99 s later still at −2: (−302 − 2·99) / 400 = −1.25 → −2.
        assert_eq!(o.twap_tick(400, -2, 400), Ok(-2));
    }

    #[test]
    fn window_zero_is_the_current_tick_and_a_long_window_is_refused() {
        let o = fresh(5_000, 7);
        assert_eq!(o.twap_tick(5_000, 123, 0), Ok(123));
        assert_eq!(o.twap_tick(5_100, 7, 200), Err(TwapError::WindowTooLong));
        assert_eq!(o.twap_tick(5_200, 7, 200), Ok(7));
        assert_eq!(TwapOracle::empty().twap_tick(1, 1, 1), Err(TwapError::Uninitialized));
        assert_eq!(TwapOracle::empty().twap_tick(1, 9, 0), Ok(9));
    }

    #[test]
    fn checkpoints_are_spaced_and_the_ring_keeps_the_newest_eight() {
        let mut o = fresh(0, 10);
        // Writes every 100 s: a checkpoint only every 300 s.
        let mut t = 0u32;
        for _ in 0..60 {
            t += 100;
            let r = o.observe(t, 10);
            assert_eq!(r, Observed::Accrued { checkpointed: t % 300 == 0 }, "t = {t}");
        }
        assert_eq!(o.count as usize, TWAP_CHECKPOINTS);
        let mut ts: Vec<u32> = o.checkpoints.iter().map(|c| c.timestamp).collect();
        ts.sort_unstable();
        assert_eq!(ts, vec![3_900, 4_200, 4_500, 4_800, 5_100, 5_400, 5_700, 6_000]);
        assert_eq!(o.newest_checkpoint().timestamp, 6_000);
        // The oldest reachable window is 6_000 − 3_900 = 2_100 s.
        assert_eq!(o.twap_tick(6_000, 10, 2_100), Ok(10));
        assert_eq!(o.twap_tick(6_000, 10, 2_101), Err(TwapError::WindowTooLong));
        for c in o.checkpoints {
            assert_eq!(c.tick_cumulative, 10 * c.timestamp as i64);
        }
    }

    #[test]
    fn a_clock_behind_the_last_write_is_a_no_op() {
        let mut o = fresh(1_000, 5);
        let before = o;
        assert_eq!(o.observe(999, 6), Observed::SameSecond);
        assert_eq!(o, before);
        // A read behind the last write: the wrapping age reads huge, so it is the live tick (the EVM rule).
        assert_eq!(o.twap_tick(999, 6, 60), Ok(6));
        // And the clock catching up accrues from the stored timestamp, not from the backwards reading.
        assert_eq!(o.observe(1_010, 5), Observed::Accrued { checkpointed: false });
        assert_eq!(o.current.tick_cumulative, 5 * 10);
    }

    /// No write for at least `window` seconds means nothing moved the price: the live tick, exactly —
    /// whatever the older cumulative says.
    #[test]
    fn a_window_with_no_write_in_it_is_the_live_tick_exactly() {
        let mut o = fresh(0, -500);
        o.observe(1_000, -500); // −500 over [0, 1_000]
        assert_eq!(o.twap_tick(1_600, 42, 600), Ok(42), "600 s since the last write, window 600");
        assert_eq!(o.twap_tick(1_600, 42, 601), Ok(floor_div(-500 * 1_000 + 42 * 600, 1_600) as i32));
        assert_eq!(o.twap_tick(1_599, 42, 600), Ok(floor_div(-500 * 1_000 + 42 * 599, 1_599) as i32));
    }

    /// The cumulative wraps like v3's and a read across the wrap is still exact.
    #[test]
    fn the_cumulative_wraps_and_still_reads() {
        let mut o = fresh(0, 0);
        let near = i64::MAX - 1_000;
        o.current.tick_cumulative = near;
        o.checkpoints[0].tick_cumulative = near;
        let t = 887_272;
        assert_eq!(o.observe(600, t), Observed::Accrued { checkpointed: true });
        assert!(o.current.tick_cumulative < 0, "it wrapped");
        assert_eq!(o.current.tick_cumulative, near.wrapping_add(t as i64 * 600));
        assert_eq!(o.twap_tick(600, t, 600), Ok(t));
        assert_eq!(o.twap_tick(900, -t, 900), Ok(floor_div(t as i64 * 600 - t as i64 * 300, 900) as i32));
    }

    /// `u32` timestamps wrap too (2106): a write across the wrap accrues the true elapsed span.
    #[test]
    fn the_timestamp_wraps() {
        let mut o = fresh(u32::MAX - 99, 3);
        assert_eq!(o.observe(200, 3), Observed::Accrued { checkpointed: true });
        assert_eq!(o.current.tick_cumulative, 3 * 300);
        assert_eq!(o.twap_tick(500, 3, 600), Ok(3));
        assert_eq!(o.twap_tick(250, 7, 350), Ok(floor_div(3 * 300 + 7 * 50, 350) as i32));
    }

    #[test]
    fn the_account_tail_round_trips_and_a_short_account_has_none() {
        let mut data = vec![0u8; FLOOR_POOL_ACCOUNT_LEN_WITH_TWAP];
        assert_eq!(TwapOracle::from_account(&data), Some(TwapOracle::empty()));
        let o = fresh(77, -3);
        assert!(o.store(&mut data));
        assert_eq!(TwapOracle::from_account(&data), Some(o));
        assert!(data[..TWAP_ORACLE_OFFSET].iter().all(|b| *b == 0), "the struct's bytes are untouched");
        let mut short = vec![0u8; FloorPool::ACCOUNT_LEN];
        assert_eq!(TwapOracle::from_account(&short), None);
        assert!(!o.store(&mut short));
    }

    /// The cushion's TWAP: a pump inside one second — swaps that write first, then a
    /// ratchet reading — weighs nothing in the settled tick; a young ring reads over its oldest checkpoint; the very
    /// first second reads the tick it started at.
    #[test]
    fn the_settled_tick_ignores_a_same_second_pump() {
        let mut o = TwapOracle::empty();
        assert_eq!(o.observe(1_000, 80_000), Observed::Initialized);
        // the first second: the pump (a dearer token is a LOWER tick) writes nothing new
        assert_eq!(o.observe(1_000, 60_000), Observed::SameSecond);
        assert_eq!(o.settled_tick(1_000, 60_000, CUSHION_TWAP_WINDOW_SECS), Ok(80_000), "the tick the second started at");
        // 100 s later: the pre-pump price stood the whole time; the pump inside THIS second weighs 0
        assert_eq!(o.observe(1_100, 80_000), Observed::Accrued { checkpointed: false });
        assert_eq!(o.settled_tick(1_100, 60_000, CUSHION_TWAP_WINDOW_SECS), Ok(80_000), "young ring: over its oldest");
        // and with a full window behind it
        let mut t = 1_100;
        while t < 3_500 {
            t += 300;
            o.observe(t, 80_000);
        }
        assert_eq!(o.observe(t, 50_000), Observed::SameSecond, "the pump's own swap, same second as the last write");
        assert_eq!(o.settled_tick(t, 50_000, CUSHION_TWAP_WINDOW_SECS), Ok(80_000));
        // one second of pumped price moves the window's average by about 1/window of the move (1/900)
        let later = o.settled_tick(t + 1, 50_000, CUSHION_TWAP_WINDOW_SECS).unwrap();
        let one_second = 30_000 / CUSHION_TWAP_WINDOW_SECS as i32;
        assert!(later < 80_000 && later >= 80_000 - one_second - 1, "{later}");
    }
}
