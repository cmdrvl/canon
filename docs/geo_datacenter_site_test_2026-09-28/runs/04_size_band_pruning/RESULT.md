# Run 04: how much would a size band prune? (upper bound)

Run 2026-09-28. Script `prune_test.py`, output `prune_results.json` (each query keeps its `vid`). Read-only.
Question (Zac): our news often carries a site size; canon has an `integer_sum_band` hard constraint. What is a size
band worth?

## Method
For each of the 18 Franklin County parcels the Auditor classes "data center" (labels, notes/truth_candidates_franklin_auditor.md),
put an anchor 550 m from the parcel centroid (median anchor disagreement measured in run 01) at 8 compass bearings
= 144 placements. Candidates = every valid Franklin parcel with centroid within 1,000 m of the anchor. Apply an acreage
band around the TRUE acreage (assumes the stated size is exactly right, so this is an upper bound on the value of a
size, not a measurement of news accuracy):
- wide [0.5x, 2.6x] (the band canon's D1 calibrated for building area on NYC: 13/16 truth in band)
- narrow [0.75x, 1.25x]
Only ACRES is used as a filter. Class, owner and building area are not.

## Result (144 anchor placements)
| | median survivors | mean | unique (== 1) | <= 3 |
|---|---|---|---|---|
| no size band | 840 | 1,141 | 0 / 144 | 1 / 144 |
| wide band 0.5x-2.6x | **14** | 18.0 | 32 / 144 (22%) | 41 / 144 |
| narrow band 0.75x-1.25x | **6** | 7.6 | 34 / 144 (24%) | 52 / 144 (36%) |
The truth parcel is inside the 1 km candidate set in 144 / 144 placements (reach is not the problem once there is an
anchor within about 1 km).

By parcel size (median survivors, wide band): the six parcels of 55 acres or more (55, 78, 95, 143, 222, 480 acres,
the hyperscale-scale campuses) leave **1 to 4** survivors, and 4 of the 6 leave exactly 1. The small colo/enterprise
parcels (4 to 30 acres) leave 10 to 50.

## Reading
- A stated acreage would collapse ~840 candidate parcels to ~14 (60x), and to a unique parcel for the big campuses.
  Those large campuses are exactly the ones with no Foursquare anchor (run 03).
- So the levers are complementary: an anchor within about 1 km (permit coordinate or news address) plus a stated
  acreage plus landed parcels.
- This is an upper bound. It assumes the stated acres are right and describe one parcel. News acreage is often a
  whole-campus or planned-phase number, and campuses can be several parcels (a sum-band over a parcel SET, which canon
  supports but which is a harder search).

## Correction (first attempt was invalid)
The first run counted candidates with a bare `SELECT` of rows. The server caps such a query at 200 rows and silently
returns a prefix, so "candidates = 200" and "truth reached 55/144" were artifacts of truncation. Discarded. The
rewrite aggregates in SQL (`COUNT_IF`), which has no row cap. Lesson: never count rows from a bare SELECT.

## What it does not show
- No measurement of how accurate news-stated acreage is. That needs pairs of (news size, official size), which is the
  calibration canon's doctrine requires before a band may act as a HARD constraint (D1: a named population plus a
  falsification rule). Until then a size band may only be soft or diagnostic, and soft evidence cannot subtract.
- Only Franklin County has landed parcels. Only one news site (AWS campus, Hilliard OH: "72.9 MW", "several acres")
  overlaps; Amazon's four Auditor parcels in that area are 55 to 143 acres each, so that one news size looks loose or
  refers to something narrower. Worth checking, not a conclusion.
