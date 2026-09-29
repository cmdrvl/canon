# PRYOR blind Step 3 trace (frozen snapshot only)

Inputs used: protocol Step 3 text; snapshots/PRYOR/{overture_buildings,microsoft_globalml,fema_usa_structures,foursquare_places}.jsonl and manifest.json. No searches, no canon run, no evaluation key. I recognize the place name "Pryor, OK / Google" from general knowledge; I have set that aside and used nothing from it. All numbers below were computed by local Python (shapely, local tangent-plane metres, anchor = origin, x east, y north) over the JSONL.

## 0. Snapshot facts that bound every claim

- Only ONE anchor has a coordinate: A1 = Foursquare place "Google", 4581 Webb St (created 2011-09-29, refreshed 2026-07-15, category "Business and Professional Services", no sub-category). A2 (address string) and A3 (geocoder no_match) have no coordinate. The manifest calls the anchors competing; in practice A2 and A3 restate the same address claim as A1 (one lineage: an address string, a failed geocode of it, and a POI pin carrying it). There is no second location for the site in the snapshot.
- Retrieval = 1.6 km geodesic buffer around A1 only. Parcels unavailable. No Overture address records. Records: Overture 175, Microsoft 168, FEMA 110 (vintage 2023-10-03 release, PROD_DATE 2018-08-21 on rows I sampled), Foursquare 66 = 519 rows / 453 footprint records.
- The pin lies inside NO footprint of any source (0/175, 0/168, 0/110). Nearest footprint edge: 60 m.
- No footprint, name, or place in the snapshot states "data center" for anything. The only Google-named record is the pin itself (a point, not a building).

## 1. Shared lineage: physical entities

Union of footprints by overlap (intersection > 50% of the smaller footprint, across sources) gives 453 records -> 202 physical entities.
Source sets: FEMA+MS+Overture 70, MS+Overture 75, FEMA only 23, Overture only 17, FEMA+Overture 8, MS only 8, FEMA+MS 1.
- 128 of 175 Overture rows list "Microsoft ML Buildings" in their own SOURCES_JSON and 105 list OpenStreetMap. So an Overture and Microsoft polygon with matching shape is usually the SAME upstream footprint (Overture copied it), not corroboration. 139 of 175 Overture rows match a Microsoft polygon at IoU>0.5 for this reason.
- FEMA/ORNL is a separate extraction (2018 production date) so FEMA agreement with MS/Overture is real evidence that the *building existed by 2018 and is one object*, but is still only footprint existence, not membership in any site.
- Effect rule used below: N sources on one entity = 1 physical entity, existence corroborated; membership evidence stays at whatever the single non-footprint clue carries.
- Partition disagreement (grain): the entity at (251,175) is ONE Overture/OSM polygon (11,590 m2, OSM way v4, edited 2022-03-29), ONE FEMA polygon (10,344 m2), but TWO Microsoft polygons (4,985 m2 h 10.1 m and 2,707 m2 h 13.0 m). Whether this is one building or several is unresolved by footprints.

## 2. Candidate groups (kept separate)

Defined by geometry only, not by any membership claim.

| Group | Members (physical entities) | Where |
|---|---|---|
| G-W "west/even side of Webb" | 2 entities: 2,793 m2 (FEMA "4590 WEBB STREET") at 60 m; 2,729 m2 (FEMA "4570 WEBB STREET", Overture name "D & D Industrial SW") at 92 m. Plus 4,585 m2 (Overture name "HE&M Saw") at 193 m and 10,880 m2 (FEMA 4570 Webb) at 245 m as outer ring | x about -90 |
| G-E1 "east/odd side, small" | 5 entities <1,000 m2 within 300 m: 56 m2 (189,36) [O+M], 152 m2 (189,51) [FEMA only, addr "3649 MAIN MAIP STREET"], 901 m2 (188,154) [M+O], 101 m2 (185,221) [M+O], 450 m2 (187,250) [M+O] | 186-295 m NE/E |
| G-E2 "east complex" | 1 entity 11,590 m2 (1 O + 2 M + 1 F polygon) at 224 m, FEMA "3649 MAIN MAIP STREET", occupancy "Professional/Technical Services" (modeled), no name | (251,175) |
| G-P "industrial-park warehouses" | 12 entities >=10,000 m2 in the buffer (128,738 m2 at 636 m; 59,746 at 639; 30,954 at 776; 30,207 at 968; 29,189 at 859; 13,191; 13,015 [Red Devil POI inside]; 12,905; 10,880; 10,294; 10,059; plus G-E2), 5 of which are 29-129k m2, all 511-1,492 m from the pin, bearings 7-105 deg (mostly E/NE). FEMA addresses "3649/3500 MAIN MAIP STREET" (MAIP appears to be the industrial-park street name; I do not assume more) | E/NE |
| G-N "named other operators" | 22 entities carry an Overture name or contain a non-Google Foursquare POI (Fabricut, Red Devil, Harbison Walker/A.P. Green, D&D, HE&M, hotels, restaurants, airport...) | scattered |
| G-U "unknown extent" | everything beyond the 1.6 km buffer: 41 entities already touch d>=1,400 m and one of them >1,500 m2 (10,059 m2 at 1,492 m), so the industrial buildings continue past the cut | edge |

Known members of a Google site: none established beyond the pin. Possible members: G-E1, G-E2 (nearest same-side structures), G-W (nearest but contradicted, see O5/O6). Unknown extent: G-P and G-U.

## 3. Interpretations (subject relationship, time, candidate association)

Each: subject = "Google data center site" as the entity whose buildings the question asks about.

- **H1 pin-parcel facility, east side (present-day, 2018-2026 footprints).** The record at 4581 Webb St denotes a facility on the odd-numbered (east) side of Webb St; its buildings are the nearest east-side structures. Candidate association: G-E1 and possibly G-E2. Relationship: pin = facility address/location, operator = Google (owner or tenant not stated).
- **H2 pin-as-campus-gate (present-day).** The address is the frontage/entrance of a larger campus; the campus is a contiguous run of buildings back from Webb St (G-E1 + G-E2 and onward into G-P). Extent unknown, needs parcel or fence data.
- **H3 pin-on-west-side (present-day).** The intended facility is the 4570/4590 Webb St buildings (G-W), i.e. the pin is slightly mis-placed across the street.
- **H4 pin is a stale/generic office POI; the data center is elsewhere in the industrial park (G-P) or beyond.** The POI is a user-created generic "Business and Professional Services" record from 2011 with no data-center category; it does not itself denote a data center. Candidate association: any G-P building or something outside the buffer.
- **H5 pre-construction / land-only or post-vintage build (time = after 2026-07 footprints, or facility not yet footprinted).** The pin sits on ground with no building in any vintage; the site (or its data hall) may not exist in the footprints at all. Candidate association: none in inventory.
- **H6 answer absent from inventory.** True site lies outside the 1.6 km retrieval or is unrelated to the pin; every candidate above is a wrong-answer decoy. Kept as a standing alternative to H1-H5 (the protocol's "answer absent from inventory" condition); it is not eliminated by anything in the snapshot.

Number of interpretations: 6 (H1-H6).

## 4. Consequential observations and chains

Format: source record and text/geometry -> proposition -> candidates and lineage -> effect.

**O1. Pin not inside any footprint.**
Foursquare fsq 4e84cca2..., POINT(-95.33284 36.23959); distances to nearest footprint edge 60 m (O), 61.5 m (M), 65 m (FEMA). -> "The place 'Google' is located at a point with no building on it." -> affects all groups; all three footprint sources agree (shared lineage here does not matter; it is agreement on absence). -> Supports H5 and H2 (set-back campus) and H3/H4 mildly; contradicts nothing outright; a strict "pin is inside building" H1 variant is contradicted. Point positional error is unknown (Foursquare gives none).

**O2. Address numbering places the pin on the odd side of Webb St, y about 0.**
FEMA PROP_ADDR values on Webb St retained with geometry: 4177 (y about 550-640), 4570 (y=95, x=-92), 4590 (y=-47, x=-93), 5123 (y=-833), 5162, 5218, 5249 (y about -890 to -1100). Numbers rise southward. Linear interpolation puts 4581 at y about +17. Pin y=0. -> "4581 Webb St and the pin are consistent with each other; 4570/4590 are even-side, opposite the pin." -> The pin geometry and the address are one Foursquare record (same lineage), so this shows internal consistency only, not that Google occupies anything. FEMA addresses are modeled point addresses (occupancy provenance "modeled_source_occupancy"). -> Favors H1/H2 over H3 for the pin's own parcel (odd side = east); does not identify a building. Caution: the geocoder returned no_match for that address (A3) so no independent geocode corroborates it.

**O3. Named other operators on west-side and nearby buildings.**
Overture NAMES_JSON "D &D Industrial SW" on the 4570 Webb building (92 m), "HE&M Saw" (193 m), "Fabricut Factory Outlet" (404 m; Foursquare Fabricut POI inside it), Red Devil (FSQ POI inside 13,015 m2 entity; Overture "Red Devil Inc"), Harbison Walker / A.P. Green (FSQ POIs inside a 5,571 m2 entity). -> "These buildings host or hosted other named businesses." Lineage: Overture names come from OSM; Foursquare independent of OSM, so Fabricut and Red Devil have two independent name clues, D&D and HE&M only one (OSM). -> Contradicts (weakly, time-unresolved: OSM edit years 2016-2023, FSQ refresh 2024-2026, and multi-tenancy and successor use are possible) Google membership for those specific entities; supports H4 only insofar as it shows the district is multi-operator. Unknown is not disagreement: nothing here says Google is NOT also present.

**O4. Structures with no operator evidence: the big east/northeast buildings.**
128,738 / 59,746 / 30,954 / 30,207 / 29,189 m2 entities (FEMA+MS+Overture, so 5 physical entities, not 15 records), FEMA addr "3500/3649 MAIN MAIP STREET" (shared address across many separate footprints = one address label on multiple buildings, plausibly one parcel or estate-level address; not independent per-building identity). No name, no POI inside them. OSM edit dates 2020-09-04 to 2021-03-13; MS record 2019-11-09; FEMA 2018. -> "Large buildings exist, in place, since <=2018; occupant unknown." -> Compatible but nondiscriminating for every hypothesis. A size-ranked answer would pick these; nothing here links them to Google. Effect: unknown.

**O5. What sits on the same side of Webb St as the pin.**
Overture/MS: 56 m2 at (189,36), h 4.5 m, OSM w1199938686 (MS also); FEMA-only 152 m2 at (189,51), addr "3649 MAIN MAIP STREET"; 901 m2 (188,154) MS+O, h 5.5 m; 101 m2 (185,221) h 6.3 m; 450 m2 (187,250) h 6.5 m; then the 11,590 m2 entity at (251,175). All exist in the 2018 (FEMA) or 2019 (MS) vintage where checked. -> "A line of small structures at x about 186-189 m east of the pin, set back about 190 m, then a larger building." Lineage: each is the same object across sources except the FEMA 152 m2 vs the O/M 56 m2 which are adjacent 14 m apart, non-overlapping (two small objects, not one). -> Compatible with H1/H2 (parcel with entrance-side ancillary structures behind the pin, if the parcel runs east); also compatible with H5 (pin on vacant frontage) and H4. Nondiscriminating between them without parcel/ownership data. Note: same address label "3649 MAIN MAIP STREET" on the 152 m2 and the 11,590 m2 entities is a weak link of those two to one FEMA address, opposed to the pin's own "4581 Webb" address. It suggests they belong to the MAIP-address estate, not to 4581 Webb. This favors H4/H2-with-different-address over H1.

**O6. Small objects that a size filter drops.**
102 of 202 physical entities are under 200 m2; 168 of 202 are under 1,000 m2. Within 300 m of the pin 10 of 15 entities are under 1,000 m2 (the five in G-E1, plus 56/22 m2-scale west ones and others). A >=1,000 m2 (or >=2,000 m2) filter would delete the entire same-side set G-E1 (56, 101, 152, 450, 901 m2) and leave only G-W (60/92 m) and G-E2 (224 m). That changes which candidate is nearest and which side of the street the answer sits on: the filtered answer is "the west-side 4570/4590 buildings" (H3-shaped) while the unfiltered inventory keeps H1/H2's east-side structures. The small objects could be guard/utility/equipment buildings whose function I cannot see; no source states it. -> unknown effect, high consequence for candidate reach.

**O7. Partition of the 11,590 m2 complex.**
Overture/OSM one polygon (h None), FEMA one polygon (10,344 m2, "Professional/Technical Services" modeled), MS two polygons (h 10.1, 13.0). MS heights 10-13 m are unusual against nearby 3-8 m; only one other 14.7 m (a 32,571 m2 entity at 858 m). -> "Complex may be one building or two/more taller blocks." Lineage: MS heights are from an ML model (CONFIDENCE -1 everywhere = unreported), so height evidence is one source, weak. -> Nondiscriminating; flags grain risk (count of buildings depends on source).

**O8. Vintage/time asymmetry.**
FEMA PROD_DATE 2018-08-21. Entities present in Overture+MS but absent from FEMA with area >=1,500 m2: 13,191 (1,276 m), 12,905 (1,086 m), 10,059 (1,492 m; OSM "warehouse", OSM edit 2025-10-08), 5,708 (1,373 m; OSM edit 2025-10-08), all NE/E at 43-74 deg. Also Overture-only 2,821 m2 at 653 m. -> "These four were likely built after the FEMA vintage, or FEMA missed them; MS 2019 source date on three of them suggests earliest 2019." -> Time-relevant to H5/H4: if a Google facility were recent, the recent large buildings are more likely candidates than the 2018-era stock; but there is no Google/operator link. Effect: unknown; not to be promoted to support. 3 of 4 lie at >1,270 m, near the retrieval cut, so their neighbors may be missing (G-U).

**O9. Foursquare pin history.**
Created 2011-09-29; category generic; refreshed 2026-07-15; DATE_CLOSED None. -> "Something listed as Google at 4581 Webb St has persisted in that listing's freshness." Single Foursquare record, no corroborating POI, no second Google-named place in 66 POIs. -> Supports existence of a Google-labelled business location at/near the address; says nothing about whether it is a data center or what buildings it includes. Favors H1-H3 over H6 on pin reality, is nondiscriminating between H1-H5.

**O10. Retrieval boundary.**
Manifest: 1.6 km buffer around the single pin, polygons not clipped, counts stable with k+1 disk. -> Extent of any site extending beyond 1.6 km (or of the park) is not observed. -> Keeps H6/G-U alive; supports "unknown extent" not "no more buildings".

## 5. What is supported

Counts, by identification method (computed above):
1. Supported (existence of an association at the point level): one Foursquare place named Google at 4581 Webb St, consistent with an odd-side Webb St address by FEMA neighbour-number interpolation (y +17 m vs pin y 0). Not a building; no membership.
2. Known members of a Google site (building-level): 0. No footprint contains the pin; no footprint carries a Google name or operator link; the ONE operator name available for the nearest buildings on the west side belongs to someone else (D&D Industrial SW, HE&M Saw; time unresolved).
3. Possible members (nearest, positional only): 2 entities on the even/west side at 60 and 92 m (weakened by O2/O3), and 6 entities on the east/odd side within 300 m (5 small, 1 complex of 11,590 m2). Ranked purely by proximity these are candidates, not supported members.
4. Ambiguous large group: 12 industrial-park entities >=10,000 m2 (5 of them 29-129k m2) at 511-1,492 m; no evidence tying them to the pin's operator; their FEMA "MAIN MAIP STREET" address is a district address, not a Google link.
5. Unknown extent: everything past 1.6 km; 41 entities touch d>=1,400 m.

Whether the snapshot alone supports a single winner: NO. The evidence supports "a Google-labelled place at 4581 Webb St, on a parcel that is not building-covered at the pin," and nearest structures on the same side are small ancillary-scale objects plus one mid-size complex, none named. H1, H2, H4, H5 and H6 are all still open; H3 is disfavoured (odd/even side, named other operators) but not eliminated because pin error is unbounded. Do not select G-P by size and do not drop G-E1 by size.

Answer-absent-from-inventory (H6): stays live. The bounded 1.6 km list cannot prove the true site is in it, and the anchor's only independent claim is a POI whose category does not say data center.

## 6. Missing observations that would settle it

1. Parcel/lot boundaries and ownership or lessee for the parcel containing 4581 Webb St (Mayes County parcel layer is stated unavailable): would say which east-side footprints belong to that parcel (H1 vs H2 vs H4) and whether the pin is on vacant land (H5).
2. Any source that states Google as owner/operator of a building or parcel (permit, tax, utility, interconnection or filing record), with its footprint or address, so a building can be given a positive relation instead of adjacency.
3. Building-function evidence for the small east-side objects (56, 152, 101, 450, 901 m2): a use/permit/imagery label would show whether they are ancillary to a larger campus.
4. Extent beyond 1.6 km (a second ring or a footprint-and-address retrieval keyed on the operator-linked parcel), to test H6 and G-U; specifically the industrial buildings continuing NE/E at the buffer edge.
5. Imagery or footprints of a later vintage (post-2026-07) or construction records for the vacant pin ground (H5).
6. Overture/OSM address records or a working geocode for 4581 Webb St (A2/A3 currently no coordinate; Overture address coverage zero).
7. Positional accuracy of the Foursquare pin (would bound H3).
