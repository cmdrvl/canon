# Step 1 pre-registration: data-center recovery suite (2026-09-28)

Committed and pushed BEFORE any label is sourced. Protocol: `docs/RESPONSE_CANON_GEO_DATACENTER_SITE_TEST.md`, Step 1.
This is a **diagnostic suite, not an accuracy sample**: 1 development case plus 3 holdout roles, each with an ordered reserve list. No rate is claimed from it.

## Frozen artifacts
| File | Role |
|---|---|
| `step1_case_selection.py`, `step1_selection.json` | Deterministic draw (seed `canon-geo-data-center-recovery-2026-09-28`, ordering `sha256(seed|role|site)`), pool definitions, 38 excluded examined sites, Epoch snapshot sha256 `3acb45e525d20b38...` (full digest in the selection file) |
| `step1_build_manifest.py`, `case_manifest.json` | Analyst-facing packet: question, target relationship, grain, as-of date, supplied observations, permissible and prohibited sources, spatial scope, resource budget |

Blind analysts receive `case_manifest.json` only (one case at a time), never `step1_selection.json` (it names the H3 sites), this directory's other files, or `docs/geo_datacenter_site_test_2026-09-28/`.

## Cases
- **DEV-1** Microsoft SAT14, 3545 Wiseman Blvd, San Antonio TX (47 MW). Chosen deliberately as the development case. It is already exposed in the committed field-test evidence, so it is used to develop and debug, and it is never counted as a holdout.
- **H1** known address the geocoder could not place: primary Microsoft Goodyear AZ; reserves CoreWeave Denton TX, Meta Temple TX.
- **H2** no address, region and attributes only: primary Google Storey County; reserves Google Mesa, Google Kansas City East. The pool held only 3 sites, so a fourth is not available.
- **H3** constructed same-owner ambiguity (name, city, address withheld): primary Meta (AL, 146 MW); reserves Google (VA, 77 MW), Google (VA, 238 MW). Names are in `step1_selection.json` and are not analyst-facing.

## Label policy (fixed now)
1. Labels come from a source independent of every permissible evidence source: state or local permit, planning record, or official project page that names the facility and gives a location or parcel. Adjudicators are separate subagents that never see analyst outputs.
2. Labels are sealed outside the repository. Only sha256 commitments and metadata (source type, date, grain, whether independent) are recorded here. The label values stay out of my working context until scoring.
3. If no independent label is found for a primary, the reserves are used in order; the substitution and the gap are recorded, never silently dropped.
4. Development-case fallback: if no independent label exists for DEV-1, the Bexar County parcel record is the label and the TxGIO Bexar file is excluded from that case's permissible evidence.
5. Grain follows the case (parcel, campus, or site). Extent for a multi-building campus is reported separately and is not required.
6. Overlaps between label sources and permissible evidence are disclosed in `step1_label_status.md`; an overlapping label is downgraded to diagnostic.

## Measures
Frozen before scoring; each is reported per case, not pooled.
- Association correct: the supported association contains the labeled site, or the run correctly declares ambiguity / abstains when the labeled evidence is insufficient.
- Wrong-site commitment: a single association named that excludes the labeled site (the failure to be minimized).
- Retained ambiguity is scored as correct only when the named alternatives include the labeled site and the missing observation is named.
- Superset check: the reported set is no larger than what the evidence supports (reported as a count, not a pass/fail).

## Gate for leaving Step 1
Development case has a usable label; at least one holdout per role has an independent sealed label; every substitution and gap is recorded. A role with no label after reserves is reported as a coverage gap, not filled by a convenient site.

## Not claimed
No accuracy, no release claim (`bd-179b` still gates), no code change. Step 3 for Pryor remains non-blind until a fresh reviewer repeats it.
