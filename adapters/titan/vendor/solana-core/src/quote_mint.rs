// SPDX-License-Identifier: Apache-2.0
use solana_pubkey::Pubkey;

use crate::hook::FLOOR_HOOK_ID;
use crate::keys::TOKEN_2022_PROGRAM_ID;

pub const EXT_TRANSFER_FEE: u16 = 1;
pub const EXT_TRANSFER_HOOK: u16 = 14;

pub type TransferFee = (u64, u64, u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QuoteMint {
    pub hooked: bool,
    pub fee: Option<(TransferFee, TransferFee)>,
}

pub fn mint_tlv(data: &[u8]) -> Vec<(u16, &[u8])> {
    let mut out = vec![];
    if data.len() <= 166 {
        return out;
    }
    let mut o = 166;
    while o + 4 <= data.len() {
        let t = u16::from_le_bytes([data[o], data[o + 1]]);
        let l = u16::from_le_bytes([data[o + 2], data[o + 3]]) as usize;
        if t == 0 || o + 4 + l > data.len() {
            break;
        }
        out.push((t, &data[o + 4..o + 4 + l]));
        o += 4 + l;
    }
    out
}

impl QuoteMint {
    pub fn decode(holder: &Pubkey, data: &[u8]) -> QuoteMint {
        let tlv = mint_tlv(data);
        let hooked = *holder == TOKEN_2022_PROGRAM_ID && tlv.iter().any(|(t, v)| *t == EXT_TRANSFER_HOOK && v.get(32..64) == Some(FLOOR_HOOK_ID.as_ref()));
        let fee = tlv.iter().find(|(t, _)| *t == EXT_TRANSFER_FEE).and_then(|(_, v)| {
            let at = |o: usize| -> Option<TransferFee> {
                let r = v.get(o..o + 18)?;
                Some((u64::from_le_bytes(r[..8].try_into().ok()?), u64::from_le_bytes(r[8..16].try_into().ok()?), u16::from_le_bytes([r[16], r[17]])))
            };
            Some((at(72)?, at(90)?))
        });
        QuoteMint { hooked, fee }
    }

    pub fn transfer_fee(&self, epoch: u64, amount: u64) -> u64 {
        let Some((old, new)) = self.fee else { return 0 };
        let (_, max, bps) = if epoch >= new.0 { new } else { old };
        if bps == 0 {
            return 0;
        }
        u64::try_from((amount as u128 * bps as u128).div_ceil(10_000)).unwrap_or(u64::MAX).min(max)
    }

    pub fn received(&self, epoch: u64, amount: u64) -> u64 {
        amount - self.transfer_fee(epoch, amount).min(amount)
    }
}
