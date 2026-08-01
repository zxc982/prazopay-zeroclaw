# Competitive positioning

Snapshot date: 2026-07-29

## Crowded lanes

Public ZeroClaw bounty projects already cover:

- Solana Pay invoice/QR creation;
- confirmed payment monitoring;
- unsigned SOL/SPL transfer construction;
- token-risk checks;
- durable-nonce transfers; and
- x402 metered API settlement.

PrazoPay should not claim novelty in any of those lanes.

## PrazoPay's lane

| Question | Existing payment tools | PrazoPay |
|---|---|---|
| Primary problem | How to request, observe, or execute a payment | How to make a funded work deadline enforceable |
| Money timing | Payment happens now or after human signing | Worker first accepts committed terms; only then can the Funder lock funds and start delivery time |
| Ghosting | Usually outside scope | Worker-signed silence rule, full review, claim grace, then fixed-recipient settlement |
| Missed deadline | Usually outside scope | Deterministic refund when no submission is pending |
| Terms | Invoice fields or spend policy | Immutable terms commitment plus lifecycle |
| Agent authority | May build, monitor, or micro-spend | Read-only coordination; program owns settlement rules |
| Agent workflow | Often request/response | Native heartbeat assigns the next human role; five-minute polls produce only state-entry, boundary, and sparse escalation alerts |
| Demo proof | Payment or receipt | Competing terminal paths and forbidden transitions |

## Judge-facing sentence

> PrazoPay is not a wallet tool. It is a two-party deadline protocol: the
> Funder proposes committed terms, the named Worker must sign acceptance, and
> only then can SOL be locked and delivery time begin. After submission, the
> program guarantees the accepted review and grace windows and enforces the
> fixed Worker payout or Funder refund. ZeroClaw coordinates and escalates the
> workflow without ever holding a key.

## Evidence required before submission

- real Anchor program, not a simulated ledger;
- LiteSVM tests for both payout paths and the expiry refund path;
- unauthorized and too-early/too-late failure tests;
- read-only Rust/WASI Preview 2 ZeroClaw tool;
- one real ZeroClaw invocation;
- one no-prompt ZeroClaw heartbeat alert plus a terminal `NO_REPLY` control;
- one devnet lifecycle with explorer links;
- concise threat model and custody statement; and
- a demo under three minutes showing the state transitions, not slides alone.
