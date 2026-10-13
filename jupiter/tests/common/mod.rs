//! Shared test support: the program binaries, the devnet sale snapshots, and synthetic `Launch`
//! accounts.
#![allow(dead_code)]

use floor_types::ids::DEVNET;
use std::collections::HashMap;
use std::path::PathBuf;

use floor_sale_amm::accounts::{launch_pda, launch_program, sale_vault_pda};
use floor_types::constants::{LAUNCH_STATE_BONDING, SALE_TOKENS};
use floor_types::keys::WSOL_MINT;
use floor_types::layout::{Launch, Layout};
use floor_types::U128;
use jupiter_amm_interface::{Amm, AmmContext, KeyedAccount};
use jupiter_amm_test_kit::PoolSnapshot;
use sha2::{Digest, Sha256};
use solana_account::Account;
use solana_pubkey::Pubkey;

pub const JUP: Pubkey = Pubkey::from_str_const("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");
pub const CLOCK: Pubkey = Pubkey::from_str_const("SysvarC1ock11111111111111111111111111111111");
/// The sysvars a buy reads implicitly. `Rent` is included so LiteSVM charges the cluster's rent, not
/// its built-in default: the sale escrow is funded at the cluster's rent-exempt minimum, and under
/// LiteSVM's higher default a small first buy would leave it below that minimum.
pub const SYSVARS: [Pubkey; 4] = [
    CLOCK,
    Pubkey::from_str_const("SysvarRent111111111111111111111111111111111"),
    Pubkey::from_str_const("SysvarLastRestartS1ot1111111111111111111111"),
    Pubkey::from_str_const("SysvarStakeHistory1111111111111111111111111"),
];

pub fn wsol() -> Pubkey {
    Pubkey::new_from_array(WSOL_MINT)
}

// ---- the programs ------------------------------------------------------------------------------

/// One program the tests load: its fixture name, id, ELF length and sha256, and whether it can be
/// dumped from devnet. `floor` is a `floor_launch` build carrying the stateless sell-back and
/// `sell_back_wsol` (a build of the program source, not dumped: devnet may run an older build).
/// Token-2022 is kept as the whole ProgramData payload, as `solana program dump` writes it.
pub struct Program {
    pub name: &'static str,
    pub id: &'static str,
    pub len: usize,
    pub sha256: &'static str,
    pub dumpable: bool,
}

pub const PROGRAMS: &[Program] = &[
    Program {
        name: "floor",
        id: "93VBXddbRj7py1PxGGCy1cAhPJQYNseca2bajaeqdnAG",
        len: 1_059_840,
        sha256: "8307d1a2f2d45378f3116e89dc581607fd9119d89d7da940084f5c9b52e99090",
        dumpable: false,
    },
    Program {
        name: "spl_token_2022",
        id: "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
        len: 711_008,
        sha256: "b781b71d230a9910de4d3c103e72608f597fc73c83ee207242ac82b708382a6a",
        dumpable: true,
    },
];

pub fn program_path(name: &str) -> PathBuf {
    PathBuf::from("tests/fixtures").join(format!("{name}.so"))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Dump `p` from the cluster: the ProgramData account's payload after its 45-byte header, trimmed to
/// the recorded length (every byte past it must be zero), hash-checked.
fn dump_program(rpc: &str, p: &Program) -> Result<Vec<u8>, String> {
    let id: Pubkey = p.id.parse().unwrap();
    let program = jupiter_amm_test_kit::fetch_account(rpc, &id).map_err(|e| e.to_string())?.ok_or("program account missing")?;
    let programdata = Pubkey::try_from(&program.data[4..36]).map_err(|_| "malformed program account")?;
    let pd = jupiter_amm_test_kit::fetch_account(rpc, &programdata).map_err(|e| e.to_string())?.ok_or("programdata missing")?;
    let payload = &pd.data[45..];
    if payload.len() < p.len || payload[p.len..].iter().any(|b| *b != 0) {
        return Err(format!("{}: payload {} B does not end in zeros after {} B", p.name, payload.len(), p.len));
    }
    let elf = payload[..p.len].to_vec();
    let got = sha256_hex(&elf);
    if got != p.sha256 {
        return Err(format!("{}: deployed sha256 {got}, expected {}", p.name, p.sha256));
    }
    Ok(elf)
}

/// Every program present with the recorded hash (with `RPC` set, a missing one is dumped first).
/// Panics on a binary that is present but not the recorded build.
pub fn programs_ready() -> Result<Vec<(Pubkey, Vec<u8>)>, String> {
    let rpc = std::env::var("RPC").ok();
    let mut out = Vec::new();
    for p in PROGRAMS {
        let path = program_path(p.name);
        if !path.exists() {
            if !p.dumpable {
                return Err(format!("{path:?} is missing: build `floor_launch` from the program source and copy it there"));
            }
            let Some(rpc) = rpc.as_deref() else {
                return Err(format!("{path:?} is missing; rerun with RPC=<devnet url> to dump it"));
            };
            std::fs::write(&path, dump_program(rpc, p)?).map_err(|e| e.to_string())?;
        }
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        assert_eq!(sha256_hex(&bytes), p.sha256, "{path:?} is not the recorded build");
        assert_eq!(bytes.len(), p.len, "{path:?} length");
        out.push((p.id.parse().unwrap(), bytes));
    }
    Ok(out)
}

// ---- the devnet sales --------------------------------------------------------------------------

/// The devnet launches that were still in their sale when the snapshots were taken, all quoted in
/// wSOL: `(launch, mint)`.
pub const SALES: &[(&str, &str)] = &[
    ("GQYcgETr9DSPb6bd2zibHViviE8bPTa5chZgDDGhQiii", "5swrdx3cBuiWPYx2KXedvpnMNixRUBTmSA6242vsiB1D"),
    ("4bCBM8p8K63t6HKKjAtjEM1ov4KpuW9M5NsJxfq3XetN", "9S4YM9ETMJTTdZEXhJRT5qUwymSifHG1Ro4TxZ96rB1D"),
    ("ykpUGi82DfmivZiHv2t8CDFHntJjignsMLCQFSGzzjM", "9gijWgMZrBCF888SQgJuCLAA72bWMT1uHE3M2tbptB1D"),
    ("iqYuAz2r1m8MsMZbJM7VwG1Zkn5FApm3goYa1Y3jrhd", "2P7sDUcvcWsNhbAPQ7eVZkzha9168H7K4sSuXazJeB1D"),
];

pub fn pk(s: &str) -> Pubkey {
    s.parse().unwrap()
}

pub fn fixtures_dir(launch: &Pubkey) -> PathBuf {
    PathBuf::from("tests/fixtures/accounts").join(launch.to_string())
}

/// The accounts a buy reads or writes that exist before it: the launch, its native escrow, its
/// token vault, the mint, the wSOL mint (the kit funds the input from it) and the sysvars.
fn snapshot_keys(launch_key: &Pubkey, launch: &Launch) -> Vec<Pubkey> {
    let mut keys = vec![
        *launch_key,
        sale_vault_pda(&DEVNET, launch_key),
        Pubkey::new_from_array(launch.token_vault),
        Pubkey::new_from_array(launch.mint),
        wsol(),
    ];
    keys.extend(SYSVARS);
    keys
}

/// Snapshot `launch` into its fixtures directory when it is missing or `REFRESH` is set (needs
/// `RPC`). Read-only: nothing is sent to the cluster.
pub fn ensure_snapshot(launch: &Pubkey) -> Result<PathBuf, String> {
    let dir = fixtures_dir(launch);
    if dir.exists() && std::env::var_os("REFRESH").is_none() {
        return Ok(dir);
    }
    let rpc = std::env::var("RPC").map_err(|_| format!("{launch} has no snapshot at {dir:?}; rerun with RPC=<devnet url>"))?;
    let account = jupiter_amm_test_kit::fetch_account(&rpc, launch).map_err(|e| e.to_string())?.ok_or("launch not found")?;
    let decoded = floor_quote::state::decode_launch(&account.data).map_err(|e| e.to_string())?;
    let mut accounts = HashMap::new();
    for key in snapshot_keys(launch, &decoded) {
        if let Some(a) = jupiter_amm_test_kit::fetch_account(&rpc, &key).map_err(|e| e.to_string())? {
            accounts.insert(key, a);
        }
    }
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    PoolSnapshot::from_accounts(accounts).save_dir(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Every `<pubkey>.bin` of a snapshot directory.
pub fn load_accounts(dir: &PathBuf) -> HashMap<Pubkey, Account> {
    let mut out = HashMap::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let Some(key) = path.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<Pubkey>().ok()) else { continue };
        let account: Account = bincode::deserialize(&std::fs::read(&path).unwrap()).unwrap();
        out.insert(key, account);
    }
    out
}

// ---- synthetic launches ------------------------------------------------------------------------

pub const PRICE_ONE: u128 = 1u128 << 64;

/// A bonding wSOL `Launch` with the given price, sold amount and platform fee rate.
pub fn synth_launch(mint: &Pubkey, price_x64: u128, sale_tokens: u64, sold: u64, team_bps: u16) -> Launch {
    let mut l: Launch = bytemuck::Zeroable::zeroed();
    l.version = 3;
    l.state = LAUNCH_STATE_BONDING;
    l.mint = mint.to_bytes();
    l.token_vault = Pubkey::new_unique().to_bytes();
    l.quote_mint = WSOL_MINT;
    l.price_x64 = U128::new(price_x64);
    l.sale_tokens = sale_tokens;
    l.sold = sold;
    l.team_bps = team_bps;
    l
}

pub fn default_sale(price_x64: u128, sold: u64, team_bps: u16) -> (Pubkey, Launch) {
    let mint = Pubkey::new_unique();
    (mint, synth_launch(&mint, price_x64, SALE_TOKENS, sold, team_bps))
}

/// The account bytes `floor_launch` stores: discriminator, then the struct.
pub fn launch_account(l: &Launch) -> Account {
    let mut data = Launch::DISCRIMINATOR.to_vec();
    data.extend_from_slice(bytemuck::bytes_of(l));
    Account { lamports: 10_000_000, data, owner: launch_program(&DEVNET), executable: false, rent_epoch: 0 }
}

pub fn keyed(l: &Launch) -> KeyedAccount {
    let key = launch_pda(&DEVNET, &Pubkey::new_from_array(l.mint));
    KeyedAccount { key, account: launch_account(l), params: None }
}

pub fn amm_of(l: &Launch) -> floor_sale_amm::FloorSaleAmm {
    floor_sale_amm::FloorSaleAmm::from_keyed_account(&keyed(l), &AmmContext::default()).unwrap()
}
