# Cursor pages

Every list route of the FLOOR indexer that pages by cursor follows one contract.

## Request

`?<cursor key>=<cursor>&limit=<n>`. The cursor key is `cursor` on every route, except where a route already took `before`
before cursor pages existed (`launches/<id>/trades`, `wallets/<w>/trades`, `launches/<id>/marks`, `wallets/<w>/inbox`).
`limit` has a per-route maximum (500 at most for a wallet page); the contract table (`INDEXER_CONTRACT`, `routeLimit`)
holds each route's default and maximum. A `limit` above the maximum is a 400 (`limit must be 1..500`, with the route's
own bounds), never a silent clamp.

The wallet routes `positions`, `locks`, `claims`, `roles` and `referrals/<w>/people` answer a page only when one is asked
(`limit`, or a non-empty `cursor`). The inbox answers a page when `before` is present, and an empty `before` asks the
newest page. Asked with none of these, each route answers its whole list in one body.

## Defaults

| route | reader | cursor key | `limit` default | `limit` maximum |
|---|---|---|---|---|
| `launches` | `readLaunches` | `cursor` | `LAUNCHES_PAGE_DEFAULT` (100) | `LAUNCHES_PAGE_MAX` (1,000) |
| `search?q=` | `readLaunchSearch` | `cursor` | `SEARCH_PAGE_DEFAULT` (12) | `SEARCH_PAGE_MAX` (100) |
| `wallets/<w>/positions`, `locks`, `claims`, `roles/<w>`, `referrals/<w>/people` | `readPositions`, `readWalletLocks`, `readClaimHistory`, `readRoles`, `readPeople` | `cursor` | `WALLET_PAGE_DEFAULT` (100) | `WALLET_PAGE_MAX` (500) |
| `wallets/<w>/inbox` | `readInbox` | `before` | `INBOX_PAGE_DEFAULT` (100) | `INBOX_PAGE_MAX` (500) |

No request answers every launch: a bare `launches` is one page of 100. The list body also carries `cursors` (the cursor
after each row) and `keys` (each row's ranked key); `launches` takes a `preset` filter.

## Answer

`{ …, <rows>: [...], next: <string> | null }`. `next` is null at the end; passing it back as the cursor returns the
following page. `cursorPage(body, rowsKey)` reads any decoded page body as `{ rows, next }`; `drain` walks every page.

## Cursor

One format on every cursored route, except the trades pages' `before` (see below). The indexer alone writes and reads
it: it is base64url of a small JSON object whose first keys are
`v` (the version, 1), `r` (the route) and `f` (a hash of the query). The position of the page's last row follows. A
client treats it as opaque: it passes back the `next` the indexer served, and bounds the length
(`WALLET_CURSOR_MAX` for a wallet page, `LAUNCH_CURSOR_MAX` for `launches` and `search`).

| route | order | `next` positions the page after |
|---|---|---|
| `wallets/<w>/inbox` (`before`), `wallets/<w>/claims` | newest first | the last row (block, txnIndex, eventIndex) |
| `wallets/<w>/positions`, `wallets/<w>/locks`, `roles/<w>` | launch create order | the last launch (createdBlock, id) |
| `referrals/<w>/people` | referred wallet ascending | the last wallet |
| `launches`, `search` | the asked sort | the last row's key and tie (`cursors[i]` per row on the list) |

A cursor names its query: one served for another route, another wallet or other filters is a 400 `cursor: not for this
query`. One that does not decode, or whose fields do not fit the chain, is a 400 `cursor: malformed`. A row id or a bare
block is not a cursor. A row keeps its own `id` (the inbox's `<block>:<txnIndex>:<eventIndex>`).

A page never splits rows that share one position: it ends before them, or takes them all when they alone fill it.

The trades pages (`launches/<id>/trades`, `wallets/<w>/trades`, `launches/<id>/marks`) predate this format and keep
`before` = a block or a trade's `id` (`<block>:<txnIndex>:<eventIndex>`), with `next` the last trade's `id`.
