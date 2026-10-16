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
- **Pool swaps**: the quote side of every Uniswap v4 `Swap` whose pool id is a FLOOR launch's (launches from FLOOR's on-chain registry, pool ids from the hook's `poolForAccount`; the PoolManager's logs are asked only for those pool ids), plus on a buy the `HookFee` the hook took from the quote before the swap (the Uniswap Foundation hook data standard), so a buy is its whole size. One side only.

On Solana the same three parts come from the programs' events on Dune: `floor_launch`'s `Bought` / `TeamBought` and `SoldBack` / `TeamBuySoldBack`, and the quote side of `carpenter_amm`'s `Swapped` on each FLOOR pool.

Excluded: swaps the hook sends itself during a ratchet. These are the protocol's own rebalancing, not user trades.

Marked `doublecounted` because graduated launches are Uniswap v4 pools.

##### Fees and revenue:
The same adapters report fees and revenue, each part read from its own event (see the adapter's breakdown methodology):
- Protocol revenue: the FLOOR Platform fee, and FLOOR's 2.5% cut (`PlatformCut`) of everything that enters a pool's backing: the skim, the raise at graduation, the staircase's sales and the swept LP fees paid in the quote.
- Holders' revenue: the holder fee and the rest of the skim; the LP fee paid in the quote on a buy, which becomes the pool's backing; and the LP fee paid in the launch token on a sell, which joins the pool vault's inventory (not backing), stated in the quote at the swap's own rate. The LP fee is the hook's because the hook owns the pool's positions; the hook does not gate adding liquidity, so a position added by anyone else would earn a share the adapter cannot separate (none is known).
- Supply side: the creator fee and its referral share.

##### Answers to the usual review questions:
- **Is a zero day correct?** Yes. A chain with no FLOOR trades in a window reports 0.
- **Why might the numbers differ from FLOOR's own pages?** They should not. Checked on 2026-10-13 against each pool's own lifetime counters in the hook's `poolState`, over every hour since the launches: the pool volume equals `cumulativeVolumeEth`, protocol revenue equals `teamFeesOwed + teamFeesPaid + platformPaidEth`, and the creator and referral fees equal their owed + paid counters, to the wei on all five EVM chains. FLOOR's tape also leaves out hook-sent swaps.
- **Did the fee rate ever change?** Each launch copies the FLOOR Platform fee setting when it is created and keeps it. Volume adds the fee back from the logged amount, so a rate change does not affect it.
- **Fee-to-volume ratio:** high while a pool is young (the skim is up to 5% far from the floor and holder fees up to 5%), and it falls as the price nears the floor.

##### Everything else an integrator may need
Every chain, contract address, live pool, sample transaction, event and endpoint, and how these adapters were checked against the chain: https://github.com/gudtekdonkey/floor-sdk/blob/main/docs/integration-facts.md
