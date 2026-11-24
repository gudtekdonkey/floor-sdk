# Solana: one swap

Instruction data is `sha256("global:<name>")[..8]` plus Borsh args.

| Instruction | Discriminator | Args |
|---|---|---|
| `buy` | `66063d1201daebea` | `amount: u64, min_tokens_out: u64` |
| `sell` | `33e685a4017f83ad` | `tokens: u64, min_quote_out: u64` |
| `buy_from_balance` | `cafc5c36569a0440` | `keep: u64, min_tokens_out: u64` |
| `sell_from_balance` | `01e8333e49441b9a` | `keep: u64, min_quote_out: u64` |

Accounts, in order (`w` writable, `s` signer):

```text
1 trader (w s)   2 launch (w)   3 base_mint   4 quote_mint
5 token_vault (w)   6 quote_vault (w)   7 trader_base (w)   8 trader_quote (w)
9 base_token_program (Token-2022)   10 quote_token_program
11 trader_ref (w; program id when absent)   12 escrow (w; program id when absent)
13 event_authority   14 program
rem[0..=4]  referral group: the user (w s; the same wallet as the trader when it trades directly), its link, the System and Associated Token programs, the recipient's link; the program id in each slot when there is no referral record
rem[5..=10] hook accounts: extra_account_metas, launch, holder_book (w), source position (w), destination position (w), floor_hook
```

`launch` is the PDA `["launch", base_mint]`. A plain swap is 25 account metas. When the quote is itself a FLOOR launch
token, add the quote mint's six hook accounts (`rem[11..=16]`, 31 metas) and, with a referral record, `rem[17]` (32).

The `Amm` shape, in the `floor-amm-jupiter` crate:

```rust
// One Amm per LaunchV2 account.
fn get_accounts_to_update(&self) -> Vec<Pubkey> { vec![self.key, self.launch.quote_mint, self.launch.base_mint] } // the base mint holds the live supply
fn quote(&self, p: &QuoteParams) -> Result<Quote> { /* the program's own math on the decoded account */ }
fn supports_exact_out(&self) -> bool { false }
```

`Quote.in_amount` is the part spent. A buy past what is left is partly refunded on chain.
