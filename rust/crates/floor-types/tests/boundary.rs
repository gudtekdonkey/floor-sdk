//! The FLOOR side of the crate boundary with the Carpenter AMM.
//!
//! The arrow is `floor-types → carpenter-types`, never back: FLOOR's mechanism is built over the AMM's
//! bytes, and the AMM side links no FLOOR crate (`carpenter-types`' `compile_fail` doctests and its own
//! `tests/boundary.rs`, in carpenter-sdk). This file checks the half that lives here: the manifest's
//! direction, and that the FLOOR names those doctests refuse are really defined in this crate — so the
//! doctests refuse something that exists, not a typo.

use std::fs;
use std::path::PathBuf;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn floor_types_depends_on_carpenter_types() {
    let manifest = fs::read_to_string(crate_dir().join("Cargo.toml")).unwrap();
    let deps: Vec<&str> = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("a [dependencies] table")
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .take_while(|l| !l.starts_with('['))
        .filter(|l| !l.is_empty())
        .collect();
    assert!(deps.iter().any(|l| l.starts_with("carpenter-types")), "floor-types must depend on carpenter-types: {deps:?}");
}

#[test]
fn the_floor_names_the_amm_side_refuses_are_defined_here() {
    let src = crate_dir().join("src");
    let read = |f: &str| fs::read_to_string(src.join(f)).unwrap();
    assert!(read("seeds.rs").contains("pub const FLOOR_POOL: &[u8]"));
    assert!(read("seeds.rs").contains("pub const LAUNCH: &[u8]"));
    assert!(read("layout.rs").contains("pub struct FloorPool"));
    assert!(read("layout.rs").contains("pub struct Launch "));
    assert!(read("constants.rs").contains("pub const SUPPLY:"));
    assert!(read("constants.rs").contains("pub const FLOOR_HOOK_FLAGS:"));
    assert!(read("ids.rs").contains("pub const FLOOR_HOOK_ID:"));
    assert!(read("disc.rs").contains("pub const FLOOR_POOL: [u8; 8]"));
}
