# DEX Screener adapter (spec v1.1)

One base URL per chain, serving `/latest-block`, `/asset`, `/pair` and `/events`.

| Field | Value |
|---|---|
| `priceNative` | the price each trade leaves on the book |
| `reserves.asset1` | `F + C + K`, the money held to buy back, plus Bonding's money before graduation |
| `reserves.asset0` | `0`: the tokens the program still sells are its inventory, not liquidity |
| `pairId` (EVM) | `pairIdOf(id)` = `keccak256(abi.encode(FloorAmm, id))` |
| events | one `swap` per trade (the Team Buy included), one asset in and one out; no join or exit events |
| `totalSupply` | the live supply (burns are real) |
