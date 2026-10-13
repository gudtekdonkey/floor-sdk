//! `floor-quote-wasm`: the JavaScript surface of `floor-quote`, bound with wasm-bindgen.
//!
//! The contract with the TypeScript wrapper (`@floorsupply/sdk-solana`'s `createQuoter` / `initQuote`):
//! - Accounts go in as their raw bytes (`Uint8Array`), exactly as `getAccountInfo` returns them.
//!   Tick arrays go in CONCATENATED, each its 3,888-byte body in a 3,888-byte stride. An empty
//!   `bitmap` / `floor_pool` means "not supplied".
//! - Every integer that can exceed 2^53 goes in and out as a DECIMAL STRING. Ticks, decimals and
//!   spacings are numbers.
//! - Every function returns one JSON string, `{"ok": …}` or `{"err": "<Name>", …}`, and never throws
//!   or panics on bad input; `<Name>` is the program's own error name. `version` returns its object
//!   bare and `holder_claimable` takes and returns a u64 (a JS bigint) and throws `Error(<Name>)`.
//!
//! Nothing here computes: every number comes from `floor-quote` (the AMM walk is `carpenter-quote`'s,
//! with FLOOR's callbacks replayed through `HookQuoter`).

use floor_quote::price;
use floor_quote::state::{decode_account, decode_cushion_top, decode_floor_pool, decode_launch, decode_pool, TickArraySet};
use floor_quote::types::hook::HookAction;
use floor_quote::types::layout::{BuyerPosition, Layout, Position, Tick, TickArray};
use floor_quote::{PoolAccount, QuoteError, SwapQuote, SwapQuoteParams};
use wasm_bindgen::prelude::*;

// ---- JSON ---------------------------------------------------------------------------------------

struct Obj(String);

impl Obj {
    fn new() -> Self {
        Obj(String::from("{"))
    }
    fn key(&mut self, k: &str) {
        if self.0.len() > 1 {
            self.0.push(',');
        }
        self.0.push('"');
        self.0.push_str(k);
        self.0.push_str("\":");
    }
    fn str(mut self, k: &str, v: impl ToString) -> Self {
        self.key(k);
        self.0.push('"');
        self.0.push_str(&v.to_string());
        self.0.push('"');
        self
    }
    fn num(mut self, k: &str, v: impl ToString) -> Self {
        self.key(k);
        self.0.push_str(&v.to_string());
        self
    }
    /// A JSON `true` / `false`.
    fn bool(mut self, k: &str, v: bool) -> Self {
        self.key(k);
        self.0.push_str(if v { "true" } else { "false" });
        self
    }
    fn raw(mut self, k: &str, json: &str) -> Self {
        self.key(k);
        self.0.push_str(json);
        self
    }
    fn done(mut self) -> String {
        self.0.push('}');
        self.0
    }
}

fn ok(body: String) -> String {
    Obj::new().raw("ok", &body).done()
}

fn err(e: QuoteError) -> String {
    let mut o = Obj::new().str("err", e.name());
    match e {
        QuoteError::TickArrayMissing { start_index } | QuoteError::HookActionAccountMissing { start_index } => {
            o = o.num("start_index", start_index);
        }
        QuoteError::QuoteBadAccount(which) => {
            o = o.str("account", which);
        }
        _ => {}
    }
    o.done()
}

fn out(r: Result<String, QuoteError>) -> String {
    match r {
        Ok(body) => ok(body),
        Err(e) => err(e),
    }
}

fn ints(v: &[i32]) -> String {
    let items: Vec<String> = v.iter().map(|x| x.to_string()).collect();
    format!("[{}]", items.join(","))
}

fn parse<T: core::str::FromStr>(s: &str) -> Result<T, QuoteError> {
    s.trim().parse::<T>().map_err(|_| QuoteError::QuoteBadAccount("number"))
}

fn key32(b: &[u8]) -> Result<[u8; 32], QuoteError> {
    b.try_into().map_err(|_| QuoteError::QuoteBadAccount("pool_key"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---- swap ---------------------------------------------------------------------------------------

/// Quote a swap. `sqrt_price_limit_x64` empty = no limit. `supply1` = the base mint's supply
/// (handed to FLOOR's callbacks; any value on a plain pool).
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn quote_swap(
    pool_key: &[u8],
    pool: &[u8],
    bitmap: &[u8],
    tick_arrays: &[u8],
    floor_pool: &[u8],
    zero_for_one: bool,
    amount_specified: &str,
    sqrt_price_limit_x64: &str,
    supply1: &str,
) -> String {
    out(quote_swap_inner(pool_key, pool, bitmap, tick_arrays, floor_pool, zero_for_one, amount_specified, sqrt_price_limit_x64, supply1))
}

#[allow(clippy::too_many_arguments)]
fn quote_swap_inner(
    pool_key: &[u8],
    pool: &[u8],
    bitmap: &[u8],
    tick_arrays: &[u8],
    floor_pool: &[u8],
    zero_for_one: bool,
    amount_specified: &str,
    sqrt_price_limit_x64: &str,
    supply1: &str,
) -> Result<String, QuoteError> {
    let key = key32(pool_key)?;
    let p = decode_pool(pool)?;
    if tick_arrays.len() % TickArray::ACCOUNT_LEN != 0 {
        return Err(QuoteError::QuoteBadAccount(TickArray::NAME));
    }
    let arrays: Vec<&[u8]> = tick_arrays.chunks_exact(TickArray::ACCOUNT_LEN).collect();
    let bm = if bitmap.is_empty() { None } else { Some(bitmap) };
    let set = TickArraySet::from_accounts(&key, p.tick_spacing, &arrays, bm)?;
    let fp = if floor_pool.is_empty() { None } else { Some(decode_floor_pool(floor_pool)?) };
    let limit = if sqrt_price_limit_x64.trim().is_empty() { floor_quote::no_limit(zero_for_one) } else { parse::<u128>(sqrt_price_limit_x64)? };
    let params = SwapQuoteParams {
        zero_for_one,
        amount_specified: parse::<i128>(amount_specified)?,
        sqrt_price_limit_x64: limit,
        supply0: 0,
        supply1: if supply1.trim().is_empty() { 0 } else { parse::<u64>(supply1)? },
        // The cushion top, read off the same FloorPool account bytes the chain reads it from.
        cushion_top: if floor_pool.is_empty() { None } else { decode_cushion_top(floor_pool) },
    };
    let q = floor_quote::quote_swap(&PoolAccount { key, pool: p }, &set, fp.as_ref(), &params)?;
    Ok(swap_json(&q))
}

fn swap_json(q: &SwapQuote) -> String {
    let fees = Obj::new()
        .str("lp", q.fees.lp)
        .str("skim", q.fees.skim)
        .str("creator", q.fees.creator)
        .str("holder", q.fees.holder)
        .str("platform_cut", q.fees.platform_cut)
        .str("team", q.fees.team)
        .str("protocol", q.fees.protocol)
        .num("protocol_currency", q.fees.protocol_currency)
        .done();
    let actions: Vec<String> = q
        .replace_actions
        .iter()
        .map(|a| match *a {
            HookAction::Swap { zero_for_one, amount_specified, sqrt_price_limit_x64 } => Obj::new()
                .str("kind", "swap")
                .bool("zero_for_one", zero_for_one)
                .str("amount_specified", amount_specified)
                .str("sqrt_price_limit_x64", sqrt_price_limit_x64)
                .done(),
            HookAction::ModifyPosition { salt, tick_lower, tick_upper, liquidity_delta } => Obj::new()
                .str("kind", "modify_position")
                .str("salt", salt)
                .num("tick_lower", tick_lower)
                .num("tick_upper", tick_upper)
                .str("liquidity_delta", liquidity_delta)
                .done(),
        })
        .collect();
    let replace = match q.replace {
        Some(r) => Obj::new()
            .str("absorbed", r.booking.absorbed)
            .str("eth_back", r.booking.eth_back)
            .str("nudge_spent", r.booking.nudge_spent)
            .str("nudge_got", r.booking.nudge_got)
            .num("from_lower", r.booking.from_lower)
            .bool("redeployed", r.booking.redeployed)
            .num("to_lower", r.booking.to_lower)
            .num("to_upper", r.booking.to_upper)
            .str("bid_eth_deployed", r.booking.bid_eth_deployed)
            .str("bid_liquidity", r.booking.bid_liquidity)
            .bool("fees_unknown", r.booking.fees_unknown)
            .num("from_tick", r.from_tick)
            .num("to_tick", r.to_tick)
            .done(),
        None => "null".to_string(),
    };
    Obj::new()
        .str("amount_in", q.amount_in)
        .str("amount_out", q.amount_out)
        .raw("fees", &fees)
        .str("sqrt_after", q.sqrt_after)
        .num("tick_after", q.tick_after)
        .str("sqrt_after_trade", q.sqrt_after_trade)
        .num("tick_after_trade", q.tick_after_trade)
        .raw("arrays_touched", &ints(&q.arrays_touched))
        .raw("action_arrays", &ints(&q.action_arrays))
        .raw("all_arrays", &ints(&q.all_arrays()))
        .raw("replace_actions", &format!("[{}]", actions.join(",")))
        .raw("replace", &replace)
        .num("signals", q.signals)
        .str("hook_delta0", q.hook_delta.amount0)
        .str("hook_delta1", q.hook_delta.amount1)
        .done()
}

// ---- liquidity ----------------------------------------------------------------------------------

#[wasm_bindgen]
pub fn amounts_for_liquidity(sqrt_price_x64: &str, tick_current: i32, tick_lower: i32, tick_upper: i32, liquidity_delta: &str) -> String {
    out((|| {
        let (a0, a1) = floor_quote::amounts_for_liquidity(parse(sqrt_price_x64)?, tick_current, tick_lower, tick_upper, parse(liquidity_delta)?)?;
        Ok(Obj::new().str("amount0", a0).str("amount1", a1).done())
    })())
}

#[wasm_bindgen]
pub fn quote_liquidity(sqrt_price_x64: &str, tick_lower: i32, tick_upper: i32, amount0_max: &str, amount1_max: &str) -> String {
    out((|| {
        let l = floor_quote::quote_liquidity(parse(sqrt_price_x64)?, tick_lower, tick_upper, parse(amount0_max)?, parse(amount1_max)?)?;
        Ok(Obj::new().str("liquidity", l).done())
    })())
}

/// Fees owed to a position: the pool's key and bytes, the position, and the arrays holding its two
/// ticks (the same account twice when both ticks share an array). Every account must name the pool:
/// a position of another pool is `QuoteBadAccount("Position")`, an array of another pool
/// `QuoteBadTickArray`.
#[wasm_bindgen]
pub fn fees_owed(pool_key: &[u8], pool: &[u8], position: &[u8], lower_array: &[u8], upper_array: &[u8]) -> String {
    out((|| {
        let key = key32(pool_key)?;
        let p = decode_pool(pool)?;
        let pos: Position = decode_account(position)?;
        if pos.pool != key {
            return Err(QuoteError::QuoteBadAccount(Position::NAME));
        }
        let lo: TickArray = decode_account(lower_array)?;
        let up: TickArray = decode_account(upper_array)?;
        if lo.pool != key || up.pool != key {
            return Err(QuoteError::QuoteBadTickArray);
        }
        let tick_of = |a: &TickArray, t: i32| -> Result<Tick, QuoteError> {
            let s = i32::from(p.tick_spacing);
            let i = t.checked_sub(a.start_index).filter(|d| *d >= 0 && d % s == 0).map(|d| d / s).filter(|i| *i < 60);
            i.map(|i| a.ticks[i as usize]).ok_or(QuoteError::QuoteBadTickArray)
        };
        let (f0, f1) = floor_quote::fees_owed(&p, &pos, &tick_of(&lo, pos.tick_lower)?, &tick_of(&up, pos.tick_upper)?);
        Ok(Obj::new().str("fees0", f0).str("fees1", f1).done())
    })())
}

// ---- price and tick -----------------------------------------------------------------------------

#[wasm_bindgen]
pub fn sqrt_price_at_tick(tick: i32) -> String {
    out(price::sqrt_price_at_tick(tick).map(|s| Obj::new().str("sqrt_price_x64", s).done()))
}

#[wasm_bindgen]
pub fn tick_at_sqrt_price(sqrt_price_x64: &str) -> String {
    out((|| Ok(Obj::new().num("tick", price::tick_at_sqrt_price(parse(sqrt_price_x64)?)?).done()))())
}

#[wasm_bindgen]
pub fn tick_at_price_ui(price_ui: f64, decimals0: u8, decimals1: u8) -> String {
    out(price::tick_at_price_ui(price_ui, decimals0, decimals1).map(|t| Obj::new().num("tick", t).done()))
}

/// Display only. `NaN` for a tick out of range.
#[wasm_bindgen]
pub fn price_ui_at_tick(tick: i32, decimals0: u8, decimals1: u8) -> f64 {
    price::price_ui_at_tick(tick, decimals0, decimals1).unwrap_or(f64::NAN)
}

#[wasm_bindgen]
pub fn floor_to_spacing(tick: i32, spacing: u16) -> String {
    out(price::floor_to_spacing(tick, spacing).map(|t| Obj::new().num("tick", t).done()))
}

#[wasm_bindgen]
pub fn ceil_to_spacing(tick: i32, spacing: u16) -> String {
    out(price::ceil_to_spacing(tick, spacing).map(|t| Obj::new().num("tick", t).done()))
}

// ---- the sale -----------------------------------------------------------------------------------

/// A sale buy: `lamports_in` is the GROSS cap; `{tokens, cost, team_fee, spent, graduates}` with
/// `spent = cost + team_fee`.
#[wasm_bindgen]
pub fn sale_buy(launch: &[u8], lamports_in: &str) -> String {
    out((|| {
        let l = decode_launch(launch)?;
        let q = floor_quote::sale_buy(&l, parse(lamports_in)?)?;
        Ok(Obj::new().str("tokens", q.tokens).str("cost", q.cost).str("team_fee", q.team_fee).str("spent", q.spent).num("graduates", q.graduates).done())
    })())
}

/// A sell-back: `{payout, team_fee, received}`, `received = payout − team_fee`. The position is read
/// only to check that it covers `tokens` (`InsufficientAllocation`).
#[wasm_bindgen]
pub fn sale_sell_back(launch: &[u8], buyer_position: &[u8], tokens: &str) -> String {
    out((|| {
        let l = decode_launch(launch)?;
        let bp: BuyerPosition = decode_account(buyer_position)?;
        let q = floor_quote::sale_sell_back(&l, &bp, parse(tokens)?)?;
        Ok(Obj::new().str("payout", q.payout).str("team_fee", q.team_fee).str("received", q.received).done())
    })())
}

// ---- holder rewards -----------------------------------------------------------------------------

/// What `floor_holders::claim` would pay this record now, in the quote's base units. Full account
/// data of the `HolderBook`, the `HolderRecord` and the launch's `FloorPool`; `live_balance` is the
/// holder's token account amount (0 when closed). A refusal throws `Error(<Name>)`.
#[wasm_bindgen]
pub fn holder_claimable(book_data: &[u8], record_data: &[u8], floor_pool_data: &[u8], live_balance: u64) -> Result<u64, JsError> {
    floor_quote::holder_claimable(book_data, record_data, floor_pool_data, live_balance).map_err(|e| JsError::new(e.name()))
}

// ---- referrals ----------------------------------------------------------------------------------

/// What `claim_referral_fees(tier)` would pay each of the three tiers now, from the full `FloorPool`
/// account data: `{"ok":{"due1","due2","due3"}}`, decimal strings.
#[wasm_bindgen]
pub fn referral_tier_dues(floor_pool_data: &[u8]) -> String {
    out(floor_quote::referral_tier_dues(floor_pool_data).map(|d| Obj::new().str("due1", d[0]).str("due2", d[1]).str("due3", d[2]).done()))
}

// ---- identity -----------------------------------------------------------------------------------

/// The crate version, the hook interface version this quoter replays and the `floor_hook` id it was
/// built against (hex).
#[wasm_bindgen]
pub fn version() -> String {
    Obj::new()
        .str("floor_quote_wasm", env!("CARGO_PKG_VERSION"))
        .num("hook_interface", floor_quote::types::hook::HOOK_INTERFACE_VERSION)
        .str("floor_hook_id", hex(&floor_quote::types::ids::FLOOR_HOOK_ID))
        .done()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shapes() {
        assert_eq!(err(QuoteError::TickArrayMissing { start_index: -3600 }), r#"{"err":"TickArrayMissing","start_index":-3600}"#);
        assert_eq!(ok(Obj::new().str("a", 1u64).num("b", -2).done()), r#"{"ok":{"a":"1","b":-2}}"#);
        assert!(sqrt_price_at_tick(0).starts_with(r#"{"ok":{"sqrt_price_x64":"18446744073709551616"}"#));
        assert_eq!(sqrt_price_at_tick(1_000_000), r#"{"err":"TickOutOfRange"}"#);
        assert_eq!(quote_swap(&[0; 3], &[], &[], &[], &[], true, "-1", "", ""), r#"{"err":"QuoteBadAccount","account":"pool_key"}"#);
    }

    /// Never panics on input: an out-of-range tick is a refusal, not an overflow (this profile sets
    /// `overflow-checks = true` with `panic = "abort"`, so a panic would abort the whole module).
    #[test]
    fn the_spacing_snaps_refuse_an_out_of_range_tick_instead_of_panicking() {
        for f in [floor_to_spacing as fn(i32, u16) -> String, ceil_to_spacing] {
            assert_eq!(f(i32::MIN, 60), r#"{"err":"TickOutOfRange"}"#);
            assert_eq!(f(i32::MAX, 60), r#"{"err":"TickOutOfRange"}"#);
            assert_eq!(f(0, 0), r#"{"err":"InvalidTickSpacing"}"#);
        }
        assert_eq!(ceil_to_spacing(443_636, 60), r#"{"ok":{"tick":443520}}"#);
        assert_eq!(floor_to_spacing(-443_636, 60), r#"{"ok":{"tick":-443520}}"#);
    }

    #[test]
    fn fees_owed_refuses_accounts_of_another_pool() {
        use floor_quote::state::encode_account;
        use floor_quote::synth::{build_pool, PoolKeys, SynthKind, SynthPosition};
        use floor_quote::types::U128;
        let keys = PoolKeys { pool: [1; 32], mint0: [2; 32], mint1: [3; 32], ..Default::default() };
        let sqrt = floor_quote::clmm::tick_math::get_sqrt_price_at_tick(0).unwrap();
        let sp = build_pool(&keys, SynthKind::Plain { fee: 3_000 }, 60, sqrt, 9, 6, &[SynthPosition { tick_lower: -6_000, tick_upper: 6_000, liquidity: 1_000_000 }]).unwrap();
        let pool = encode_account(&sp.account.pool);
        let mut pos: Position = bytemuck::Zeroable::zeroed();
        pos.pool = [1; 32];
        pos.tick_lower = -6_000;
        pos.tick_upper = 6_000;
        pos.liquidity = U128::new(1_000_000);
        let lo = encode_account(&sp.arrays[0]);
        let up = encode_account(&sp.arrays[1]);

        assert_eq!(fees_owed(&[1; 32], &pool, &encode_account(&pos), &lo, &up), r#"{"ok":{"fees0":"0","fees1":"0"}}"#);
        let mut foreign = pos;
        foreign.pool = [9; 32];
        assert_eq!(fees_owed(&[1; 32], &pool, &encode_account(&foreign), &lo, &up), r#"{"err":"QuoteBadAccount","account":"Position"}"#);
        let mut stray = sp.arrays[1];
        stray.pool = [9; 32];
        assert_eq!(fees_owed(&[1; 32], &pool, &encode_account(&pos), &lo, &encode_account(&stray)), r#"{"err":"QuoteBadTickArray"}"#);
        assert_eq!(fees_owed(&[7; 32], &pool, &encode_account(&pos), &lo, &up), r#"{"err":"QuoteBadAccount","account":"Position"}"#);
    }

    #[test]
    fn referral_tier_dues_is_the_quoters_number() {
        use floor_quote::state::encode_account;
        use floor_quote::types::layout::FloorPool;
        let mut fp: FloorPool = bytemuck::Zeroable::zeroed();
        fp.referrer = [1; 32];
        fp.referrer_t2 = [2; 32];
        fp.referrer_t3 = [3; 32];
        fp.referral_fees_paid = 1_000;
        fp.referral_owed = 1_000;
        assert_eq!(referral_tier_dues(&encode_account(&fp)), r#"{"ok":{"due1":"900","due2":"90","due3":"10"}}"#);
        assert_eq!(referral_tier_dues(&[0; 3]), r#"{"err":"QuoteBadAccount","account":"FloorPool"}"#);
    }

    /// Only the OK path runs on the host: a refusal builds a `JsError`, which needs a JS runtime.
    #[test]
    fn holder_claimable_is_the_quoters_number() {
        use floor_quote::state::encode_account;
        use floor_quote::types::holders::{HolderBook, HolderRecord};
        use floor_quote::types::layout::FloorPool;
        let mut hb: HolderBook = bytemuck::Zeroable::zeroed();
        hb.version = 1;
        hb.mint = [1; 32];
        hb.quote_mint = [2; 32];
        hb.eligible = 4;
        hb.indexed = 1_000;
        hb.pending = 1_000;
        let mut hr: HolderRecord = bytemuck::Zeroable::zeroed();
        hr.version = 1;
        hr.mint = [1; 32];
        hr.shares = 3;
        let mut fp: FloorPool = bytemuck::Zeroable::zeroed();
        fp.base_mint = [1; 32];
        fp.quote_mint = [2; 32];
        fp.holders_hooked = 1;
        fp.holder_owed = 1_000;
        let (b, r, p) = (encode_account(&hb), encode_account(&hr), encode_account(&fp));
        let got = holder_claimable(&b, &r, &p, 3).map_err(|_| ()).unwrap();
        assert_eq!(Ok(got), floor_quote::holder_claimable(&b, &r, &p, 3));
        assert_eq!(got, 750, "3 of 4 shares of 1_000");
    }

    #[test]
    fn the_sale_json_carries_the_team_fee() {
        use floor_quote::state::encode_account;
        use floor_quote::types::layout::Launch;
        let mut l: Launch = bytemuck::Zeroable::zeroed();
        (l.version, l.sale_tokens, l.team_bps) = (3, 550_000_000_000_000, 25);
        l.price_x64 = floor_quote::types::U128::new(1u128 << 64);
        let lb = encode_account(&l);
        let j = sale_buy(&lb, "1002500000");
        for want in [r#""tokens":"1000000000""#, r#""cost":"1000000000""#, r#""team_fee":"2500000""#, r#""spent":"1002500000""#, r#""graduates":false"#] {
            assert!(j.contains(want), "{j}");
        }
        let mut bp: BuyerPosition = bytemuck::Zeroable::zeroed();
        (bp.allocation, bp.contributed) = (1_000_000_000, 1_000_000_000);
        let s = sale_sell_back(&lb, &encode_account(&bp), "1000000000");
        for want in [r#""payout":"1000000000""#, r#""team_fee":"2500000""#, r#""received":"997500000""#] {
            assert!(s.contains(want), "{s}");
        }
    }

    /// `fees.protocol` (a decimal string) and `fees.protocol_currency` (an integer) carry the quoter's
    /// Carpenter Platform fee: non-zero at the default rate, `"0"` on a pool at a 0 rate.
    #[test]
    fn the_swap_json_carries_the_protocol_fee() {
        use floor_quote::state::{encode_account, encode_bitmap};
        use floor_quote::synth::{build_pool, PoolKeys, SynthKind, SynthPosition};
        use floor_quote::types::constants::{PROTOCOL_FEE_DEFAULT_PACKED, PROTOCOL_FEE_ON_CURRENCY1};
        let keys = PoolKeys { pool: [1; 32], mint0: [2; 32], mint1: [3; 32], ..Default::default() };
        let sqrt = floor_quote::clmm::tick_math::get_sqrt_price_at_tick(0).unwrap();
        let positions = [SynthPosition { tick_lower: -6_000, tick_upper: 6_000, liquidity: 1_000_000_000_000 }];
        for (packed, code) in [(PROTOCOL_FEE_DEFAULT_PACKED, PROTOCOL_FEE_ON_CURRENCY1), (0, 0)] {
            let sp = build_pool(&keys, SynthKind::Plain { fee: 3_000 }, 60, sqrt, 9, 6, &positions).unwrap().with_protocol_fee(packed, code);
            let pool = encode_account(&sp.account.pool);
            let arrays: Vec<u8> = sp.arrays.iter().flat_map(encode_account).collect();
            let bitmap = encode_bitmap(&keys.pool, 60, &sp.words);
            let json = quote_swap(&keys.pool, &pool, &bitmap, &arrays, &[], true, "-1000000000", "", "");
            let want = floor_quote::quote_swap(
                &PoolAccount { key: keys.pool, pool: sp.account.pool },
                &sp.tick_array_set().unwrap(),
                None,
                &SwapQuoteParams { zero_for_one: true, amount_specified: -1_000_000_000, sqrt_price_limit_x64: floor_quote::no_limit(true), supply0: 0, supply1: 0, cushion_top: None },
            )
            .unwrap();
            assert_eq!(want.fees.protocol > 0, packed != 0, "packed {packed}");
            let expect = format!(r#""protocol":"{}","protocol_currency":{}"#, want.fees.protocol, want.fees.protocol_currency);
            assert!(json.contains(&expect), "packed {packed}: {json}");
        }
    }
}
