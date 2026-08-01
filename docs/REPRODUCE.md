# Reproduction and live verification

PrazoPay has three deliberately separate verification targets:

1. **local candidate verification** rebuilds and executes protocol v2, including
   Worker acceptance before funding;
2. **live v2 read-only verification** checks the current deployed bytes,
   Worker-accepted Agreement, funded Milestone, signers, and exact payout; and
3. **historical v1 verification** preserves compatibility evidence.

Neither path needs a wallet, keypair, Discord token, or model API key.

## Version boundary

| Target | Command | Network writes | What it proves |
| --- | --- | --- | --- |
| local v2 build | `scripts/reproduce.*` | none | current source compiles, tests pass, fresh SBF enforces Worker acceptance, and matches the deployed-v2 fixture |
| deployed v2 | `scripts/verify-devnet-v2-live.*` | none | current ProgramData, Agreement, Milestone, six finalized transactions, actual signer roles, and exact Worker payout |
| historical v1 | `scripts/verify-devnet-live.*` | none | preserved v1 executable prefix and finalized historical lifecycle |
| fresh lifecycle | `prazopay-devnet` client | yes | a new lifecycle for whichever matching program version is actually deployed |

Protocol v2 is deployed at slot `480289270` and independently checked against
the committed `fixtures/prazopay-v2.so`.

## Prerequisites

The pinned environment is:

```text
Rust: 1.97.1
Solana CLI / cargo-build-sbf: 3.1.10
wasm-tools: 1.254.0
Python: 3.x
```

Windows reviewers need WSL 2 and an `Ubuntu-24.04` distribution. Linux and WSL
reviewers need Bash plus the tools above.

Install Rust and WASM tooling:

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

The scripts fail closed when a prerequisite or pinned version is missing.

## One-command local v2 verification

### Windows with WSL

```powershell
git clone https://github.com/zxc982/prazopay-zeroclaw.git
Set-Location .\prazopay-zeroclaw
.\scripts\reproduce.ps1
```

To select another WSL distribution:

```powershell
.\scripts\reproduce.ps1 -Distro Ubuntu
```

### Linux or WSL

```bash
git clone https://github.com/zxc982/prazopay-zeroclaw.git
cd prazopay-zeroclaw
bash ./scripts/reproduce.sh
```

Build output is written under the Linux user cache, not into the repository.
A successful run contains:

```text
V2_LIFECYCLE=PASS
CANDIDATE_SBF_SHA256=<64 lowercase hex characters>
CANDIDATE_SBF_BUILD=PASS
fixtures/prazopay-v2.so: OK
DEPLOYED_V2_FIXTURE_MATCH=PASS
fixtures/prazopay-v1.so: OK
DEPLOYED_V1_FIXTURE_HASH=PASS
WASM_SOURCE_ARTIFACT=PASS components=2
WASM_VALIDATE=PASS components=2
PUBLIC_EVIDENCE=PASS
REPRODUCE=PASS
```

Every line is required.

### What the command checks

| Check | Evidence |
| --- | --- |
| formatting | `cargo fmt --all -- --check` |
| state-machine behavior | Rust unit tests for v0, v1, Agreement, and v2 Milestone rules |
| v2 program execution | a fresh SBF runs propose → premature-fund rejection → Worker acceptance → funding → submission → permissionless settlement in LiteSVM |
| v1 compatibility | existing tests execute the committed deployed-v1 fixture |
| ZeroClaw behavior | Agreement and Milestone status/monitor unit tests |
| WASM components | both read-only tools build from a fresh target for WASI Preview 2 with project, Cargo Home, and target paths remapped to stable virtual prefixes; rebuilt and committed components pass `wasm-tools validate` and must match byte for byte |
| delivery reliability | Python relay acknowledgement and deduplication tests |
| scripts | Bash syntax checks for monitor, relay, approval, skill, and verifier |
| public evidence | fixture metadata and hashes are internally consistent |

## Read-only live v2 verification

This is the primary public-chain check. It uses finalized RPC reads and has no
wallet or signer.

### Windows

```powershell
.\scripts\verify-devnet-v2-live.ps1
```

### Linux or WSL

```bash
bash ./scripts/verify-devnet-v2-live.sh
```

A successful check ends with:

```text
DEPLOYED_SLOT=480289270
ONCHAIN_SBF_SHA256=a54c676c98f526425ba77b54cfdb64a6ddddab2cf218d12f732dfa95bb4d8294
AGREEMENT_STATUS=FUNDED
MILESTONE_STATUS=PAID
FINALIZED_TRANSACTIONS=6
WORKER_BALANCE_DELTA=1_LAMPORT
LIVE_DEVNET_V2_VERIFY=PASS
```

The verifier also proves from transaction headers that the Funder signed the
proposal/funding/approval, the Worker independently signed
acceptance/delivery, and the Worker did not sign the original proposal.

## Historical live v1 verification

This compatibility path verifies the preserved v1 bytes and historical
transactions. It makes finalized RPC reads and has no signer.

### Windows

```powershell
.\scripts\verify-devnet-live.ps1
```

### Linux or WSL

```bash
bash ./scripts/verify-devnet-live.sh
```

An alternative devnet RPC may be supplied when the public endpoint is
rate-limited:

```powershell
.\scripts\verify-devnet-live.ps1 -RpcUrl $env:SOLANA_RPC_URL
```

A successful check ends with:

```text
PROGRAM_ID=DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm
PROGRAMDATA=4qVQJLEipmRqcKYbEUnptxJ8aYbtBojryEDEHSzwf6BM
ONCHAIN_SBF_SHA256=b792b9099410354b8f940bb7fa9aef4bbfdb8f26b51161c5a5942884199d5bf2
MILESTONE_STATUS=PAID
FINALIZED_TRANSACTIONS=3
WORKER_BALANCE_DELTA=1_LAMPORT
LIVE_DEVNET_VERIFY=PASS
```

The verifier resolves the ProgramData account, compares deployed bytes with
`fixtures/prazopay-v1.so`, decodes the recorded v1 Milestone, checks all three
transactions, and confirms that the immutable Worker gained one lamport.

## Fresh lifecycle writes

These commands sign devnet transactions and must use isolated test-only
identities. The deployed v2 flow is:

Copy [`fixtures/agreement-terms.example.json`](../fixtures/agreement-terms.example.json)
and replace its public Funder/Worker keys with the isolated test identities.
Keep lamports and durations as integers and keep
`revision_delivery_window_secs == review_window_secs`.

```bash
cargo run -p prazopay-devnet --bin prazopay-demo -- \
  propose <funder-keypair.json> <worker-pubkey> <terms.json> <session.json>
cargo run -p prazopay-devnet --bin prazopay-demo -- \
  accept <worker-keypair.json> <terms.json> <session.json>
cargo run -p prazopay-devnet --bin prazopay-demo -- \
  fund <funder-keypair.json> <session.json>
cargo run -p prazopay-devnet --bin prazopay-demo -- \
  submit <worker-keypair.json> <session.json>
```

`propose` never receives the Worker keypair. `accept` independently hashes the
same canonical terms file, compares every committed field with the finalized
Agreement and session, and fails before signing on any mismatch.

The Funder may then call `approve`, or the session may be inspected until
permissionless `settle` becomes valid. The Worker may use `reject` instead of
`accept`; no Milestone amount has been locked at that point.

The historical deployed-v1 three-path runner remains:

```bash
cargo run -p prazopay-devnet -- \
  <funder-keypair.json> \
  <worker-keypair.json> \
  <permissionless-trigger-keypair.json> \
  target/devnet/lifecycle.json
```

## Public v2 Explorer evidence

- [deployed program](https://explorer.solana.com/address/DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm?cluster=devnet)
- [v2 upgrade](https://explorer.solana.com/tx/2tyAdkSNL7WfjzE31yoGSPCLuL2uCJWWip96LDATbHQCLqW7UWtFTG7RXWDnDiQhmQspWcCtP3rgZ1RuEfX9ZGpA?cluster=devnet)
- [Agreement](https://explorer.solana.com/address/Cg3xWCC4SiCEshSLcMSt6GGWSpb25zAhv9iTXuuBXeaW?cluster=devnet)
- [Worker acceptance](https://explorer.solana.com/tx/4Jg6ZzySRszaiyHhiZoTNwAu7HQB22UDPEkvCh5fWc5NQ5rYTaj5knyoa7eaXyT7EHxFcYn4VHAQxWiUez5G5BuE?cluster=devnet)
- [Milestone](https://explorer.solana.com/address/Can5CgbqzVcH2rSJmPY8p73QYecAUcxdaFXFnYN2Qvvk?cluster=devnet)
- [Funder approval](https://explorer.solana.com/tx/3gJaTe5sdqpXCruCnjWHrTqLyh5YtsygH9NWbsQmYkDrfZi9YDoHpcdHphvniuo6ZAmYrfGpDeHPUcc4rWaz78vd?cluster=devnet)

The exact deployed-v2 fixture SHA-256 is:

```text
a54c676c98f526425ba77b54cfdb64a6ddddab2cf218d12f732dfa95bb4d8294
```

## What is not proven

These checks do not prove subjective work quality, that a human saw a Discord
message, that every RPC provider is honest, or that the host is uncompromised.
The local path verifies the v2 public rules and executes a fresh rebuild. The
current live path independently verifies the deployed v2 bytes and finalized
state reported by devnet.

This repository has not registered a Solana Verified Build PDA. Instead it
commits the exact deployed SBF, reproducible build hash, ProgramData byte
comparison, and public transactions; this is strong evidence but not a
registry-issued verified-build attestation.

## Safety boundary

Local reproduction builds and tests only. Live verification performs read-only
RPC calls. Only the fresh lifecycle commands sign and submit devnet
transactions, and they require a matching deployed version plus reviewer-
supplied isolated identities. Nothing deploys to mainnet or gives ZeroClaw
custody.
