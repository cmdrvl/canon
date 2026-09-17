# Courtney Cove: hints discover a parcel; supplied address corroborates it

Canon independently ranked parcel **1029330100**, with assessor address
**5510 N Himes Ave, Tampa, FL 33614**, first among **889** county multifamily
candidates using the disclosed name **Courtney Cove** and **324 units**. The
target address was absent from that request. A subsequent operator-source
lookup confirmed the address, and a separate run accepted that supplied
address as additional soft evidence for the same parcel.

This demonstrates candidate discovery and source corroboration from a landed
inventory. It does **not** certify physical identity, complete property extent,
building membership, or population accuracy. The hard residual remains 889 in
both runs; neither promotes a registry identity. The supported grain in this
experiment is a **parcel candidate**, with a source-recorded situs address.

| Native result | Hints only | Hints plus supplied address |
|---|---:|---:|
| Inventory / hard residual | 889 / 889 | 889 / 889 |
| Known name agreements | 1 | 1 |
| Known unit-count agreements | 7 | 7 |
| Known address agreements | Not supplied | 1 |
| Unknown unit counts | 133 | 133 |
| Equally best ranked models | 1 | 1 |
| Best parcel | 1029330100 | 1029330100 |
| Best / next-best native soft cost | 6 / 7 | 6 / 8 |
| Truth survives / appears first | Yes / yes | Yes / yes |
| Forced identity | No | No |

Soft cost is the existing solver's additive absent-preference cost. It is not
a probability or a count of independent sources. The name, unit count and
address fields from one assessor row share lineage. The six other 324-unit
parcels remain explicit alternatives; a missing count earns no agreement.

## Admission correction and the original failure

The original Summit experiment mislabeled raw source comparisons as logical
relaxations. That allowed an uncalibrated 252-versus-254 discrepancy to exclude
the true parcel. The descriptive adapter now refuses that declaration. An
explicit `uncalibrated` rho basis allows only diagnostic or soft admission;
calibrated hard bands require an empirical contract. No tolerance was tuned
to either property.

The corrected Summit regression preserves the true parcel `0652990300` among
889 hard-feasible models. Two different parcels reporting 252 units rank
first; Summit does not. Preserving truth fixes admission safety without
claiming that these hints resolve Summit. The original failed archives remain
unchanged in [the first experiment](../addressless_nxrt_2026-09-17/README.md).

Native bounded search also now retains affordable residual models for soft
ranking, including components wider than 128 variables. Search, exact counts
and backbone semantics are unchanged. Tests cover the presentation cap and
assignments without any selected-grain entity.

## Selection and source boundaries

Courtney Cove was the only untested member of the retained two-property Tampa
cohort. Selection at `2026-09-17T15:29:18.576913+00:00` used the recorded seed
`31e0eb118b3166bc0f3dd2aaec9886c2` and minimum SHA256(seed + property name).
This is a held-out case from that small cohort, not a random portfolio sample.
The channel policies and zero unit tolerance were frozen before target lookup.

The hints-only solve was read in a fresh process and hashed at
`2026-09-17T15:47:41.177951+00:00`, before the operator page and city parcel
query were retained at 15:48 UTC. The final build reproduces that solve
byte for byte. The archive preserves this initial readback and the final runs.

- The retained [NXRT company supplement](https://s26.q4cdn.com/937656400/files/doc_financials/2025/q4/NXRT-Esupp-Q4-25-FINAL.pdf)
  and [SEC exhibit](https://www.sec.gov/Archives/edgar/data/1620393/000119312526065382/nxrt-ex99_1.htm)
  provide name, market and units, as of December 31, 2025. The property table
  is on printed page 28; construction year is absent.
- A fresh [Hillsborough county parcel export](https://gisdextweb1.hillsboroughcounty.org/arcgis/rest/services/Hosted/Parcels/FeatureServer/0)
  contains all 889 returned DOR 03xx records without truncation. Its query has
  no target predicate. Every candidate carries the same selected assessor
  fields, including situs address; the hints-only claim contains no address.
- The [operator page](https://livebh.com/apartments/courtney-cove-apartments/)
  supplies `5510 N. Himes Ave`. The external caller records the raw text and
  explicitly supplies `5510 N Himes Ave`, removing the directional's period.
  Canon applies only its declared ASCII case/whitespace comparison. It neither
  fetches the page nor silently drops directionals, suffixes or unit numbers.
- A separate [city parcel view](https://arcgis.tampagov.net/arcgis/rest/services/Parcels/TaxParcel/FeatureServer/0)
  address query returns folio `102933.0100`, the same site address and DBA. It
  derives from assessor data and is not an independent assessor calibration.
  Its parcel ID is evaluation-only and is never supplied as the target.

The offline adapter consumes these pinned local artifacts. Its `prepare`
phase does not read evaluation truth. Its optional `--address-evidence` input
is caller-provided evidence with retained source provenance. `evaluate` uses
a separate native truth-feasibility probe after the run; that probe does not
modify or resolve the discovery request.

## Replay and remaining limits

`measurement.json` reports both final runs, the Summit regression, and their
separate reach, admission, exactness, ranking and cost observations. The source
and run archives retain original bytes, input manifests, native stage outputs,
receipts, and fresh-process verification. Absolute paths are historical;
replay creates new path bindings. The proof class is a retained public-data
experiment, not a 1,000-property accuracy or throughput benchmark.

```bash
cargo build --bin canon
experiment_dir=$(mktemp -d)
tar -xzf scripts/geo_measurements/fixtures/addressless_courtney_2026-09-17/sources.tar.gz -C "$experiment_dir"
uv run scripts/geo_measurements/addressless_asset.py prepare --sources "$experiment_dir/sources" --work-dir "$experiment_dir/hints"
uv run scripts/geo_measurements/addressless_asset.py run --sources "$experiment_dir/sources" --work-dir "$experiment_dir/hints"
uv run scripts/geo_measurements/addressless_asset.py evaluate --sources "$experiment_dir/sources" --work-dir "$experiment_dir/hints"
uv run scripts/geo_measurements/addressless_asset.py prepare --sources "$experiment_dir/sources" --work-dir "$experiment_dir/address" --address-evidence "$experiment_dir/sources/supplied-address.json"
uv run scripts/geo_measurements/addressless_asset.py run --sources "$experiment_dir/sources" --work-dir "$experiment_dir/address"
uv run scripts/geo_measurements/addressless_asset.py evaluate --sources "$experiment_dir/sources" --work-dir "$experiment_dir/address"
```

The single-parcel premise and DOR filter limit reach. Current assessor rows do
not establish 2025 physical truth or complete building composition. Both runs
finish all nine stages but report `ABSTAINED`; total next-evidence ranking lacks
a versioned loss model. The original hard-narrowing success criterion remains
false. The demonstrated gain is correct candidate discovery and corroboration.

Source-neutral address membership and a positive agent-facing association
claim remain work under `bd-3mft` / `bd-33hh`. Source comparability calibration
under `bd-ie4t` also remains open. A unique soft winner must not silently become
the entity-grain resolution claim those features are intended to support.
