# Data center site test: evidence folder (2026-09-28)

Durable copy of the evidence behind [`../BRIEF_CANON_GEO_DATACENTER_SITE_TEST.md`](../BRIEF_CANON_GEO_DATACENTER_SITE_TEST.md). Same idea as
[`../geo_design_session/`](../geo_design_session/README.md): preserved so every claim in the brief can be traced to what was actually run and
returned. Read the brief first.

The exercise was run from a local working directory, `outcomes/datacenter-canon-geo-test/`, which is not under version control. Paths in the
write-ups and scripts that mention it refer to that directory. This folder is the versioned record.

## Claim tags (same as the brief)
**[V]** verified by a tool result, file hash or re-run. **[D]** read in a document at `HEAD`. **[A]** reported by a research subagent, not
verified. **[S]** read through a web-fetch summarizer. **[J]** judgment.

## Layout
| Path | What it is |
|---|---|
| `runs/00_bridge_site_g/` | Canon site G re-solve on 0.13.0 versus stored 0.12.1: write-up plus the small canon inputs (spec, warehouse rows, evidence request, evidence compilation) |
| `runs/01_epoch_arms_abc/` | Arms A to C on 69 Epoch sites: write-up, `per_site_summary.csv`, `results_epoch69_v2.jsonl` (full per-site evidence, one receipt per call) |
| `runs/02_pryor_arm_d/` | Arm D, canon composition on Google Pryor: write-up plus the full canon input chain (spec, warehouse rows, evidence request, compilation) |
| `runs/03_franklin_parcel_arm/` | Anchor availability for the 18 Franklin "data center" parcels: write-up (with a correction note), `anchors.json` |
| `runs/04_size_band_pruning/` | Size-band pruning upper bound: write-up, `prune_results.json` |
| `runs/05_txgio_spike/` | TxGIO parcel spike: `RESULT_taylor.md` and `RESULT_ector_bexar.md` |
| `runs/06_bexar_arm_e/` | Arm E, canon composition on Bexar parcels: write-up, `arm_e_report.json`, the 12 authored composition requests |
| `notes/` | Doc-review briefs (subagent, **[A]**), truth labels, Franklin county-classed parcels |
| `data/solve_manifest.json` | Size and sha256 of every saved canon solve, and whether it re-solved byte-identically |
| `data/source_snapshots.json` | URL, bytes, sha256, ETag, Last-Modified, attribution for each downloaded source file (Epoch CSVs; TxGIO county zips) |
| `data/txgio_schema_by_county.csv`, `data/txgio_layer_summary.csv` | Field-level schema (dtype, null rate, distinct count) and layer stats for the three Texas counties, no example values |
| `scripts/` | The analysis scripts, unchanged. They import the `canon-geo-site-workup` skill's MCP client from a local path and read MCP credentials from the local Claude config; neither is part of this repo |
| `internal_working_brief_2026-09-28.md` | The working summary I wrote during the exercise, before this brief. Kept unedited for traceability; the docs brief supersedes it |

## Deliberately not included
- **Large canon solve outputs** (0.4 to 7 MB each). They are deterministic: `canon geo solve --request <input>` reproduces them byte for byte (see the
  manifest). Only inputs and hashes are committed.
- **Raw source data.** Epoch AI files are CC BY 4.0 (attribution: "Epoch AI, AI Data Centers. Published online at epoch.ai"); TxGIO parcel zips are
  public records. Neither is copied; `data/source_snapshots.json` pins them by URL and sha256.
- **County extraction summaries with example values.** They contained private individuals' names and addresses from public records. Only
  schema-level facts are kept.
- **Snowflake query logs.** The write-ups and per-site JSONL keep the receipt (`verifiable_id`) for each call.

## Key receipts
| Fact | Receipt / hash |
|---|---|
| Site G stored and re-run solve | sha256 `31cdea2e3521a011...` (both); evidence blake3 `bc6d587393599f28...` |
| Pryor solve | sha256 `a8584ef52d222118...` (406,430 bytes) |
| 18 Franklin "data center" parcels | `sha256:8f0e0cc639cb050cc9b0919a36d7d5ab2819803a35db665b6225a168fa85d9ee` |
| Amazon/VADATA parcel detail | `sha256:2b31f1711c3c1ea62a0804e43ee4a27f81b7969c3eaf5e4e0e5f6be1935020da` |
| News size coverage (38 of 129) | `sha256:085837ef6ca2a1c20ecd16ecf5b650295572b9db54210c39467802e2efd2ab8e` |
| News size by state | `sha256:032b6957e6e1e199e1b09bacaf181523a3103b0ef498050c9e760f88f2db73fd` |
| Pryor Overture buildings within 1.5 km | `sha256:f8d00bb78b088f2d9259d8441b668f13e229547ec2badd9143c2bcccdfdf2471` |
| Pryor Microsoft buildings within 1.5 km | `sha256:5b665b0485251e26dfa1284355a3526d97a036d1804e7f2ce687aa0ef782a7cb` |
| Epoch data_centers.csv | sha256 `3acb45e525d20b38071415e354c139adf898d55838c639c50909f9650f06724c` |
| Epoch data_center_timelines.csv | sha256 `cf72d0072e6b1a104120bc680007d81bc7fbd481ff304da5cfbad8ef1ecae06b` |
| TxGIO Taylor / Ector / Bexar (shapefile) | see `data/source_snapshots.json` |
Per-call receipts for arms A to C are inside `runs/01_epoch_arms_abc/results_epoch69_v2.jsonl`.

## Reproduce
Environment used: canon 0.13.0; `cmdrvl-curves` at commit `9eaecb5` or later for the TxGIO spot scraper; read-only access to the `cmdrvl-data`
and `cmdrvl-geo` MCP servers.
1. Canon solves: `canon geo solve --request runs/06_bexar_arm_e/<case>.request.json` (and `runs/02_pryor_arm_d/pryor.evidence_compilation.json`,
   `runs/00_bridge_site_g/site_g.evidence_compilation.json`). Compare the output's sha256 with `data/solve_manifest.json`.
2. Pryor chain from inputs: `canon geo materialize-evidence --rows pryor.warehouse_rows.json`, then `compile-evidence --request`, then `solve --request`.
3. Arms A to C: `uv run --with mcp --with httpx2 python scripts/run_arms.py <sites.json> <results.jsonl>`. `sites.json` is built from the Epoch CSV
   (address parsing described in `runs/01_epoch_arms_abc/RESULT.md`).
4. TxGIO: `uv run python -m spot_scrapers.txgio_land_parcels.cli snapshot --county taylor --format shp` in `cmdrvl-curves`.
5. Everything queried Snowflake read-only; each table's query contract (state plus exact H3 cell, or an r7 disk) was honored.

## Known weaknesses of this evidence
Two verified official point labels; 18 county-classed parcels; three Texas counties; one vintage of each layer. Epoch sites are large AI campuses.
Composition sources, weights and thresholds were chosen by the author. See the brief's §8 and §9.
