# Signed communication model

PrazoPay does not put chat messages on chain. It turns the decisions that affect
money into signed state transitions and commits the related plaintext by hash.

## Authoritative versus explanatory channels

| Item | Authority | Discord role |
| --- | --- | --- |
| parties, price, windows, terms hash, silence policy | Funder proposal plus Worker acceptance on Solana | render the accepted facts and link Explorer |
| funding and delivery deadline | funded Milestone on Solana | announce that escrow is live |
| delivery evidence hash | Worker-signed submission on Solana | link the off-chain manifest and transaction |
| revision feedback hash | Funder-signed revision on Solana | notify Worker and link the committed feedback |
| approval, settlement, refund | terminal Solana transaction | send one final outcome card |
| negotiation text or questions | humans in Discord or another agreed channel | non-authoritative until committed and signed |

The chain is authoritative for payment state. Discord is the collaboration and
notification surface. ZeroClaw reads finalized state and explains it; it
cannot invent an accepted term or approve a payment.

## Recommended interaction

1. The Funder and Worker discuss the task in Discord.
2. The Funder publishes a canonical terms document and signs its SHA-256 in
   `propose_agreement`.
3. ZeroClaw posts a proposal card showing the parties, amount, all windows,
   complete 64-character terms hash, expiry, and Explorer link.
4. The Worker independently checks the plaintext hash, then signs
   `accept_agreement` or `reject_agreement`.
5. Only after `Accepted` does the Funder fund the escrow.
6. Delivery and revision feedback use the same manifest-plus-hash pattern.
7. ZeroClaw sends alerts only at actionable state changes and sparse escalation
   points; one final card closes monitoring.

The deployed v2 workflow implements this as two least-privilege read-only tools:

- `prazopay_agreement_status` monitors proposal, Worker acceptance/rejection,
  expiry, and Funder funding;
- `prazopay_status` monitors the funded Milestone lifecycle.

The journey relay closes after one rejected/expired Agreement or one terminal
Milestone outcome. When an Agreement becomes `Funded`, the same ZeroClaw
heartbeat reads the exact `milestone` PDA recorded by the Agreement and calls
`prazopay_status`; no human address copy or monitor reinstall is required. The
handoff emits at most one stable `Escrow Funded` card when the Milestone itself
has no actionable alert. The Agreement report says `milestone_created`; it
does not claim that funds remain locked. Live custody state always comes from
the Milestone tool. The handoff remains read-only: neither tool can sign,
simulate, or submit a transaction.

## Canonical commitment rules

For reviewer-visible demos, each off-chain payload should be UTF-8 JSON:

- object keys sorted lexicographically;
- no insignificant whitespace;
- timestamps as Unix seconds;
- lamports as integers, never floating-point SOL;
- public keys as base58 strings;
- external artifact URLs paired with a content SHA-256; and
- no secrets, local user paths, bot tokens, API keys, or wallet material.

Protocol-v2 terms use schema `prazopay.agreement-terms.v1` and must include
`funder`, `worker`, `amount_lamports`, `delivery_window_secs`,
`review_window_secs`, `revision_delivery_window_secs`, `funding_window_secs`,
`proposal_lifetime_secs`, and `silence_acceptance`. The v2 client rejects
unknown/duplicate fields, floating-point values, party mismatches, and any
revision-delivery window that differs from the review window before the Worker
is asked to sign.

Hash the exact UTF-8 bytes:

```text
sha256(canonical_json_bytes)
```

PrazoPay stores only the resulting 32 bytes. Anyone with the plaintext can
recompute the hash; the on-chain program neither downloads nor judges it.

## What this fixes

- The Worker cannot be named and funded without signing acceptance first.
- The Funder cannot later replace the accepted Worker, amount, timing, or terms
  commitment during funding.
- A Discord edit cannot rewrite the agreement that controls settlement.
- Revision feedback and delivery artifacts are attributable to their signing
  roles without exposing the content on chain.

## What remains human

PrazoPay deliberately does not solve subjective quality disputes. If a task
needs arbitration, cancellation by mutual consent, or structured counteroffers,
those require separate protocol states and should not be inferred from chat.
