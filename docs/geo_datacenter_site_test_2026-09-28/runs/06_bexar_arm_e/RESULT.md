# Run 06: arm E on Bexar County TX (canon composition over landed parcels, set-sum size band)

Run 2026-09-28. Script `run_arm_e_bexar.py`; every request and solve is saved here (`*.request.json`, `*.solve.json`),
plus `arm_e_report.json`. Parcels: TxGIO 2025 Bexar file (spot scrape 20260929T000034Z). Canon 0.13.0.
Rules fixed before running: universes A, B, C below; bands +/-5%, +/-10%, +/-25% and canon's NYC-calibrated D1 band
0.5x-2.6x; parcel acres derived from polygons (Bexar's `LEGAL_AREA` is 13% filled).

## The case
KSAT, 2026-09-22 (read through a fetch summarizer, quote not independently verified): "the 158-acre Microsoft Texas
Research Park" in Bexar County. Our news row (GDELT_DC_SITES_OFFICIAL, "Bexar County") kept the size (158-acre, scope
"site or campus") but **dropped the operator and the park name**: OPERATORS and TENANTS are empty. That is an
extraction gap. In the county's legal descriptions "Texas Research Park" is a residential subdivision name (KB Homes,
HOA parcels), so no county field labels Microsoft's campus; **there is no independent truth for the 158 acres**. This run
reports what canon enumerates and refuses. It is not an accuracy measurement, and the band is a hypothesis about the
article's size, not calibrated evidence (canon's doctrine reserves hard bands for calibrated sources), so results are
workbench output, not release claims.

## Results
| Case (universe) | Band on 158 ac | Canon status | Feasible parcel sets |
|---|---|---|---|
| **A** Microsoft-owned parcels within 2.5 km of 15434 Lambda Dr: 2 parcels, 116.9 ac total | +/-5%, +/-10%, +/-25% (min 118.5 ac) | **conflict** | **0** |
| A | D1 0.5x-2.6x (79-411 ac) | ambiguous | 2 |
| **B** all 11 Microsoft-owned Bexar parcels >= 4 ac (326.4 ac) | +/-5% / 10% / 25% / D1 | ambiguous | 173 / 338 / 849 / 1,860 (all tie at cost 0: no preference supplied) |
| **C** any owner within 2 km, >= 22 ac, 15 parcels (786.9 ac), soft preference for Microsoft (cost 3 per absent) | +/-5% | ambiguous | 420 feasible, **17 tied at cost 0** |
| C | +/-10% | ambiguous | 912 feasible, **28 tied at cost 0** |
| C | +/-25% / D1 | ambiguous | 2,244 / 17,716 |

## What the enumeration says
- **Case A is a typed conflict.** No set of Microsoft-owned parcels near the Lambda/Omicron cluster sums to about 158 acres
  (there are only 116.9). So either the article's 158 acres includes land the county lists under other owners, or it
  uses a different definition. Simple arithmetic gives the same warning here; canon adds an exact statement that no
  composition exists, and returns a set the moment the band is widened past 116.9.
- **Case B: a size band alone cannot pick.** Microsoft owns land in two clusters (Lambda/Omicron; Wiseman/Rogers), and
  hundreds of parcel subsets sum near 158.
- **Case C: the best explanations are exactly enumerated.** In every cost-0 model both Microsoft parcels (15434 Lambda Dr
  94.5 ac and 14785 Omicron Dr 22.4 ac) are present and the remaining 25 to 57 acres come from one or two neighbor
  parcels. At +/-10% the filler candidates, by distance from the Lambda parcel: WALAN VENTURES LLC 15355 Lambda Dr 25.3 ac
  (519 m; in 7 of 28 ties), TEXAS RESEARCH & TECHNOLOGY FOUNDATION 14815 Omicron Dr 22.9 ac (836 m; 6/28), WALAN
  Lambda Dr 52.2 ac (886 m), LADERA I 46.9 ac (1,182 m), VANTAGE DATA CENTERS TX1 44.7 ac (1,327 m; 1/28), C1 SAN
  ANTONIO V TRP 22.0 ac (1,545 m; 6/28), LADERA I Vistablue Ln 23.2 ac (1,804 m), QT SOUTH LLC Hwy 211 22.4 ac
  (1,825 m), RANCHO COACHELLA Hwy 211 31.0 ac (1,916 m; 7/28), POTRANCO PATIENCE 40.2 ac (1,983 m), C1 SAN ANTONIO XI Lambda
  Dr 25.7 ac (1,997 m; 7/28).
- **What canon cannot do with this request.** Nothing in the request says the parcels must touch, and canon
  cannot tell an adjacent filler from a parcel 2 km away. It also cannot tell whether an owner is a Microsoft affiliate
  or landlord. The tie-break among the 17 or 28 best models is not evidence; the cost weights are hand-chosen.

## Value versus a spreadsheet
Canon returned exact counts (420/912 feasible; 17/28 best), a typed conflict, and an audit trail per constraint. An analyst
could reproduce the subset sums, but not the typed statement that the model set is empty or the exact tie structure
without code. The added value is exactness and honest ambiguity, not a location answer. Without adjacency, ownership
relationship or calibrated evidence, composition ends in a list of alternatives.

## Mistakes and process notes
- First attempt failed: canon returned `E_PARSE` because each hard constraint must be wrapped as
  `{"id": ..., "constraint": {...}}`; fixed after reading the schema and a known-valid request from site G.
- I also tried to clear stale request files with an `rm` glob; the guard blocked it, correctly (this repo's rule is no
  deletions without permission). The script overwrites its own files, so nothing needed deleting.
- The article quote came through a fetch summarizer. The character spans in our own extraction ("Bexar County" at 579-591,
  "158-acre" at 597-605) are consistent with the quoted sentence, but that is not the same as reading the page.

## Caveats
- One county, one article, no truth label.
- Ownership in the county file is not proof of a data center or of Microsoft's campus boundary.
- Sets larger than 16 parcels were not attempted (2^16 assignment budget).
