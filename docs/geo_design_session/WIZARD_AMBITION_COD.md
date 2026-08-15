# WIZARD_AMBITION_COD

## Property Identifier Stamping Authority

The product is not a geocoder and not a parcel table. The product is a
versioned authority that emits statements of the form:

```text
subject: one precisely named entity level
namespace: external identifier system
identifier: value
valid_time: when the binding was true in the world, if knowable
source_time: source vintage or filing period
evidence: source documents, rows, geometries, hashes, and fields
method: deterministic resolver version and inputs
verdict: confirmed | conflict | abstain
```

The central mistake to avoid: treating parcel, address, building, unit,
delivery point, service location, facility, business, and loan collateral as
the same thing. They are not the same thing. The authority is defensible only
if every stamp says what level it keys and refuses when the evidence does not
support a unique binding at that level.

## Ranked Company Bets

Ordered by leverage, not ease.

| Rank | Acquisition / Stamp | Highest-value bridges | Entity level keyed | Public / encumbered | Maintenance burden | Strategic moat |
|---:|---|---|---|---|---|---|
| 1 | Nationwide parcel, assessor, tax, recorder/title spine | Parcel/APN <-> situs address <-> owner <-> geometry <-> legal description <-> instrument/MERS/loan where present | Legal/tax parcel, tax account, recorded instrument, lien | County-public but fragmented; compiled parcel/title products licensable | High if self-built; medium-high if licensed; monthly/quarterly refresh | Very high |
| 2 | Authoritative address and delivery spine: NAD, state/local NG911 address points, USPS AIS/DPV/ZIP+4/carrier/eLOT | Address point <-> unit/subaddress <-> USPS delivery point <-> parcel/building by geometry | Addressable object, delivery point, unit, route | NAD public; USPS licensed/restricted; local NG911 varies | Medium-high; USPS monthly, NAD releases, local deltas | Very high |
| 3 | Structure geometry spine: FEMA USA Structures UUID, Overture buildings/address GERS, Microsoft/Meta/OSM footprints, UBID-derived stamps | Building footprint <-> address <-> parcel <-> POI <-> hazard/exposure | Structure/footprint, sometimes building record | Mostly public/open; OSM ODbL; Overture mixed open licenses | Medium; large ETL and release tracking | Very high |
| 4 | FCC Broadband Serviceable Location Fabric | BSL Location ID <-> address <-> unit <-> building type <-> provider availability | Broadband-serviceable location, often structure/unit | License-limited CostQuest/FCC fabric; public map has Location IDs but not full detail | Biannual plus challenge cycles | Very high |
| 5 | Local permit, certificate of occupancy, inspection, and building-ID systems | Permit/job/CO <-> address <-> parcel <-> owner <-> building/structure | Transactional record, work scope, legal occupancy, local structure ID | Public by jurisdiction; portal/API access fragmented | Very high; thousands of jurisdictions | Very high |
| 6 | EPA Facility Registry Service and environmental program IDs | FRS ID <-> RCRA/NPDES/TRI/RMP/SDWIS/SEMS/etc. <-> address/geocode/name/NAICS | Facility, site, regulated program interest | Public CSV/API for most non-sensitive layers | Low-medium; monthly/public API | Medium-high |
| 7 | Energy, emissions, and benchmarking identity | Portfolio Manager Property/Meter/Parent IDs <-> local standard IDs such as BBL/BIN/DC SSL/CA ref <-> address/building/parcel | Benchmarked property, campus, building, meter grouping | Public disclosures by city/state; PM account/meter detail private | Annual for public disclosures; higher with utility data | High in campuses and multifamily |
| 8 | Capital-stack identity: SEC CMBS ABS-EE/prospectus data, recorder mortgage docs, CUSIP/ISIN, MERS MIN | Loan/security/deal <-> property name/address <-> borrower/owner <-> parcel/title instrument | Loan collateral, mortgage lien, security/deal, borrower | SEC public; recorder public but fragmented; CUSIP/MERS/title data encumbered | Medium-high; filings monthly/quarterly, recorder deltas | High |
| 9 | Commercial place and tenant graph | Overture Place GERS/bridge IDs, Placekey, OSM, Foursquare/Google/Yelp/licensed IDs, business licenses <-> address/unit/building | Establishment, business-at-place, POI | Overture/Placekey mostly open/API; Google/Yelp/Foursquare restricted/licensed; business licenses local | High churn; daily/monthly | Medium-high in dense commercial property |
| 10 | Public and special-use facility registries | HUD LIHTC/MF/FHA, NCES, CMS CCN/NPI, FAA LID, FCC ASR, EIA plant/ORISPL, PHMSA, GSA/FRPP, SAM UEI <-> address/geocode/operator | Program facility, campus, regulated asset, operator | Mostly public; some ownership/license caveats | Low-medium by registry | Medium; commodity unless geometry is complex |

## 1. Parcel, Assessor, Tax, Recorder, Title Spine

**Why it is first:** parcel identity is the legal/tax anchor. It connects the
operator's portfolio to land ownership, tax rolls, legal descriptions, deeds,
mortgages, easements, recorded instruments, zoning overlays, census geography,
and nearly every public local workflow.

**Identifier universe unlocked:** APN/PIN/PID/AIN/SBL/BBL, tax account number,
tax bill number, cadastral parcel geometry, legal lot/block/subdivision, plat
book/page, deed/instrument/document number, book/page, grantor/grantee IDs where
available, mortgage instrument IDs, MERS MIN where recorded or licensed,
assessor property-class codes, land-use codes, tax district IDs, owner mailing
address, situs address, county FIPS, census GEOIDs, flood panel overlays, local
zoning district IDs.

**Entity-level danger:** APN keys a tax parcel, not a building and not always a
legal lot. Condos invert the model. Campuses can have many buildings on one
parcel. A building can cross parcels. Air rights, mobile homes, co-ops,
leaseholds, tax-exempt properties, railroad corridors, and subdividing parcels
can all break naive assumptions. Recorder instruments key legal acts, not
current real-world occupancy.

**Acquisition:** either license a national parcel fabric from Regrid, LightBox,
ATTOM/CoreLogic/DataTree/DataTrace, or build a county/state harvester. Regrid
publishes national parcel schemas with parcel geometry, situs and mailing
addresses, owner fields, legal references, related geographies, and hazard
attributes; public pricing has historically put nationwide bulk parcels in the
tens of thousands per year, while title/recorder depth is materially more
expensive. Self-building means 3,000-plus county workflows, inconsistent
licensing, schema drift, and refresh monitoring.

**Cheap wrong way:** geocode the address, take the parcel containing the point,
and call the APN solved. This silently fails for large parcels, corner lots,
condos, malls, campuses, stacked parcels, parcel splits, rural addressing, and
records where the situs address is stale.

**Expensive exact way:** create a time-vintaged parcel topology and solve each
tile as a constrained graph:

- parcel polygons, parcel centroids, address points, building footprints, street
  frontage, unit ranges, tax roll fields, legal descriptions, and recorder
  references become typed nodes.
- candidate bindings must satisfy containment/intersection, frontage, address
  sequence parity, owner continuity, legal-description consistency, and valid
  source vintage.
- parcel splits and merges become temporal graph operations, not overwrites.
- if multiple parcels remain consistent, return a conflict or abstention.

**Leverage:** this is the single most load-bearing acquisition. It connects the
largest number of currently disconnected namespaces and gives every later
dataset a legal/tax coordinate system.

## 2. Authoritative Address And Delivery Spine

**Why it is second:** every operator starts with an address, and every weak
competitor stops at string normalization. We need address identity split into
addressable object, unit, postal delivery point, and local emergency-service
address point.

**Identifier universe unlocked:** NAD OID/source record IDs, local address point
IDs, NG911/NENA address point IDs where exposed, MSAG/ESN/PSAP references,
OpenAddresses source IDs, Overture Address GERS IDs, USPS ZIP, ZIP+4, delivery
point code/check digit/DPBC, carrier route, eLOT, RDI, LACSLink aliases,
No-Stat/CMRA/secondary-unit DPV indicators, urbanization codes in Puerto Rico,
county/city/state postal tables.

**Entity-level danger:** an address can identify a delivery point, a unit, a
site entrance, a mailbox cluster, a landmark, a building, or merely a range on a
street. USPS delivery existence is not the same as legal occupancy or parcel
ownership. Local 911 address points often represent the authoritative civic
address, not mail delivery.

**Acquisition:** NAD is public-domain and compiled from state, local, and tribal
government address programs. USPS AIS/DPV/ZIP+4 products are licensed, updated
monthly, and the AIS Viewer data is encrypted/non-exportable in that product
surface. Production use should go through CASS/DPV licensed providers plus any
direct USPS products we are allowed to retain. Local NG911 address datasets are
state-by-state: some are open, some require public records requests, and some
are not distributable.

**Cheap wrong way:** CASS-normalize strings and join exact address text to
assessor situs. This collapses unit-level delivery points into parent parcels,
misses LACS/rural-route history, treats vanity addresses as legal addresses,
and converts "valid mail" into "same property."

**Expensive exact way:** tile-level address asterism:

- model street centerlines, address parity, interpolated ranges, local address
  points, building entrances, parcels, USPS ZIP+4 ranges, and carrier-route
  constraints.
- use Patterson-vector or asterism-style signatures over address-point
  constellations to match source datasets whose coordinates differ but whose
  local geometry is the same.
- solve unit stacks separately from parent structures.
- preserve every alias as evidence, including LACSLink and retired addresses.

**Leverage:** this is the bridge from messy operator input to every other
namespace. It also makes FCC Fabric, energy benchmarking, permits, POIs, and
CMBS materially cheaper to stamp.

## 3. Structure Geometry Spine

**Why it is third:** structures are the missing middle layer between legal
parcels and delivery/service/tenant identities. Most national property data
skips this layer or treats building footprints as decoration.

**Identifier universe unlocked:** FEMA USA Structures UUID, Overture Building
GERS ID, Microsoft/Meta building footprint source IDs where available,
OpenStreetMap node/way/relation IDs, local building IDs such as NYC BIN, UBID
or other deterministic building-footprint encodings, roof/structure exposure
IDs, building-count and square-footage assertions.

**Entity-level danger:** a footprint is not always a building. A building can
have multiple footprints, courtyards, podium/tower forms, connected structures,
parking garages, or additions. FEMA's USA Structures is a national inventory and
uses a UUID, but it is not a local legal building registry. Overture GERS is
intended to be stable across releases, but it is still a conflated reference
map, not a county building department record. OSM element IDs can change when
contributors remodel features.

**Acquisition:** FEMA USA Structures is public and includes structure polygons
and UUIDs; Overture provides buildings, addresses, places, bridge files, GERS
IDs, release changelogs, and source links under open licenses; Microsoft/Meta
footprints and OSM add independent geometry. Cost is compute, storage, release
diffing, and license hygiene more than purchase price.

**Cheap wrong way:** intersect building centroids with parcels and dedupe with
an IoU threshold. This fails on rowhouses, dense urban parcels, campus parcels,
footprints crossing parcel lines, attached retail, and changes across releases.

**Expensive exact way:** exact per-tile geometry:

- build polygon arrangements for parcels, structures, roads, water, and hazards.
- derive rotation/translation-tolerant local geometry signatures from building
  centroid constellations and parcel-boundary relations.
- run maximum common subgraph between source footprint graphs across releases.
- solve exact cover: which footprints explain which address points, which parcel
  improvements, which permit records, and which benchmarked floor areas.
- stamp a structure ID only when the same entity is supported across geometry,
  address, and temporal evidence.

**Leverage:** this layer makes "property" computable. It unlocks responsible
unit, meter, broadband, energy, permit, risk, and tenant stamping.

## 4. FCC Broadband Serviceable Location Fabric

**Why it is fourth:** the FCC Fabric is a national service-location namespace
that already sits at the parcel/address/building/unit boundary. It is one of the
few national datasets whose identifier is intentionally about serviceability,
not land or mail.

**Identifier universe unlocked:** FCC/CostQuest Location ID, Broadband
Serviceable Location status, secondary address records, building type,
residential/business/MDU indicators, provider availability records by Location
ID from the National Broadband Map, challenge IDs/evidence where filed.

**Entity-level danger:** a Broadband Serviceable Location is not necessarily a
parcel, building, USPS delivery point, or customer account. MDUs, mixed-use
buildings, group quarters, farms, outbuildings, and campus networks are the
hard cases. The full fabric is license-restricted; public availability downloads
by Location ID do not include the full address/coordinate/building details.

**Acquisition:** access requires a Fabric license through FCC/CostQuest routes.
Some entities get no-cost licenses for BDC/challenge/public-policy purposes, but
commercial reuse and redistribution must be reviewed carefully. Fabric versions
are released on a regular cycle, with challenge and availability updates.

**Cheap wrong way:** normalize the service address and take the nearest Fabric
Location ID or first exact address match. In MDUs this assigns the wrong unit;
in rural areas it assigns the wrong structure; in commercial campuses it assigns
the wrong service location but looks plausible.

**Expensive exact way:** use the address/building/parcel spine to challenge the
Fabric itself:

- parent/secondary address matching with USPS and NAD evidence.
- footprint-level serviceability: main structure vs accessory structures.
- unit-stack constraints from USPS DPV, permits, apartment registries, and local
  address points.
- versioned comparison of Fabric changes against building and parcel changes.
- abstain when a BSL cannot be uniquely connected without license-prohibited or
  customer-private evidence.

**Leverage:** this is strategically high because competitors will approximate
it away. Correct MDU and rural service-location stamping is a durable moat.

## 5. Local Permits, Certificates Of Occupancy, Inspections, And Building IDs

**Why it is fifth:** permits and COs are the temporal truth for buildings:
construction, demolition, additions, legal occupancy, use changes, unit counts,
and inspections. They are also where local building IDs often live.

**Identifier universe unlocked:** permit/job/application numbers, plan-review
IDs, inspection IDs, certificate of occupancy numbers, demolition permit IDs,
local building IDs such as NYC BIN, fire inspection IDs, code-enforcement case
IDs, contractor/license IDs, Accela/EnerGov/CivicPlus record IDs, parcel/address
owner objects in APO systems.

**Entity-level danger:** a permit is a transaction, not a building. A permit can
cover one unit, a tenant improvement, a facade, a roof, a campus project, or a
parcel subdivision. Many portals attach permits to addresses rather than
structures, and some attach to owner/applicant records. Jurisdictions vary
radically.

**Acquisition:** public but painful. The market is fragmented across Accela,
Tyler EnerGov, OpenGov, CivicPlus, Salesforce/public portals, Socrata, ArcGIS,
PDF ledgers, and custom municipal systems. Start with high-value metros and
states with open data. Then build adapters by platform family. This is an
engineering/data-operations program, not a single download.

**Cheap wrong way:** fuzzy-match permit address to parcel and attach all permits
to the parcel. This invents building history for the wrong structure and
pollutes downstream energy, risk, tenant, and capital stamps.

**Expensive exact way:** turn permit systems into event streams:

- parse APO records: address, parcel, owner, structure, establishment.
- attach each permit to the narrowest possible entity level.
- model lifecycle transitions: proposed, issued, inspected, completed, CO,
  demolished, expired, withdrawn.
- cross-check against changes in footprints, assessor year-built/area,
  energy-benchmark floor area, and unit counts.
- keep stale and superseded local IDs alive with valid-time intervals.

**Leverage:** permits are expensive to acquire, but they make the authority
temporal. Without them we know where things are; with them we know when the
thing changed.

## 6. EPA FRS And Environmental Program Identity

**Why it matters:** EPA already solved one cross-program identity problem:
FRS links many environmental program records to a facility/site registry ID. We
should not redo that. We should stamp it onto parcels/buildings/facilities with
evidence and entity-level care.

**Identifier universe unlocked:** FRS Registry ID, Program System ID, TRI
Facility ID, RCRAInfo handler ID, NPDES permit number, RMP ID, SDWIS/PWS IDs,
SEMS/Superfund IDs, ACRES brownfields IDs, ICIS-Air/ICIS-NPDES IDs, E-GGRT/GHG
reporter IDs, NEI/EIS IDs, NAICS/SIC, facility names and addresses.

**Entity-level danger:** FRS keys facility/site/program interest. A refinery,
airport, university, hospital, or manufacturing campus can span many parcels and
buildings. Program IDs can point to discharge points, tanks, outfalls, handlers,
or reporting entities. A point coordinate is often a representative point, not
the regulated boundary.

**Acquisition:** public FRS APIs, prepackaged CSV relational downloads, and
geospatial downloads, with sensitive layers excluded. EPA indicates many
downloads are updated monthly and program systems refresh on varying schedules.
Cost is low; validation and facility-boundary modeling are the real work.

**Cheap wrong way:** nearest parcel or point-in-parcel from FRS coordinates. It
will confidently assign a campus facility to a single tax parcel or the wrong
adjacent parcel.

**Expensive exact way:** facility graph resolution:

- start from FRS program links, not geometry.
- use source program addresses, supplemental location, facility names, NAICS,
  owner/operator, outfall/tank/site metadata, and parcel/building geometry.
- allow many-parcel and many-building facility stamps.
- abstain from parcel-level stamps when the source only supports facility-level.

**Leverage:** medium-high. It connects many environmental namespaces in one
public edge, but the join to physical property still requires careful geometry.

## 7. Energy, Emissions, And Benchmarking Identity

**Why it matters:** energy/emissions IDs are where buildings, campuses, meters,
regulation, and reported performance collide. This gets valuable as regulation
moves from voluntary benchmarking to fines, retrofit mandates, and capital
planning.

**Identifier universe unlocked:** EPA Portfolio Manager Property ID, Parent
Property ID, Meter ID, Custom IDs, Standard IDs, city benchmarking record IDs,
NYC LL84/LL97 BBL/BIN references, DC Real Property SSL/Portfolio Manager IDs,
California benchmarking reference numbers, local building performance standard
IDs, utility aggregated meter/account references where legally accessible.

**Entity-level danger:** Portfolio Manager "property" may be a building,
campus, parcel bundle, or an energy-meter aggregation. A campus can report one
parent and several children. A local disclosure row can carry multiple parcel or
building IDs. Meter IDs are not public property identity, and utility account
data is customer-sensitive.

**Acquisition:** public annual disclosures from NYC, DC, Boston, Chicago,
Seattle, California, Colorado, Montgomery County, St. Paul, and similar
programs; EPA Portfolio Manager public aggregate tools; private Portfolio
Manager/API access only with owner authorization. Maintenance is annual for
public reports, but utility-meter integration requires consent and compliance.

**Cheap wrong way:** join disclosure address to parcel or treat a PM Property ID
as a building. This breaks multi-building campuses, multi-BBL records, parking
structures, owner-submitted errors, and properties whose reported GFA differs
from assessor area.

**Expensive exact way:** reconcile energy reporting as an exact cover problem:

- which buildings/parcels/units explain the reported gross floor area, use type,
  unit count, and Standard IDs?
- which meters correspond to which physical sub-entities?
- where local law requires BBL/BIN entry, use the local IDs as bridge evidence
  but still validate against building and parcel geometry.
- keep Portfolio Manager parent/child relationships explicit.

**Leverage:** high for commercial/multifamily markets, especially where climate
law creates financial consequences. Less universal than parcel/address, but
high strategic value.

## 8. Capital-Stack Identity

**Why it matters:** property finance has its own identifiers: loans, recorded
instruments, MERS MINs, securitization assets, servicer loan IDs, CUSIPs, ISINs,
trust CIKs, and prospectus property names. This is how a place appears in the
capital markets.

**Identifier universe unlocked:** SEC accession numbers, trust/depositor CIKs,
CMBS deal names, ABS-EE asset numbers, asset number types, property names,
property addresses/county/type/NRSA/unit counts/year built/appraised value,
loan numbers, servicer IDs, CUSIP/ISIN for bonds, recorded mortgage/deed of
trust document numbers, assignment IDs, MERS Mortgage Identification Number,
FHA case numbers, Ginnie/Fannie/Freddie pool/security IDs where applicable.

**Entity-level danger:** a CMBS asset number keys a loan asset in a deal, not a
building. The collateral may be "various," cross-collateralized, substituted,
defeased, or reported under stale property names. A CUSIP keys a security, not
the property. MERS MIN keys a mortgage loan registration, not the parcel.

**Acquisition:** SEC ABS-EE and 10-D filings are public and structured; Schedule
AL requires asset-level disclosures for registered CMBS including asset numbers
and property information. Prospectus annexes and free-writing prospectuses add
detail. Recorder/title data is public but fragmented or licensable. CUSIP/ISIN
redistribution is encumbered. MERS data is privately governed, though MINs
appear in many recorded instruments and homeowner lookup surfaces exist.

**Cheap wrong way:** geocode the property address in a prospectus and attach the
loan to the nearest parcel. This fails on portfolios, malls, hotels with vanity
names, recapitalizations, address changes, substitutions, and stale filings.

**Expensive exact way:** evidence-chain resolution:

- extract SEC rows, prospectus tables, recorded mortgage instruments, borrower
  entities, legal descriptions, parcel/APN references, and historical assessor
  snapshots.
- tie loan collateral to the source-time parcel/building set, not current
  parcel only.
- require consistency among property name, address, rentable area, unit count,
  appraised value, borrower, and recorded instrument.
- preserve defeasance, payoff, substitution, split, and foreclosure events.

**Leverage:** high for commercial real estate. It is not universal, but it is
one of the strongest "nobody else has done the geometry" opportunities because
capital-market records are source-rich and physically under-resolved.

## 9. Commercial Place And Tenant Graph

**Why it matters:** a large share of the world refers to property through
business/venue identifiers: Google Place IDs, Overture Place GERS IDs,
Placekeys, OSM amenities, Yelp/Foursquare IDs, brand store numbers, liquor
licenses, food permits, and business licenses. These are not property IDs, but
operators ask for them because tenants and amenities drive revenue.

**Identifier universe unlocked:** Overture Place GERS ID and bridge/source IDs,
Placekey, OpenStreetMap element IDs, Google Place ID, Foursquare FSQ ID, Yelp
business alias/id, Wikidata QID, brand store/location number, NAICS/SIC,
business license IDs, health/food establishment IDs, liquor license IDs, sales
tax location IDs where public, Secretary of State entity IDs for operators.

**Entity-level danger:** these key establishments or named destinations, not
buildings. Businesses move, close, rebrand, sublease, share suites, and operate
seasonally. A POI can represent a campus, kiosk, mall store, floor, entrance, or
abstract brand location.

**Acquisition:** Overture Places is open and carries GERS IDs, source/license
metadata, addresses, categories, point geometry, and confidence. Placekey is a
free/open identifier/API surface with H3-based "where" plus "what" semantics.
Google/Yelp/Foursquare are licensed and redistribution-restricted. Local
business licenses and health inspections are public but fragmented.

**Cheap wrong way:** nearest POI to the parcel centroid or text match tenant
name to the property address. This creates wrong joins in malls, food halls,
office towers, airports, hospitals, mixed-use developments, and downtown blocks.

**Expensive exact way:** tenant/place resolution:

- use unit/suite evidence, phone/web/name/category/brand, business-license and
  health-permit records, local floor/entrance geometry, and POI source IDs.
- represent POI occupancy as a temporal relationship to a unit/building, never
  as the property itself.
- use cycles: a food permit validates a POI; a POI validates a suite address; a
  suite address validates a building/unit stack.

**Leverage:** medium-high. It is strategically valuable in dense commercial
property, but commodity in simple standalone retail where spatial matching works.

## 10. Public And Special-Use Facility Registries

**Why it matters:** many properties have authoritative program IDs because they
are schools, hospitals, subsidized housing, airports, towers, power plants,
bridges, public buildings, public housing, rail crossings, ports, dams, or
regulated infrastructure.

**Identifier universe unlocked:** HUD LIHTC project IDs, HUD Multifamily
property IDs, Section 8 contract IDs, FHA mortgage/project IDs, REAC/iREMS-style
property references, NCES School ID and District ID, IPEDS UnitID, CMS CCN,
NPI, CLIA, FAA Location ID, FCC Antenna Structure Registration number, ULS
license IDs, EIA Plant Code, ORISPL, PHMSA facility/operator IDs, National
Bridge Inventory structure number, DOT rail crossing inventory number, GNIS
feature ID, SAM UEI, CAGE, federal real-property IDs where exposed.

**Entity-level danger:** most of these key a facility, institution, campus,
license, provider, regulated asset, or operator. A hospital CCN is not the
building. A school ID is not the parcel. A tower ASR is not the land lease. HUD
LIHTC property locations may be generalized and not individual buildings.

**Acquisition:** mostly public federal/state downloads with annual or monthly
refresh. HUD LIHTC includes project addresses, units, financing fields, and
geocoding. HUD multifamily assistance data has property and contract tables
linked by property ID. NCES publishes school search and geocode files. CMS
provider data includes CCN and address. FAA/FCC/EIA/PHMSA/NBI are public.

**Cheap wrong way:** attach each facility point to one parcel. It is often good
enough for a small school but wrong for campuses, hospitals, airports, towers on
leased land, power plants, public housing complexes, and multi-building HUD
properties.

**Expensive exact way:** registry-specific entity models:

- each registry gets a subject-level contract: facility, campus, provider,
  regulated asset, program property, or operator.
- geometry resolves only to the supported level.
- parcel/building stamps require independent evidence: local parcel, permit,
  assessor, building footprint, or legal description.

**Leverage:** lower than the core spine because many joins are easy. The value
is as high-precision anchors and validation cycles for important property
classes.

## Identifier Universe By Entity Level

This is the operating taxonomy. Every namespace must land in one of these
levels before we stamp it.

| Category | Identifier systems | Issuer / maintainer | What it identifies | Public status | Stability / danger |
|---|---|---|---|---|---|
| Legal parcel and tax | APN, PIN, PID, AIN, SBL, BBL, tax account, tax bill, cadastral parcel ID | County/city assessor, tax authority | Tax parcel or assessment account | Mostly public; compiled national products licensable | Stable until split/merge/renumber; jurisdiction-specific formatting |
| Legal lot and survey | lot/block/subdivision, plat book/page, metes-and-bounds legal description, PLSS aliquot, condominium unit/air-rights lot | Recorder, surveyor, assessor, planning office | Legal land interest or subdivision lot | Public but unstandardized | Hard to parse; not always equal to tax parcel |
| Recorded instruments | deed, mortgage, assignment, lien, easement, foreclosure, document number, book/page | County recorder/clerk/register | Recorded legal act | Public; bulk access often paid | Instrument ID stable; index quality varies |
| Mortgage registry | MERS MIN, servicer/investor lookup references | MERS/lenders/servicers | Mortgage loan registration | Private with limited public lookup and recorded appearances | Encumbered; loan can deactivate/transfer |
| Structure/building | local building ID/BIN, USA Structures UUID, Overture Building GERS, OSM element, Microsoft/Meta footprint ID, UBID | Local DOB or open map/footprint projects | Structure, footprint, or building record | Mixed public/open | Footprint IDs not legal IDs; local IDs jurisdiction-specific |
| Addressable location | NAD OID/source ID, local address point ID, NG911/NENA address point, OpenAddresses source ID, Overture Address GERS | DOT/state/local/tribal address authorities, open aggregators | Addressable object, sometimes sub-unit/landmark | NAD public-domain; local varies | Not equivalent to delivery point or parcel |
| Postal/delivery | ZIP, ZIP+4, DPBC, carrier route, eLOT, RDI, LACSLink, DPV flags, No-Stat/CMRA indicators | USPS | Mail delivery and routing objects | Licensed/restricted | Monthly churn; cannot treat as ownership/legal identity |
| Unit/subaddress | apartment/suite/unit, secondary address records, local unit registries, FCC Fabric secondary address | USPS/local addressing/FCC/vendor | Unit or secondary delivery/service point | Mixed; USPS/Fabric restricted | Often missing, inconsistent, or private |
| Utility service | electric premise ID, service point ID, meter number, ESIID, gas/water/sewer meter/account, solar interconnection ID | Utility/ISO/local utility | Service location, account, or meter | Mostly private; Texas ESIIDs partly searchable | Regulated/PII; meters/accounts change |
| Broadband | FCC/CostQuest Fabric Location ID, BSL, provider availability by Location ID, FRN/provider IDs | FCC/CostQuest/providers | Broadband serviceable location and provider filing | Location IDs public; Fabric details licensed | Versioned; MDU and unit ambiguity |
| Permits and occupancy | permit/job/application, inspection, CO, demolition permit, code case, fire inspection, health/food permit, liquor license | Local jurisdictions/platforms | Work transaction, legal occupancy, establishment, or inspection | Public but fragmented | Level varies by record type |
| Environmental facility | FRS Registry ID, TRI, RCRAInfo, NPDES, RMP, SDWIS/PWS, SEMS, ACRES, ICIS, E-GGRT/GHG | EPA/state programs | Facility/site/program interest | Mostly public; sensitive layers excluded | Campus-level; coordinates often representative |
| Hazard geography | NFHL flood zone, FIRM panel, community number, LOMC, WUI, seismic, landslide, hurricane wind, census GEOID, HUC | FEMA/USGS/Census/NOAA/state agencies | Geographic overlay, not property | Public | Must be source-vintaged; map updates change results |
| Energy/emissions | Portfolio Manager Property/Parent/Meter IDs, city benchmarking IDs, BBL/BIN standard IDs, building-performance-standard IDs | EPA and state/local regulators | Benchmarked property, campus, meter group | Public disclosures partial; PM/meter private | Owner-entered errors; campus aggregation |
| Insurance/risk | NFIP policy, ISO PPC/BCEGS, Verisk/ISO property/location IDs, CoreLogic/Marshall & Swift, First Street, HazardHub | Carriers, ISO/Verisk, risk vendors, FEMA | Policy, protection class, property-risk object | Mostly proprietary; NFIP property policy not broadly public | Redistribution constraints; model versions matter |
| Commercial place | Overture Place GERS, Placekey, OSM, Google Place ID, Foursquare FSQ, Yelp ID, brand store number, Wikidata QID | Map/POI platforms, brands, communities | Establishment/POI/business-at-place | Mixed open/API/proprietary | High churn; not property identity |
| Business/legal entity | Secretary of State entity ID, EIN, CIK, LEI, UEI, DUNS, NPI, CCN, CAGE | State/Federal/private registries | Owner/operator/provider, not property | Mixed public/proprietary | Only bridges through ownership/operator evidence |
| Capital stack | ABS-EE asset number, servicer loan ID, deal/trust CIK, accession, CUSIP, ISIN, FHA case, Ginnie/Fannie/Freddie pool IDs | SEC, trustees, servicers, CUSIP agencies, GSEs | Loan/security/deal/collateral | SEC public; CUSIP/GSE/servicer data encumbered | Collateral may be multi-property or stale |
| Public facility | HUD LIHTC/MF IDs, NCES School ID, IPEDS UnitID, CMS CCN, FAA LID, FCC ASR, EIA Plant Code, NBI bridge ID, DOT crossing ID | Federal/state programs | Facility, campus, provider, asset | Mostly public | Program-specific entity levels |
| Derived spatial index | Census GEOID, ZCTA, H3, S2, geohash, Plus Code, what3words, UBID | Census/open standards/private standards | Coordinate cell/area/footprint encoding | Mixed open/proprietary | Useful for indexing, not evidence of identity |

## Bridge Graph

Nodes:

```text
P   legal/tax parcel
L   legal lot / recorded land description
A   authoritative address point
U   unit / subaddress
D   USPS delivery point / route
B   building / structure / footprint
R   permit / occupancy / inspection record
T   title / recorder / recorded instrument
M   mortgage / MERS / lien / loan
C   capital-market security / CMBS asset / CUSIP
O   owner / operator / legal entity
F   environmental facility / regulated program interest
E   energy / emissions / meter / benchmarking property
BB  broadband serviceable location
POI commercial place / establishment
PF  public or special-use facility registry
H   hazard / census / regulatory geography
```

Bridge edges worth acquiring:

| Dataset edge | Graph edge | Why it matters | Rank effect |
|---|---|---|---|
| Parcel assessor/tax fabric | P-A-O-H-B? | Situs address, owner, geometry, tax/legal metadata | Connects the core graph |
| Recorder/title index | P/L-T-O-M | Legal acts, ownership transfer, liens, MERS appearances | Connects property to capital and history |
| NAD/state/local NG911 address points | A-U-B?/P? | Civic address authority and point geometry | Connects input addresses to physical entities |
| USPS AIS/DPV/ZIP+4 | A-U-D | Delivery point and route truth | Connects address to delivery namespace |
| Structure footprints / Overture / USA Structures | B-A-P-POI-H | Structure identity and geometry | Creates the missing middle layer |
| FCC Fabric | BB-A-U-B | Serviceability namespace | Connects telecom to unit/building |
| Permit/CO systems | R-P-A-B-O | Building lifecycle and legal occupancy | Adds time and local building IDs |
| EPA FRS | F-A-O-H + program IDs | Environmental program crosswalk | Already bridges many environmental IDs |
| Energy benchmarking disclosures | E-A-B-P + PM IDs | Building/campus energy identity | Bridges regulation, meters, parcels/buildings |
| SEC ABS-EE/prospectuses | C-M-A-O + property facts | Capital stack to physical collateral | Bridges finance to place |
| Business license / POI graph | POI-A-U-B-O | Tenant/establishment identity | Bridges commercial reality to units |
| HUD/NCES/CMS/FAA/FCC/EIA/etc. | PF-A-B?/P?/O | Special facility IDs | High-precision anchors |
| FEMA NFHL/Census/TIGER | H-P/B/A by geometry | Regulatory geography and hazard | Commodity but required |

Currently isolated or only geometry-reachable:

- Utility meter, account, premise, and most service-point IDs. Public bridges are
  sparse; customer authorization or utility partnership is usually required.
- USPS DPBC/DPV details are licensed; we can resolve under license but cannot
  freely redistribute raw postal data.
- Google Place IDs, Verisk/ISO/CoreLogic/CoStar IDs, CUSIP data, and many risk
  vendor identifiers are legally encumbered.
- Internal bank, servicer, insurer, property-manager, and utility IDs are
  private unless the operator supplies them.
- Unit-level truth in MDUs is often not public outside USPS/Fabric/local
  addressing/permit evidence.

Minimum acquisition set to connect the practical graph:

1. Parcel/assessor geometry with situs and owner.
2. NAD plus state/local NG911 address points plus USPS licensed address products.
3. Structure geometry/GERS stack.
4. Local permit/CO IDs in priority jurisdictions.
5. Recorder/title/MERS-capable document spine.
6. FCC Fabric license.
7. EPA FRS.
8. Energy benchmarking disclosures and Portfolio Manager bridges.
9. POI/business-license graph.
10. Public/special facility registries.

If forced to pick one acquisition, pick parcel plus assessor geometry. If forced
to pick the most leverage per row after that, pick address/delivery. If forced
to pick the most defensible mathematical moat, pick structure/address/parcel
resolution plus FCC Fabric/MDU serviceability.

## Accretion Sequence

1. **Define the stamp contract and entity ontology.** No identifier enters the
   registry without a subject level, source vintage, evidence hash, method ID,
   and abstention/conflict state.
2. **Stamp parcels and geography first.** APN/PIN/parcel geometry, census
   GEOIDs, FIRM panels, flood zones, zoning overlays where easy. This creates
   the legal/tax coordinate system.
3. **Stamp address identity.** NAD/local address points and USPS licensed
   delivery evidence. This turns operator strings into evidence-bearing
   addressable objects.
4. **Stamp structures.** Build exact parcel-address-building tiles. Add FEMA
   UUID, Overture GERS, OSM, UBID/derived footprint stamps where supported.
5. **Stamp local building lifecycle.** Permits, COs, inspections, demolition,
   local building IDs. This converts the static map into temporal identity.
6. **Stamp serviceability.** FCC Fabric, units, USPS delivery points, and
   eventually utility service IDs where rights allow.
7. **Stamp regulated facilities and public facilities.** EPA FRS, HUD, NCES,
   CMS, FAA, FCC ASR, EIA, PHMSA. These are lower acquisition cost and validate
   the core graph.
8. **Stamp energy/emissions.** Portfolio Manager and city/state disclosure IDs
   need the structure/campus model first.
9. **Stamp capital stack.** CMBS/SEC/title/MERS stamps need historical
   parcel-building identity and recorder evidence.
10. **Stamp POIs and tenants last.** Tenant identity is high churn; it becomes
   reliable only after unit/building/address identity exists.

Important bootstrapping cycles:

- **Parcel <-> address <-> building:** each layer makes the others cheaper and
  catches silent errors.
- **Permits <-> footprints <-> assessor attributes:** new construction,
  demolition, area changes, and unit counts validate each other.
- **USPS/FCC Fabric <-> local address/unit stacks:** delivery and broadband
  evidence reveal hidden MDUs and secondary addresses.
- **FRS/POI/business license <-> SOS/entity IDs:** operator identity validates
  facility/tenant identity without collapsing it into property identity.
- **CMBS/title <-> parcel history:** capital-market collateral validates
  historical property names and ownership, while parcel/title evidence prevents
  stale loan records from poisoning current property identity.

## What Makes The Authority Defensible

Data breadth is not the moat. The moat is disciplined, replayable, entity-aware
resolution.

What matters most:

1. **Entity-level correctness.** A stamp must say whether it keys a parcel,
   structure, unit, delivery point, service location, facility, establishment,
   owner, permit, loan, or security. This prevents silent false joins.
2. **Temporal identity.** Lots merge, buildings get demolished, delivery points
   retire, tenants churn, mortgages pay off, and map features split. The
   authority tracks valid-time and source-time rather than overwriting.
3. **Evidence per binding.** Every identifier binding points to named source
   rows, filings, map features, geometries, and hashes. A bare crosswalk is not
   enough.
4. **Deterministic replay.** Given the same source vintages and resolver
   version, the stamp is reproducible byte-for-byte.
5. **First-class abstention.** "No responsible stamp" is a product feature. It
   is the difference between an authority and a vendor table.
6. **Conflict handling.** Multiple plausible bindings must be exposed as
   conflicts with evidence, not buried under a confidence score.
7. **Licensing discipline.** Some namespaces can be resolved internally but not
   redistributed. The stamp contract must separate "we can know" from "we can
   publish."

## Where This Breaks

Stamping is irresponsible when the source namespace does not stably identify a
physical entity at the claimed level, or when the license prevents the use case.

Hard breakpoints:

- **Utility meter/account/premise IDs:** high value but often private, regulated,
  customer-specific, and non-redistributable. Stamp only with customer authority
  or utility partnership, and never imply public completeness.
- **USPS DPV/DPBC and detailed delivery data:** authoritative but licensed. We
  can use it to make decisions under license; redistribution must be designed
  around USPS terms.
- **Google/Yelp/Foursquare/Verisk/CoStar/CoreLogic/CUSIP-like identifiers:**
  useful but terms may prohibit caching, derivative publication, or identifier
  redistribution. Treat as licensed overlays, not open registry facts.
- **Parcel IDs in split/merge-heavy jurisdictions:** APNs can change and
  assessor maps are not legal surveys. Stamp by source vintage and keep parcel
  lineage, not "current APN forever."
- **OSM element IDs and ML footprint IDs:** not legal authority and not
  guaranteed permanent. Stamp only as source-specific map-feature identity with
  release/version.
- **POIs and tenant IDs:** establishments move and close. A POI stamp should be
  an occupancy relationship over time, not a property identity.
- **FRS and facility points:** coordinates can be representative and facilities
  can span campuses. Parcel-level environmental stamps need additional evidence.
- **CMBS and financial records:** collateral can be stale, multi-property,
  defeased, substituted, or hidden behind "various." Strong evidence is required
  before attaching to current parcel/building identity.
- **Permits:** transactional records can apply to tenant work, a unit, a
  building, a parcel, or an owner. The source usually does not tell us the level
  cleanly.
- **Private owner/operator IDs:** DUNS, UEI, EIN, CIK, LEI, Secretary of State,
  and NPI/CCN identify people or organizations. They should never be presented
  as property identifiers, only as related-party stamps.

The line: we can stand behind a stamp when the evidence names the same entity at
the claimed level, under a source vintage, with no unresolved equally plausible
competitor. If the evidence only says "nearby," "same string," or "probably the
same parcel," the correct output is abstention or conflict.

## Source Notes

- USDOT National Address Database: https://www.transportation.gov/gis/national-address-database
- NAD public data.gov listing: https://catalog.data.gov/dataset/national-address-database-nad-text-file
- USPS AIS products and licensing surface: https://postalpro.usps.com/address-quality/ais-viewer
- FCC Fabric access and Location ID explanation: https://help.bdc.fcc.gov/hc/en-us/articles/10419121200923-How-Entities-Can-Access-the-Location-Fabric
- FCC Fabric overview: https://help.bdc.fcc.gov/hc/en-us/articles/5375384069659-What-is-the-Location-Fabric
- FEMA USA Structures technical paper: https://www.nature.com/articles/s41597-024-03219-x
- Overture GERS: https://docs.overturemaps.org/gers/
- Overture Addresses: https://docs.overturemaps.org/guides/addresses/
- Overture Places: https://docs.overturemaps.org/guides/places/
- EPA FRS: https://www.epa.gov/frs
- EPA FRS downloads: https://www.epa.gov/frs/frs-data-download-options
- EPA ECHO FRS data dictionary: https://echo.epa.gov/tools/data-downloads/frs-download-summary
- FEMA Flood Map Service Center: https://msc.fema.gov/
- NOAA summary of FEMA NFHL: https://coast.noaa.gov/digitalcoast/data/flood.html
- ENERGY STAR Portfolio Manager glossary: https://portfoliomanager.energystar.gov/pm/glossary
- NYC LL84 BBL/BIN Portfolio Manager guidance: https://www.nyc.gov/site/buildings/codes/ll84-benchmarking-law.page
- California benchmarking FAQ: https://www.energy.ca.gov/programs-and-topics/programs/building-energy-benchmarking-program/building-energy-benchmarking
- DOE Building Performance Database listing: https://catalog.data.gov/dataset/building-performance-database
- SEC ABS issuance and Reg AB context: https://www.sec.gov/data-research/statistics-data-visualizations/asset-backed-securities-abs-issuances
- SEC Form ABS-EE filing guidance: https://www.sec.gov/rules-regulations/staff-guidance/corporation-finance-interpretations/information-form-abs-ee-filings
- SEC CMBS issuance data: https://www.sec.gov/data-research/statistics-data-visualizations/commercial-mortgage-backed-securities-cmbs-issuances
- eCFR Schedule AL asset-level information: https://www.ecfr.gov/current/title-17/chapter-II/part-229/subpart-229.1100/section-229.1125
- HUD LIHTC property data: https://www.huduser.gov/portal/datasets/lihtc/property.html
- HUD Multifamily Assistance and Section 8 database: https://www.hud.gov/hud-partners/multifamily-assist-section8-database
- NCES school locator/geocodes: https://nces.ed.gov/programs/edge/geographic/schoollocations
- Census TIGER/Line shapefiles: https://www.census.gov/geographies/mapping-files/time-series/geo/tiger-line-file.html
- Fannie Mae MERS guide: https://selling-guide.fanniemae.com/sel/b8-7-01/mortgage-electronic-registration-systems-mers-inc
- Accela root object model: https://developer.accela.com/docs/construct-rootObjects.html
