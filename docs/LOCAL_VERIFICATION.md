# Local verification

## Purpose

The local suite verifies PrazoPay's deterministic settlement rules and
ZeroClaw's read-only operational boundary. It does not connect a wallet, write
to devnet, send Discord messages, or require any private credential.

## Canonical command

From a clean Windows checkout with WSL:

```powershell
.\scripts\reproduce.ps1
```

On Linux or inside WSL:

```bash
bash ./scripts/reproduce.sh
```

Full setup instructions and expected output are in
[`REPRODUCE.md`](REPRODUCE.md).

## Acceptance properties

The suite checks that:

- a Funder proposal locks no milestone amount;
- only the immutable Worker can accept or reject the exact Agreement;
- funding before acceptance and funding after expiry both fail;
- successful funding copies the accepted parties, amount, terms hash, timing,
  and silence policy into a v2 Milestone;
- the delivery window starts only when funding succeeds;
- only the immutable Worker can submit before the active deadline;
- only the immutable Funder can approve or request a revision;
- a last-second submission still receives the complete review window;
- a bounded revision opens a deterministic replacement delivery window;
- silence settlement cannot occur before both review and claim grace expire;
- any permissionless trigger can settle only to the immutable Worker;
- an unsubmitted expired milestone refunds only to the immutable Funder;
- every terminal path releases the escrow exactly once;
- the read-only WASM tool validates the cluster, PDA, owner, discriminator,
  account length, protocol version, and finalized commitment;
- the Agreement WASM tool derives role-specific acceptance/funding actions and
  sparse alerts from chain timestamps while keeping pre-funding funds false;
- monitor decisions are based on structured tool output rather than prompt
  memory; and
- the relay acknowledges delivered event IDs, suppresses duplicates across
  restarts, and stops after the single terminal outcome.

## Reproduced artifacts

The clean-checkout suite:

1. runs all Rust workspace tests, including transaction-level LiteSVM cases;
2. builds a fresh v2 SBF without a deployment keypair and compares it byte for
   byte with the exact deployed-v2 fixture;
3. loads that SBF and executes Worker acceptance, funding, delivery, and
   settlement, while compatibility tests retain the historical deployed-v1
   fixture;
4. compiles both `prazopay-status` and `prazopay-agreement-status` for
   `wasm32-wasip2`;
5. requires `wasm-tools` validation of both components;
6. runs the relay and live-verifier Python tests plus Bash syntax checks; and
7. verifies consistency across the public evidence fixtures.

The current committed SBF is
[`fixtures/prazopay-v2.so`](../fixtures/prazopay-v2.so):

| Property | Value |
| --- | --- |
| Program ID | `DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm` |
| Length | `286368` bytes |
| SHA-256 | `a54c676c98f526425ba77b54cfdb64a6ddddab2cf218d12f732dfa95bb4d8294` |

That hash identifies the current deployed-v2 program prefix recorded in
[`fixtures/devnet-v2-lifecycle.json`](../fixtures/devnet-v2-lifecycle.json).
The historical v1 SBF remains available for compatibility tests.

## Optional deployment-workspace check

The original deployment workspace can additionally rebuild the Anchor
artifacts and verify Program ID consistency:

```powershell
.\scripts\deployment-workspace-check.ps1
```

This advanced command intentionally requires the ignored
`target/deploy/prazopay-keypair.json`, Anchor CLI, Solana CLI, and
`wasm-tools`. The keypair is not published, and this command is not required
for clean-checkout reproduction.

For public deployment, lifecycle, and real ZeroClaw evidence, see
[`DEVNET_EVIDENCE.md`](DEVNET_EVIDENCE.md).

For live, finalized, read-only comparison against Solana devnet, run:

```powershell
.\scripts\verify-devnet-v2-live.ps1
```
