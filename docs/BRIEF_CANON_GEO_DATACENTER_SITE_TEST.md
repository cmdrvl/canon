# Canon geo on data center sites: a field test, and what it says about the plan

**Status:** working brief for a brainstorm, 2026-09-28. Written to be attacked, not to defend the plan.
**Question the brainstorm should answer:** what may be wrong with the Canon geo plan, judged against a field test on a
real, awkward asset class?
**Beads:** `bd-dzdy1` (cmdrvl-curves, the test), `bd-399so` and `bd-ncw6p` (Epoch landing/scraper), `bd-2opq` (Franklin parcel
plane), `bd-179b` (truth rebuild that gates every release claim).
**Evidence folder:** [`geo_datacenter_site_test_2026-09-28/`](geo_datacenter_site_test_2026-09-28/README.md) holds every run
write-up, the scripts, the small canon inputs and a hash manifest. Nothing here is a release claim; canon composition
output is workbench data (`PLAN_CANON_GEO.md` P:107; `bd-179b`).

Claim tags used below: **[V]** verified by a tool result, file hash or re-run in this exercise. **[D]** read directly in a
document at `HEAD`. **[A]** reported by a research subagent, not verified. **[S]** read through a web-fetch summarizer.
**[J]** my judgment. Untagged numbers come from the run tables and are [V].

---

## 1. Summary in one screen

1. **Canon geo did not locate a data center better than a plain ladder** (geocoder, then Foursquare, then footprints). On the
   one site where it could be scored, Google Pryor OK, its best answer was the whole 10-building candidate universe, and
   the building nearest the official facility point ranked 1,019th of 1,023 when taken alone. [V] (run 02)
2. **The binding constraint was evidence reach, not the solver.** Only 34 of 69 Epoch sites got a real anchor; of 18 Franklin
   parcels the county itself classes "data center", none of the large campuses had any independent anchor. [V] (runs 01, 03)
3. **What canon does give is real:** byte-identical reproducibility across a version upgrade, exact enumeration, typed
   conflict and ambiguity instead of a false answer, and hashed provenance. All 13 saved solves (Pryor and the 12 Bexar cases) re-solve byte-identically from the committed inputs, and site G's stored and re-run solves share one hash. [V]
4. **The levers that moved outcomes were not composition:** landed parcels with owner names, an independent anchor (state
   air-permit coordinates), and a stated size. [V] (runs 04, 05b, 06)
5. **Every one of these findings is consistent with what the plan already says** (reach is an upstream obligation; soft
   evidence cannot subtract; assemblages are expected). The risk is not that the plan is wrong about mechanisms. It is that
   the *product* built on those mechanisms may not be a locator for this asset class. See §5 and §6.

## 2. What we did

**Sites.** (a) Epoch AI's Frontier Data Centers Hub, snapshot 2026-09-28: 93 sites, 77 US, 69 with an address string (large AI
campuses, not a sample of data centers in general). (b) Our news pipeline (GDELT to Jev to `GDELT_DC_SITES_OFFICIAL`, 129
sites on 2026-09-28, about four days of production data). (c) Official records: Franklin County OH Auditor parcels, TxGIO
county parcel files, two state air-permit records.

**Layers** (queried read-only in `EDGAR_DB.SOURCE`, honoring each table's query contract): Foursquare OS Places (release
2026-08-11, 24.5M rows), Microsoft GlobalML footprints (2026-07-24, 141.8M), FEMA USA Structures (2023-10-03, 135.3M), Overture
buildings (2026-07, ODbL). Parcels: Franklin (release 2026-09-01) and TxGIO 2025 Taylor, Ector and Bexar (file Last-Modified
2026-05-15).

**Arms.** A: plain Census geocoder with a false-exact guard. B: A plus Foursquare address or name lookup. C: B plus large
footprints near the anchor, ranked. D: canon composition over buildings (`materialize-evidence`, `compile-evidence`, `solve`).
E: canon composition over parcels with a set-sum size band (authored composition request, `canon geo solve`).
Tooling: canon 0.13.0.

**Rules kept.** Nothing placed from memory; every anchor cites a tool result. Universe and threshold rules fixed before looking
at truth. No canon output quoted as a release number. All Snowflake work read-only.

## 3. Evidence chain

Each run has a write-up under `geo_datacenter_site_test_2026-09-28/runs/`. Paths below are relative to that folder.

### Run 00: bridge, Google New Albany OH (canon site G)
- **Question:** does canon reproduce a stored result after an upgrade, and do independent sources agree on the site?
- **Method:** `canon geo solve` on the stored site G evidence compilation, canon 0.13.0 versus stored 0.12.1.
- **Result [V]:** byte-identical (sha256 `31cdea2e3521a011...` both; canonical-JSON hash `bc1849c742c9cbdd`, evidence blake3
  `bc6d587393599f28`). Status ambiguous, 11 building candidates, 2,047 feasible compositions, 0.05 s. Epoch's address for the
  site (1101 Beech Rd SW) is the same address as the Foursquare "Google Data Center" that anchored the old run. The Census
  geocoder returns `no_match` on it. Rank 1 is all 11 buildings.
- **Shows:** determinism; that a new-build address can be findable only through a POI feed. **Does not show:** which buildings
  are the site.
- **Files:** `runs/00_bridge_site_g/`.

### Run 01: arms A to C on 69 Epoch sites
- **Question:** how far does a plain ladder get, and how reliable are anchors?
- **Method:** `scripts/run_arms.py` (own code, reuses the canon-geo-site-workup MCP client). False-exact guard: a resolver
  "exact" is demoted unless house number and street name match the input.
- **Result [V]:** Census exact 31, no match 29, false exact 1, not attempted 8 (no city/state). Foursquare address hits 12 (3
  rescued sites the geocoder missed; 9 corroborated), name-only 4. **Anchored 34 of 69 (49%).** 31 sites got ranked large-building
  candidates. Geocoder versus Foursquare anchors, where both exist (n=9), differ by 87, 111, 233, 312, 552, 677, 681, 1,017 and
  1,017 m (median 552 m). Large footprints (>= 100k sq ft within 1.5 km of the anchor) appear in all three layers at 22 of 34
  anchored sites; the other patterns are 4 (Overture and FEMA), 3 (Overture only), 1 each (Microsoft and Overture; Microsoft
  and FEMA), 3 (none).
- **Guard audit:** the first run demoted 3 "exacts"; only "Co Rd 42" to "42 COUNTY CT" was wrong. The other two were my guard's
  mistakes ("Hwy 54" versus "State Rte 54"; a matched number inside the range "14436-14998"). Fixed, unit-checked, all 69
  re-run.
- **Shows:** the ladder covers about half; anchors are hundreds of metres apart, so they are hints. **Does not show:** accuracy
  (only two truth points exist, below).
- **Files:** `runs/01_epoch_arms_abc/` (`results_epoch69_v2.jsonl` keeps a receipt for every call), `scripts/run_arms.py`.

### Truth labels (used from run 01 on)
- **Google Pryor OK, official point [V]:** Oklahoma DEQ permit memo (application 2021-0235-C (M-1), "Myall, LLC Pryor Data
  Storage Facility", Facility ID 6417): "Latitude 36.24250°N, Longitude 95.33020°W", "4581 Webb Street". I extracted the PDF
  text and read the lines. The memo does not say the operator is Google; that link comes from Epoch and Foursquare.
- **Meta Los Lunas NM, official point [V]:** NMED public notice, permit 7026-M5, "Greater Kudu, LLC at 4250 Messenger Loop NW,
  Los Lunas, NM" at "34.828611, -106.781389 Datum: NAD83". Does not say the operator is Meta.
- **Franklin OH, 18 parcels the Auditor classes "data center" [V]:** owners, acres, building area from the landed Auditor table
  (receipt `sha256:8f0e0cc639cb050cc9b0919a36d7d5ab2819803a35db665b6225a168fa85d9ee`).
- **Reported but unverified [A]:** New Carlisle IN (IDEM notice address), the Abilene Comptroller abatement accounts (two of ten
  later confirmed in the Taylor county file [V]), Ohio city-page addresses, and the absence of any official Colossus record.
- **Files:** `notes/truth_labels_v0.md`, `notes/truth_candidates_franklin_auditor.md`.

### Run 02: arm D, canon composition on Google Pryor
- **Question:** given anchors and layers, does composition narrow to the site?
- **Method:** universe = Overture buildings >= 10,000 m2 within 1,300 m of the Foursquare "Google" POI (10 buildings). Three soft,
  uncalibrated, per-member sources, as the builder requires: nearer the Google POI than a named industrial neighbor (5 of 10),
  repeated hall module within +/-5% (3), Microsoft cross-layer footprint within 30 m (9). Rules fixed before the truth point
  was looked at; the official point was never evidence.
- **Result [V]:** status ambiguous, 1,023 feasible compositions, `hard_forced` empty. **Rank 1 is unique, cost 0, and is the whole
  universe.** Ranks of truth-like compositions: partition set 136, three repeated halls 629, building nearest the official point
  (149 m away) plus next nearest 1,009, that building alone **1,019**, the largest building alone 1,020. The "nearest large
  building to the anchor" rule picks the building nearest the official point.
- **Mechanism [D]:** uncalibrated evidence is soft or diagnostic only, is per-member `prefer_member`, and cannot subtract (`README.md`
  @HEAD:576-577; the earlier canon runs on sites B, C, G reached the same shape). Each source charges a cost only when an
  asserted member is absent, never for extras, so the cheapest composition is the union of everything asserted.
- **Limit:** N=1; sources hand-built by me. **Files:** `runs/02_pryor_arm_d/`, `scripts/run_arm_d_pryor.py`.

### Run 03: independent anchors for the 18 Franklin "data center" parcels
- **Method:** Foursquare POI within 600 m matching one fixed brand and category list applied identically to every parcel.
- **Result [V]:** **8 of 18** have any match; the matches are small colo and enterprise sites (and one false positive, "Oracle
  Elevator Company"). **None of the large campuses does**, including three Amazon/VADATA parcels (55 to 143 acres, 695k to 880k sq ft; a fourth VADATA parcel nearby is classed "other industrial", not "data center", so it is not in the 18)
  and a 480-acre, 1.17M sq ft parcel.
- **Shows:** the ladder cannot start at exactly the sites that matter. **Files:** `runs/03_franklin_parcel_arm/`.

### Run 04: what a size band would be worth (upper bound)
- **Method:** for each of the 18 parcels, an anchor 550 m away at 8 bearings (144 placements); candidates = valid Franklin parcels
  within 1 km; keep those whose acres fall in a band around the true acreage. A first attempt counted rows from a bare
  `SELECT`, which the server caps at 200 rows; it silently truncated and was **discarded**. The rerun aggregates in SQL.
- **Result [V]:** no band: median 840 candidates. Wide band 0.5x-2.6x (canon's NYC-calibrated D1 band): median **14**, unique in 32
  of 144. Narrow band 0.75x-1.25x: median **6**, at most 3 in 52 of 144. The six parcels of 55 acres or more leave 1 to 4
  candidates. The truth parcel was inside the 1 km set in 144 of 144. That is about 5.9 bits (wide) and 7.1 bits (narrow) of
  pruning at the median [J], the same order as the plan's "few bits per row" premise (P:1275).
- **News coverage of size [V]:** 38 of 129 official news sites (29%) state acres or building sq ft (31 acres, 9 sq ft, 30 give
  power, 11 give power and area). Receipts `sha256:085837ef6ca2a1c20ecd16ecf5b650295572b9db54210c39467802e2efd2ab8e`,
  `sha256:032b6957e6e1e199e1b09bacaf181523a3103b0ef498050c9e760f88f2db73fd`.
- **Limit:** assumes the stated acreage is exactly right; news accuracy was not measured. **Files:** `runs/04_size_band_pruning/`.

### Run 05: TxGIO parcel spike (Taylor, Ector, Bexar counties TX)
- **Question (Zac):** is Texas easy to land under our conventions, and would it look like Franklin?
- **Method:** new spot scraper `cmdrvl-curves/spot_scrapers/txgio_land_parcels/` (commit `9eaecb5`, README `010a1b0`; ruff, ty and 7
  tests pass) fetched one county at a time from the public TNRIS bucket and inspected schema, CRS, keys, null rates and
  stated-versus-geometry acreage. Source manifest: `data/source_snapshots.json`.
- **Result [V]:** one core schema, three per-county traps. All WGS84, ~35 shared fields. But: Ector's `Prop_ID` is not the parcel
  key (3,791 distinct in 75,947 rows, 2,974 rows `0.00000000`; `GEO_ID` is); Ector's `GIS_AREA` is all zeros while `LEGAL_AREA` is
  97.5% filled and correct; Bexar's `LEGAL_AREA` is 13% filled; keys repeat in Taylor (786 IDs over 3,613 polygons); `STAT_LAND_` is
  codes in Taylor and Bexar and code-plus-text in Ector; `DATE_ACQ` differs per county (20250201, 20250801, 20250701). No building
  area and no sale data in Texas, unlike Franklin. Polygon-derived acres are the only universally reliable size. Table:
  `data/txgio_schema_by_county.csv`.
- **Cross-check [V]:** two of ten Taylor account numbers that a research agent reported from the Texas Comptroller Ch. 312 abatement
  record are in the county file (Prop_ID 1089582 and 1061063, owner LANCIUM ABILENE LLC, Spinks Rd). Twelve Lancium parcels total
  345.2 acres: Abilene is an assemblage.
- **Bexar owner and situs [V]:** county records (independent of Epoch and any geocoder) give MICROSOFT CORPORATION at 3545 WISEMAN
  BLVD (33.7 ac, exact match to Epoch's SAT14 address), MICROSOFT at 15434 LAMBDA DR (94.5 ac; same street as Epoch's SAT40) and
  VANTAGE DATA CENTERS TX1 LLC on OMICRON DR (44.7 ac). Run 01 had SAT14 at no better than city level and SAT40 unresolved. Foursquare returned
  the same POI ("Microsoft Data Center, 5150 Rogers Rd") as a name-only match for both SAT40 and SAT14; the county shows that is a different
  Microsoft parcel about 13 km from SAT40. Run 01 did not use name-only matches as anchors, so it never became a placement.
- **Size band alone [V]:** Ector's news "493-acre" site leaves 204 candidate parcels (narrow band); Bexar's "158-acre" leaves 340.
- **Files:** `runs/05_txgio_spike/`.

### Run 06: arm E, canon composition over Bexar parcels with a set-sum band
- **Case [S]:** KSAT 2026-09-22, "the 158-acre Microsoft Texas Research Park". Our extraction kept "158-acre" and dropped the operator and
  park name. In county legal descriptions "Texas Research Park" is a residential subdivision, so there is no independent truth for the
  158 acres; this run reports what canon enumerates, not accuracy. The band is a hypothesis, not calibrated evidence.
- **Method:** three universes, four bands, requests authored directly and solved with `canon geo solve` (12 solves, saved).
- **Result [V]:**

  | Case | Band | Status | Feasible sets |
  |---|---|---|---|
  | A: Microsoft-owned parcels near Lambda/Omicron (2 parcels, 116.9 ac) | +/-5%, 10%, 25% | **conflict** | **0** |
  | A | D1 0.5x-2.6x | ambiguous | 2 |
  | B: all 11 Microsoft parcels >= 4 ac in Bexar | +/-5% / 10% / 25% / D1 | ambiguous | 173 / 338 / 849 / 1,860 (all tie) |
  | C: any owner within 2 km, >= 22 ac, 15 parcels, soft Microsoft preference | +/-5% / 10% / 25% / D1 | ambiguous | 420 / 912 / 2,244 / 17,716 |

  In case C, 17 (at +/-5%) and 28 (at +/-10%) compositions tie at cost 0. Each contains both Microsoft parcels plus one or two neighbor
  parcels 519 m to about 2 km away. Canon's request format has no adjacency.
- **Shows:** a typed conflict for a size claim, and an exact tie structure. **Does not show:** which alternative is the campus.
- **Mistake [V]:** first attempt refused with `E_PARSE`: each hard constraint must be `{"id", "constraint"}`. **Files:** `runs/06_bexar_arm_e/`.

### Reproducibility [V]
`data/solve_manifest.json` lists every saved solve with size and sha256. All 13 solves (Pryor plus the 12 Bexar cases) re-solve byte-identically
from the committed inputs (`canon geo solve --request <file>`); the large outputs are therefore not committed. Site G's stored and
re-run solves have the same hash. Commands are in the evidence README.

## 4. How the findings map onto the plan

| Plan statement (verified at HEAD) | What we observed | Reading [J] |
|---|---|---|
| GEO is a build-time workbench, core canon stays exact replay (P:91) | Every run here was workbench use | Consistent |
| Uncalibrated evidence is soft or diagnostic; hard bands require empirical calibration (README:576-577) | Composition returned the universe (run 02); no calibrated size population exists for news sizes | Consistent, and it bites: for data centers, no hard evidence class exists yet |
| "A singleton is a decision, a doubleton is an honest doubleton" (P:265) | 1,023 models is honest but not usable | Stressed: what is the product when the residual is large? |
| Geocodes are weak bounds; rooftop r=8 m, interpolated r=150 m (P:330-331) | Census exact and Foursquare anchors differ by a median 552 m; TIGER has no match on 29 of 69 | Stressed: address geocodes on new-build sites are weaker than the radii imply |
| Few bits per row, by design; joint residual is what matters (P:1275-1287) | Acreage band worth ~6-7 bits at the median; anchor worth more | Consistent |
| Kill condition: if evidence cannot rank the true lot, "the architecture survives only as an honest-abstention engine, which is not the product" (P:1365-1370); re-ranking dropped as a premise (P:1376) | For data centers the result looks like the abstention-engine regime | Open: the plan set that condition for NYC re-ranking, not for DCs |
| E4 reaches only 9/17 truths (P:1391); G1 restack: truth in universe 7/15 is "the binding constraint" (P:2019) | Reach failed at the anchor stage for large campuses (run 03) | Consistent, and worse on this asset class |
| 70/70 exact residuals but 0 backbone true positives: "positive solver operation, not evidence-driven resolution" (P:3599-3610) | Same shape at Pryor (exact residual, no discrimination) | Consistent |
| E5 non-NYC tier curve not run; E4/E5 open (P:104) | Our results are an unplanned E5-like probe on a new asset class | Informative for the curve |
| Shipped descriptive profile is one parcel or one building (README@HEAD:584); data-center result "may be a parcel assemblage" (ADDR:277); no data-center experiment run | Lancium 12 parcels, Microsoft 13 parcels in Bexar | Consistent; profile does not fit |
| ADDR §9 first data-center channels: county, developer name, acreage, land use/zoning, owner/legal entity, planning/permit id, building count/sq ft, substation proximity; defer PowerMW (ADDR:258-277) | See §5 channel table | Mostly available except developer name and land use; permit coordinates are a channel the list omits |
| Freework canon-geo skills have no live gate, no frozen suite (freework/policies/canon-geo-live-gate.md:1-12) | We had no rubric for site grades and built a small labeled set | Gap: DC truth planes do not exist |

### Channels named in the plan's data-center follow-on (ADDR §9) versus what we found
| Channel | Availability in our test |
|---|---|
| County / municipality | Available; sometimes only "Bexar County" |
| Project / developer name | **Lost in our own extraction** (Bexar row kept "158-acre", dropped Microsoft and the park name) |
| Acreage | News: 29% of sites; county parcels: everywhere via polygon area |
| Land use / zoning | Franklin: text class incl. "data center". Texas: state codes only, 24% null in Taylor |
| Owner / legal entity | County files: yes (3.7% null Taylor), but LLC shells; not proof of operator |
| Planning or permit identifier | **State air-permit notices** gave address and coordinates for the two verified points |
| Building count / square footage | Franklin `BLDGAREA`; footprints from Overture/Microsoft/FEMA |
| Substation / infrastructure proximity | Not tested |
| PowerMW | Present in Epoch and news; not used as a constraint |

## 5. Hypotheses about what may be wrong with the plan

Ranked by how much the evidence leans on them. Each is a hypothesis to attack, with the strongest counter-argument.

**H1. Reach dominates, and canon cannot create it.** The plan treats acquisition as external and reach as an upstream proof
obligation. For data centers reach is the main determinant of success, so plan value depends on a data-acquisition capability the
plan does not own.
*For:* runs 01, 03; P:1391, P:2019. *Against:* the plan already says this. It may be an acquisition-product problem, not a canon one.
*Cheapest test:* a permit-coordinate harvest for the Epoch sites and re-measure anchored share.

**H2. With data-center evidence, soft-only composition degenerates to "the union of what is asserted".** No calibrated exclusion exists
for this asset class, so composition cannot narrow.
*For:* run 02 (unique cost-0 rank 1 is the universe; truth-like sets rank last); the earlier three canon runs. *Against:* N=1 for scoring; my
sources were hand-built; NYC has calibrated bands (D1: 0.5x-2.6x, 13 of 16 in band).
*Test:* build a DC-specific calibration population and check whether a size or ownership channel can be admitted as hard.

**H3. Grain mismatch: sites are parcel assemblages.** The shipped profile is single-member; the physical answer is a set.
*For:* Lancium 12 parcels/345 ac; Microsoft 13 parcels in Bexar; run 06. *Against:* the composition core supports sets and set-sum
constraints; the profile can be extended.

**H4. Parcel-plane heterogeneity strains the "generic core" and cost story.** Each county chose a different parcel key and a different
reliable acreage field.
*For:* Ector's `Prop_ID` and `GIS_AREA`; Franklin's 3,500-line landing versus Texas needing per-county selection. *Against:* adapters can
encapsulate; canon core needs ids and geometry.
*Test:* attempt the E5 tier curve on Texas counties using geometry-derived ids and acres.

**H5. NYC may overstate transfer.** The proving ground has uniform keys (PAD/PLUTO) and deed-grain truth.
*For:* 49% anchored; TIGER no-match on new builds; DC truth is unconventional. *Against:* E5 is exactly the planned test of this.

**H6. The product may be claim auditing rather than location.** Typed conflict and exact alternatives worked (run 06); location did not.
*For:* run 06 case A; run 00 determinism. *Against:* commercial value of auditing is unproven; the plan's own language calls an
abstention engine "not the product" (P:1365-1370). This is a strategy question, not a technical one.

**H7. Doctrine is enforced on the evidence path, not on authored requests.** `canon geo solve` accepted an authored composition request
with an uncalibrated hard `integer_sum_band` and no ρ contract (all 12 Bexar solves succeeded).
*For:* run 06. *Against:* composition_request v0 legitimately carries hard constraints; the MCP tool labels itself experimental and
"not release claims". *Question:* should `solve` tag or refuse hard constraints that lack a calibrated basis, or is the workbench meant to allow them?

**H8. Set-sum constraints may break the decomposition and cost argument (untested).** One sum over all parcels couples every member
into one component, so the forest-decomposition evidence (P:100-102) is about a different constraint shape.
*For:* our universes were capped at 16 parcels by the 2^16 assignment budget; case C at the D1 band produced 17,716 models (a 7.0 MB
result). *Against:* the solver has typed budget fallback and exact counting without materialization; not exercised here.
*Test:* 30, 60, 200 parcels with a set-sum band.

**H9. Truth instrument gap.** The truth that worked for data centers (state air-permit facility points; a county land-use class
in one county; owner-plus-situs records) is not among the plan's truth planes (deed, ACRIS).
*Against:* `canon geo evaluate` accepts a `human_adjudication` plane; the labeled set could be built.

**H10. Our own pipeline loses the evidence canon needs.** Operator and place names were dropped on a size mention (run 06); this is
not a canon flaw but it decides whether canon gets a usable universe at all.

### What was right in the plan
Determinism held across an upgrade. Typed conflict and honest ambiguity behaved as designed. The plan anticipated reach, the
soft-cannot-subtract limit, assemblages, and the need for parcels (ADDR §9; P:2019). None of the failures were surprises to the plan;
the surprise is how much of the outcome sat upstream of canon.

## 6. Questions for the brainstorm
1. If reach is the binding constraint for this asset class, who owns acquisition, and is the plan complete without it?
2. What is the sellable product when the honest residual is large: claim auditing, candidate lists with provenance, or something else?
3. Can any evidence class for data centers be calibrated to hard status? On what named population, with what falsification rule?
4. Should the profile grow a set-valued, size-banded parcel-assemblage mode, and what stops it exploding combinatorially?
5. Should `solve` distinguish calibrated from authored hard constraints in its output?
6. How much of E5's tier curve can be run on Texas-like heterogeneous parcel planes before landing them?
7. Is "abstention engine, not the product" (P:1365-1370) the right bar for a workbench, or only for a commercial claim?
8. What would make this whole reading wrong? See §8.

## 7. Mistakes and corrections made during the exercise
Kept because a brainstorm should know how far to trust the numbers.
- First guard demoted 3 "false exacts"; 2 were wrong (fixed, run 01).
- A first size-band run counted rows from a bare `SELECT` (server cap 200) and produced invalid numbers; discarded and re-run (run 04).
- I said FEMA (2023) "missed" Pryor; around the official point it has three buildings over 100k sq ft. My read came from one small H3 cell (run 01).
- I said Texas was "cheap" and "looks like Franklin". It is neither (run 05).
- I first thought TNRIS had only a 2.8 GB statewide zip; per-county zips exist (I had printed only base names).
- The earlier memory note "15/15 exact residuals" means the solver produced an exact residual for all 15 cases, not that it found the true
  site; truth was inside the universe in 7/15 (P:2019). Corrected.
- Addressed Epoch US sites are 69, not 67 (recount).
- I repeatedly said "four Amazon/VADATA parcels" among the 18 county-labeled parcels. Three are in the set of 18; the fourth (275-000010) is classed
  "other industrial". The run-03 write-up and its notes carry an explicit correction line.
- First canon request for run 06 failed `E_PARSE` (wrapper); I also tried an `rm` glob that the shell guard blocked, correctly.

## 8. What would make this reading wrong
- If a permit-coordinate harvest lifts anchored share sharply and composition then narrows without calibration, H1 and H2 weaken.
- If a DC calibration population admits a size or ownership channel as hard and pruning holds, H2 weakens.
- If canon's set-sum solve scales to hundreds of parcels without fallback, H8 dissolves.
- If Foursquare, Overture and Microsoft gaps close as builds age, the reach problem is partly temporal (the layers here are 2026-07/08; FEMA is 2023).
- Scoring is N=2 verified points plus small county-record checks. Nothing here is an accuracy result.

## 9. Limits
Two verified point labels; 18 county-classed parcels; three Texas counties; one vintage; Epoch sites are large AI campuses. The KSAT quote
came through a fetch summarizer [S]. Labels from research agents [A] are not scored. Ownership in a county file is not proof of an operator.
Composition sources and weights were chosen by me. Owner-keyword searches use a list I wrote.

## 10. Reproduce
Local paths in the scripts refer to the working directory used (`outcomes/datacenter-canon-geo-test/`, not versioned); this folder is the
durable copy. Order: Epoch snapshot (`spot_scrapers.epoch_ai`), arms A-C (`scripts/run_arms.py`), Pryor arm D (`scripts/run_arm_d_pryor.py`),
Franklin anchors and pruning (`find_anchors.py`, `prune_test.py`), TxGIO (`spot_scrapers.txgio_land_parcels`), Bexar arm E
(`run_arm_e_bexar.py`). Canon steps: `canon geo materialize-evidence`, `compile-evidence`, `solve --request`. Details and the hash manifest:
[`geo_datacenter_site_test_2026-09-28/README.md`](geo_datacenter_site_test_2026-09-28/README.md).
