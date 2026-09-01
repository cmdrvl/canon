# Canon Geo pinned B/C/D measurements

These queries are the executable source for Appendices B, C, and D of
`docs/PLAN_CANON_GEO.md`. They are a release gate, not retained evidence for the
solver: fixture results must never be presented as a fresh warehouse run.

## Execution contract

1. Call the cmdrvl-data MCP table list and describe operations first. Use the
   fully qualified names returned by discovery; bare names may describe
   successfully but fail in SQL.
2. Run one SQL file at a time through the guarded read-only query operation.
3. Require `ok=true`, a nonzero declared denominator, and all sanity columns to
   pass. Record the SQL returned by the MCP, not a prose summary.
4. Recover the Snowflake query ID from
   `INFORMATION_SCHEMA.QUERY_HISTORY` when the MCP success envelope omits it.
5. A changed release is `snapshot_moved`, not a failed reproduction. Update a
   pin only as a new measurement with its own receipt.

Pinned source snapshots for the 2026-08-28 run:

| Source | Pin | Declared feature grain |
|---|---|---|
| MapPLUTO | `RELEASE='26v1'`, `RELEASE_DT='2026-05-01'` | distinct nonempty `BBL` |
| NYC building footprints | `RELEASE_DT='2026-08-09'`, active only | distinct `OBJECTID` |
| FEMA USA Structures | `RELEASE_DT='2025-06-06'` | distinct `PROVIDER_FEATURE_ID` |

## Fresh receipts

All successful queries below ran through cmdrvl-data MCP on 2026-08-28. Query
IDs were read back from Snowflake query history after execution.

| Measurement | Query ID | Snowflake elapsed | Result |
|---|---|---:|---|
| release-string positive control | `01c6b1b6-0821-83a1-006c-c7030888b86e` | 509 ms | 2,343 parcels / 2,343 distinct BBLs |
| `00_discovery.sql` | `01c6b1c1-0821-83a1-006c-c7030888b8f2` | 1,192 ms | all 9 source-cell counts nonzero |
| Appendix B observations | `01c6b1c1-0821-784b-006c-c7030888c3ce` | 221 ms | 100 parcels + 93 footprints |
| Appendix C density + reach control | `01c6b1d2-0821-784b-006c-c7030888c4da` | 2,222 ms | 1,192 cells / 856,614 parcels; 824 footprints outside parcel-home cells; 0 null H3 |
| Appendix D same-cell file | `01c6b1c0-0821-83a1-006c-c7030888b8de` | 3,527 ms | BX 287/4/0; BK 2,332/22/0 |
| Appendix D complete-reach file | `01c6b1c0-0821-784b-006c-c7030888c3c6` | 2,332 ms | BX 290/1/0; BK 2,352/2/0 |

## 2026-08-31 E5 Franklin County thin-tier preflight

`e5_franklin_county_thin_tier_readiness.sql` is a file-exact, bounded
availability measurement around collateral points in Franklin County, Ohio
(`39049`). It is not E5 execution: no county parcel layer was present in the
live warehouse inventory, so it does not measure parcel reach, parcel/building
composition, precision, or an evidence-tier operating point. The other layers
remain a legitimate future minimal-stack case; current composition v0 cannot
yet execute it because it requires at least one parcel candidate. That is a
named generic entity-universe gap, not evidence that parcel-free regions have
no usable evidence.

Query `01c6c151-0821-a0dc-006c-c703088daaba` completed in 3,354 ms and emitted
four guard-`ok` rows over 151 distinct geocoded properties, 202 loans, 152
multi-property loans, 114 r8 center cells, and 585 distinct center+k1 work
cells:

| Evidence class | Pinned release | Distinct features | Occupied work cells |
|---|---|---:|---:|
| FEMA structures | Ohio partition `2023-05-02` | 160,773 | 582 |
| Microsoft GlobalML footprints | `2026-07-24` | 168,778 | 581 |
| Overture addresses | `2026-07-22.0` / `2026-07-22` | 310,650 | 581 |
| Overture buildings | `2026-07-22.0` / `2026-07-22` | 203,367 | 584 |

The FEMA HOT table also contains an 85,090,164-row `2025-06-06` partition, but
the only Ohio rows are pinned to `2023-05-02`. A global latest date therefore
cannot be inherited as a geography's source vintage. The four source counts
are availability/provenance, never four independent votes. SQL SHA-256:
`25ddd0d29095c583ac0478d6d228162667b8be62a15218c5acd5ef29d3326aab`.

The two historical D claim classes are deliberately separate. `same-cell`
reproduces the legacy H3-home-cell candidate restriction. `complete bbox reach`
scans the pinned parcel snapshot behind a complete bounding-box prefilter before
applying the warehouse spatial predicate. The difference is candidate reach,
upstream of predicate or solver truth. Snowflake GEOGRAPHY arithmetic is an
empirical reference here, not Canon exact-local-integer truth.

The original appendix labeled `882a100d8bfffff` as Manhattan. A fresh
borough/coordinate control on 2026-08-29 proved it is Brooklyn: 2,343/2,343
MapPLUTO rows have `BOROUGH='BK'`, 2,354/2,354 footprint BBLs have borough
prefix 3, and centroid bounds are longitude -73.9361..-73.9236 / latitude
40.6811..40.6897. The executable labels now say `BK_DENSE`; the numeric
predicate and reach denominators are unchanged in those historical queries.

## 2026-08-29 home-cell bridge receipt

Fresh list/describe-first controls used the deployed
`EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_GEOM_V3_EXT` and
`EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT` contracts.

- MapPLUTO v3 completeness query
  `01c6b6dc-0821-9a4f-006c-c703088b300e`: 26v1 has 856,614 rows and
  26v2 has 856,687; both have zero null centroids/WGS84 WKT and all rows have
  null `H3_R7_SOURCE`/`H3_R8_SOURCE`. H3 therefore remains a derived WGS84
  sibling, not source-plane geometry.
- Footprint parity sample `01c6b6e6-0821-9a4f-006c-c703088b3012`: ten
  release-pinned active footprint centroids from `882a100d8bfffff` replayed
  through h3o at r8 with 10 matches, zero mismatches, and zero 1-microdegree
  boundary-sensitive rows. This is a bounded positive sample, not global
  parity proof.
- Geography correction query `01c6b6e9-0821-9a4f-006c-c703088b3016` proves
  the `MN_DENSE` label was false and is superseded as described above.
- MapPLUTO v3 export query `01c6b6f4-0821-9a4f-006c-c703088b306a`: five
  26v2 derived-centroid rows materialized into two r8 cells, with zero
  boundary-sensitive rows. Two fresh processes emitted identical artifact
  bytes, SHA-256
  `6c95edd213259bef185021b14b5b77321fe809a7766ca84a00119b94612c17bf`.
- Controlled-halo reach query `01c6b6f9-0821-83a1-006c-c703088a39aa`
  (10,819 ms): Canon supplied the explicit h3o r8+k1 cells and Snowflake
  compared same-cell and k1 blocking with a complete bbox reference over the
  release-pinned geom-v3 parcel plane. Brooklyn produced same-cell 2,333/21,
  k1 2,353/1, and complete-reference 2,353/1/0; Bronx produced 287/4,
  290/1, and 290/1/0. The complete reference found zero majority edges outside
  k1 in either cell, and k1 repaired 20 Brooklyn plus 3 Bronx same-cell misses.
  All denominators reconciled.

The v3 rerun supersedes the HOT-only operational counts by one Brooklyn row;
the earlier query IDs remain historical receipts rather than being rewritten.
These receipts validate an executable source-to-blocking bridge and bounded
k1 reach for exactly two cells. They do not prove citywide candidate recall,
other resolutions, or solver correctness.

## 2026-08-30 stratified r8/r9 receipt

`appendix_d_stratified_halo_centers.sql` deterministically chooses one logical
r9 child of each declared r8 stratum by ranking the combined parcel-plus-footprint
population whose representative points independently bin to that r8, breaking ties
by canonical H3 text. H3 logical ancestry is exact but geometric containment across
resolutions is approximate, so the query separately reports the complete population
of each selected r9 cell and the points that independently bin to another r8. The
file-exact query `01c6bc81-0821-9afc-006c-c703088c04f6` ran in 7,171 ms;
both population-partition sanity checks passed in all six rows.

`appendix_d_stratified_halo.sql` pins MapPLUTO geom-v3 to `26v2` /
`2026-08-01` and active NYC footprints to `2026-08-09`. It expands the audit
to six r8 strata across all boroughs and the stress-selected r9 logical child.
The r9 selection is a deliberate stress sample, not a population distribution.

Complete-r9 versus selection-in-r8 populations differed in Manhattan small by
10 parcels and 10 footprints, Queens medium by 20 and 35, and Staten Island by
0 and 6; the other three strata had no difference. The measurement below uses
the complete independently point-binned population at each resolution.

Before the measurement, both current Snowflake point-to-cell functions returned
h3o's `892a100d62bffff` for the historical bad control point. Canon emitted all
twelve explicit k1 disks. File-exact query
`01c6bc7d-0821-a0dc-006c-c703088c231a` ran in 23,572 ms and returned twelve
nonzero rows with all seven sanity columns passing.

- r8: 6,002 center footprints; same-cell `5,895 / 107 / 0`, k1 and complete
  reference both `5,995 / 7 / 0`; 100 same-cell misses repaired by k1.
- r9: 1,419 center footprints; same-cell `1,344 / 75 / 0`, k1 and complete
  reference both `1,418 / 1 / 0`; 74 same-cell misses repaired by k1.
- `truth_outside_k1=0` in all twelve strata. Two-source work-unit sizes were
  2,260–25,786 nodes at r8 and 378–4,670 at r9.
- Majority-incidence component maxima were 4–71 at r8 and 3–65 at r9. The
  Staten Island parcel-star survives the resolution change.

The component graph is explicitly limited to k1 parcels, center-owned NYC
footprints, and geometric-area-majority edges. It is not the final solver graph
and excludes FEMA, client layers, and every non-geometric constraint. The result
does not establish citywide recall, exact-local-integer predicate parity, full
Snowflake↔h3o assignment parity, or a solver runtime distribution.

## 2026-08-30 Overture third-plane receipt

`appendix_f_overture_three_source.sql` adds Overture buildings pinned to
`2026-07-22.0` / `2026-07-22` without changing the MapPLUTO or NYC-footprint
pins above. The statement returned 24 nonzero source-stratum rows under query
`01c6bcc3-0821-a0dc-006c-c703088c2682` in 35,772 ms. Every emitted sanity field
passed.

- Overture center observations: 6,018 at r8 and 1,401 at r9. Controlled k1 and
  the complete parcel reference both classified `6,005 / 13 / 0` at r8 and
  `1,400 / 1 / 0` at r9 (one / zero / multiple majority parcels).
- Overture repaired 88 same-cell misses at r8 and 66 at r9; no complete-reference
  majority edge fell outside k1.
- The combined raw-observation graph remains a forest, but its maximum parcel
  star grows to 128 at r8 and 118 at r9. Those are observation-node counts, not
  deduplicated latent buildings or final solver variables.
- 7,360/7,419 measured Overture observations declare OpenStreetMap lineage.
  Overture and direct OSM geometry therefore cannot be counted as independent
  evidence. Raw OSM semantic tags may still be a distinct evidence channel when
  preserved with record/version lineage and ODbL attribution.

The upstream convenience contracts are not healthy: the pinned H3 coverage
projection returned zero rows (`01c6bcbd-0821-a0dc-006c-c703088c2502`), and the
typed building view failed its 28-versus-33-column contract
(`01c6bcbc-0821-9afc-006c-c703088c06e6`). The working base table has 6,443,512
distinct New York buildings with valid H3 anchors
(`01c6bcbd-0821-a0dc-006c-c703088c24fe`), which is the explicitly documented
bypass used by this measurement.

## 2026-08-30 H.7 staging-table truth control

`h7_staging_denominator_control.sql` uses the release-pinned ACRIS staging
MASTER/PARTIES/LEGALS tables and keeps the H.7 ordering explicit: candidate
documents are formed before filed-borough equality is tested against LEGALS.
MASTER `RECORDED_BOROUGH` is diagnostic and cannot reject a candidate.

Query `01c6bfd2-0821-a0dc-006c-c703088d1612` completed in 3,508 ms with two
nonzero rows and all denominator controls true. Non-round reproduced
653 eligible / 262 candidate / 221 legal-confirmed / 172 accepted / 49
ambiguous / 41 no-legal / 391 no-candidate, including 35 accepted multi-BBL
subjects. Round measured 2,321 / 311 / 306 / 270 / 36 / 5 / 2,010, including
36 accepted multi-BBL subjects. The 71 subject keys are returned as a bounded
array; this is a denominator and subject-selection control, not a typed
population artifact or solver evaluation.

`h7_staging_truth_export.sql` turns that selection into a bounded accepted-
truth handoff without claiming candidate reach. Query
`01c6bfda-0821-a0dc-006c-c703088d161e` completed in 10,305 ms and returned 71
distinct loan/document rows: 35 non-round and 36 round. Each row carries sorted
distinct truth BBLs plus bridge, MASTER, selected exact-PARTY, and LEGALS source
records and raw-file hashes. A direct `RESULT_SCAN` validation found zero
contract, row-cap, BBL-count, provenance, party-witness, or plane-leakage
failures; the 71 subjects contain 626 distinct BBL edges, from 2 to 172 per
subject. The query's source SHA-256 is
`230e40407e805e0ec4783185dcd731edb2285553051c91f69e726dd32aea13e1`.
It is not `canon_geo_h7_population_rows.v0`: MapPLUTO candidate parcels and
both release runs remain deliberately absent and must be measured separately.

`h7_staging_halo_reach_control.sql` measures that next plane without merging it
into truth or solver correctness. A first formulation recomputed H3 over both
full parcel releases and was cancelled as
`01c6bfe1-0821-a0dc-006c-c703088d1642`; it was discarded and not repeated.
The positive path uses `STG_GEO_GEOMETRY_HOT_KEYS`, which has exactly 856,614
and 856,687 valid, populated r8 MapPLUTO keys for the two pins. Rendered against
the accepted-truth query above and halo 1, query
`01c6bff9-0821-a6c8-006c-c703088d25d2` completed in 3,218 ms with four
guard-`ok` rows. Halo-2 sensitivity query
`01c6bff9-0821-a6c8-006c-c703088d25d6` completed in 4,557 ms with the same.

- Non-round reach is 24 full / 2 partial / 9 none over 35 subjects in both
  releases; round reach is 28 / 1 / 7 over 36. Thus 52/71 subjects have full
  r8+k1 candidate reach, independently of any solver result.
- The 164 point-owned work sections are not yet small exact residuals. Median
  parcel counts are 5,758 non-round and 4,931 round; p90 is 12,576 and about
  9,652; the maximum is 13,663. One non-round section is empty.
- Unioning sections by loan is explicitly diagnostic and produces 1,192–27,120
  candidates. Those unions must never be presented as monolithic solve inputs;
  the next boundary is section incidence components.
- The non-round point envelope reaches latitude 42.913397 and longitude
  -75.596272 despite accepted NYC filed/legal truth. This is candidate-channel
  error, not evidence against ACRIS truth.
- k2 recovered no additional accepted legal-truth edge or subject. It raised
  median section candidates to 14,449 non-round and 10,992 round, with maxima
  31,631 and 29,788; loan-union maxima rose to 50,035 and 58,184. Wider halos
  therefore do not repair the remaining association failures and must not be
  used as a substitute for better candidate generation.

The source SHA-256 is
`6eaec54140218e1ecb8154abb76fe770b26737d1abe6c0dada3a71a4a2368dee`.
Snowflake H3 is an empirical blocking calculation here; canonical home-cell
artifacts still require h3o replay.

`h7_staging_pip_block_reach_control.sql` measures an address-independent
alternative candidate channel from the repaired parcel-vintage staging table.
It first finds every 26v1/26v2 MapPLUTO parcel containing each collateral point,
then expands to every parcel sharing the containing parcel's six-digit BBL
block. Accepted ACRIS truth is flattened only after those candidate relations
exist. Thus the query can measure reach but cannot seed candidates from either
the filed address or the answer set.

Query `01c6c11c-0821-a0dc-006c-c703088da762` completed in 15,612 ms. All
eight release/plane/association rows had `guard_status=ok` and zero reach-
accounting failures. The two pinned releases produced the same counts:

| truth / association | subjects | full / partial / none | truth edges reached / total | candidate BBLs min / median / p90 / max |
|---|---:|---:|---:|---:|
| non-round / multi | 20 | 11 / 8 / 1 | 88 / 247 | 3 / 83.5 / 189.2 / 755 |
| non-round / single | 15 | 4 / 2 / 9 | 13 / 62 | 0 / 19 / 48.6 / 58 |
| round / multi | 17 | 13 / 3 / 1 | 41 / 73 | 10 / 63 / 119 / 154 |
| round / single | 19 | 10 / 2 / 7 | 44 / 244 | 1 / 26 / 51.599 / 61 |

The containing-parcel step reached 158/168 collateral points. Combined subject
reach is 38 full / 15 partial / 18 none over 71 accepted loans. This channel is
far narrower than the H3 loan unions (maximum 755 versus 27,120 candidates),
but it also has lower full-truth reach (38 versus 52 subjects). It is therefore
a bounded baseline for candidate strategy evaluation, not a replacement for
controlled tile ownership or a solver input chosen on size alone. One accepted
subject has no containing parcel and no block candidates; the typed H.7
materializer preserves that row as `reach=none` while excluding it from the
nonempty exact-solver population.

Precursor query `01c6c11b-0821-a6c8-006c-c703088db796` is discarded: joining
candidate rows before aggregating truth multiplied hit counts and produced the
impossible result 75 reached truth edges from 73 truth edges. The checked-in
query counts distinct truth membership and asserts reached edges never exceed
truth edges. Its source SHA-256 is
`26d77c2eb78740c60d386c372d0e2c3fa8a7f049ff3c089ffc485a92e37a39b4`.

`h7_candidate_strategy_comparison.sql` evaluates H3 r8+k1, PIP-to-six-digit-
block, and their union over exactly the same 71 accepted loans, two pinned
MapPLUTO releases, and four truth/association strata. Query
`01c6c14f-0821-aa0e-006c-c703088dc33a` returned 24 rows with every guard true.
Per release, all selectors shared 626 truth edges: H3 reached 208 and classified
52 full / 3 partial / 16 none; PIP-block reached 186 and classified 38 / 15 /
18. The union matched H3 because PIP was a subset on this cohort. That union is
reach accounting only, never a monolithic solve. SQL SHA-256:
`4532e3635dcd1aa4655064489c0e248a3bd0c80b03dad7c4a789ce405ece4f2e`.

`h7_staging_pip_block_population_export.sql` converts the successful accepted-
truth result into one raw candidate row per subject/release. Query
`01c6c174-0821-aa0e-006c-c703088dc742` completed in 43,370 ms and returned the
expected 142 rows for 71 loans × two releases, with zero guard rows and two
explicit zero-candidate rows. The superseding execution fails closed unless
the accepted rows remain pinned to ACRIS `2026-08-10` and property state `NY`.
It preserves available ACRIS and MapPLUTO
locators/digests but is not the typed Canon population request and is not a
solver receipt. A nested-object precursor
`01c6c14b-0821-a0dc-006c-c703088da9fa` cancelled at the client boundary and is
discarded. SQL SHA-256:
`d3e287532a83da6b66d0250eb5c6e71d29a088c990b34a7a997eef0121f10e77`.

`h7_staging_source_record_bytes_export.sql` re-joins those candidate locators
and emits derived Canon key/value payload bytes for bridge, ACRIS, and MapPLUTO
roles. These are intentionally compact derived evidence records, not full or
original warehouse rows; each preserves its upstream locator and binds the
role, vintage, and applicable parcel edge. Live role diagnostic query
`01c6c180-0821-aa0e-006c-c703088dc906` covered 142 release rows / 71 loans and
reported zero role, parcel-union, source-id uniqueness, hash, locator, or
NY-scope failures. Payload aggregate query
`01c6c189-0821-a0dc-006c-c703088de03e` reported 5–817 derived records per row,
a 1,353-byte maximum record, a 1,804-character maximum base64 record, an
876,919-byte maximum row payload, and two zero-candidate rows.

The current file additionally projects all eight accepted-plane denominator
columns needed for adapter drift checks. That projection-only change occurred
after the live aggregate, so the current SHA-256
`d806b0949cbcc2dd6a66817529de8efd72cf733cc87f5304d8b19e9e23f174c8`
is covered by static SQL contract tests but is not described as file-exact live
execution.

`canon geo materialize-h7-staging-batch --batch <BATCH.json>` is the offline
typed handoff for this H.7/NYC profile. It accepts lowercase Canon keys or the
uppercase top-level keys returned by Snowflake, rejects guard rows, mixed query
metadata, release/denominator/count drift, arbitrary base64, and payload/wrapper
mismatches, then delegates to `materialize_h7_population_rows`. No live
142-row batch artifact is committed and no solver evaluation has been run, so
the H.7 population and frozen E4 gate remain open. This adapter does not make
MapPLUTO or ACRIS part of the generic rho compiler.

`h7_e4_consensus_truth_extension.sql` probes the honest five-case E4 deficit.
It keeps the controlling amount/date/filed-borough/legal-borough and exact-
lender planes, starts from document-ambiguous loans, and admits only complete
candidate document sets whose legal multi-BBL arrays are byte-identical.
Query `01c6c162-0821-aa0e-006c-c703088dc4c6` completed in 35,426 ms with two
guard-`ok` summary rows and zero admitted subjects. This is a useful negative
result: the deficit remains five; no release duplication or weakened truth
gate is used to fill it. The described warehouse inventory exposed no Gate
V2/H4 extension-key bridge, so that cross-gate dedupe remains explicitly
unavailable rather than inferred.

2026-09-01 bd-179b retention-control rerun: root pinned the 2026-08-10
ACRIS staging aggregate at 17,065,090 master rows / 17,049,742 distinct
documents / 17,049,742 derived-usable rows, 46,540,137 party rows /
17,049,584 distinct documents / 46,537,120 derived-usable rows, and
22,727,180 legal rows / 17,005,665 distinct documents / 22,727,180
derived-usable rows. These are raw staging denominators only, not H.7 accepted
subjects; successful MCP responses did not expose Snowflake query IDs for
these checks.

The historical bridge pin
`3aed6660-ce1c-46a9-aeb2-7296c134ce8f` now reads as 0 bridge rows / 0
distinct loans in current `PROPERTY_MART.LOAN_ISSUANCE_PROPERTY`. That is a
snapshot-retention finding, not permission to replace the pin. The SQL now
emits `historical_bridge_build_not_retained_in_current_snapshot` plus
fail-closed truth-plane and population mismatch guards, preserving the two
expected planes and their retained counts instead of silently returning an
empty result. A compact live guard check emitted two truth-plane rows and the
guard failures `accepted_71_population_count_mismatch`,
`eligible_plane_population_count_mismatch`, and
`historical_bridge_build_not_retained_in_current_snapshot`; that successful
MCP response also omitted a Snowflake query ID. The compact check did not run
the full checked-in SQL and is not a file-equivalent execution receipt. Given
the zero historical bridge rows, the full SQL's per-plane guards would also
emit `truth_plane_eligible_count_mismatch` and
`truth_plane_multi_bbl_count_mismatch`. A fresh file-equivalent run remains
blocked on restoration or immutable retention of the historical bridge
snapshot.

The current observed `PROPERTY_MART` build
`ce3953ac-c2d4-4b48-bf02-29f0cf341389` is a separate live observation:
51,778 rows / 17,372 distinct loans, built at 2026-09-01 07:33:41.258 -0700.
Under the same H.7-shaped aggregation it produced non-round
652 eligible / 262 candidate / 221 legal-confirmed / 172 accepted / 49
ambiguous / 35 multi-BBL loans and round 2,323 eligible / 312 candidate / 307
legal-confirmed / 271 accepted / 36 ambiguous / 36 multi-BBL loans. Because no
immutable retained artifact containing the accepted 71 H.7 loan keys is checked
in locally, this current-build cohort cannot be compared key-for-key and cannot
extend H.7 truth. Zero new bd-179b subjects are admitted from this pass.

Discarded attempts are retained for audit: the first current-build discovery
query used `rows` as an alias and failed with Snowflake query ID
`01c6c882-0821-b531-006c-c7030893ca92`; earlier exact document-date/lender
probes against the historical bridge pin returned zero rows due to the same
bridge retention loss and were not used as positive discriminator evidence.

`h7_staging_incidence_shard.sql` performs the next bounded reduction. It owns
work by r8 center cell, expands only the selected center+k1 sections, joins
pinned MapPLUTO geometry, and measures raw NYC/Overture majority-overlap
incidence components. All 16 shards over accepted-truth query
`01c6bfda-0821-a0dc-006c-c703088d161e` completed:

| shard | query id | ms | centers |
|---:|---|---:|---:|
| 0 | `01c6bfee-0821-a6c8-006c-c703088d259a` | 15,159 | 4 |
| 1 | `01c6bff2-0821-a0dc-006c-c703088d165e` | 17,931 | 6 |
| 2 | `01c6bff2-0821-a6c8-006c-c703088d25a6` | 14,457 | 4 |
| 3 | `01c6bff2-0821-a6c8-006c-c703088d25a2` | 15,740 | 8 |
| 4 | `01c6bff2-0821-a6c8-006c-c703088d25aa` | 13,635 | 4 |
| 5 | `01c6bff3-0821-a6c8-006c-c703088d25ae` | 13,803 | 5 |
| 6 | `01c6bff3-0821-a6c8-006c-c703088d25b2` | 15,959 | 9 |
| 7 | `01c6bff3-0821-a6c8-006c-c703088d25b6` | 12,452 | 2 |
| 8 | `01c6bff4-0821-a0dc-006c-c703088d1662` | 13,415 | 3 |
| 9 | `01c6bff4-0821-a0dc-006c-c703088d1666` | 15,713 | 8 |
| 10 | `01c6bff5-0821-a0dc-006c-c703088d166e` | 13,512 | 3 |
| 11 | `01c6bff5-0821-a6c8-006c-c703088d25ba` | 15,689 | 9 |
| 12 | `01c6bff4-0821-a0dc-006c-c703088d166a` | 14,502 | 5 |
| 13 | `01c6bff6-0821-a6c8-006c-c703088d25be` | 13,312 | 8 |
| 14 | `01c6bff5-0821-a0dc-006c-c703088d1672` | 13,634 | 4 |
| 15 | `01c6bff6-0821-a0dc-006c-c703088d1676` | 14,466 | 6 |

Aggregate query `01c6bff7-0821-a6c8-006c-c703088d25c2` reconciled 88
distinct centers, 497,128 parcel memberships, and 176,086 raw observations.
Work units contain 5–17,617 nodes (median 6,987; p90 13,219.6). Every section
has component median 1; the median section p90 is 3, and the global observed
maximum is 109. There were zero multi-majority observations and zero shape or
accounting failures.

Exception query `01c6bff7-0821-a0dc-006c-c703088d167e` found eight unique-
majority observations outside k1 across three sections and one remote section
with five observations but zero MapPLUTO parcels. Ring diagnostic
`01c6bff8-0821-a0dc-006c-c703088d1682` places all eight majority parcels in
k2. The empty section is the collateral point at 42.913397, -75.596272; it is
candidate-channel error, not a contradiction of accepted NYC legal truth.

A discarded precursor joined raw MapPLUTO numeric BBLs directly to normalized
text keys and silently produced zero parcels. The file-backed query strips the
raw `.0` suffix and makes a nonzero work unit an independent sanity condition.
Components remain raw observation stars, not reconciled latent buildings or
the final constraint-incidence graph; Overture/OSM lineage overlap also
prevents source count from becoming independent evidence. Snowflake geometry
and H3 remain empirical until exact local integer and h3o replay. The source
SHA-256 is
`d289cc42f742cdfb2b009a8630b10a9122d22fe8c9faa5fd8d71ff94c26734e1`.

Appendix B's query returns the frozen observation set. Build the bipartite graph
between parcel and footprint centroids using haversine distance with mean Earth
radius `6,371,008.8 m`; for each declared radius, connected components include
isolated observations. The 2026-08-28 output exactly reproduced every retained
row:

| radius m | components | mean | max | p50 | p90 | size 6–20 | isolated |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 10 | 64 | 3.02 | 17 | 2 | 7 | 8 | 16 |
| 15 | 51 | 3.78 | 25 | 2 | 8 | 7 | 7 |
| 20 | 33 | 5.85 | 34 | 2 | 15 | 7 | 4 |
| 25 | 24 | 8.04 | 37 | 4 | 16 | 9 | 1 |
| 30 | 12 | 16.08 | 49 | 8 | 37 | 5 | 1 |
| 35 | 7 | 27.57 | 59 | 31 | 59 | 1 | 1 |
| 40 | 6 | 32.17 | 59 | 31 | 59 | 1 | 1 |
| 50 | 3 | 64.33 | 77 | 59 | 77 | 0 | 0 |
| 60 | 1 | 193.00 | 193 | 193 | 193 | 0 | 0 |
| 150 | 1 | 193.00 | 193 | 193 | 193 | 0 | 0 |
