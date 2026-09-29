# Representability check and blind Step 3 repeat (2026-09-29)

Follows `step2_record.md` (commit `0ec6217`). Neither item changes any code or claims accuracy.

## 1. Representability of the sealed labels in the frozen snapshots (evaluator-side)
An evaluator subagent read the sealed labels and the snapshots and reported metadata only. Its full report stays sealed
(`outcomes/datacenter-canon-geo-test/sealed/representability_report.md`, sha256 `c1642ce2b963d67b...`). Labels carry no coordinates, so the evaluator located each site inside
the snapshot from label-derived cues (building sizes, legal-description and address cues); the counts below are approximate and rest on those inferences.

| Case | Representable | In neighborhood | Overture | Microsoft | FEMA | Foursquare |
|---|---|---|---|---|---|---|
| DEV-1 | yes | yes | yes (~13, 2 shell-sized) | yes (~21, 2 shell-sized) | partial (~28, mostly tiny, 1 substantial) | no (nearest two within ~100 m) |
| H1-P | **partial** | one labeled parcel yes; the other, larger, most likely outside (unverified) | yes (~5 campus-scale) | trivially (~8, all tiny) | no | no |

Reading:
- H1-P's frozen retrieval, centered on the address-derived anchors at 1600 m, probably does not cover the whole labeled campus. This is a reach limit of the frozen
  retrieval, not something to patch in place; wider retrieval is a new snapshot and a separately reported revision (Step 2 rule 8).
- In H1-P, the campus-scale evidence exists only in Overture; Microsoft has only tiny structures and FEMA nothing, so agreement across sources would not corroborate anything there.
- DEV-1 representable means candidates exist; it does not mean the current path selects them.

**Disclosure:** the evaluator's reply leaked label detail into my (analyst-side) context: H1-P's label has two parcels, one larger and Microsoft-owned. I have not
passed this to any analyst or reviewer and it has not been used to alter a snapshot, case or measure. Future evaluator replies will be constrained further.

## 2. Blind fresh-reviewer Step 3 repeat, PRYOR
A separate reviewer with no access to the earlier trace, labels or the analysis directory worked only from the frozen snapshot and the Step 3 protocol text.
Trace: `step3_pryor_blind_trace.md` (sha256 `22c111dc580807a3ead4a7a27bf9c89c9a7850c75c0bc35bf4b97833690aec27`). No canon run was made; this repeats the hypotheses and observation-effect
chain, not the kernel ablation. The reviewer recognized the site name from general knowledge and set it aside (self-reported).

Reported by the reviewer (I have not independently recomputed these numbers):
- 6 interpretations: pin-parcel facility on the east side; pin as gate of a larger campus; west-side buildings across the street; pin is a stale generic POI and the site is
  elsewhere in the park; vacant or pre-construction ground; answer absent from inventory.
- 453 footprint records collapse to 202 physical entities; 128 of 175 Overture rows list Microsoft as a source, so Overture and Microsoft agreement is one footprint, not corroboration.
- Known member buildings: 0. Possible members are nearest-only: 2 west-side entities (60 and 92 m; disfavoured by odd/even numbering and other named operators) and 6 east-side entities within 300 m.
  12 park buildings of at least 10,000 m2 lie at 511 to 1,492 m with no operator link, so size does not identify them. 41 entities touch the 1.6 km edge.
- 168 of 202 entities are under 1,000 m2; the whole east-side set would vanish under a 1,000 m2 filter and flip the nearest answer to the west side.
- The snapshot alone supports no single winner; answer-absent-from-inventory stays live.
- Missing observations: parcel and ownership at the address; an operator record tied to a building or parcel; function of the small east-side objects; retrieval beyond 1.6 km; imagery or
  construction record for the pin ground; a working geocode or address point; Foursquare pin accuracy.

### Comparison with the non-blind trace
Both reach the same top-level conclusion: retained ambiguity with named missing observations, not a winner. Two points are new relative to what the earlier trace stated:
the small-object filter flips the nearest answer, and shared lineage collapses 453 records to 202 entities. Cross-checking of specifics beyond that has not been done.
Caveat on independence: the frozen Pryor snapshot's only point anchor comes from the earlier run (see `step2_record.md`), so the blind reviewer's starting conditions were partly shaped by prior work.
Step 3's gate (a reviewer without the answer key reproduces the first broken link from the frozen bytes) is only half met: this repeat covers the observation-effect side,
and the first-broken-link locating (running the current path and ablation on the frozen snapshot) has not been repeated on the new snapshot.

## Status
| Step | State |
|---|---|
| 1 | Done with caveats (`step1_label_status.md`) |
| 2 | Done for PRYOR, DEV-1, H1-P; H2/H3 not frozen (no point anchor) |
| 3 | Blind repeat done for PRYOR at the observation level; kernel run on frozen snapshot pending |
| 4 to 7 | Not started. No patch is proposed |
