# Deferred decision space: matcher techniques and interoperability after core acceptance

> Updated 2026-07-10 by operator direction. The filename is retained as historical context, but
> this document no longer commits Canon to porting or integrating any named matcher. P00-P12 and
> the sec10d migration proceed without Dedupe, Splink, OpenRefine, active learning, or email review.

## Decision now

1. Build and empirically accept the domain-neutral native core first: exact replay, bounded native
   retrieval, typed evidence, hard anti-merge controls, calibration, abstention, review artifacts,
   promotion, and immutable registry accretion.
2. Keep the evidence IR and project boundaries expressive enough that a future native technique,
   bounded external matcher, or reconciliation client can contribute candidates or evidence with
   zero promotion authority.
3. Defer P09 and all technique-specific work until the final core acceptance has produced real
   failure slices, scale measurements, and operator demand. These options must not block v1.
4. Do not claim parity with a third-party tool merely because one underlying algorithm is known.
   Any later replacement claim requires a source-pinned behavioral inventory and differential proof.

## Decision options retained for later

### Option A — port a technique or model into Rust

Use `cmdrvl-tabfm` as the local precedent, not an informal rewrite. A future Dedupe, Splink, or
other port starts in a separately scoped project or packet with:

- pinned upstream commit, dependencies, license, and source hashes;
- a behavior/spec inventory covering preprocessing, blocking, scoring, clustering, missingness,
  training, serialization, randomness, and failure semantics;
- a pinned reference oracle with frozen input/output and intermediate fixtures;
- a layered differential-parity ladder and measured numeric/determinism floor;
- provenance, discrepancy, performance, and negative-evidence ledgers;
- adversarial, metamorphic, scale, and second-machine replay gates;
- an explicit decision about which capabilities are intentionally not ported.

Only the typed candidate/evidence output crosses into Canon. The port cannot write registries,
review decisions, or promotion receipts.

### Option B — bounded external matcher adapter

Implement the generic adapter contract only when an operator brings a real model or workflow whose
maintenance cost is justified by measured lift. Pin tool/model/config/input digests, sandbox the
runner, validate typed output, and compare it against the same sealed P12 corpus. Absence or failure
of the adapter must leave native behavior and registry state unchanged.

### Option C — standard reconciliation interoperability

Expose or consume a transport-neutral reconciliation protocol, such as the W3C Entity
Reconciliation API used by OpenRefine, without making OpenRefine a dependency or authority. This is
an adoption/client option: candidates and human judgments still enter Canon through stable IDs,
review imports, decision ledgers, audit, and promotion.

### Option D — no integration

If the accepted native core meets quality, scale, and review-cost goals, keep named-tool work
unimplemented. The generic artifacts remain sufficient future optionality.

## Evidence that triggers a later decision

A specific option may move out of backlog only when at least one of these exists:

- a reproducible candidate-recall, precision, calibration, or review-yield gap on a sealed corpus;
- a measured scale/resource limit that native bounded retrieval cannot satisfy;
- an operator-owned trained model, review workflow, or interoperability requirement;
- a compelling maintenance/adoption case with acceptable license and security posture.

The selected option gets a new or reopened implementation packet with files, invariants, refusal
codes, test matrices, commands, and acceptance evidence. A generic desire for optionality is not
enough.

## Non-negotiables

- No named matcher, Python runtime, UI, email provider, or network service is required by core.
- Learned or probabilistic output is uncertain evidence, never trusted identity authority.
- Training/development data cannot read sealed acceptance labels.
- Every model/operator result is bound to inputs, feature schema, parameters, code, and policy.
- Optional work cannot weaken native acceptance or delay the sec10d migration.
