//! Program IDs, as raw bytes (no Solana crate), per cluster.
//!
//! One build serves every cluster in [`CLUSTERS`]: code that meets a FLOOR or Carpenter account looks
//! the cluster up by the account's owner ([`cluster_by_amm`], [`cluster_by_launch`],
//! [`cluster_by_hook`]) and takes every other program id, and every PDA, from that cluster's
//! [`ClusterIds`]. Adding a cluster is one more entry in [`CLUSTERS`] (with its Carpenter half in
//! `carpenter_types::ids::AMM_CLUSTERS`).
//!
//! The AMM's ids and the test-only mock hook's are `carpenter_types::ids`, re-exported here.

pub use carpenter_types::ids::*;

/// One cluster's program ids: the AMM FLOOR is hooked on, and FLOOR's four programs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClusterIds {
    /// The cluster's name (`"devnet"`, `"mainnet"`, ...).
    pub name: &'static str,
    /// `carpenter_amm`.
    pub carpenter_amm: [u8; 32],
    /// `floor_hook`: FLOOR's approved hook.
    pub floor_hook: [u8; 32],
    /// `floor_launch`: factory, sale, lock, registry, claims.
    pub floor_launch: [u8; 32],
    /// `floor_holders`: the holder-rewards transfer hook.
    pub floor_holders: [u8; 32],
    /// `floor_vesting`: the standalone vesting lock.
    pub floor_vesting: [u8; 32],
}

/// Mainnet-beta.
///
/// | program | base58 |
/// |---|---|
/// | `carpenter_amm` | `DdeiDx4ncHo9DXGZm1GrPe7mFKv7kNU3yx95uQEV2APA` |
/// | `floor_hook` | `9uXLyyZginMWdUeu22kp7XV7cpV8183xfsVqUxKi5Viy` |
/// | `floor_launch` | `8vfdPWfaA3b9GtJoFUwc36JKDaMfVCJ4oT9HrCrQ6Tdi` |
/// | `floor_holders` | `2EUkJXSwCdRAuhesw2D9FXyGfbi6Kw2yZDyQUnCwc7te` |
/// | `floor_vesting` | `HHiwbXs3egRwtgEPEKx1PHpBU8gq5VSbhfrGrcCd4CSC` |
pub const MAINNET: ClusterIds = ClusterIds {
    name: "mainnet",
    carpenter_amm: AMM_MAINNET.carpenter_amm,
    floor_hook: [132, 82, 2, 21, 189, 64, 53, 236, 151, 14, 216, 63, 254, 189, 160, 51, 203, 253, 199, 159, 102, 236, 30, 202, 145, 219, 195, 38, 58, 59, 2, 34],
    floor_launch: [117, 193, 68, 121, 229, 78, 125, 173, 249, 224, 50, 216, 0, 167, 196, 132, 241, 125, 16, 17, 234, 106, 118, 29, 48, 71, 92, 148, 36, 204, 160, 17],
    floor_holders: [18, 79, 156, 81, 23, 202, 14, 16, 103, 20, 208, 65, 66, 82, 27, 10, 124, 163, 196, 152, 104, 47, 235, 237, 219, 233, 36, 117, 6, 105, 137, 3],
    floor_vesting: [242, 3, 180, 250, 98, 146, 129, 227, 251, 255, 129, 251, 122, 253, 144, 116, 128, 65, 155, 178, 120, 123, 83, 207, 201, 143, 69, 155, 200, 16, 39, 25],
};

/// Devnet.
///
/// | program | base58 |
/// |---|---|
/// | `carpenter_amm` | `Arv1M4LifBrWMywvxpeAGvh37aVHPjpXPCg7sacbe3Hy` |
/// | `floor_hook` | `6foted69N6wB1oJzsxccbXUxefBt3Hkn7tN2yHifXnTS` |
/// | `floor_launch` | `93VBXddbRj7py1PxGGCy1cAhPJQYNseca2bajaeqdnAG` |
/// | `floor_holders` | `DShmCjpc34oRb3mqjLvp1fs1rYfY6ZLvj1G38d1s6XPZ` |
/// | `floor_vesting` | `5UfmuRmDdsVCE4q9UXW82thrzcaTAGMNgQZcugy7jjfm` |
pub const DEVNET: ClusterIds = ClusterIds {
    name: "devnet",
    carpenter_amm: AMM_DEVNET.carpenter_amm,
    floor_hook: [84, 59, 134, 77, 250, 63, 209, 214, 14, 157, 191, 217, 181, 243, 127, 231, 251, 160, 234, 155, 119, 97, 176, 101, 71, 75, 18, 49, 240, 173, 140, 1],
    floor_launch: [119, 128, 133, 160, 33, 92, 24, 90, 89, 49, 100, 187, 190, 164, 138, 173, 110, 37, 63, 85, 220, 229, 15, 152, 96, 187, 86, 118, 116, 251, 170, 109],
    floor_holders: [184, 225, 218, 107, 213, 68, 63, 48, 119, 209, 30, 156, 130, 134, 212, 108, 153, 59, 223, 1, 78, 253, 157, 200, 245, 121, 189, 235, 214, 17, 241, 188],
    floor_vesting: [66, 133, 69, 141, 25, 58, 96, 40, 63, 123, 72, 28, 167, 246, 24, 251, 162, 70, 153, 74, 1, 29, 38, 54, 10, 146, 231, 105, 7, 12, 161, 96],
};

/// Every cluster this build knows. Each program id appears in one entry only (a test checks it),
/// so an owner names at most one cluster.
pub const CLUSTERS: &[ClusterIds] = &[MAINNET, DEVNET];

/// The cluster whose `carpenter_amm` is `program`.
pub fn cluster_by_amm(program: &[u8; 32]) -> Option<&'static ClusterIds> {
    cluster_by_amm_in(CLUSTERS, program)
}

/// The cluster whose `floor_hook` is `program`.
pub fn cluster_by_hook(program: &[u8; 32]) -> Option<&'static ClusterIds> {
    cluster_by_hook_in(CLUSTERS, program)
}

/// The cluster whose `floor_launch` is `program`.
pub fn cluster_by_launch(program: &[u8; 32]) -> Option<&'static ClusterIds> {
    cluster_by_launch_in(CLUSTERS, program)
}

/// [`cluster_by_amm`] over a caller's table.
pub fn cluster_by_amm_in<'a>(clusters: &'a [ClusterIds], program: &[u8; 32]) -> Option<&'a ClusterIds> {
    clusters.iter().find(|c| c.carpenter_amm == *program)
}

/// [`cluster_by_hook`] over a caller's table.
pub fn cluster_by_hook_in<'a>(clusters: &'a [ClusterIds], program: &[u8; 32]) -> Option<&'a ClusterIds> {
    clusters.iter().find(|c| c.floor_hook == *program)
}

/// [`cluster_by_launch`] over a caller's table.
pub fn cluster_by_launch_in<'a>(clusters: &'a [ClusterIds], program: &[u8; 32]) -> Option<&'a ClusterIds> {
    clusters.iter().find(|c| c.floor_launch == *program)
}

/// Whether `program` is `floor_hook` on any cluster in [`CLUSTERS`].
pub fn is_floor_hook(program: &[u8; 32]) -> bool {
    cluster_by_hook(program).is_some()
}

/// `floor_hook` on mainnet-beta, the id the shipped IDL names. Code that may meet another
/// cluster's accounts uses the cluster lookups instead.
///
/// base58 `9uXLyyZginMWdUeu22kp7XV7cpV8183xfsVqUxKi5Viy`
pub const FLOOR_HOOK_ID: [u8; 32] = MAINNET.floor_hook;

/// `floor_launch` on mainnet-beta (see [`FLOOR_HOOK_ID`]).
///
/// base58 `8vfdPWfaA3b9GtJoFUwc36JKDaMfVCJ4oT9HrCrQ6Tdi`
pub const FLOOR_LAUNCH_ID: [u8; 32] = MAINNET.floor_launch;

/// `floor_vesting` on mainnet-beta (see [`FLOOR_HOOK_ID`]).
///
/// base58 `HHiwbXs3egRwtgEPEKx1PHpBU8gq5VSbhfrGrcCd4CSC`
pub const FLOOR_VESTING_ID: [u8; 32] = MAINNET.floor_vesting;

/// `floor_holders` on mainnet-beta (see [`FLOOR_HOOK_ID`]).
///
/// base58 `2EUkJXSwCdRAuhesw2D9FXyGfbi6Kw2yZDyQUnCwc7te`
pub const FLOOR_HOLDERS_ID: [u8; 32] = MAINNET.floor_holders;

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic second cluster, test-only.
    const OTHER: ClusterIds = ClusterIds {
        name: "other",
        carpenter_amm: [1u8; 32],
        floor_hook: [2u8; 32],
        floor_launch: [3u8; 32],
        floor_holders: [4u8; 32],
        floor_vesting: [5u8; 32],
    };
    const TABLE: &[ClusterIds] = &[DEVNET, OTHER];

    #[test]
    fn the_id_aliases_are_the_mainnet_entry() {
        assert_eq!(FLOOR_HOOK_ID, MAINNET.floor_hook);
        assert_eq!(FLOOR_LAUNCH_ID, MAINNET.floor_launch);
        assert_eq!(FLOOR_HOLDERS_ID, MAINNET.floor_holders);
        assert_eq!(FLOOR_VESTING_ID, MAINNET.floor_vesting);
        assert_eq!(CARPENTER_AMM_ID, MAINNET.carpenter_amm);
        assert_eq!(cluster_by_hook(&DEVNET.floor_hook).map(|c| c.name), Some("devnet"));
        assert_eq!(cluster_by_launch(&MAINNET.floor_launch).map(|c| c.name), Some("mainnet"));
    }

    #[test]
    fn lookups_find_the_cluster_by_any_of_its_programs_and_refuse_an_unknown_one() {
        assert_eq!(cluster_by_amm_in(TABLE, &[1u8; 32]).map(|c| c.name), Some("other"));
        assert_eq!(cluster_by_hook_in(TABLE, &[2u8; 32]).map(|c| c.name), Some("other"));
        assert_eq!(cluster_by_launch_in(TABLE, &[3u8; 32]).map(|c| c.name), Some("other"));
        assert_eq!(cluster_by_hook_in(TABLE, &DEVNET.floor_hook).map(|c| c.name), Some("devnet"));
        assert!(cluster_by_hook_in(TABLE, &MAINNET.floor_hook).is_none(), "mainnet is not in the test table");
        // an id is looked up only in its own role
        assert!(cluster_by_amm_in(TABLE, &[2u8; 32]).is_none());
        assert!(cluster_by_hook_in(TABLE, &[1u8; 32]).is_none());
        assert!(cluster_by_launch_in(TABLE, &[9u8; 32]).is_none());
        assert!(cluster_by_hook(&[2u8; 32]).is_none(), "the synthetic cluster is not in the shipped table");
    }

    #[test]
    fn every_cluster_names_a_known_amm_and_no_program_id_repeats() {
        let mut seen: Vec<[u8; 32]> = Vec::new();
        for c in CLUSTERS {
            assert!(amm_cluster_by_program(&c.carpenter_amm).is_some(), "{}: carpenter_amm is not in AMM_CLUSTERS", c.name);
            for id in [c.carpenter_amm, c.floor_hook, c.floor_launch, c.floor_holders, c.floor_vesting] {
                assert!(!seen.contains(&id), "{}: a program id repeats", c.name);
                seen.push(id);
            }
        }
    }
}
