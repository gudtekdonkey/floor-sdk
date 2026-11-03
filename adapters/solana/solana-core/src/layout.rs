// SPDX-License-Identifier: Apache-2.0
use floor_math::v2::book::{self, Book, Range};
use floor_math::v2::fees::FEE_LEGS;
use floor_math::v2::preset::{FLAG_FLOOR_MS, FLAG_PLATFORM_MS};
use floor_math::v2::split::SplitRow;
use floor_math::v2::supply::Supply;
use solana_pubkey::Pubkey;

pub const LAUNCH_V2_DISCRIMINATOR: [u8; 8] = [205, 11, 170, 172, 112, 31, 209, 221];
pub const LAUNCH_V2_SIZE: usize = 6_016;
pub const LAUNCH_V2_ACCOUNT_LEN: usize = 8 + LAUNCH_V2_SIZE;
pub const VERSION_V2: u8 = 2;
pub const STATUS_LIVE_V2: u8 = 0;
pub const RANGES_MAX: usize = 48;
pub const BIDS_MAX: usize = 24;
pub const ROW_BANDS: usize = 4;
pub const SLOT_MEMBERS: usize = 10;
const _: () = assert!(RANGES_MAX == book::RANGES_MAX && BIDS_MAX == book::BIDS_MAX && ROW_BANDS == floor_math::v2::rows::MAX_BANDS && FEE_LEGS == 5);

pub trait Le: Sized {
    const SIZE: usize;
    fn get(b: &[u8]) -> Self;
    fn put(&self, b: &mut [u8]);
}

macro_rules! le_int {
    ($($t:ty),*) => {$(
        impl Le for $t {
            const SIZE: usize = core::mem::size_of::<$t>();
            fn get(b: &[u8]) -> Self {
                <$t>::from_le_bytes(b[..Self::SIZE].try_into().unwrap())
            }
            fn put(&self, b: &mut [u8]) {
                b[..Self::SIZE].copy_from_slice(&self.to_le_bytes());
            }
        }
    )*};
}
le_int!(u8, u16, u32, u64, i64, u128);

impl Le for Pubkey {
    const SIZE: usize = 32;
    fn get(b: &[u8]) -> Self {
        Pubkey::new_from_array(b[..32].try_into().unwrap())
    }
    fn put(&self, b: &mut [u8]) {
        b[..32].copy_from_slice(self.as_ref());
    }
}

impl<T: Le + Copy + Default, const N: usize> Le for [T; N] {
    const SIZE: usize = T::SIZE * N;
    fn get(b: &[u8]) -> Self {
        let mut a = [T::default(); N];
        for (i, v) in a.iter_mut().enumerate() {
            *v = T::get(&b[i * T::SIZE..]);
        }
        a
    }
    fn put(&self, b: &mut [u8]) {
        for (i, v) in self.iter().enumerate() {
            v.put(&mut b[i * T::SIZE..]);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutV2Error {
    Length(usize),
    Discriminator([u8; 8]),
    Version(u8),
    MoneyWide(&'static str),
}

impl core::fmt::Display for LayoutV2Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LayoutV2Error::Length(n) => write!(f, "LaunchV2: {n} bytes, want {LAUNCH_V2_ACCOUNT_LEN}"),
            LayoutV2Error::Discriminator(d) => write!(f, "LaunchV2: discriminator {d:?} is not sha256(\"account:LaunchV2\")[..8]"),
            LayoutV2Error::Version(v) => write!(f, "LaunchV2: version {v}, want {VERSION_V2}"),
            LayoutV2Error::MoneyWide(n) => write!(f, "LaunchV2: {n} does not fit u64"),
        }
    }
}

impl std::error::Error for LayoutV2Error {}

macro_rules! launch_v2 {
    ($($off:literal $name:ident: $ty:ty,)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct LaunchV2Account {
            $(pub $name: $ty,)*
        }

        pub const FIELDS: &[(&str, usize, usize)] = &[$((stringify!($name), $off, <$ty as Le>::SIZE),)*];

        impl LaunchV2Account {
            pub fn from_body(b: &[u8; LAUNCH_V2_SIZE]) -> LaunchV2Account {
                LaunchV2Account { $($name: <$ty as Le>::get(&b[$off..]),)* }
            }

            pub fn to_body(&self) -> [u8; LAUNCH_V2_SIZE] {
                let mut b = [0u8; LAUNCH_V2_SIZE];
                $(<$ty as Le>::put(&self.$name, &mut b[$off..]);)*
                b
            }
        }
    };
}

launch_v2! {
    0 version: u8,
    1 bump: u8,
    2 status: u8,
    3 graduated: u8,
    4 decimals: u8,
    5 quote_decimals: u8,
    6 preset_id: u8,
    7 own_fees: u8,
    8 ms_class: u8,
    9 flags: u8,
    10 n_ranges: u8,
    11 n_bands: u8,
    12 n_c: u8,
    13 team_buy_rule: u8,
    14 listed: u8,
    15 pad_slot_rate: u8,
    16 slot_rate_bps: u16,
    18 pad_preset_version: [u8; 2],
    20 preset_version: u32,
    24 id: u64,
    32 created_at: i64,
    40 created_slot: u64,
    48 creator_unlock_at: i64,
    56 base_mint: Pubkey,
    88 quote_mint: Pubkey,
    120 creator: Pubkey,
    152 quote_token_program: Pubkey,
    184 token_vault: Pubkey,
    216 quote_vault: Pubkey,
    248 preset_hash: [u8; 32],
    280 phi_bps: u16,
    282 wc_bps: u16,
    284 psi_tail_bps: u16,
    286 team_buy_share_bps: u16,
    288 team_slot_max_bps: u16,
    290 platform_ms_bps: u16,
    292 floor_ms_bps: u16,
    294 ck_bps: u16,
    296 fee_ppm: [u32; 5],
    316 fee_start_ppm: [u32; 5],
    336 band_width_bps: [u16; ROW_BANDS],
    344 band_share_bps: [u16; ROW_BANDS],
    352 sqrt_p0: u128,
    368 sqrt_p: u128,
    384 sqrt_f: u128,
    400 sqrt_l_max: u128,
    416 sqrt_fee_a: u128,
    432 sqrt_fee_b: u128,
    448 quote_usd_cents_x64: u128,
    464 band_edge: [u128; BIDS_MAX + 1],
    864 rng_sqrt_a: [u128; RANGES_MAX],
    1632 rng_sqrt_b: [u128; RANGES_MAX],
    2400 rng_tokens: [u64; RANGES_MAX],
    2784 rng_ask_left: [u64; RANGES_MAX],
    3168 rng_back: [u64; RANGES_MAX],
    3552 rng_place: [u64; RANGES_MAX],
    3936 rng_ms_cap_ppm: [u32; RANGES_MAX],
    4128 rng_theta_bps: [u16; RANGES_MAX],
    4224 rng_psi_bps: [u16; RANGES_MAX],
    4320 rng_kind: [u8; RANGES_MAX],
    4368 rng_class: [u8; RANGES_MAX],
    4416 supply: u64,
    4424 dead_tokens: u64,
    4432 floor: u64,
    4440 floor_tokens: u64,
    4448 band_money: [u64; BIDS_MAX],
    4640 band_tokens: [u64; BIDS_MAX],
    4832 band_f: [u64; BIDS_MAX],
    5024 band_f_claim: [u64; BIDS_MAX],
    5216 ck_at_f: u64,
    5224 ck_floor_tokens: u64,
    5232 ms_booked: [u64; 4],
    5264 ms_paid: [u64; 4],
    5296 creator_owed: u64,
    5304 platform_owed: u64,
    5312 cut_owed: u64,
    5320 holder_owed: u64,
    5328 slot_owed_total: u64,
    5336 slot_owed: [u64; SLOT_MEMBERS],
    5416 slot_weight_ppb: [u32; SLOT_MEMBERS],
    5456 creator_ref: [Pubkey; 3],
    5552 creator_ref_owed: [u64; 3],
    5576 team_buy_owed: [u64; SLOT_MEMBERS],
    5656 team_buy_claimed: [u64; SLOT_MEMBERS],
    5736 slot_bought_owed: [u64; SLOT_MEMBERS],
    5816 slot_bought_claimed: [u64; SLOT_MEMBERS],
    5896 team_share_buy_day: u64,
    5904 window_raised: u64,
    5912 raise: u64,
    5920 volume_buy: u64,
    5928 volume_sell: u64,
    5936 trades: u64,
    5944 graduated_slot: u64,
    5952 creator_locked: u64,
    5960 backing_credited: u64,
    5968 incinerated: u64,
    5976 last_ts: i64,
    5984 book_hash: [u8; 32],
}

impl Default for LaunchV2Account {
    fn default() -> Self {
        LaunchV2Account::from_body(&[0u8; LAUNCH_V2_SIZE])
    }
}

impl LaunchV2Account {
    pub fn decode(data: &[u8]) -> Result<LaunchV2Account, LayoutV2Error> {
        if data.len() != LAUNCH_V2_ACCOUNT_LEN {
            return Err(LayoutV2Error::Length(data.len()));
        }
        let disc: [u8; 8] = data[..8].try_into().unwrap();
        if disc != LAUNCH_V2_DISCRIMINATOR {
            return Err(LayoutV2Error::Discriminator(disc));
        }
        let l = LaunchV2Account::from_body(data[8..].try_into().unwrap());
        if l.version != VERSION_V2 {
            return Err(LayoutV2Error::Version(l.version));
        }
        Ok(l)
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut a = Vec::with_capacity(LAUNCH_V2_ACCOUNT_LEN);
        a.extend_from_slice(&LAUNCH_V2_DISCRIMINATOR);
        a.extend_from_slice(&self.to_body());
        a
    }

    pub fn is_v2(data: &[u8]) -> bool {
        data.len() == LAUNCH_V2_ACCOUNT_LEN && data[..8] == LAUNCH_V2_DISCRIMINATOR
    }
}

impl LaunchV2Account {
    pub fn split_row(&self) -> SplitRow {
        let mut r = SplitRow::new(self.phi_bps, self.ck_bps, self.ms_class);
        r.wc_bps = self.wc_bps;
        r.platform_ms_bps = if self.flags & FLAG_PLATFORM_MS != 0 { self.platform_ms_bps } else { 0 };
        r.floor_ms_bps = if self.flags & FLAG_FLOOR_MS != 0 { self.floor_ms_bps } else { 0 };
        r
    }

    pub fn supply(&self) -> Supply {
        Supply { live: self.supply, dead: self.dead_tokens }
    }

    pub fn sync_supply(&mut self, mint_supply: u64) -> Option<()> {
        if mint_supply < self.supply {
            self.incinerated = self.incinerated.checked_add(self.supply - mint_supply)?;
            self.supply = mint_supply;
        }
        Some(())
    }

    pub fn book(&self) -> Book {
        let mut b = Book::empty(0, [0; FEE_LEGS]);
        for (i, r) in b.ranges.iter_mut().enumerate() {
            *r = Range {
                sqrt_a: self.rng_sqrt_a[i],
                sqrt_b: self.rng_sqrt_b[i],
                tokens: self.rng_tokens[i],
                ask_left: self.rng_ask_left[i],
                back: self.rng_back[i],
                place: self.rng_place[i] as u128,
                theta_bps: self.rng_theta_bps[i],
                psi_bps: self.rng_psi_bps[i],
                kind: self.rng_kind[i],
                class: self.rng_class[i],
                ms_cap_ppm: self.rng_ms_cap_ppm[i],
            };
        }
        b.sqrt_p = self.sqrt_p;
        b.sqrt_f = self.sqrt_f;
        b.sqrt_l_max = self.sqrt_l_max;
        b.floor = self.floor as u128;
        b.floor_tokens = self.floor_tokens;
        b.n_bands = self.n_bands;
        b.n_c = self.n_c;
        b.band_edge = self.band_edge;
        b.band_money = self.band_money.map(|v| v as u128);
        b.band_tokens = self.band_tokens;
        b.band_f = self.band_f.map(|v| v as u128);
        b.band_f_claim = self.band_f_claim.map(|v| v as u128);
        b.ck_at_f = self.ck_at_f as u128;
        b.ck_floor_tokens = self.ck_floor_tokens;
        b.fee_ppm = self.fee_ppm;
        b.fee_start_ppm = self.fee_start_ppm;
        b.sqrt_fee_a = self.sqrt_fee_a;
        b.sqrt_fee_b = self.sqrt_fee_b;
        b.graduated = self.graduated != 0;
        b.own_fees = self.own_fees != 0;
        b.n_ranges = self.n_ranges;
        b
    }

    pub fn set_book(&mut self, b: &Book) -> Result<(), LayoutV2Error> {
        let n = |v: u128, name: &'static str| u64::try_from(v).map_err(|_| LayoutV2Error::MoneyWide(name));
        let mut place = [0u64; RANGES_MAX];
        for (i, r) in b.ranges.iter().enumerate() {
            place[i] = n(r.place, "rng_place")?;
        }
        let (mut band_money, mut band_f, mut band_f_claim) = ([0u64; BIDS_MAX], [0u64; BIDS_MAX], [0u64; BIDS_MAX]);
        for i in 0..BIDS_MAX {
            band_money[i] = n(b.band_money[i], "band_money")?;
            band_f[i] = n(b.band_f[i], "band_f")?;
            band_f_claim[i] = n(b.band_f_claim[i], "band_f_claim")?;
        }
        let (floor, ck_at_f) = (n(b.floor, "floor")?, n(b.ck_at_f, "ck_at_f")?);
        for (i, r) in b.ranges.iter().enumerate() {
            self.rng_sqrt_a[i] = r.sqrt_a;
            self.rng_sqrt_b[i] = r.sqrt_b;
            self.rng_tokens[i] = r.tokens;
            self.rng_ask_left[i] = r.ask_left;
            self.rng_back[i] = r.back;
            self.rng_place[i] = place[i];
            self.rng_theta_bps[i] = r.theta_bps;
            self.rng_psi_bps[i] = r.psi_bps;
            self.rng_kind[i] = r.kind;
            self.rng_class[i] = r.class;
            self.rng_ms_cap_ppm[i] = r.ms_cap_ppm;
        }
        self.sqrt_p = b.sqrt_p;
        self.sqrt_f = b.sqrt_f;
        self.sqrt_l_max = b.sqrt_l_max;
        self.floor = floor;
        self.floor_tokens = b.floor_tokens;
        self.n_bands = b.n_bands;
        self.n_c = b.n_c;
        self.band_edge = b.band_edge;
        self.band_money = band_money;
        self.band_tokens = b.band_tokens;
        self.band_f = band_f;
        self.band_f_claim = band_f_claim;
        self.ck_at_f = ck_at_f;
        self.ck_floor_tokens = b.ck_floor_tokens;
        self.fee_ppm = b.fee_ppm;
        self.fee_start_ppm = b.fee_start_ppm;
        self.sqrt_fee_a = b.sqrt_fee_a;
        self.sqrt_fee_b = b.sqrt_fee_b;
        self.graduated = b.graduated as u8;
        self.own_fees = b.own_fees as u8;
        self.n_ranges = b.n_ranges;
        Ok(())
    }
}
