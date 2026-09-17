# Three more NXRT properties: Orlando

**Two of three properties were discovered from hints and corroborated by a
subsequently supplied operator address. Cornerstone remains unresolved by the
native descriptive profile.** All six solves retain 2,983 hard-feasible parcel
models and force no identity.

| Property | Operator address (Orlando, FL) | Hints-only result | Supplied-address result |
|---|---|---|---|
| [Residences at West Place](https://livebh.com/apartments/residences-at-west-place-apartments/) | 753 Sherwood Terrace Dr, 32818 | Unique soft best: `282223920000100` | Same parcel; name + address agree |
| [Cornerstone](https://livebh.com/apartments/the-cornerstone/) | 2409 South Conway Rd, 32812 | All 2,983 tie; no name agreement | All 2,983 still tie; no address agreement |
| [Sabal Palm at Lake Buena Vista](https://livebh.com/apartments/sabal-palm-at-lake-buena-vista/) | 13675 Lake Vining Dr, 32821 | Unique soft best: `282427000000023` | Same parcel; name + address agree |

For each positive case, the best/next soft cost changes from **0/1** to **0/2**.
These are absent-preference costs, not probabilities. Separate county address
points explicitly link the same recorded street addresses to the same parcel
IDs; that is source association evidence, not complete-property extent.

Cornerstone's likely reference parcel is `302304550403000`. The county records
`CORNERSTONE, THE` and `2409 CONWAY RD`; its address-point layer likewise omits
the operator's `South`. The bounded number/basename query locates this reference
candidate but is not an exact match of the full operator address. It is retained
for diagnosis, not injected as a required parcel. No article removal, name
reordering, or directional stripping was added to make the case pass. A better
source alias/address-membership contract is the concrete follow-up.

This experiment repeats the Courtney Cove hints/address comparison on all three
Orlando properties in the retained NXRT December 31, 2025 property table:
Residences at West Place (342 units), Cornerstone (430), and Sabal Palm at Lake
Buena Vista (400). Canon runtime code and the descriptive admission policy are
unchanged from `db72dfa`.

The cohort was fixed at `2026-09-17T17:18:47.036602+00:00`, before target-address
lookup. Seed `081c7912a638242815e3eda4ea15dfce` orders the three by
SHA256(seed + property name). This is the complete disclosed Orlando cohort,
chosen to test another county, not a random portfolio sample.

## Inputs and interpretation

The [Orange County Property Appraiser parcel layer](https://vgispublic.ocpafl.org/server/rest/services/DYNAMIC/Dynamic_Parcels/MapServer/3)
returned 2,992 geometry rows for `DOR_CODE LIKE '03%'`. All three pages agree
with the separately fetched count and object-ID list. Grouping by the native
parcel ID yields **2,983 parcel candidates**. Nine additional geometry rows
belong to three already represented parcel IDs. Their parts contribute to
blocking centroids; each parcel participates once in the candidate universe.
Any disagreeing descriptive attributes would remain unknown; none conflicted.

The county schema has `PROP_NAME`, `SITUS`, and `AYB`, but no explicit
residential-unit-count field. `LAND_QTY` is a land valuation quantity; its
unit code is not a sufficient residential-count contract. Every candidate's
unit count therefore remains unknown. The disclosed counts are retained in
the questions and cannot exclude or support any candidate in these runs.
After the hints runs, the three source-linked reference parcels' land valuation
quantities were observed to equal 342, 430, and 400. That agreement is retained
as context; it does not retroactively change the frozen unit interpretation.

Known name and separately supplied address agreements have soft weight 1,
using ASCII case/whitespace comparison. Category and year are diagnostic.
No name aliases, suffix expansion, or source-specific tolerance was added.
The only hard constraint is the declared single-parcel question premise.
Names, addresses, and year from one assessor record share lineage.

The external caller acquires source evidence; Canon's plan/run remains offline.
The native hints-only solve artifacts were saved, read in a fresh Python
process, and hashed at `17:34:51 UTC`, before operator-address acquisition at
`17:35:10–11 UTC`. The full runs subsequently receive native `geo inspect`
readback and resume checks. The later address pass uses the same inventory
and policy. No target parcel ID is supplied as a resolving constraint.

## Retained artifacts and replay

`sources.tar.gz` retains acquisition URLs/times/hashes, raw county pages,
the REIT disclosure, frozen selection, operator evidence, and the small offline
input-assembly adapter. `run.tar.gz` retains native inputs, plans, stage outputs,
receipts, fresh-process inspections, and resume checks. `measurement.json`
reports the outcomes and archive hashes.

All six runs finish all nine stages with status `ABSTAINED`. Fresh native
inspection succeeds; a subsequent resume reuses all nine stages and preserves
their bytes. The original hints solve hashes are unchanged after address
lookup and after the prospective representation change. Regenerating inputs
from the archived adapter reproduces all 39 checked input artifacts exactly.
The original hard-narrowing success criterion remains false in all six runs.

Validation on unchanged runtime revision `db72dfa`: `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, and `cargo test` pass. The full
suite reports 3,158 passed, 11 ignored, and zero failures; logs are retained in
the run archive. No goldens or runtime behavior changed.

The optimized measurement binary is built from unchanged `db72dfa` with
`CARGO_PROFILE_RELEASE_OPT_LEVEL=2`, `CARGO_PROFILE_RELEASE_LTO=false`, and
`CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`. Its hash is recorded before execution.
Concurrent tests and runs make observed elapsed times unsuitable as a throughput
benchmark.

The initial hypothetical separation inputs used dense 0/1 numeric masks. After
all six solves and explanations completed, that extra analysis remained CPU
bound. Those attempts were interrupted after roughly 16 minutes for hints and
12 minutes for addresses. The two outcomes were then expressed as explicit
singleton allowed sets, with identical membership checked for every one of the
2,983 feasible models. The native runner resumes the seven completed stages;
no current evidence, candidate, or solve changes. Both encodings, interruption
records, and equivalence checks are retained. Completion-attempt time therefore
excludes earlier interrupted work and must not be reported as total run cost.

To reproduce a case with fresh path bindings, use index 0 (West Place), 1
(Cornerstone), or 2 (Sabal Palm). Acquisition is not repeated by these commands.

```bash
CARGO_PROFILE_RELEASE_OPT_LEVEL=2 CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 cargo build --release --bin canon
experiment_dir=$(mktemp -d)
tar -xzf scripts/geo_measurements/fixtures/addressless_orlando_2026-09-17/sources.tar.gz -C "$experiment_dir"
case_index=0
uv run "$experiment_dir/sources/prepare.py" --sources "$experiment_dir/sources" --work-dir "$experiment_dir/hints" --subject-index "$case_index" --canon "$PWD/target/release/canon"
python3 "$experiment_dir/sources/run_native.py" --work-dir "$experiment_dir/hints" --canon "$PWD/target/release/canon"
uv run "$experiment_dir/sources/prepare.py" --sources "$experiment_dir/sources" --work-dir "$experiment_dir/address" --subject-index "$case_index" --canon "$PWD/target/release/canon" --address-evidence "$experiment_dir/sources/case-$case_index/supplied-address.json"
python3 "$experiment_dir/sources/run_native.py" --work-dir "$experiment_dir/address" --canon "$PWD/target/release/canon"
```

Each case is a parcel-candidate association experiment. The county filter is
not proof of complete candidate reach; current data does not establish 2025
physical truth. Neither a top soft rank nor a source-recorded address proves
complete property, building, or collateral membership. The hard-narrowing and
accepted-identity requirements remain open under `bd-ie4t`, `bd-3mft`, and
`bd-33hh`.
