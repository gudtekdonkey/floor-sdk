# Third-party code and dependencies of the Rust crates

No third-party source is copied into `rust/crates/`. This file lists every crate the crates depend on
directly, with its pin, source and licence. `Cargo.lock` (and `crates/floor-quote-wasm/Cargo.lock`
for the wasm build) is committed: the versions it resolves are the ones the tests ran on.

Normal dependencies are `borsh`, `bytemuck` and (through `carpenter-clmm-math`) `uint` from
crates.io, the sibling crates here and the Carpenter crates. No Solana crate and no git source.

## Normal dependencies

| Crate | Pin | Locked | Source | Licence | Used by |
|---|---|---|---|---|---|
| `carpenter-types` | path `../../carpenter-sdk/crates/carpenter-types` (a tagged git dependency once published) | 0.1.0 | https://github.com/gudtekdonkey/carpenter-sdk | Apache-2.0 | `floor-types` |
| `carpenter-clmm-math` | path `../../carpenter-sdk/crates/carpenter-clmm-math` (likewise) | 0.1.0 | https://github.com/gudtekdonkey/carpenter-sdk | Apache-2.0 AND MIT (derived from Raydium CLMM, Apache-2.0, and Uniswap v4-core, MIT; see that crate's `NOTICE`, reproduced in this repository's `NOTICE`) | `floor-math` |
| `carpenter-quote` | path `../../carpenter-sdk/crates/carpenter-quote` (likewise) | 0.1.0 | https://github.com/gudtekdonkey/carpenter-sdk | Apache-2.0 | `floor-quote` |
| `borsh` | `1.5.7` (feature `derive`) | 1.8.1 | crates.io | MIT OR Apache-2.0 | `floor-types` |
| `bytemuck` | `1` (`floor-types`, `floor-quote`: features `derive`, `min_const_generics`; `floor-math`: no default features) | 1.25.2 | crates.io | Zlib OR Apache-2.0 OR MIT | `floor-types`, `floor-math`, `floor-quote` |
| `uint` | `=0.9.5` (through `carpenter-clmm-math`, not a direct dependency) | 0.9.5 | crates.io | MIT OR Apache-2.0 | `carpenter-clmm-math` |

The `bytemuck` requirement is `1` in every crate, as in the Carpenter crates; `Cargo.lock` pins the
version.

## The wasm build (`crates/floor-quote-wasm`, its own workspace)

| Crate | Pin | Locked | Source | Licence |
|---|---|---|---|---|
| `floor-quote` | path `../floor-quote` | 0.1.0 | this repository | Apache-2.0 |
| `wasm-bindgen` | `=0.2.128` (equal to the `wasm-bindgen-cli` the build script requires) | 0.2.128 | crates.io | MIT OR Apache-2.0 |
| `bytemuck` | `1` (dev only) | 1.25.2 | crates.io | Zlib OR Apache-2.0 OR MIT |

The shipped `packages/sdk-solana/wasm/floor_quote_bg.wasm` is compiled from these crates, the Rust
standard library (MIT OR Apache-2.0) and the crates `cargo tree --target wasm32-unknown-unknown`
lists from `crates/floor-quote-wasm/Cargo.lock`.

## Test-only dependencies

| Crate | Pin | Locked | Source | Licence | Used by |
|---|---|---|---|---|---|
| `serde_json` | `1` | 1.0.154 | crates.io | MIT OR Apache-2.0 | `floor-quote` tests (golden vectors) |

The transitive closure (the proc-macro crates behind the `borsh` / `bytemuck` derives, `crunchy`,
`byteorder`, `hex` and `static_assertions` under `uint`) is in the lock files; a full licence audit
of it is a `cargo deny check licenses` run, which this file does not replace.
