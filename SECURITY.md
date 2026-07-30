# Security

## Supported scope

PrazoPay is a devnet demonstration. The repository does not provide a mainnet
deployment, custody service, hosted signer, or production payment service.

Security reports should identify the affected commit, component, expected
invariant, reproduction steps, and concrete impact. Do not include private
keys, seed phrases, API keys, bot tokens, or confidential customer data.

## Trust boundary

- The Solana program controls escrow state transitions and recipients.
- Human wallets sign funding, delivery, and review instructions.
- ZeroClaw and the WASM status tool are read-only coordinators.
- The Discord relay is an availability and deduplication layer, not a
  settlement authority.
- RPC, model, host, and Discord compromise can affect observation or delivery,
  but cannot redirect escrow without an authorized Solana instruction.

See [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md) for the detailed model and
explicit non-goals.
