//! The launch modes (Custom, Moonshot, Cook, Bluechip) and the tranche price (the EVM's `FloorModes`).

use carpenter_clmm_math::big_num::U256;
use carpenter_clmm_math::fixed_point_64::Q64;
use carpenter_clmm_math::full_math::MulDiv;

use crate::constants::{quote_unit, BPS, SUPPLY};
use crate::error::{FloorMathError, FloorMathResult};

/// Ordered by graduation cap. Every reference INSIDE Rust is by name, never by index -- and that is
/// where the guarantee stops.
///
/// ⛔ The discriminant CROSSES FOUR BOUNDARIES as a bare `u8`, and is validated at NONE of them but
/// the first: it arrives Borsh-encoded as `CreateLaunchParams.mode` (wire field 0), is range-checked
/// once by [`Mode::try_from`] in `floor_launch::sale::resolve_config` and once more against
/// `MODE_CUSTOM` in `floor_hook::register_pool`, then is written VERBATIM into three permanent
/// zero-copy accounts at offsets pinned by `floor-types::layout` -- `FloorPool.mode`,
/// `Launch.mode`, `RegistryEntry.mode` -- re-emitted in `LaunchCreated` / `PoolRegistered`, and
/// decoded POSITIONALLY by clients. So renumbering rewrites the meaning of every launch already
/// stored, silently.
///
/// ⛔ A client must REFUSE rather than relabel: a decoder that maps an out-of-range ordinal onto a
/// default shows a launch wearing a preset it never had. Bytes 4 and above refuse here, which is the
/// right failure.
///
/// ⛔ A member may be APPENDED for free; one may never be removed or reordered without renumbering
/// every account that has ever stored one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Mode {
    /// Custom 0, Moonshot 1, Cook 2, Bluechip 3 (the presets by graduation cap), the EVM's `FloorModes.Mode` too. A
    /// zeroed byte is Custom (cap 0 refused), never a silent preset. ⛔ The stored byte is read after create
    /// (`floor_hook::mech::stair`, `floor_launch::genesis`), so a renumber needs a FRESH program set.
    Custom = 0,
    Moonshot = 1,
    Cook = 2,
    Bluechip = 3,
}

impl TryFrom<u8> for Mode {
    type Error = FloorMathError;
    fn try_from(v: u8) -> FloorMathResult<Mode> {
        match v {
            0 => Ok(Mode::Custom),
            1 => Ok(Mode::Moonshot),
            2 => Ok(Mode::Cook),
            3 => Ok(Mode::Bluechip),
            // 4 and everything past it are refused.
            _ => Err(FloorMathError::UnknownMode),
        }
    }
}

/// A mode's defaults (the EVM's `FloorModes`). The rates are `u16` bps, `FloorPool`'s width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Preset {
    /// Market cap at graduation in whole USD: price × total supply; the sale raises `sale_bps` of it. 0 for Custom
    /// (the launcher's choice).
    pub graduation_mcap_usd: u64,
    /// Creator fee, both sides.
    pub creator_bps: u16,
    /// Floor skim far from the floor.
    pub skim_buy_far_bps: u16,
    pub skim_sell_far_bps: u16,
    /// Holder rewards, both sides; 0 on every preset. A launch with a holder rate is HOOKED
    /// (`floor_launch::sale::ResolvedConfig::hooked`).
    pub holder_bps: u16,
    /// The supply split: the sale's and the POL's share of supply in bps; the staircase is the rest. Every preset
    /// obeys [`split_tokens`]'s rule. 0 / 0 for Custom (its own pick).
    pub sale_bps: u16,
    pub pol_bps: u16,
    /// ⛔ THE AT-FLOOR SELL MULTIPLE, PER LAUNCH, bps of a multiple of the far SELL skim that a sell INTO the bid pays:
    /// 15,000 (1.5×) on every preset (`constants::PRESET_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS`); 0 on Custom's row, which
    /// [`skim_sell_at_floor_multiple_for`] reads as `constants::SKIM_SELL_AT_FLOOR_MULTIPLE_BPS` (1.5×). `floor_launch`
    /// carries it into `floor_hook::register_pool` (the trailing `SkimSellMultipleArg`); buys still ramp to 0.
    pub skim_sell_at_floor_multiple_bps: u32,
    /// ⛔ THE STAIRCASE: the number of fixed token-only steps, each one doubling of the price from the open
    /// (`crate::stair`). NON-ZERO = this preset is a STAIRCASE preset: 20 / 10 / 69 / 1 (sale / classic POL /
    /// staircase / open tail), the two-number split and the fed, following cushion. Moonshot 15, Cook 15, Bluechip 14;
    /// 0 on Custom's ROW — a Custom launch's count is [`custom_stair`] of its own cap, resolved at create.
    pub stair_steps: u8,
    /// The staircase's growth between consecutive steps, bps (10,000 = equal steps; 10,400 = each step 4 % heavier
    /// than the one below). Moonshot 10,000, Cook and Bluechip 10,400; 0 on Custom.
    pub stair_growth_bps: u16,
}

impl Preset {
    /// Is this a STAIRCASE preset? The ONE spelling: `stair_steps != 0`.
    pub const fn is_stair(&self) -> bool {
        self.stair_steps != 0
    }
}

/// The at-floor sell multiple a launch of mode `m` registers with: the preset's 15,000 (1.5×), or 15,000 (1.5×) for
/// Custom — and so for `$FLOOR`'s genesis, a Custom launch. The ONE table `floor_launch::sale::resolve_config` and
/// the SDK's `skimSellAtFloorMultipleFor` read.
pub const fn skim_sell_at_floor_multiple_for(m: Mode) -> u32 {
    match m {
        Mode::Custom => crate::constants::SKIM_SELL_AT_FLOOR_MULTIPLE_BPS as u32,
        _ => preset(m).skim_sell_at_floor_multiple_bps,
    }
}

/// ⛔ Custom on the staircase (the EVM's `FloorModes.customPoolWord`): a Custom launch's `(steps, growth_bps)` —
/// [`crate::stair::custom_stair_steps`] of its graduation cap in whole USD, equal steps. Every pool is a staircase;
/// there is no reserve wall.
pub fn custom_stair(cap_usd: u64) -> (u8, u16) {
    crate::stair::custom_stair(cap_usd)
}

/// Each zone's least share of supply, and the standard split a Custom launch gets when it sends 0 / 0 (each zone at
/// least 10 %, the three summing to 100 %). Held equal to `floor_types::constants` by `tests/floor_types_pin.rs`.
pub const MIN_ZONE_BPS: u16 = 1_000;
pub const STANDARD_SALE_BPS: u16 = 3_000;
pub const STANDARD_POL_BPS: u16 = 1_000;

/// A launch's split in TOKENS (base units): `(sale, pol, wall)`, the three summing to `SUPPLY`.
/// Refuses (`BadSplit`) unless the sale, the POL and the wall are EACH at least `MIN_ZONE_BPS`.
/// ⛔ The wall is the REMAINDER, never a third input, so the three can never disagree with `SUPPLY`.
pub fn split_tokens(sale_bps: u16, pol_bps: u16) -> FloorMathResult<(u64, u64, u64)> {
    let (s, p) = (sale_bps as u64, pol_bps as u64);
    let min = MIN_ZONE_BPS as u64;
    if s < min || p < min || s + p > BPS - min {
        return Err(FloorMathError::BadSplit);
    }
    let sale = SUPPLY / BPS * s;
    let pol = SUPPLY / BPS * p;
    Ok((sale, pol, SUPPLY - sale - pol))
}

// The presets. Each is 20 / 10 / 69 / 1 (sale / classic POL / staircase / open tail; `split_tokens(2_000, 1_000)`
// gives the 70 % graduation deposits for the staircase and the tail together) and carries no reserve wall
// (`crate::stair`). Near the floor only the SKIM moves: buys slide to 0, sells pay `skim_sell_at_floor_multiple_bps`
// (1.5×) of the far skim. The preset rows are read only at create, and a pool records its own staircase
// (`FloorPool.stair_steps`).
//
//   | preset | graduation cap (raise, 20 %) | steps | growth | far skim | creator fee | holder |
//   | MOONSHOT | $25,000 ($5,000) | 15 | 10,000 (equal) | 2 % | 0.5 % | 0 |
//   | COOK | $50,000 ($10,000) | 15 | 10,400 (4 % heavier each step up) | 2 % | 0.75 % | 0 |
//   | BLUECHIP | $100,000 ($20,000) | 14 | 10,400 | 2 % | 1 % | 0 |
//
// A sell INTO the bid pays 1.5× the far skim: 3 % on all three; buys ramp to 0 at the floor.
/// Cook: $50k, raising $10k, fifteen steps to ≈ $1.64B.
pub const COOK_GRADUATION_MCAP_USD: u64 = 50_000; // raises 20 % = $10,000 (20 / 10 / 69 / 1)
/// 0.75 % both sides.
pub const COOK_CREATOR_BPS: u16 = 75;
/// 2 % far skim both sides (3 % on a sell into the bid).
pub const COOK_SKIM_BPS: u16 = 200;
/// No holder rewards.
pub const COOK_HOLDER_BPS: u16 = 0;
/// Cook is 20 / 10 / 69 / 1 (the 70 % `split_tokens` names the wall is the staircase and the tail).
pub const COOK_SALE_BPS: u16 = 2_000;
pub const COOK_POL_BPS: u16 = 1_000;
/// Fifteen steps, BACK-LOADED: each step 4 % heavier than the one below.
pub const COOK_STAIR_STEPS: u8 = 15; // $50k → $1.64B
pub const COOK_STAIR_GROWTH_BPS: u16 = 10_400;
/// Bluechip: $100k, raising $20k, fourteen steps to ≈ $1.64B.
pub const BLUECHIP_GRADUATION_MCAP_USD: u64 = 100_000; // raises 20 % = $20,000 (20 / 10 / 69 / 1)
pub const BLUECHIP_CREATOR_BPS: u16 = 100;
/// 2 % far skim both sides (3 % on a sell into the bid).
pub const BLUECHIP_SKIM_BPS: u16 = 200;
/// No holder rewards (no preset carries a holder rate).
pub const BLUECHIP_HOLDER_BPS: u16 = 0;
pub const BLUECHIP_SALE_BPS: u16 = 2_000;
pub const BLUECHIP_POL_BPS: u16 = 1_000;
/// Fourteen steps, each 4 % heavier than the one below.
pub const BLUECHIP_STAIR_STEPS: u8 = 14; // $100k → $1.64B
pub const BLUECHIP_STAIR_GROWTH_BPS: u16 = 10_400;
/// Moonshot: $25k, fifteen equal steps to ≈ $819M.
pub const MOONSHOT_GRADUATION_MCAP_USD: u64 = 25_000; // raises 20 % = $5,000 (20 / 10 / 69 / 1)
pub const MOONSHOT_CREATOR_BPS: u16 = 50;
/// 2 % far skim both sides (3 % on a sell into the bid).
pub const MOONSHOT_SKIM_BPS: u16 = 200;
/// No holder rewards.
pub const MOONSHOT_HOLDER_BPS: u16 = 0;
pub const MOONSHOT_SALE_BPS: u16 = 2_000;
pub const MOONSHOT_POL_BPS: u16 = 1_000;
/// Fifteen equal steps.
pub const MOONSHOT_STAIR_STEPS: u8 = 15; // $25k → $819M
pub const MOONSHOT_STAIR_GROWTH_BPS: u16 = 10_000;
const _: () = assert!(COOK_SALE_BPS == crate::stair::STAIR_SALE_BPS && COOK_POL_BPS == crate::stair::STAIR_POL_BPS);
const _: () = assert!(BLUECHIP_SALE_BPS == crate::stair::STAIR_SALE_BPS && BLUECHIP_POL_BPS == crate::stair::STAIR_POL_BPS);
const _: () = assert!(MOONSHOT_SALE_BPS == crate::stair::STAIR_SALE_BPS && MOONSHOT_POL_BPS == crate::stair::STAIR_POL_BPS);

/// The EVM's `FloorModes.preset`. No preset carries a holder rate; a Custom launch may carry up to `MAX_HOLDER_BPS`
/// a side (`floor_launch::resolve_config`). ⛔ There is no FLOOR preset, and there must never be one.
pub const fn preset(m: Mode) -> Preset {
    match m {
        Mode::Cook => Preset {
            graduation_mcap_usd: COOK_GRADUATION_MCAP_USD,
            creator_bps: COOK_CREATOR_BPS,
            skim_buy_far_bps: COOK_SKIM_BPS,
            skim_sell_far_bps: COOK_SKIM_BPS,
            holder_bps: COOK_HOLDER_BPS,
            sale_bps: COOK_SALE_BPS,
            pol_bps: COOK_POL_BPS,
            skim_sell_at_floor_multiple_bps: crate::constants::PRESET_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS as u32,
            stair_steps: COOK_STAIR_STEPS,
            stair_growth_bps: COOK_STAIR_GROWTH_BPS,
        },
        Mode::Bluechip => Preset {
            graduation_mcap_usd: BLUECHIP_GRADUATION_MCAP_USD,
            creator_bps: BLUECHIP_CREATOR_BPS,
            skim_buy_far_bps: BLUECHIP_SKIM_BPS,
            skim_sell_far_bps: BLUECHIP_SKIM_BPS,
            holder_bps: BLUECHIP_HOLDER_BPS,
            sale_bps: BLUECHIP_SALE_BPS,
            pol_bps: BLUECHIP_POL_BPS,
            skim_sell_at_floor_multiple_bps: crate::constants::PRESET_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS as u32,
            stair_steps: BLUECHIP_STAIR_STEPS,
            stair_growth_bps: BLUECHIP_STAIR_GROWTH_BPS,
        },
        Mode::Moonshot => Preset {
            graduation_mcap_usd: MOONSHOT_GRADUATION_MCAP_USD,
            creator_bps: MOONSHOT_CREATOR_BPS,
            skim_buy_far_bps: MOONSHOT_SKIM_BPS,
            skim_sell_far_bps: MOONSHOT_SKIM_BPS,
            holder_bps: MOONSHOT_HOLDER_BPS,
            sale_bps: MOONSHOT_SALE_BPS,
            pol_bps: MOONSHOT_POL_BPS,
            skim_sell_at_floor_multiple_bps: crate::constants::PRESET_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS as u32,
            stair_steps: MOONSHOT_STAIR_STEPS,
            stair_growth_bps: MOONSHOT_STAIR_GROWTH_BPS,
        },
        // Custom: the launcher's own cap, rates, split and at-floor sell multiple; its staircase is DERIVED from its cap at
        // create ([`custom_stair`]), so the row carries none.
        Mode::Custom => Preset {
            graduation_mcap_usd: 0,
            creator_bps: 0,
            skim_buy_far_bps: 0,
            skim_sell_far_bps: 0,
            holder_bps: 0,
            sale_bps: 0,
            pol_bps: 0,
            // 0 on the row; `skim_sell_at_floor_multiple_for(Custom)` reads 1.5×.
            skim_sell_at_floor_multiple_bps: 0,
            stair_steps: 0,
            stair_growth_bps: 0,
        },
    }
}

/// `2^77`: the exclusive upper bound on a tranche price, under 8,192 QUOTE BASE UNITS per token
/// base unit, so the whole supply's cost `SUPPLY · price_x64 / 2^64` stays below 2^63 quote base
/// units.
///
/// ⛔ A FIXED-POINT / u64 BOUND, not a unit assumption: what it protects is the narrowing of
/// `SUPPLY · price / 2^64` into a u64, which is the same arithmetic in every quote.
pub const MAX_TRANCHE_PRICE_X64: u128 = 1 << 77;

/// **THE ONE PLACE A DECIMALS COUNT ENTERS THE MECHANISM.** Quote base units per token base unit,
/// × 2^64, at which the whole `SUPPLY` is worth `mcap_usd` whole dollars, given the quote's price
/// in whole CENTS and the quote mint's own `decimals` (the Solana form of the EVM's
/// `tranchePriceFor`, and the twin of the EVM's `Params.quoteUsdCents`).
///
/// `price_x64 = floor(mcap_usd · 100 · 10^d · 2^64 / (quote_usd_cents · SUPPLY))` in U256, rounded
/// DOWN as the EVM's integer division. Requires `0 < price_x64 < 2^77`
/// (`TranchePriceOutOfRange`); `quote_usd_cents == 0` is `ZeroDenominator`; `decimals` outside
/// `MIN_QUOTE_DECIMALS..=MAX_QUOTE_DECIMALS` is `QuoteDecimalsOutOfRange`. No oracle is ever read:
/// the price is the deployer's declared figure, fixed at create.
///
/// ⛔ **Any amount of decimals**: the window is `0..=36` and the REAL ceiling is this function's own
/// `0 < price_x64 < 2^77`, refused BY NAME per (cap, quote price, decimals) triple:
/// `10^d < 8192 · quote_usd_cents · 1e13 / mcap_usd`. That is `d ≤ 14` at a $30,000 cap against a $1.00
/// quote, `d ≤ 14` at $60,000, `d ≤ 11` against a $0.01 quote,
/// decimals constant cannot express a bound that depends on all three, which is why one is not
/// used for it — the window's top, `36`, is the last `d` at which ANY input passes this check
/// (`constants::some_launch_prices_at` is the derivation). ⚠ At the LOW end nothing new binds:
/// `p` floors to zero only for an absurdly small cap against an absurdly dear quote, and that is
/// `TranchePriceOutOfRange` too — while `resolve_config`'s own `backing >= min_bid` check is what
/// refuses a sale that would raise nothing, in the quote's own raw units, at any `d`.
///
/// Why `10^d` and whole CENTS:
/// - **`10^d` replaced a hard-coded `1_000_000_000`.** That literal was wSOL's 9 decimals and had
///   no other derivation. Every other quantity in this crate is a raw-base-unit ratio, so once a
///   pool's tranche price is right for its quote, NOTHING downstream needs to know the decimals —
///   `open_tick`, `sale_tokens_for`, the solve, `position_liquidity`, `take_fees` and the AMM
///   walk all consume base units and Q64.64. ⛔ That is why there is no second conversion and
///   must never be one: a normalisation step inside the mechanism would import the EVM's
///   cancellation dependence into a design that never had it.
/// - **whole CENTS, not whole DOLLARS.** A whole-dollar form carries a SECOND, independent unit
///   assumption: a quote under $1 (`sol_usd = 0`) would be `ZeroDenominator`, and $1.99 would round to a
///   2× price error. With USDC ($1.00) and tokenised equities as quotes, sub-dollar and fractional
///   quote prices are ordinary, not exotic.
///
/// ⚠ `d = 9` with `quote_usd_cents = 100 · sol_usd` reproduces the whole-dollar wSOL form bit for bit: the
/// added factors 100 and 10^9/10^9 cancel inside one `mul_div_floor`. `price_reproduces_the_wsol_whole_dollar_form`
/// below is that proof.
pub fn tranche_price_x64(mcap_usd: u64, quote_usd_cents: u64, quote_decimals: u8) -> FloorMathResult<u128> {
    if quote_usd_cents == 0 {
        return Err(FloorMathError::ZeroDenominator);
    }
    let unit = quote_unit(quote_decimals).ok_or(FloorMathError::QuoteDecimalsOutOfRange)?;
    // num < 2^64 · 100 · 10^36 ≈ 1.8e57 and den < 2^64 · 1e15 ≈ 1.8e34: both inside U256
    // (≈1.16e77) — `uint`'s `*` PANICS on overflow, so this bound is load-bearing — and
    // `mul_div_floor` widens to U512 before the multiply by 2^64. ⚠ The numerator's bound is
    // `10^36`, the largest `quote_unit` can return.
    let num = U256::from(mcap_usd) * U256::from(100u64) * U256::from(unit);
    let den = U256::from(quote_usd_cents) * U256::from(SUPPLY);
    let p = num.mul_div_floor(U256::from(Q64), den).ok_or(FloorMathError::Overflow)?;
    if p.is_zero() || p >= U256::from(MAX_TRANCHE_PRICE_X64) {
        return Err(FloorMathError::TranchePriceOutOfRange);
    }
    Ok(p.as_u128())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{MAX_QUOTE_DECIMALS, MIN_QUOTE_DECIMALS};

    // The staircase presets: the caps, the split and the staircase; the fees; no wall columns.
    // Mutations this kills: a cap moved; a step count or growth moved; a holder rate on a preset; Custom off its
    // all-zero row.
    #[test]
    fn presets_are_the_contracts() {
        let stair = |cap: u64, creator: u16, skim: u16, holder: u16, steps: u8, growth: u16| Preset {
            graduation_mcap_usd: cap, creator_bps: creator, skim_buy_far_bps: skim, skim_sell_far_bps: skim,
            holder_bps: holder, sale_bps: 2_000, pol_bps: 1_000, skim_sell_at_floor_multiple_bps: 15_000, stair_steps: steps, stair_growth_bps: growth,
        };
        assert_eq!(preset(Mode::Moonshot), stair(25_000, 50, 200, 0, 15, 10_000));
        assert_eq!(preset(Mode::Cook), stair(50_000, 75, 200, 0, 15, 10_400));
        assert_eq!(preset(Mode::Bluechip), stair(100_000, 100, 200, 0, 14, 10_400));
        for m in [Mode::Cook, Mode::Bluechip, Mode::Moonshot] {
            let p = preset(m);
            assert!(p.is_stair(), "{m:?} is a staircase preset");
            assert!(crate::stair::stair_shape_ok(p.stair_steps, p.stair_growth_bps), "{m:?}'s staircase is admitted");
            assert!(split_tokens(p.sale_bps, p.pol_bps).is_ok(), "{m:?}'s split breaks the rule");
            assert!(p.holder_bps <= crate::policy::MAX_HOLDER_BPS, "{m:?}'s holder rate under the cap");
            assert_eq!(p.holder_bps, 0, "{m:?}: no preset carries a holder rate");
        }
        // the at-floor sell each preset registers with — 1.5x far: 2 % far -> 3 % on all three
        let at = |m: Mode| crate::constants::skim_sell_at_floor_bps_at(preset(m).skim_sell_far_bps, skim_sell_at_floor_multiple_for(m) as u64);
        assert_eq!(at(Mode::Cook), 300);
        assert_eq!(at(Mode::Moonshot), 300);
        assert_eq!(at(Mode::Bluechip), 300);
        assert_eq!(skim_sell_at_floor_multiple_for(Mode::Custom), 15_000, "Custom (and $FLOOR) keep 1.5x");
        assert_eq!(preset(Mode::Custom), Preset::default());
        // ⛔ Custom's ROW carries no staircase — its steps are derived from its own cap at create (`custom_stair`),
        // never a table value.
        assert!(!preset(Mode::Custom).is_stair(), "Custom's row: steps derived from the cap at create");
        assert_eq!(custom_stair(200_000), (12, 10_000));
        for b in [0u8, 1, 2, 3] {
            assert_eq!(Mode::try_from(b).unwrap() as u8, b);
        }
        // Custom 0, Moonshot 1, Cook 2, Bluechip 3 — the EVM's `FloorModes.Mode` too.
        assert_eq!(
            (Mode::Custom as u8, Mode::Moonshot as u8, Mode::Cook as u8, Mode::Bluechip as u8),
            (0, 1, 2, 3)
        );
        // ⛔ 4 is refused: the FIRST byte past the enum. The staircase is a preset
        //    REVISION on the same bytes, recorded per pool (`FloorPool.stair_steps`), so no byte was appended for it.
        for b in 4u8..=255 {
            assert_eq!(Mode::try_from(b), Err(FloorMathError::UnknownMode), "{b}");
        }
    }

    /// The staircase presets. Each preset resolves to EXACTLY this table, stated in its own units (dollars, a step
    /// count, a weighting) and converted here independently of the constants.
    ///
    /// | | Moonshot | Cook | Bluechip |
    /// |---|---|---|---|
    /// | starting MC · raise | $25k · $5k | $50k · $10k | $100k · $20k |
    /// | steps · weighting | 15 · equal | 15 · × 1.04 a step up | 14 · × 1.04 a step up |
    /// | supply | 20 sale / 10 classic POL / 69 staircase / 1 open tail | same | same |
    ///
    /// The steps reach about $1 billion (Moonshot $819M, Cook and Bluechip $1.64B), to reduce sell impact near the top.
    ///
    /// Mutations this kills: a cap moved; the raise not 20 % of the cap; a step count or a weighting moved; the split
    /// moved off 20 / 10.
    #[test]
    fn the_staircase_presets_are_the_table() {
        let table = [
            (Mode::Moonshot, 25_000u64, 5_000u64, 15u8, 1.0f64),
            (Mode::Cook, 50_000, 10_000, 15, 1.04),
            (Mode::Bluechip, 100_000, 20_000, 14, 1.04),
        ];
        for (m, cap, raise, steps, growth) in table {
            let p = preset(m);
            assert_eq!(p.graduation_mcap_usd, cap, "{m:?} cap");
            let (sale, pol, rest) = split_tokens(p.sale_bps, p.pol_bps).unwrap();
            assert_eq!((cap as u128 * sale as u128 / SUPPLY as u128) as u64, raise, "{m:?}: the raise is 20 % of the cap");
            assert_eq!((sale, pol), (SUPPLY / 5, SUPPLY / 10), "{m:?} sale and classic POL");
            let (st, tail) = crate::stair::stair_tail_split(rest);
            assert_eq!((st, tail), (SUPPLY / 100 * 69, SUPPLY / 100), "{m:?}: 69 staircase / 1 tail");
            assert_eq!(p.stair_steps, steps, "{m:?} steps");
            assert_eq!(p.stair_growth_bps, (growth * 10_000.0).round() as u16, "{m:?} weighting");
            // Steps double the price from the start to about $1B.
            let top = cap as f64 * 2f64.powi(steps as i32);
            assert!((800e6..=1_700e6).contains(&top), "{m:?}: the staircase tops out at ${top}");
        }
    }

    /// Each preset's fees, stated in percent and converted here independently of the constants; every other column
    /// is the staircase's (`the_staircase_presets_are_the_table`).
    ///
    /// | | Moonshot | Cook | Bluechip |
    /// |---|---|---|---|
    /// | far floor skim, buy and sell | 2 % | 2 % | 2 % |
    /// | creator fee, both sides | 0.5 % | 0.75 % | 1 % |
    /// | holder rewards, both sides | 0 | 0 | 0 |
    /// | a sell INTO the bid | 1.5× far = 3 % | 1.5× = 3 % | 1.5× = 3 % |
    ///
    /// Mutations this kills: any fee of any preset moved; the preset multiple left at 1.5×; Custom moved off 1.5×
    /// (it would move `$FLOOR`'s config); a holder rate on a preset.
    #[test]
    fn the_new_fees_are_the_table() {
        let bps = |pct: f64| (pct * 100.0).round() as u16;
        // (mode, far skim %, creator %, holder %, at-floor sell %)
        let table = [
            (Mode::Moonshot, 2.0, 0.5, 0.0, 3.0),
            (Mode::Cook, 2.0, 0.75, 0.0, 3.0),
            (Mode::Bluechip, 2.0, 1.0, 0.0, 3.0),
        ];
        for (m, skim, creator, holder, at_floor) in table {
            let p = preset(m);
            assert_eq!((p.skim_buy_far_bps, p.skim_sell_far_bps), (bps(skim), bps(skim)), "{m:?} far skim");
            assert_eq!(p.creator_bps, bps(creator), "{m:?} creator");
            assert_eq!(p.holder_bps, bps(holder), "{m:?} holder");
            assert_eq!(p.skim_sell_at_floor_multiple_bps, 15_000, "{m:?}: 1.5x into the bid");
            assert_eq!(skim_sell_at_floor_multiple_for(m), 15_000);
            let at = crate::constants::skim_sell_at_floor_bps_at(p.skim_sell_far_bps, skim_sell_at_floor_multiple_for(m) as u64);
            assert_eq!(at, bps(at_floor), "{m:?} at-floor sell");
            assert!(p.skim_sell_far_bps <= crate::policy::MAX_SKIM_FAR_BPS && p.creator_bps <= crate::policy::MAX_CREATOR_BPS);
            let multiple = u64::from(p.skim_sell_at_floor_multiple_bps);
            assert!(
                (crate::constants::MIN_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS..=crate::constants::MAX_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS).contains(&multiple),
                "{m:?}'s multiple inside register_pool's window"
            );
        }
        // Custom — and `$FLOOR`'s genesis, a Custom launch — keep 1.5x (`$FLOOR`'s 2.5 % far -> 3.75 % at the floor).
        assert_eq!(skim_sell_at_floor_multiple_for(Mode::Custom) as u64, crate::constants::SKIM_SELL_AT_FLOOR_MULTIPLE_BPS);
        assert_eq!(crate::constants::skim_sell_at_floor_bps_at(250, skim_sell_at_floor_multiple_for(Mode::Custom) as u64), 375);
        assert_eq!(preset(Mode::Custom).skim_sell_at_floor_multiple_bps, 0, "Custom's row is all zero");
    }

    // A $5,000 cap at SOL $100 is 2^64/20,000 lamports per base unit × 2^64, i.e. the whole
    // supply costs 50 SOL = $5,000 (an example Custom cap, not a preset read). ⛔ Do not "re-anchor"
    // these literals onto another cap by multiplying: floor(x)·4 != floor(4x) here (it lands 2 short),
    // so a transplanted constant would be wrong.
    /// The wSOL vectors in the three-argument signature: whole dollars are `100 · cents` and wSOL is
    /// 9 decimals; the literals are hand-computed.
    #[test]
    fn tranche_price_matches_hand_computation() {
        let p = tranche_price_x64(5_000, 100 * 100, 9).unwrap();
        assert_eq!(p, (1u128 << 64) / 20_000);
        assert_eq!(p, 922_337_203_685_477);
        let whole_supply = (U256::from(SUPPLY) * U256::from(p)) >> 64usize;
        assert_eq!(whole_supply.as_u64(), 49_999_999_999); // 50 SOL, floored twice
        // equal to floor(mcap·2^64/(sol·1e6)) across the modes and a range of SOL prices
        for mcap in [5_000u64, 20_000, 100_000, 50_000] {
            for sol in [1u64, 17, 100, 250, 2_400, 100_000] {
                let direct = (U256::from(mcap) << 64usize) / (U256::from(sol) * U256::from(1_000_000u64));
                assert_eq!(U256::from(tranche_price_x64(mcap, sol * 100, 9).unwrap()), direct, "{mcap}@{sol}");
            }
        }
    }

    /// **The regression bar for wSOL**: for EVERY (mcap, whole-dollar SOL
    /// price) pair, the new three-argument call at `(mcap, sol·100, 9)` equals the OLD two-argument
    /// formula `floor(mcap·1e9·2^64 / (sol·1e15))`, recomputed here from scratch with no shared code.
    ///
    /// Mutations this kills: `100` dropped from the numerator (every price 100x too cheap);
    /// `quote_unit` consulted for the DENOMINATOR instead of the numerator; floor changed to ceil.
    #[test]
    fn price_reproduces_the_wsol_whole_dollar_form() {
        for mcap in [1u64, 5_000, 20_000, 100_000, 8_191_999] {
            for sol in [1u64, 3, 17, 100, 101, 250, 2_400, 100_000] {
                let old = (U256::from(mcap) * U256::from(1_000_000_000u64))
                    .mul_div_floor(U256::from(Q64), U256::from(sol) * U256::from(SUPPLY))
                    .unwrap();
                let want = if old.is_zero() || old >= U256::from(MAX_TRANCHE_PRICE_X64) {
                    None
                } else {
                    Some(old.as_u128())
                };
                assert_eq!(tranche_price_x64(mcap, sol * 100, 9).ok(), want, "{mcap} at {sol} USD/SOL");
            }
        }
    }

    /// The EVM differential harness feeds this function the EVM’s own
    /// WHOLE-DOLLAR price column, and does it by dropping the decimals TWO rungs rather than
    /// multiplying the price by 100: `(mcap, usd, d-2) == (mcap, usd·100, d)` exactly, because the
    /// numerator carries `100 · 10^d` and `100·10^(d-2) = 10^d` -- one division, one floor, same
    /// result. ⛔ The harness needs the identity to be EXACT: `usd·100` overflows a u64 column
    /// above ~1.8e17 and a saturating product would price such a row 5.4x high.
    ///
    /// Mutation this kills: the `100` moved from the numerator to the denominator (the two forms
    /// then differ by 10^4).
    #[test]
    fn the_evm_whole_dollar_shape_is_two_decimals_down() {
        for mcap in [1u64, 5_000, 20_000, 100_000, 8_191_999] {
            for usd in [1u64, 3, 17, 150, 2_400, 100_000, 1_000_000_000] {
                for d in (MIN_QUOTE_DECIMALS + 2)..=MAX_QUOTE_DECIMALS {
                    let Some(x100) = usd.checked_mul(100) else { continue };
                    assert_eq!(
                        tranche_price_x64(mcap, usd, d - 2).ok(),
                        tranche_price_x64(mcap, x100, d).ok(),
                        "{mcap} at {usd} USD, d={d}"
                    );
                }
            }
        }
    }

    /// **THE DECIMALS PROPERTY**, stated as an equation rather than as a table of magic numbers: the
    /// whole supply must cost the graduation cap in the QUOTE's own money, whatever that quote's
    /// decimals are. The oracle is `whole_supply_cost = mcap_usd · 100 · 10^d / cents`, floored --
    /// the dollar arithmetic done independently of the function under test.
    ///
    /// ⛔ **THE ORACLE IS INDEPENDENT**: it raises ten to the power `d` itself rather than calling
    /// `quote_unit(d)`, so the mutation "`quote_unit` returns a constant" cannot move the oracle and
    /// the subject together.
    ///
    /// Mutations this kills: ANY 10^k error (the whole point -- a 10^3 slip shows up as a 1000x
    /// cost here and nowhere else); `10^d` hard-coded back to 1e9; `quote_unit` returning a
    /// constant for every `d`; the cents factor dropped.
    #[test]
    fn the_whole_supply_costs_the_cap_in_every_admissible_quote() {
        // ⛔ The window's own high end cannot be walked with a fixed (mcap, cents): `2^77` binds
        //    first and by VALUE. Walk to the last `d` these inputs can still price, and assert
        //    separately that the refusal past it is the price bound and not the decimals one.
        for d in MIN_QUOTE_DECIMALS..=MAX_QUOTE_DECIMALS {
            // ⛔ INDEPENDENT of `quote_unit`: ten multiplied by itself `d` times, in u128.
            let unit: u128 = (0..d).fold(1u128, |a, _| a * 10);
            // $1.00 (USDC), $0.50 and $1.99 (the sub-dollar and fractional cases whole dollars
            // could not express at all), $101.52 (a SOL price) and $230.00 (an equity-like price).
            for cents in [100u64, 50, 199, 10_152, 23_000] {
                for mcap in [20_000u64, 40_000, 100_000] {
                    let p = match tranche_price_x64(mcap, cents, d) {
                        Ok(p) => p,
                        // ⛔ The ONLY refusal admissible inside the window is the price bound, and
                        //    it must be that one BY NAME: a `QuoteDecimalsOutOfRange` here would
                        //    mean the window and `quote_unit` had drifted apart.
                        Err(e) => {
                            assert_eq!(e, FloorMathError::TranchePriceOutOfRange, "d={d} cents={cents} mcap={mcap}");
                            continue;
                        }
                    };
                    let got = ((U256::from(SUPPLY) * U256::from(p)) >> 64usize).as_u128();
                    let want = (mcap as u128) * 100 * unit / (cents as u128);
                    // Two floors (the price, then the supply walk) can lose only dust: the price
                    // floor costs at most SUPPLY/2^64 of a base unit, the walk at most one.
                    // ⛔ `got <= want` FIRST: under the 1e9 mutant `got` is 10^(9-d)× too big and a
                    //    bare `want - got` would die of an underflow instead of by this name.
                    assert!(
                        got <= want && want - got <= 1,
                        "d={d} cents={cents} mcap={mcap}: whole supply costs {got}, cap is {want}"
                    );
                }
            }
        }
        // ⛔ The two extremes, asserted rather than left to the loop: a 0-decimal and a 2-decimal
        //    quote must PRICE, not merely not panic. At a $20,000 cap and a quote of $1.00 a WHOLE
        //    unit, the whole supply costs 20,000 whole units of it — 20,000 raw units at d=0 and
        //    2,000,000 at d=2.
        assert_eq!(
            ((U256::from(SUPPLY) * U256::from(tranche_price_x64(20_000, 100, 0).unwrap())) >> 64usize).as_u128(),
            20_000 - 1,
            "a 0-decimal quote: the whole supply costs the cap in whole units of it, less one floor"
        );
        assert_eq!(
            ((U256::from(SUPPLY) * U256::from(tranche_price_x64(20_000, 100, 2).unwrap())) >> 64usize).as_u128(),
            2_000_000 - 1,
            "a 2-decimal quote: a hundred raw units to the whole unit"
        );
    }

    /// One decimal step is one factor of ten, TO THE FLOOR. The RATIO is
    /// what a 10^k bug breaks, so it is asserted directly.
    ///
    /// ⚠ Not `hi == lo * 10` exactly: with `x = mcap·100·10^d·2^64/(cents·SUPPLY)` the function
    /// returns `floor(x)` at `d` and `floor(10x)` at `d+1`, and `floor(10x) ∈ [10·floor(x),
    /// 10·floor(x) + 9]` -- one floor of a ten-times-finer quantity keeps up to nine extra units.
    /// Exact equality fails at d=7 by 3 (36893488147419103 vs ...100), which is the floor, not a
    /// scale error; the window below is the exact algebraic bound.
    ///
    /// Mutations this kills: `quote_unit` returning a constant (the ratio becomes 1); the power
    /// applied to the denominator (the ratio inverts).
    #[test]
    fn one_decimal_step_is_exactly_one_factor_of_ten() {
        // ⚠ The window is 0..=36, and `2^77` stops these inputs before the window does — so the
        //   ladder is walked while BOTH rungs price, and the first `d` that does not is asserted to
        //   be the PRICE bound rather than the decimals one.
        let mut rungs = 0usize;
        for d in MIN_QUOTE_DECIMALS..MAX_QUOTE_DECIMALS {
            let (Ok(lo), hi) = (tranche_price_x64(20_000, 100, d), tranche_price_x64(20_000, 100, d + 1)) else {
                break;
            };
            let Ok(hi) = hi else {
                assert_eq!(hi.unwrap_err(), FloorMathError::TranchePriceOutOfRange, "d={}", d + 1);
                break;
            };
            assert!(hi >= lo * 10 && hi <= lo * 10 + 9, "{d} -> {}: {lo} then {hi}", d + 1);
            rungs += 1;
        }
        // ⛔ The ladder must START at 0 (the low extreme) and be long enough to be a ladder at all; a
        //    `quote_unit` stuck at a constant makes every rung equal and fails above, a window that no
        //    longer includes 0 fails here.
        assert!(tranche_price_x64(20_000, 100, 0).is_ok(), "a 0-decimal quote prices");
        assert!(rungs >= 14, "only {rungs} rungs: the 2^77 bound should allow d = 0..=14 here");
        assert!(rungs <= 20, "{rungs} rungs: the cap has stopped binding, which it cannot at $1.00");
    }

    #[test]
    fn tranche_price_bounds() {
        assert_eq!(tranche_price_x64(5_000, 0, 9), Err(FloorMathError::ZeroDenominator));
        assert_eq!(tranche_price_x64(0, 100 * 100, 9), Err(FloorMathError::TranchePriceOutOfRange));
        // price < 1 unit per 2^64: floors to 0
        assert_eq!(tranche_price_x64(1, u64::MAX, 9), Err(FloorMathError::TranchePriceOutOfRange));
        // far above the cap
        assert_eq!(tranche_price_x64(u64::MAX, 100, 9), Err(FloorMathError::TranchePriceOutOfRange));
        // the largest accepted mcap at sol $1: price just under 2^77 -> mcap_usd < 8192·1e6
        assert!(tranche_price_x64(8_191_999_999, 100, 9).is_ok());
        assert_eq!(tranche_price_x64(8_192_000_000, 100, 9), Err(FloorMathError::TranchePriceOutOfRange));
    }

    /// Decimals outside the window are refused BY NAME, and INSIDE it the decimals refusal never fires — whatever
    /// else does. The subject is `d`, so the only thing asserted about an in-window `d` is that its verdict is not
    /// about `d`.
    ///
    /// ⛔ **THE TWO EXTREMES ARE ASSERTED TO WORK, not merely to not revert**: `d = 0` and `d = 2` price, and their
    /// prices stand in the exact 1:100 ratio.
    ///
    /// Mutations this kills: the window narrowed back to `6..=9` (d=0 and d=2 would refuse); the
    /// check moved after the divide; `quote_unit` saturating at d=37 instead of returning `None`;
    /// a `QuoteDecimalsOutOfRange` raised for a value fault.
    ///
    /// ⛔ AND THE TOP OF THE WINDOW IS SHOWN TO BE THE TOP: `d = 36` PRICES for the one input the
    /// derivation names (a $1 cap against `u64::MAX` cents) and `d = 37` is refused BY NAME as a
    /// decimals fault — while `d = 36` against an ORDINARY input is a VALUE fault, which is what
    /// "the effective ceiling is value-shaped" means in a test.
    #[test]
    fn decimals_outside_the_window_are_refused_by_name() {
        assert!(tranche_price_x64(1, u64::MAX, 36).is_ok(), "the derivation's own input prices at 36");
        // 10^23 · 2^64 / (2^64 − 1): the 5,421 is the `u64::MAX` denominator, not 2^64
        assert_eq!(tranche_price_x64(1, u64::MAX, 36).unwrap(), 10u128.pow(23) + 5_421);
        assert_eq!(tranche_price_x64(1, u64::MAX, 37), Err(FloorMathError::QuoteDecimalsOutOfRange));
        assert_eq!(tranche_price_x64(20_000, 100, 36), Err(FloorMathError::TranchePriceOutOfRange));
        assert_eq!(
            tranche_price_x64(20_000, 100, 20),
            Err(FloorMathError::TranchePriceOutOfRange),
            "20 is INSIDE the window"
        );
        for d in 0..=255u8 {
            let r = tranche_price_x64(20_000, 15_000, d);
            if (MIN_QUOTE_DECIMALS..=MAX_QUOTE_DECIMALS).contains(&d) {
                assert_ne!(r, Err(FloorMathError::QuoteDecimalsOutOfRange), "d={d} is admissible");
            } else {
                assert_eq!(r, Err(FloorMathError::QuoteDecimalsOutOfRange), "d={d}");
            }
        }
        // ⛔ d = 0 and d = 2 WORK, and one step of two decimals is exactly a hundred.
        let p0 = tranche_price_x64(20_000, 15_000, 0).expect("a 0-decimal quote prices");
        let p2 = tranche_price_x64(20_000, 15_000, 2).expect("a 2-decimal quote prices");
        assert!(p2 >= p0 * 100 && p2 <= p0 * 100 + 99, "{p0} then {p2}");
        // ⛔ A zero price is still ZeroDenominator, which is what tells a caller WHICH input was
        // wrong; the decimals refusal must not swallow it — at an in-window `d` or past it.
        assert_eq!(tranche_price_x64(20_000, 0, 18), Err(FloorMathError::ZeroDenominator));
        assert_eq!(tranche_price_x64(20_000, 0, 99), Err(FloorMathError::ZeroDenominator));
    }

    /// The CENTS half. Whole dollars could not express a quote under $1 at all (it would read as zero) and would
    /// round $1.99 to $1. Both are ordinary once a quote may be USDC or a tokenised equity.
    ///
    /// Mutation this kills: a caller "helpfully" re-rounding cents to whole dollars.
    #[test]
    fn a_sub_dollar_quote_prices_and_a_fractional_one_is_not_rounded() {
        // $0.99: under the old whole-dollar signature this was sol_usd = 0 -> ZeroDenominator.
        assert!(tranche_price_x64(20_000, 99, 6).is_ok());
        // $1.99 is neither $1 nor $2: strictly between the two prices.
        let p199 = tranche_price_x64(20_000, 199, 6).unwrap();
        let p100 = tranche_price_x64(20_000, 100, 6).unwrap();
        let p200 = tranche_price_x64(20_000, 200, 6).unwrap();
        assert!(p200 < p199 && p199 < p100, "{p200} < {p199} < {p100}");
    }

    /// The supply split: each zone >= 10 %, the three = supply. ⛔ Mutations:
    /// the wall's own minimum dropped (90 / 10 / 0 resolves); `<` for `<=` at a boundary; the wall
    /// computed as a third share instead of the remainder.
    #[test]
    fn split_tokens_holds_each_zone_to_ten_percent_and_the_sum_to_supply() {
        let tenth = SUPPLY / 10;
        assert_eq!(split_tokens(3_000, 1_000), Ok((3 * tenth, tenth, 6 * tenth)));
        assert_eq!(split_tokens(1_000, 1_000), Ok((tenth, tenth, 8 * tenth)));
        assert_eq!(split_tokens(8_000, 1_000), Ok((8 * tenth, tenth, tenth)));
        assert_eq!(split_tokens(1_000, 8_000), Ok((tenth, 8 * tenth, tenth)));
        for (s, p) in [(999, 1_000), (1_000, 999), (8_001, 1_000), (1_000, 8_001), (4_500, 4_501), (0, 0), (9_000, 1_000)] {
            assert_eq!(split_tokens(s, p), Err(FloorMathError::BadSplit), "{s} / {p}");
        }
        for (s, p) in [(1_000, 1_000), (7_000, 1_000), (3_333, 3_333), (8_000, 1_000)] {
            let (a, b, c) = split_tokens(s, p).unwrap();
            assert_eq!(a + b + c, SUPPLY);
        }
    }
}
