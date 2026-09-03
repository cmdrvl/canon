# D1 residual re-stack with warehouse evidence (2026-09-03)

Inputs pulled through the cmdrvl-data MCP (edgar_db): PROPERTY_MART.LOAN_ISSUANCE_PROPERTY (geocoded
property points, per-county property counts), PROPERTY_MART.PROPERTY_PERIOD_FACT (asserted SIZE /
SIZE_MEASURE), DBT_STAGING_GEO.STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES 26v1 (ST_CONTAINS containment,
LOTAREA/BLDGAREA per lot). Run:

    canon geo stack-evidence --population d1_population_evidence_stack.json --overlay overlay_request.json > stack.json
    canon geo evaluate --population stack.json --artifact-dir <dir>

Two channels: (1) rho.size.asserted_sqft_band, IntegerSumBand over MapPLUTO BLDGAREA, band 0.5x..2.6x of
asserted ABS-EE sqft, EmpiricalCalibration with admissible_hard_band (calibration_size_band.json: 13/16
truth subjects in band); (2) rho.address.geocode.parcel_containment, PreferMember soft preference per
geocode point-in-parcel hit (calibration_pip.json: 50/70 subjects hit a truth lot). Proof class: observed
warehouse snapshot, not a frozen gate input. See compare.py for the per-case table.

## Second pass: tax-roll owner exclusion (same day)

Third channel, rho.owner.taxroll_borrower_match: PLUTO 26v1 OWNERNAME against ACRIS party_type 1 names for the
subject document, encoded as IntegerSumBand(owner_mismatch, min 0, max 0) so any lot whose tax owner matches no
borrower is excluded (overlay_request_owner.json, calibration_owner.json: 194/224 truth lots match, 43/58 subjects
fully). Stacked on the PAD-only base (evaluation_pad_plus_owner.json) and on the size+geocode stack
(evaluation_all_layers.json). condo_bridge_measurement.json maps truth unit lots to the single PLUTO billing lot
on the block; it recovers 3 of 18 condo subjects, the rest need the Digital Tax Map (cmdrvl-curves bd-2q5m).

## Third pass: sources found in the warehouse after re-checking the catalog

- NYC Building Footprints (SOURCE.NYC_BUILDING_FOOTPRINTS_HOT): footprints.json, calibration_footprint.json. Active BIN
  count over truth lots >= filed property count - 1 on 60/60; admitted as a floor (weak: 7 cases).
- PAD BBL (SOURCE.NYC_DCP_PAD_BBL_HOT): pad_bbl.json.gz, condo_bridge_pad.json. Unit lot -> billing lot via
  LOW/HIGH ranges; 10/18 condo subjects fully reached at billing grain.
- DOF assessment roll FY2026 final (DBT_WRANGLING_NYC_OPENDATA ... PROPERTY_VALUATION ... __STRUCTURED):
  assessment_roll_fy2026p3_lots.json.gz, calibration_roll_owner.json. Holds 619/623 truth lots at unit-lot grain
  with current owner names. population_request_roll_universe.json.gz widens every case universe to the roll lots
  on its blocks; overlay_request_roll_universe.json.gz re-applies PAD, roll-owner exclusion (hard) and geocode
  preferences; evaluation_roll_universe_owner.json is the result (truth fully in universe 47/70, both dossier
  cases hold the truth inside a 16- and 15-set residual). evaluation_soft_owner_footprint.json shows owner as a
  soft preference leaves the residual unchanged.

## Fourth pass: exact owner as hard, affiliates as preference, roll square-footage band

overlay_request_roll_exact_owner_gsf_band.json.gz on population_request_roll_universe.json.gz. Exact
normalized-name equality between the roll OWNER and an ACRIS borrower is the hard exclusion (calibration
201/619 truth lots, 27/70 subjects fully exact); stop-word token matches become prefer_member; sum of roll
GROSS_SQFT over the chosen lots must fall in 0.7x..1.6x of the filed ABS-EE square feet (17/25 truth subjects
in band). evaluation_roll_exact_owner_gsf_band.json: 16 resolved of which 6 exactly equal the deed truth
(444 86th Street among them), 44 ambiguous, 4 conflict, 15 truth exclusions. compare6.py prints all seven stacks.
