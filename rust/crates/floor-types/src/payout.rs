pub const AUTHORIZE_CLAIM: [u8; 8] = [7, 168, 241, 195, 41, 85, 16, 181];
pub const PAY_HOLDER_AUTHORIZED: [u8; 8] = [90, 160, 177, 182, 35, 33, 10, 229];

pub struct InstructionView<'a> {
    pub program: &'a [u8],
    pub accounts: &'a [u8],
    pub data: &'a [u8],
}

impl InstructionView<'_> {
    pub fn key(&self, index: usize) -> Option<&[u8]> {
        let start = index.checked_mul(33)?.checked_add(1)?;
        self.accounts.get(start..start.checked_add(32)?)
    }

    pub fn signer(&self, index: usize) -> bool {
        index.checked_mul(33).and_then(|at| self.accounts.get(at)).is_some_and(|flags| flags & 1 != 0)
    }
}

fn u16_at(data: &[u8], at: usize) -> Option<usize> {
    let bytes = data.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]) as usize)
}

pub fn current_index(data: &[u8]) -> Option<usize> {
    u16_at(data, data.len().checked_sub(2)?)
}

pub fn instruction(data: &[u8], index: usize) -> Option<InstructionView<'_>> {
    let count = u16_at(data, 0)?;
    if index >= count { return None; }
    let start = u16_at(data, 2usize.checked_add(index.checked_mul(2)?)?)?;
    let accounts_start = start.checked_add(2)?;
    let program_start = accounts_start.checked_add(u16_at(data, start)?.checked_mul(33)?)?;
    let payload_start = program_start.checked_add(34)?;
    Some(InstructionView {
        program: data.get(program_start..program_start.checked_add(32)?)?,
        accounts: data.get(accounts_start..program_start)?,
        data: data.get(payload_start..payload_start.checked_add(u16_at(data, program_start + 32)?)?)?,
    })
}

pub fn adjacent(data: &[u8], amount: u64, paying: bool) -> bool {
    let Some(index) = current_index(data) else { return false; };
    let Some(auth_index) = (if paying { index.checked_sub(1) } else { Some(index) }) else { return false; };
    let Some(auth) = instruction(data, auth_index) else { return false; };
    let Some(pay) = instruction(data, auth_index + 1) else { return false; };
    matches(&auth, &pay, amount)
}

pub fn matches(authorize: &InstructionView<'_>, payout: &InstructionView<'_>, amount: u64) -> bool {
    let args = amount.to_le_bytes();
    amount > 0
        && authorize.program == crate::ids::FLOOR_HOLDERS_ID
        && payout.program == crate::ids::FLOOR_HOOK_ID
        && authorize.data.len() == 16
        && payout.data.len() == 16
        && authorize.data[..8] == AUTHORIZE_CLAIM
        && payout.data[..8] == PAY_HOLDER_AUTHORIZED
        && authorize.data[8..] == args
        && payout.data[8..] == args
        && authorize.signer(0)
        && payout.signer(0)
        && [0, 3, 9, 10, 6, 11, 12, 13, 14, 15, 16, 8].iter().enumerate()
            .all(|(pay, auth)| authorize.key(*auth).is_some() && authorize.key(*auth) == payout.key(pay))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(program: [u8; 32], keys: &[[u8; 32]], discriminator: [u8; 8], amount: u64) -> Vec<u8> {
        let mut out = (keys.len() as u16).to_le_bytes().to_vec();
        for (i, key) in keys.iter().enumerate() {
            out.push(u8::from(i == 0));
            out.extend_from_slice(key);
        }
        out.extend_from_slice(&program);
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(&discriminator);
        out.extend_from_slice(&amount.to_le_bytes());
        out
    }

    fn pair(current: u16) -> Vec<u8> {
        let auth: Vec<_> = (0..20).map(|i| [i; 32]).collect();
        let pay: Vec<_> = [0, 3, 9, 10, 6, 11, 12, 13, 14, 15, 16, 8].map(|i| auth[i]).to_vec();
        let a = encoded(crate::ids::FLOOR_HOLDERS_ID, &auth, AUTHORIZE_CLAIM, 42);
        let p = encoded(crate::ids::FLOOR_HOOK_ID, &pay, PAY_HOLDER_AUTHORIZED, 42);
        let mut out = vec![2, 0, 6, 0];
        out.extend_from_slice(&((6 + a.len()) as u16).to_le_bytes());
        out.extend_from_slice(&a);
        out.extend_from_slice(&p);
        out.extend_from_slice(&current.to_le_bytes());
        out
    }

    #[test]
    fn binds_every_payout_account_amount_program_and_signer() {
        let data = pair(1);
        assert_eq!(current_index(&data), Some(1));
        let a = instruction(&data, 0).unwrap();
        let p = instruction(&data, 1).unwrap();
        assert!(matches(&a, &p, 42));
        assert!(!matches(&a, &p, 41));
        assert!(!matches(&a, &p, 0));
        for index in 0..12 {
            let mut accounts = p.accounts.to_vec();
            accounts[index * 33 + 1] ^= 1;
            let bad = InstructionView { accounts: &accounts, program: p.program, data: p.data };
            assert!(!matches(&a, &bad, 42), "binding {index}");
        }
        let mut accounts = p.accounts.to_vec();
        accounts[0] = 0;
        assert!(!matches(&a, &InstructionView { accounts: &accounts, program: p.program, data: p.data }, 42));
        assert!(!matches(&a, &InstructionView { accounts: p.accounts, program: &[0; 32], data: p.data }, 42));
    }

    #[test]
    fn replay_or_nonadjacent_authorization_is_refused() {
        let data = pair(1);
        let payout = instruction(&data, 1).unwrap();
        assert!(!matches(&payout, &payout, 42));
        assert!(instruction(&data, 2).is_none());
        for len in 0..6 { assert!(instruction(&data[..len], 0).is_none()); }
    }
    #[test]
    fn authorization_and_consumption_must_be_adjacent_and_current() {
        assert!(adjacent(&pair(0), 42, false));
        assert!(adjacent(&pair(1), 42, true));
        assert!(!adjacent(&pair(0), 42, true));
        assert!(!adjacent(&pair(1), 42, false));
        assert!(!adjacent(&pair(2), 42, true));
        assert!(!adjacent(&pair(1), 43, true));
        let data = pair(1);
        for len in 0..data.len() - 2 {
            assert!(!adjacent(&data[..len], 42, true));
        }
    }

    #[test]
    fn wrong_authorizer_program_signature_and_payload_are_refused() {
        let data = pair(1);
        let a = instruction(&data, 0).unwrap();
        let p = instruction(&data, 1).unwrap();
        assert!(!matches(&InstructionView { program: &[0; 32], accounts: a.accounts, data: a.data }, &p, 42));
        let mut accounts = a.accounts.to_vec();
        accounts[0] = 0;
        assert!(!matches(&InstructionView { program: a.program, accounts: &accounts, data: a.data }, &p, 42));
        let mut args = a.data.to_vec();
        args[0] ^= 1;
        assert!(!matches(&InstructionView { program: a.program, accounts: a.accounts, data: &args }, &p, 42));
        assert!(!a.signer(usize::MAX));
        assert!(a.key(usize::MAX).is_none());
    }

}
