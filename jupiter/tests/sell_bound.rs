//! The stateless sell-back against the pro-rata rule it replaced, over random buy / sell-back
//! sequences, with the program's arithmetic (`floor-math`).
//!
//! The sale has one fixed price `p` (`Launch.price_x64`). A buy of `t` tokens records
//! `contributed += ceil(t · p / 2^64)`. `floor_launch` pays a sell-back of `k` tokens
//! `stateless(k) = floor(k · p / 2^64)` less the fee, the same for every seller, and records
//! `contributed −= floor(k · p / 2^64)`. The pro-rata rule paid `floor(contributed · k / allocation)`
//! (then less the fee), where `contributed / allocation` is the price plus each wallet's own rounding
//! history.
//!
//! The property checked here, on positions that follow the pro-rata rule: `pro_rata(k) ≥ stateless(k)`
//! always, and `pro_rata(k) − stateless(k) ≤` the number of buys and sell-backs the position has made
//! before (each buy rounds its cost up by under one lamport, and each partial sell-back floors its
//! payout, leaving under one lamport more in the position). So the stateless payout never exceeds what
//! a position's own share would be, and the difference stays in the sale (and goes to the pool at
//! graduation). `tests/custody.rs` measures the same against the program.

use floor_math::{sale_buy_with_team, sell_back_with_team};

/// xorshift64*: a deterministic stream, no dependency.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

fn stateless(price: u128, bps: u16, k: u64) -> u64 {
    let gross = ((k as u128 * price) >> 64) as u64;
    sell_back_with_team(gross, bps).unwrap().1
}

#[test]
fn the_stateless_payout_is_never_above_the_pro_rata_share_and_within_the_operation_count() {
    let prices: [u128; 5] = [92_233_720_368_547, 3_808_478_006_794_440, 7_682_302_212_939_176, 1u128 << 64, (1u128 << 64) * 3 + 12_345];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let (mut sells, mut exact, mut worst) = (0u64, 0u64, 0u64);
    for &price in &prices {
        for bps in [0u16, 40, 100] {
            for _wallet in 0..200 {
                let (mut alloc, mut contributed, mut ops) = (0u64, 0u64, 0u64);
                for _step in 0..30 {
                    if alloc == 0 || rng.below(3) != 0 {
                        let span = if rng.below(2) == 0 { 10_000 } else { 10_000_000_000 };
                        let gross = 1 + rng.below(span);
                        let (t, cost, _fee) = sale_buy_with_team(gross, price, u64::MAX, bps).unwrap();
                        if t == 0 {
                            continue;
                        }
                        alloc += t;
                        contributed += cost;
                        ops += 1;
                    } else {
                        let k = 1 + rng.below(alloc);
                        let payout = (contributed as u128 * k as u128 / alloc as u128) as u64;
                        let received = sell_back_with_team(payout, bps).unwrap().1;
                        let bound = stateless(price, bps, k);
                        assert!(received >= bound, "price {price} bps {bps}: received {received} < stateless {bound}");
                        assert!(received - bound <= ops, "price {price} bps {bps}: over by {} after {ops} operations", received - bound);
                        worst = worst.max(received - bound);
                        exact += (received == bound) as u64;
                        sells += 1;
                        alloc -= k;
                        contributed -= payout;
                        ops += 1;
                    }
                }
            }
        }
    }
    eprintln!("{sells} sell-backs: the pro-rata share equals the stateless amount on {exact}, and exceeds it by at most {worst} lamport(s)");
    assert!(sells > 10_000);
    assert!(exact < sells, "the pro-rata share depends on the seller's history");
}
