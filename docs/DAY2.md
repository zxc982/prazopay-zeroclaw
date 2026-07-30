# Day 2 execution record

Date: 2026-07-30

## Goal

Deploy the exact Day 1 SBF to Solana devnet, execute every terminal milestone
path with valueless test identities, and invoke the read-only status component
through the real ZeroClaw v0.8.3 WASM host.

No mainnet wallet, real-value asset, private key, seed phrase, Discord token,
or model API key entered the program, plugin, fixture, or agent prompt.

## Deployment

| Field | Value |
| --- | --- |
| Cluster | `devnet` |
| Program ID | `DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm` |
| ProgramData | `4qVQJLEipmRqcKYbEUnptxJ8aYbtBojryEDEHSzwf6BM` |
| Upgrade authority | `9yZoUQRdQ13cZkf6nPb4apAV7S2pPkuwYUyDmTpM8c8g` |
| Deployment slot | `479993358` |
| ProgramData length | `225784` bytes |
| Local v1 SBF length | `216936` bytes |
| Deployed v1 SBF SHA-256 | `b792b9099410354b8f940bb7fa9aef4bbfdb8f26b51161c5a5942884199d5bf2` |
| Deployment status | `finalized` |

- [Program on Solana Explorer](https://explorer.solana.com/address/DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm?cluster=devnet)
- [Upgrade transaction](https://explorer.solana.com/tx/5C5g6QpBAVAhQTd499HYrrxxG8kk69E68R2QnQndcWFEQZzhcS2VJq4qLxzhWCxXoqS32ngehArrqP1cgPVHcWE4?cluster=devnet)

The v1 SBF was written to a temporary buffer, dumped back, and compared before
deployment. After the upgrade, the first `216936` ProgramData bytes matched the
local SBF byte for byte. The remaining `8848` capacity bytes were all zero.

The current public program therefore contains the v1 fair-settlement
instruction: after Funder review and claim grace, any signer may trigger
settlement, but the locked amount can be transferred only to the immutable
Worker.

## Current v1 fair lifecycle

The fresh v1 milestone locked exactly **1 lamport**.

| Path | Milestone PDA | Final state | Evidence |
| --- | --- | --- | --- |
| submit → Funder silence → claim grace → permissionless trigger | `ikUaYZUARH3KXK9y98MgfgSVsZJu3tcgHfgeKnCTTqB` | `PAID`; immutable Worker gained exactly `1` lamport | [settlement transaction](https://explorer.solana.com/tx/2AZLiK1TaQ3GRFWpvkkbvHaQhXQJyr4Kz4TywZHJgKYCnkY3hJtyhDSqTvVZbjAYt3MqUUaLvFQbvKS12TJAQrBJ?cluster=devnet) |

- [create transaction](https://explorer.solana.com/tx/2Eaf8P85jm5YhfsRg9akqKGgMqHf44BZ9PWxXbigSLKkUQgc1hRJAonr5Hx9UZZgmDpM3eSfyc5qzXPk2YjrA8cY?cluster=devnet)
- [submit transaction](https://explorer.solana.com/tx/3KoickzBmXxBbWpEpPn96CvnbpvW2po2Yz9ZdWA8162ZDCJWWvEuJa9EComt9mcsUrDZuc64Q7kJEata3rqUQh4p?cluster=devnet)

ZeroClaw independently observed `PERMISSIONLESS_SETTLEMENT_READY`, reported
that the Worker was not overdue, and then emitted one two-party
`SETTLEMENT_SUCCESS` final card after the real chain transition. The heartbeat
was disabled after that final delivery.

## Historical v0 compatibility lifecycle

Each independent milestone locked exactly **1 lamport**.

| Path | Milestone PDA | Final state | Evidence |
| --- | --- | --- | --- |
| submit v1 → revision → submit v2 → Funder approval | `6n2oqcwJgz2sCFZ8c97n3TvMY8ojzoNCWWBUWqkmmcdp` | `PAID`, revision count `1` | [approval transaction](https://explorer.solana.com/tx/5Qs2JrpdzzGdMyaCV5qD7mhnpH5Q8sBYea2C44YaQvn5FsSuoc24ZmRpQGB9tBWv4ztv9NmUemas12MogAkE9n9p?cluster=devnet) |
| submit → Funder silence → legacy Worker claim | `4oMWLLRKckTP3EkFMwK2Zjz2sD2fQDTKmk1LqQT2jsPk` | `PAID` | [claim transaction](https://explorer.solana.com/tx/5934HLJ1fupUJxCeyVBnxajgca2WP9AthhbvSScYb4o6365G4Sss23uRNh5kAuBwnf22G99yZ5tuq32SoPBRzzJ2?cluster=devnet) |
| no submission → deadline → third-party trigger | `2CDnXi8DhATJHfiS3jhJiCivFEWWfKr342DPzGNGixMp` | `REFUNDED` to immutable Funder | [refund transaction](https://explorer.solana.com/tx/3eSoX7GTpLczK7LqbTKn7qAP11g8saQiJKM4frEGhEzz8ikcoyTzCrWrvQroX4m7zoWPgpvjZrvtfixRPu1ZSWXz?cluster=devnet) |

All ten lifecycle transaction signatures were independently queried after the
run and returned `Finalized`.

The Rust client also asserted:

- approval increased the immutable worker balance by exactly one lamport;
- silent-review claim reduced escrow by exactly one lamport;
- third-party refund increased the immutable funder balance by exactly one
  lamport;
- paid/refunded terminal status and revision count matched expectations.

## Real ZeroClaw invocation

Runtime:

```text
zeroclaw 0.8.3
feature: plugins-wasm-cranelift
plugin: prazopay-status 0.1.0
capability: Tool
permission: HttpClient
```

Invocation:

```text
tool: prazopay_status
cluster: devnet
milestone: 6n2oqcwJgz2sCFZ8c97n3TvMY8ojzoNCWWBUWqkmmcdp
trace_id: 54ef9166-7c9d-4192-9ed2-2845b39ed369
result: success
status: paid
revision_count: 1
amount_lamports: 1
slot: 479859583
reason_codes: [TERMINAL_PAID]
```

The public trace contains the provider request/response, `tool_call_start`,
`tool_call_result`, second provider response, and `turn_final_response`. The
model issued one native call; the component fetched Solana devnet itself and
returned deterministic account facts. The host log separately recorded a
successful component execution.

Creator used its existing `locked_down` risk profile. The exact read-only tool
name was temporarily added to `auto_approve` because a single-shot CLI turn has
no interactive operator. The list was restored to `[]` immediately after the
successful call. No wildcard approval was used.

Both creator and worker Discord daemons were then restarted and
`zeroclaw channel doctor` reported Discord healthy for both.

## Timing correction discovered on devnet

Two initial create attempts failed closed with
`InvalidReviewWindow (0x1772)`. The devnet RPC Clock sysvar observed through the
client lagged local UTC by about 13 seconds, and transaction confirmation added
more latency. The attempted accounts and transfers rolled back atomically.

The client now:

1. reads the Clock sysvar directly instead of recent block time;
2. uses `max(clock_sysvar, current_utc)` as its deadline baseline; and
3. leaves a 100-second safety margin for the short expiry fixture.

The program validation rules were not weakened.

## Evidence files

- [`../fixtures/devnet-deployment.json`](../fixtures/devnet-deployment.json)
- [`../fixtures/devnet-lifecycle.json`](../fixtures/devnet-lifecycle.json)
- [`../fixtures/zeroclaw-trace.jsonl`](../fixtures/zeroclaw-trace.jsonl)
- [`../fixtures/devnet-active-monitor.json`](../fixtures/devnet-active-monitor.json)
- [`../fixtures/devnet-fair-lifecycle.json`](../fixtures/devnet-fair-lifecycle.json)

The proactive no-prompt heartbeat and Discord delivery are documented
separately in [`ACTIVE_MONITOR.md`](ACTIVE_MONITOR.md).

| File | SHA-256 |
| --- | --- |
| `devnet-deployment.json` | `a1af7b87b0b5b378d5fb37408f52f5f8ea672a32d68e6d0edf4ac1ed6ab55f06` |
| `devnet-lifecycle.json` | `7a81e50a80d8a014a7fb07de99974387533fc8c2a09fafa098d1449a3de00db1` |
| `zeroclaw-trace.jsonl` | `a9190a53ab289390848870b3aebe3722dd5a54a44fba345c2e7e00a7497eff7d` |
| `devnet-active-monitor.json` | `5a5429dfcd8befce236a2bb42df16f9907abe20da166f09bbf99626633a34ed1` |
| `devnet-fair-lifecycle.json` | `c4f1a2f309da7c3beae10710003ed881f29db475254d0fe06dd97f40d811ef8f` |

## Security boundary observed

ZeroClaw reported that no OS sandbox backend was available and used its
application-layer security. This does not affect the on-chain settlement
authority, but it remains an honest host-hardening limitation. The plugin has
no wallet or transaction-submission interface and accepts only a devnet
milestone PDA.
