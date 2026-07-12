# BDC Issuer Resolution — extension requirements for Canon's P06/P07 contracts

> Vetting note (2026-07-10) against **bd-137o** (CANON-V1) and **bd-doe2** (out-of-tree
> demanding deployment). Purpose: use the CMD+RVL BDC SOI issuer-resolution problem as the
> **concrete forcing function** while the P06 extension contracts are still being designed,
> so the whole BDC layer provably sits *on top* (no Canon core domain branches) rather than
> discovering a gap after release.
>
> Context: the SOI tournament (cmdrvl-soi, 44/53 funds at F1=1.0) produces per-holding rows —
> issuer surface + BDC filer + as-of quarter + industry + instrument (lien/rate/maturity/FV).
> Private borrowers, **no LEI/CUSIP/ISIN**. This is the case `bd-2gf3` names as where string
> ER earns its keep ("NPORT holdings issuer names where lei/cusip/isin are N/A ... → canon
> ORGs"). The tournament is the domain preprocessor; Canon should only ever see clean evidence.

Each requirement is: **real data → what a generic contract must express → acceptance test.**
If a contract can't express it, that's a small *generic* core/contract change to make during
design (cheap), not a fork after release (expensive).

## R1 — dba / holding-company surfaces (contract: bd-3hz3 normalization)
- **Real:** `Rocket Bidco, Inc. (dba Recochem)`; later filings say `Recochem` / `Recochem Holdings`.
- **Requirement:** the normalization bundle preserves the raw value and emits BOTH the legal
  surface (`Rocket Bidco, Inc.`) and operating/dba surface (`Recochem`) as typed, provenance-linked
  views via a generic parenthetical-capacity split — no company dictionary. Either view may
  retrieve candidates, but neither view grants merge authority and legal-form/holdco/SPV tokens
  remain available as protected distinctions.
- **Test:** candidate recall@50 retrieves `Rocket Bidco, Inc. (dba Recochem)` ↔ `Recochem
  Holdings`; the dba surface appears as a first-class blocking key in miss forensics (bd-21nh).

## R2 — tranche assignment vs. identity (contract: bd-3bst assignment; bd-1092 mapping)
- **Real:** GSBD holds `Idera, Inc. | 1st Lien Term Loan` AND `Idera, Inc. | 2nd Lien Term Loan`
  — two SOI rows, ONE issuer, TWO instruments. ARCC also holds Idera.
- **Requirement:** each SOI row maps to ONE issuer *observation* plus a separate *assignment*
  fact `{subject: BDC filer, role: holder-of-instrument, object: issuer obs, interval: as-of
  quarter, payload: lien/rate/maturity/FV, provenance: SOI row}`. Two tranches must NOT inflate
  into two issuers; resolving Idera@GSBD == Idera@ARCC is an identity decision independent of
  the instrument.
- **Test:** two tranche rows → 1 issuer observation + 2 assignments; identity can be accepted
  while an assignment stays disputed; promotion of an assignment never writes an alias.
- **Why it's load-bearing:** conflating tranches into the issuer is precisely the SEC-XBRL
  tranche-collapse bug that produced the false GSBD "gap" in the tournament's XBRL comparison.

## R3 — anti-merge veto on lookalikes (contracts: bd-1w3y evidence IR; bd-3hz3 protected features)
- **Real:** `Idera, Inc.` (software) vs `Idera Pharmaceuticals` (biotech) — high name similarity,
  DIFFERENT entities.
- **Requirement:** normalization preserves distinguishing name tokens. `industry` remains typed
  contextual evidence because the same issuer may be classified differently by different filers.
  A hard cannot-link veto requires an authoritative incompatibility or explicit reviewed
  distinctness; industry disagreement alone can lower confidence or force review, never prove
  non-identity.
- **Test:** the Idera lookalikes do NOT auto-merge; differing name tokens and industry context are
  visible in the evidence waterfall (bd-393u). A same-issuer fixture with inconsistent industry
  labels must remain linkable/reviewable rather than receiving a hard veto.

## R4 — cross-BDC co-occurrence as evidence (contract: bd-1mr6 packaged evidence operators)
- **Real:** `Recochem` is held by GSBD, ARCC, and Golub in the same quarter. With no LEI for
  private borrowers, "who holds it" is a primary signal.
- **Requirement:** a generic evidence operator may compute a **relational feature over the
  assignment graph** — "observations co-held by ≥N distinct filers" — emitted as
  context/support evidence, never as authority or hard merge evidence. Its marginal value must be
  established by leakage-resistant ablation. (The evidence IR already declares
  pair/hyperedge/record-link support; confirm the operator set can express the group-by-filer
  count, else add it as a GENERIC operator — useful beyond BDC.)
- **Test:** a name co-held across 3 filers gets more support / ranks above a singleton; but
  co-occurrence alone never auto-promotes a merge (still needs identity review).

## R5 — time-forward renames & alias accretion (contracts: bd-1092 intake; P05 temporal facts)
- **Real:** a Q3 filing introduces `Recochem Inc.`, effectively a rename of `Rocket Bidco, Inc.
  (dba Recochem)`.
- **Requirement:** observations carry as-of; resolution is time-forward; a confirmed same-as
  accretes the new spelling as a **timestamped alias with a valid interval** keyed to the
  resolved entity (the deterministic-alias-accretion pattern from bd-2gf3, applied to ORGs).
- **Test:** entity-disjoint + time-forward discovery benchmark (bd-2w13) recovers the rename
  without a same-quarter leak; the alias history is queryable as-of.

## R6 — attribute-corroborated linkage (contracts: bd-1w3y evidence IR; bd-1rlk scorer; bd-3bst assignment)
- **Real:** matching our parse ↔ DERA per holding. Names are formatted differently (DERA's
  `investment_identifier_axis` is a concatenated breadcrumb; ours is a clean `portfolio_company`),
  but within a filing the STRUCTURED columns line up: fair value, cost, par/principal, interest
  rate, maturity date, industry. A name-only match manufactured 400–1861% false conflicts because
  it couldn't align tranches; matching on the numeric/date columns aligns them uniquely.
- **Requirement:** the evidence IR + scorer must combine **non-name structured comparison
  features** (numeric-within-tolerance on FV/cost/par, exact/near date on maturity, rate match,
  categorical industry) alongside the name feature into one match decision — so a strong
  structured agreement resolves an ambiguous name, and each tranche (1st lien vs 2nd lien vs
  equity of the same issuer) aligns to its DERA counterpart by its distinct FV/rate/maturity. The
  instrument/tranche is an ASSIGNMENT on the resolved issuer (bd-3bst), not a second identity: the
  ISSUER is resolved (name + attributes); the tranche is a typed holding fact hung off it.
- **Bootstrap value:** a high-confidence structured match yields a labeled name correspondence
  ("DERA aggregate string" ↔ "our clean name") for free — auto-generating the must-link pairs the
  R1/R4 resolver and the gold corpus (bd-epmu) train on, at scale, without hand labeling.
- **Test:** on a filing with multi-tranche issuers, matching on FV+cost+maturity aligns each
  tranche to its DERA row and recovers the correct issuer name pair; the numeric/date features
  must carry the linkage when name similarity alone is below threshold; and the emitted labeled
  pairs feed the corpus.
- **Note:** current canon lookup is exact byte-match only — this is a CANON-V1 engine capability
  (multi-feature record linkage), core to the evidence engine, not an add-on.

## What this implies for the plan (the "on top" verdict)

- Every R above lands on an **existing open P06/P07 contract** — nothing here asks Canon core
  to know "BDC". R2/R5 also reuse the deterministic-alias-accretion pattern already blessed in
  bd-2gf3.
- **The one genuine risk is R1/R4 recall.** First use the accepted native candidate operators and
  this normalization bundle. If bd-21nh still shows a sealed-corpus shortfall, record the failure
  slice and revisit the deferred P09 option space after core acceptance: a separately proven native
  technique port, bounded external adapter, or generic relational candidate operator. No named
  matcher or speculative operator blocks the core architecture.
- **Corpus dependency:** R1–R5 acceptance tests need a labeled BDC issuer gold corpus
  (must-link/cannot-link). We are well-positioned to bootstrap it from tournament output: 44
  funds × thousands of issuer strings with quarter/industry/instrument context, with the
  sealed/human-reviewed golds as clean anchors. This is domain-owned (bd-epmu), out-of-tree.

## Shortest public-safe BDC proof path

The shortest useful proof is not the full R1–R6 program. It is one bounded
counterfactual issuer-attachment run through the shipped
`canon entity alias-withholding` contract. `bd-3gym` completed the fixture-only
micro-proof that the available BDC-shaped source can be adapted into this
harness. `bd-1t2y` is the production-path convergence gate; it must not be
reported as passing until the fresh runs below complete against the tightened
artifact contract.

1. **Prepare outside the repository.** In an owner-only work directory, select
   reviewed issuer cases and derive only deterministic manifests and artifacts.
   Raw issuer surfaces, source paths, assignment payloads, and private corpus
   excerpts never enter Git, Beads, logs, or the public report.
2. **Exercise the minimum decision matrix.** Include at least one supported
   incumbent attachment, one evidence-insufficient abstention or ambiguity, one
   unmatched case, and one related-but-distinct/lookalike hard negative. Include
   a typed holding/tranche assignment source so the issuer-identity versus
   assignment firewall is exercised rather than merely declared.
3. **Prove a genuinely clean counterfactual.** For every trial, the real clean
   registry must exactly equal the retained benchmark mapping set and registry
   id/version, and exact lookup must prove the withheld alias absent. Mapping,
   search-index, cache, normalization-patch, generated-corpus, and display-name
   scans must enumerate nonempty concrete sources bound to the validated native
   chain. The mapping scan must cover the complete clean registry tree.
4. **Derive outcomes from shipped artifacts.** Candidate rank and miss
   forensics come from the candidate-recall report; decision and evidence come
   from public link/run/solve artifacts; review state comes from a rebuilt
   review queue; and audit must certify the real solve while the run proves
   stage continuity. The link-ID-to-surface-ID join must come from the
   derivation-validated link sidecar. A `prepared_surface_collapse` is reported
   separately with no candidate-rank or recall-denominator credit. A
   `relation_policy_control` is excluded from same-entity recall and turns any
   automatic attachment into a visible false merge. Caller-authored outcome
   receipts do not count.
5. **Close the positive loop only after review.** A supported attachment must
   use a typed native review-import receipt, add exactly one withheld alias to a
   sandbox registry, pass zero-error registry lint, and resolve that alias by
   ordinary exact replay. The claim must say whether it was an `evaluated_pair`
   attachment with a real candidate rank and support path or a
   `prepared_surface_collapse` attachment; collapse/accretion is not retrieval.
   Abstentions, unmatched cases, and relation-policy hard negatives must produce
   no promotion or replay evidence.
6. **Repeat from clean state.** Run the same sealed manifest twice in separate
   owner-only directories using the same built Canon binary. Require identical
   sanitized report bytes and digests, the same per-trial outcomes, ranks, and
   explicit candidate dispositions,
   zero unexpected file writes, and zero private-value/path hits in all public
   stdout, stderr, evidence, and comparison artifacts.

This is the stop/go boundary. If it passes, the next direct beads are
`bd-wr0g` for sealed no-theatre empirical acceptance and `bd-2w13` for
entity-disjoint/time-forward generalization. R4 co-occurrence ablation and R6
DERA-style attribute corroboration can then expand the evidence base; neither
is allowed to substitute for this first clean issuer-identity proof, and no
external DERA fetch is a dependency of `bd-1t2y`.

### Current acceptance checkpoint

`bd-1t2y` passed this bounded stop/go gate on 2026-07-12. Two fresh owner-only
attempts using the same pinned Canon binary produced byte-identical strict
alias-withholding reports (`sha256:5b8bb711834f6b3e396cb7f5dc589a7b68b6a272a1c854885c078e74bea5db8c`).
The four-case matrix recorded one rank-1 evaluated-pair attachment with typed
review, one-entry promotion, and exact replay; ambiguity, unmatched, and
related-distinct controls all abstained correctly. Aggregate results were one
credited attachment, three abstentions, zero rejects, and zero unsupported
guesses. The sanitized public evidence had zero private-value or private-path
hits, and the two attempt trees were kept owner-only.

The shipped `canon entity alias-withholding` command performed final envelope
compilation. The typed native-review receipt and registry-backed replay artifact
were constructed through Canon's public Rust APIs; native review CLI import
wiring remains the explicit `bd-14m6` follow-up. This checkpoint proves the
bounded decision matrix, not production readiness or corpus generalization;
`bd-wr0g` and `bd-2w13` remain the next claim-expansion gates.

## Recommendation
Adopt R1–R6 as the concrete checklist that `bd-doe2`'s neutral-twin acceptance must exercise
(as invented non-BDC analogues) and that the private local acceptance runs against real BDC
issuers. Co-designing the P06 contracts against these six now guarantees amenability by
construction and keeps the extension firewall (bd-2axg) intact.
