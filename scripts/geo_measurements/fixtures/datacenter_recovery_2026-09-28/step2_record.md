# Step 2 record: frozen raw neighborhoods (2026-09-28)

Protocol: `docs/RESPONSE_CANON_GEO_DATACENTER_SITE_TEST.md` Step 2. Acquired by one subagent from read-only warehouse access, after the Step 1 pre-registration (`6dff912`) and label status (`a60aee9`).
The agent attests it did not read the sealed labels and centered no retrieval on an evaluation point; that is self-reported and unaudited. Raw rows are outside the repo
(`outcomes/datacenter-canon-geo-test/snapshots/<CASE>/`, some sources redistribution-restricted); this directory holds the manifests (`step2_snapshot_manifests/`), which carry per-file sha256, query text, receipts, vintages and gaps.

## Snapshots
| Case | Anchors (with point) | Overture bldgs | Microsoft | FEMA | Foursquare | Manifest sha256 |
|---|---|---|---|---|---|---|
| PRYOR | 3 (1) | 175 | 168 | 110 | 66 | `56eda4e61a5682de...` |
| DEV-1 | 4 (3) | 3,674 | 3,807 | 3,827 | 812 | `02438849b47643e4...` |
| H1-P | 9 (7) | 635 | 613 | 643 | 88 | `916b326e579cb695...` |

I re-hashed the three repo manifests against the outside copies (identical). I did not re-hash the JSONL rows or rerun counts; the agent reports it did, per file.

## Policy
1600 m geodesic radius around every anchor that has a coordinate; H3 r7 k=2 cover, bbox prefilter, exact `ST_DWITHIN`; intersection semantics, whole footprints unclipped; EPSG:4326.
No size filter, no largest-N cap. Returned rows equal `COUNT(*)` under the same bounds for every source in every case; a k=3 disk gives the same counts (no coverage hole).
Possible misses: objects over about 2.5 km across whose keyed centroid cell lies outside the disk. No anchor was chosen as correct; competing anchors keep their provenance.

## Gaps and caveats
- **Parcels:** unavailable for PRYOR and H1-P; excluded by design for DEV-1 (TxGIO Bexar).
- **Geocoder:** no_match for PRYOR and H1-P. The DEV-1 point is Census address-range interpolation, not a building location. PRYOR's only point anchor is a Foursquare pin from the earlier run-02, which came from the prior analysis, so this snapshot is not independent of it.
- **Overture address points:** none on the PRYOR and DEV-1 streets. H1-P has seven (PHX10..PHX16); PHX1..PHX9 were not returned, so that anchor list may be incomplete.
- **FEMA vintage differs from the pre-registered 2023-10-03:** TX 2025-06-06, AZ 2023-05-02, OK 2023-10-03. Vintages are recorded per case; do not compare across cases as one release.
- **Geometry** is WKT from Snowflake GEOGRAPHY (about 1e-9 deg); area and distance fields are derived and marked so.
- **Transport:** the data MCP clips cells at about 2000 characters and its `fetch_result` paging returned duplicate rows. The first PRYOR extract was discarded; final rows were fetched by key range and verified by MD5, row count and `COUNT(DISTINCT key)`.
- **Acquisition scripts** (`freeze.py`, `freeze2.py`, `mq.py`) live in the session scratchpad, not the repo, so the freeze is not reproducible from the repo alone. Follow-up: save them next to the manifests.
- **H2 and H3 are not frozen.** They supply a region only, no point anchor. A neighborhood cannot be frozen without a separate anchor-generation step, so those two roles have no Step 2 snapshot and cannot yet be run through the Step 3 trace. Decision needed before they proceed.

## Gate
The declared retrieval and its gaps are inspectable for PRYOR, DEV-1 and H1-P. Whether the labeled answer is representable in each snapshot has not been checked; the evaluator does that against the sealed labels without disclosing anything to the analyst, and it is the next action.
