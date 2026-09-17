# One addressless NXRT experiment: exact-count premise falsified

The seeded subject is **The Summit at Sabal Park, Tampa, Florida**, disclosed
with **252 units**. The resolver receives its name, market, multifamily category,
unit count, and evidence date. Construction year is absent. It receives no
target address, geocode, parcel ID, or footprint. Parcel IDs and geometry in
the regional inventory describe all candidates, not a supplied target.

The native solver reduces **889 regional parcel candidates to an exact residual
of 135** under the declared single-parcel, recorded-attribute compatibility
premise. Two parcels report 252 units; 133 have unknown counts and remain
feasible. Exact name equality supplies no match. No identity is forced or
promoted. See [measurement.json](measurement.json) for evaluation and execution
evidence; the archives contain the exact retained sources and native artifacts.

**The blind resolution attempt fails the truth-reach criterion.** The two
recorded 252-unit parcels are Lakeview Oaks and Grand Cypress Apartments.
The withheld Summit parcel records **254 units**, so the frozen zero-tolerance
policy excludes it. Candidate reach in the initial inventory is distinct from
truth feasibility after pruning. The reduction is exact for the recorded-value
premise but unsafe as a physical-identity conclusion. No post-truth tolerance
change repairs or replaces the reported experiment.

Implementation and this experiment are tracked by **bd-11pt**. The observed
source-comparability gap remains open as **bd-ie4t**; it gates a defensible
identity-pruning policy rather than a report or another solver.

All nine project stages executed successfully in **502.5 seconds** using the
debug build. The run's domain status is `ABSTAINED`: next-evidence options are
available, but total ranking requires a versioned loss model, which this
experiment does not supply. The separate native truth-feasibility probe
returns `conflict`, with exactly zero models. A fresh-process resume reused
all nine stages and preserved their bytes. Neither abstention nor exactness
turns this failed identity experiment into a successful resolution.

| Step | Remaining single-parcel hypotheses |
|---|---:|
| Regional inventory: Hillsborough DOR 03xx | 889 |
| Multifamily compatibility | 889 |
| Recorded units equal 252, or unknown | 135 |
| Construction year absent | 135 |
| Name evidence, soft only | 135 |

These counts concern the declared representation. Exact arithmetic does not
calibrate assessor reliability, establish historical coverage, or prove that a
REIT property equals one tax parcel. The single-member premise is an explicit
experimental restriction. This is not a complete physical-asset resolver or
population accuracy benchmark, and it does not advance E4/E5 acceptance.

## Selection, sources, and withheld truth

The cohort was the two Tampa properties in the public Q4 2025 disclosure,
chosen for obtainable regional data before looking up either property's
address. Selection minimizes SHA256(seed + property name), with seed
`acc8c21c43de30bde96aa978a132908a`. The frozen selection artifact retains the
cohort, chosen descriptors, tolerance policy, and selection time. Its original
page-27 reference is a clerical error; the table is on printed page 28. The
source manifest records that correction without changing the seed or inputs.

- [Company supplement](https://s26.q4cdn.com/937656400/files/doc_financials/2025/q4/NXRT-Esupp-Q4-25-FINAL.pdf)
  supplied the initial descriptors; the same table was subsequently verified
  in the [SEC exhibit](https://www.sec.gov/Archives/edgar/data/1620393/000119312526065382/nxrt-ex99_1.htm).
- [Hillsborough public parcels](https://gisdextweb1.hillsboroughcounty.org/arcgis/rest/services/Hosted/Parcels/FeatureServer/0)
  supplied 889 unique, untruncated DOR 03xx records. The acquisition query has
  no target address, name, parcel, or coordinate predicate. Its selected fields
  exclude site addresses. Unit-count zero/null is explicitly unknown.
- The [operator's property page](https://livebh.com/apartments/the-summit-at-sabal-park/)
  supplied the evaluation-only street address, 4006 Sabal Park Drive. A separate
  Tampa parcel-view address query establishes parcel `0652990300`. The city
  view derives from assessor data; it is not independent assessor calibration.
- [HCPA building footprints](https://downloads.hcpafl.org/?subfolder=_building_footprints)
  supply a 2022 zone-3 snapshot: 287 subarea polygons grouped under 16 building
  numbers on that parcel. These are not 287 buildings. Current completeness is
  unverified; the download's maintenance caveat is retained.

Address and parcel truth were withheld before the exploratory solve. Footprint
truth was acquired during implementation bring-up, then all truth was frozen
before the final fresh process. The offline `prepare` function never opens
truth files. The separate `evaluate` function adds the withheld parcel as an
evaluation-only constraint to the compiled request and invokes native `geo
solve`; this tests feasibility without interpreting unmaterialized model rows
as missing truth. It never changes the blind run.

## Next evidence and limits

First investigate the **252 disclosed versus 254 assessor units** discrepancy:
source definition, aggregation grain, date, and source error. The exact-count
premise cannot support identity until that comparability problem is addressed.
Revoke its hard admission or obtain independently justified bounds; choosing
a tolerance of two after seeing this answer would not be a blind success.

The native separation stage evaluates the previously declared modeled
observation partitioning known and unknown counts: its outcomes leave 2 or
133 candidates. This is a counterfactual model, not an acquired answer. It
cannot recover truth already removed by a hard constraint. Independent
unit-count support for the 133 missing values remains a secondary inventory
gap. Any construction-year or ownership evidence needs its own source contract.

The count band is an explicit **conditional record-compatibility premise**,
not an empirically validated geographic truth constraint. Uncalibrated
empirical contracts remain diagnostic in the implementation's negative test.
Present records may be stale or describe a different aggregation grain; the
2025 disclosure and current parcel snapshot do not prove temporal equivalence.
The regional DOR filter may miss physical assets, and no building composition
is inferred by this parcel-only run. Truth footprints are evaluation support.

An exploratory run completed the main solve but was interrupted after 503.3
seconds during separation. The final request represents the same two
prospective outcomes as integer masks instead of hundreds of explicit allowed
sets. The solver, acceptance conditions, inventory, and descriptors are
unchanged. This cost observation is retained in the source manifest.

## Replay

From the repository root, extract into a fresh directory and run the offline
adapter. Acquisition is not repeated; replay has retained-data proof class.

```bash
cargo build --bin canon
experiment_dir=$(mktemp -d)
tar -xzf scripts/geo_measurements/fixtures/addressless_nxrt_2026-09-17/sources.tar.gz -C "$experiment_dir"
uv run scripts/geo_measurements/addressless_asset.py prepare --sources "$experiment_dir/sources" --work-dir "$experiment_dir/replay"
uv run scripts/geo_measurements/addressless_asset.py run --sources "$experiment_dir/sources" --work-dir "$experiment_dir/replay"
uv run scripts/geo_measurements/addressless_asset.py evaluate --sources "$experiment_dir/sources" --work-dir "$experiment_dir/replay"
```

`run.tar.gz` retains the original nine-stage project run, receipts, inputs,
outputs, execution timing, binary hash, truth-feasibility probe, and fresh
process readback. Absolute workspace paths in run receipts are historical;
the replay adapter builds new path bindings from the retained source bytes.
The source manifest hashes the complete public footprint ZIP; only the
evaluation subset and grouping are bundled, with the extraction recipe.
