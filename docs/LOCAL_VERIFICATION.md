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

- milestone parties, amount, terms hash, deadline, review window, and
  silence-acceptance acknowledgement are stored from the signed instruction;
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
- monitor decisions are based on structured tool output rather than prompt
  memory; and
- the relay acknowledges delivered event IDs, suppresses duplicates across
  restarts, and stops after the single terminal outcome.

## Reproduced artifacts

The clean-checkout suite:

1. runs all Rust workspace tests, including transaction-level LiteSVM cases;
2. rebuilds the public program source without a deployment keypair and requires
   byte-for-byte equality with the committed devnet SBF;
3. loads that exact committed SBF in the execution tests;
4. compiles `prazopay-status` for `wasm32-wasip2`;
5. requires `wasm-tools` validation of that component;
6. runs the relay and live-verifier Python tests plus Bash syntax checks; and
7. verifies consistency across the public evidence fixtures.

The committed SBF is
[`fixtures/prazopay-v1.so`](../fixtures/prazopay-v1.so):

| Property | Value |
| --- | --- |
| Program ID | `DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm` |
| Length | `216936` bytes |
| SHA-256 | `b792b9099410354b8f940bb7fa9aef4bbfdb8f26b51161c5a5942884199d5bf2` |

That hash matches the rebuilt source output and the deployed program prefix
recorded in
[`fixtures/devnet-fair-lifecycle.json`](../fixtures/devnet-fair-lifecycle.json).

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
.\scripts\verify-devnet-live.ps1
```
