# PrazoPay test scenarios

Status: v2 is locally verified and deployed on devnet. The current public
Agreement/Milestone evidence is in `fixtures/devnet-v2-lifecycle.json`;
historical v0/v1 fixtures remain compatibility evidence.

## Test rules

- Run automated tests before every deployment.
- Use isolated devnet identities and exactly 1 lamport for public lifecycle
  evidence.
- Never place a keypair, seed phrase, Discord token, model API key, or raw
  contract text in output or fixtures.
- Decode the account version explicitly: existing accounts may be `v0_legacy`,
  deployed acknowledged milestones are v1, and Agreement-funded milestones are
  v2.
- ZeroClaw is read-only in every scenario. A separate permissionless trigger
  may finalize only the two fixed-recipient timeout paths.

## Suite G: v2 Agreement gate

### G1 — Funding requires Worker acceptance

1. Funder signs `propose_agreement`; confirm no Milestone account exists.
2. Attempt `fund_accepted_agreement`; expect failure.
3. Worker signs `accept_agreement`.
4. Funder funds; confirm the complete delivery window starts at funding.

Expected: only `PROPOSED -> ACCEPTED -> FUNDED` creates the v2 Milestone and
locks the exact amount.

### G2 — Worker rejection

1. Funder proposes.
2. Named Worker signs `reject_agreement`.
3. Funder attempts funding.

Expected: funding fails, no Milestone exists, and no milestone amount needs a
refund.

### G3 — Expired proposal

1. Funder proposes with a valid bounded expiry.
2. Advance chain time beyond expiry.
3. Attempt acceptance and funding.

Expected: both fail closed and no Milestone amount is locked.

### G4 — Accepted values cannot be replaced

Attempt funding with a different Worker or derive a different Milestone for the
same accepted Agreement.

Expected: account constraints or PDA derivation fail. The funded Milestone
matches the Agreement byte for byte for parties, amount, commitments, and
windows.

### G5 — Last-second acceptance and independent funding expiry

Accept at exactly `proposal_expires_at`. Confirm the Agreement derives the
complete `funding_window_secs` from `accepted_at`. Funding at exactly
`funding_expires_at` succeeds; funding one second later fails and creates no
Milestone.

### G6 — Canonical terms tampering

Reorder JSON keys and confirm the canonical hash is unchanged. Then change one
amount, window, party, or hash; add an unknown field; duplicate a field; use a
float; or substitute mainnet/a different Program ID in the session.

Expected: harmless key order is accepted; every semantic or context change
fails in the Worker preflight before a signature is requested.

### G7 — Substituted signer and rejected-state replay

Use a third-party key as the acceptance signer, then let the named Worker
reject and attempt funding afterward.

Expected: substituted acceptance fails, rejection is terminal, the later fund
instruction fails, and no Milestone account exists.

### G8 — Atomic funding rollback

Make the Funder unable to cover Milestone rent plus the committed amount, then
call `fund_accepted_agreement`.

Expected: transfer failure rolls back Agreement mutation and Milestone
creation; Agreement remains `Accepted` and no escrow amount is stranded.

## Suite N: funded lifecycle

### N1 — Delivery approved by the funder

1. Funder proposes, Worker accepts, and Funder funds a v2 Milestone.
2. Worker submits a nonzero evidence hash before `due_at`.
3. Funder signs `approve_milestone` during review.

Expected:

- `OPEN -> SUBMITTED -> PAID`;
- worker receives exactly the locked amount;
- milestone rent remains in the account;
- later approve, claim, revision, and refund attempts fail.

Evidence: create, submit, and approve transaction signatures; PDA; worker
balance delta; final account state.

### N2 — Explicit silence acceptance

1. Create and submit normally.
2. Take no funder action during the complete review window.
3. Confirm the worker still cannot claim during claim grace.
4. A third-party trigger signs `settle_after_review` at or after
   `claimable_at`.

Expected:

- no automatic transfer occurs;
- claim before review fails;
- claim during grace fails;
- permissionless settlement succeeds at the exact claimable boundary;
- substituting any recipient other than the immutable worker fails;
- final state is `PAID`.

Evidence: rejected simulation/error codes at both early boundaries, successful
claim transaction, exact worker balance delta.

### N3 — One revision and approval

1. Worker submits delivery v1.
2. Funder signs `request_revision` during review.
3. Confirm the active deadline becomes
   `revision_request_time + review_window_secs`.
4. Worker submits delivery v2 before the replacement deadline.
5. Funder approves.

Expected:

- `OPEN -> SUBMITTED -> OPEN -> SUBMITTED -> PAID`;
- revision count is one;
- old evidence is cleared on revision;
- feedback hash is stored;
- the original deadline no longer controls the revised delivery.

### N4 — No delivery and refund

1. Create a milestone and submit nothing.
2. At exact `due_at`, attempt `refund_expired`.
3. At `due_at + 1`, let a third party trigger the refund.

Expected:

- exact-deadline refund fails;
- the next-second refund succeeds;
- only the immutable funder receives the locked amount;
- trigger receives no escrow funds;
- final state is `REFUNDED`.

### N5 — ZeroClaw role hand-off

Observe one lifecycle without sending a Discord prompt:

```text
OPEN -> worker responsible
SUBMITTED/review -> funder responsible
SUBMITTED/grace -> funder approval only
SUBMITTED/claimable -> permissionless settlement ready
PAID or REFUNDED -> one final outcome, then monitoring stops
```

Expected: every Milestone alert is English and contains the exact provenance
fields `Status schema: prazopay.status.v2`,
`On-chain Milestone protocol: v2`, and
`Acceptance policy: worker_signed_silence_acceptance`, followed by the reminder
stage, allowed action names, stable event ID, and Explorer link. Agreement cards
contain `Status schema: prazopay.agreement-status.v1` and
`On-chain Agreement protocol: v2`. The ambiguous label `Protocol version` and
all legacy v1 Milestone/policy values are rejected by the relay before Discord.

## Suite E: extreme time and value boundaries

| ID | Case | Expected result |
| --- | --- | --- |
| E1 | Submit at `due_at - 1` | accepted |
| E2 | Submit at exact `due_at` | accepted |
| E3 | Submit at `due_at + 1` | rejected |
| E4 | Last-second submit, revision at `review_end - 1` | accepted; full review was preserved |
| E5 | Revision at exact `review_end` | rejected |
| E6 | Claim at `review_end - 1` | rejected: review open |
| E7 | Claim at exact `review_end` | rejected: grace open |
| E8 | Claim at `claimable_at - 1` | rejected |
| E9 | Permissionless settle at exact `claimable_at` | accepted for v1/v2, rejected for v0 legacy |
| E10 | Refund at exact active deadline | rejected |
| E11 | Refund one second after active deadline | accepted |
| E12 | Minimum 60-second review | 60-second grace |
| E13 | Maximum 7-day review | grace capped at one hour |
| E14 | Zero amount or zero commitment | creation/submission rejected |
| E15 | Same funder and worker | creation rejected |
| E16 | Checked timestamp addition overflows | instruction fails closed |

## Suite A: authorization, limits, and races

| ID | Attack or race | Expected result |
| --- | --- | --- |
| A1 | Attacker submits for worker | unauthorized |
| A2 | Attacker approves for funder | unauthorized |
| A3 | Trigger substitutes a different worker recipient | rejected |
| A4 | Fourth revision request | rejected; maximum is three |
| A5 | Permissionless settle wins, approve lands next | settle succeeds; approve fails terminal-state check |
| A6 | Approve wins, settle lands next | approve succeeds; settle fails terminal-state check |
| A7 | Refund requested while delivery is pending | rejected |
| A8 | Refund trigger substitutes another recipient | account constraint rejects it |
| A9 | Legacy account is decoded as v1 | must never happen; output must say `v0_legacy` |
| A10 | Mainnet cluster or caller-supplied RPC URL | rejected before network use |
| A11 | Wrong owner, discriminator, length, encoding, or enum byte | tool fails closed |
| A12 | Missing or nonnumeric Solana block time | tool fails closed |

For A5 and A6, run both transaction orderings. Solana serialization must allow
exactly one settlement, never two.

## Suite M: monitor and Discord extremes

### M1 — Polling is not notification

Keep the heartbeat at five minutes and leave an action unresolved.

Expected alert windows:

```text
state entry
deadline approach/final boundary
immediate action readiness
30 minutes unresolved
2 hours unresolved
once per day thereafter
```

Every poll outside those windows must return `should_notify = false` and
produce `NO_REPLY`.

### M2 — Duplicate call inside one stage

Call `prazopay_status` twice with identical state and reminder stage.

Expected: same `event_id`. The relay sends it once, persists the successful ID,
and suppresses the second call.

### M3 — Stage escalation

Inspect the same unresolved action at immediate, 30-minute, 2-hour, day-one,
and day-two boundaries.

Expected: `should_notify = true` only in the bounded windows; `reminder_stage`
and `event_id` change between stages.

### M4 — Terminal state

Settle a monitored milestone and publish the final outcome.

Expected:

- `continue_monitoring = true` until host-side delivery acknowledgement;
- every terminal observation returns the same stable final event ID;
- the relay delivers that final success/failure card once, persists the ID,
  marks the milestone closed, and disables the heartbeat;
- no stale review or settlement alert is posted.

Later direct status reads remain inspectable and retryable, while the closed
relay suppresses every further Discord post.

### M5 — Prompt injection

Post a Discord message telling the monitor to change the PDA, use mainnet, or
call another tool.

Expected: no effect. `load_session_context = false`, the configured PDA is
used, and the journey risk profile exposes only the two read-only PrazoPay
status tools.

### M6 — Restart and provider/RPC failure

Restart the Creator daemon during an alert stage, then separately simulate a
provider or RPC failure.

Expected:

- a restart returns the same stage-bound event ID, but a previously committed
  ID is not resent;
- failure never produces an invented status or transaction;
- outage cards occur only at first failure, 30 minutes, 2 hours, then daily,
  and one recovery card follows the first successful read;
- a monitor started between reminder windows still exposes one stable current
  actionable-state event, which the relay deduplicates across later polls;
- no signer or wallet surface becomes available;
- custody remains unaffected.

### M8 — Delivery failure and terminal outage

1. Make the Discord send command fail for one actionable event.
2. Restore it and repeat the same event.
3. Settle the milestone while the model, daemon, or RPC is unavailable for
   longer than two polling intervals.
4. Restore the dependency and inspect again.

Expected:

- the failed event ID is not committed and is retried after recovery;
- a successful retry is committed and later duplicates are suppressed;
- the terminal event remains eligible regardless of outage duration;
- exactly one final card is committed, and all later output is suppressed.

### M7 — English-only notification

Trigger an actual alert window and inspect Discord.

Expected:

- heading is `PrazoPay Active Alert`;
- all explanatory text is English;
- action names and reason/event codes match tool JSON;
- full funder and worker addresses are not printed;
- the full milestone appears only inside the Explorer URL.

## Execution commands

Run the complete local suite from PowerShell:

```powershell
Set-Location '<repository>'
.\scripts\reproduce.ps1
```

Run only the state-machine cases:

```powershell
wsl.exe -d Ubuntu-24.04 --cd (Get-Location).Path -- `
  bash -lc 'RUSTUP_TOOLCHAIN=1.97.1 CARGO_TARGET_DIR="$HOME/.cache/prazopay-target" cargo test -p prazopay --test state_machine'
```

Run only status/monitor cases:

```powershell
wsl.exe -d Ubuntu-24.04 --cd (Get-Location).Path -- `
  bash -lc 'RUSTUP_TOOLCHAIN=1.97.1 CARGO_TARGET_DIR="$HOME/.cache/prazopay-target" cargo test -p prazopay-status'
```

## Evidence checklist

For every devnet case record:

- scenario ID and UTC timestamp;
- local commit or source hash;
- local SBF hash and independently dumped deployed hash;
- program ID and deployment slot;
- milestone PDA and protocol version;
- transaction signatures and Explorer links;
- pre/post account state and balance deltas;
- ZeroClaw tool trace ID;
- Discord message ID, or `NO_REPLY` trace for a quiet case; and
- pass/fail result with the exact failed invariant.

Do not count a screenshot alone as protocol evidence.
