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

## Recommendation
Adopt R1–R5 as the concrete checklist that `bd-doe2`'s neutral-twin acceptance must exercise
(as invented non-BDC analogues) and that the private local acceptance runs against real BDC
issuers. Co-designing the P06 contracts against these five now guarantees amenability by
construction and keeps the extension firewall (bd-2axg) intact.
