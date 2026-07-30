# Threat model

## Security objective

PrazoPay prevents either party or the model from redirecting a funded
milestone or silently changing its deadline and terms. It does not decide the
quality of human work.

## Assets

- the locked milestone amount;
- immutable settlement destination;
- immutable refund destination;
- deadline and review-window integrity;
- terms, evidence, and feedback commitments; and
- terminal-state uniqueness.

## Trust boundary

| Component | Trusted for | Not trusted for |
|---|---|---|
| Solana runtime | instruction ordering, signatures, clock sysvar, account ownership | off-chain content quality |
| PrazoPay program | authorization, timing, state transitions, exact settlement | fiat pricing or dispute judgment |
| Human wallets | signing their own role-specific actions | changing another party's role |
| ZeroClaw/model | explaining observed state | custody, signing, time, or authorization |
| RPC endpoint | transporting finalized account data | final authority; wallets and explorers may cross-check |
| Delivery relay | durable event acknowledgement and duplicate suppression | chain state, authorization, custody, or human receipt |

## Threats and controls

| ID | Threat | Control |
|---|---|---|
| T1 | Prompt asks the agent to redirect payment | Worker and funder are immutable account fields; no redirect instruction exists |
| T2 | Attacker submits another worker's delivery | Worker signature and `has_one` constraint |
| T3 | Attacker approves a delivery | Funder signature and `has_one` constraint |
| T4 | Funder refunds after worker submitted | Refund accepts only `Open`; submission moves to `Submitted` |
| T5 | Funder ghosts after delivery | V1 requires signed silence-acceptance acknowledgement; after full review and claim grace, any trigger can settle only to the immutable Worker |
| T6 | A Worker or third party attempts premature silence settlement | Program compares Solana clock against the full review and claim-grace boundaries |
| T7 | Worker submits after deadline | Program rejects `now > due_at` |
| T8 | Funder requests endless or deadline-truncating revisions | Maximum of three; every valid v1 request opens exactly one fresh delivery window |
| T9 | Overflow changes review time | Checked integer addition |
| T10 | Settlement occurs twice | `Paid` and `Refunded` are terminal |
| T11 | ZeroClaw invents a status | Read-only tool returns structured state; chain remains authoritative |
| T12 | Raw contract or deliverable leaks on-chain | Store only fixed-size hashes |
| T13 | Public RPC lies or is stale | Output reports advisory status; settlement is enforced by the program, not the RPC response |
| T14 | Program upgrade changes rules | Public deployment evidence discloses upgrade authority; production requires a documented freeze or revocation plan |
| T15 | Discord prompt injection changes monitoring behavior | Heartbeat session-context loading is disabled; the monitor accepts a configured PDA and fresh tool output only |
| T16 | Model copies a malformed or different PDA | Tool validates base58, program owner, discriminator, and exact account length; invalid inputs fail closed |
| T17 | Repeated or ambiguous alert delivery | Polling is separated from sparse entry/boundary/escalation windows; the loopback relay commits stage-bound event IDs only after successful Discord send and suppresses committed IDs across restarts |
| T18 | Host or model fabricates an alert | Explorer link and structured tool trace are independently checkable; alerts never authorize settlement |
| T19 | Host compromise suppresses monitoring | Missing alerts are a liveness failure, not a custody failure; human wallets and the program remain authoritative |
| T20 | A legacy account is described using stronger v1 guarantees | Version bit is decoded; output labels `v0_legacy` versus `v1` and reports the actual acceptance policy |

## Known residual risks

- A malicious funder can request a revision with an unhelpful feedback hash
  before the window ends. The revision count and deterministic replacement
  delivery window are bounded, but subjective disputes are not solved.
- A malicious worker can submit a meaningless evidence hash. The funder may
  request a revision before the review window expires.
- A lost funder or worker key can strand role-specific actions.
- Native SOL settlement does not model SPL token-account edge cases.
- Upgrade authority remains a governance risk until the deployment policy is
  frozen and documented.
- ZeroClaw reported application-layer security because no OS sandbox backend
  was available. The dedicated heartbeat therefore retains only the read-only
  `prazopay_status` tool and no signing surface.
- Notifications are operational hints with at-least-once delivery. They do not
  prove that Discord displayed a message or that a human read it. A crash after
  Discord accepts a send but before the relay commits its event ID can repeat
  the message.
- The current WASM host provides no persistent storage surface to this tool.
  The loopback host-side relay supplies durable delivery acknowledgement;
  model memory is never treated as durable state.
- ZeroClaw 0.8.3 requires the single webhook alias to be enabled so its
  heartbeat can resolve the bare `webhook` target. That opens an otherwise
  unused inbound listener on the configured high port. A random encrypted HMAC
  secret protects it, while the actual relay binds only to loopback and requires
  a separate bearer check using the same local secret.

## Secret policy

Never place a seed phrase, private key, wallet JSON, bot token, model API key,
raw private contract, or raw deliverable in source control, prompts, fixtures,
logs, screenshots, or bounty submissions.
