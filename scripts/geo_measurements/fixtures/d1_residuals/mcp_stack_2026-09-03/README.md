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
