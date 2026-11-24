# KyberSwap: the FloorAmm liquidity source

`floor-amm-v2.patch` adds the FloorAmm liquidity source to [kyberswap-dex-lib](https://github.com/KyberNetwork/kyberswap-dex-lib)
as `pkg/liquidity-source/floor-amm`: pool discovery and state tracking from the contract's views and logs, and a
simulator that walks each launch's book with a Go port of the FLOOR Program's reference math (`math/v2`), tested
against the protocol's golden book walks (`math/v2/testdata`, copied verbatim).

It applies to upstream commit `62d6685c`.

```
git clone https://github.com/KyberNetwork/kyberswap-dex-lib && cd kyberswap-dex-lib
git checkout 62d6685c
git am /path/to/floor-amm-v2.patch
go vet ./pkg/liquidity-source/floor-amm/...
go test -count=1 ./pkg/liquidity-source/floor-amm/...
```
