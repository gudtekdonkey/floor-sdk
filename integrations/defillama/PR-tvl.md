##### Name (to be shown on DefiLlama):
FLOOR

##### Twitter Link:
https://x.com/floordotsupply

##### List of audit links if any:
TODO

##### Website Link:
https://floor.supply

##### Logo (High resolution, will be shown with rounded borders):
TODO: the F mark, Floor Orange on black

##### Current TVL:
TODO at submission: the adapter's own output

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
For every launch in FLOOR's on-chain registry, only launches quoted in the chain's native asset or a quote asset FLOOR lists:
- EVM: while the launch is still selling, the quote asset raised and held by its sale contract (`raised()`); once it has graduated, its pool's backing in the quote asset (`backingEth()`: the vault plus the deployed bid, read through the hook's `execute`).
- Solana: while selling, the SOL in the launch's sale vault and its quote vault; once graduated, the FloorPool account's `vault_eth + bid_eth_deployed`.

Launch tokens and the platform's 2.5% cut are not counted. Marked `doublecounted` because a graduated EVM launch's bid is a Uniswap v4 position.

##### Github org/user (Optional, if your code is open source, we can track activity):

##### Does this project have a referral program?
Yes. A launch can name up to three referrers, paid from the creator's fee.
