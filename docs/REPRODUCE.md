# One-command reproduction

This guide verifies PrazoPay from a clean checkout without a wallet, Discord
token, model API key, Solana keypair, or transaction signing.

## Quick start

### Windows with WSL

Prerequisites:

- Windows 10 or 11 with WSL 2;
- an `Ubuntu-24.04` WSL distribution;
- Rust `1.97.1` with the `wasm32-wasip2` target;
- Python 3; and
- optional `wasm-tools` for component validation.

```powershell
git clone https://github.com/zxc982/prazopay-zeroclaw.git
Set-Location .\prazopay-zeroclaw
.\scripts\reproduce.ps1
```

Use a different WSL distribution when needed:

```powershell
.\scripts\reproduce.ps1 -Distro Ubuntu
```

### Linux or WSL

Prerequisites:

- Bash;
- Rust `1.97.1` with `rustfmt` and the `wasm32-wasip2` target;
- Python 3; and
- optional `wasm-tools`.

```bash
git clone https://github.com/zxc982/prazopay-zeroclaw.git
cd prazopay-zeroclaw
bash ./scripts/reproduce.sh
```

The first run may download Rust dependencies. Cargo outputs are written under
the user's cache directory, not committed to the repository.

## Expected completion

A successful run ends with:

```text
fixtures/prazopay-v1.so: OK
SBF_FIXTURE_HASH=PASS
WASM_VALIDATE=PASS
PUBLIC_EVIDENCE=PASS
REPRODUCE=PASS
```

If `wasm-tools` is not installed, the component check reports
`WASM_VALIDATE=SKIPPED`; every other check still runs. Any failed required
check exits non-zero and prevents `REPRODUCE=PASS`.

## What the command proves

| Check | Evidence |
| --- | --- |
| Source formatting | `cargo fmt --all -- --check` |
| Protocol behavior | Rust state-machine and transaction-level LiteSVM tests |
| Deployed byte execution | LiteSVM loads `fixtures/prazopay-v1.so` |
| Read-only agent behavior | ZeroClaw status and monitor tests |
| WASM build | `prazopay-status` compiles for WASI Preview 2 |
| Component validity | `wasm-tools validate` when available |
| Delivery reliability | Python relay acknowledgement and deduplication tests |
| Shell integrity | Monitor, relay, approval, and skill scripts pass `bash -n` |
| Artifact identity | Committed SBF SHA-256 matches the recorded deployed prefix |
| Evidence consistency | Public fixtures agree on cluster, Program ID, lifecycle, and terminal outcomes |

The command does not prove subjective work quality, Discord human receipt, RPC
honesty, or host integrity. Those remain outside the protocol boundary. It
does prove that the committed program rules reject unauthorized or premature
transitions and constrain every terminal transfer to the immutable on-chain
recipient.

## Independent public-chain checks

No PrazoPay wallet or ZeroClaw configuration is needed to inspect:

- [deployed program](https://explorer.solana.com/address/DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm?cluster=devnet)
- [current v1 milestone](https://explorer.solana.com/address/ikUaYZUARH3KXK9y98MgfgSVsZJu3tcgHfgeKnCTTqB?cluster=devnet)
- [creation transaction](https://explorer.solana.com/tx/2Eaf8P85jm5YhfsRg9akqKGgMqHf44BZ9PWxXbigSLKkUQgc1hRJAonr5Hx9UZZgmDpM3eSfyc5qzXPk2YjrA8cY?cluster=devnet)
- [delivery transaction](https://explorer.solana.com/tx/3KoickzBmXxBbWpEpPn96CvnbpvW2po2Yz9ZdWA8162ZDCJWWvEuJa9EComt9mcsUrDZuc64Q7kJEata3rqUQh4p?cluster=devnet)
- [permissionless settlement](https://explorer.solana.com/tx/2AZLiK1TaQ3GRFWpvkkbvHaQhXQJyr4Kz4TywZHJgKYCnkY3hJtyhDSqTvVZbjAYt3MqUUaLvFQbvKS12TJAQrBJ?cluster=devnet)

The machine-readable record is in
[`fixtures/devnet-fair-lifecycle.json`](../fixtures/devnet-fair-lifecycle.json).
The exact SBF is
[`fixtures/prazopay-v1.so`](../fixtures/prazopay-v1.so), with SHA-256:

```text
b792b9099410354b8f940bb7fa9aef4bbfdb8f26b51161c5a5942884199d5bf2
```

## Optional deployment-workspace verification

The original deployment workspace can also rebuild Anchor artifacts and check
that the generated Program ID matches every source and IDL reference:

```powershell
.\scripts\deployment-workspace-check.ps1
```

This advanced command intentionally requires the ignored
`target/deploy/prazopay-keypair.json`, Anchor CLI, Solana CLI, and
`wasm-tools`. It is not required for clean-checkout reproduction and does not
publish or expose the keypair.

## Safety boundary

The reproduction command performs local builds and tests only. It does not
deploy, connect a wallet, sign a transaction, call a Solana RPC endpoint, send
a Discord message, or modify the public devnet state.
