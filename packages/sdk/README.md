# @floorlaunchpad/sdk

One API for FLOOR launches on every chain: chain-neutral types and `FloorClient`, which routes each call to the chain
adapter that serves the launch's chain. The adapters are separate packages:
[`@floorlaunchpad/sdk-evm`](https://github.com/gudtekdonkey/floor-sdk/tree/main/packages/sdk-evm) and
[`@floorlaunchpad/sdk-solana`](https://github.com/gudtekdonkey/floor-sdk/tree/main/packages/sdk-solana).

```sh
npm install @floorlaunchpad/sdk @floorlaunchpad/sdk-solana @solana/kit
```

```ts
import { FloorClient, solanaRef } from '@floorlaunchpad/sdk';

const floor = new FloorClient([solanaAdapter /*, evmAdapter */]);
const { launches, cursor } = await floor.listLaunches(solanaRef('devnet'), { limit: 20 });
const q = await floor.quote(launches[0]!.ref, 'buy', 1_000_000_000n);
const tx = await floor.buildTrade({ launch: launches[0]!.ref, side: 'buy', amountIn: 1_000_000_000n, minAmountOut: (q.amountOut * 99n) / 100n, trader });
```

- `Launch`: name, symbol, decimals, supply, the quote asset, the phase (`pending`, `bonding`, `graduated`), the mode, the
  creator and the published floor (`floorPriceX18`, whole quote per whole token × 1e18). `raw` carries the chain's own
  decoded accounts.
- `Quote`: `amountIn`, `amountOut`, the trader's whole take as `fees` (or the call throws), `feesExact`, an optional
  `refund`; `graduates` and `maxExtraNative` when the trade graduates a sale and may take native currency on top of
  `amountIn`.
- `UnsignedTx`: everything one action needs, in order, for one wallet prompt.
- Listings are unfiltered: anyone can create a launch with any name and symbol. Identify a launch by its address.

Amounts are bigints in base units; addresses are kept verbatim (a Solana address is case-sensitive).

## Licence

Apache-2.0.
