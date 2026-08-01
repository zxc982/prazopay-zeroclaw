# prazopay-agreement-status

A read-only ZeroClaw WASM tool for protocol-v2 Agreement negotiation:

```text
prazopay_agreement_status(
  cluster = "devnet",
  agreement = "<base58 Agreement PDA>",
  alert_before_secs = 300,
  poll_interval_secs = 300
)
```

It validates the account owner, discriminator, exact v2 Agreement layout and
finalized Solana block time. The result identifies whether the named Worker
must accept or reject, the Funder may fund, or the Agreement is rejected,
expired, or funded.

The tool is advisory and read-only. It never accepts wallet material, signs a
transaction, chooses a recipient, or treats Discord content as agreement
state. Alerts use stable event IDs and sparse stages: immediate, 30 minutes,
2 hours, daily, and the proposal deadline boundary.

`AGREEMENT_REJECTED` and `AGREEMENT_EXPIRED` are terminal. `AGREEMENT_FUNDED`
is a non-terminal handoff event: the same journey monitor reads the exact
Milestone PDA recorded by the Agreement and continues through
`prazopay_status`. The Agreement report says only that the Milestone was
created; it never claims that funds remain locked after the Milestone settles.

The matching protocol v2 program is deployed on devnet at slot `480289270`.
The tool remains advisory and read-only; deployment does not grant it signing
or settlement authority.
