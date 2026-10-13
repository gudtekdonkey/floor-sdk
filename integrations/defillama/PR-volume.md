##### Name (to be shown on DefiLlama):
FLOOR

##### Twitter Link:
https://x.com/floordotsupply

##### Website Link:
https://floor.supply

##### Chain:
Ethereum, Base, BSC, Robinhood Chain, Arc (`dexs/floor-launchpad`); Solana (`dexs/floor-launchpad-solana`, through Dune)

##### Category:
Launchpad

##### Methodology:
Volume is every trade on a FLOOR launch, in its own quote asset (the chain's native asset or an approved ERC-20):

- **Sale buys**: `Bought.ethSpent` plus the FLOOR Platform fee `TeamFeeCharged` reports for that buy, so the figure is gross of the fee.
- **Sale sell-backs**: `SoldBack.ethReturned`, the seller's gross share.
- **Pool swaps**: the quote side of every Uniswap v4 `Swap` whose pool id is a FLOOR launch's (launches from FLOOR's on-chain registry, pool ids from the hook's `poolForAccount`). One side only.

On Solana the same three parts come from the programs' events on Dune: `floor_launch`'s `Bought` / `TeamBought` and `SoldBack` / `TeamBuySoldBack`, and the quote side of `carpenter_amm`'s `Swapped` on each FLOOR pool.

Excluded: swaps the hook sends itself during a ratchet. These are the protocol's own rebalancing, not user trades.

Marked `doublecounted` because graduated launches are Uniswap v4 pools.

##### Fees and revenue:
The same adapters report fees and revenue: the FLOOR Platform fee and FLOOR's 2.5% cut of the skim (protocol revenue), the rest of the skim and the holder fee (holders' revenue: the skim becomes the pool's backing), and the creator fee, its referral share and the LP fee (supply side). Each part is read from its own event; see the adapter's breakdown methodology.

##### Answers to the usual review questions:
- **Is a zero day correct?** Yes. A chain with no FLOOR trades in a window reports 0.
- **Why might the numbers differ from FLOOR's own pages?** They should not. On Sepolia the pool volume equals the site's indexer swap totals to the wei, hour by hour. FLOOR's tape also leaves out hook-sent swaps.
- **Did the fee rate ever change?** Each launch copies the FLOOR Platform fee setting when it is created and keeps it. Volume adds the fee back from the logged amount, so a rate change does not affect it.
- **Fee-to-volume ratio:** this PR adds volume only.
