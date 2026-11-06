# Show the FLOOR on your UI

**If you list, chart, route or hold FLOOR tokens, please show the FLOOR.** Every FLOOR launch has a floor: a price in
its quote token that the program's own money stands behind, and that never falls. It is the single most useful number
a holder of a FLOOR token can see, and right now it is only visible on floor.supply. With your help it can be visible
wherever the token trades.

It is a request, not a requirement, and it is highly appreciated. It costs one field and one line.

[examples/floor-line.html](../examples/floor-line.html) shows the finished result: a token page with the FLOOR price next to
the price and the FLOOR line on the chart, a price / market-cap switch, a replay of a launch with a sell-off, and every
snippet on this page. Open it in a browser. It needs no network and no key.

## What we ask

**The minimum: two things.**

1. **The FLOOR line on the token's price chart.** One line, FLOOR orange `#FF6A1A`, 2 px, labelled `FLOOR`, under the
   price.
2. **The FLOOR price on the token's page,** next to the price. For example: `FLOOR $0.0189`.

**Anything more is a bonus.** In rough order of how much it helps:

- How far the price is above the floor, as `% backed` (floor ÷ price) or `price is 1.33× the floor`.
- A FLOOR column in token lists and screeners.
- The FLOOR price in a wallet's holding row, next to the holding's value.
- An alert when the price comes near the floor.
- A "FLOOR" badge on launches that trade on the FLOOR program.

| You are | Show |
|---|---|
| A chart site or screener (DexScreener-style) | The line on the chart, the floor price in the pair's stats |
| A terminal or trading bot UI | The line on the chart, the floor price next to the buy and sell box |
| An aggregator or swap UI with a token page | The floor price on the token page; the line if you draw a chart |
| A wallet | The floor price on the token's detail screen |
| A data API that others build on | The floor as a field on the pair (and on each trade, if you serve trades) |

## Get the number

There are three ways. Use the first one you can.

### 1. The FLOOR Partner API (simplest)

Base URL `https://floor.supply/api/v1`. Server to server, with a key sent as `Authorization: Bearer <key>` (a key in the
URL is refused). Ask for a key at contact@floor.supply or on X. The reference is
<https://floor.supply/developers> and the OpenAPI document is `https://floor.supply/api/v1/openapi.json`. Every answer is
`{ data, meta }`.

| Read | Field | Gives |
|---|---|---|
| `GET /launches/{chain}/{token}` | `data.market.floorPriceQuote` | The floor now, whole quote per whole token |
| | `data.market.floorPriceUsd` | The same in USD at the live rate |
| | `data.market.priceQuote`, `data.market.backedPct` | The price now, and floor ÷ price as a percentage |
| `GET /launches/{chain}/{token}/trades` | `data[].floorPriceQuote` | The floor after each trade: the history of the line |

`null` means there is no floor yet (the launch is still in Bonding) or the figure was not read. Never draw a `null` as 0.

```js
// server side: the FLOOR price for the token page
const r = await fetch(`https://floor.supply/api/v1/launches/${chain}/${token}`, {
  headers: { Authorization: `Bearer ${process.env.FLOOR_API_KEY}` },
});
const { data } = await r.json();
const floor = data.market.floorPriceQuote; // null before graduation
const price = data.market.priceQuote;
```

```js
// server side: the history of the line, newest trade first (page with meta.nextBefore)
const t = await fetch(`https://floor.supply/api/v1/launches/${chain}/${token}/trades?limit=100`, {
  headers: { Authorization: `Bearer ${process.env.FLOOR_API_KEY}` },
});
const { data: trades, meta } = await t.json();
const floorPoints = trades
  .filter((x) => x.floorPriceQuote !== null)
  .map((x) => ({ time: x.time, value: x.floorPriceQuote }))
  .reverse(); // oldest first, for a chart
```

### 2. From the chain, if you index trades yourself

Every trade event carries the floor after the trade, as `sqrtF`:

| Chain | Event | Field |
|---|---|---|
| EVM | `Trade` on the `FloorAmm` contract | `sqrtF` (`uint128`) |
| Solana | `TradeV2` from `floor_amm` (an Anchor CPI event) | `sqrt_f` (`u128`) |

`sqrtF` is the square root of the floor price in fixed point, with 72 fraction bits. The floor in whole quote per whole
token is:

```
floor = sqrtF² × 10^6 ÷ (2^144 × 10^quoteDecimals)
```

`quoteDecimals` is the quote token's decimals (6 for USDC, 9 for SOL, 18 for ETH). The `10^6` is there because the
program counts tokens in millionths on both chains. `sqrtF` is `0` until the launch graduates: draw no line then.

```js
const floorOf = (sqrtF, quoteDecimals) =>
  Number((sqrtF * sqrtF * 10n ** 6n * 10n ** 12n) >> 144n) / 10 ** (quoteDecimals + 12);
```

Keep the multiplications in `bigint` (or a 256-bit integer); `sqrtF²` does not fit in 128 bits. The `10n ** 12n` keeps
twelve more digits through the shift. This is exactly what the example page's generator checks against the SDK.

### 3. With the SDK

`@floorlaunchpad/sdk` reads the same number from a decoded launch: `priceX64(book.sqrtF)` in
`@floorlaunchpad/sdk/math/v2/sqrt` gives a Q64.64 price in quote base units per millionth of a token. If you run the FLOOR
indexer, `readLaunch(get, id)` in `@floorlaunchpad/sdk/indexer/read` returns it as `floorX64`, and the indexer's
`/v2/dex/summary?id=<pair>` serves it as `floorPriceNative`, next to `priceNative` in the same units. To get whole quote
per whole token from a Q64.64 value:

```ts
const floor = Number((floorX64 * 10n ** 6n * 10n ** 12n) / (1n << 64n)) / 10 ** (quoteDecimals + 12);
```

## Draw it

**The look.** A solid line, 2 px, FLOOR orange `#FF6A1A`, labelled `FLOOR`, with its value on the price axis in the same
colour. The price stays your usual colour; the floor line goes under it.

**The shape.** A step line: it holds its value between trades and steps up at a trade that lifts it. It never steps down
in quote terms. It starts at graduation; before that there is no point to draw.

**The axis.** The same y axis and the same time axis as the price. Do not let the floor line widen the price scale on its
own: if the floor is far below the visible candles, pin a small `FLOOR` tag to the bottom edge of the chart instead.

**Market-cap mode.** If your chart can show market cap instead of price, multiply the floor by the same supply you
multiply the price by, so the line stays in the same place under the candles. Keep the label `FLOOR`. Do not call that
number a market cap of its own.

### Lightweight Charts (v5)

```js
import { createChart, CandlestickSeries, LineSeries, LineType } from "lightweight-charts";

const chart = createChart(el, { autoSize: true });
const candles = chart.addSeries(CandlestickSeries);
const floorLine = chart.addSeries(LineSeries, {
  color: "#FF6A1A",
  lineWidth: 2,
  lineType: LineType.WithSteps, // holds between trades, steps up
  title: "FLOOR",
  priceLineVisible: false,
  lastValueVisible: true,
  crosshairMarkerVisible: false,
  autoscaleInfoProvider: () => null, // the floor never stretches the price scale
});

candles.setData(ohlc);           // your candles, oldest first
floorLine.setData(floorPoints);  // [{ time, value }] from the trades, oldest first; none before graduation

// live: on each new trade
floorLine.update({ time: trade.time, value: trade.floorPriceQuote });
```

### TradingView Advanced Charts (the Charting Library)

The simplest version is a locked horizontal line at the current floor, moved when the floor moves:

```js
const chart = widget.activeChart();
const id = await chart.createShape(
  { price: floor },
  {
    shape: "horizontal_line",
    lock: true,
    disableSelection: true,
    disableSave: true,
    text: "FLOOR",
    overrides: { linecolor: "#FF6A1A", linewidth: 2, showLabel: true, textcolor: "#FF6A1A", showPrice: true },
  },
);

// when a trade lifts the floor
chart.getShapeById(id).setPoints([{ price: newFloor }]);
```

For the full step history, add a custom indicator that plots the floor series from your own trade store, styled as
above.

### Your own canvas or SVG

Walk the trades oldest first. At each trade with a floor, draw a horizontal segment from the previous trade's time at the
previous floor, then a vertical step up to the new floor. The example page does exactly this in about fifteen lines
(`floorPath` in its script).

### The FLOOR price on the token page

```html
<div class="stat">
  <span class="label">FLOOR</span>
  <span class="value" style="color:#FF6A1A">$0.0189</span>
  <span class="note">75% backed</span> <!-- bonus -->
</div>
```

Before graduation, show `FLOOR —` (or leave the stat out) and, if you like, `Bonding: no floor yet`.

## What to get right

**Units.** The floor is a price in the quote token, and in quote terms it never falls. If you chart in USD, the line moves
with the quote token's own USD price: a floor in a falling quote token falls in dollars though it has not fallen in the
quote. Chart in quote units where you can; where you cannot, say which units the chart uses.

**Read it, do not derive it.** Do not compute the floor from the money in the program and a supply you hold. The supply
the program uses is the live supply, and burns change it. Take `floorPriceQuote`, `sqrtF` or `floorPriceNative` as the
program reports it.

**The gap.** Once the price is past the end of the launch's sales curve (the Stack), and in the open-ended stretch after it
(the Tail), the price can rise faster than the floor, so floor ÷ price can fall while both rise. The floor itself does not
drop. A widening gap is how the design works, not a loss of backing; do not label it as one.

**Graduation is not a migration.** The first window (Bonding, one flat price) selling out is graduation. The launch stays on
the same program, with the same pair and address, and the floor starts. Keep drawing the line after graduation and in the
Tail, and do not look for the token on another venue.

**No floor, no line.** Draw nothing before graduation (`null` or `sqrtF = 0`), and hide the line if circulating supply
reads 0 (every token outside the program burned).

**Words.** Call it the "FLOOR" or the "floor price". It is what the program's money pays for a token sold into it. It is
not a promise about the market price, and the market price can sit far above it.

## Checklist

- [ ] The FLOOR price is on the token page, in the quote token or clearly marked as USD.
- [ ] The FLOOR line is on the chart: `#FF6A1A`, 2 px, a step line, labelled `FLOOR`, under the price.
- [ ] Nothing is drawn before graduation; `null` is never drawn as 0.
- [ ] The line keeps its history and keeps going after graduation.
- [ ] Market-cap mode scales the floor by the same supply as the price, or hides it.
- [ ] Optional: `% backed`, a list column, a wallet row, an alert.

Shipped it, or stuck? Tell us at contact@floor.supply. We will check it with you and point FLOOR holders to your page.
