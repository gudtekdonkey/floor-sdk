##### Name (to be shown on DefiLlama):
FLOOR

##### Twitter Link:
https://x.com/floordotsupply

##### List of audit links if any:


##### Website Link:
https://floor.supply

##### Logo (High resolution, will be shown with rounded borders):
https://floor.supply/icons/icon-512.png

##### Current TVL:
About $2.0k on 2026-10-13 (Ethereum $1.13k, BSC $389, Robinhood Chain $209, Arc $183, Solana $52, Base $47), the adapter's own output

##### Treasury Addresses (if the protocol has treasury)
Not counted in TVL. The same address on Ethereum, Base, BSC, Robinhood Chain and Arc:
- Platform treasury: `0x6134E81DB1C853354E5D6B0cd70C5d04a4A6508c`
- Team treasury: `0x062B6c421E37579728877337B47Cf528AE4d49bb`

##### Chain:
Ethereum, Base, BSC, Robinhood Chain, Arc, Solana

##### Coingecko ID (so your TVL can appear on Coingecko, leave empty if not listed):

##### Coinmarketcap ID (so your TVL can appear on Coinmarketcap, leave empty if not listed):

##### Short Description (to be shown on DefiLlama):
A token launchpad where every launch's raise becomes a protocol-owned bid under its price: a Uniswap v4 hook holds the raise as the pool's floor, and the floor only moves up.

##### Token address and ticker if any:
$FLOOR (Solana): `5qhVgzoXcy8LWzwfZAWJWNEwZCVqfeJFK6MLhAWHUB1D`

##### Category (full list at https://defillama.com/categories) *Please choose only one:
Launchpad

##### Oracle Provider(s): Specify the oracle(s) used (e.g., Chainlink, Band, API3, TWAP, etc.):
None. The floor is set by the pool's own vault, not a price feed.

##### Implementation Details: Briefly describe how the oracle is integrated into your project:
N/A

##### Documentation/Proof: Provide links to documentation or any other resources that verify the oracle's usage:
https://floor.supply/technical

##### forkedFrom (Does your project originate from another project):
None. Graduated launches trade on Uniswap v4 pools through FLOOR's own hook.

##### methodology (what is being counted as tvl, how is tvl being calculated):
For every launch in FLOOR's on-chain registry, only launches quoted in the chain's native asset or a quote asset FLOOR lists, the quote asset FLOOR holds:
- EVM: while the launch is still selling, its sale contract's quote balance (the raise). Once it has graduated: the quote in its Uniswap v4 pool's liquidity (every initialized range at the current price, read from Uniswap's StateView: the bid, the protocol-owned liquidity, the cushion and any staircase step the price is in), plus its custody vault's quote balance (the rest of the backing and every fee owed but not yet claimed).
- Solana: while selling, the SOL in the launch's sale vault and its quote vault. Once graduated, the Carpenter pool's quote vault (every position's quote and the hook's claims), less Carpenter's own protocol fees owed in it.

Launch tokens and quote already paid out of the protocol (to the platform treasury, creators, referrers or holders) are not counted. Marked `doublecounted` because a graduated launch's liquidity sits in a Uniswap v4 pool (EVM) or a Carpenter pool (Solana), which those protocols count too.

Checked against the chain on 2026-10-13: on each EVM chain the quote in the pool equals the hook's seed + bid + cushion positions to the wei, and the custody vault's balance equals `vaultEth` plus every owed fee in the hook's `poolState`.

##### Github org/user (Optional, if your code is open source, we can track activity):
gudtekdonkey

##### Does this project have a referral program?
Yes, three tiers: a launch creator's referrers share 10% of the creator fee, and a trader's referrers earn part of the FLOOR Platform fee that trader's trades pay.

##### Everything else an integrator may need
Every chain, contract address, live pool, sample transaction, event and endpoint, and how this adapter was checked against the chain: https://github.com/gudtekdonkey/floor-sdk/blob/main/docs/integration-facts.md
