# Ground Truth Test: 305 East 72nd Street Tile

Scope note: this report uses only the pasted summary and literal rows. I did
not inspect `docs/PLAN_CANON_GEO.md` or any repository file.

## 1. Actual Model Instantiation

The pasted tile has:

- Parcel rows: 100 MapPLUTO rows.
- Building footprint rows: 93 NYC building footprint rows.
- Raw source features: 193 rows. That is close to the claimed "~200 features
  per tile" by total row count: 193 vs 200, delta -7 (-3.5%).
- Latent building slots from MapPLUTO `NUMBLDGS`: 99 slots.

Slot arithmetic:

- 89 parcels have `NUMBLDGS:1.0`.
- 5 parcels have `NUMBLDGS:2.0`.
- 6 parcels have `NUMBLDGS:0.0`.
- Total slots = `89*1 + 5*2 + 6*0 = 99`.

The five two-slot parcels are:

- `1014477501.0`, `305 EAST 72 STREET`, `NUMBLDGS:2.0`.
- `1014480003.0`, `1408 2 AVENUE`, `NUMBLDGS:2.0`.
- `1014260018.0`, `249 EAST 71 STREET`, `NUMBLDGS:2.0`.
- `1014260035.0`, `232 EAST 72 STREET`, `NUMBLDGS:2.0`.
- `1014280028.0`, `1417 2 AVENUE`, `NUMBLDGS:2.0`.

The six zero-slot parcels are:

- `1014460003.0`, `1386 2 AVENUE`.
- `1014460002.0`, `1384 2 AVENUE`.
- `1014460001.0`, `1382 2 AVENUE`.
- `1014260121.0`, `259 EAST 71 STREET`.
- `1014260120.0`, `257 EAST 71 STREET`.
- `1014250128.0`, `242 EAST 71 STREET`.

The full solver variable count, if parcels, slots, and footprints are all
variables, is:

- `100 parcel variables + 99 slot variables + 93 footprint variables = 292`
  variables.

That is not the same as the "~200 features" estimate. The source-row count is
near 200, but the model variable count is 292.

Comparison against plan estimates:

- Footprints: actual 93 vs estimated ~180, delta -87. The actual footprint
  count is 51.7% of the estimate.
- Slots: actual 99 vs estimated ~80, delta +19. The actual slot count is
  123.75% of the estimate.
- Total source rows: actual 193 vs estimated ~200, delta -7.
- Total model variables: actual 292, not estimated by the summary.

Before any filtering, each footprint-to-slot variable has a domain of 99 slots.
That is `93 * 99 = 9,207` possible footprint-slot edges.

Using the strongest literal identifier available in the footprint source,
`MAPPLUTO_BBL`, after normalizing parcel `BBL` values by removing the warehouse
`.0` suffix, the domains collapse as follows:

- 83 footprint variables have domain size 1.
- 10 footprint variables have domain size 2, because they map to one of the
  five two-slot parcels.
- Total footprint-slot edges after this identifier join: `83*1 + 10*2 = 103`.
- Mean domain size after identifier join: `103 / 93 = 1.108`.

That is much smaller than the claimed `d_typ ~8`: delta -6.892. But this does
not validate the geometric-filter estimate. It means the municipal footprint
source already carries a strong parcel join.

The six positive-slot parcels with no returned footprint row are:

- `1014260027.0`, `252 EAST 72 STREET`, `NUMBLDGS:1.0`,
  `DISTANCE_METERS:76.32353424419641`.
- `1014470041.0`, `318 EAST 73 STREET`, `NUMBLDGS:1.0`,
  `BLDGAREA:0`, `YEARBUILT:0`, `DISTANCE_METERS:89.48039953416578`.
- `1014280129.0`, `248 EAST 74 STREET`, `NUMBLDGS:1.0`,
  `DISTANCE_METERS:146.1952346791301`.
- `1014480042.0`, `326 EAST 74 STREET`, `NUMBLDGS:1.0`,
  `DISTANCE_METERS:146.47425891677366`.
- `1014280030.0`, `246 EAST 74 STREET`, `NUMBLDGS:1.0`,
  `DISTANCE_METERS:148.79751927236086`.
- `1014470033.0`, `334 EAST 73 STREET`, `NUMBLDGS:1.0`,
  `DISTANCE_METERS:149.94223186643706`.

The last four are near the 150m boundary. A tile built by centroid radius is not
closed under "all footprints belonging to parcels in the tile"; boundary
expansion is required before treating missing footprints as evidence.

## 2. Component Size Measurement

The requested geometric component measurement cannot be honestly instantiated
from the pasted rows. The architecture says geometry is integer millimetres in
a per-tile local frame, but the literal rows include only:

- WGS84 centroid floats.
- Derived `DISTANCE_METERS` floats.
- Area and length floats.
- No parcel polygons.
- No footprint polygons.
- No slot coordinates.
- No projection, local-frame origin, or integer geometry encoding.

Therefore the actual `diffn`/`geost` geometric graph cannot be measured from
this extract. Reporting exact 25m geometric components from these rows would
invent geometry that is not present.

What can be measured is the identifier-constrained component graph using
`footprint.MAPPLUTO_BBL -> parcel.BBL` and parcel-derived slots. Including
parcel, slot, and footprint variables, the component distribution is:

- 6 components of size 1: zero-slot parcel only.
- 6 components of size 2: positive-slot parcel with no returned footprint.
- 83 components of size 3: parcel + one slot + one footprint.
- 5 components of size 5: parcel + two slots + two footprints.
- Total components: 100.
- Total variables accounted for: `6*1 + 6*2 + 83*3 + 5*5 = 292`.
- Maximum measured identifier component: 5 variables.
- Mean component size: `292 / 100 = 2.92` variables.

If parcel variables are excluded and only slot/footprint assignment is counted,
the distribution is:

- 6 components of size 1: unobserved positive slots.
- 83 components of size 2: one slot + one footprint.
- 5 components of size 4: two slots + two footprints.
- Total components: 94.
- Total slot/footprint variables: `99 + 93 = 192`.
- Mean component size: `192 / 94 = 2.04`.

This meets the exact-compilation budget only because the source supplies
`MAPPLUTO_BBL`. It does not prove the plan's claim that geometric filtering
creates components of 6-20 variables. In this pasted extract, the geometry
needed to measure that claim is absent.

A centroid-only 25m substitute would be unsound. One concrete failure: the
asserted property's geocode is 31.575516303358757m from the centroid of the
correctly addressed parcel `1014477501.0` (`305 EAST 72 STREET`). A 25m
geocode-to-parcel-centroid filter would remove the apparent true parcel. The
nearest footprint for that parcel is 18.56468461153372m from the geocode, while
the second footprint on the same parcel is 49.0347809647215m away. A rooftop
point is not a parcel extent and not a multi-building property extent.

## 3. Sound `rho` For Each Field

### CMBS property extract

- `PROPERTY_ADDRESS`: relax to a set of address mentions and address ranges
  asserted for the property. It does not mean every mention is a primary parcel
  address. The string contains separators `A/K/A`, ranges `301-305`,
  `1392-1396`, `1398-1402`, `300-302`, street spellings, and ordinals.
- `longitude`, `latitude`: relax to a geocode uncertainty region, not a point.
  The source/provider/version and published error model are not present, so a
  sound radius for `ACCURACY_TYPE=rooftop` cannot be written from this extract.
- `ACCURACY_TYPE`: relax to an error-model selector only if the geocoder data
  dictionary is present. Here it is not. `rooftop` is not exact ownership,
  parcel, footprint, or building identity.
- `parsed NUMBER`: parser output `305`; relax to one parsed address token for
  one mention. It is not exhaustive for the A/K/A ranges.
- `parsed STREET`: parser output `E 72nd St`; relax to one parsed street token.
  It does not equal MapPLUTO's `EAST 72 STREET` without a versioned address
  grammar.
- `COUNTY_FIPS`: relax to county membership `36061`. The parcel rows do not
  carry county FIPS, so this requires an external borough/county mapping from
  BBL borough digit `1` to Manhattan/New York County.

### MapPLUTO parcel fields

- `BBL`: relax to a parcel identifier in the MapPLUTO BBL namespace. The pasted
  values are strings with a decimal suffix, e.g. `1014477501.0`, while
  footprint BBLs are 10-digit strings. A versioned normalization is required.
- `ADDRESS`: relax to "one address associated with this tax lot", not "the only
  address for this lot". Example: `305 EAST 72 STREET` can support the 305
  alias, but absence of `301 EAST 72 STREET`, `1392 2 AVENUE`, or
  `300 EAST 73 STREET` is not disproof.
- `BLDGCLASS`: relax to an uninterpreted DCP building-class code unless the
  DCP code dictionary is versioned. Values include `RM`, `D9`, `U7`, `V1`,
  `Z9`, etc.
- `LANDUSE`: relax to an uninterpreted or dictionary-backed DCP land-use code.
  `null` means no constraint. Example: `1014470041.0` has `LANDUSE:null`.
- `LOTAREA`: relax to lot-area interval/equality in the source unit only if the
  unit and rounding basis are documented. It cannot be compared to
  `SHAPE_AREA` without unit/basis conversion.
- `BLDGAREA`: relax to aggregate building floor area for the parcel if the
  PLUTO dictionary is present. It is not footprint area. `0` may mean no
  building, nonstandard class, or missing/sentinel depending on code.
- `NUMBLDGS`: relax to a nonnegative integer slot-cardinality constraint if the
  value is integral. Here values are encoded as floats: `0.0`, `1.0`, `2.0`.
- `NUMFLOORS`: semantics are not clear enough from the extract. Values include
  `20.5`, so it is not an integer floor count. It needs the PLUTO dictionary
  before it can constrain height or slot count.
- `YEARBUILT`: relax to a weak temporal attribute if `>0`; `0` is a sentinel
  and gives no construction-year constraint. For multi-building parcels, the
  target building is ambiguous without the dictionary.
- `OWNERNAME`: equal owner strings may permit an assemblage hypothesis; unequal
  strings constrain nothing. The field is dirty/truncated, e.g.
  `1390 SECOND AVENUE REALTY,`, `UNAVAILABLE OWNER`,
  `MEMORIAL HOSPTL ETAL`.
- `CENTROID_LON`, `CENTROID_LAT`: relax to a centroid location, not parcel
  geometry. They cannot support non-overlap, containment, or parcel extent.
- `DISTANCE_METERS`: derived from this query's geocode and centroid. It is
  provenance/debug data, not a source attribute. It should be recomputed from
  versioned geometry if used at all.

### NYC building footprint fields

- `RELEASE_DT`: dataset release/vintage date. It is not construction date.
- `SOURCE_ROW_NUMBER`: source-row provenance. It should not constrain physical
  identity except as a release-scoped row handle.
- `OBJECTID`: footprint feature identifier in the NYC footprint namespace for
  this release. It is not a BIN, BBL, or parcel id.
- `BIN`: NYC building identification number. This can enter congruence closure
  in the BIN namespace. No parcel address table is present here to connect BINs
  to the CMBS address.
- `BBL`: footprint-source BBL/base lot. It is not safely joinable to
  MapPLUTO parcel `BBL` for condos. Example: footprint `BBL:"1014470001"`
  maps to `MAPPLUTO_BBL:"1014477501"`.
- `BASE_BBL`: base lot BBL. The difference between `BBL`, `BASE_BBL`, and
  `MAPPLUTO_BBL` needs the footprint data dictionary. It is not interchangeable
  with MapPLUTO parcel id.
- `MAPPLUTO_BBL`: strongest supplied parcel join to MapPLUTO. Relax to
  "this footprint is associated with the MapPLUTO parcel whose normalized BBL
  is this value".
- `CONSTRUCTION_YEAR`: relax to construction-year temporal evidence for the
  footprint, subject to status. For `LAST_STATUS_TYPE:"Marked for Construction"`
  and `FEATURE_CODE:"5100"`, year `2026` is not evidence of an existing
  constructed building.
- `FEATURE_CODE`: code semantics are undocumented in the extract. Values include
  `2100`, `5100`, `5110`. Use only as an uninterpreted enum until the NYC
  dictionary is supplied.
- `GEOM_SOURCE`: provenance and possible accuracy-class selector. Values here
  are `Photogrammetric` and `Other (Manual)`. It is not geometry by itself and
  needs an error model before it can define a spatial band.
- `GROUND_ELEVATION`: numeric elevation. Unit and vertical datum are absent, so
  no sound physical constraint can be written from the extract alone.
- `HEIGHT_ROOF`: numeric roof height. Unit, datum, and sentinel behavior are
  absent. `0.0` appears on the row marked for construction, so zero is not a
  normal constructed-building height.
- `LAST_EDITED_DATE`: record-maintenance timestamp. It is not a building
  construction or demolition interval.
- `LAST_STATUS_TYPE`: status enum. It can constrain whether a footprint is
  eligible for a constructed-building slot. Values here include `Constructed`
  and `Marked for Construction`.
- `IS_ACTIVE_FOOTPRINT`: active-row flag for the release. It is not equivalent
  to constructed: one active row has `LAST_STATUS_TYPE:"Marked for Construction"`.
- `SHAPE_AREA`: geometry-derived area. The unit and basis are absent from the
  extract. It cannot be safely compared to `LOTAREA` or `BLDGAREA`.
- `SHAPE_LENGTH`: geometry-derived perimeter/length. The unit and basis are
  absent.
- `CENTROID_LON`, `CENTROID_LAT`: relax to footprint centroid location only.
  They are not the footprint polygon.
- `H3_R7`, `H3_R8`: relax to coarse spatial index-cell membership for the
  centroid only, if H3 versioning is pinned. They are not geometry constraints.
- `DISTANCE_METERS`: derived from this query's geocode and footprint centroid.
  It is not an independent source attribute.

## 4. Format And Join Defects

Concrete defects in this tile:

- Parcel BBL formatting does not join literally. MapPLUTO uses
  `BBL:"1014477501.0"` while footprints use `MAPPLUTO_BBL:"1014477501"`.
  Exact string comparison fails without a declared normalization.
- Choosing the wrong footprint BBL field breaks condo joins. The first
  footprint has `BBL:"1014470001"`, `BASE_BBL:"1014470001"`,
  `MAPPLUTO_BBL:"1014477501"`. The matching parcel row is
  `BBL:"1014477501.0"`, not `1014470001.0`.
- This condo/base-lot issue repeats:
  - `BBL:"1014470001"` -> `MAPPLUTO_BBL:"1014477501"` for BINs `1076314`
    and `1085187`.
  - `BBL:"1014460149"` -> `MAPPLUTO_BBL:"1014467502"` for BIN `1085184`.
  - `BBL:"1014460041"` -> `MAPPLUTO_BBL:"1014467503"` for BIN `1044873`.
  - `BBL:"1014460035"` -> `MAPPLUTO_BBL:"1014467501"` for BIN `1072637`.
  - `BBL:"1014450001"` -> `MAPPLUTO_BBL:"1014457501"` for BIN `1044853`.
- Address strings do not compare literally. CMBS has
  `305 East 72nd Street`; MapPLUTO has `305 EAST 72 STREET`.
- The parsed CMBS street is `E 72nd St`; MapPLUTO uses `EAST 72 STREET`.
  This requires a versioned address grammar for `E`/`EAST`, ordinal stripping,
  and `St`/`STREET`.
- The CMBS address has `A/K/A` clauses. Treating the whole string as one
  address will fail.
- The CMBS address has ranges: `301-305 East 72nd Street`,
  `1392-1396 2nd Avenue`, `1398-1402 2nd Avenue`,
  `300-302 East 73rd Street`. MapPLUTO rows expose one primary `ADDRESS` per
  parcel, not all alternate addresses.
- The only direct MapPLUTO primary-address match is `305 EAST 72 STREET` on
  `BBL:"1014477501.0"`.
- The following CMBS A/K/A values are not present as MapPLUTO primary
  addresses in the pasted 150m tile: `301 EAST 72 STREET`,
  `303 EAST 72 STREET`, `1392 2 AVENUE`, `1394 2 AVENUE`,
  `1396 2 AVENUE`, `1398 2 AVENUE`, `1400 2 AVENUE`,
  `1402 2 AVENUE`, `300 EAST 73 STREET`, `302 EAST 73 STREET`.
- Nearby rows can look tempting but are not exact matches:
  `1390 2 AVENUE`, `1390 1/2 2 AVENUE`, `1391 2 AVENUE`,
  `1393 2 AVENUE`, `1403 2 AVENUE`, `1404 2 AVENUE`, and
  `304 EAST 73 STREET`.
- Fractional address `1390 1/2 2 AVENUE` will break a naive integer house
  number parser.
- Ordinals and numeric avenues differ: CMBS has `2nd Avenue`; MapPLUTO has
  `2 AVENUE`.
- County cannot join directly. CMBS has `COUNTY_FIPS 36061`; parcel rows do
  not carry `COUNTY_FIPS`.
- Numeric fields are encoded inconsistently for identity work. Parcel
  `NUMBLDGS`, `NUMFLOORS`, and parcel `BBL` are float-like (`2.0`,
  `20.5`, `1014477501.0`), while footprint identifiers are strings.
- `NUMFLOORS` is not an integer: `20.5` appears on
  `BBL:"1014470009.0"`, `315 EAST 72 STREET`.
- Nullability matters: `LANDUSE:null` appears on `1014470041.0`,
  `318 EAST 73 STREET`.
- Sentinel zeros matter: `YEARBUILT:0`, `BLDGAREA:0`, and `NUMFLOORS:0.0`
  appear on several rows and cannot be treated as real year/height/area facts.
- Temporal fields disagree because they are different concepts. For
  `1014460051.0`, MapPLUTO has `YEARBUILT:2012`; footprint BIN `1044871`
  has `CONSTRUCTION_YEAR:2016`. For `1014270023.0`, MapPLUTO has
  `YEARBUILT:2012`; footprint BIN `1090098` has `CONSTRUCTION_YEAR:2017`.
- Active footprint is not constructed footprint. BIN `1091813` has
  `IS_ACTIVE_FOOTPRINT:true`, `LAST_STATUS_TYPE:"Marked for Construction"`,
  `FEATURE_CODE:"5100"`, `HEIGHT_ROOF:0.0`.
- Units silently mis-compare. Parcel `LOTAREA`/`BLDGAREA` and footprint
  `SHAPE_AREA`/`SHAPE_LENGTH` do not declare units. For the apparent target
  parcel `1014477501.0`, `BLDGAREA:194949` cannot be compared to footprint
  areas `1530.19921875` and `1322.7109375` without knowing whether footprint
  area is square feet, square metres, or computed in another CRS.
- `OWNERNAME` is not a clean identifier. Examples include
  `UNAVAILABLE OWNER`, `1390 SECOND AVENUE REALTY,`,
  `MEMORIAL HOSPTL ETAL`, and truncated-looking names such as
  `SECOND AVE. 1355 REA`.
- `DISTANCE_METERS` is a warehouse-derived float. It may differ across engines,
  CRS choices, and query implementations; it should not be replayed as an exact
  proof value.
- H3 fields are centroid indexes. A footprint and its actual polygon can cross
  cell boundaries; H3 equality is not containment.

## 5. What The Model Needs But This Data Does Not Provide

- Parcel polygons: needed for parcel extent, containment, and real geometric
  filtering. Source: MapPLUTO geometry field, not included here. Absence forces
  abstention for `diffn`/`geost`; centroids only widen or mislead.
- Footprint polygons: needed for non-overlap, footprint-slot geometry, area
  from geometry, and integer millimetre compilation. Source: NYC Building
  Footprints geometry field, not included here. Absence forces abstention for
  exact geometry.
- Slot coordinates or slot generation rules: needed to define the 25m
  footprint-to-slot relation. Source: derived from parcel geometry plus
  `NUMBLDGS`, or from footprint geometry. Absence prevents real component
  measurement.
- Address alias/range table: needed to prove that `301-305 EAST 72 STREET`,
  `1392-1396 2 AVENUE`, `1398-1402 2 AVENUE`, and
  `300-302 EAST 73 STREET` belong to the same lot. Source: NYC PAD,
  Geosupport, RPAD, or another versioned address-to-BBL/BIN directory. Absence
  forces abstention on alias coverage.
- Geocoder data dictionary and error model: needed to assign a sound radius to
  `ACCURACY_TYPE=rooftop`. Source: the geocoding provider/version used for the
  CMBS row. Absence widens the geocode band; a point constraint is unsound.
- CMBS as-of date: needed to compare against current 2026 municipal sources.
  Source: filing date, loan tape cutoff, securitization collateral date, or
  source extract date. Absence widens temporal constraints and can force
  abstention.
- CMBS BBL/BIN/property identifier: would directly collapse the identity
  residual. Source: loan tape fields, annex fields, or servicer collateral
  detail. Absence prevents proof-grade resolution.
- CMBS building count, parcel count, gross/net rentable area, and year built:
  needed for `gcc`, knapsack/subset-sum, and temporal constraints. Source:
  collateral tape or prospectus property detail. Absence widens bands.
- MapPLUTO data dictionary: needed for `BLDGCLASS`, `LANDUSE`, `NUMFLOORS`,
  `YEARBUILT`, `BLDGAREA`, units, sentinel values, and condo/base-lot behavior.
  Absence prevents sound rho for those fields beyond uninterpreted values.
- NYC footprint data dictionary: needed for `FEATURE_CODE`, `GEOM_SOURCE`,
  `HEIGHT_ROOF`, `GROUND_ELEVATION`, status, units, and datum. Absence prevents
  sound physical constraints.
- Independent source levels: FEMA USA Structures, Microsoft GlobalML, and
  Overture have no rows here. The tile has two NYC municipal levels, not five.
  Absence removes independent confirmation and turns many "proofs" into
  correlated municipal self-consistency checks.
- Owner/legal assemblage evidence: needed if owner equality is used to permit
  assemblages. Source: ACRIS, DOF owner history, condo declaration, or tax-lot
  history. Absence widens assemblage possibilities.
- Projection/local frame version: needed for integer millimetre geometry.
  Source: explicit CRS, tile origin, rounding rule, and geometry encoding.
  Absence prevents deterministic geometry compilation.

Constraint impact:

- `alldifferent`: partially instantiable for row ids, not enough for physical
  exclusivity across condo/base BBL namespaces.
- `gcc`: partially instantiable from `NUMBLDGS`, but property-level count is
  absent.
- Knapsack/subset-sum: not instantiable for this property; no CMBS area/size
  and source units/bases differ.
- `diffn`/`geost`: not instantiable from the pasted rows; polygons are absent.
- `regular` address grammar: grammar is needed immediately, but the address
  alias directory is absent.
- Allen interval algebra: weak only; CMBS as-of date is absent and municipal
  rows are current/hot.
- Congruence closure: partially instantiable for BBL/BIN namespaces, but only
  if `.0`, base BBL, tax lot BBL, condo BBL, and `MAPPLUTO_BBL` are explicitly
  modeled.

## 6. Can This Case Actually Be Resolved?

Walk:

1. The geocode tile contains 100 parcel rows and 93 footprint rows within 150m.
   Geocode alone does not resolve the property.
2. The property address contains one directly matching mention after address
   normalization: `305 East 72nd Street` matches MapPLUTO
   `305 EAST 72 STREET`.
3. That primary-address match points to `BBL:"1014477501.0"`.
4. The footprint source links two footprints to that MapPLUTO BBL:
   - BIN `1076314`, `BBL:"1014470001"`, `MAPPLUTO_BBL:"1014477501"`,
     `SHAPE_AREA:1530.19921875`, `HEIGHT_ROOF:162.0`.
   - BIN `1085187`, `BBL:"1014470001"`, `MAPPLUTO_BBL:"1014477501"`,
     `SHAPE_AREA:1322.7109375`, `HEIGHT_ROOF:66.0`.
5. MapPLUTO says the parcel has `NUMBLDGS:2.0`, so the two-footprint/two-slot
   relation is internally consistent.
6. The other A/K/A address ranges do not appear as MapPLUTO primary addresses,
   but that is not a contradiction because parcel `ADDRESS` is only one address
   for a lot.

Residual:

- If the target is "best supported parcel in the pasted data", the residual is
  a strong single candidate: `1014477501.0`.
- If the target is proof-grade canonical property identity, the solver should
  abstain from claiming a proof singleton. The alternate addresses are not
  checkable with the pasted data, the geocode error model is missing, and the
  sources are only two correlated NYC municipal sources.
- If the target is building-slot assignment for `1014477501.0`, the residual is
  two anonymous slots and two footprints. There are `2! = 2` bijections between
  slots and footprints, but the footprint set is fixed:
  `{BIN 1076314, BIN 1085187}`.
- If the target is full assemblage extent across all A/K/A addresses, the
  residual is not singleton. The data cannot prove whether the ranges are
  alternate addresses for the same tax lot or imply additional lots.

This is not hopeless. It is a small, obvious candidate with insufficient proof
surface.

The single highest-value additional fact is an authoritative BBL for the CMBS
property, e.g. `1014477501`. That collapses the parcel residual immediately.
If BBL is unavailable, the next best single dataset is a versioned NYC PAD or
Geosupport address-to-BBL/BIN table showing that the A/K/A ranges
`301-305 EAST 72 STREET`, `1392-1396 2 AVENUE`,
`1398-1402 2 AVENUE`, and `300-302 EAST 73 STREET` resolve to
`1014477501`.

A building count of 2 would not collapse the residual by itself. This tile has
five two-building parcels: `1014477501.0`, `1014480003.0`,
`1014260018.0`, `1014260035.0`, and `1014280028.0`.

## 7. Where The Methodology Fails On Contact

- The exact geometry story cannot run on this extract. The rows provide floats
  and centroids, not integer millimetre geometries. The core geometric
  constraints are not merely unimplemented; they are uninstantiable from the
  literal data.
- The component-size claim remains unmeasured. With the supplied
  `MAPPLUTO_BBL` join, components are tiny; with geometry alone, the required
  geometry is absent. Neither result validates "components of 6-20 variables
  after geometric filtering".
- A fixed 25m point filter is dangerous. The apparent correct parcel centroid
  is 31.575516303358757m from the rooftop geocode, and the second same-parcel
  footprint is 49.0347809647215m away. The geocode is a locator, not a parcel
  or property extent.
- The bounded universe has edge effects. Parcels inside the 150m centroid tile
  can have footprints outside the 150m footprint-centroid query. The positive
  no-footprint parcels near 146-150m show this immediately.
- "Five source levels" is false for this tile. FEMA, Microsoft, and Overture
  provide no rows. The available evidence is two NYC municipal sources with
  correlated identifiers and likely shared lineage.
- Address modeling is the real problem, not geometry. The property's decisive
  evidence is the A/K/A address string, and the necessary address alias/range
  table is absent.
- `parcel ADDRESS` as "one of this lot's addresses" is the right relaxation,
  but it makes absence of aliases non-evidential. Without PAD/Geosupport, exact
  replay cannot prove or disprove most of the asserted address string.
- Condo/base-lot namespaces are a first-class constraint, not a formatting
  detail. `1014470001` and `1014477501` both appear and mean different things.
  Direct BBL equality would silently fail on the target parcel.
- `NUMBLDGS` is useful but not clean enough to be the whole slot model.
  `FEATURE_CODE:"5110"` auxiliary-looking footprints, active construction rows,
  boundary-missing footprints, and `BLDGAREA:0` parcels all require source
  semantics.
- Temporal alignment is missing. The footprint release is `2026-08-09`; nearby
  parcels include `YEARBUILT:2025` and a footprint with
  `CONSTRUCTION_YEAR:2026`, `LAST_STATUS_TYPE:"Marked for Construction"`.
  A CMBS property row without an as-of date cannot be safely compared to this
  hot/current tile.
- Empty model set is not automatically proof of source violation here. Without
  the geocoder error model, address alias directory, data dictionaries, and
  geometries, an empty set would more likely prove that the model overconstrained
  the data.

Bottom line: the methodology's relaxation principle is right, especially for
addresses and owner names. The current architectural claim about exact
geometric compilation is not supported by this tile. On this real case, the
resolution signal comes from address normalization plus `MAPPLUTO_BBL`, while
the geometry-heavy solver cannot be instantiated from the available rows and
would be easy to overconstrain.
