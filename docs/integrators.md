# FLOOR for integrators

One page for routers, data providers, explorers and wallets. The partner notes link to the section that fits them.
Chain-by-chain addresses and the latest numbers live in the docs at <https://floor.supply/developers>.

**What FLOOR is.** Tokens trade directly against one program per chain, with no pool and no migration; the program's
own money buys back every sell, holding a price floor that rises as the token is bought and never falls.

**How it works.** Buys walk up a fixed price curve and take tokens from the program. Sells are bought back by the
program's own money. Part of every buy is added to the floor, which never falls.

## Words used on this page

- **Launch:** one token and its market. The pair is `[launch token, quote]`, for life.
- **Floor (`F`):** the money the program holds to bid for the token at one flat price, the floor price.
- **Cushion (`C`, with `K` below it):** a second pot of buy-back money laid under the price.
- **Bonding:** the first phase, a flat opening price. **Graduation:** the first window sells out and the launch moves up the price curve on the same program. A `Graduated` event fires and nothing migrates.
- **Fees:** charged on top of every trade, each may be 0: the Backing Fee, Team Fee, Platform fee, `$FLOOR` fee and Holder Fee.
- **Marketcap Milestones:** not fees. They are a share of a buy's money, paid inside the price, so they are already in the amount out.

## Rules that hold for every integration

- Exact-in only. There is no exact-out entry point.
- A buy the program cannot finish (sold out, or past the fixed step budget) fills what it reached and refunds the rest in the same call. A sell fills what the bid takes and returns the rest of the tokens. Price such an amount as unavailable, so a route never carries a refund.
- The step budget is 16 price pieces for a Solana buy, 24 for an EVM buy and 24 for any sell. The SDK quotes with these caps, so a quote equals the fill.
- Buys and sells cannot be paused. Only new launches can be.
- No pool and no outside LPs: the program's own money is the liquidity. Liquidity is `F + C + K`, the money held to buy back. Before graduation the money Bonding has raised counts too. The program's unsold inventory is not liquidity.
- Fees go on top of the trade, in the quote. The Platform fee and `$FLOOR` fee are 0.40 % and 0.10 % of the trade before graduation; the other fees start at graduation.
- A quote must be on the chain's quote registry. A launch with another quote is skipped unless you choose to route it.
- One market per launch. A launch token can itself be the quote of another launch.

## Solana routing

- Program `floor_amm`: `GUhcaRccPyjdXtXmRWWopvsERfTEweqSS1Sa1FsFLooR`. Transfer hook `floor_hook`: `HVo1eRwj5SHtjkmHa4cVSV5KeL2BenN2d5qM2bqFLooR`.
- A market is the `LaunchV2` account, PDA `["launch", base_mint]`. List them with `getProgramAccounts` (data size 6,024, discriminator `cd0baaac701fd1dd`).
- A quoter watches three accounts: the launch, its base mint (the live supply: every trade first lowers the stored supply to it) and its quote mint (whether the quote is a launch token, and any transfer fee).
- `buy(amount, min_tokens_out)` and `sell(tokens, min_quote_out)`: one signer, one instruction, 25 account metas for a plain swap (31 when the quote is itself a FLOOR launch token, 32 with a referral record). Chained routers use `buy_from_balance(keep, min_tokens_out)` and `sell_from_balance(keep, min_quote_out)`.
- The program reads no caller allow-list: top-level or CPI works alike.
- Launch tokens are Token-2022, 6 decimals, mint authority removed at create, no freeze authority, one extension (TransferHook). The six hook accounts are derived from the mint and the two wallets.
- Compute: the heaviest swap measured 266,657 CU; the `floor-amm-jupiter` crate sets a 900,000 CU limit.
- Code: the `Amm` crate `floor-amm-jupiter`, with `floor-amm-solana` and `floor-math`, in `adapters/solana`. Account list and instruction layout: [examples/solana-swap.md](../examples/solana-swap.md).

## EVM routing

- Contract `FloorAmm`, a UUPS proxy at the same CREATE2 address on every chain. A pool is a launch id; the pair is `(quote, token)`.
- Discovery: `launchCount()`, then `launchToken(id)` and `launchQuote(id)`. New pairs arrive as `LaunchCreated` events.
- Quote off chain from `launchBook(id)` and `launchFeeCurve(id)` read at one block with the SDK math (it matches the golden vectors bit for bit). Re-read after any `Trade`, `Graduated` or burn. On chain, `quoteBuy` and `quoteSell` by `eth_call` run the same code path as the trade.
- Pay-first execution (no approval): transfer the input to the contract, then `buyFromBalance` or `sellFromBalance`. Native input: `buy` with `value`. Native coin is `address(0)`, never wrapped.
- Tokens: ERC-20, 18 decimals, no admin key, no mint, no pause, no fee on transfer. A sell is in whole math units (10⁻⁶ token).
- Gas, largest plain trade measured on the v2 contract, rounded up: buy ≈ 700,000, sell ≈ 750,000. A long walk costs more. To be re-confirmed on the deployed contract.
- Selectors, events and the call shapes: [examples/evm-pay-first.md](../examples/evm-pay-first.md). A quote-API shape: [examples/quote-api.md](../examples/quote-api.md).

## Data, listing and indexing

- Events: one trade event per buy or sell (the Team Buy included), one create event per launch, one `Graduated` event once. Nobody adds or removes liquidity, so there are no join or exit events.
- Price: the price each trade leaves on the book. Market cap is the price the next buy pays times the live supply.
- Volume: the quote side of every buy and sell, before fees.
- Liquidity: the money the program holds to buy back, `F + C + K` (`bid_money` on Solana, `bidMoney` on EVM), plus Bonding's money before graduation. The `/api/dex` adapter serves this sum.
- Supply: burns are real. Total supply is the live supply.
- Graduation is a state change on the same program. The pair, the address and the pool id do not change.
- DEX Screener adapter (spec v1.1), one base URL per chain. On EVM its pair id is `pairIdOf(id)`, `keccak256(abi.encode(FloorAmm, id))`, not the launch id: [examples/dex-adapter.md](../examples/dex-adapter.md).
- Data API: `/api/v1` with an OpenAPI file. Fees and revenue: the five fees are in each trade event; Milestones are not fees and are not counted as fees.

## Security, wallets and explorers

- **Solana launch token:** Token-2022, 6 decimals, 1,000,000,000 minted to the program, mint authority removed in the same instruction, no freeze authority. The one extension, TransferHook, names `floor_hook`. The hook takes no fee, moves no tokens and has no blocklist, pause or per-wallet limit. It keeps the record the Holder Fee is shared by.
- **EVM launch token:** an EIP-1167 clone of one verified implementation. ERC-20, 18 decimals, 1,000,000,000 minted to the contract, no admin key, no mint function, no pause, no fee on transfer.
- **Largest holder:** the launch's own token vault, held by the program. It holds tokens not sold yet and is not a team wallet.
- **No LP tokens** to lock or burn. The liquidity is the money the program holds, and no wallet can withdraw it.
- **Fees** are charged by the program on buys and sells, not by the token on a transfer.
- **Metadata:** name, symbol and uri are set at create. The uri is `<metadata base>/<mint>.json`, a JSON file that holds the image.
- **EVM source verification:** solc 0.8.28, EVM version cancun, via-IR, optimizer 1 run, no metadata hash.

## Trading bots and terminals

- Everything needed is public: the program ids and addresses, the SDK (quotes and instruction builders), and the event layouts. The routing sections above apply as written.
- Bots trade against the program directly, not against a pool, so a venue entry needs a quote function and an instruction builder, both in the SDK.
- Referrals: pass the program id in the referral slots when you have no referral record. The Platform fee is then paid whole.

## Links

- Docs: <https://floor.supply/developers>; contracts: <https://floor.supply/developers/contracts>; SDK: <https://floor.supply/developers/sdk>
