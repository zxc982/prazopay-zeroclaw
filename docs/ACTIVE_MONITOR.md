# ZeroClaw active monitor

Date: 2026-07-30

## What ZeroClaw does

PrazoPay's Solana program enforces custody, roles, deadlines, review windows,
and settlement. ZeroClaw supplies the missing operational loop:

```text
ZeroClaw heartbeat
  -> read one public devnet milestone through prazopay_status
  -> validate owner, discriminator, length, state, and Solana block time
  -> derive the currently responsible role and allowed protocol actions
  -> suppress quiet observations with NO_REPLY
  -> deliver an actionable English Discord alert at a policy window
  -> wait for the correct human wallet to sign
  -> observe the resulting state transition on the next heartbeat
  -> deliver one terminal SUCCESS or FAILED card, then stop
```

This is not a chatbot waiting to be mentioned. The schedule, native WASM tool
invocation, runtime trace, quiet sentinel, and Discord delivery all belong to
ZeroClaw.

## Current v1 live devnet proof

The fair-settlement demo locked exactly one lamport in a fresh v1 milestone:

- [milestone account](https://explorer.solana.com/address/ikUaYZUARH3KXK9y98MgfgSVsZJu3tcgHfgeKnCTTqB?cluster=devnet)
- [create transaction](https://explorer.solana.com/tx/2Eaf8P85jm5YhfsRg9akqKGgMqHf44BZ9PWxXbigSLKkUQgc1hRJAonr5Hx9UZZgmDpM3eSfyc5qzXPk2YjrA8cY?cluster=devnet)
- [submit transaction](https://explorer.solana.com/tx/3KoickzBmXxBbWpEpPn96CvnbpvW2po2Yz9ZdWA8162ZDCJWWvEuJa9EComt9mcsUrDZuc64Q7kJEata3rqUQh4p?cluster=devnet)
- [permissionless settlement transaction](https://explorer.solana.com/tx/2AZLiK1TaQ3GRFWpvkkbvHaQhXQJyr4Kz4TywZHJgKYCnkY3hJtyhDSqTvVZbjAYt3MqUUaLvFQbvKS12TJAQrBJ?cluster=devnet)

After the Funder review window and claim grace elapsed, the Creator heartbeat
called the native status component without a Discord prompt and produced:

```text
event: PERMISSIONLESS_SETTLEMENT_READY
responsible_role: permissionless_trigger
allowed_actions: approve_milestone, claim_after_review, settle_after_review
worker_overdue: false
```

The alert explained that any account may trigger settlement, while the Solana
program can transfer the locked amount only to the immutable Worker. A
third-party trigger then called `settle_after_review`; the milestone became
`PAID`, and the Worker balance increased by exactly one lamport.

The next heartbeat produced one `PrazoPay Final Outcome` card:

```text
event: SETTLEMENT_SUCCESS
status: paid
outcome: success
responsible_role: both
continue_monitoring: true
```

The card named both shortened parties and stated that it was the final
notification. Under the durable delivery workflow, `continue_monitoring: true`
keeps the same final event retryable until the loopback relay confirms a
successful Discord send. The relay then commits the event ID, closes that
milestone, and disables the heartbeat. Both Creator and Worker Discord channel
doctors remained healthy.

The machine-readable record is
[`../fixtures/devnet-fair-lifecycle.json`](../fixtures/devnet-fair-lifecycle.json).

## Historical v0 compatibility proof

The active-monitor test created and submitted a new milestone that locked one
lamport:

- [milestone account](https://explorer.solana.com/address/gjnjUCGw33rDXWP66ztrR8KiX13DZWStkj3jR5LuhV8?cluster=devnet)
- [create transaction](https://explorer.solana.com/tx/hSUHxE8rsS7b1KzeYqQPgNB5wKqGRw3huZ4KP4RzS6xUgbMFmcv4D8hB8oLnTVUPzpvfBAp8gQHUmp2Xz31Lvzk?cluster=devnet)
- [submit transaction](https://explorer.solana.com/tx/2mfcM66ZGJJucrCgpW7yVWBj5boxd6v46myx84d1aU3VEUvgpqx1KcDqWHJpb8Qa7mBVVeKsSH7jGigqCe6FrYrb?cluster=devnet)

Without a Discord prompt, the Creator heartbeat read slot `479874165` and
emitted:

```text
event: FUNDER_REVIEW_REQUIRED
responsible_role: funder
allowed_actions: approve_milestone, request_revision
event_id: prazopay:21498f1ac097a294b16e17570ba0fe6a
Discord message: 1532213551857668266
```

After the immutable review boundary elapsed, a later heartbeat read slot
`479874716`. This public milestone is `v0_legacy`, so the next permitted role
changed immediately:

```text
event: WORKER_CLAIM_READY
responsible_role: worker
allowed_actions: approve_milestone, claim_after_review
event_id: prazopay:17194f928a067a7fbedf1b8ff0a4d6cf
```

The event changed because the chain's action boundary changed, not because the
model guessed that time had passed.

This earlier run documents backward-compatible `v0_legacy` behavior. It is not
the current v1 silence-settlement policy.

Its machine-readable record is
[`../fixtures/devnet-active-monitor.json`](../fixtures/devnet-active-monitor.json).

## Least-authority runtime

The dedicated Creator heartbeat is configured with:

```text
agent: creator
interval: 5 minutes
two_phase: false
load_session_context: false
allowed_tools: [prazopay_status]
delivery: ZeroClaw webhook -> loopback relay -> ZeroClaw Discord send
```

Disabling Discord session-context loading prevents channel messages from
becoming monitor instructions. Disabling the speculative first phase ensures
each tick reaches the deterministic status tool. The risk profile retains one
read-only tool: no shell, browser, memory write, wallet, signer, transaction
builder, or scheduler mutation is available.

Install or replace a monitor:

```bash
./scripts/zeroclaw-prazopay-monitor.sh install \
  <MILESTONE_PDA> <DISCORD_CHANNEL_ID> 5 300 \
  "$HOME/.config/zeroclaw-entrega/creator"
```

Restart the Creator daemon after configuration. ZeroClaw v0.8.3 waits one full
configured interval before its first heartbeat tick.

## Honest delivery semantics

The five-minute value is a **polling interval**, not a promise to send a message
every five minutes. The status tool uses Solana time and the supplied poll
interval to open sparse alert windows:

- state entry;
- approaching and final deadline boundaries;
- immediate action readiness;
- first delay, 30-minute, and 2-hour unresolved-action escalation; and
- one window per day thereafter.

Worker delivery delay and Funder review delay use the same sparse schedule.
Once v1 silence settlement becomes permissionless, the Worker is not described
as late for failing to submit the settlement transaction.

All other polls return `should_notify = false` and become `NO_REPLY`. Every
outbound monitor message is English. A stage-bound `event_id` lets the relay
deduplicate retries within the same alert window and after process restarts.

Notifications remain **at least once**, not exactly once. ZeroClaw v0.8.3
creates a fresh WASM store for each tool call and exposes no persistent
file/memory host function to this plugin. The host-side PrazoPay relay therefore
accepts only authenticated loopback webhook output, sends it with ZeroClaw's
Discord channel command, and atomically commits the event ID only after that
command succeeds. The state survives relay and daemon restarts.

The model cannot prove that a previous Discord delivery was seen. A crash in
the narrow interval after Discord accepts the message but before the relay
commits its local state can still repeat an alert; exactly once is not claimed.
A provider or RPC outage delays delivery instead of silently exhausting a
terminal notification window. None of those failures can redirect or release
escrow because ZeroClaw has no signing capability and the Solana program remains
the sole settlement authority.

When the milestone becomes terminal, the tool returns
`continue_monitoring = true` with the same stable final event on every
observation. After a successful Discord send, the relay persists the
acknowledgement, marks the milestone closed, disables the heartbeat, and
suppresses every later message. No review, delay, or terminal reminder is sent
after that final card.

## Judge demo sequence

1. Open the live milestone in Solana Explorer and show `SUBMITTED`.
2. Show the Creator policy retaining only `prazopay_status`.
3. Start the Creator daemon and do not send a Discord message.
4. Wait for the proactive English alert and point out the protocol version,
   reminder stage, event ID, responsible role, allowed actions, and Explorer
   link.
5. Let any test trigger submit `settle_after_review`; point out that the
   immutable Worker is the only possible recipient.
6. Show the real transaction and exact Worker balance delta in Explorer.
7. On the next heartbeat, show one final two-party SUCCESS card.
8. Show the relay's closed state and confirm that no further reminders are
   emitted.
