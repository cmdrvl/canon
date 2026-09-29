# Run 01: arms A-C on 69 addressed Epoch US sites (v2, guard fixed)

Run 2026-09-28 by `run_arms.py` (own code; reuses the canon-geo-site-workup MCP client, skill untouched).
Read-only against the geo MCP and Snowflake. Files: `results_epoch69_v2.jsonl` (full evidence, every call keeps its
`vid`), `per_site_summary.csv`. The first run (`results_epoch69.jsonl`, old guard) is kept for the audit trail.
Sample: Epoch AI Frontier Data Centers Hub, snapshot 20260928, 69 US rows that have an Address string. These are
large AI data centers, mostly hyperscaler and neocloud campuses. It is not a sample of data centers in general.

## Coverage by arm
| Arm | Result (n=69) |
|---|---|
| A. Census geocoder, guarded | exact_verified 31 · no_match 29 · false_exact 1 · not_attempted 8 (address has no city/state) |
| B. + Foursquare address / operator name | poi_address 12 · poi_name_only 4 · none 53. Rescued 3 sites A missed; corroborated 9 that A had |
| Anchored (A or B) | **34 of 69 (49%)**: 31 by A, 3 by B |
| C. + large footprints (>=100k sq ft, <=1.5 km) | ranked_candidates 31 · anchor_only 3 · city_level 27 · unresolved 8 |

Rescued by B: Google Pryor, Google New Albany, QTS Richmond 1. The one remaining false exact is Meta Montgomery
("Co Rd 42" -> "42 COUNTY CT").

## The false-exact guard, audited
First run flagged 3 false exacts. On inspection only 1 was right; 2 were my guard's mistakes (Hwy 54 = State Rte 54;
a matched number inside the range "14436-14998"). Fixed and unit-checked (8 cases). Lesson for the test: a guard needs
its own error rate reported, and demotions must be audited by hand.

## Anchors are hints, not sites
On the 9 sites where the Census geocoder and Foursquare both give a point, they differ by 87, 111, 233, 312, 552,
677, 681, 1,017 and 1,017 m (median 552 m). Two Epoch rows (Core42 and Anthropic Lake Mariner) share one address.
The candidate radius (1,500 m) is about the same scale as anchor disagreement, so "anchor + nearby large building"
is a ranking hint, not an identification.

## Footprint layers mostly agree on presence
Among the 34 anchored sites, large buildings (>=100k sq ft within 1.5 km) appear in: Microsoft+Overture+FEMA 22,
Overture+FEMA only 4, Overture only 3, Microsoft+Overture 1, Microsoft+FEMA 1, none of the three 3. So the layers
usually agree that something big is near the anchor; they do not say which building is the data center.

## Truth-scored (two verified official point labels, notes/truth_labels_v0.md)
| Site | Official point | Pipeline | Footprints within 1.5 km of the official point |
|---|---|---|---|
| Google Pryor OK | 36.24250, -95.33020 (ODEQ memo) | anchor via Foursquare, **401 m** from truth | Microsoft 10 (nearest 352 m), Overture 12 (148 m), FEMA 3 (150 m; largest 1.31M sq ft) |
| Meta Los Lunas NM | 34.828611, -106.781389 (NMED notice) | **no anchor** (Census no_match even with the permit's spelling "Messenger Loop NW"; Foursquare none) | Microsoft 3 (nearest 943 m), Overture 10 (185 m), FEMA 2 (940 m) |

Los Lunas is the informative case: evidence to rank exists (Overture has 10 large buildings within 1.5 km), but no
arm finds the anchor. Overture is ahead of Microsoft and FEMA on this new site.

## Corrections to earlier statements
- I said FEMA (2023) missed the Pryor campus. Around the official point FEMA has three buildings over 100k sq ft
  (largest 1.31M). My earlier read used a single small H3 cell around the Foursquare pin.

## What this does NOT show yet
- Nothing about canon composition (arm D). Arms A-C are ladder + evidence gathering only.
- Only 2 verified truth points. Any accuracy statement would be an over-claim.
- No claim that a ranked candidate is the site.

## Caveats
- Epoch addresses are Epoch's; 8 rows have none and 8 more do not parse to city+state. Address quality is a source
  property, not a canon property.
- Foursquare "release 2026-08-11"; Microsoft 2026-07-24; FEMA 2023-10-03; Overture 2026-07 (ODbL, keep segregated).
- ORDER of ladder matters: A first, B only if A fails. The 9 corroborated sites show B would have given a different
  anchor even when A succeeded.
