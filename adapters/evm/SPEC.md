# FloorAmm for EVM aggregators — the integration spec

One spec for every EVM aggregator (KyberSwap, Velora, 1inch, LI.FI). The interface is `IFloorAmm.sol`. The reference
implementation is `src/protocol.ts`, `src/v2.ts` and `src/v2pool.ts`, on the SDK's v2 math.

## 1. What FloorAmm is

- ONE contract per chain: a proxy at the same CREATE2 address on every chain. Every launch trades against it from its
  first block. There is no pool contract; a "pool" is a launch id (`uint256`, from 1).
- A launch is a pair (quote, token).
  - The quote is an asset on the quote registry (`quoteEntry`) or any ERC-20 (see unlisted quotes, section 2); native ETH is
    `address(0)`.
  - The token is an ERC-20 with 18 decimals and 1B minted to the program at create. It has no admin key, no mint, no
    pause and no fee on transfer.
- One book per launch holds the market. Buys are filled in this order:
  1. the Bonding window, flat at `p₀` (a last remainder no offer can buy joins the book and the launch graduates in that
     buy);
  2. the tokens the bid bought back, cheapest first, each at the price it was bought, before any new issuance;
  3. the issuance ranges, up the curve, then the Tail range: a constant-product range from the schedule's end to an
     infinite price (it never runs out, so a buy past the schedule is not refunded for want of tokens).
  Read the price from the book's `sqrtP`.
- Sells are filled by the program's bid: the Cushion's segments (C, then K) down to the floor, then the floor `F`, a flat
  bid at `f`. Every holder is backed by the floor the program's money holds. A buy splits its money in the same call (the
  floor first, then the Cushion, then the Marketcap Milestones, paid at once) and the Cushion is re-laid after every trade.
- Buys and sells can NEVER be paused. Only new launches can be paused, so an aggregator needs no "paused" check.

## 2. Discovery

- `launchCount() → n`. `launchToken(id)` and `launchQuote(id)` for `id = 1…n` name the pair; `launchBook(id)` is the launch's book (the pots, the
  price ranges, the fee legs, `graduated`), read at one block.
- New launches emit `LaunchCreated(uint256 indexed launchId, address indexed token, address indexed quote, address creator,
  uint8 presetId, uint32 presetVersion, bytes32 presetHash, uint256 p0X64, uint256 raise, uint256 windowTokens,
  uint256 creatorBuyTokens, uint256 createFee, string name, string symbol, string uri, bool listed,
  uint256 quoteUsdCentsX64, uint256 nativeCreateFee)` (topic `0xe484fc74438ba25825d3967d814e27f10e8dab75644fcc23a8135715ecb7c75e`).
- `launchOf(token) → id`; 0 means not a FloorAmm token.
- **Unlisted quotes.** A launch's quote may be any ERC-20, not only one on FLOOR's quote registry. `LaunchCreated.listed`
  and `quoteUsdCentsX64` say whether the quote was on the registry at create;
  `quoteEntry(quote).listed` says whether it is now. FLOOR shows a launch on an unlisted quote with a warning. The
  references do NOT route such a launch unless the integrator opts in (`routeUnlisted`): nothing has vetted the quote
  token's transfer behaviour (fees on transfer, rebasing, blocklists). A launch is routed from the moment its quote is
  on the registry, at create or later. Trading itself is the same for every launch.

## 3. Quoting

There are two ways to quote, and they give the same numbers:

1. **On chain:** `quoteBuy(id, amountIn, to) → BuyResult (tokensOut, spent, refund, priceAfterX64, graduatedNow,
   endedNow, budgetHit)` and `quoteSell(id, tokensIn, to) → quoteOut`. `to` is the beneficiary: its trader referrers
   apply. A router hop has none; pass the zero address. (`endedNow` is always false on this contract.)
2. **Off chain, from state:** the protocol's reference walk (the SDK's v2 book math, held to its published golden
   vectors) on the state read in section 4: `amountOutV2` in `src/v2pool.ts`.

Rules every quote must keep:
- Exact-in only. There is no exact-out entry point.
- **Fill and refund.** A buy the book cannot fully fill (for example one whose compute budget runs out, `budgetHit`)
  fills what it can and REFUNDS the rest in the same call. An aggregator should price such an amount as 0 (the references
  do), so a route never carries a refund. There is no per-block or per-call cap on a buy.
- Fees are ON TOP of every buy and sell, on the trade's quote leg, each rounded up, five legs in this order: the Backing
  Fee (into the floor), the Team Fee, the Platform fee, the `$FLOOR` fee and the Holder Fee. They are already in every
  quote's amount out. Before graduation (the Bonding window) only the Platform fee (0.40 %) and the `$FLOOR` fee (0.10 %)
  apply, flat; from graduation each leg is a curve on market cap from the launch's fee curve (`launchFeeCurve(id)`, copied
  from its preset row at create), with `launchBook.feePpm` the curve's end rate. A launch keeps the row it was created with.
- From graduation a buy on its way up also carries the Marketcap Milestones (Team, Platform, `$FLOOR`, Holder): inside
  the price, already in the amount out, and not fees. Bonding buys and sells never carry them.
- A sell is in whole math units (section 4): `sell` of any other amount reverts `BadArgument`; `sellFromBalance` sells the whole
  units and hands the rest back to `to`. A part the bid cannot take is handed back to the seller (the references price
  such an amount as 0). If the quote's transfer to the seller fails (a quote that blacklists or pauses), the sell still
  settles and the payout is owed to the seller (`PayoutOwed`, claimable later), so a route's sell never reverts on the
  payout.

## 4. State (for off-chain pricing)

At one block, read:
- `launchBook(id)`: the book: `√P`, the floor `F` (`f`, its tokens and the Cushion money parked at `f`), the Cushion's
  segments (money, tokens, edges, the floor's share in each; segments `0 … nC` are C, the rest K), the highest price
  issuance reached, the price ranges (each with its money in place), `graduated` and the legs' end rates (`feePpm`);
- `launchFeeCurve(id)`: the launch's fee curves on market cap, as copied at create;
- `circulatingOf(id) → (sCirc, sBelow, dead)` and `bidMoney(id)` (F + C + K); `launchMilestones(id)`: its Marketcap
  Milestones; `teamSlotOf(id)`: the Team Slot per place;
- `pairIdOf(id)`: the id partners list the launch under, `keccak256(abi.encode(FloorAmm, id))`.

Units. A launch's book runs in MATH units of `k = 10¹²` token base units (18-decimal tokens, 6-decimal math units), on
every launch whatever its quote's decimals. Quote amounts never scale. The book views are in MATH units; every event's
token field is in CHAIN units (math × k); prices are per CHAIN base unit (math ÷ k, rounded down). A simulator restores the
math state from `launchBook` and trades in chain units: a buy's tokens are a multiple of `k`, a sell takes
`⌊tokensIn ÷ k⌋ × k`.

`bookOf` and `amountOutV2` in `src/v2pool.ts` map all of this to the SDK's v2 book walk. The reference reads the book at
a block; it does not replay logs.

The logs, for an indexer:
- `Trade(uint256 indexed launchId, address indexed trader, address indexed recipient, bool isBuy, uint256 quoteGross,
  uint256 quoteNet, uint256 refund, uint256 tokens, uint256[5] fees, uint256 traderRef, uint256 fromResale,
  uint256 fromFirst, uint256 unfilled, uint128 sqrtBefore, uint128 sqrtAfter, uint128 sqrtF, uint256 floor,
  uint256 cushion, uint256 sCirc, uint8 steps, bool budgetHit, uint64 slot, uint64 ts, uint8 kind, uint256 creatorRef,
  uint256 bidMoney, uint256[4] msPaid)`, topic
  `0xf13d84552bb2de89213afd3ee8de6d2f901a8ef3c46baecd3ba56d079ec5bbee`, is emitted on EVERY buy and sell. `fees` is the
  Backing Fee, the Team Fee, the Platform fee, the `$FLOOR` fee and the Holder Fee, in that order. `cushion` is C + K's
  money, `bidMoney` is F + C + K after the trade, and `msPaid` the buy's Marketcap Milestones, paid at once (inside the
  price, not fees; 0 on a sell). `kind` is 0 for a trade, 1 for the create's Team Buy, 2 for a Team Share buy. Check the
  book you read against `sqrtAfter`, `sqrtF`, `floor`, `cushion`, `bidMoney` and `sCirc`; on a mismatch, re-read the book.
- `DeadSynced(uint256 indexed launchId, uint256 added, uint256 deadTokens, uint256 sCirc)`: tokens sent to `0x…dEaD`
  were counted out of the circulating supply. The token's `Transfer(from, to, value)` to `address(0)` is a burn.
  Burns count.
- Also emitted: `PotsSwept` and `Graduated(launchId, windowPriceX64, priceX64, windowRaised, windowTokens)` (the Bonding
  window sold out).
- `creditBacking(id, amount)` (a donation to the floor) raises `f` and emits no event of its own, so a book kept by logs
  can miss it: re-read `launchBook` (or `bidMoney(id)`) at the block you quote before routing a sell.

## 5. Execution

| Who | Buy (quote → token) | Sell (token → quote) |
|---|---|---|
| A router / executor (Velora, 1inch, KyberSwap) | transfer `amountIn` to the proxy, then `buyFromBalance(id, minOut, to, refundTo, deadline)`; ETH: `buy(id, amountIn, minOut, to, deadline)` with `value = amountIn` | transfer `tokensIn` to the proxy, then `sellFromBalance(id, minOut, to, deadline)` |
| A wallet (LI.FI's tx) | approve the proxy, `buy(id, amountIn, minOut, to, deadline)`; ETH: `value = amountIn` | approve the proxy, `sell(id, tokensIn, minOut, to, deadline)` |

- `buy*` returns a `BuyResult` and `sell*` returns `quoteOut`. The amount out is the first return word
  (`returnAmountPos 0`).
- `*FromBalance` spends everything above what the program books of that asset, so send and call in ONE transaction. Its
  refund (none, when priced as section 3 says) goes to `refundTo`; `buy`'s refund goes to `msg.sender`.
- A buy reverts `BadValue` when `msg.value ≠ amountIn` (ETH) or `msg.value ≠ 0` (ERC-20). Under `minOut` it reverts
  `SlippageExceeded`; past `deadline`, `Expired`. The references pass no deadline (`2^256 − 1`) for a router hop.
- Gas per hop: buy ~700k, sell ~750k (`GAS` in `src/protocol.ts`): FloorAmmV2's plain buy / sell measured at most 693,651
  / 740,775 over every launch state (presets 1 and 3), rounded up; a long walk costs more.

## 6. The token list

`src/tokenlist.ts` builds a list in the open token-list schema:
- one entry per launch token, with 18 decimals, the tag `floor-amm`, and `extensions: { launchId, quote, floorAmm }`;
- each token's own `name()` / `symbol()`;
- entries the schema would refuse are skipped and reported;
- a launch whose quote was not on the registry at create (`LaunchCreated.listed` false, read from the launch's own log) is never an entry and is not reported. No option lists it, and listing its quote later does not either. Routing
  such a launch (`routeUnlisted`) is not listing it.

A launch token may also be the QUOTE of other launches, so a pair may be token ↔ token.

## 7. Chains

Ethereum, Base, BNB Chain, Arbitrum, Arc, Robinhood Chain and HyperEVM, at one address on every chain, all on day one.
