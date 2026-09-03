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
