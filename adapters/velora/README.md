# Velora: the FloorAmm dex

`floor-amm-v2.patch` adds the FloorAmm dex to [paraswap-dex-lib](https://github.com/VeloraDEX/paraswap-dex-lib) as `src/dex/floor-amm`:
pools are launches read from the contract's `launchBook` view, prices are the program's own book walk (`math/`, a port
of the protocol's reference math, tested bit for bit against its golden book walks in `testdata/`), exact-in both ways.

It applies to upstream commit `2c4307582ccf69fe96fafbe4cc5a34b88adf130d`.

```
git clone https://github.com/VeloraDEX/paraswap-dex-lib && cd paraswap-dex-lib
git checkout 2c4307582ccf69fe96fafbe4cc5a34b88adf130d
git am /path/to/floor-amm-v2.patch
yarn install
yarn test src/dex/floor-amm
```
