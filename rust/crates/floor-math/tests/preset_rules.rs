//! ⛔ THE PRESET REGISTRY'S RULE TABLE, ADVERSARIALLY — the EVM's `test/PresetRegistryAdversarial.t.sol`
//! `PresetRegistryRulesTest`, ported case for case onto [`floor_math::presets::validate_preset_row`], the ONE validator
//! `floor_launch::add_preset` runs: the SAME preset row validates the SAME way on both chains.
//!
//! The base row is the EVM test's "Monkey" (`_monkey()`), every field unlike every seed; each case moves one or two
//! fields off it. Where the EVM test pins the FIELD a refusal names, this file pins the same field; where it accepts any
//! field (the split's sum, the at-floor product), this file pins the field the EVM's `FloorPresetRegistry._validate`
//! names (`saleBps` for a sale outside 10 %..=80 %, `polBps` otherwise; the multiple for the product), so the two chains
//! agree on the name too.
//!
//! What the EVM table also covers and a pure validator cannot (enabled written 1, count 4 after init, the first add is id 4,
//! `PresetTableFull` past 31 with the count left at 32, a refused add takes no id) is covered by `floor_launch`'s own
//! registry tests, on `PresetRegistry::append`.

use floor_math::presets::{validate_preset_row, PresetField, PresetField::*, PresetRow};

/// The EVM test's `_monkey()`: at-floor = 80 × 3.0 = 240 bps. Holder 0: a preset carries no holder rate.
fn monkey() -> PresetRow {
    PresetRow {
        graduation_mcap_usd: 40_000,
        creator_bps: 120,
        skim_buy_far_bps: 50,
        skim_sell_far_bps: 80,
        holder_bps: 0,
        sale_bps: 2_500,
        pol_bps: 1_500,
        skim_sell_at_floor_multiple_bps: 30_000,
        stair_steps: 12,
        stair_growth_bps: 9_000,
        team_buy_max_bps: 700,
        team_vest_days: [15, 30, 60, 120],
    }
}

fn with(f: impl FnOnce(&mut PresetRow)) -> Result<(), PresetField> {
    let mut r = monkey();
    f(&mut r);
    validate_preset_row(&r)
}

#[test]
fn the_base_row_passes() {
    assert_eq!(validate_preset_row(&monkey()), Ok(()));
}

/// graduation cap: 0 refused; 1 and `u64::MAX` (the EVM's `uint64` ceiling) accepted.
#[test]
fn rule_graduation_mcap_usd() {
    assert_eq!(with(|r| r.graduation_mcap_usd = 0), Err(GraduationMcapUsd));
    assert_eq!(with(|r| r.graduation_mcap_usd = 1), Ok(()));
    assert_eq!(with(|r| r.graduation_mcap_usd = u64::MAX), Ok(()));
}

/// creator ≤ 500.
#[test]
fn rule_creator_bps() {
    assert_eq!(with(|r| r.creator_bps = 0), Ok(()));
    assert_eq!(with(|r| r.creator_bps = 500), Ok(()));
    assert_eq!(with(|r| r.creator_bps = 501), Err(CreatorBps));
    assert_eq!(with(|r| r.creator_bps = u16::MAX), Err(CreatorBps));
}

/// far buy skim ≤ 1,000.
#[test]
fn rule_skim_buy_far_bps() {
    assert_eq!(with(|r| r.skim_buy_far_bps = 1_000), Ok(()));
    assert_eq!(with(|r| r.skim_buy_far_bps = 1_001), Err(SkimBuyFarBps));
}

/// far sell skim ≤ 1,000, the multiple held at 1× so only the field's own bound is broken.
#[test]
fn rule_skim_sell_far_bps() {
    let at_1x = |far: u16| with(|r| { r.skim_sell_at_floor_multiple_bps = 10_000; r.skim_sell_far_bps = far });
    assert_eq!(at_1x(1_000), Ok(()));
    assert_eq!(at_1x(1_001), Err(SkimSellFarBps));
    assert_eq!(at_1x(0), Ok(()));
}

/// holder == 0 (as the EVM registry's `holderBps != 0` refusal): 0 passes, 1 and the old cap
/// (`MAX_HOLDER_BPS`, Custom's) are refused by name.
#[test]
fn rule_holder_bps() {
    assert_eq!(with(|r| r.holder_bps = 0), Ok(()));
    assert_eq!(with(|r| r.holder_bps = 1), Err(HolderBps));
    assert_eq!(with(|r| r.holder_bps = floor_math::policy::MAX_HOLDER_BPS), Err(HolderBps));
    assert_eq!(with(|r| r.holder_bps = 1_001), Err(HolderBps));
    assert_eq!(with(|r| r.holder_bps = u16::MAX), Err(HolderBps));
}

/// the split: each ≥ 1,000, sum ≤ 9,000; an overflowing pair is a refusal by name, never a panic.
#[test]
fn rule_split() {
    let split = |s: u16, p: u16| with(|r| { r.sale_bps = s; r.pol_bps = p });
    assert_eq!(split(1_000, 1_000), Ok(()));
    assert_eq!(split(8_000, 1_000), Ok(()));
    assert_eq!(split(1_000, 8_000), Ok(()));
    assert_eq!(split(999, 1_000), Err(SaleBps));
    assert_eq!(split(1_000, 999), Err(PolBps));
    assert_eq!(split(8_001, 1_000), Err(SaleBps));
    assert_eq!(split(4_500, 4_501), Err(PolBps));
    assert_eq!(split(0, 0), Err(SaleBps));
    assert_eq!(split(u16::MAX, 1_000), Err(SaleBps));
    assert_eq!(split(1_000, u16::MAX), Err(PolBps));
}

/// the multiple's window 10,000..=50,000 (far sell 300, so 5× lands exactly on the at-floor cap); 0 is not a row value.
#[test]
fn rule_skim_sell_at_floor_multiple_range() {
    let m = |x: u32| with(|r| { r.skim_sell_far_bps = 300; r.skim_sell_at_floor_multiple_bps = x });
    assert_eq!(m(10_000), Ok(()));
    assert_eq!(m(50_000), Ok(()));
    assert_eq!(m(9_999), Err(SkimSellAtFloorMultipleBps));
    assert_eq!(m(50_001), Err(SkimSellAtFloorMultipleBps));
    assert_eq!(m(0), Err(SkimSellAtFloorMultipleBps));
}

/// the at-floor product: far sell × multiple / 10,000 ≤ 1,500, floored (as `register_pool` stores it).
#[test]
fn rule_at_floor_product() {
    let p = |far: u16, x: u32| with(|r| { r.skim_sell_far_bps = far; r.skim_sell_at_floor_multiple_bps = x });
    assert_eq!(p(300, 50_000), Ok(()), "3 % at 5× = 15 %");
    assert_eq!(p(1_000, 15_000), Ok(()), "10 % at 1.5× = 15 %");
    assert_eq!(p(0, 50_000), Ok(()), "0 % at 5×");
    assert_eq!(p(301, 50_000), Err(SkimSellAtFloorMultipleBps), "1,505");
    assert_eq!(p(1_000, 15_010), Err(SkimSellAtFloorMultipleBps), "1,501");
    assert_eq!(p(1_000, 50_000), Err(SkimSellAtFloorMultipleBps), "5,000");
    assert_eq!(p(1_000, u32::MAX), Err(SkimSellAtFloorMultipleBps));
}

/// steps 1..=16.
#[test]
fn rule_stair_steps() {
    assert_eq!(with(|r| r.stair_steps = 1), Ok(()));
    assert_eq!(with(|r| r.stair_steps = 16), Ok(()));
    assert_eq!(with(|r| r.stair_steps = 0), Err(StairSteps));
    assert_eq!(with(|r| r.stair_steps = 17), Err(StairSteps));
}

/// growth 5,000..=20,000.
#[test]
fn rule_stair_growth_bps() {
    assert_eq!(with(|r| r.stair_growth_bps = 5_000), Ok(()));
    assert_eq!(with(|r| r.stair_growth_bps = 20_000), Ok(()));
    assert_eq!(with(|r| r.stair_growth_bps = 4_999), Err(StairGrowthBps));
    assert_eq!(with(|r| r.stair_growth_bps = 20_001), Err(StairGrowthBps));
    assert_eq!(with(|r| r.stair_growth_bps = 0), Err(StairGrowthBps));
}

/// team buy ≤ 1,000; 0 (no team buy offered) is inside the rule.
#[test]
fn rule_team_buy_max_bps() {
    assert_eq!(with(|r| r.team_buy_max_bps = 0), Ok(()));
    assert_eq!(with(|r| r.team_buy_max_bps = 1_000), Ok(()));
    assert_eq!(with(|r| r.team_buy_max_bps = 1_001), Err(TeamBuyMaxBps));
}

/// the four cliffs: strictly increasing, each 10..=365.
#[test]
fn rule_team_vest_days() {
    for ok in [[10, 11, 12, 13], [362, 363, 364, 365], [10, 20, 40, 365]] {
        assert_eq!(with(|r| r.team_vest_days = ok), Ok(()), "{ok:?}");
    }
    for bad in [
        [9, 20, 40, 80],     // the first under 10
        [10, 20, 40, 366],   // the last over 365
        [10, 10, 40, 80],    // equal neighbours
        [10, 20, 20, 80],    // equal neighbours
        [10, 20, 40, 40],    // equal neighbours
        [80, 40, 20, 10],    // decreasing
        [0, 0, 0, 0],        // all zero (Custom's "no cliffs" is not a row value)
        [10, 20, 40, 0],     // a missing last cliff
        [366, 367, 368, 369], // increasing, all above the cap
    ] {
        assert_eq!(with(|r| r.team_vest_days = bad), Err(TeamVestDays), "{bad:?}");
    }
}

/// The names the refusals are logged by (`PresetInvalid: <field>`), the Solana wire's spelling.
#[test]
fn every_field_has_its_wire_name() {
    let all = [
        (GraduationMcapUsd, "graduation_mcap_usd"),
        (CreatorBps, "creator_bps"),
        (SkimBuyFarBps, "skim_buy_far_bps"),
        (SkimSellFarBps, "skim_sell_far_bps"),
        (HolderBps, "holder_bps"),
        (SaleBps, "sale_bps"),
        (PolBps, "pol_bps"),
        (SkimSellAtFloorMultipleBps, "skim_sell_at_floor_multiple_bps"),
        (StairSteps, "stair_steps"),
        (StairGrowthBps, "stair_growth_bps"),
        (TeamBuyMaxBps, "team_buy_max_bps"),
        (TeamVestDays, "team_vest_days"),
    ];
    for (f, name) in all {
        assert_eq!(f.name(), name);
    }
}
