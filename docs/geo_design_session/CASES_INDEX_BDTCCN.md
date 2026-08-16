# bd-tccn Worked Case Index

This index tracks the operator ladder from `bd-tccn`. Every completed case records
structured Loom results and exact SQL in its own file.

Standing measured context used across cases:

- Appendix E: baselines to beat are naive address-string at 28.89% and geometry-only PIP
  at 94.65%; nearest_rooftop_match is the named silent-error tier.
- Appendix F: canonical footprint-to-parcel predicate is geometric
  `ST_AREA(intersection)/ST_AREA(footprint) > 0.5`; source asserted areas are not
  denominators.
- Appendix G: feature-count sizing is component-wise; r10/r9 tile counts do not define
  solver cost by themselves.

| case | file | status | property | verdict | design decision forced |
|---:|---|---|---|---|---|
| 1 | `CASE_1_CLEAN_ROOFTOP.md` | complete | One Grace Court Corporation, 1 Grace Court, Brooklyn | resolved singleton: parcel `3002510001` plus one building observation from each footprint source | Defines the unambiguous floor and ablation control; both baselines clear when the exact assertion row is pinned |
| 2 | `CASE_2_ROADBED_GEOCODE.md` | complete | 982 Madison Street, Brooklyn | resolved singleton by address after geocode channel abstains; nearest-lot probe picks wrong BBL `3033570147` | Tile-bounded proximity, no snap-to-nearest, address channel as discriminator |
| 3 | `CASE_3_RANGE_ASSEMBLAGE.md` | complete | 107-109-111 North 9th Street, Brooklyn | resolved assemblage: three parcels `3023030029`, `3023030028`, `3023030027` plus three NYC BINs | Assemblage, interval semantics, endpoint expansion, and why one BBL is a false answer |
| 4 | `CASE_4_CHIMERA_MULTI_STREET.md` | complete | 199, 201, 203, 205 First Avenue and 349 & 351 East 12th Street, Manhattan | resolved six-parcel core plus explicit `351/353 EAST 12 STREET` address-set gap; parsed `199 E 12th St` is rejected as synthesized | Multi-address fields, chimera parse detection, and parsed-address membership checks |
| 5 | `CASE_5_*.md` | pending | two addresses, same corner building | pending | Address disagreement can be noise; geometry may have to win |
| 6 | `CASE_6_*.md` | pending | dense block, multiple buildings to one parcel | pending | Building-level false-merge risk when parcel geometry cannot discriminate |

Case 1 source availability snapshot:

| source | status |
|---|---|
| MapPLUTO | landed, 856,614 rows |
| NYC building footprints | landed, 1,081,999 rows |
| FEMA USA Structures | landed for NY, 5,015,922 rows |
| Microsoft GlobalML | landed for NY, 5,424,624 rows |
| Overture | aggregate/features/buildings/places report 0 NY rows |
| NYC PAD | not landed per bd-35qg; record as gap when a case needs an address set |
