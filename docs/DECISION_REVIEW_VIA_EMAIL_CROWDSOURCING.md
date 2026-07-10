# Deferred idea: review delivery transports after the core promotion contract

> Updated 2026-07-10 by operator direction. The filename is retained for history. Email
> crowdsourcing is one possible later delivery mechanism, not a Canon v1 decision or dependency.

## Decision now

Canon v1 builds only the transport-neutral review and promotion substrate:

1. stable review-item and candidate IDs bound to project, policy, evidence, and registry digests;
2. self-contained evidence-waterfall packets suitable for offline or external presentation;
3. structured decision import with explicit match/new/distinct/relation/defer actions;
4. immutable decision-ledger receipts, stale/tampered-input refusals, audit, and promotion;
5. deterministic replay showing exactly what accepted knowledge changes.

No email sender, mailbox reader, UI, ticketing system, routing policy, or active learner is required
for core acceptance. P04/P07 artifacts must remain usable by CLI and files alone.

## Delivery options retained for later

- Agent-orchestrated email crowdsourcing to the domain expert best placed to answer.
- A third-party UI or OpenRefine-compatible reconciliation client.
- Ticketing/approval systems, batch CSV review, or another offline workflow.
- A custom first-party UI, only if future operator evidence justifies its maintenance.
- No additional delivery layer: operators may keep using review export/import directly.

Canon should expose stable artifacts and imports that let these clients exist without embedding any
transport. A later delivery choice must not become a second promotion authority.

## If email is selected later

Create a separate sidecar/integration packet covering responder authorization, disclosure policy,
recipient routing, redaction, thread and candidate binding, structured and ambiguous reply parsing,
spoof/tamper detection, stale-snapshot refusal, idempotency, duplicate replies, bounce/timeout and
reassignment behavior, audit receipts, retention, and no-mutation failure paths. Run real-service
E2E tests in addition to frozen protocol fixtures.

## Decision gate

Revisit delivery only after the core review artifact, import, ledger, promotion, and sec10d journey
pass acceptance. Choose from observed review volume, routing needs, privacy constraints, and operator
workflow—not from architectural speculation. `bd-1bjn` records this deferred option space at P4.
