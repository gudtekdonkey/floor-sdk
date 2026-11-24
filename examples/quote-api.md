# A quote API over the contract

For routers that ask a quote endpoint instead of reading state. The shape the LI.FI form used:

```text
GET /quote?chainId&fromToken&toToken&fromAmount&fromAddress&toAddress&slippageBps
→ { estimate: { toAmount, toAmountMin, approvalAddress, gasLimit },
    transactionRequest: { to, data, value, from } }
```

- `toAmount` comes from the contract's own `quoteBuy` or `quoteSell` by `eth_call`.
- Exact-in only. A buy the contract would partly refund is refused with `WOULD_REFUND`; quote a smaller amount.
- Native coin is `0x0000000000000000000000000000000000000000` on chain. The API also accepts
  `0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE` and maps it.
- The approval address for an ERC-20 input is the FloorAmm address.
