# FLOOR Program on Solana: shared code and the Jupiter `Amm`

- `floor-math`: the program's math (the one implementation of the trade).
- `solana-core`: keys, the `LaunchV2` account layout, the transfer-hook accounts, the quote mint, a trade's account metas,
  an allocation-free quote and the `LaunchCreatedV2` event.
- `jupiter`: the `jupiter-amm-interface` `Amm` for one launch.

`cargo test` runs the tests on the frozen fixtures in `jupiter/fixtures`. Licence: Apache-2.0.
