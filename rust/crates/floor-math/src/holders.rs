//! The holder-rewards TALLY: the ONE implementation. `floor_holders` runs it on chain (`execute`, `open_record`,
//! `touch`, `sync`, `claim`, `close_record`), `floor-quote` runs it for the quoter, and the wasm's `holder_claimable`
//! runs it in the browser — so clients show the program's own arithmetic.
//!
//! No rounds: no sealing, no proofs, no publisher key, no scheduler. The tally is a reward-per-share accumulator
//! (`acc`, Q64.64) over the RECORDS that exist: each swap's holder tax is booked by `take_fees` into
//! `FloorPool.holder_owed` (the swap callbacks change NOTHING here — no CPI in a callback), and the tally catches up
//! LAZILY from the pool's lifetime counter `holder_owed + holder_paid_eth` whenever a hooked transfer, a sync, a touch
//! or a claim runs.
//!
//! ⛔ **COUNTING IS RECORDS-ONLY.** A wallet with no record — a plain-send receiver that never
//! opened one, the pool's own vault, an escrow, any program-owned (off-curve) account —
//! earns nothing, and its share goes to holders WITH records.
//! ⛔ **ROUNDING IS DOWN, EVERYWHERE A HOLDER IS CREDITED**, and the dust stays on the pool:
//! `holder_owed` is never over-drawn. Every counter SATURATES; `acc` alone WRAPS (it is only ever
//! read as a difference, `acc − snapshot`, which is exact modulo 2^128).
//! ⛔ **THE BURN CLIP.** A burn is not a transfer, so a burn outside FLOOR's own Burn (which is
//! `sync → burn → sync`) leaves a record's `shares` above its live balance. The next settle pays
//! only on `min(shares, live)` and moves what the burned tokens earned since that record's last
//! settle into `unallocated`, where every recorded holder shares it. It is the burner's own loss,
//! never a gain — nobody can earn on tokens they no longer hold.
//!
//! ⛔ **EACH SLOT'S TAX IS SHARED AT THE SLOT'S END** (the same-transaction-loan fix). The tax a slot indexes waits in
//! `pending`, and the FIRST catch-up of a later slot ([`catch_up_at`]) folds it — with anything parked or clipped —
//! over the shares as they stood when that slot ended: every share change runs a catch-up first, so nothing has moved
//! yet. Why: folding the tax over the shares of THAT instant lets a wallet that borrowed tokens inside its own
//! transaction (a lender, another AMM, its own parked LP), traded and repaid be credited `tax × loan / (loan +
//! eligible)` of its own tax — MEASURED 697,885 of a 1,000,000 tax
//! (`a_loan_inside_one_transaction_earns_none_of_its_tax`). A loan cannot outlive its transaction, so it never holds at
//! a slot's end. The trade-off, accepted: a buyer still holding at the slot's end shares (pro rata) in its own buy tax,
//! and a seller no longer shares in its own sell tax. The EVM twin keeps "shares held before the trade" instead
//! (`FloorHolderRewards`, "NEW TOKENS EARN FROM THE NEXT BLOCK"): its exact rule needs per-block history, which costs
//! rent here (one account per slot).
//!
//! ⚠ **The released amount is the CEILING of what `acc` gave out.** `distributed = floor(inc × eligible >> 64);
//! unallocated = pot − distributed` is UNSOUND: `inc × eligible / 2^64` is almost never an integer, so `floor` leaves up
//! to one unit in `unallocated` that `acc` has ALREADY released, and the next `catch_up` — with no new tax at all —
//! releases it AGAIN. `sync` is permissionless, so anyone could fold the same unit into `acc` over and over and mint
//! claims out of nothing (three shares, one unit of tax: every `sync` would add a third of a unit to each share;
//! `tests/holders_props.rs` reproduces it). The released amount must be the CEILING of what `acc` gave out, so the book never
//! under-counts its own promises. With a positive `eligible` below 2^64 that ceiling is exactly `pot`, so
//! `unallocated` returns to 0 on every fold with holders — and the sub-unit rounding (< eligible / 2^64 of a unit per
//! fold) is the dust that stays on the pool.

use carpenter_clmm_math::big_num::U256;

/// `acc` is Q64.64: one unit of quote per share is `1 << ACC_SHIFT`.
pub const ACC_SHIFT: u32 = 64;

/// A `HolderBook`'s tally fields (`floor_types::holders::HolderBook` carries them on chain).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Book {
    /// Wrapping Q64.64 reward-per-share.
    pub acc: u128,
    /// Σ `shares` of open records.
    pub eligible: u64,
    /// The lifetime (`holder_owed + holder_paid_eth`) already folded.
    pub indexed: u64,
    /// Quote base units not yet distributed (tax with no eligible holder, and the burn clip).
    pub unallocated: u64,
    /// Tax indexed in `last_slot`, not yet shared: [`catch_up_at`] folds it at a later slot's first touch.
    pub pending: u64,
    /// The slot of the last [`catch_up_at`].
    pub last_slot: u64,
}

/// A `HolderRecord`'s tally fields (`floor_types::holders::HolderRecord` carries them on chain).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Record {
    /// The balance this record last settled on.
    pub shares: u64,
    /// `Book.acc` at the last settle.
    pub snapshot: u128,
    /// Earned and not yet claimed.
    pub owed: u64,
}

/// What one [`settle`] moved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settled {
    /// Credited to the record's `owed`.
    pub earned: u64,
    /// The burn clip: what shares the holder no longer holds earned since the last settle,
    /// returned to `Book.unallocated`.
    pub clipped: u64,
}

/// The tally's LIFETIME counter: `holder_owed + holder_paid_eth`, saturating. It only ever rises —
/// `take_fees` adds to `holder_owed`, and `pay_holder` moves `holder_owed` into `holder_paid_eth`
/// without changing the sum.
pub fn lifetime(holder_owed: u64, holder_paid_eth: u64) -> u64 {
    holder_owed.saturating_add(holder_paid_eth)
}

/// `floor(a × b / 2^64)` with a 256-bit intermediate (`a × b` reaches 2^192).
fn mul_shr64_floor(a: u64, b: u128) -> U256 {
    (U256::from(a) * U256::from(b)) >> (ACC_SHIFT as usize)
}

/// `ceil(a × b / 2^64)` with a 256-bit intermediate.
fn mul_shr64_ceil(a: u128, b: u64) -> U256 {
    let p = U256::from(a) * U256::from(b);
    let q = p >> (ACC_SHIFT as usize);
    if (p & U256::from(u64::MAX)).is_zero() {
        q
    } else {
        q + U256::from(1u8)
    }
}

/// Narrow to `u64`, saturating.
fn sat_u64(x: U256) -> u64 {
    if x > U256::from(u64::MAX) {
        u64::MAX
    } else {
        x.low_u64()
    }
}

/// THE PROGRAMS' CATCH-UP (each slot's tax at the slot's end, the module doc). At the first call in a
/// slot later than `last_slot`, fold `pending` and `unallocated` over the current `eligible` — the
/// shares as they stood at the end of the last slot touched; then index what arrived since
/// (`lifetime − indexed`) into `pending`, shared at a later slot. Never fails.
pub fn catch_up_at(book: &mut Book, lifetime: u64, slot: u64) {
    if slot != book.last_slot {
        let p = book.pending;
        book.pending = 0;
        fold(book, p);
        book.last_slot = slot;
    }
    let delta = lifetime.saturating_sub(book.indexed);
    book.indexed = book.indexed.max(lifetime);
    book.pending = book.pending.saturating_add(delta);
}

/// Fold every unit of tax that arrived since the last fold (`lifetime − indexed`), plus whatever
/// sits in `unallocated`, into `acc` over the current `eligible` AT ONCE. Never fails.
///
/// ⛔ The fold PRIMITIVE, kept for its property suite and the fold arithmetic's own tests: the
/// programs and [`claimable`] call [`catch_up_at`], which never folds a slot's tax inside that slot.
///
/// With `eligible == 0` the whole pot PARKS in `unallocated` and is released to whoever holds
/// records at the next fold with holders.
pub fn catch_up(book: &mut Book, lifetime: u64) {
    let delta = lifetime.saturating_sub(book.indexed);
    book.indexed = book.indexed.max(lifetime);
    fold(book, delta);
}

/// Fold `amount` plus `unallocated` into `acc` over the current `eligible` (or park the lot).
fn fold(book: &mut Book, amount: u64) {
    let pot = book.unallocated.saturating_add(amount);
    if pot == 0 {
        return;
    }
    if book.eligible == 0 {
        book.unallocated = pot;
        return;
    }
    // `pot < 2^64`, so `pot << 64 < 2^128` fits a u128, and `eligible >= 1` keeps the division sane.
    let inc = ((pot as u128) << ACC_SHIFT) / book.eligible as u128;
    if inc == 0 {
        // unreachable while `eligible < 2^64` (then `inc >= 2^64 / eligible >= 1`); kept so the
        // function stays total under any input
        book.unallocated = pot;
        return;
    }
    book.acc = book.acc.wrapping_add(inc);
    // ⛔ The CEILING of what `acc` just promised (see the module doc's deviation note): never floor.
    let released = sat_u64(mul_shr64_ceil(inc, book.eligible));
    book.unallocated = pot.saturating_sub(released);
}

/// Open a record on `balance` shares. Call AFTER [`catch_up`], so the new record does not share
/// in tax that arrived before it existed.
pub fn open(book: &mut Book, balance: u64) -> Record {
    book.eligible = book.eligible.saturating_add(balance);
    Record { shares: balance, snapshot: book.acc, owed: 0 }
}

/// Settle `rec` against `book`: credit what its shares earned since the last settle — on at most
/// `pre` of them, the balance the holder actually had going into this change — then move it to
/// `post` shares. Never fails.
///
/// - a hooked transfer's SENDER: `pre = post + amount`, `post = source.amount`;
/// - its RECEIVER: `pre = post − amount`, `post = destination.amount`;
/// - `sync` / `claim` / `close_record`: `pre = post = live balance` (a closed account reads 0).
pub fn settle(book: &mut Book, rec: &mut Record, pre: u64, post: u64) -> Settled {
    let d = book.acc.wrapping_sub(rec.snapshot);
    let base = rec.shares.min(pre);
    let earned = sat_u64(mul_shr64_floor(base, d));
    let clipped = if rec.shares > base { sat_u64(mul_shr64_floor(rec.shares - base, d)) } else { 0 };
    rec.owed = rec.owed.saturating_add(earned);
    book.unallocated = book.unallocated.saturating_add(clipped);
    book.eligible = book.eligible.saturating_sub(rec.shares).saturating_add(post);
    rec.shares = post;
    rec.snapshot = book.acc;
    Settled { earned, clipped }
}

/// What `claim` would pay `rec` if sent now: [`catch_up_at`] to `lifetime` at a slot after the book's
/// last (a claim lands in a later slot; one in the SAME slot as the book's last touch pays less, by
/// that slot's still-pending tax), then [`settle`] on the live balance, on COPIES — nothing is
/// written. This is the number the site shows, computed by the program's own arithmetic
/// (`wasm/floor-quote-wasm` `holder_claimable`).
pub fn claimable(book: Book, rec: Record, lifetime: u64, live_balance: u64) -> u64 {
    let mut b = book;
    let mut r = rec;
    let next = b.last_slot.wrapping_add(1);
    catch_up_at(&mut b, lifetime, next);
    settle(&mut b, &mut r, live_balance, live_balance);
    r.owed
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: u128 = 1 << 64;

    #[test]
    fn lifetime_saturates() {
        assert_eq!(lifetime(3, 4), 7);
        assert_eq!(lifetime(u64::MAX, 1), u64::MAX);
    }

    /// One holder with every share takes the whole tax, to the unit.
    #[test]
    fn a_lone_holder_takes_the_whole_tax() {
        let mut b = Book::default();
        catch_up(&mut b, 0);
        let mut r = open(&mut b, 1_000);
        catch_up(&mut b, 777);
        assert_eq!(b.acc, 777 * ONE / 1_000);
        let s = settle(&mut b, &mut r, 1_000, 1_000);
        assert_eq!(s.earned, 776, "777/1000 per share floors: 1000 × floor(777·2^64/1000) / 2^64 = 776.99…");
        assert_eq!(b.unallocated, 0, "the ceiling of what acc released is the whole pot");
    }

    /// Rounding is DOWN: two holders splitting an odd unit each get the floor.
    #[test]
    fn rounding_is_down_for_every_holder() {
        let mut b = Book::default();
        let mut a = open(&mut b, 1);
        let mut c = open(&mut b, 1);
        catch_up(&mut b, 1);
        let ea = settle(&mut b, &mut a, 1, 1).earned;
        let ec = settle(&mut b, &mut c, 1, 1).earned;
        assert_eq!(ea + ec, 0, "half a unit each floors to nothing — the unit stays on the pool");
    }

    /// The eligible==0 pot parks, then releases.
    #[test]
    fn a_pot_with_no_holder_parks_then_releases() {
        let mut b = Book::default();
        catch_up(&mut b, 500);
        assert_eq!((b.unallocated, b.acc, b.indexed), (500, 0, 500));
        let mut r = open(&mut b, 10);
        catch_up(&mut b, 500);
        assert_eq!(b.unallocated, 0);
        assert_eq!(settle(&mut b, &mut r, 10, 10).earned, 500);
    }

    /// One hooked transfer as `floor_holders::execute` runs it at `slot`: catch up, then settle the
    /// sender (`pre = post + amount`) and the receiver (`pre = post - amount`). `None` is a side with
    /// no record (the pool's vault, a lender that never opened one).
    #[allow(clippy::too_many_arguments)]
    fn transfer(b: &mut Book, lifetime: u64, slot: u64, from: Option<&mut Record>, to: Option<&mut Record>, from_post: u64, to_post: u64, amount: u64) {
        catch_up_at(b, lifetime, slot);
        if let Some(r) = from {
            settle(b, r, from_post + amount, from_post);
        }
        if let Some(r) = to {
            settle(b, r, to_post - amount, to_post);
        }
    }

    /// `sync` at `slot`: catch up, settle on the live balance.
    fn sync(b: &mut Book, lifetime: u64, slot: u64, r: &mut Record, live: u64) {
        catch_up_at(b, lifetime, slot);
        settle(b, r, live, live);
    }

    /// THE SAME-TRANSACTION LOAN (the EVM twin is `test/holders/FlashHolderTax.t.sol`). An honest holder H has held 300
    /// since slot 1; the attacker A opened a record and holds nothing. In ONE transaction, in slot 10: a lender L sends
    /// A 693 tokens; A buys, paying `TAX` of holder tax (booked into the pool's lifetime counter by the
    /// swap callback, indexed by the settlement transfer's `execute`); A sends the 693 back. The loan
    /// never holds at a slot's end, so it earns NOTHING: every unit of the tax is H's.
    /// MUTATION (with `catch_up` in `transfer`, folding at the transfer's instant): A is credited 697,885 of the
    /// 1,000,000.
    #[test]
    fn a_loan_inside_one_transaction_earns_none_of_its_tax() {
        const TAX: u64 = 1_000_000;
        const E: u64 = 300; // H's long-held balance
        const X: u64 = 693; // the loan
        const BOUGHT: u64 = 50; // what A's buy delivers
        let mut b = Book::default();
        catch_up_at(&mut b, 0, 1);
        let mut h = open(&mut b, E);
        let mut a = open(&mut b, 0);
        // -- one transaction, slot 10 --
        transfer(&mut b, 0, 10, None, Some(&mut a), 0, X, X); // L -> A: the loan
        // A's buy: the callback books TAX; the vault -> A settlement transfer indexes it
        transfer(&mut b, TAX, 10, None, Some(&mut a), 0, X + BOUGHT, BOUGHT);
        transfer(&mut b, TAX, 10, Some(&mut a), None, BOUGHT, 0, X); // A -> L: repaid
        assert_eq!(b.pending, TAX, "the slot's tax waits for the slot to end");
        // -- a later slot: both records settle on what they hold --
        sync(&mut b, TAX, 11, &mut a, BOUGHT);
        sync(&mut b, TAX, 11, &mut h, E);
        // 142,857: A's purchase alone, 50 of the 350 shares held at the slot's end. With the loan
        // counted A would hold 743 of 1,043 shares and take 712,368 (697,885 before the fix, which
        // folded the tax at the settlement transfer, before A's 50 had arrived).
        assert_eq!(a.owed, TAX * BOUGHT / (E + BOUGHT), "only what A still held at the slot's end earned: not the loan");
        assert!(h.owed + a.owed >= TAX - 2 && h.owed + a.owed <= TAX, "the rest is H's, rounding down");
    }

    /// The accepted trade-off, pinned: a buyer still holding at the slot's end shares its own buy tax
    /// pro rata; a seller that sold in the slot does not share its own sell tax.
    #[test]
    fn the_slot_end_holders_share_the_slots_tax() {
        let mut b = Book::default();
        catch_up_at(&mut b, 0, 1);
        let mut h = open(&mut b, 100);
        let mut buyer = open(&mut b, 0);
        let mut seller = open(&mut b, 100);
        // slot 5: buyer buys 100 (tax 1,000), seller sells all 100 (tax 1,000)
        transfer(&mut b, 1_000, 5, None, Some(&mut buyer), 0, 100, 100);
        transfer(&mut b, 2_000, 5, Some(&mut seller), None, 0, 0, 100);
        sync(&mut b, 2_000, 6, &mut h, 100);
        sync(&mut b, 2_000, 6, &mut buyer, 100);
        sync(&mut b, 2_000, 6, &mut seller, 0);
        assert_eq!(seller.owed, 0, "sold before the slot ended: none of the slot's tax");
        assert_eq!((h.owed, buyer.owed), (1_000, 1_000), "the two slot-end holders split both taxes");
    }

    /// Nothing folds inside a slot: a clip or a parked pot waits for the next slot too.
    #[test]
    fn nothing_folds_inside_the_slot_it_arrived_in() {
        let mut b = Book::default();
        catch_up_at(&mut b, 500, 1); // tax with nobody eligible
        catch_up_at(&mut b, 500, 2);
        assert_eq!((b.unallocated, b.pending), (500, 0), "parked: nobody held at slot 1's end");
        let mut r = open(&mut b, 10);
        catch_up_at(&mut b, 500, 2);
        assert_eq!(b.unallocated, 500, "a record opened in slot 2 does not take slot 2's fold");
        sync(&mut b, 500, 3, &mut r, 10);
        assert_eq!(r.owed, 500, "it takes the parked pot at the next slot");
    }

    /// `claimable` writes nothing and is what a claim sent now -- landing in a later slot -- credits:
    /// what was indexed before, not tax nobody has indexed yet.
    #[test]
    fn claimable_is_the_real_settle_on_copies() {
        let mut b = Book::default();
        catch_up_at(&mut b, 0, 1);
        let mut r = open(&mut b, 3);
        catch_up_at(&mut b, 100, 1); // a swap in slot 1 indexed 100
        let before = (b, r);
        let c = claimable(b, r, 150, 3); // and 50 more booked, not yet indexed
        assert_eq!((b, r), before, "nothing written");
        catch_up_at(&mut b, 150, 2);
        settle(&mut b, &mut r, 3, 3);
        assert_eq!(c, r.owed);
        assert_eq!(c, 99, "slot 1's 100 over 3 shares floors to 99; the 50 waits for slot 2's end");
    }
}
