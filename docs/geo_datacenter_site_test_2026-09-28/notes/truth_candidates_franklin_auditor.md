# Truth candidates: Franklin County OH parcels classed "data center" by the Auditor

> **CORRECTION (2026-09-28):** later text says "the four Amazon parcels above". Only three Amazon/VADATA parcels are in the table of 18;
> the fourth (275-000010) is listed separately below as classed "OTHER INDUSTRIAL".

Queried 2026-09-28 from `EDGAR_DB.SOURCE.FRANKLIN_COUNTY_AUDITOR_PARCELS_HOT` (release 2026-09-01, current release,
landed from the Franklin County Auditor's public parcel export). The label is the Auditor's own land-use class
(`CLASSDSCRP = 'data center'`), so it is independent of any geocoder, Epoch, or our news pipeline.
Receipts: all 18 parcels `sha256:8f0e0cc639cb050cc9b0919a36d7d5ab2819803a35db665b6225a168fa85d9ee`;
Amazon/VADATA detail `sha256:2b31f1711c3c1ea62a0804e43ee4a27f81b7969c3eaf5e4e0e5f6be1935020da`.

Caveats: an assessor class can be stale or wrong. The Auditor's disclaimer says the primary source controls.
Owner names are shells or holding entities in several rows; **do not attribute a parcel to a company from the owner
name unless the record says so.** Rows below are labels for "a data-center parcel exists here", not for which
Epoch site it is.

| Parcel | Site address | Zip | Owner (as recorded) | Acres | BLDGAREA (sq ft) | Sale date |
|---|---|---|---|---|---|---|
| 510-180711 | 225 RATHMELL RD | 43137 | MAGELLAN ENTERPRISES LLC | 480.4 | 1,172,656 | 2022-06-08 |
| 050-011455 | 4580-4640 COSGRAY RD | 43026 | AMAZON DATA SERVICES INC | 95.0 | 880,352 | 2022-10-18 |
| 222-004984 | 5266 BABBITT RD | 43054 | MONTAUK INNOVATIONS LLC | 221.9 | 811,338 | 2018-12-13 |
| 050-011444 | 5109-5129 HAYDEN RUN RD | 43026 | VADATA INC | 55.2 | 768,498 | 2016-08-18 |
| 050-002806 | 4120 SCIOTO DARBY CRK | 43026 | AMAZON DATA SERVICES INC | 143.3 | 695,120 | 2024-08-15 |
| 610-207094 | 7500 ALTA VIEW BLVD | 43085 | COLOGIX COL4 LLC | 7.3 | 273,264 | 2022-12-15 |
| 222-002127 | 7600 W CAMPUS BLVD | 43054 | EDGED COLUMBUS LLC | 14.8 | 207,192 | 2023-12-29 |
| 610-210593 | 555-585 SCHERERS CT | 43085 | COMMUNICATIONS REALTY INVESTMENTS COLUMBUS JV LLC | 7.6 | 206,724 | 2016-06-21 |
| 222-004355 | 7300 SOUDER RD | 43054 | SI NAL01A ABS LLC | 28.5 | 115,500 | 2025-12-02 |
| 222-002056 | 7205 E NEW ALBANY CONDT RD | 43054 | DISCOVER PROPERTIES LLC | 19.0 | 100,116 | 2023-03-08 |
| 050-011895 | 4861 EDWARDS FARMS RD | 43026 | CLOP HILLIARD LLC | 15.0 | 90,108 | 2023-03-24 |
| 222-004441 | 6595 NEW ALBANY E RD | 43054 | TJX COMPANIES INC | 12.0 | 84,930 | 2010-07-23 |
| 222-001940 | 6595 NEW ALBANY RD | 43054 | COMPUGEN REAL ESTATE HOLDINGS INC | 7.8 | 31,192 | 2023-02-06 |
| 222-004644 | 6625 E NEW ALBANY RD | 43054 | OHIO POWER COMPANY | 9.2 | 31,129 | 2014-10-01 |
| 273-012619 | 5700 INNOVATION DR | 43016 | CONTINENTAL BROADBAND LLC | 4.4 | 28,732 | 2026-03-20 |
| 222-004365 | 6650 E NEW ALBANY RD | 43054 | MOTORISTS MUTUAL INSURANCE CO | 7.1 | 17,613 | 2009-07-17 |
| 222-005361 | 5760-5780 BABBITT RD | 43054 | QTS NEW ALBANY III LLC | 78.2 | (null) | 2025-09-26 |
| 050-011984 | SCIOTO DARBY CREEK RD | 43026 | OHIO POWER COMPANY | 9.1 | (null) | 2023-12-20 |

Also found: 275-000010 (6685 CROSBY CT, 43016), VADATA INC, 66.8 ac, 768,498 sq ft, classed "OTHER INDUSTRIAL
RESEARCH/CHEMICAL LAB", not "data center" (same owner and building area as 050-011444: likely a sister site).

## What the auditor lookup ruled out (for the Epoch Ohio sites)
- No Franklin SITEADDRES matches "5076 S HIGH", "1101 BEECH", "13360 MILLER", "1 COMMUNITY C" or "1500 BEECH" (only an
  unrelated house at 1101 BEECHWOOD RD, zip 43227). Google New Albany (1101 Beech Rd SW) is therefore not a Franklin
  site-address record; the New Albany city page (per the Ohio agent, unverified) lists it at "1101 Beech Road".
- No parcel in Franklin is owned by an entity named GOOGLE%, META PLATFORMS%, or FACEBOOK%.
- The Epoch Ohio list has no site in Hilliard/Dublin, so the four Amazon parcels above are absent from Epoch.
- Open question for adjudication: 225 RATHMELL RD (480.4 ac, 1.17M sq ft, owner MAGELLAN ENTERPRISES LLC) is the right
  size and area for Epoch's "Google Columbus" (5076 S High St, Columbus 43207), but nothing in the record says Google.
  Distance between them will be measured from the arms results. Do not label it as Google from the owner field.

## How this can be used
1. Footprint test with no geocoder: for each labeled parcel, do the large Microsoft/Overture/FEMA footprints fall
   inside the parcel polygon? (scores arm C's candidate ranking against official parcels).
2. Blind resolution test: give arms B/C only an operator name and city and see whether they find these parcels.
3. Cross-source gap: 18 official data-center parcels vs how many appear in Epoch and in our news sites.
