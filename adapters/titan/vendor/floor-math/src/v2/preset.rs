// SPDX-License-Identifier: Apache-2.0
use super::ask::{build, Kind};
use super::book::{KIND_BONDING, KIND_CLIMB, KIND_RETIRED_3, KIND_SALE_BAND, KIND_TAIL, KIND_UNUSED, RANGES_MAX};
use super::fees::DEFAULT_FEE_PPM;
use super::rows::{MilestoneClass, Row};
use super::split::{SplitRow, MS_HOLDER, MS_TEAM};
use super::V2Error;
use crate::u256::U256;
use crate::MathError;

pub const ROW_GLOBALS: usize = 112;
pub const RANGE_ROW: usize = 48;
pub const ROW_LEN: usize = 2_416;
const _: () = assert!(ROW_LEN == ROW_GLOBALS + RANGES_MAX * RANGE_ROW && ROW_GLOBALS % 16 == 0 && RANGE_ROW % 16 == 0);
pub const ROW_VERSION: u8 = 2;
pub const BANDS_MAX: usize = 4;
const _: () = assert!(BANDS_MAX == super::rows::MAX_BANDS && BANDS_MAX <= super::book::BIDS_MAX);

pub mod off {
    pub const VERSION: usize = 0;
    pub const MS_CLASS: usize = 1;
    pub const N_RANGES: usize = 2;
    pub const N_BANDS: usize = 3;
    pub const FLAGS: usize = 4;
    pub const TEAM_BUY_RULE: usize = 5;
    pub const PHI: usize = 6;
    pub const WC: usize = 8;
    pub const PSI_TAIL: usize = 10;
    pub const TEAM_BUY_SHARE: usize = 12;
    pub const TEAM_SLOT_MAX: usize = 14;
    pub const PLATFORM_MS: usize = 16;
    pub const FLOOR_MS: usize = 18;
    pub const FEE_PPM: usize = 20;
    pub const BANDS: usize = 40;
    pub const RAISE_USD_CENTS: usize = 56;
    pub const LAUNCH_CAP_USD_CENTS: usize = 64;
    pub const PAD_CK: usize = 72;
    pub const CK_TARGET_BPS: usize = 74;
    pub const FEE_START: usize = 76;
    pub const FEE_A: usize = 96;
    pub const FEE_B: usize = 104;
    const _: () = assert!(FEE_B + 8 == super::ROW_GLOBALS);
}
pub const FEE_REL_ONE: u64 = 1 << 32;
pub const REL_ONE: u128 = 1 << 64;
pub const SHARE_PPB_TOTAL: u32 = 1_000_000_000;
pub const FLAG_PLATFORM_MS: u8 = 1;
pub const FLAG_FLOOR_MS: u8 = 2;
pub const FLAG_UNUSED_OTHER: u8 = 4;
pub const FLAGS_KNOWN: u8 = 0x07;
pub const PHI_MAX_BPS: u16 = 9_900;
pub const FEE_LEG_MAX_PPM: u32 = 10_000;
pub const FEE_SUM_MAX_PPM: u32 = 20_000;
pub const BAND_SHARE_DEFAULT_BPS: u16 = 5_000;
pub const MS_CAP_OFF: usize = 42;
pub const RANGE_PAD_OFF: usize = 46;
pub const MS_CAP_PPM_MAX: u32 = 1_000_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BandRow {
    pub width_bps: u16,
    pub share_bps: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RangeRow {
    pub sqrt_a_rel: u128,
    pub sqrt_b_rel: u128,
    pub share_ppb: u32,
    pub theta_bps: u16,
    pub psi_bps: u16,
    pub kind: u8,
    pub class: u8,
    pub ms_cap_ppm: u32,
}

impl RangeRow {
    pub fn is_flat(&self) -> bool {
        self.sqrt_a_rel == self.sqrt_b_rel
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowV2 {
    pub ms_class: u8,
    pub n_ranges: u8,
    pub n_bands: u8,
    pub flags: u8,
    pub team_buy_rule: u8,
    pub phi_bps: u16,
    pub wc_bps: u16,
    pub psi_tail_bps: u16,
    pub team_buy_share_bps: u16,
    pub team_slot_max_bps: u16,
    pub platform_ms_bps: u16,
    pub floor_ms_bps: u16,
    pub fee_ppm: [u32; 5],
    pub bands: [BandRow; BANDS_MAX],
    pub raise_usd_cents: u64,
    pub launch_cap_usd_cents: u64,
    pub ck_target_bps: u16,
    pub fee_start_ppm: [u32; 5],
    pub fee_a_rel: u64,
    pub fee_b_rel: u64,
    pub ranges: [RangeRow; RANGES_MAX],
}

impl RowV2 {
    pub const EMPTY: RowV2 = RowV2 {
        ms_class: 0,
        n_ranges: 0,
        n_bands: 0,
        flags: 0,
        team_buy_rule: 0,
        phi_bps: 0,
        wc_bps: 0,
        psi_tail_bps: 0,
        team_buy_share_bps: 0,
        team_slot_max_bps: 0,
        platform_ms_bps: 0,
        floor_ms_bps: 0,
        fee_ppm: [0; 5],
        bands: [BandRow { width_bps: 0, share_bps: 0 }; BANDS_MAX],
        raise_usd_cents: 0,
        launch_cap_usd_cents: 0,
        ck_target_bps: 0,
        fee_start_ppm: [0; 5],
        fee_a_rel: 0,
        fee_b_rel: 0,
        ranges: [RangeRow { sqrt_a_rel: 0, sqrt_b_rel: 0, share_ppb: 0, theta_bps: 0, psi_bps: 0, kind: 0, class: 0, ms_cap_ppm: 0 }; RANGES_MAX],
    };

    pub fn ranges(&self) -> &[RangeRow] {
        &self.ranges[..(self.n_ranges as usize).min(RANGES_MAX)]
    }

    pub fn split_row(&self) -> SplitRow {
        let mut s = SplitRow::new(self.phi_bps, self.ck_target_bps, self.ms_class);
        s.platform_ms_bps = if self.flags & FLAG_PLATFORM_MS != 0 { self.platform_ms_bps } else { 0 };
        s.floor_ms_bps = if self.flags & FLAG_FLOOR_MS != 0 { self.floor_ms_bps } else { 0 };
        s
    }
}

fn put(b: &mut [u8], off: usize, v: u128, n: usize) {
    b[off..off + n].copy_from_slice(&v.to_le_bytes()[..n]);
}

fn get(b: &[u8], off: usize, n: usize) -> u128 {
    let mut x = [0u8; 16];
    x[..n].copy_from_slice(&b[off..off + n]);
    u128::from_le_bytes(x)
}

pub fn row_to_bytes(r: &RowV2) -> [u8; ROW_LEN] {
    let mut b = [0u8; ROW_LEN];
    row_write(r, &mut b);
    b
}

pub fn row_write(r: &RowV2, b: &mut [u8; ROW_LEN]) {
    b.fill(0);
    let nb = (r.n_bands as usize).min(BANDS_MAX);
    let n = (r.n_ranges as usize).min(RANGES_MAX);
    for (off, v, w) in [
        (off::VERSION, ROW_VERSION as u128, 1),
        (off::MS_CLASS, r.ms_class as u128, 1),
        (off::N_RANGES, n as u128, 1),
        (off::N_BANDS, nb as u128, 1),
        (off::FLAGS, r.flags as u128, 1),
        (off::TEAM_BUY_RULE, r.team_buy_rule as u128, 1),
        (off::PHI, r.phi_bps as u128, 2),
        (off::WC, r.wc_bps as u128, 2),
        (off::PSI_TAIL, r.psi_tail_bps as u128, 2),
        (off::TEAM_BUY_SHARE, r.team_buy_share_bps as u128, 2),
        (off::TEAM_SLOT_MAX, r.team_slot_max_bps as u128, 2),
        (off::PLATFORM_MS, r.platform_ms_bps as u128, 2),
        (off::FLOOR_MS, r.floor_ms_bps as u128, 2),
        (off::RAISE_USD_CENTS, r.raise_usd_cents as u128, 8),
        (off::LAUNCH_CAP_USD_CENTS, r.launch_cap_usd_cents as u128, 8),
        (off::CK_TARGET_BPS, r.ck_target_bps as u128, 2),
        (off::FEE_A, r.fee_a_rel as u128, 8),
        (off::FEE_B, r.fee_b_rel as u128, 8),
    ] {
        put(b, off, v, w);
    }
    for i in 0..5 {
        put(b, off::FEE_PPM + 4 * i, r.fee_ppm[i] as u128, 4);
        put(b, off::FEE_START + 4 * i, r.fee_start_ppm[i] as u128, 4);
    }
    for i in 0..nb {
        put(b, off::BANDS + 4 * i, r.bands[i].width_bps as u128, 2);
        put(b, off::BANDS + 2 + 4 * i, r.bands[i].share_bps as u128, 2);
    }
    for (i, g) in r.ranges[..n].iter().enumerate() {
        let o = ROW_GLOBALS + i * RANGE_ROW;
        put(b, o, g.sqrt_a_rel, 16);
        put(b, o + 16, g.sqrt_b_rel, 16);
        put(b, o + 32, g.share_ppb as u128, 4);
        put(b, o + 36, g.theta_bps as u128, 2);
        put(b, o + 38, g.psi_bps as u128, 2);
        put(b, o + 40, g.kind as u128, 1);
        put(b, o + 41, g.class as u128, 1);
        put(b, o + 42, g.ms_cap_ppm as u128, 4);
    }
}

pub fn range_at(b: &[u8; ROW_LEN], i: usize) -> RangeRow {
    let o = ROW_GLOBALS + i * RANGE_ROW;
    RangeRow {
        sqrt_a_rel: get(b, o, 16),
        sqrt_b_rel: get(b, o + 16, 16),
        share_ppb: get(b, o + 32, 4) as u32,
        theta_bps: get(b, o + 36, 2) as u16,
        psi_bps: get(b, o + 38, 2) as u16,
        kind: b[o + 40],
        class: b[o + 41],
        ms_cap_ppm: get(b, o + 42, 4) as u32,
    }
}

pub fn row_from_bytes(b: &[u8; ROW_LEN]) -> Result<RowV2, MathError> {
    let mut r = RowV2::EMPTY;
    row_read(b, &mut r)?;
    Ok(r)
}

pub fn row_read(b: &[u8; ROW_LEN], r: &mut RowV2) -> Result<(), MathError> {
    if b[off::VERSION] != ROW_VERSION {
        return Err(MathError::RowVersion);
    }
    let (n, nb) = (b[off::N_RANGES] as usize, b[off::N_BANDS] as usize);
    if n > RANGES_MAX {
        return Err(MathError::RowTooManyRanges);
    }
    if nb > BANDS_MAX {
        return Err(MathError::RowBands);
    }
    if b[off::PAD_CK..off::PAD_CK + 2].iter().any(|x| *x != 0) || b[off::BANDS + 4 * nb..off::BANDS + 4 * BANDS_MAX].iter().any(|x| *x != 0) {
        return Err(MathError::RowNonCanonical);
    }
    for i in 0..RANGES_MAX {
        let o = ROW_GLOBALS + i * RANGE_ROW;
        let tail = if i < n { &b[o + RANGE_PAD_OFF..o + RANGE_ROW] } else { &b[o..o + RANGE_ROW] };
        if tail.iter().any(|x| *x != 0) {
            return Err(MathError::RowNonCanonical);
        }
    }
    *r = RowV2 {
        ms_class: b[off::MS_CLASS],
        n_ranges: n as u8,
        n_bands: nb as u8,
        flags: b[off::FLAGS],
        team_buy_rule: b[off::TEAM_BUY_RULE],
        phi_bps: get(b, off::PHI, 2) as u16,
        wc_bps: get(b, off::WC, 2) as u16,
        psi_tail_bps: get(b, off::PSI_TAIL, 2) as u16,
        team_buy_share_bps: get(b, off::TEAM_BUY_SHARE, 2) as u16,
        team_slot_max_bps: get(b, off::TEAM_SLOT_MAX, 2) as u16,
        platform_ms_bps: get(b, off::PLATFORM_MS, 2) as u16,
        floor_ms_bps: get(b, off::FLOOR_MS, 2) as u16,
        fee_ppm: [0; 5],
        bands: [BandRow::default(); BANDS_MAX],
        raise_usd_cents: get(b, off::RAISE_USD_CENTS, 8) as u64,
        launch_cap_usd_cents: get(b, off::LAUNCH_CAP_USD_CENTS, 8) as u64,
        ck_target_bps: get(b, off::CK_TARGET_BPS, 2) as u16,
        fee_start_ppm: [0; 5],
        fee_a_rel: get(b, off::FEE_A, 8) as u64,
        fee_b_rel: get(b, off::FEE_B, 8) as u64,
        ranges: r.ranges,
    };
    for i in 0..5 {
        r.fee_ppm[i] = get(b, off::FEE_PPM + 4 * i, 4) as u32;
        r.fee_start_ppm[i] = get(b, off::FEE_START + 4 * i, 4) as u32;
    }
    for i in 0..nb {
        r.bands[i] = BandRow { width_bps: get(b, off::BANDS + 4 * i, 2) as u16, share_bps: get(b, off::BANDS + 2 + 4 * i, 2) as u16 };
    }
    for i in 0..RANGES_MAX {
        r.ranges[i] = if i < n { range_at(b, i) } else { RangeRow::default() };
    }
    Ok(())
}

pub fn row_hash_input(b: &[u8; ROW_LEN]) -> &[u8] {
    &b[..]
}

pub fn spec_ok(r: &RowV2) -> Result<(), MathError> {
    structure(r)?;
    bounds(r)?;
    economics(r)
}

pub fn spec_ok_genesis(r: &RowV2) -> Result<(), MathError> {
    spec_ok(r)?;
    if r.team_buy_share_bps != 0 {
        return Err(MathError::RowTeam);
    }
    Ok(())
}

fn structure(r: &RowV2) -> Result<(), MathError> {
    let n = r.n_ranges as usize;
    if n > RANGES_MAX {
        return Err(MathError::RowTooManyRanges);
    }
    if n < 2 {
        return Err(MathError::RowTail);
    }
    if r.n_bands == 0 || r.n_bands as usize > BANDS_MAX {
        return Err(MathError::RowBands);
    }
    let g = r.ranges();
    let mut shares = 0u64;
    for (i, x) in g.iter().enumerate() {
        let k = x.kind;
        if k > KIND_TAIL
            || k == KIND_RETIRED_3
            || (k == KIND_BONDING) != (i == 0)
            || (k == KIND_TAIL) != (i + 1 == n)
            || (k == KIND_UNUSED && i + 2 != n)
        {
            return Err(MathError::RowEdges);
        }
        if x.sqrt_a_rel == 0 || (i > 0 && x.sqrt_a_rel != g[i - 1].sqrt_b_rel) {
            return Err(MathError::RowEdges);
        }
        if k == KIND_TAIL {
            if x.sqrt_b_rel != 0 {
                return Err(MathError::RowTail);
            }
        } else if x.sqrt_b_rel < x.sqrt_a_rel || (x.is_flat() && k == KIND_CLIMB) {
            return Err(MathError::RowEdges);
        }
        if x.share_ppb == 0 && k != KIND_TAIL {
            return Err(MathError::RowShares);
        }
        shares += x.share_ppb as u64;
    }
    if g[0].sqrt_a_rel != REL_ONE || !g[0].is_flat() {
        return Err(MathError::RowEdges);
    }
    if shares != SHARE_PPB_TOTAL as u64 {
        return Err(MathError::RowShares);
    }
    Ok(())
}

fn bounds(r: &RowV2) -> Result<(), MathError> {
    let nb = r.n_bands as usize;
    let wc: u32 = r.bands[..nb].iter().map(|b| b.width_bps as u32).sum();
    let share: u32 = r.bands[..nb].iter().map(|b| b.share_bps as u32).sum();
    if r.bands[..nb].iter().any(|b| b.width_bps == 0) || wc != r.wc_bps as u32 || wc >= 10_000 || share > 10_000 {
        return Err(MathError::RowBands);
    }
    if r.phi_bps == 0
        || r.phi_bps > PHI_MAX_BPS
        || r.psi_tail_bps == 0
        || r.psi_tail_bps > 10_000
        || r.platform_ms_bps as u32 + r.floor_ms_bps as u32 > 10_000
        || r.raise_usd_cents == 0
        || r.launch_cap_usd_cents == 0
        || r.ranges().iter().any(|x| x.theta_bps > 10_000 || x.psi_bps == 0 || x.psi_bps > 10_000 || x.class > MS_HOLDER || x.ms_cap_ppm > MS_CAP_PPM_MAX)
    {
        return Err(MathError::RowBounds);
    }
    let legs_bad = |f: &[u32; 5]| f.iter().any(|x| *x > FEE_LEG_MAX_PPM) || f.iter().sum::<u32>() > FEE_SUM_MAX_PPM;
    let no_curve = r.fee_a_rel == 0 && r.fee_b_rel == 0 && r.fee_start_ppm == [0; 5];
    if legs_bad(&r.fee_ppm) || (!no_curve && (legs_bad(&r.fee_start_ppm) || r.fee_a_rel == 0 || r.fee_b_rel <= r.fee_a_rel)) {
        return Err(MathError::RowFees);
    }
    if r.team_buy_rule != 0 || r.team_buy_share_bps > 10_000 || r.team_slot_max_bps > 10_000 {
        return Err(MathError::RowTeam);
    }
    if r.flags & !FLAGS_KNOWN != 0 {
        return Err(MathError::RowFlags);
    }
    if r.ms_class != MS_TEAM && r.ms_class != MS_HOLDER {
        return Err(MathError::RowMsClass);
    }
    Ok(())
}

fn ov<T>(o: Option<T>) -> Result<T, MathError> {
    o.ok_or(MathError::Overflow)
}

fn economics(r: &RowV2) -> Result<(), MathError> {
    let phi = r.phi_bps as u64;
    let g = r.ranges();
    let money = |x: &RangeRow| ov(U256::mul_u128(x.sqrt_a_rel, x.sqrt_b_rel).checked_mul_u64(x.share_ppb as u64));
    let mut f10 = ov(money(&g[0])?.checked_mul_u64(10_000))?;
    let mut s = g[0].share_ppb as u64;
    for x in &g[1..g.len() - 1] {
        let (ra, rb) = (x.sqrt_a_rel, x.sqrt_b_rel);
        if rb > ra && x.kind != KIND_UNUSED {
            let lo = ov(ra.checked_mul(10_000))?;
            let hi = ov(rb.checked_mul(phi as u128))?;
            if lo <= hi {
                return Err(MathError::RangeCover);
            }
            let have = ov(U256::mul_u128(rb, lo - hi).checked_mul_u64(x.share_ppb as u64))?;
            let need = ov(ov(U256::sqr_u128(rb).checked_sub(U256::sqr_u128(ra)))?.checked_mul_u64(phi))?;
            let need = ov(need.checked_mul_u64(s))?;
            if have < need {
                return Err(MathError::RangeCover);
            }
        }
        s += x.share_ppb as u64;
        let need10 = ov(ov(U256::sqr_u128(rb).checked_mul_u64(phi))?.checked_mul_u64(s))?;
        let all = ov(f10.checked_add(ov(money(x)?.checked_mul_u64(10_000))?))?;
        f10 = all.min(f10.max(need10));
        if f10 < need10 {
            return Err(MathError::RowNotFeasible);
        }
    }
    Ok(())
}

fn q64(x: f64) -> u128 {
    (x * 18_446_744_073_709_551_616.0) as u128
}

pub fn from_design(r: &Row) -> Result<RowV2, V2Error> {
    let ask = build(r)?;
    let cap = r.launch_cap as f64;
    let class = match r.class {
        MilestoneClass::Team => MS_TEAM,
        MilestoneClass::Holder => MS_HOLDER,
    };
    let mut out = RowV2::EMPTY;
    let mut n = 0usize;
    let mut used = 0u64;
    for s in &ask.segs {
        let (kind, theta_default, psi) = match s.kind {
            Kind::Bonding => (KIND_BONDING, 0, 10_000),
            Kind::Climb => (KIND_CLIMB, 10_000, 10_000),
            Kind::Sale => (KIND_SALE_BAND, r.theta_bps, 10_000),
            Kind::Unused => (KIND_UNUSED, 0, 10_000),
            Kind::Tail => (KIND_TAIL, 10_000, r.psi_t_bps),
        };
        let a_rel = q64((s.a / cap).sqrt());
        let b_rel = if kind == KIND_TAIL { 0 } else { q64((s.b / cap).sqrt()) };
        let ppb = if kind == KIND_TAIL { 0 } else { (s.tokens * 1e9).floor().max(0.0) as u64 };
        if ppb == 0 && a_rel == b_rel && kind != KIND_TAIL {
            continue;
        }
        if n == RANGES_MAX {
            return Err(V2Error::RowInvalid);
        }
        let rest = s.to_ck + s.to_ms;
        let theta = if rest > 0.0 { ((s.to_ck / rest) * 1e4).round() as u16 } else { theta_default };
        let ms_cap_ppm = if kind == KIND_SALE_BAND && s.to_ms > 0.0 && s.money > 0.0 {
            ((s.to_ms / s.money * 1e6).floor() as u32).clamp(1, MS_CAP_PPM_MAX)
        } else {
            0
        };
        out.ranges[n] = RangeRow { sqrt_a_rel: a_rel, sqrt_b_rel: b_rel, share_ppb: ppb as u32, theta_bps: theta, psi_bps: psi, kind, class, ms_cap_ppm };
        used += ppb;
        n += 1;
    }
    if n == 0 || out.ranges[n - 1].kind != KIND_TAIL || used > SHARE_PPB_TOTAL as u64 {
        return Err(V2Error::SupplyShort);
    }
    out.ranges[n - 1].share_ppb = SHARE_PPB_TOTAL - used as u32;
    let nb = r.bands.len();
    if nb == 0 || nb > BANDS_MAX {
        return Err(V2Error::RowInvalid);
    }
    for (i, w) in r.bands.iter().enumerate() {
        out.bands[i] = BandRow { width_bps: *w, share_bps: BAND_SHARE_DEFAULT_BPS / nb as u16 };
    }
    let cents = |x: u128| u64::try_from(x.checked_mul(100).ok_or(V2Error::Overflow)?).map_err(|_| V2Error::Overflow);
    out.ms_class = class;
    out.n_ranges = n as u8;
    out.n_bands = nb as u8;
    out.flags = (r.platform_ms_bps > 0) as u8 * FLAG_PLATFORM_MS
        | (r.floor_ms_bps > 0) as u8 * FLAG_FLOOR_MS;
    out.phi_bps = r.phi_bps;
    out.wc_bps = r.wc_bps() as u16;
    out.psi_tail_bps = r.psi_t_bps;
    out.team_buy_share_bps = r.team_buy_bps;
    out.team_slot_max_bps = r.team_slot_max_bps;
    out.platform_ms_bps = r.platform_ms_bps;
    out.floor_ms_bps = r.floor_ms_bps;
    out.fee_ppm = DEFAULT_FEE_PPM;
    if let Some(c) = r.fee_curve {
        let from_usd = if c.from_usd == super::rows::FEE_FROM_GRAD { r.launch_cap } else { c.from_usd };
        let to_usd = if c.to_usd == super::rows::FEE_TO_TAIL { ask.tail_start.round() as u128 } else { c.to_usd };
        let rel = |usd: u128| {
            let x = (usd as f64 / r.launch_cap as f64).sqrt() * FEE_REL_ONE as f64;
            if x.is_finite() && x >= 1.0 && x < u64::MAX as f64 { Ok(x as u64) } else { Err(V2Error::RowInvalid) }
        };
        out.fee_ppm = c.end_ppm;
        out.fee_start_ppm = c.start_ppm;
        out.fee_a_rel = rel(from_usd)?;
        out.fee_b_rel = rel(to_usd)?;
    }
    out.raise_usd_cents = cents(r.bonding_raise)?;
    out.launch_cap_usd_cents = cents(r.launch_cap)?;
    out.ck_target_bps = r.ck_bps;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2::rows;

    fn spec_rows() -> Vec<Row> {
        vec![rows::wagmi(), rows::moonshot(), rows::floor_genesis(), rows::wagmi_genesis(10_000), rows::wagmi_genesis(1_000_000), rows::wagmi_genesis(100_000_000), rows::hodl()]
    }

    #[test]
    fn the_spec_rows_encode_round_trip_and_pass_spec_ok() {
        for d in spec_rows() {
            let r = from_design(&d).unwrap();
            let b = row_to_bytes(&r);
            assert_eq!(b.len(), ROW_LEN);
            assert_eq!(row_from_bytes(&b).unwrap(), r, "{}", d.name);
            assert_eq!(row_to_bytes(&row_from_bytes(&b).unwrap()), b);
            assert_eq!((b[off::PAD_CK], b[off::PAD_CK + 1], get(&b, off::CK_TARGET_BPS, 2) as u16), (0, 0, d.ck_bps), "{}: the C + K target bytes", d.name);
            let ok = if matches!(d.id, 4 | 5) { spec_ok_genesis(&r) } else { spec_ok(&r) };
            assert_eq!(ok, Ok(()), "{}", d.name);
        }
    }

    #[test]
    fn non_canonical_bytes_are_refused() {
        let b = row_to_bytes(&from_design(&rows::wagmi()).unwrap());
        for (o, want) in [(off::VERSION, MathError::RowVersion), (off::PAD_CK, MathError::RowNonCanonical), (off::PAD_CK + 1, MathError::RowNonCanonical), (off::BANDS + 4 * BANDS_MAX - 1, MathError::RowNonCanonical), (ROW_GLOBALS + RANGE_PAD_OFF, MathError::RowNonCanonical), (ROW_GLOBALS + RANGE_ROW - 1, MathError::RowNonCanonical), (ROW_LEN - 1, MathError::RowNonCanonical)] {
            let mut c = b;
            c[o] ^= 1;
            assert_eq!(row_from_bytes(&c), Err(want), "byte {o}");
        }
        let mut c = b;
        c[off::N_RANGES] = 49;
        assert_eq!(row_from_bytes(&c), Err(MathError::RowTooManyRanges));
    }

    #[test]
    fn spec_ok_refuses_each_bound() {
        let good = from_design(&rows::moonshot()).unwrap();
        let n = good.n_ranges as usize;
        let cases: Vec<(fn(&mut RowV2), MathError)> = vec![
            (|r| r.phi_bps = 9_901, MathError::RowBounds),
            (|r| r.fee_ppm[0] = 10_001, MathError::RowFees),
            (|r| r.fee_start_ppm[0] = 10_001, MathError::RowFees),
            (|r| r.fee_b_rel = r.fee_a_rel, MathError::RowFees),
            (|r| r.fee_a_rel = 0, MathError::RowFees),
            (|r| r.team_buy_rule = 1, MathError::RowTeam),
            (|r| r.flags = 0x10, MathError::RowFlags),
            (|r| r.ms_class = 3, MathError::RowMsClass),
            (|r| r.wc_bps += 1, MathError::RowBands),
            (|r| r.n_bands = 0, MathError::RowBands),
            (|r| r.ranges[1].share_ppb += 1, MathError::RowShares),
            (|r| r.ranges[2].sqrt_a_rel += 1, MathError::RowEdges),
            (|r| r.ranges[0].kind = KIND_CLIMB, MathError::RowEdges),
            (|r| r.ranges[1].ms_cap_ppm = MS_CAP_PPM_MAX + 1, MathError::RowBounds),
        ];
        for (i, (f, want)) in cases.into_iter().enumerate() {
            let mut r = good;
            f(&mut r);
            assert_eq!(spec_ok(&r), Err(want), "case {i}");
        }
        let mut r = good;
        r.ranges[n - 1].sqrt_b_rel = 1;
        assert_eq!(spec_ok(&r), Err(MathError::RowTail));
        assert_eq!(spec_ok_genesis(&good), Err(MathError::RowTeam));
    }

    #[test]
    fn spec_ok_refuses_a_falling_floor_share() {
        let mut r = from_design(&rows::wagmi()).unwrap();
        let i = r.ranges().iter().position(|x| x.kind == KIND_CLIMB).unwrap();
        let half = r.ranges[i].share_ppb / 2;
        r.ranges[i].share_ppb -= half;
        let n = r.n_ranges as usize;
        r.ranges[n - 1].share_ppb += half;
        assert_eq!(spec_ok(&r), Err(MathError::RangeCover));
        let mut r = from_design(&rows::moonshot()).unwrap();
        r.phi_bps = 5_000;
        assert!(matches!(spec_ok(&r), Err(MathError::RangeCover | MathError::RowNotFeasible)));
        let mut r = from_design(&rows::wagmi()).unwrap();
        let g = |kind, a, b, ppb| RangeRow { sqrt_a_rel: a, sqrt_b_rel: b, share_ppb: ppb, theta_bps: 0, psi_bps: 10_000, kind, class: MS_HOLDER, ms_cap_ppm: 0 };
        r.n_ranges = 3;
        r.ranges[0] = g(KIND_BONDING, REL_ONE, REL_ONE, 1_000_000);
        r.ranges[1] = g(KIND_UNUSED, REL_ONE, 2 * REL_ONE, 10_000_000);
        r.ranges[2] = g(KIND_TAIL, 2 * REL_ONE, 0, SHARE_PPB_TOTAL - 11_000_000);
        r.phi_bps = 2_000;
        assert_eq!(spec_ok(&r), Ok(()), "money x·2·p0 + raise ≥ 0.2·4·p0·s");
        r.phi_bps = 9_900;
        assert_eq!(spec_ok(&r), Err(MathError::RowNotFeasible), "the floor share at 4 × p0 is not funded");
    }
}
