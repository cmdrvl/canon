# Field report on the data-center recovery protocol: what was done, what it found, what to do next

**Date:** 2026-09-29.
**Status:** operator-requested review brief. Documentation only. No Canon code was changed, no gate is claimed, and no accuracy is measured.
**Responds to:** [RESPONSE_CANON_GEO_DATACENTER_SITE_TEST.md](RESPONSE_CANON_GEO_DATACENTER_SITE_TEST.md), which responded to [BRIEF_CANON_GEO_DATACENTER_SITE_TEST.md](BRIEF_CANON_GEO_DATACENTER_SITE_TEST.md).
**Related beads:** `bd-dzdy1` (cmdrvl-curves, the field test), `bd-179b` (truth rebuild, still gates every release claim), `bd-2omi` (replay harness; comment only). No bead was closed or filed.
**Evidence tags:** [V] I ran or hash-checked it myself this session. [A] a subagent reported it and I did not recompute it. [J] my judgment.
**Artifacts:** all under `scripts/geo_measurements/fixtures/datacenter_recovery_2026-09-28/` unless noted. Raw rows and sealed labels are outside the repository.

## 1. Bottom line

1. The response's central diagnosis holds on everything tested. The tested path cannot subtract a candidate, so the full candidate universe is always an optimum. Reproduced byte-identically on the pinned build [V], then replayed from frozen bytes on Pryor at three universe sizes and on two further cases [V]. This is an objective-formulation property, not a data problem.
2. It is not the only failure. A second, separate failure is reach: a fixed size filter dropped 30 of 33 entities of one labeled site before the solver ran [A]. Fixing the objective does not fix that, and fixing reach does not fix the objective.
3. The evidence that any alternative would work is not yet gathered. Pryor's frozen evidence supports retained ambiguity, not a winner, per a blind reviewer [A] and per the earlier trace [J]. So a patch cannot be judged by whether it "picks the right building" on Pryor. It has to be judged on the two cases with labels (DEV-1, H1-P), and those are two cases at diagnostic grain.
4. Two of the three holdout roles were never run. H2 (no address) and H3 (constructed ambiguity) have labels or a label attempt but no frozen neighborhood, because they carry no point anchor. The protocol's hardest claim, discovery from region and attributes, is untested.
5. Recommendation: proceed to Step 4 with the narrowest patch that the trace supports, tested against controls that a naive shrinking rule would fail (section 7). Do not start the formulation redesign until the narrow patch's result is in. Do not add more process artifacts; the next work should be code and tests.

## 2. What the response claimed, and where the field data stands

| Response claim | Status after this work |
|---|---|
| Inclusion-only costs make the full universe an optimum (section 2.1) | **Confirmed** on the pinned build and on three frozen snapshots [V]. Not a Canon solver bug: the solve is exact for the objective. |
| Alternatives and members are different variables (2.2) | **Consistent, not tested.** No patch was tried. The solver's per-building factorization (F2) is what an alternatives-as-members translation looks like from the output side [J]. |
| Source agreement is not identity evidence (2.3) | **Supported.** Frozen Pryor: 453 footprint records collapse to 202 physical entities, and 128 of 175 Overture rows cite Microsoft as a source, so two "sources" are one footprint [A]. |
| Soft disagreement is not hard exclusion; rank lower without deleting (2.4) | **Untested.** No signed or absence-preferring evidence exists in the request format to try it on. |
| The agent supplies part of the missing model (2.5) | **Confirmed.** Every rule (radius, min area, neighbors, module threshold, tolerances, weights) was a hand-chosen input in all three replays [V]. |
| The experiments used a restricted neighborhood (section 3) | **Confirmed and extended.** Removing the 100,000 sq ft filter changes what is reachable (F3). |
| H2/H3 region-and-attributes inquiry is a first-class entry point | **Not exercised.** No anchor-generation step exists for it (F7). |

## 3. What was executed

| Step | What was done | Commit | Gate status |
|---|---|---|---|
| Earlier field test | Runs 00 to 06, brief, evidence chain | `e7d65e1` | done |
| 0. Pin and reproduce | Clean worktree, `canon 0.14.0` built `--locked`; Pryor solve reproduced byte-identical on 0.14.0 and 0.13.0; independent objective check | `05529f5` | **met** [V] |
| 1. Freeze case, labels | 1 development case plus 3 drawn roles with 2 reserves each; pre-registered draw, manifest, label policy and measures before any label; labels sourced by separate subagents and sealed outside the repo | `6dff912`, `a60aee9` | **met with caveats** (F5) |
| 2. Freeze neighborhood | Pryor, DEV-1, H1-P; 1600 m radius, no size filter, intersection semantics, per-source count reconciliation, manifests hashed | `0ec6217` | **met for 3 of 5 cases** (H2, H3 not frozen) |
| 2b. Representability | Evaluator checked whether each label is inside its snapshot, metadata-only reply | `2603907` | done |
| 3. Trace | Non-blind Pryor trace (Step 3 in `05529f5`); blind repeat from the frozen bytes; kernel replay on frozen Pryor at 10, 27, 110 candidates; generic replay on DEV-1 and H1-P | `2603907`, `18a7793`, `85841dc` | **partly met**: blind trace and replay done; no blind kernel ablation on the new snapshot (see 8) |
| 4 to 7 | Not started | none | open |

Counts: 6 subagent runs sourced or scored labels, 2 ran acquisition or trace. No Canon source file was modified.

## 4. Findings

Each finding states its strength and what would falsify it.

### F1. The full-universe optimum reproduces byte-for-byte and scales [V]
- Original run: 10 Overture buildings, 17 soft preferences, zero hard constraints; solve sha256 `a8584ef52d22...` on both 0.14.0 and 0.13.0. All ten buildings, cost zero, sole model at that cost, 1,023 candidates.
- Frozen-bytes replay of the same pre-registered rules (`step3_frozen_replay.md`): the same 10 buildings and the same 17 preferences; same signature. The solve hash differs (`6978a1f6...`); I did not diagnose why (likely rounding or a hand-transcribed field in the original). The byte-identical claim applies to the retained run-02 input only.
- At 27 candidates (>= 1,000 m2) the solve completes with 134,217,727 residual models, none materialized or ranked. At 110 candidates (no size filter) the residual count saturates at the u64 maximum. In both, the solver splits into one independent variable per building (27 of 27 and 110 of 110), and each reports exactly one positive assignment of two [V]. I read that as "include" being each building's only optimum; I inferred the field meaning from its name and did not read its definition in source [J].
- Falsifier: a solve on any of these requests where some building is excluded from rank 1 without a hard constraint.

### F2. Ablation: no source removes a candidate [V for Pryor, from the earlier trace]
For every subset of the three retained sources the optima are exactly the supersets of the union of what those sources assert: no sources 1,023 ties; partition-only 32; module-only 128; cross-layer-only 2; all three 1 (the full universe). The first broken link is translation (evidence became inclusion rewards), then formulation.

### F3. Reach is a second, independent failure [A]
Evaluator numbers, approximate because the site boundary was inferred from label cues:

| | DEV-1 | H1-P |
|---|---|---|
| Candidates (Overture >= 1,000 m2, 1,300 m of any anchor) | 69 | 35 |
| Site entities | 33 | 5 |
| Site entities in the universe | 3 of 33 | 5 of 5 |
| Site built area in the universe | 83% | 100% |
| Site share of the returned universe (count / area) | 4% / 7% | 14% / 41% |

- DEV-1 loses reach: 30 of 33 entities are under 1,000 m2 and never enter. By area, 83% still reaches, so "reach" depends on grain (address grain here counts every structure).
- H1-P has full reach and 86% of candidates (59% by area) are non-site: a pure precision failure that the objective produces.
- Blind Pryor trace: 168 of 202 entities are under 1,000 m2, and a 1,000 m2 filter flips the nearest candidate from the east side to the west side [A].
- Do not inherit a size filter as a default. Whether small objects are members is an evidence question, not a retrieval one.
- Discrepancy: the earlier representability check called H1-P "partial" (larger parcel possibly outside the 1600 m neighborhood); the reach check finds 5 of 5 inside the universe. Different site definitions. Unreconciled.

### F4. Retained ambiguity is the correct output for Pryor [A, J]
The blind reviewer found six interpretations, zero known member buildings, and no operator link for any building. It could not pick a winner from the frozen bytes and named the missing observations: parcel and ownership at the address, an operator record tied to a building or parcel, function of the small east-side objects, retrieval beyond 1.6 km, imagery or construction record, a working geocode, and Foursquare pin accuracy. The earlier non-blind trace reached the same top-level conclusion. The blind reviewer added two points: the filter flip and lineage collapse. Independence caveat: the frozen snapshot's only point anchor came from the earlier run.

### F5. Independent labels are scarce and shape the suite [A]
| Case | Label | Grain |
|---|---|---|
| DEV-1 | state filing (accessibility project registration) | address only |
| H1-P | county assessor record | parcel, partial campus |
| H2-P | none found; aggregator sources only | none |
| H2-R1 (reserve) | city council record, 2019 | campus by intersection |
| H3-P | city release plus operator page | site, city level; MW unconfirmed |
- The suite can only score sites with a municipal or state paper trail. That is a selection effect, disclosed but not correctable inside this suite.
- Two of the labels do not reach parcel grain, so parcel-grain scoring is not available for them. DEV-1 relies on the fallback rule (county parcel record as label, TxGIO Bexar excluded), which is not independent of that file.
- All labels are single-adjudicator and unaudited. The H2-R1 site link rests on an applicant name, not on a matching address or MW.

### F6. Anchors are weak more often than not [V]
The geocoder returned no match for Pryor and H1-P, and its DEV-1 result is Census address-range interpolation, not a building location. Overture address points existed for H1-P (7, possibly not all) and none for the Pryor and DEV-1 streets. Foursquare gave a stale-looking generic pin for Pryor and none at the H1-P house number. An address is an assertion to test (AGENTS.md); in 2 of 3 cases the tested path had no anchor better than a single unverified pin.

### F7. H2 and H3 were not executed [J]
Neither has a point anchor, so no neighborhood can be frozen without a new anchor-generation step. H3 has a label; H2 has a reserve's label. The protocol's claim that region-and-attributes discovery is a first-class entry point is therefore unmeasured. This is the largest untested part of the protocol, not a minor omission.

### F8. Infrastructure hazards found [A]
- The data MCP clips cells at about 2,000 characters, byte-budgets responses, and its `fetch_result` paging returned duplicate rows. The acquisition agent used key-range fetches, checked by MD5, row counts and distinct-key counts, after discarding a corrupted first Pryor extract.
- FEMA vintages differ by state: TX 2025-06-06, AZ 2023-05-02, OK 2023-10-03 (the last matches the pre-registered value). Do not treat FEMA as one release.
- Acquisition scripts live in a session scratchpad; the frozen snapshots are not reproducible from the repo alone.

## 5. Process problems and what they weaken

- **Self-attestation.** That subagents did not read sealed labels or center retrieval on evaluation points is self-reported. No independent audit exists.
- **Evaluator leak.** An evaluator's reply disclosed to me that H1-P's label has two parcels, one larger and Microsoft-owned. It reached no analyst and altered no snapshot, case or measure, but it was in my context while I designed later steps. Disclosed in `step2b_representability_and_blind_step3.md`.
- **Rules were mine.** The replay rules for DEV-1 and H1-P were adapted from the Pryor rules by me and fixed before scoring, but they are one rule set, one filter value, and not derived from any independent design. Results depend on them.
- **Not blind on the kernel.** The blind reviewer did observation-level tracing only. The kernel replay and reach scoring were run by me and by the evaluator, not repeated by a fresh agent.
- **Artifact weight.** By AGENTS.md's ratio test, this work produced a lot of process artifact (pre-registration, label status, manifests, records) and no runnable Canon behavior. Steps 1 and 2 are justified only as gates for a specific patch; they should not be extended. [J]

## 6. What can and cannot be concluded

Can conclude (with the evidence tags above): the objective cannot subtract; the failure reproduces and scales; a fixed size filter is a separate reach hazard; Pryor's evidence supports ambiguity; independent parcel-grain labels are rare.
Cannot conclude: that any patch improves accuracy; that the failure explains all the field-test misses; anything about H2, H3, or any site without a label; anything about rates. Two labeled cases do not support a percentage, and a diagnostic suite with a selection effect would not support one with more.

## 7. Recommendations

Ranked. Each names what it gates.

1. **Step 4, narrow patch (recommended).** Add one evidence-justified way for a candidate to be disfavored, so that the full universe stops being the automatic optimum. The response ruled out an arbitrary extra-member penalty, so the patch must attach a penalty to a declared relation: for example a `prefer_absent` (cost if present) preference emitted only for a candidate that a source records as belonging to a competing named occupant, and never from the mere absence of support. Scope it to the translation layer plus the one objective term, behind a declared preference kind.
   - Controls a naive shrink-everything rule would fail (per the acceptance-criteria rule in AGENTS.md): (a) a request with no disfavoring evidence must still return all candidates, ambiguity unchanged; (b) a request where the true-site candidate is disfavored must not be silently dropped; it must fall in a lower-ranked model, not be excluded; (c) byte-identical output for requests that contain no new preference kind.
   - Judged on: DEV-1 and H1-P precision at unchanged reach, and Pryor staying ambiguous. Success on Pryor is retained ambiguity with a narrower named alternative set, not a pick.
   - Honest expectation: on the current snapshots the effect may be small, because the retained evidence rarely names a competing occupant. That is itself a finding and should be reported, not tuned away.
2. **Fix reach separately, before scoring anything.** Stop applying a size filter as a default universe rule; either remove it or make it a declared, evidence-based band. This is a retrieval and profile change, not an objective change, and can proceed independently.
3. **Build an anchor-generation step for region-only cases (H2, H3), then run them.** Without it the protocol's discovery entry point stays untested. Scope it small: one region, ranked candidate anchors with provenance, no chosen winner.
4. **Defer the formulation change (site interpretations as variables).** It is the principled fix and the larger one. Decide it after the narrow patch shows how far translation alone gets, so the redesign is chosen by evidence rather than by preference.
5. **Reconcile the H1-P discrepancy** with the same adjudicator, and have a second adjudicator independently source a label for DEV-1 at parcel grain, so the fallback label is not the only parcel truth.
6. **Save the acquisition scripts and the warehouse-rows builder next to the manifests**, so the frozen snapshots are reproducible from the repo.
7. **Do not** file a large bead tree, extend the pre-registration machinery, or claim a gate. Follow-up beads worth filing after your review: (a) reach-filter removal, (b) `prefer_absent` translation with the controls above, (c) region-only anchor generation, (d) label independence audit. I have filed none.

## 8. Decisions for you

1. Approve Step 4 as scoped in recommendation 1, or choose the formulation change first, or stop at a proposal.
2. Do you accept the DEV-1 fallback (parcel record as label) or require an independent parcel-grain label first?
3. Should H2 and H3 be run through a new anchor-generation step, or dropped and reported as an untested part of the protocol?
4. Should the kernel replay and reach scoring be repeated by a fresh agent (closing the last Step 3 gap), and is that worth doing before Step 4?
5. Is the H1-P discrepancy worth a reconciliation now, or only if the patch moves H1-P?

## 9. Index of artifacts

| Artifact | Purpose | Key hash |
|---|---|---|
| `step0_baseline_record.json` | pin and reproduce | solve `a8584ef52d22...` |
| `step1_preregistration.md`, `step1_selection.json`, `case_manifest.json` | frozen cases and rules | commit `6dff912` |
| `step1_label_status.md` | sealed-label metadata and gate | sealed sha256s in file |
| `step2_record.md`, `step2_snapshot_manifests/` | frozen neighborhoods | PRYOR `56eda4e6...`, DEV-1 `02438849...`, H1-P `916b326e...` |
| `step2b_representability_and_blind_step3.md`, `step3_pryor_blind_trace.md` | representability, blind trace | trace `22c111dc...` |
| `step3_frozen_replay.{py,json,md}` | Pryor kernel replay at 10/27/110 | commit `18a7793` |
| `step3_generic_replay.{py,md}`, `step3_generic_replay_{DEV-1,H1-P}.json` | DEV-1, H1-P replay and reach | commit `85841dc`; reach report sealed `017e8e11...` |

Not in the repository: raw snapshot rows and the sealed labels and reports (`outcomes/datacenter-canon-geo-test/`).

## 10. Reproduce

Follow `README.md` in the fixtures directory for Steps 0 and 3. For the frozen replays, the snapshots must first be regenerated by an acquisition run against the warehouse (the scripts are not yet saved); the replay scripts then run as `python3 step3_frozen_replay.py <snapshot> <canon> <builder> <outdir>`.
