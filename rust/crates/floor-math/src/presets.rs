//! ⛔ THE PRESET REGISTRY'S ONE RULE TABLE. A preset is a ROW of `floor_launch`'s `["preset_registry"]` account;
//! [`validate_preset_row`] is the ONE validator `add_preset` runs, refusing by the FIELD's name (`PresetInvalid`), and
//! the EVM's `FloorPresetRegistry.addPreset` runs the same table. [`seed_row`] builds the three rows
//! `init_preset_registry` writes — 1 Moonshot, 2 Cook, 3 Bluechip — from [`crate::modes::preset`]'s constants,
//! unchanged, plus the team-buy ceilings and cliffs.
//!
//! ⛔ Nothing here branches on "the four" for a LAUNCH: `floor_launch::sale::resolve_config` reads the stored row for any
//! id ≠ 0. [`crate::modes::Mode`] / [`crate::modes::preset`] stay as the NAMED SEED IDS and their values.
//! ⛔ No `floor-types` here; the row's on-chain twin is `floor_types::layout::PresetRowZc`, and the constants this
//! file copies are held equal by `tests/floor_types_pin.rs`.

use crate::constants::{MAX_SKIM_SELL_AT_FLOOR_BPS, MAX_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS, MIN_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS, PRESET_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS};
use crate::modes::{preset, split_tokens, Mode};
use crate::owners::floor_types_team_buy::{BLUECHIP_TEAM_BUY_MAX_BPS, TEAM_BUY_MAX_BPS};
use crate::policy::{MAX_CREATOR_BPS, MAX_SKIM_FAR_BPS};
use crate::stair::MAX_STAIR_STEPS;

/// `floor_types::constants::MAX_PRESET_ID`: 32 slots counting Custom's.
pub const MAX_PRESET_ID: u8 = 31;
/// The seeded ids are 1..=SEEDED_PRESETS (Moonshot, Cook, Bluechip); a fresh registry's `count` is `SEEDED_PRESETS + 1`.
pub const SEEDED_PRESETS: u8 = 3;
/// A row's growth between consecutive steps, bps: 0.5×..2×.
pub const MIN_PRESET_STAIR_GROWTH_BPS: u16 = 5_000;
pub const MAX_PRESET_STAIR_GROWTH_BPS: u16 = 20_000;
/// The four wallet cliffs' bounds (`floor_types::constants::TEAM_CUSTOM_VEST_DAYS_MIN` / `_MAX`, the EVM's
/// `MIN/MAX_TEAM_BUY_CLIFF_DAYS`): each 10..=365 days, strictly rising.
pub const TEAM_VEST_DAYS_MIN: u16 = 10;
pub const TEAM_VEST_DAYS_MAX: u16 = 365;
/// The preset cliffs (`floor_types::constants::TEAM_VEST_DAYS_COOK_MOONSHOT` / `TEAM_VEST_DAYS_BLUECHIP`): presets vest
/// 10 / 20 / 40 / 80 days, Bluechip 30 / 60 / 90 / 120 days.
pub const TEAM_VEST_DAYS_COOK_MOONSHOT: [u16; 4] = [10, 20, 40, 80];
pub const TEAM_VEST_DAYS_BLUECHIP: [u16; 4] = [30, 60, 90, 120];

/// A preset's numbers in chain units, in the rule table's (and `PresetRowArgs`' Borsh) order. `enabled` is not a
/// field here: `add_preset` writes it `1`, and only `set_preset_enabled` moves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PresetRow {
    /// Whole USD.
    pub graduation_mcap_usd: u64,
    /// Both sides.
    pub creator_bps: u16,
    pub skim_buy_far_bps: u16,
    pub skim_sell_far_bps: u16,
    /// Both sides.
    pub holder_bps: u16,
    pub sale_bps: u16,
    pub pol_bps: u16,
    pub skim_sell_at_floor_multiple_bps: u32,
    pub stair_steps: u8,
    pub stair_growth_bps: u16,
    pub team_buy_max_bps: u16,
    pub team_vest_days: [u16; 4],
}

/// The field a row was refused on (`PresetInvalid(field)`), named as the wire spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresetField {
    GraduationMcapUsd,
    CreatorBps,
    SkimBuyFarBps,
    SkimSellFarBps,
    HolderBps,
    SaleBps,
    PolBps,
    SkimSellAtFloorMultipleBps,
    StairSteps,
    StairGrowthBps,
    TeamBuyMaxBps,
    TeamVestDays,
}

impl PresetField {
    /// The field's name on the Solana wire (`PresetRowArgs`).
    pub const fn name(self) -> &'static str {
        match self {
            PresetField::GraduationMcapUsd => "graduation_mcap_usd",
            PresetField::CreatorBps => "creator_bps",
            PresetField::SkimBuyFarBps => "skim_buy_far_bps",
            PresetField::SkimSellFarBps => "skim_sell_far_bps",
            PresetField::HolderBps => "holder_bps",
            PresetField::SaleBps => "sale_bps",
            PresetField::PolBps => "pol_bps",
            PresetField::SkimSellAtFloorMultipleBps => "skim_sell_at_floor_multiple_bps",
            PresetField::StairSteps => "stair_steps",
            PresetField::StairGrowthBps => "stair_growth_bps",
            PresetField::TeamBuyMaxBps => "team_buy_max_bps",
            PresetField::TeamVestDays => "team_vest_days",
        }
    }
}

/// THE ONE VALIDATOR, checked in the table's order; the first rule a row breaks names its field.
/// - `graduation_mcap_usd > 0` (Custom's cap rule in `resolve_config`; the price checks need a quote and run at create);
/// - `creator_bps ≤ MAX_CREATOR_BPS`, each far skim `≤ MAX_SKIM_FAR_BPS`;
/// - ⛔ `holder_bps == 0` (as the EVM's `FloorPresetRegistry._validate`, which refuses `holderBps != 0` with
///   `PresetInvalid("holderBps")`): a preset carries NO holder rate —
///   only Custom may set one (`MAX_HOLDER_BPS` still caps Custom's);
/// - the split: [`split_tokens`]' rule (each ≥ 10 %, sale + POL ≤ 90 %) — the sale named when it alone is out of
///   10 %..=80 %, the POL otherwise (the EVM's naming);
/// - the multiple `10,000..=50,000` and `far sell × it / 10,000 ≤ MAX_SKIM_SELL_AT_FLOOR_BPS` (the at-floor cap, by the
///   same floored product `skim_sell_at_floor_bps_at` registers);
/// - `stair_steps` 1..=16, `stair_growth_bps` 5,000..=20,000;
/// - `team_buy_max_bps ≤ BLUECHIP_TEAM_BUY_MAX_BPS` (1,000);
/// - the four cliffs strictly rising, each 10..=365 days.
pub fn validate_preset_row(r: &PresetRow) -> Result<(), PresetField> {
    use PresetField::*;
    if r.graduation_mcap_usd == 0 {
        return Err(GraduationMcapUsd);
    }
    if r.creator_bps > MAX_CREATOR_BPS {
        return Err(CreatorBps);
    }
    if r.skim_buy_far_bps > MAX_SKIM_FAR_BPS {
        return Err(SkimBuyFarBps);
    }
    if r.skim_sell_far_bps > MAX_SKIM_FAR_BPS {
        return Err(SkimSellFarBps);
    }
    if r.holder_bps != 0 {
        return Err(HolderBps);
    }
    if split_tokens(r.sale_bps, r.pol_bps).is_err() {
        // The field named is the EVM's (`FloorPresetRegistry._validate`): the SALE when it alone
        // is out of its own window (< 10 %, or > 80 % — no POL could then fit), the POL otherwise (short, or the sum > 90 %).
        let min = crate::modes::MIN_ZONE_BPS;
        let sale_alone_bad = r.sale_bps < min || r.sale_bps as u64 > crate::constants::BPS - 2 * min as u64;
        return Err(if sale_alone_bad { SaleBps } else { PolBps });
    }
    let m = r.skim_sell_at_floor_multiple_bps as u64;
    if !(MIN_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS..=MAX_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS).contains(&m)
        || crate::constants::skim_sell_at_floor_bps_at(r.skim_sell_far_bps, m) as u64
            > MAX_SKIM_SELL_AT_FLOOR_BPS
    {
        return Err(SkimSellAtFloorMultipleBps);
    }
    if !(1..=MAX_STAIR_STEPS).contains(&r.stair_steps) {
        return Err(StairSteps);
    }
    if !(MIN_PRESET_STAIR_GROWTH_BPS..=MAX_PRESET_STAIR_GROWTH_BPS).contains(&r.stair_growth_bps) {
        return Err(StairGrowthBps);
    }
    if r.team_buy_max_bps as u64 > BLUECHIP_TEAM_BUY_MAX_BPS {
        return Err(TeamBuyMaxBps);
    }
    let d = r.team_vest_days;
    if d.iter().any(|x| !(TEAM_VEST_DAYS_MIN..=TEAM_VEST_DAYS_MAX).contains(x)) || d.windows(2).any(|w| w[0] >= w[1]) {
        return Err(TeamVestDays);
    }
    Ok(())
}

/// The seeded row for id 1..=3 (`None` for any other id): today's [`preset`] values, unchanged, with the team-buy ceiling
/// (Bluechip 10 %, the others 5 %) and the cliffs (Moonshot / Cook 10 / 20 / 40 / 80, Bluechip 30 / 60 / 90 / 120).
pub fn seed_row(id: u8) -> Option<PresetRow> {
    let mode = match id {
        1..=SEEDED_PRESETS => Mode::try_from(id).ok()?,
        _ => return None,
    };
    let p = preset(mode);
    let (team_buy_max_bps, team_vest_days) = match mode {
        Mode::Bluechip => (BLUECHIP_TEAM_BUY_MAX_BPS as u16, TEAM_VEST_DAYS_BLUECHIP),
        _ => (TEAM_BUY_MAX_BPS as u16, TEAM_VEST_DAYS_COOK_MOONSHOT),
    };
    Some(PresetRow {
        graduation_mcap_usd: p.graduation_mcap_usd,
        creator_bps: p.creator_bps,
        skim_buy_far_bps: p.skim_buy_far_bps,
        skim_sell_far_bps: p.skim_sell_far_bps,
        holder_bps: p.holder_bps,
        sale_bps: p.sale_bps,
        pol_bps: p.pol_bps,
        skim_sell_at_floor_multiple_bps: p.skim_sell_at_floor_multiple_bps,
        stair_steps: p.stair_steps,
        stair_growth_bps: p.stair_growth_bps,
        team_buy_max_bps,
        team_vest_days,
    })
}

// `<=`, not `==` (the presets sit at 1.5×): the seeds sit inside the window governance rows may use;
// the window's top (5×) is no longer the seeds' multiple, and must stay open for rows added later.
const _: () = assert!(PRESET_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS >= MIN_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS && PRESET_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS <= MAX_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS);
const _: () = assert!((MAX_PRESET_ID as usize) < 32 && SEEDED_PRESETS < MAX_PRESET_ID);

#[cfg(test)]
mod tests {
    use super::*;
    use PresetField::*;

    /// A row in the middle of every range ("Monkey", id 4 in the design): every rule is tested by moving ONE field off it.
    fn monkey() -> PresetRow {
        PresetRow {
            graduation_mcap_usd: 75_000,
            creator_bps: 60,
            skim_buy_far_bps: 50,
            skim_sell_far_bps: 50,
            holder_bps: 0,
            sale_bps: 2_000,
            pol_bps: 1_000,
            skim_sell_at_floor_multiple_bps: 30_000,
            stair_steps: 12,
            stair_growth_bps: 10_200,
            team_buy_max_bps: 700,
            team_vest_days: [15, 30, 60, 90],
        }
    }

    fn with(f: impl FnOnce(&mut PresetRow)) -> Result<(), PresetField> {
        let mut r = monkey();
        f(&mut r);
        validate_preset_row(&r)
    }

    #[test]
    fn a_row_inside_every_rule_passes() {
        assert_eq!(validate_preset_row(&monkey()), Ok(()));
    }

    /// Each rule at its boundary: the last legal value passes, one past it is refused BY THAT FIELD'S NAME.
    #[test]
    fn each_rule_is_refused_by_name_at_its_boundary() {
        // the cap
        assert_eq!(with(|r| r.graduation_mcap_usd = 1), Ok(()));
        assert_eq!(with(|r| r.graduation_mcap_usd = 0), Err(GraduationMcapUsd));
        // the four rates
        assert_eq!(with(|r| r.creator_bps = 500), Ok(()));
        assert_eq!(with(|r| r.creator_bps = 501), Err(CreatorBps));
        assert_eq!(with(|r| { r.skim_buy_far_bps = 1_000 }), Ok(()));
        assert_eq!(with(|r| r.skim_buy_far_bps = 1_001), Err(SkimBuyFarBps));
        // the sell far rate's own cap, at a multiple that keeps the at-floor product legal
        assert_eq!(with(|r| { r.skim_sell_far_bps = 1_000; r.skim_sell_at_floor_multiple_bps = 15_000 }), Ok(()));
        assert_eq!(with(|r| { r.skim_sell_far_bps = 1_001; r.skim_sell_at_floor_multiple_bps = 10_000 }), Err(SkimSellFarBps));
        // ⛔ the holder rate: a preset carries none — 1 and Custom's own cap are both refused
        assert_eq!(with(|r| r.holder_bps = 0), Ok(()));
        assert_eq!(with(|r| r.holder_bps = 1), Err(HolderBps));
        assert_eq!(with(|r| r.holder_bps = crate::policy::MAX_HOLDER_BPS), Err(HolderBps));
        assert_eq!(with(|r| r.holder_bps = crate::policy::MAX_HOLDER_BPS + 1), Err(HolderBps));
        // the split: each ≥ 1,000, sum ≤ 9,000
        assert_eq!(with(|r| { r.sale_bps = 1_000; r.pol_bps = 1_000 }), Ok(()));
        assert_eq!(with(|r| { r.sale_bps = 8_000; r.pol_bps = 1_000 }), Ok(()));
        assert_eq!(with(|r| r.sale_bps = 999), Err(SaleBps));
        assert_eq!(with(|r| r.pol_bps = 999), Err(PolBps));
        assert_eq!(with(|r| { r.sale_bps = 8_001; r.pol_bps = 1_000 }), Err(SaleBps), "the sale alone past 80 %, as the EVM names it");
        assert_eq!(with(|r| { r.sale_bps = 4_500; r.pol_bps = 4_501 }), Err(PolBps), "the sum past 90 %");
        // the multiple's window, then the at-floor cap (far × multiple ≤ 1,500)
        assert_eq!(with(|r| r.skim_sell_at_floor_multiple_bps = 10_000), Ok(()));
        assert_eq!(with(|r| r.skim_sell_at_floor_multiple_bps = 9_999), Err(SkimSellAtFloorMultipleBps));
        assert_eq!(with(|r| { r.skim_sell_far_bps = 300; r.skim_sell_at_floor_multiple_bps = 50_000 }), Ok(()));
        assert_eq!(with(|r| { r.skim_sell_far_bps = 0; r.skim_sell_at_floor_multiple_bps = 50_001 }), Err(SkimSellAtFloorMultipleBps));
        assert_eq!(with(|r| { r.skim_sell_far_bps = 301; r.skim_sell_at_floor_multiple_bps = 50_000 }), Err(SkimSellAtFloorMultipleBps));
        // the product is FLOORED, as `register_pool` stores it: 1,000 × 15,009 / 10,000 = 1,500.9 → 1,500 passes, 15,010 does not
        assert_eq!(with(|r| { r.skim_sell_far_bps = 1_000; r.skim_sell_at_floor_multiple_bps = 15_009 }), Ok(()));
        assert_eq!(with(|r| { r.skim_sell_far_bps = 1_000; r.skim_sell_at_floor_multiple_bps = 15_010 }), Err(SkimSellAtFloorMultipleBps));
        // the staircase
        assert_eq!(with(|r| r.stair_steps = 1), Ok(()));
        assert_eq!(with(|r| r.stair_steps = 16), Ok(()));
        assert_eq!(with(|r| r.stair_steps = 0), Err(StairSteps));
        assert_eq!(with(|r| r.stair_steps = 17), Err(StairSteps));
        assert_eq!(with(|r| r.stair_growth_bps = 5_000), Ok(()));
        assert_eq!(with(|r| r.stair_growth_bps = 20_000), Ok(()));
        assert_eq!(with(|r| r.stair_growth_bps = 4_999), Err(StairGrowthBps));
        assert_eq!(with(|r| r.stair_growth_bps = 20_001), Err(StairGrowthBps));
        // the team buy
        assert_eq!(with(|r| r.team_buy_max_bps = 0), Ok(()));
        assert_eq!(with(|r| r.team_buy_max_bps = 1_000), Ok(()));
        assert_eq!(with(|r| r.team_buy_max_bps = 1_001), Err(TeamBuyMaxBps));
        assert_eq!(with(|r| r.team_vest_days = [10, 11, 12, 365]), Ok(()));
        for bad in [[9, 20, 40, 80], [10, 20, 40, 366], [10, 10, 40, 80], [10, 40, 20, 80], [0, 0, 0, 0]] {
            assert_eq!(with(|r| r.team_vest_days = bad), Err(TeamVestDays), "{bad:?}");
        }
        assert_eq!(TeamVestDays.name(), "team_vest_days");
        assert_eq!(GraduationMcapUsd.name(), "graduation_mcap_usd");
    }

    /// The first rule broken in table order is the one named, so a row with two faults names the earlier field.
    #[test]
    fn the_first_broken_rule_in_table_order_is_named() {
        assert_eq!(with(|r| { r.creator_bps = 501; r.stair_steps = 0 }), Err(CreatorBps));
        assert_eq!(with(|r| { r.graduation_mcap_usd = 0; r.team_vest_days = [0; 4] }), Err(GraduationMcapUsd));
    }

    /// ⛔ The seeded rows equal the preset values byte for byte — each field against `preset(mode)` and the
    /// team-buy table — and each passes the validator it never has to pass on chain (`init_preset_registry` writes them).
    #[test]
    fn the_seeded_rows_are_todays_presets() {
        for (id, mode) in [(1u8, Mode::Moonshot), (2, Mode::Cook), (3, Mode::Bluechip)] {
            let r = seed_row(id).unwrap();
            let p = preset(mode);
            assert_eq!(
                (r.graduation_mcap_usd, r.creator_bps, r.skim_buy_far_bps, r.skim_sell_far_bps, r.holder_bps),
                (p.graduation_mcap_usd, p.creator_bps, p.skim_buy_far_bps, p.skim_sell_far_bps, p.holder_bps),
                "{mode:?}"
            );
            assert_eq!((r.sale_bps, r.pol_bps, r.skim_sell_at_floor_multiple_bps), (p.sale_bps, p.pol_bps, p.skim_sell_at_floor_multiple_bps));
            assert_eq!(r.skim_sell_at_floor_multiple_bps as u64, PRESET_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS);
            assert_eq!((r.stair_steps, r.stair_growth_bps), (p.stair_steps, p.stair_growth_bps));
            assert_eq!(r.team_buy_max_bps as u64, crate::owners::team_buy_max_bps(id), "{mode:?}");
            assert_eq!(validate_preset_row(&r), Ok(()), "{mode:?} passes the one rule table");
        }
        // the literal values, so a moved constant is seen here too
        assert_eq!(seed_row(1).unwrap().graduation_mcap_usd, 25_000);
        assert_eq!(seed_row(2).unwrap().graduation_mcap_usd, 50_000);
        assert_eq!(seed_row(3).unwrap().graduation_mcap_usd, 100_000);
        assert_eq!((seed_row(1).unwrap().team_buy_max_bps, seed_row(1).unwrap().team_vest_days), (500, [10, 20, 40, 80]));
        assert_eq!((seed_row(2).unwrap().team_buy_max_bps, seed_row(2).unwrap().team_vest_days), (500, [10, 20, 40, 80]));
        assert_eq!((seed_row(3).unwrap().team_buy_max_bps, seed_row(3).unwrap().team_vest_days), (1_000, [30, 60, 90, 120]));
        for id in [0u8, 4, 31, 255] {
            assert_eq!(seed_row(id), None, "{id} is not a seeded row");
        }
    }
}
