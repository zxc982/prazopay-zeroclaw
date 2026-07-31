# Reproduction and live verification

PrazoPay exposes two independent verification paths:

1. **deterministic reproduction** rebuilds and tests the public source locally;
2. **live devnet verification** compares the committed evidence with finalized
   Solana state through read-only RPC calls.

Neither path needs a wallet, Solana keypair, Discord token, model API key, or
transaction signature.

## Verification levels

| Level | Command | Network | Signing | What it establishes |
| --- | --- | --- | --- | --- |
| A — deterministic | `scripts/reproduce.*` | dependency download only | none | source behavior, source-to-SBF byte identity, WASM validity, relay behavior |
| B — live read-only | `scripts/verify-devnet-live.*` | Solana devnet RPC | none | current ProgramData bytes, milestone state, finalized transactions, Worker balance delta |
| C — fresh lifecycle | `prazopay-devnet` client | Solana devnet writes | three isolated test identities | a newly created one-lamport lifecycle; optional because it consumes devnet SOL and waits on chain time |

Levels A and B are the normal reviewer path. Level C is intentionally not
hidden inside a supposedly harmless verification command.

## Prerequisites

The pinned verification environment is:

```text
Rust: 1.97.1
Solana CLI / cargo-build-sbf: 3.1.10
wasm-tools: 1.254.0
Python: 3.x
SBF platform-tools: selected by Solana CLI 3.1.10
```

Windows reviewers need WSL 2 and an `Ubuntu-24.04` distribution. Linux and WSL
reviewers need Bash plus the tools above.

Install the Rust pieces:

```bash
rustup toolchain install 1.97.1 --profile minimal --component rustfmt
rustup target add wasm32-wasip2 --toolchain 1.97.1
cargo install wasm-tools --version 1.254.0 --locked
```

Install the pinned Solana CLI:

```bash
sh -c "$(curl -sSfL https://release.anza.xyz/v3.1.10/install)"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
```

The scripts fail closed with `MISSING_PREREQUISITE`, `SOLANA_CLI_VERSION=FAIL`,
or `RUST_TARGET=FAIL` instead of silently skipping a required check.

## Level A: deterministic reproduction

### Windows with WSL

```powershell
git clone https://github.com/zxc982/prazopay-zeroclaw.git
Set-Location .\prazopay-zeroclaw
.\scripts\reproduce.ps1
```

Use a different WSL distribution when necessary:

```powershell
.\scripts\reproduce.ps1 -Distro Ubuntu
```

### Linux or WSL

```bash
git clone https://github.com/zxc982/prazopay-zeroclaw.git
cd prazopay-zeroclaw
bash ./scripts/reproduce.sh
```

The first run downloads Rust dependencies and SBF platform tools. Build output
is stored under the user's cache directory rather than committed to the
repository.

### Expected completion

A successful run includes:

```text
SOURCE_TO_SBF=PASS
fixtures/prazopay-v1.so: OK
SBF_FIXTURE_HASH=PASS
WASM_VALIDATE=PASS
PUBLIC_EVIDENCE=PASS
REPRODUCE=PASS
```

Every entry above is required. Missing `wasm-tools` is a failure, not a
successful run with a skipped validation.

### Verification map

| Check | Evidence |
| --- | --- |
| Source formatting | `cargo fmt --all -- --check` |
| Protocol behavior | Rust state-machine and transaction-level LiteSVM tests |
| Source-to-SBF identity | `cargo build-sbf` output is byte-identical to `fixtures/prazopay-v1.so` |
| Deployed-byte execution | LiteSVM loads the exact committed SBF |
| Read-only agent behavior | ZeroClaw status and monitor tests |
| WASM build | `prazopay-status` compiles for WASI Preview 2 |
| Component validity | mandatory `wasm-tools validate` |
| Delivery reliability | Python relay acknowledgement and deduplication tests |
| Live-verifier decoding | Python tests cover loader and milestone binary layouts |
| Shell integrity | monitor, relay, approval, skill, and verifier wrappers pass `bash -n` |
| Fixture consistency | public files agree on cluster, Program ID, lifecycle, and outcomes |

## Level B: finalized live devnet verification

This path performs no write and uses no signer.

### Windows

```powershell
.\scripts\verify-devnet-live.ps1
```

### Linux or WSL

```bash
bash ./scripts/verify-devnet-live.sh
```

Use a private or alternative devnet RPC endpoint when the public endpoint is
rate-limited:

```powershell
.\scripts\verify-devnet-live.ps1 -RpcUrl $env:SOLANA_RPC_URL
```

The URL is never written to a fixture, and query parameters or credentials are
not printed. A successful live check ends with:

```text
PROGRAM_ID=DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm
PROGRAMDATA=4qVQJLEipmRqcKYbEUnptxJ8aYbtBojryEDEHSzwf6BM
ONCHAIN_SBF_SHA256=b792b9099410354b8f940bb7fa9aef4bbfdb8f26b51161c5a5942884199d5bf2
MILESTONE_STATUS=PAID
FINALIZED_TRANSACTIONS=3
WORKER_BALANCE_DELTA=1_LAMPORT
LIVE_DEVNET_VERIFY=PASS
```

The verifier:

1. fetches the executable Program account at finalized commitment;
2. resolves and decodes its Upgradeable Loader ProgramData account;
3. confirms deployment slot and disclosed upgrade authority;
4. compares the live executable prefix byte for byte with the locally rebuilt
   SBF and checks the remaining allocation is zero padding;
5. decodes the current milestone's Anchor discriminator and fixed binary layout;
6. confirms protocol v1, immutable parties, amount, review window, terminal
   timestamp, and `PAID` state;
7. verifies creation, delivery, and permissionless settlement are finalized,
   successful, and reference both PrazoPay and the recorded milestone; and
8. calculates the settlement transaction's balance arrays to confirm the
   immutable Worker gained exactly one lamport.

The default endpoint is Solana's public devnet RPC. Network failure, RPC
rate-limiting, a later program upgrade, missing transaction history, or any
chain/fixture mismatch fails closed and prevents `LIVE_DEVNET_VERIFY=PASS`.

## Level C: fresh devnet lifecycle

The repository includes a real client for reviewers who want to create new
state rather than inspect the recorded lifecycle:

```bash
cargo run -p prazopay-devnet -- \
  <funder-keypair.json> \
  <worker-keypair.json> \
  <permissionless-trigger-keypair.json> \
  target/devnet/lifecycle.json
```

The identities must be distinct, isolated devnet-only keypairs with enough
devnet SOL for rent and transaction fees. The client creates three independent
one-lamport milestones, exercises revision plus approval, silence settlement,
and expiry refund, waits for the real Clock sysvar boundaries, checks recipient
balance deltas, and writes all transaction signatures to the requested JSON.

This write path is deliberately separate because devnet airdrops are
rate-limited and a reviewer must knowingly authorize transaction signing. The
read-only ZeroClaw/Discord integration can then monitor one of the resulting
milestone PDAs using the procedure in
[`ACTIVE_MONITOR.md`](ACTIVE_MONITOR.md).

## Independent Explorer inspection

- [deployed program](https://explorer.solana.com/address/DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm?cluster=devnet)
- [current v1 milestone](https://explorer.solana.com/address/ikUaYZUARH3KXK9y98MgfgSVsZJu3tcgHfgeKnCTTqB?cluster=devnet)
- [creation transaction](https://explorer.solana.com/tx/2Eaf8P85jm5YhfsRg9akqKGgMqHf44BZ9PWxXbigSLKkUQgc1hRJAonr5Hx9UZZgmDpM3eSfyc5qzXPk2YjrA8cY?cluster=devnet)
- [delivery transaction](https://explorer.solana.com/tx/3KoickzBmXxBbWpEpPn96CvnbpvW2po2Yz9ZdWA8162ZDCJWWvEuJa9EComt9mcsUrDZuc64Q7kJEata3rqUQh4p?cluster=devnet)
- [permissionless settlement](https://explorer.solana.com/tx/2AZLiK1TaQ3GRFWpvkkbvHaQhXQJyr4Kz4TywZHJgKYCnkY3hJtyhDSqTvVZbjAYt3MqUUaLvFQbvKS12TJAQrBJ?cluster=devnet)

The exact SBF is
[`fixtures/prazopay-v1.so`](../fixtures/prazopay-v1.so), with SHA-256:

```text
b792b9099410354b8f940bb7fa9aef4bbfdb8f26b51161c5a5942884199d5bf2
```

## What is not proven

These checks do not prove subjective work quality, that a human read a Discord
message, that every possible RPC provider is honest, or that the reviewer's
host is uncompromised. The local and live paths provide independent evidence:
the local path verifies the public rules and rebuilt bytes, while the live path
verifies the bytes and resulting state currently reported by finalized devnet.

This repository has not registered a Solana Verified Build PDA. Its
source-to-chain evidence is the combination of a pinned no-keypair rebuild,
byte equality in CI, and a live ProgramData comparison. Registering an official
Verified Build requires a controlled Docker build plus an authority-signed
verification record and is therefore separate from reviewer-safe read-only
reproduction.

## Solana references

- [Install the Solana CLI](https://solana.com/docs/intro/installation)
- [Build, deploy, and dump programs](https://solana.com/docs/programs/deploying)
- [Verified Builds](https://solana.com/docs/programs/verified-builds)
- [`getAccountInfo`](https://solana.com/docs/rpc/http/getaccountinfo)
- [`getSignatureStatuses`](https://solana.com/docs/rpc/http/getsignaturestatuses)
- [`getTransaction`](https://solana.com/docs/rpc/http/gettransaction)

## Safety boundary

Level A performs local builds and tests only. Level B makes read-only finalized
RPC calls. Only Level C signs and submits devnet transactions, and it requires
reviewer-supplied isolated test identities. No path deploys to mainnet or gives
ZeroClaw custody.
