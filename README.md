# PrazoPay

**Deadline-native milestone escrow with proactive ZeroClaw operations.**

[![CI](https://github.com/zxc982/prazopay-zeroclaw/actions/workflows/ci.yml/badge.svg)](https://github.com/zxc982/prazopay-zeroclaw/actions/workflows/ci.yml)

PrazoPay turns a freelance milestone into an immutable Solana state machine:
the funder locks the worker, amount, terms hash, initial delivery deadline, and
review window before work begins. The signed creation instruction must
explicitly acknowledge silence-based acceptance. Every on-time submission
starts a complete review window. If no review action occurs, a deterministic
claim grace period follows before any permissionless trigger may settle the
exact amount only to the immutable worker. A revision opens a new delivery
window of the same duration as the agreed review window. If no delivery is
pending when the active deadline expires, the locked amount returns only to
the original funder.

> Lock the money. Lock the deadline. Remove the ghosting.

## Why this is not another payment bot

The current ZeroClaw bounty field already contains Solana Pay invoice
generators, receipt monitors, unsigned transfers, and x402 micro-spending.
PrazoPay addresses a different failure mode: **post-delivery payment delay**.
Its differentiator is an on-chain deadline and review protocol, not another way
for an agent to hold or send money.

ZeroClaw is a coordinator, never a custodian:

- the Anchor program is the settlement authority;
- human wallets sign funding, delivery, and review actions;
- permissionless triggers may finalize an expired refund or acknowledged
  silence settlement, but can never choose the recipient;
- the ZeroClaw WASM tool is read-only and returns state plus possible next
  actions;
- a native ZeroClaw heartbeat workflow watches the public milestone, suppresses
  quiet polls, and pushes role-specific Discord alerts when review, settlement,
  or refund becomes actionable;
- a loopback delivery relay persists successful event IDs, suppresses duplicate
  heartbeat output across restarts, and closes monitoring only after the single
  terminal Discord card is accepted;
- the model cannot change the funder, worker, amount, terms hash, deadline, or
  review window (the active delivery deadline advances only through the
  program's deterministic revision rule); and
- no wallet key, seed phrase, or signing endpoint is exposed to ZeroClaw.

## Day 1 acceptance suite

The local LiteSVM and state-machine suite demonstrates:

1. a funder creates and funds one milestone;
2. the stored parties, amount, terms hash, initial deadline, review window, and
   explicit silence-acceptance policy match the signed instruction;
3. only the worker can submit an evidence hash before the due date;
4. only the funder can approve or request a revision;
5. a last-second submission still receives the complete review window;
6. only v1 milestones can be permissionlessly settled after both review and
   claim grace, and the immutable worker is the only possible recipient;
7. a bounded revision receives a fresh delivery window;
8. an open, unsubmitted milestone can refund only after the active deadline;
   and
9. every terminal path releases the locked amount exactly once.

To reproduce the exact deployed SBF from the original deployment workspace,
run the Windows/WSL check below. It intentionally expects the ignored
`target/deploy/prazopay-keypair.json`; that file is not published.

```powershell
.\scripts\day1-check.ps1
```

For a clean checkout that does not contain the ignored deployment keypair, run
the repository verification suite instead:

```powershell
.\scripts\repo-check.ps1
```

It checks formatting, all Rust workspace tests, the WASI Preview 2 status
component, Bash syntax, and the durable relay's Python tests. If `wasm-tools`
is installed, it also validates the compiled component.

## Verified devnet evidence

The exact Day 1 SBF is deployed at
[`DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm`](https://explorer.solana.com/address/DjdT1wW8zEoK395yujT5ujBsDboBUFyx5LCfLBSwxAjm?cluster=devnet).
Independent one-lamport milestones exercised revision plus approval,
silent-review settlement, and permissionless expiry refund. A real ZeroClaw v0.8.3
turn then loaded `prazopay_status` and read the finalized `PAID` account from
devnet.

The active-monitor extension adds a machine-readable alert decision, an
agent-owned heartbeat workflow, and a durable host-side delivery acknowledgement
layer. This makes ZeroClaw the operational layer: it observes finalized chain
state on schedule, classifies the next permitted human action, and delivers the
alert without ever becoming the signer or settlement authority.

See the [live active-monitor record](docs/ACTIVE_MONITOR.md),
[`docs/DAY2.md`](docs/DAY2.md), and the public [`fixtures/`](fixtures/)
evidence.

Protocol v1 is deployed at slot `479993358`; the deployed prefix matches the
local SBF SHA-256
`b792b9099410354b8f940bb7fa9aef4bbfdb8f26b51161c5a5942884199d5bf2`.
Legacy accounts retain their original worker-signed claim timing.

The public evidence is independently inspectable without a PrazoPay wallet or
ZeroClaw configuration:

- [current v1 milestone](https://explorer.solana.com/address/ikUaYZUARH3KXK9y98MgfgSVsZJu3tcgHfgeKnCTTqB?cluster=devnet);
- [creation transaction](https://explorer.solana.com/tx/2Eaf8P85jm5YhfsRg9akqKGgMqHf44BZ9PWxXbigSLKkUQgc1hRJAonr5Hx9UZZgmDpM3eSfyc5qzXPk2YjrA8cY?cluster=devnet);
- [delivery transaction](https://explorer.solana.com/tx/3KoickzBmXxBbWpEpPn96CvnbpvW2po2Yz9ZdWA8162ZDCJWWvEuJa9EComt9mcsUrDZuc64Q7kJEata3rqUQh4p?cluster=devnet); and
- [permissionless settlement](https://explorer.solana.com/tx/2AZLiK1TaQ3GRFWpvkkbvHaQhXQJyr4Kz4TywZHJgKYCnkY3hJtyhDSqTvVZbjAYt3MqUUaLvFQbvKS12TJAQrBJ?cluster=devnet).

## Repository layout

```text
programs/prazopay/          Anchor program and LiteSVM tests
plugins/prazopay-status/    Read-only ZeroClaw WASM status tool
clients/prazopay-devnet/    Three-path devnet lifecycle runner
scripts/zeroclaw-prazopay-monitor.sh
fixtures/prazopay-v1.so     Exact SBF used by clean-room LiteSVM tests
docs/PROTOCOL.md            State machine and invariants
docs/THREAT_MODEL.md        Assets, trust boundary, threats, controls
docs/COMPETITIVE_POSITIONING.md
docs/TEST_SCENARIOS.md      Normal, boundary, adversarial, and monitor suites
docs/ACTIVE_MONITOR.md      Proactive devnet-to-Discord evidence
docs/DAY1.md                Day 1 checklist and evidence
docs/DAY2.md                Devnet deployment and real ZeroClaw evidence
```

## Safety boundary

Day 1 is localnet-only. Day 2 uses isolated devnet test identities and
one-lamport milestones. Nothing here deploys to mainnet, handles a real-value
asset, exposes a signing interface to ZeroClaw, or submits a bounty entry.
Publishing this source and its public devnet evidence does not grant access to
any wallet, Discord bot, model provider, or deployment authority.
