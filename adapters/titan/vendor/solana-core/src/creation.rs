// SPDX-License-Identifier: Apache-2.0
use solana_pubkey::Pubkey;

pub const EVENT_IX_TAG: [u8; 8] = [228, 69, 165, 46, 81, 203, 154, 29];
pub const LAUNCH_CREATED_V2_DISCRIMINATOR: [u8; 8] = [76, 14, 158, 185, 239, 107, 86, 142];
pub const LAUNCH_CREATED_V2_DATA_LEN: usize = 16 + BODY_LEN;
const BODY_LEN: usize = 32 * 4 + 8 + 1 + 4 + 32 + 16 + 8 * 3 + 8 + 32 + 10 + 8 + 8 + 1 + 16 + 8 + 1 + 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaunchCreatedV2 {
    pub launch: Pubkey,
    pub mint: Pubkey,
    pub quote_mint: Pubkey,
    pub creator: Pubkey,
    pub id: u64,
    pub preset_id: u8,
    pub preset_version: u32,
    pub preset_hash: [u8; 32],
    pub p0_x64: u128,
    pub raise: u64,
    pub window_tokens: u64,
    pub creator_buy_tokens: u64,
    pub create_fee: u64,
    pub name: [u8; 32],
    pub symbol: [u8; 10],
    pub slot: u64,
    pub ts: i64,
    pub listed: bool,
    pub quote_usd_cents_x64: u128,
    pub native_create_fee: u64,
    pub row_version: u8,
    pub sqrt_p0: u128,
}

struct Rd<'a>(&'a [u8]);

impl Rd<'_> {
    fn take<const N: usize>(&mut self) -> [u8; N] {
        let (h, t) = self.0.split_at(N);
        self.0 = t;
        h.try_into().unwrap()
    }
    fn key(&mut self) -> Pubkey {
        Pubkey::new_from_array(self.take::<32>())
    }
    fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.take())
    }
    fn u128(&mut self) -> u128 {
        u128::from_le_bytes(self.take())
    }
}

impl LaunchCreatedV2 {
    pub fn parse(data: &[u8]) -> Option<LaunchCreatedV2> {
        if data.len() != LAUNCH_CREATED_V2_DATA_LEN || data[..8] != EVENT_IX_TAG || data[8..16] != LAUNCH_CREATED_V2_DISCRIMINATOR {
            return None;
        }
        let mut r = Rd(&data[16..]);
        Some(LaunchCreatedV2 {
            launch: r.key(),
            mint: r.key(),
            quote_mint: r.key(),
            creator: r.key(),
            id: r.u64(),
            preset_id: r.take::<1>()[0],
            preset_version: u32::from_le_bytes(r.take()),
            preset_hash: r.take(),
            p0_x64: r.u128(),
            raise: r.u64(),
            window_tokens: r.u64(),
            creator_buy_tokens: r.u64(),
            create_fee: r.u64(),
            name: r.take(),
            symbol: r.take(),
            slot: r.u64(),
            ts: i64::from_le_bytes(r.take()),
            listed: r.take::<1>()[0] != 0,
            quote_usd_cents_x64: r.u128(),
            native_create_fee: r.u64(),
            row_version: r.take::<1>()[0],
            sqrt_p0: r.u128(),
        })
    }

    pub fn name_str(&self) -> &str {
        trimmed(&self.name)
    }

    pub fn symbol_str(&self) -> &str {
        trimmed(&self.symbol)
    }

    pub fn to_data(&self) -> Vec<u8> {
        let mut d = Vec::with_capacity(LAUNCH_CREATED_V2_DATA_LEN);
        d.extend_from_slice(&EVENT_IX_TAG);
        d.extend_from_slice(&LAUNCH_CREATED_V2_DISCRIMINATOR);
        for k in [&self.launch, &self.mint, &self.quote_mint, &self.creator] {
            d.extend_from_slice(k.as_ref());
        }
        d.extend_from_slice(&self.id.to_le_bytes());
        d.push(self.preset_id);
        d.extend_from_slice(&self.preset_version.to_le_bytes());
        d.extend_from_slice(&self.preset_hash);
        d.extend_from_slice(&self.p0_x64.to_le_bytes());
        for v in [self.raise, self.window_tokens, self.creator_buy_tokens, self.create_fee] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&self.name);
        d.extend_from_slice(&self.symbol);
        d.extend_from_slice(&self.slot.to_le_bytes());
        d.extend_from_slice(&self.ts.to_le_bytes());
        d.push(self.listed as u8);
        d.extend_from_slice(&self.quote_usd_cents_x64.to_le_bytes());
        d.extend_from_slice(&self.native_create_fee.to_le_bytes());
        d.push(self.row_version);
        d.extend_from_slice(&self.sqrt_p0.to_le_bytes());
        d
    }
}

fn trimmed(b: &[u8]) -> &str {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    std::str::from_utf8(&b[..end]).unwrap_or("")
}
