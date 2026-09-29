# Step 1 label status (2026-09-28)

Written after `step1_preregistration.md` (commit `6dff912`). Labels were sourced by separate adjudicator subagents and sealed outside the repository
(`outcomes/datacenter-canon-geo-test/sealed/`, local, unversioned). This file holds metadata and sha256 commitments only. No label value has been read by the analyst side.
Adjudicator reports are model output, not verified by a human; the sealed files have not been audited.

| Case | Site role | Label found | Source type | Source date | Grain achieved | Independent | Confidence | sealed sha256 |
|---|---|---|---|---|---|---|---|---|
| DEV-1 | development | partial | State regulatory filing (TDLR accessibility project registration) | 2021-07-06 | address (no parcel id) | yes | high (address/owner/county); none for parcel | `b4c465fffa2d8b8c259a25fdcb9019afc92079ff0811a137445be5c60de2c570` |
| H1-P | known address not placeable | yes | County assessor parcel record (Maricopa) | retrieved 2026-09-28 | parcel level, partial campus enumeration | yes (not a footprint/POI dataset) | medium | `809331e9a04022308094a16da3d8a5cfdeaa67ea7cdfe26aa8af0cf3fc12e250` |
| H2-P | no address | **no** (label null) | none independent; only aggregator/directory sources | none | not achieved | no | none | `d8ab349abac3187d7fd1b73213671a79f55b7b81589f41a5f4c300cffcc099ed` |
| H2-R1 | no address (reserve 1, substituted) | yes | City of Mesa council record (Legistar): development agreement and GPLET lease | 2019-07-01 | campus by intersection description; no parcel ids | yes | medium-high | `3f03bb2dc836a1d25dca2368760693567c947537c7cce1f850dbba169ebcc984` |
| H3-P | constructed ambiguity | yes | City of Huntsville release, operator page, state governor release | 2021-12-30 / 2024-05 | site (city and industrial-park level); no address or parcel | yes | medium-high on identity and the count of other same-owner sites (1); low on the 146 MW value | `ec8145550cec9521346d08660d59911b0835161090dcc6676ce168ef460de79e` |

## Substitutions and gaps (per policy rule 3)
- **H2 primary (Google Storey County) had no independent label.** Reserve 1 (Google Mesa) was used. Reserve 2 is unused. The substitution is a consequence of label availability, so H2 results describe a site whose existence is documented in a municipal record; that is a selection effect to disclose, not to correct.
- The H2-P adjudicator did not verify whether Google has more than one Storey County site, so Storey is not treated as a coverage claim either way. H2-R1: whether Google has more than one Mesa site is unknown.

## Overlaps and caveats to disclose
- **DEV-1 needs a parcel label.** The independent label is address grain only. Per policy rule 4, the Bexar County parcel record supplies the parcel label and the TxGIO Bexar file is excluded from DEV-1's permissible evidence. That parcel label is derived from county appraisal data, so it is not independent of that file; that is why the file is excluded, and DEV-1 is scored at parcel grain only under that arrangement.
- **H1-P** uses a county assessor record. Assessor parcels are not among the permissible evidence sources for H1, so there is no overlap, but the label enumerates the campus partially: extent is not scorable, association is.
- **H3-P** is grain-limited: it labels which facility and roughly where, not a parcel. It also carries a low-confidence MW value, so MW cannot be used to score the case.
- **H2-R1** links to the 183 MW site through the record's Google title and applicant name, not a matching MW or address. That linkage is an inference by the adjudicator.
- The 2019 date on the H2-R1 label predates the as-of date; a site described then may have changed. Not assessed.

## Gate check (protocol Step 1)
| Gate condition | Status |
|---|---|
| Development case has a usable label | Met only through the parcel-record fallback (label not independent of TxGIO; file excluded from that case) |
| At least one holdout per role has an independent sealed label | Met: H1-P, H2-R1, H3-P |
| Every substitution and gap recorded | Recorded above |
| Fresh reviewer or adjudicator audit of sealed labels | **Not done.** Labels are single-adjudicator and unaudited |

Step 1 is complete with those caveats. No accuracy is claimed; this is a diagnostic suite.
