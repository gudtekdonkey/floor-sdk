//! Golden vectors: `fixtures/quote-vectors/floor.json` — FLOOR pools through `carpenter-quote` +
//! `FloorHookQuoter`.
//!
//! The file was written by the single-crate quoter BEFORE the AMM / FLOOR split, where FLOOR's callbacks lived inside
//! the AMM's quoter: 37 FLOOR
//! cases, 454 swaps — buys and sells from dust to the whole book, the skim ramp on both sides of the
//! floor, the in-swap bid re-place (close, nudge, re-open), graduated pools, the Carpenter Platform fee
//! on each currency code, the cushion top, partial hook flags, the round cap, and every refusal by
//! name. Each case stores its INPUT bytes (pool, bitmap, tick arrays, `FloorPool`) and every field of
//! the answer: amounts, the full fee breakdown, prices and ticks, the arrays read, the actions, the
//! re-place booking, signals, the four deltas, both fee splits, and the pool's, the arrays' and the
//! `FloorPool`'s post-state bytes (the arrays as FNV-1a 64 digests, to keep the file small).
//!
//! This test replays every swap from the stored bytes through the split crates and requires the
//! recorded answer exactly. It also replays each one a second way — `carpenter_quote::quote_swap_with`
//! with a `FloorHookQuoter` among the registered quoters, the composition a router adapter uses — and
//! requires the AMM-side fields to agree.
//!
//! ```bash
//! FLOOR_WRITE_VECTORS=1 cargo test -p floor-quote --test floor_vectors   # rewrite the expectations (inputs kept)
//! cargo test -p floor-quote --test floor_vectors                         # compare
//! ```
//! ⛔ Rewrite only after a deliberate behaviour change, and let the diff show what moved.
//!
//! The recorded pools name the `floor_hook` id of the deployment they were recorded under, which is
//! not a cluster this build lists. The replay quotes them as a cluster whose `floor_hook` is that id
//! ([`recorded_cluster`], `quote_swap_for` / `FloorHookQuoter::for_cluster`), so the inputs and the
//! expectations stay byte-for-byte the recording's.

use std::path::PathBuf;

use floor_quote::amm::{quote_swap_with, HookQuoter};
use floor_quote::state::{decode_floor_pool, decode_pool, encode_account};
use floor_quote::types::ids::{ClusterIds, DEVNET};
use floor_quote::{quote_swap_for, FloorHookQuoter, PoolAccount, SwapQuote, SwapQuoteParams, TickArraySet};
use serde_json::{json, Value};

fn path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/quote-vectors/floor.json")
}

fn unhex(s: &str) -> Vec<u8> {
    assert!(s.len() % 2 == 0, "odd hex");
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn fnv64(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for x in b {
        h ^= u64::from(*x);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn int<T: std::str::FromStr>(v: &Value) -> T
where
    T::Err: std::fmt::Debug,
{
    v.as_str().expect("decimal string").parse().unwrap()
}

fn answer(r: &Result<SwapQuote, floor_quote::QuoteError>) -> Value {
    match r {
        Err(e) => json!({ "err": e.name(), "display": e.to_string() }),
        Ok(q) => json!({ "ok": {
            "amount_in": q.amount_in.to_string(), "amount_out": q.amount_out.to_string(),
            "fees": { "lp": q.fees.lp.to_string(), "skim": q.fees.skim.to_string(), "creator": q.fees.creator.to_string(), "holder": q.fees.holder.to_string(),
                "platform_cut": q.fees.platform_cut.to_string(), "team": q.fees.team.to_string(), "protocol": q.fees.protocol.to_string(), "protocol_currency": q.fees.protocol_currency },
            "sqrt_after": q.sqrt_after.to_string(), "tick_after": q.tick_after,
            "sqrt_after_trade": q.sqrt_after_trade.to_string(), "tick_after_trade": q.tick_after_trade,
            "arrays_touched": q.arrays_touched, "action_arrays": q.action_arrays, "all_arrays": q.all_arrays(),
            "actions": q.replace_actions.iter().map(|a| format!("{a:?}")).collect::<Vec<_>>(),
            "replace": q.replace.map(|r| json!({ "from_tick": r.from_tick, "to_tick": r.to_tick,
                "absorbed": r.booking.absorbed.to_string(), "eth_back": r.booking.eth_back.to_string(), "nudge_spent": r.booking.nudge_spent.to_string(),
                "nudge_got": r.booking.nudge_got.to_string(), "from_lower": r.booking.from_lower, "redeployed": r.booking.redeployed,
                "to_lower": r.booking.to_lower, "to_upper": r.booking.to_upper, "bid_eth_deployed": r.booking.bid_eth_deployed.to_string(),
                "bid_liquidity": r.booking.bid_liquidity.to_string(), "fees_unknown": r.booking.fees_unknown })),
            "signals": q.signals,
            "swap_delta": [q.swap_delta.amount0.to_string(), q.swap_delta.amount1.to_string()],
            "hook_delta": [q.hook_delta.amount0.to_string(), q.hook_delta.amount1.to_string()],
            "trader_delta": [q.trader_delta.amount0.to_string(), q.trader_delta.amount1.to_string()],
            "fee_split0": q.fee_split0.iter().map(|x| x.to_string()).collect::<Vec<_>>(),
            "fee_split1": q.fee_split1.iter().map(|x| x.to_string()).collect::<Vec<_>>(),
            "pool_after": hex(&encode_account(&q.pool_after)),
            "arrays_after_fnv64": q.arrays_after.iter().map(|a| format!("{:016x}", fnv64(&encode_account(a)))).collect::<Vec<_>>(),
            "floor_after": q.floor_after.map(|f| hex(&encode_account(&f))),
        }}),
    }
}

/// The cluster the file was recorded under: its first case's pool hook as `floor_hook`, every other
/// id the shipped devnet entry's (the quoter reads only `floor_hook`).
fn recorded_cluster(doc: &Value) -> ClusterIds {
    let first = decode_pool(&unhex(doc["cases"][0]["pool"].as_str().unwrap())).unwrap();
    assert_ne!(first.hook, [0u8; 32], "the first case is a FLOOR pool");
    ClusterIds { name: "recorded", floor_hook: first.hook, ..DEVNET }
}

#[test]
fn floor_vectors() {
    let p = path();
    let raw = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let mut doc: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(doc["schema"], "floor-quote/floor/v1");
    let cluster = recorded_cluster(&doc);
    let write = std::env::var("FLOOR_WRITE_VECTORS").as_deref() == Ok("1");
    let (mut swaps, mut oks, mut replaces) = (0usize, 0usize, 0usize);
    let mut mismatches = Vec::new();

    for case in doc["cases"].as_array_mut().unwrap() {
        let name = case["name"].as_str().unwrap().to_string();
        let key: [u8; 32] = unhex(case["pool_key"].as_str().unwrap()).try_into().unwrap();
        let pool = decode_pool(&unhex(case["pool"].as_str().unwrap())).unwrap();
        let arrays: Vec<Vec<u8>> = case["tick_arrays"].as_array().unwrap().iter().map(|a| unhex(a.as_str().unwrap())).collect();
        let refs: Vec<&[u8]> = arrays.iter().map(Vec::as_slice).collect();
        let bitmap = unhex(case["bitmap"].as_str().unwrap());
        let set = TickArraySet::from_accounts(&key, pool.tick_spacing, &refs, Some(&bitmap)).unwrap();
        // The stored states carry `ladder_seeded = 0`, which `floor_hook` refuses (`NotSeeded`) and the
        // quoter therefore refuses too. The file predates that check and records the answers of seeded
        // pools, so every state is replayed seeded, and the stored flag is put back into `floor_after`
        // before comparing (the quoter never writes it). `NotSeeded` itself is covered in `tests/quote.rs`.
        let stored = case["floor_pool"].as_str().map(|h| decode_floor_pool(&unhex(h)).unwrap());
        let stored_seeded = stored.map(|f| f.ladder_seeded);
        let fp = stored.map(|mut f| {
            f.ladder_seeded = 1;
            f
        });
        let account = PoolAccount { key, pool };

        for (n, s) in case["swaps"].as_array_mut().unwrap().iter_mut().enumerate() {
            let params = SwapQuoteParams {
                zero_for_one: s["zero_for_one"].as_bool().unwrap(),
                amount_specified: int(&s["amount_specified"]),
                sqrt_price_limit_x64: int(&s["sqrt_price_limit_x64"]),
                supply0: int(&s["supply0"]),
                supply1: int(&s["supply1"]),
                cushion_top: s["cushion_top"].as_i64().map(|t| t as i32),
            };
            let got = quote_swap_for(&cluster, &account, &set, fp.as_ref(), &params);

            // The same swap through the AMM's registry entry point, as a router adapter composes it.
            let mut floor_q = FloorHookQuoter::for_cluster(fp, params.cushion_top, &cluster);
            let mut quoters: [&mut dyn HookQuoter; 1] = [&mut floor_q];
            let via_amm = quote_swap_with(&account, &set, &mut quoters, &params.amm());
            match (&got, &via_amm) {
                (Ok(a), Ok(b)) => {
                    assert_eq!((a.amount_in, a.amount_out, a.sqrt_after, a.tick_after), (b.amount_in, b.amount_out, b.sqrt_after, b.tick_after), "{name} #{n}");
                    assert_eq!((a.hook_delta, a.trader_delta, a.fee_split0, a.fee_split1), (b.hook_delta, b.trader_delta, b.fee_split0, b.fee_split1), "{name} #{n}");
                    assert_eq!(&a.replace_actions, &b.hook_actions, "{name} #{n}");
                    assert_eq!(bytemuck::bytes_of(&a.pool_after), bytemuck::bytes_of(&b.pool_after), "{name} #{n}");
                    assert_eq!(a.fees.hook_total() as u128, (a.fee_split0.iter().chain(a.fee_split1.iter())).enumerate()
                        .filter(|(i, _)| i % 5 != 3) // the platform slot is inside the skim
                        .map(|(_, x)| *x as u128).sum::<u128>(), "{name} #{n}: the breakdown is the fee split");
                }
                (Err(a), Err(b)) => assert_eq!(a, b, "{name} #{n}"),
                _ => panic!("{name} #{n}: the two entry points disagree: {got:?} vs {via_amm:?}"),
            }

            let mut got = got;
            if let (Ok(q), Some(seeded)) = (&mut got, stored_seeded) {
                if let Some(f) = q.floor_after.as_mut() {
                    assert_eq!(f.ladder_seeded, 1, "{name} #{n}: the quoter does not write ladder_seeded");
                    f.ladder_seeded = seeded;
                }
            }
            let now = answer(&got);
            swaps += 1;
            if let Ok(q) = &got {
                oks += 1;
                replaces += usize::from(q.replace.is_some());
            }
            if write {
                s["expect"] = now;
            } else if now != s["expect"] {
                mismatches.push(format!("{name} #{n}"));
            }
        }
    }

    if write {
        std::fs::write(&p, serde_json::to_string_pretty(&doc).unwrap() + "\n").unwrap();
        eprintln!("wrote {}", p.display());
        return;
    }
    assert!(mismatches.is_empty(), "{} of {swaps} swaps moved: {:?}", mismatches.len(), &mismatches[..mismatches.len().min(20)]);
    assert_eq!((swaps, oks, replaces), (454, 310, 18), "the file's swap count, answers and re-places");
}
