# Drawing the floor line

A FLOOR launch has a floor: a price in the quote token that the program's own money stands behind. The bare minimum we ask for is two things: the FLOOR line on the token's price chart, and the floor price somewhere on the token's page. Anything else is a bonus. It is a request, not a requirement, and it is highly appreciated. It costs one extra field and one extra line on the chart.

[examples/floor-line.html](../examples/floor-line.html) is a working page: a price line, the floor line and the code that fetches and draws it. Open it in a browser.

## What the floor is

The program keeps money aside to buy tokens back at a fixed price. That price is the floor: the money in the floor pot divided by the tokens in circulation. A holder is backed by the floor the program's money holds. The floor is not a promise about the market price, and the market price can sit far above it.

Three facts shape the line:

- The floor is a price in the launch's quote token (USDC, SOL, ETH or another listed quote), per whole token.
- It never falls in quote terms. Buys can lift it and sells do not lower it.
- It starts when the launch graduates, which is when its flat-priced opening sale sells out. Before then the launch has no floor, and the field reads `0`.

## Which field to read

**Partner API.** `GET /v2/dex/summary?id=<pair>` returns `floorPriceNative`, a decimal string in whole quote tokens per whole launch token, next to `priceNative` in the same units. The SDK decodes it as `DEX_SUMMARY` and builds the path as `dexSummary`. Read both fields from the same response so the two lines come from the same moment.

**SDK, from the indexer.** `readLaunch(get, id)` in `@floorlaunchpad/sdk/indexer/read` returns the launch with `floorX64`. That is a Q64.64 price in quote base units per token math unit. One math unit is a millionth of a token on both chains. To get whole quote per whole token:

```ts
const floor = Number((floorX64 * 10n ** 6n * 10n ** 12n) / (1n << 64n)) / 10 ** (quoteDecimals + 12);
```

**SDK, from the chain.** The math reads the same number from a decoded book: `priceX64(book.sqrtF)` (`@floorlaunchpad/sdk/math/v2/sqrt`). `sqrtF` is 0 until the launch has a floor. The example page's numbers come from this math (see the note at the end).

## How to draw it

Draw a horizontal line at the floor value, under the price line, in FLOOR orange `#FF6A1A`, and label it "FLOOR". The line is a step line: it holds its value between trades and moves up at a trade that lifts it. If your chart already has a price line, use the same y axis and the same time axis.

```js
const s = await (await fetch(`${API}/v2/dex/summary?id=${pair}`)).json();
const price = Number(s.priceNative);
const floor = Number(s.floorPriceNative);
const showFloor = floor > 0 && Number(s.circulatingSupply) > 0;
series.price.update({ time, value: price });
if (showFloor) series.floor.update({ time, value: floor });
```

## Pitfalls

**Units.** The floor is a price in the quote token. In quote terms it never falls. If you chart in USD, the line moves with the quote token's own USD price. A floor in a falling quote token then falls in dollars, though it has not fallen in the quote. Chart in quote units where you can, and say which units the chart uses where you cannot.

**Read it, do not derive it.** Do not compute the floor from the money in the program and the total supply. The supply the program uses is the live supply, and it changes with every burn, including burns that happen with no trade. Take `floorPriceNative` as the program reports it.

**The gap.** The floor divided by the price can fall as the price rises. This happens once the price is above the end of the launch's sales curve. It also happens in the Tail, the open-ended stretch after the curve ends. The price keeps rising in both, while the floor rises more slowly. The floor itself does not drop. A widening gap between the two lines is how the design works, not a bug in your chart. Do not label it as a loss of backing.

**Lifecycle.** Keep the line after graduation and in the Tail. A graduated launch stays on the same program, trades there as before and keeps its floor. Do not stop drawing the floor when the launch graduates, and do not look for it on another venue.

**All tokens burned.** If every token in circulation has been burned, nothing is left to back and the program sends its backing money to the platform. There is no floor to show. Hide the line. Do not plot 0, and do not divide by a zero supply to work one out. A zero `circulatingSupply` in the summary is the signal.

**Before graduation.** A launch that has not graduated has no floor yet and the field reads `0`. Hide the line until it is above zero.

**Caching.** Refresh the floor together with the price. If you cache the price for ten seconds, cache the floor for ten seconds with it, and never keep the floor longer than the price. A floor older than the price can look as if it has fallen behind.

**Words.** Never write that the floor is "guaranteed", and never call it a "floor market cap". Write "backed by the floor the program's money holds".

## The example page

`examples/floor-line.html` draws a simulated launch: a run of buys and sells through the SDK's own math, not a live pool. The source repository holds the small script that makes its data (`examples/gen-floor-line.mjs`), so the page can be rebuilt after a change to the math. The npm package ships the page and not the script.

## Optional extras

None of these is asked for. If you want to go further: show the gap between price and floor as a percentage, add the floor to a tooltip or a table of pools, or link a holder to this guide's explanation of what backs the token.
