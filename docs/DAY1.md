# Day 1 execution record

Date: 2026-07-29

## Goal

Finish the local, deterministic core of PrazoPay. No public repository, devnet
deployment, wallet connection, or bounty submission is part of Day 1.

## Scope frozen

- [x] Product thesis and crowded-lane comparison
- [x] Parties, immutable fields, states, and transitions
- [x] Threat model and secret policy
- [x] Anchor program instructions
- [x] State-machine unit tests
- [x] LiteSVM end-to-end tests
- [x] Read-only ZeroClaw WASM status tool
- [x] Local build and test evidence

## Day 1 exit criteria

```powershell
.\scripts\day1-check.ps1
```

The wrapper keeps Cargo build products in the WSL-native cache, synchronizes
only the generated SBF, IDL, and WASM artifacts, then verifies formatting,
component validity, all tests, and Program ID consistency. A generated local
program keypair stays under the ignored `target/` directory and is never
committed or printed.

## Evidence

Toolchain:

```text
rustc 1.97.1 (8bab26f4f 2026-07-14)
cargo 1.97.1 (c980f4866 2026-06-30)
anchor-cli 1.1.2
solana-cli 3.1.10
wasm-tools 1.254.0
```

Deployed v0 baseline verification on 2026-07-29:

```text
anchor build: PASS
wasm-tools validate: PASS
cargo fmt --all -- --check: PASS
cargo test --workspace: 20 passed, 0 failed
Program ID consistency: PASS
Program ID: DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm
```

Test distribution:

- 1 generated program identity test;
- 3 transaction-level LiteSVM settlement tests;
- 8 pure state-machine invariant tests; and
- 8 read-only ZeroClaw status-tool tests.

Archived v0 artifact hashes:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `target/deploy/prazopay.so` | 205304 | `3dc650ca83d181cdadc4cb6a8c81df3a241abac665d1499154ba75cf15d1b909` |
| `target/idl/prazopay.json` | 17055 | `a4f09ab5e79c3e7782fe8c4a0d7723bb22d60a32d513e8d3fce044c8103aa9d2` |
| `plugins/prazopay-status/prazopay-status.wasm` | 491453 | `a68f044ccdfc717be5b07ced81286a70726ec01e927b9d71f7e63648b48e34fa` |

The current build paths no longer contain those archived bytes. They now hold
the locally verified, not-yet-deployed v1 candidate.

## V1 hardening verification

Verification on 2026-07-30:

```text
anchor build: PASS
wasm-tools validate: PASS
cargo fmt --all -- --check: PASS
cargo test --workspace: 35 passed, 0 failed
bash syntax check (heartbeat installer): PASS
legacy account compatibility: PASS
real ZeroClaw v0-account read with schema v2: PASS
```

Current test distribution:

- 1 generated program identity test;
- 3 transaction-level LiteSVM settlement tests;
- 13 state-machine normal, boundary, compatibility, and race tests; and
- 18 read-only status/monitor validation and backoff tests.

Current local artifacts:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `target/deploy/prazopay.so` | 208656 | `f1592ba9d9f4ce589de66d2809ee26dcc29e97db57b1f85130fd5f65a4bc0fb5` |
| `target/idl/prazopay.json` | 18183 | `f2b5b38594e7e46f0dfe607177cddafa5e0350d0825857124c919881d77104c8` |
| `plugins/prazopay-status/prazopay-status.wasm` | 508370 | `492654253a54941d43c660737252b4bcebd83f1bbb6d8fe8b3b7b94fa0b82a81` |

## Deferred to Day 2

- devnet deployment and valueless lifecycle;
- ZeroClaw runtime installation and real invocation;
- video capture;
- public repository;
- final submission copy; and
- optional SPL-token settlement.
