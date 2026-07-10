# Decision: review/adjudication is agent-orchestrated email crowdsourcing — no UI

> 2026-07-10 (operator). Sets the delivery model for P04 (unresolved inbox / review flywheel)
> and P07 (operator review experience). Reinforces retiring the OpenRefine GUI interchange.

## Decision
Canon ships **no review UI, ever.** Adjudication is a **crowdsourcing** loop, not a screen:

1. The **native active-learning query strategy** (bd-tv1m) ranks the unresolved inbox by expected
   label value → produces **promotion candidates** (uncertain clusters/links near the decision
   boundary, impact-weighted).
2. An **agent** (the one operating Canon) **routes each candidate by email to the right human**
   to adjudicate — the person with domain knowledge of that entity/sector, not one central
   reviewer. Many humans, each touching the few candidates they're best placed to judge.
3. The human **replies with a decision**; the reply is parsed back into a Canon review label
   bound to the run/policy/registry snapshot; promotion stays the single Canon-authoritative path.
4. Labels feed back into the active learner → the queue re-ranks → the flywheel compounds.

## Consequences for the plan
- The "review artifact" beads (bd-14m6 standalone cluster/link review artifact; bd-fung ranked
  inbox groups → shared offline artifact) are **email-able adjudication packets**, not UI views:
  a self-contained, stable-ID, evidence-waterfall packet a human can judge from an inbox, with a
  structured reply contract. Design them for email/agent round-trip, not for a browser.
- P07 "operator review experience" = the **packet + routing + reply-parsing contract**, no GUI.
- **Agent-friendliness is the priority for the CLI/interfaces** (P10, bd-1ihg agent-readable
  lookup/planning; bd-1ihg + the CLI ergonomics pass). The operator of this loop is an agent;
  the humans only ever see an email with a decision to make and a way to answer.
- The OpenRefine review-GUI interchange (bd-z4by, already retired) is doubly moot: no UI, and the
  review surface is email packets an agent generates and routes.

## Why this fits the thesis
Uncertain evidence, distributed human judgment, deterministic promoted knowledge. The crowd
supplies scarce judgment where the engine abstains; the engine keeps the exact/reviewed/replayable
guarantees. It scales review by routing to many experts instead of bottlenecking on one console.
