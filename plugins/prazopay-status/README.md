# prazopay-status

A read-only ZeroClaw WASM tool:

```text
prazopay_status(
  cluster = "devnet",
  milestone = "<base58 PDA>",
  alert_before_secs = 300,
  poll_interval_secs = 300
)
```

The component calls only the hard-coded Solana devnet RPC endpoint. It verifies
that the account is owned by the PrazoPay program, validates the Anchor account
discriminator and exact binary length, decodes the milestone, obtains Solana
block time, and returns role-specific next actions plus a deterministic
`monitor` decision.

It decodes v0, deployed v1, and deployed v2 Milestones. A v2 Milestone
reports `worker_signed_silence_acceptance`: the Worker accepted the committed
terms before funding was allowed. The currently linked public devnet evidence
remains v1 until an explicitly authorized v2 deployment occurs.

The monitor decision contains:

- `should_notify` and `continue_monitoring`;
- an event code, severity, and responsible role;
- the status schema, on-chain Milestone protocol, acceptance policy, and sparse
  reminder stage;
- seconds to the relevant deadline boundary;
- a recommended next-check interval; and
- a stable, state-bound `event_id` for reminder correlation and downstream
  deduplication.

The component does not schedule itself. ZeroClaw's native heartbeat worker calls
the tool. Quiet results become ZeroClaw's `NO_REPLY` sentinel, so routine polls
do not produce Discord noise. Actionable output passes through the loopback
PrazoPay delivery relay before Discord.

Actionable events use at-least-once delivery. The configured interval is a poll
cadence, not a message cadence. The tool opens alert windows at state entry,
deadline boundaries, immediate readiness, 30 minutes, 2 hours, and then daily.
Other polls are quiet. Event IDs are bound to the lifecycle state and reminder
stage. The relay commits an event ID only after a successful Discord send and
suppresses that ID on later polls and after restarts.

Before Discord delivery, the relay requires the exact v2 provenance tuple
`prazopay.status.v2 / v2 / worker_signed_silence_acceptance`. It rejects the
ambiguous card label `Protocol version`, any v1 Milestone value, and the legacy
`explicit_silence_acceptance` policy. This prevents a stale installed WASM or
model-formatted legacy field from being presented as current v2 evidence.

For a v1 or v2 terminal state, the tool deliberately returns the same final
event on every poll and keeps `continue_monitoring = true`. This is a retry
contract, not an instruction to post duplicates: the relay delivers the final
event once, persists the acknowledgement, closes that milestone, and disables
the heartbeat. A terminal event therefore cannot disappear merely because the
daemon or provider was unavailable during a short notification window.

The result is advisory. The Anchor program remains the only settlement
authority.

The tool never:

- accepts a private key, seed phrase, wallet path, transaction, or arbitrary
  RPC URL;
- signs, simulates, or submits a transaction;
- writes persistent state;
- returns raw RPC data; or
- exposes raw terms, delivery evidence, or revision feedback.

Its only manifest permission is `http_client`.

Because ZeroClaw v0.8.3 creates a fresh WASM store for every call and exposes no
persistent memory/file host function to this tool, persistence belongs to the
separate host-side relay. End-to-end exactly-once delivery is still not claimed:
a host crash after Discord accepted a message but before the local event ID was
committed can cause one duplicate. Normal repeated ticks and restarts are
deduplicated.
