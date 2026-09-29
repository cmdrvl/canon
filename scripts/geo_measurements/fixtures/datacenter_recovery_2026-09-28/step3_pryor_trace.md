# Step 3 trace: Google Pryor OK (dedicated formulation regression)

Protocol: [`docs/RESPONSE_CANON_GEO_DATACENTER_SITE_TEST.md`](../../../../docs/RESPONSE_CANON_GEO_DATACENTER_SITE_TEST.md) §5 Step 3.
Date: 2026-09-28. Status: diagnostic trace, **not blind and not a patch decision by itself**. No code was changed.

## 0. Read this first: what this trace is and is not

- **Analyst contamination.** The analyst who wrote this ran the original experiment and had seen its results and the official
  facility point before writing. The trace uses nothing from the evaluation point in any statement below, but it is not a blind
  reviewer's trace. Per the protocol, a fresh reviewer must repeat it from the frozen snapshot before it justifies a patch.
- **Snapshot limits.** It works only from what is committed: `pryor.spec.json`, `pryor.evidence_compilation.json`, the constants in
  `run_arm_d_pryor.py`, and Epoch's own record for the site. Not retained as files: the ten buildings under 10,000 m2 inside the
  neighborhood, the full list of named Foursquare places, and each source's own record metadata (for example Overture's `sources`
  field). So "answer absent from the inventory" cannot be assessed from the committed bytes (see §6).
- **What it establishes.** How each retained observation is translated and what it does computationally. It does not establish
  where the site is, and it is not an accuracy measurement.
- Reproduce the numbers in §4: `python3 step3_snapshot_trace.py <repo-root>` (standard library only; output
  `step3_snapshot_trace.json`).

## 1. Question, grain, time (frozen for this trace)

> Which of the retained buildings, if any, are supported as part of the site that Epoch AI names "Google Pryor (North)"
> at 4581 Webb St, Pryor OK 74361, as of 2026-09-28?

Target relationship: **site association** at building grain (which candidates are associated), not complete extent. Extent
(what else belongs, and how much) is a separate, unanswered question. Layer vintages: Foursquare release 2026-08-11, Overture
2026-07, Microsoft GlobalML 2026-07-24, Epoch snapshot 2026-09-28.

Original observations: Epoch row "Google Pryor (North)": address 4581 Webb St; current power 368 MW; owner Google (confident);
users Google DeepMind (speculative); chips TPU v5p, v5e; three cited sources (a Google expansion post, an environmental report,
a 2022 TPU cluster news item). No coordinates, parcel ids, acreage or permit reference.

## 2. Candidate site interpretations

Site associations stay separate from members and extent. None is asserted as true.

| ID | Interpretation | Candidates it points to |
|---|---|---|
| S-A | The address point marks the entrance or office complex; the association is the buildings adjacent to it | 56e84c8d (306 m), dcc29a09 (313 m); smaller buildings near the point are **outside** the retained universe |
| S-B | The address point marks an administrative address; the association is the large hall group 0.77 to 1.08 km east-northeast | 215d1320, bd06b2c1, 154131f6, ee97c483, 3fe30483, possibly 37eade67 |
| S-C | Both are parts of one assemblage | union of S-A and S-B |
| S-D | A building is better explained by a different named occupant and is not part of this site | 72ea9c27 (15 m from the "Red Devil" factory point) |
| S-E | "(North)" implies at least one other part of a larger site not in this neighborhood | unknown |

**Retained observations that discriminate S-A from S-B: none.** Every retained observation is compatible with both (§3). The
retained data can weakly discriminate S-D for one building. That is the honest state of the evidence, and the current output does
not say it.

## 3. Observation trace

Format per the protocol: source record -> interpreted proposition -> candidates affected and lineage -> effect -> current
representation -> current computational effect -> warranted effect.

### O1. Foursquare place "Google", 4581 Webb St, Pryor (release 2026-08-11)
- **Record:** name Google; address 4581 Webb St; point (36.23959, -95.33284); category "Business and Professional Services".
- **Proposition:** a place named Google is listed at this address and point (subject: the place; relation: located-at; grain: point;
  position uncertainty unknown).
- **Candidates and lineage:** all, by proximity. Independent of the footprint sources; same POI feed as O2.
- **Effect:** supports "the site is in this neighborhood". **Compatible but non-discriminating** among S-A, S-B, S-C (a point is not an extent).
- **Current representation:** a hard universe boundary (centroid within 1,300 m, area >= 10,000 m2, both chosen by the analyst); its
  distances feed O2.
- **Current computational effect:** hard. It removes 3b5e6819 (1,358 m) and every building under 10,000 m2, including the small
  buildings nearest the point. These removals are not represented as uncertainty or as a coverage gap.
- **Warranted effect:** an anchor with declared position uncertainty that defines the search region and an association tendency
  that decays with distance, plus an explicit coverage statement for what the region cutoff drops.

### O2. Named industrial neighbors (Foursquare): Red Devil (4175 Webb St), Harbison Walker International, A.P. Green Industries, Orchid Paper Products, Praxair welding supply
- **Record:** five named places with points, categories factory or machine shop.
- **Proposition:** other named businesses are located at these points (relation: located-at; a competing possible occupant).
- **Candidates and lineage:** all, by distance to each point; same feed as O1, so not independent of O1's position quality.
- **Effect:** for a building that sits at a neighbor's point (72ea9c27 at 15 m from Red Devil) it **softly contradicts** association with
  the Google site. For a large building whose centroid is hundreds of metres from a neighbor's point it says almost nothing: distance to
  a single POI point is a weak proxy for a large footprint.
- **Current representation:** `fsq-industrial-partition` (weight 5, `claim_role: stable_identity_anchor`): a reward for including the
  5 buildings nearer the Google point than to any neighbor.
- **Current computational effect:** none for the other 5 buildings. Being nearer a different business than to the Google point carries
  **no penalty** (`candidate_table`, column `effect_of_being_closer...` = 0 for all ten). Five of ten are closer to a different named
  business than to the Google point, and the ranking treats all ten the same. The claim role "identity anchor" also overstates a
  proximity relation.
- **Warranted effect:** a visible soft disagreement for candidates that a competing occupant explains better (strong for co-location,
  weak otherwise), never exclusion.

### O3. Overture building polygons (release 2026-07)
- **Record:** 11 polygons >= 10,000 m2 within 1,500 m; ids, areas, centroids.
- **Proposition:** a building polygon with this footprint area exists at this location (subject: physical structure; measure:
  roof footprint area, **not** floor area; time: release).
- **Effect:** defines candidates. Says nothing about association.
- **Current representation:** candidate records with `m2`, `sqft` (footprint labeled as sqft).
- **Current computational effect:** defines the universe; area is not used elsewhere except by O5.
- **Warranted effect:** unchanged for existence; carry the measure meaning (footprint, not floor area) so it is not compared with a
  floor-area or power figure without a declared conversion.

### O4. Microsoft GlobalML footprint agreement (release 2026-07-24)
- **Record:** 20 polygons >= 2,000 m2. Nine of the ten candidates have a Microsoft polygon within 30 m and +/-25% area; 56e84c8d has none.
- **Proposition:** a second footprint source contains a consistent polygon (relation: agrees-with; subject: the physical structure).
- **Candidates and lineage:** nine candidates. **Lineage independence is not established:** the spec labels the two sources with
  different table names, but the retained snapshot does not record the footprint sources' own upstream `sources` metadata, and the protocol
  says distinct lineage labels alone do not establish independence.
- **Effect:** raises confidence that the polygon is a real building. For 56e84c8d, the missing counterpart is **unknown**, not
  disagreement (it may be newer or a different build state). **Does not** bear on membership.
- **Current representation:** `ms-crosslayer` (weight 3, `attribute_observation`): a reward for including the nine.
- **Current computational effect:** membership reward. It is also the largest single source (9 of 10 members).
- **Warranted effect:** a candidate-level existence confidence, not a membership vote; unknown for the uncorroborated one.

### O5. Repeated roof areas
- **Record:** three candidates at 29,321, 30,343 and 31,093 m2 (within +/-5%).
- **Proposition:** three roofs have similar area (relation: similar-in-size).
- **Effect:** **compatible but non-discriminating.** It suggests a common design or phase for those three, and says nothing about
  operator.
- **Current representation:** `hall-module-morphology` (weight 3): reward for including the three.
- **Current computational effect:** membership reward for three.
- **Warranted effect:** a relation among candidates ("these three go together") usable only after one of them is independently
  associated; not three independent rewards.

### O6. Epoch record: "Google Pryor (North)", 368 MW, owner Google
- **Proposition:** an entity with this name and capacity is located at 4581 Webb St; "(North)" implies a sibling part.
- **Effect:** identifies the subject of the question; capacity gives an order-of-magnitude scale for the extent question.
- **Current representation:** **not translated at all.** The name, capacity and the "(North)" suffix are absent from the compiled request.
- **Current computational effect:** none.
- **Warranted effect:** subject and scale metadata carried through; scale used only as an uncalibrated plausibility feature for extent (the
  plan defers power as a hard constraint until an evidence contract exists), never for association.

### O7. Absent observations (unavailable here, not evidence of absence)
Parcel or ownership records for the address and its neighbors (Mayes County is not landed); a permit or facility layout for the site
(exists as a source class; in this experiment it is the evaluation key and cannot also be evidence); building function (cooling towers,
generators); a size figure.

## 4. What the current path does with these (reproducible)

`step3_snapshot_trace.py`, from the committed snapshot only:

- **Per-building evidence table** (`step3_snapshot_trace.json`): total inclusion reward per candidate ranges from 3 to 11; 56e84c8d,
  the candidate nearest the address point, gets 5.
- **Source ablation** of the current objective (minimum cost is 0 in every row; "optimal" counts selections tied at that cost):

| Sources kept | Optimal selections | Smallest optimal size |
|---|---|---|
| none | 1,023 | 1 |
| partition only | 32 | 5 |
| module only | 128 | 3 |
| cross-layer only | 2 | 9 |
| partition + module | 16 | 6 |
| partition + cross-layer | 1 | 10 |
| module + cross-layer | 2 | 9 |
| all three | **1** | **10** |

**Reading.** For every subset of sources, the optima are exactly the supersets of the union of what the sources assert. A source
therefore cannot remove a candidate; its only effect is to force its own members in. The ranking cannot express "S-A rather than
S-B" because neither hypothesis is a variable in the model: only individual buildings are. This confirms, from the retained
compilation, the formulation defect in the response §2.1 (and Appendix A1 reproduces it independently).

## 5. First broken link (protocol table)

| Link | Finding | Evidence |
|---|---|---|
| 1. Relevant source or candidate missing from the snapshot | **Cannot be determined from the committed snapshot.** The retained universe drops small buildings near the point and one building at 1,358 m; there are no parcels or permit; Epoch's scale is unused | §3 O1, O6, O7; needs a re-freeze (Step 2) |
| 2. Subject, relation, units or time lost in translation | **Yes, demonstrable from the retained bytes.** Relations and consistency observations (proximity to other businesses, existence agreement, similar roof areas) all became inclusion rewards; the partition is labeled an identity anchor; footprint area is labeled sqft; Epoch's name, capacity and "(North)" are dropped; agreement between two footprint sources counts as a second vote without lineage evidence | §3 O2 to O6 |
| 3. Distinctions represented but ranking ignores them | **Yes.** The one represented distinction (5 buildings nearer the Google point than to a neighbor vs 5 that are not) has no effect on the ranking; any source subset yields the union | §4 |

**Named root-cause decision (for this case):** *link 2 then link 3, in sequence.* Observations were translated into per-building
inclusion rewards (link 2), and an inclusion-only objective cannot turn those into a choice among site interpretations (link 3).
Repairing link 3 alone (for example an extra-member penalty or a cardinality cap) is disallowed by the protocol and would not restore a
distinction that link 2 already lost. Link 1 stays open pending a re-freeze.

## 6. What a corrected path would need to restore, and what the evidence cannot yet settle

**Distinctions with a particular failed effect to restore (candidates for the Step 4 patch):**
1. Competing-occupant co-location: 72ea9c27 at 15 m from a different named business has zero effect today. Restore a visible soft
   disagreement, not an exclusion.
2. Existence agreement versus membership: stop counting cross-source footprint agreement as association support; keep it as
   candidate existence confidence; keep the un-corroborated candidate as unknown.
3. Alternatives versus members: represent S-A, S-B, S-C, S-D as alternatives over candidate sets so an observation can favor one without
   rewarding every member it touches. The retained data supports S-A and S-B **equally**, so the correct output for this snapshot is
   **retained ambiguity with a named missing observation**, not a winner.

**The evidence supports several explanations equally, so the protocol says specify the missing observation, do not tune toward a key:**
- an ownership or parcel boundary for 4581 Webb St and neighboring parcels (Mayes County; not available here);
- a permit or facility layout for the site (a permissible source class; in this experiment it is the evaluation key, so a different label
  would be needed if it were used as evidence);
- building-function evidence (cooling and generator structures);
- a size or capacity-to-area relation, uncalibrated and for extent only.
Whether any of these sources contains a usable record for this site is not established here.

**Expected form of a corrected output (a specification, not a result):** an association with the address neighborhood, S-A and S-B
retained as alternatives, 72ea9c27 flagged as better explained by another occupant, extent unresolved, and the missing observations
listed. It would not report a smaller set as "resolved".

**Answer-absent-from-inventory:** the universe rules (>= 10,000 m2, <= 1,300 m) can drop the true association (for example S-A if it
involves small buildings). The committed snapshot cannot show whether that happened; Step 2 must re-freeze the neighborhood without
those filters and record coverage.

## 7. Gate assessment (protocol Step 3)

A proposed patch has particular failed distinctions to restore (items 1 to 3 in §6), and the trace also identifies where the evidence
does not distinguish (S-A versus S-B) and what to acquire. **The gate is met for scoping Step 4 on this snapshot only under the
condition that a fresh reviewer reproduces §3 to §6 blind** and that Step 1 supplies an independently labeled development case. Pryor
itself cannot score building-level association: the only official label is one facility point.

## 8. Next steps

- Step 1: select the development case and three holdouts before looking at labels. Candidates already in hand for a case with parcel
  and owner relations: Bexar County (Microsoft SAT14, exact county address); a multi-parcel assemblage (Taylor County, Lancium); a case
  with an official point and no anchor (Los Lunas); an insufficient-evidence case (the Bexar "158-acre" claim). Independence of labels is
  not yet checked.
- Step 2: re-freeze the Pryor neighborhood without the size and radius filters, retaining per-source lineage fields.
- Do not start Step 4 until a blind reviewer has reproduced this trace.
