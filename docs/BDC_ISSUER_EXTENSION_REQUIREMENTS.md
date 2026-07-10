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
- **Requirement:** the normalization bundle emits BOTH the legal surface (`Rocket Bidco, Inc.`)
  and the operating/dba surface (`Recochem`) from one raw string via a generic
  parenthetical-capacity split — no company dictionary — each traced to the raw value, each a
  blocking key.
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
- **Requirement:** normalization marks the distinguishing token / `industry` as a **protected
  feature** (not normalized away); a domain operator emits a **cannot-link veto** in the evidence
  IR that overrides positive name similarity (epic invariant: "hard cannot-link vetoes positive
  similarity").
- **Test:** same-name / different-industry issuers do NOT auto-merge; the veto is visible in the
  per-decision evidence waterfall (bd-393u).

## R4 — cross-BDC co-occurrence as evidence (contract: bd-1mr6 packaged evidence operators)
- **Real:** `Recochem` is held by GSBD, ARCC, and Golub in the same quarter. With no LEI for
  private borrowers, "who holds it" is a primary signal.
- **Requirement:** a generic evidence operator computes a **relational feature over the
  assignment graph** — "observations co-held by ≥N distinct filers" — emitted as
  context/support evidence, never as authority. (The evidence IR already declares
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

## What this implies for the plan (the "on top" verdict)

- Every R above lands on an **existing open P06/P07 contract** — nothing here asks Canon core
  to know "BDC". R2/R5 also reuse the deterministic-alias-accretion pattern already blessed in
  bd-2gf3.
- **The one genuine risk is R1/R4 recall.** Mitigations, in order, all short of forking: (a)
  native declarative blocking + this normalization bundle; (b) if the bd-21nh recall gate shows
  a shortfall, the **Splink candidate adapter (bd-cuy5)** through the evidence boundary — zero
  core change; (c) only then, a **generic** relational/co-occurrence blocker in core (a widen-
  the-contract change, not a domain branch).
- **Corpus dependency:** R1–R5 acceptance tests need a labeled BDC issuer gold corpus
  (must-link/cannot-link). We are well-positioned to bootstrap it from tournament output: 44
  funds × thousands of issuer strings with quarter/industry/instrument context, with the
  sealed/human-reviewed golds as clean anchors. This is domain-owned (bd-epmu), out-of-tree.

## Recommendation
Adopt R1–R5 as the concrete checklist that `bd-doe2`'s neutral-twin acceptance must exercise
(as invented non-BDC analogues) and that the private local acceptance runs against real BDC
issuers. Co-designing the P06 contracts against these five now guarantees amenability by
construction and keeps the extension firewall (bd-2axg) intact.
