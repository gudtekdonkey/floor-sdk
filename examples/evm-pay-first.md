# EVM: quote and swap

```text
// discover
launchCount()                         0x27cca59f
launchToken(id) / launchQuote(id)     0x6a19197b / 0x98032919
launchOf(token)                       0x029282d7   (0 = not a FloorAmm token)
launchBook(id) / launchFeeCurve(id)   0x926310f1 / 0xc598083f   // price off chain with the SDK math
bidMoney(id)                          0x8e403c51   // F + C + K

// quote on chain, same code path as the trade
quoteBuy(id, amountIn, to)            0xec7a34c9   // tokensOut is the first word
quoteSell(id, tokensIn, to)           0x0cff08d1

// execute
buy(id, amountIn, minTokensOut, to, deadline)        0x102d8c79   // native: msg.value == amountIn
sell(id, tokensIn, minQuoteOut, to, deadline)        0xbadb999b
buyFromBalance(id, minTokensOut, to, refundTo, deadline)   0x5f9f56d2
sellFromBalance(id, minQuoteOut, to, deadline)             0x611b00a8
```

Pay-first, no approval, in one transaction:

```text
1. transfer(FloorAmm, amountIn)          // the quote token, or the launch token for a sell
2. buyFromBalance(id, minOut, to, refundTo, deadline)
```

The amount out is the first return word. `deadline == 0` means none. Exact-in only. A buy the contract cannot fill
(sold out, or past the 24-piece walk budget) fills what it reached and refunds the rest; a sell returns the tokens the bid cannot take.
Refresh the book after any `Trade`, `Graduated` or burn.
