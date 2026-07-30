# Public devnet evidence

These fixtures contain only public Solana addresses, transaction signatures,
commitment hashes, program facts, and sanitized ZeroClaw runtime events.

- `devnet-deployment.json` records the finalized program deployment and exact
  local/deployed SBF hash match.
- `devnet-lifecycle.json` records three independent one-lamport milestone paths
  and all ten finalized transaction signatures.
- `zeroclaw-trace.jsonl` contains the seven events sharing the successful real
  ZeroClaw invocation trace ID.

No private key, seed phrase, model API key, Discord bot token, raw terms,
delivery content, or local user path is present.

`prazopay-v1.so` is the exact public devnet SBF used by the LiteSVM execution
tests. Its SHA-256 is
`b792b9099410354b8f940bb7fa9aef4bbfdb8f26b51161c5a5942884199d5bf2`,
matching the deployed prefix recorded in `devnet-deployment.json` and
`docs/DAY2.md`. It contains executable bytecode only, not deployment authority
or wallet material.
