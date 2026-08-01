# Public devnet evidence

These fixtures contain only public Solana addresses, transaction signatures,
commitment hashes, program facts, and sanitized ZeroClaw runtime events.

- `devnet-deployment.json` records the finalized program deployment and exact
  local/deployed SBF hash match.
- `devnet-v2-lifecycle.json` records the current v2 upgrade, Worker-accepted
  Agreement, funded Milestone, five lifecycle transactions, and exact payout.
- `devnet-lifecycle.json` records three independent one-lamport milestone paths
  and all ten finalized transaction signatures.
- `zeroclaw-trace.jsonl` contains the seven events sharing the successful real
  ZeroClaw invocation trace ID.

No private key, seed phrase, model API key, Discord bot token, raw terms,
delivery content, or local user path is present.

`prazopay-v1.so` is the historical public devnet SBF used by the LiteSVM
compatibility tests. Its SHA-256 is
`b792b9099410354b8f940bb7fa9aef4bbfdb8f26b51161c5a5942884199d5bf2`,
matching the deployed prefix recorded in `devnet-deployment.json` and
`docs/DEVNET_EVIDENCE.md`. It contains executable bytecode only, not deployment authority
or wallet material.

`prazopay-v2.so` is the exact currently deployed v2 SBF. Its SHA-256 is
`a54c676c98f526425ba77b54cfdb64a6ddddab2cf218d12f732dfa95bb4d8294`.
The live verifier confirms the deployed prefix byte for byte and verifies the
45-byte all-zero ProgramData capacity suffix.

Run `scripts/verify-devnet-v2-live.ps1` on Windows or
`scripts/verify-devnet-v2-live.sh` on Linux to compare the current v2 files
against finalized Solana devnet state without a wallet or signer. The original
`verify-devnet-live.*` command remains the historical v1 compatibility check.
