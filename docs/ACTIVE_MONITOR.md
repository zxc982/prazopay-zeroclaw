# ZeroClaw v2 active monitor

Date: 2026-08-01

## Evidence scope

PrazoPay v2 has a finalized public Agreement-to-paid-Milestone lifecycle. The
checked-in monitor, relay, WASM tools, and native `prazopay` ZeroClaw Skill
implement the matching read-only journey. A fresh v2 Discord journey trace will
be captured with the replacement demo video; it is not represented by the
legacy screenshots or message IDs.

Historical v1 and v0 monitor runs remain available only as
[legacy compatibility evidence](history/V1_ACTIVE_MONITOR.md). They are not the
current Agreement-first policy.

## Agreement-to-Milestone journey

Install or replace the v2 journey monitor:

```bash
bash ./scripts/zeroclaw-prazopay-agreement-monitor.sh \
  install AGREEMENT_PDA DISCORD_CHANNEL_ID
```

The heartbeat reads finalized Agreement state through
`prazopay_agreement_status` and produces compact English cards for:

- `PROPOSED`: the Worker verifies the committed terms and may accept or reject;
- `ACCEPTED`: only the Funder may fund before the independent funding expiry;
- `REJECTED` or derived expiry: one final no-funds-locked card; and
- `FUNDED`: the same heartbeat follows the exact Milestone PDA stored in the
  Agreement and calls `prazopay_status` automatically.

If a funded Milestone has no actionable alert yet, the relay emits one
deduplicated, non-terminal `Escrow Funded` handoff card. Monitoring continues
without asking a human to reinstall it. The journey closes only after an
Agreement is rejected or expires, or the linked Milestone reaches one terminal
outcome.

The Agreement tool reports `milestone_created`, not `funds_locked`, because
only the linked Milestone tool can determine whether escrow remains live after
funding.

## What ZeroClaw does

PrazoPay's Solana program enforces custody, roles, deadlines, review windows,
and settlement. ZeroClaw supplies the operational loop:

```text
ZeroClaw heartbeat
  -> read the public v2 Agreement through prazopay_agreement_status
  -> validate owner, discriminator, length, state, and Solana time
  -> follow the linked Milestone through prazopay_status after funding
  -> derive the responsible role and currently permitted actions
  -> suppress quiet observations with NO_REPLY
  -> deliver one actionable English Discord card at a policy window
  -> wait for the correct human wallet or permissionless trigger
  -> observe the finalized state transition on the next heartbeat
  -> deliver one terminal SUCCESS or FAILED card, then stop
```

This is not a chatbot waiting to be mentioned. ZeroClaw owns the schedule,
native WASM invocation, trace, quiet sentinel, and channel send. The loopback
relay owns only durable delivery acknowledgement and duplicate suppression.

## Least-authority runtime

The dedicated Creator heartbeat uses:

```text
agent: creator
skill bundle: [prazopay]
interval: 5 minutes
two_phase: false
load_session_context: false
allowed_tools: [prazopay_agreement_status, prazopay_status]
delivery: ZeroClaw webhook -> loopback relay -> ZeroClaw Discord send
```

Disabling Discord session-context loading prevents channel messages from
becoming monitor instructions. Disabling the speculative first phase ensures
each tick reaches the deterministic status tool. The risk profile exposes only
the two read-only PrazoPay tools: no shell, browser, memory write, wallet,
signer, transaction builder, broadcaster, or scheduler mutation is available.

Enable the checked-in Skill bundle with:

```bash
./scripts/zeroclaw-prazopay-skill.sh enable \
  "$HOME/.config/zeroclaw-entrega/creator"
```

Restart the Creator daemon after configuration. ZeroClaw v0.8.3 waits one full
configured interval before its first heartbeat tick.

## Honest delivery semantics

The five-minute value is a **polling interval**, not a promise to send a message
every five minutes. The status tools use finalized Solana time and open sparse
alert windows only at:

- state entry;
- approaching and final deadline boundaries;
- immediate action readiness;
- first delay, 30-minute, and 2-hour unresolved-action escalation; and
- one window per day thereafter.

Worker delivery delay and Funder review delay use the same sparse schedule. All
other polls return `should_notify = false` and become `NO_REPLY`. Every outbound
monitor message is English. A stage-bound `event_id` lets the relay deduplicate
retries within the same alert window and after process restarts.

Notifications remain **at least once**, not exactly once. ZeroClaw v0.8.3
creates a fresh WASM store for each tool call and exposes no persistent
file/memory host function to the plugins. The host-side relay therefore accepts
only authenticated loopback webhook output, sends it with ZeroClaw's Discord
channel command, and atomically commits the event ID only after that command
succeeds.

A crash after Discord accepts a message but before the relay commits its local
state can repeat an alert; exactly once is not claimed. A provider or RPC outage
generates a sparse degraded alert at the first failure, 30 minutes, 2 hours,
then daily, followed by one recovery card after a successful read. Those
failures cannot redirect or release escrow because ZeroClaw has no signing
capability and the Solana program remains the sole settlement authority.

When the journey becomes terminal, the tool keeps the stable final event
retryable until Discord delivery succeeds. The relay then persists the
acknowledgement, marks the journey closed, disables the heartbeat, and
suppresses every later message.

## Replacement demo sequence

1. Show the public v2 Agreement in `PROPOSED` and its complete terms commitment.
2. Show the proactive Worker acceptance card without mentioning the bot.
3. Let the immutable Worker sign `accept_agreement`; show `ACCEPTED` on-chain.
4. Show the Funder funding card and atomic `fund_accepted_agreement`
   transaction that creates the linked Milestone.
5. Show the non-terminal `Escrow Funded` handoff and continued monitoring.
6. Let the Worker sign `submit_delivery`; show the Funder review card.
7. Let the Funder approve, or demonstrate the agreed silence path with a
   permissionless trigger that cannot change the Worker recipient.
8. Show the real terminal transaction, exact Worker balance delta, one final
   two-party SUCCESS card, and the relay's closed state.
