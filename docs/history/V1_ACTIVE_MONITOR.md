# Legacy ZeroClaw active-monitor evidence

These public runs prove the earlier milestone-only monitor and preserved account
decoding. They are retained for backward-compatibility verification and are not
the current PrazoPay v2 Agreement-first workflow.

For the current design and replacement demo sequence, see
[`../ACTIVE_MONITOR.md`](../ACTIVE_MONITOR.md).

## Protocol v1 fair-settlement run

The fair-settlement demo locked exactly one lamport in a fresh v1 Milestone:

- [Milestone account](https://explorer.solana.com/address/ikUaYZUARH3KXK9y98MgfgSVsZJu3tcgHfgeKnCTTqB?cluster=devnet)
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
third-party trigger called `settle_after_review`; the Milestone became `PAID`,
and the Worker balance increased by exactly one lamport.

The next heartbeat produced one `PrazoPay Final Outcome` card:

```text
event: SETTLEMENT_SUCCESS
status: paid
outcome: success
responsible_role: both
continue_monitoring: true
```

Under the durable delivery workflow, `continue_monitoring: true` kept the final
event retryable until the relay confirmed a successful Discord send. The relay
then committed the event ID, closed the Milestone, and disabled the heartbeat.

The machine-readable record is
[`../../fixtures/devnet-fair-lifecycle.json`](../../fixtures/devnet-fair-lifecycle.json).

## Protocol v0 compatibility run

The earlier active-monitor test created and submitted a one-lamport Milestone:

- [Milestone account](https://explorer.solana.com/address/gjnjUCGw33rDXWP66ztrR8KiX13DZWStkj3jR5LuhV8?cluster=devnet)
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
`479874716`. This public Milestone is `v0_legacy`, so the next permitted role
changed immediately:

```text
event: WORKER_CLAIM_READY
responsible_role: worker
allowed_actions: approve_milestone, claim_after_review
event_id: prazopay:17194f928a067a7fbedf1b8ff0a4d6cf
```

The event changed because the chain's action boundary changed, not because the
model guessed that time had passed. This record proves backward-compatible
`v0_legacy` behavior; it is not the current v2 policy.

Its machine-readable record is
[`../../fixtures/devnet-active-monitor.json`](../../fixtures/devnet-active-monitor.json).
