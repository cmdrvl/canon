# canon geo — worked-case corpus extraction queries

Staging for **bd-tccn**, the six-to-nine case corpus that gates all `canon geo`
implementation. Operator decision 2026-08-14: no code until real cases are worked end to
end from landed data.

These are extraction queries, not canon code. Nothing here ships in the binary. The output
becomes fixtures in canon's `--suite` / `--gold` format so the corpus is the evaluation
suite rather than a document that rots.

## Files

| File | What it does | Runnable today? |
|---|---|---|
| `00_discovery.sql` | Confirms table and column names, landing progress, and MapPLUTO release inventory | **Yes** |
| `01_case_selectors.sql` | Finds real instances of each case shape by query | **Yes** — geocode book + MapPLUTO only |
| `02_tile_assembly.sql` | Pulls every source at every level for one case's tile | Partly — MapPLUTO block only until NY lands |

## Run order

1. `00_discovery.sql` — **always first, and again after every source finishes landing.**
   Confirms names and records what was available at extraction time.
2. `01_case_selectors.sql` — pick the specific instance per case. Record the selection
   query alongside the case.
3. `02_tile_assembly.sql` — set the anchor point and case id at the top, run per case.

## Landing status (2026-08-14)

| Source | Table | Status |
|---|---|---|
| MapPLUTO | `NYC_DCP_MAPPLUTO_HOT` | Landed — 856,614 lots, 26v1 |
| NYC footprints / BIN | *name unconfirmed, see Q6* | Landed per cmdrvl-curves bd-397d — **highest-priority pull** |
| FEMA USA Structures | `FEMA_USA_STRUCTURES_HOT` | Landing — alphabetical by state, at AR of 135.3M features |
| Microsoft GlobalML | `MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT` | Landing — 179 of 2,415 US files |
| Overture Maps | `OVERTURE_MAPS_FEATURES_HOT` | DDL applied, 0 rows |

**New York is not loaded in any of the three national sources yet.** Cases 1–8 are
constructible on the MapPLUTO layer today; the full-tile version of every case waits on
those landings. A case that stops because a dataset is missing is a **result** — record
which source blocked which capability, per bd-tccn.

## Three rules these queries encode

**Take literal values, record the query.** Three claims in the 2026-08-14 design session
came from an LLM's prose summary rather than returned values, and all three were wrong —
most concretely a reported `UNITSTOTAL 178` for a MapPLUTO lot when `UNITSTOTAL` is not a
column in the landed table. Read `tool_responses[].structuredContent`, not the answer
markdown. Every case cites a column confirmed present by `00_discovery.sql`.

**Dedupe to a declared grain before computing any rate.** The geocode table is
`PROPERTY_NAME + PROPERTY_ADDRESS + PROPERTY_CITY + PROPERTY_STATE + SOURCE + ASOF` —
Geocodio's per-result source attribution appended, not upserted, and with **no deal or
loan identifier column at all**. 6,682 five-borough rows are not 6,682 properties, and the
absence of a deal id means case 9 needs an upstream join nobody had identified as a
prerequisite.

**Never collapse a set with `MAX` / `LIMIT 1` / first-returned.** A point can legitimately
fall inside more than one lot (condo unit BBL overlapping its parent — 157 five-borough
points, max 4). `within` returns a set. Collapsing it silently discards a real ambiguity,
which is the failure mode this whole outcome exists to prevent.

## Two non-obvious technical decisions

**Tile extent is defined by distance, not by an H3 k-ring call.** edgar-elt `br-2wir`
records that Snowflake's H3 helpers returned malformed cell boundaries (~27–48 m edges
against an expected ~174 m) and a k-ring listing one neighbour three times. `H3_R8` is used
only as a partition prefilter; `ST_DWITHIN` in metres defines the answer. Canon computes
its own cells with a vetted Rust library (bd-2b9d).

**Membership is centroid-anchored, with polygon overlap as the supplement.** cmdrvl-curves
`bd-3fyv` measured this live: `H3_POLYGON_TO_CELLS` produced 116 coverage rows for 354,804
buildings, because polygon-to-cells returns cells whose *centres* fall inside the polygon
and a building almost never contains an r8 cell centre.

## Related beads

`bd-tccn` corpus · `bd-14co` baselines · `bd-1a12` geocode plausibility ·
`bd-272d` attribute anchoring · `bd-2cbs` entity levels · `bd-2zdz` assemblage ·
`bd-3nc7` predicate regime · `bd-35qg` address-set acquisition
