# bd-3ab6 E1/E2 Failure Taxonomy and Attribute Channel

Measurement date: 2026-08-16.

Mode: Loom SQL only. Numbers below are from `--raw` structured tool results, not
Loom prose. ACRIS is used only to label the Gate V2 true BBL set; it is not
counted as a scored evidence channel in E2.

Standing inputs from `docs/PLAN_CANON_GEO.md`:

- Section 16.3 row 6 is the first unjoined asserted-attribute channel:
  Annex A / `PROPERTY_MART` square feet, units, and year.
- Section 17 says constraints are measured relatively over candidates, never as
  unary accept/refute facts.
- Appendix H Gate V2 provides the labeled geometry set: 233 covered points,
  154 PIP-lot-correct and 79 PIP-lot-incorrect.

## Result Summary

E1 partitions the 79 failures:

| failure class | points | share of 79 |
|---|---:|---:|
| gross geocode error, >500m | 40 | 50.63% |
| condo representation residue | 32 | 40.51% |
| assemblage artifact / neighbor | 2 | 2.53% |
| adjacent-lot near miss | 2 | 2.53% |
| residual truth contamination suspect | 3 | 3.80% |

Sanity gates:

- Failure classes sum: `40 + 32 + 2 + 2 + 3 = 79`.
- The same classifier applied to the 154 correct covered points returns
  `154` as `no_failure_class`.
- Predicate denominators are point-grain Gate V2 covered PIP points.

E2 result:

- `PROPERTY_MART.PROPERTY_ISSUANCE.SIZE` is not a dense square-foot channel
  unless filtered by `SIZE_MEASURE='SQFT'`. The unfiltered `SIZE` field mixes
  `SQFT` and `UNITS`.
- After `SQFT` filtering, positive asserted SF is sparse: 10 correct points and
  17 incorrect points, with 10 and 16 unique asserted SF values respectively.
- Annex `TAPE_SECURITIZATION_SQUARE_FEET` is also sparse: 8 correct points and
  14 incorrect points, with 8 and 13 unique asserted SF values respectively.
- On the incorrect points where PIP lot and true lot are both comparable, the
  asserted-SF channel does not favor truth. `PROPERTY_ISSUANCE_SIZE_SQFT` has
  PIP log-closer on 4/5 and true log-closer on 1/5. Annex tape SF has PIP
  log-closer on 3/4 and true log-closer on 1/4.
- MapPLUTO landed parcel attributes are `BLDGAREA`, `NUMBLDGS`, and
  `YEARBUILT`; no unit count column was present in `NYC_DCP_MAPPLUTO_HOT`.
  Therefore `TAPE_UNITS_BEDS_ROOMS` has no landed parcel-side unit comparator
  in this test.

Design implication: E1 says most wrong PIP points are not ordinary adjacent-lot
mistakes; the failure population is dominated by gross point errors and condo
representation issues. E2 says the first row-6 attribute test is real but sparse
and, on this Gate V2 slice, does not rescue the wrong PIP lot in aggregate.

## E2 Field Discovery

Structured result summary:

| source | relevant fields |
|---|---|
| `PROPERTY_MART.PROPERTY_ISSUANCE` | `PROPERTY_KEY`, `CIK`, `ASSET_KEY`, `LOAN_ASSET_KEY`, `ASSETNUMBER`, `ASSET_NUMBER`, `SIZE`, `SIZE_MEASURE`, `COUNTY_FIPS`, `BUILD_ID` |
| `PROPERTY_MART.PROPERTY_CURRENT` | same `SIZE` / `SIZE_MEASURE` shape, sparser on the labeled set |
| `PROPERTY_MART.PROPERTY_PERIOD_FACT` | same `SIZE` / `SIZE_MEASURE` shape |
| `PROPERTY_MART.PROPERTY_DIM` | `PROPERTY_KEY`, `LATITUDE`, `LONGITUDE`, `COUNTY_FIPS`, `BUILD_ID` |
| `DBT_WRANGLING_EDGAR.WRGL_ANNEX_ISSUANCE_FACT_COMPARISON` | `PROPERTY_KEY`, `LOAN_ASSET_KEY`, `SOURCE_A_UNIT`, `SOURCE_A_UNITS_SEEN`, `TAPE_SECURITIZATION_SQUARE_FEET`, `TAPE_UNITS_BEDS_ROOMS`, `CANONICAL_UNIT`, `BUILD_ID` |
| `DBT_WRANGLING_EDGAR.WRGL_ANNEX_PROPERTY_FACT_CROSS_CHECK` | commentary square-foot fields, but zero positive coverage on the labeled set |
| `DBT_STAGING_EDGAR.STG_EDGAR_CMBS_10D__MORTGAGE_LOANS` | `NRA_WODRA_FROM_PRINCIPAL_AMOUNT` surfaced, not used as the main property-size channel |
| `SOURCE.NYC_DCP_MAPPLUTO_HOT` | `BBL`, `BLDGAREA`, `NUMBLDGS`, `YEARBUILT`, `GEOM_GEOG`; no unit-count column |

Exact discovery SQL:

```sql
SELECT table_schema, table_name, column_name, data_type, ordinal_position
FROM EDGAR_DB.INFORMATION_SCHEMA.COLUMNS
WHERE (table_schema, table_name) IN (
  ('PROPERTY_MART','PROPERTY_ISSUANCE'),
  ('PROPERTY_MART','PROPERTY_CURRENT'),
  ('PROPERTY_MART','PROPERTY_DIM'),
  ('PROPERTY_MART','PROPERTY_PERIOD_FACT'),
  ('PROPERTY_MART','LOAN_ISSUANCE'),
  ('DBT_WRANGLING_EDGAR','WRGL_ANNEX_ISSUANCE_FACT_COMPARISON'),
  ('DBT_WRANGLING_EDGAR','WRGL_ANNEX_ISSUANCE_UNDERWRITING_METRIC_SOURCES'),
  ('DBT_WRANGLING_EDGAR','WRGL_ANNEX_PROPERTY_FACT_CROSS_CHECK'),
  ('DBT_WRANGLING_EDGAR','STG_EDGAR_CMBS_10D__MORTGAGE_LOANS')
)
AND (
  column_name IN ('PROPERTY_KEY','LOAN_KEY','CIK','ASSETNUMBER','ASSET_NUMBER','ASSET_KEY','LOAN_ASSET_KEY','BUILD_ID','PROPERTY_ADDRESS','COUNTY_FIPS','LATITUDE','LONGITUDE')
  OR column_name ILIKE '%SIZE%'
  OR column_name ILIKE '%SQUARE%'
  OR column_name ILIKE '%SQ%FT%'
  OR column_name ILIKE '%SQFT%'
  OR column_name ILIKE '%UNIT%'
  OR column_name ILIKE '%YEAR%'
  OR column_name ILIKE '%BUILD%'
  OR column_name ILIKE '%BLDG%'
  OR column_name ILIKE '%NRA%'
  OR column_name ILIKE '%GBA%'
  OR column_name ILIKE '%GLA%'
)
ORDER BY table_schema, table_name, ordinal_position;
```

MapPLUTO field SQL:

```sql
SELECT column_name, data_type, ordinal_position
FROM EDGAR_DB.INFORMATION_SCHEMA.COLUMNS
WHERE table_schema='SOURCE'
  AND table_name='NYC_DCP_MAPPLUTO_HOT'
  AND (
    column_name IN ('BBL','BOROUGH','BLOCK','LOT','BLDGAREA','NUMBLDGS','UNITSRES','UNITSTOTAL','YEARBUILT','GEOM_GEOG')
    OR column_name ILIKE '%AREA%'
    OR column_name ILIKE '%BLDG%'
    OR column_name ILIKE '%UNIT%'
    OR column_name ILIKE '%YEAR%'
  )
ORDER BY ordinal_position;
```

## E1 Failure Taxonomy

Classifier priority:

1. `gross_geocode_error_gt500m`: covered PIP point, no lot hit, and nearest true
   parcel geometry is more than 500m away from the PIP point or PIP parcel.
2. `condo_representation_residue`: remaining failure, truth set has condo-unit
   lot signature and true MapPLUTO geometry is missing on at least one edge.
3. `assemblage_artifact_neighbor`: remaining failure, truth has multiple BBLs
   and the PIP parcel is within 25m of a true parcel or block-compatible.
4. `adjacent_lot_near_miss`: remaining failure within 25m or block-compatible.
5. `residual_truth_contamination_suspect`: remaining failure after the above.

Structured result:

| class | points | contamination signal | condo sig | multi-truth | missing true geom | min PIP parcel to true m | avg PIP parcel to true m | max PIP parcel to true m | min point to true m | avg point to true m | max point to true m |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| gross_geocode_error_gt500m | 40 | 29 | 1 | 5 | 2 | 707.51 | 7131.42025 | 23321.42 | 735.62 | 7154.83950 | 23333.53 |
| condo_representation_residue | 32 | 13 | 32 | 9 | 32 | 0.00 | 0.00 | 0.00 | 6.13 | 6.13 | 6.13 |
| assemblage_artifact_neighbor | 2 | 2 | 0 | 2 | 0 | 17.26 | 19.665 | 22.07 | 25.50 | 27.835 | 30.17 |
| adjacent_lot_near_miss | 2 | 1 | 0 | 0 | 0 | 0.00 | 8.25 | 16.50 | 6.96 | 14.285 | 21.61 |
| residual_truth_contamination_suspect | 3 | 3 | 0 | 3 | 0 | 30.29 | 154.5667 | 380.87 | 39.58 | 161.27 | 384.50 |
| no_failure_class, correct sanity sample | 154 | 105 | 7 | 52 | 2 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |

Exact SQL, as returned in the successful structured result after Loom removed
the parser-rejected `* EXCLUDE` projections:

```sql
WITH ls AS (SELECT DISTINCT l.LOAN_KEY AS k, CAST(l.ORIGINATIONDATE AS DATE) AS od, ROUND(l.ORIGINALLOANAMOUNT, 2) AS amt, IFF(ROUND(l.ORIGINALLOANAMOUNT, 2) % 100000 = 0, 1, 0) AS is_round FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT AS p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE AS l ON p.CIK = l.CIK AND p.ASSETNUMBER = l.ASSETNUMBER WHERE p.COUNTY_FIPS IN ('36005', '36047', '36061', '36081', '36085') AND p.HAS_LOAN = TRUE AND NOT l.ORIGINATIONDATE IS NULL AND NOT l.ORIGINALLOANAMOUNT IS NULL),
lb AS (SELECT DISTINCT l.LOAN_KEY AS k, CASE TO_CHAR(p.COUNTY_FIPS) WHEN '36061' THEN 1 WHEN '36005' THEN 2 WHEN '36047' THEN 3 WHEN '36081' THEN 4 WHEN '36085' THEN 5 END AS boro FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT AS p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE AS l ON p.CIK = l.CIK AND p.ASSETNUMBER = l.ASSETNUMBER WHERE p.COUNTY_FIPS IN ('36005', '36047', '36061', '36081', '36085') AND p.HAS_LOAN = TRUE),
ad AS (SELECT DISTINCT DOCUMENT_ID AS doc, CAST(RECORDED_DATETIME AS DATE) AS rd, RECORDED_BOROUGH AS recorded_boro, ROUND(DOCUMENT_AMT, 2) AS amt FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT WHERE RELEASE_DT = '2026-08-10' AND RECORDED_BOROUGH IN (1, 2, 3, 4, 5) AND DOC_TYPE IN ('MTGE', 'M&CON', 'CMTG', 'SMTG', 'MMTG', 'SPRD') AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000 AND CAST(RECORDED_DATETIME AS DATE) BETWEEN (SELECT MIN(od) FROM ls) AND DATEADD(DAY, 45, (SELECT MAX(od) FROM ls))),
raw AS (SELECT ls.k, ls.od, ls.amt, ad.doc, ad.rd, ad.recorded_boro, DATEDIFF(DAY, ls.od, ad.rd) AS offset_days FROM ls JOIN ad ON ls.is_round = 0 AND ad.amt = ls.amt AND ad.rd BETWEEN ls.od AND DATEADD(DAY, 45, ls.od)),
cand AS (SELECT DISTINCT raw.k, raw.od, raw.amt, raw.doc, raw.rd, raw.recorded_boro, raw.offset_days FROM raw JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT AS le ON raw.doc = le.DOCUMENT_ID JOIN lb ON raw.k = lb.k AND le.BOROUGH = lb.boro WHERE le.RELEASE_DT = '2026-08-10' AND le.BOROUGH IN (1, 2, 3, 4, 5) AND NOT le.BLOCK IS NULL AND NOT le.LOT IS NULL),
cc AS (SELECT k, COUNT(DISTINCT doc) AS docs FROM cand GROUP BY k),
ac AS (SELECT cand.k, cand.od, cand.amt, cand.doc, cand.rd, cand.recorded_boro, cand.offset_days FROM cand JOIN cc USING (k) WHERE docs = 1),
ab AS (SELECT DISTINCT ac.k, ac.doc, ac.offset_days, ac.recorded_boro, le.BOROUGH AS legal_boro, TO_CHAR(le.BOROUGH) || LPAD(TO_CHAR(le.BLOCK), 5, '0') || LPAD(TO_CHAR(le.LOT), 4, '0') AS bbl, TO_CHAR(le.BOROUGH) || LPAD(TO_CHAR(le.BLOCK), 5, '0') AS blk, IFF(TRY_TO_NUMBER(le.LOT) BETWEEN 1001 AND 6999, 1, 0) AS condo FROM ac JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT AS le ON ac.doc = le.DOCUMENT_ID WHERE le.RELEASE_DT = '2026-08-10' AND le.BOROUGH IN (1, 2, 3, 4, 5) AND NOT le.BLOCK IS NULL AND NOT le.LOT IS NULL),
ps AS (SELECT DISTINCT p.PROPERTY_KEY AS pk, l.LOAN_KEY AS k FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT AS p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE AS l ON p.CIK = l.CIK AND p.ASSETNUMBER = l.ASSETNUMBER WHERE p.COUNTY_FIPS IN ('36005', '36047', '36061', '36081', '36085') AND p.HAS_LOAN = TRUE),
td AS (SELECT DISTINCT ps.pk, ab.k, ab.doc, ab.offset_days, ab.recorded_boro, ab.legal_boro, ab.bbl, ab.blk, ab.condo, d.LATITUDE AS lat, d.LONGITUDE AS lon FROM ab JOIN ps USING (k) JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM AS d ON ps.pk = d.PROPERTY_KEY WHERE d.COUNTY_FIPS IN ('36005', '36047', '36061', '36081', '36085') AND NOT d.LATITUDE IS NULL AND NOT d.LONGITUDE IS NULL),
r AS (SELECT LATITUDE, LONGITUDE, ACCURACY_TYPE, COUNTY_FIPS FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED WHERE COUNTY_FIPS IN ('36005', '36047', '36061', '36081', '36085')),
pts AS (SELECT LATITUDE AS lat, LONGITUDE AS lon, IFF(COUNT(DISTINCT ACCURACY_TYPE) = 1, MIN(ACCURACY_TYPE), 'mixed') AS acc FROM r WHERE NOT LATITUDE IS NULL AND NOT LONGITUDE IS NULL GROUP BY LATITUDE, LONGITUDE),
tb AS (SELECT DISTINCT pts.lat, pts.lon, pts.acc, td.pk, td.k, td.doc, td.offset_days, td.recorded_boro, td.legal_boro, td.bbl, td.blk, td.condo FROM pts JOIN td ON pts.lat = td.lat AND pts.lon = td.lon),
pe AS (SELECT DISTINCT pts.lat, pts.lon, REGEXP_REPLACE(TO_CHAR(pl.BBL), '\\.0$', '') AS pbbl, SUBSTRING(REGEXP_REPLACE(TO_CHAR(pl.BBL), '\\.0$', ''), 1, 6) AS pblk, pl.GEOM_GEOG AS pgeom FROM pts JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT AS pl ON ST_CONTAINS(pl.GEOM_GEOG, ST_MAKEPOINT(pts.lon, pts.lat))),
truth_geom AS (SELECT tb.lat, tb.lon, tb.bbl, pl.GEOM_GEOG AS tgeom FROM tb LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT AS pl ON REGEXP_REPLACE(TO_CHAR(pl.BBL), '\\.0$', '') = tb.bbl),
edge AS (SELECT tb.lat, tb.lon, tb.acc, pe.pbbl, pe.pblk, tb.bbl, tb.blk, tb.condo, tb.offset_days, tb.recorded_boro, tb.legal_boro, IFF(tg.tgeom IS NULL, 1, 0) AS missing_true_geom, ST_DISTANCE(pe.pgeom, tg.tgeom) AS pred_to_true_m, ST_DISTANCE(ST_MAKEPOINT(tb.lon, tb.lat), tg.tgeom) AS point_to_true_m FROM tb JOIN pe ON tb.lat = pe.lat AND tb.lon = pe.lon LEFT JOIN truth_geom AS tg ON tb.lat = tg.lat AND tb.lon = tg.lon AND tb.bbl = tg.bbl),
point_roll AS (SELECT lat, lon, acc, COUNT(DISTINCT bbl) AS truth_bbls, COUNT(DISTINCT blk) AS truth_blocks, COUNT(DISTINCT pbbl) AS pred_bbls, COUNT(DISTINCT IFF(pbbl = bbl, pbbl, NULL)) AS lot_hits, COUNT(DISTINCT IFF(pblk = blk, pblk, NULL)) AS block_hits, MAX(condo) AS condo_sig, MIN(pred_to_true_m) AS min_pred_to_true_m, MIN(point_to_true_m) AS min_point_to_true_m, SUM(missing_true_geom) AS missing_true_geom_edges, MAX(offset_days) AS max_offset_days, MIN(offset_days) AS min_offset_days, MAX(IFF(recorded_boro <> legal_boro, 1, 0)) AS any_recorded_legal_boro_mismatch FROM edge GROUP BY lat, lon, acc),
classified AS (SELECT lat, lon, acc, truth_bbls, truth_blocks, pred_bbls, lot_hits, block_hits, condo_sig, min_pred_to_true_m, min_point_to_true_m, missing_true_geom_edges, max_offset_days, min_offset_days, any_recorded_legal_boro_mismatch, CASE WHEN pred_bbls > 0 AND lot_hits = 0 AND COALESCE(min_point_to_true_m, min_pred_to_true_m) > 500 THEN 'gross_geocode_error_gt500m' WHEN pred_bbls > 0 AND lot_hits = 0 AND condo_sig = 1 AND missing_true_geom_edges > 0 THEN 'condo_representation_residue' WHEN pred_bbls > 0 AND lot_hits = 0 AND truth_bbls > 1 AND (min_pred_to_true_m <= 25 OR block_hits > 0) THEN 'assemblage_artifact_neighbor' WHEN pred_bbls > 0 AND lot_hits = 0 AND (min_pred_to_true_m <= 25 OR block_hits > 0) THEN 'adjacent_lot_near_miss' WHEN pred_bbls > 0 AND lot_hits = 0 THEN 'residual_truth_contamination_suspect' ELSE 'no_failure_class' END AS failure_class, IFF(max_offset_days > 30 OR any_recorded_legal_boro_mismatch = 1, 1, 0) AS contamination_signal FROM point_roll)
SELECT failure_class, COUNT(*) AS points, SUM(IFF(contamination_signal = 1, 1, 0)) AS contamination_signal_points, SUM(IFF(condo_sig = 1, 1, 0)) AS condo_signature_points, SUM(IFF(truth_bbls > 1, 1, 0)) AS multi_truth_points, SUM(IFF(missing_true_geom_edges > 0, 1, 0)) AS missing_true_geom_points, MIN(ROUND(min_pred_to_true_m, 2)) AS min_pred_to_true_m, AVG(ROUND(min_pred_to_true_m, 2)) AS avg_pred_to_true_m, MAX(ROUND(min_pred_to_true_m, 2)) AS max_pred_to_true_m, MIN(ROUND(min_point_to_true_m, 2)) AS min_point_to_true_m, AVG(ROUND(min_point_to_true_m, 2)) AS avg_point_to_true_m, MAX(ROUND(min_point_to_true_m, 2)) AS max_point_to_true_m FROM classified WHERE pred_bbls > 0 AND lot_hits = 0 GROUP BY failure_class
UNION ALL
SELECT failure_class, COUNT(*) AS points, SUM(IFF(contamination_signal = 1, 1, 0)) AS contamination_signal_points, SUM(IFF(condo_sig = 1, 1, 0)) AS condo_signature_points, SUM(IFF(truth_bbls > 1, 1, 0)) AS multi_truth_points, SUM(IFF(missing_true_geom_edges > 0, 1, 0)) AS missing_true_geom_points, MIN(ROUND(min_pred_to_true_m, 2)) AS min_pred_to_true_m, AVG(ROUND(min_pred_to_true_m, 2)) AS avg_pred_to_true_m, MAX(ROUND(min_pred_to_true_m, 2)) AS max_pred_to_true_m, MIN(ROUND(min_point_to_true_m, 2)) AS min_point_to_true_m, AVG(ROUND(min_point_to_true_m, 2)) AS avg_point_to_true_m, MAX(ROUND(min_point_to_true_m, 2)) AS max_point_to_true_m FROM classified WHERE pred_bbls > 0 AND lot_hits > 0 GROUP BY failure_class
ORDER BY IFF(failure_class = 'gross_geocode_error_gt500m', 1, IFF(failure_class = 'condo_representation_residue', 2, IFF(failure_class = 'assemblage_artifact_neighbor', 3, IFF(failure_class = 'adjacent_lot_near_miss', 4, IFF(failure_class = 'residual_truth_contamination_suspect', 5, 6))))) LIMIT 200;
```

## E2 Asserted SF Band Tests

Bands:

- `wide_0p50_2p00`: asserted SF / parcel `BLDGAREA` in `[0.5, 2.0]`
- `medium_0p70_1p40`: asserted SF / parcel `BLDGAREA` in `[0.7, 1.4]`
- `nra_0p78_0p95`: asserted SF / parcel `BLDGAREA` in `[0.78, 0.95]`

Definitions:

- `pip_denom`: unique asserted SF and positive PIP-lot `BLDGAREA`.
- `true_denom`: unique asserted SF and every true BBL has a MapPLUTO row with
  positive summed `BLDGAREA`.
- `both_denom`: both PIP and true comparable.
- `pip_log_closer` and `true_log_closer`: smaller absolute log error relative
  to asserted SF among the `both_denom` rows.

### `PROPERTY_MART.PROPERTY_ISSUANCE.SIZE`, `SIZE_MEASURE='SQFT'`

Structured result:

| label | band | any positive | unique asserted | PIP denom | true denom | both denom | PIP in band | true in band | both in band | PIP only | true only | neither | true log closer | PIP log closer | tie |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| correct | medium_0p70_1p40 | 10 | 10 | 10 | 9 | 9 | 4 | 7 | 4 | 0 | 3 | 2 | 4 | 0 | 5 |
| correct | nra_0p78_0p95 | 10 | 10 | 10 | 9 | 9 | 1 | 1 | 1 | 0 | 0 | 8 | 4 | 0 | 5 |
| correct | wide_0p50_2p00 | 10 | 10 | 10 | 9 | 9 | 6 | 8 | 5 | 0 | 3 | 1 | 4 | 0 | 5 |
| incorrect | medium_0p70_1p40 | 17 | 16 | 16 | 5 | 5 | 6 | 1 | 0 | 2 | 1 | 2 | 1 | 4 | 0 |
| incorrect | nra_0p78_0p95 | 17 | 16 | 16 | 5 | 5 | 2 | 0 | 0 | 0 | 0 | 5 | 1 | 4 | 0 |
| incorrect | wide_0p50_2p00 | 17 | 16 | 16 | 5 | 5 | 7 | 1 | 0 | 2 | 1 | 2 | 1 | 4 | 0 |

Exact SQL:

```sql
WITH l AS (SELECT DISTINCT li.LOAN_KEY k,TO_DATE(li.ORIGINATIONDATE) od,ROUND(li.ORIGINALLOANAMOUNT,2) amt FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li ON p.CIK=li.CIK AND p.ASSETNUMBER=li.ASSETNUMBER WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE AND li.ORIGINATIONDATE IS NOT NULL AND li.ORIGINALLOANAMOUNT IS NOT NULL AND MOD(ROUND(li.ORIGINALLOANAMOUNT,2),100000)<>0),
b AS (SELECT DISTINCT li.LOAN_KEY k,CASE TO_VARCHAR(p.COUNTY_FIPS) WHEN '36061' THEN 1 WHEN '36005' THEN 2 WHEN '36047' THEN 3 WHEN '36081' THEN 4 WHEN '36085' THEN 5 END bo FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li ON p.CIK=li.CIK AND p.ASSETNUMBER=li.ASSETNUMBER WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE),
m AS (SELECT DISTINCT DOCUMENT_ID d,TO_DATE(RECORDED_DATETIME) rd,ROUND(DOCUMENT_AMT,2) amt FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT WHERE RELEASE_DT='2026-08-10' AND RECORDED_BOROUGH IN (1,2,3,4,5) AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD') AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000),
c AS (SELECT DISTINCT l.k,m.d FROM l JOIN m ON m.amt=l.amt AND m.rd BETWEEN l.od AND DATEADD(day,45,l.od) JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON m.d=le.DOCUMENT_ID JOIN b ON l.k=b.k AND le.BOROUGH=b.bo WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN (1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL),
cc AS (SELECT k,COUNT(DISTINCT d) n FROM c GROUP BY k),
a AS (SELECT c.k,c.d FROM c JOIN cc USING(k) WHERE n=1),
ab AS (SELECT DISTINCT a.k,TO_VARCHAR(le.BOROUGH)||LPAD(TO_VARCHAR(le.BLOCK),5,'0')||LPAD(TO_VARCHAR(le.LOT),4,'0') bbl FROM a JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON a.d=le.DOCUMENT_ID WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN (1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL),
ps AS (SELECT DISTINCT p.PROPERTY_KEY pk,li.LOAN_KEY k FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li ON p.CIK=li.CIK AND p.ASSETNUMBER=li.ASSETNUMBER WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE),
td AS (SELECT DISTINCT ps.pk,ab.k,ab.bbl,d.LATITUDE lat,d.LONGITUDE lon FROM ab JOIN ps USING(k) JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d ON ps.pk=d.PROPERTY_KEY WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND d.LATITUDE IS NOT NULL AND d.LONGITUDE IS NOT NULL),
pt AS (SELECT LATITUDE lat,LONGITUDE lon FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND LATITUDE IS NOT NULL AND LONGITUDE IS NOT NULL GROUP BY LATITUDE,LONGITUDE),
tb AS (SELECT DISTINCT pt.lat,pt.lon,td.pk,td.bbl FROM pt JOIN td ON pt.lat=td.lat AND pt.lon=td.lon),
pe AS (SELECT DISTINCT pt.lat,pt.lon,REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$','') pbbl,pl.BLDGAREA pba FROM pt JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON ST_CONTAINS(pl.GEOM_GEOG,ST_POINT(pt.lon,pt.lat))),
lb AS (SELECT tb.lat,tb.lon,IFF(COUNT(DISTINCT IFF(pe.pbbl=tb.bbl,pe.pbbl,NULL))>0,'correct','incorrect') cls,COUNT(DISTINCT pe.pbbl) pn,MAX(pe.pba) pba FROM tb LEFT JOIN pe ON tb.lat=pe.lat AND tb.lon=pe.lon GROUP BY tb.lat,tb.lon HAVING COUNT(DISTINCT pe.pbbl)>0),
ta AS (SELECT t.lat,t.lon,COUNT(DISTINCT t.bbl) tn,COUNT(DISTINCT IFF(pl.BBL IS NOT NULL,t.bbl,NULL)) tm,SUM(COALESCE(pl.BLDGAREA,0)) tba FROM (SELECT DISTINCT lat,lon,bbl FROM tb) t LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$','')=t.bbl GROUP BY t.lat,t.lon),
ap AS (SELECT j.lat,j.lon,COUNT(DISTINCT pi.SIZE) ds,MIN(pi.SIZE) sf FROM (SELECT DISTINCT lb.lat,lb.lon,tb.pk FROM lb JOIN tb ON lb.lat=tb.lat AND lb.lon=tb.lon) j JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_ISSUANCE pi ON j.pk=pi.PROPERTY_KEY WHERE pi.SIZE>0 AND UPPER(pi.SIZE_MEASURE)='SQFT' GROUP BY j.lat,j.lon),
pm AS (SELECT lb.cls,ap.ds,ap.sf,lb.pba,ta.tba,ta.tn,ta.tm,ap.sf/NULLIF(lb.pba,0) pr,ap.sf/NULLIF(ta.tba,0) tr,ABS(LN(ap.sf/NULLIF(lb.pba,0))) pe,ABS(LN(ap.sf/NULLIF(ta.tba,0))) te FROM ap JOIN lb ON ap.lat=lb.lat AND ap.lon=lb.lon JOIN ta ON ap.lat=ta.lat AND ap.lon=ta.lon),
bd AS (SELECT 'wide_0p50_2p00' band,0.50 lo,2.00 hi UNION ALL SELECT 'medium_0p70_1p40',0.70,1.40 UNION ALL SELECT 'nra_0p78_0p95',0.78,0.95)
SELECT 'PROPERTY_ISSUANCE_SIZE_SQFT' src,cls,band,COUNT(*) any_positive,SUM(IFF(ds=1,1,0)) unique_asserted,SUM(IFF(ds=1 AND pba>0,1,0)) pip_denom,SUM(IFF(ds=1 AND tm=tn AND tba>0,1,0)) true_denom,SUM(IFF(ds=1 AND pba>0 AND tm=tn AND tba>0,1,0)) both_denom,SUM(IFF(ds=1 AND pba>0 AND pr BETWEEN lo AND hi,1,0)) pip_in_band,SUM(IFF(ds=1 AND tm=tn AND tba>0 AND tr BETWEEN lo AND hi,1,0)) true_in_band,SUM(IFF(ds=1 AND pba>0 AND tm=tn AND tba>0 AND pr BETWEEN lo AND hi AND tr BETWEEN lo AND hi,1,0)) both_in_band,SUM(IFF(ds=1 AND pba>0 AND tm=tn AND tba>0 AND pr BETWEEN lo AND hi AND NOT tr BETWEEN lo AND hi,1,0)) pip_only,SUM(IFF(ds=1 AND pba>0 AND tm=tn AND tba>0 AND NOT pr BETWEEN lo AND hi AND tr BETWEEN lo AND hi,1,0)) true_only,SUM(IFF(ds=1 AND pba>0 AND tm=tn AND tba>0 AND NOT pr BETWEEN lo AND hi AND NOT tr BETWEEN lo AND hi,1,0)) neither,SUM(IFF(ds=1 AND pba>0 AND tm=tn AND tba>0 AND te<pe,1,0)) true_log_closer,SUM(IFF(ds=1 AND pba>0 AND tm=tn AND tba>0 AND pe<te,1,0)) pip_log_closer,SUM(IFF(ds=1 AND pba>0 AND tm=tn AND tba>0 AND pe=te,1,0)) log_tie FROM pm CROSS JOIN bd GROUP BY cls,band ORDER BY cls,band;
```

### Annex `TAPE_SECURITIZATION_SQUARE_FEET`

Structured result:

| label | band | any positive | unique asserted | PIP denom | true denom | both denom | PIP in band | true in band | both in band | PIP only | true only | neither | true log closer | PIP log closer | tie |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| correct | medium_0p70_1p40 | 8 | 8 | 8 | 8 | 8 | 3 | 6 | 3 | 0 | 3 | 2 | 3 | 0 | 5 |
| correct | nra_0p78_0p95 | 8 | 8 | 8 | 8 | 8 | 1 | 1 | 1 | 0 | 0 | 7 | 3 | 0 | 5 |
| correct | wide_0p50_2p00 | 8 | 8 | 8 | 8 | 8 | 4 | 7 | 4 | 0 | 3 | 1 | 3 | 0 | 5 |
| incorrect | medium_0p70_1p40 | 14 | 13 | 13 | 4 | 4 | 5 | 1 | 0 | 2 | 1 | 1 | 1 | 3 | 0 |
| incorrect | nra_0p78_0p95 | 14 | 13 | 13 | 4 | 4 | 1 | 0 | 0 | 0 | 0 | 4 | 1 | 3 | 0 |
| incorrect | wide_0p50_2p00 | 14 | 13 | 13 | 4 | 4 | 5 | 1 | 0 | 2 | 1 | 1 | 1 | 3 | 0 |

Exact SQL:

```sql
WITH l AS (SELECT DISTINCT li.LOAN_KEY k,TO_DATE(li.ORIGINATIONDATE) od,ROUND(li.ORIGINALLOANAMOUNT,2) amt FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li ON p.CIK=li.CIK AND p.ASSETNUMBER=li.ASSETNUMBER WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE AND li.ORIGINATIONDATE IS NOT NULL AND li.ORIGINALLOANAMOUNT IS NOT NULL AND MOD(ROUND(li.ORIGINALLOANAMOUNT,2),100000)<>0),
b AS (SELECT DISTINCT li.LOAN_KEY k,CASE TO_VARCHAR(p.COUNTY_FIPS) WHEN '36061' THEN 1 WHEN '36005' THEN 2 WHEN '36047' THEN 3 WHEN '36081' THEN 4 WHEN '36085' THEN 5 END bo FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li ON p.CIK=li.CIK AND p.ASSETNUMBER=li.ASSETNUMBER WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE),
m AS (SELECT DISTINCT DOCUMENT_ID d,TO_DATE(RECORDED_DATETIME) rd,ROUND(DOCUMENT_AMT,2) amt FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT WHERE RELEASE_DT='2026-08-10' AND RECORDED_BOROUGH IN (1,2,3,4,5) AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD') AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000),
c AS (SELECT DISTINCT l.k,m.d FROM l JOIN m ON m.amt=l.amt AND m.rd BETWEEN l.od AND DATEADD(day,45,l.od) JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON m.d=le.DOCUMENT_ID JOIN b ON l.k=b.k AND le.BOROUGH=b.bo WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN (1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL),
cc AS (SELECT k,COUNT(DISTINCT d) n FROM c GROUP BY k),
a AS (SELECT c.k,c.d FROM c JOIN cc USING(k) WHERE n=1),
ab AS (SELECT DISTINCT a.k,TO_VARCHAR(le.BOROUGH)||LPAD(TO_VARCHAR(le.BLOCK),5,'0')||LPAD(TO_VARCHAR(le.LOT),4,'0') bbl FROM a JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON a.d=le.DOCUMENT_ID WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN (1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL),
ps AS (SELECT DISTINCT p.PROPERTY_KEY pk,li.LOAN_KEY k FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li ON p.CIK=li.CIK AND p.ASSETNUMBER=li.ASSETNUMBER WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE),
td AS (SELECT DISTINCT ps.pk,ab.k,ab.bbl,d.LATITUDE lat,d.LONGITUDE lon FROM ab JOIN ps USING(k) JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d ON ps.pk=d.PROPERTY_KEY WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND d.LATITUDE IS NOT NULL AND d.LONGITUDE IS NOT NULL),
pt AS (SELECT LATITUDE lat,LONGITUDE lon FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND LATITUDE IS NOT NULL AND LONGITUDE IS NOT NULL GROUP BY LATITUDE,LONGITUDE),
tb AS (SELECT DISTINCT pt.lat,pt.lon,td.pk,td.bbl FROM pt JOIN td ON pt.lat=td.lat AND pt.lon=td.lon),
pe AS (SELECT DISTINCT pt.lat,pt.lon,REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$','') pbbl,pl.BLDGAREA pba FROM pt JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON ST_CONTAINS(pl.GEOM_GEOG,ST_POINT(pt.lon,pt.lat))),
lb AS (SELECT tb.lat,tb.lon,IFF(COUNT(DISTINCT IFF(pe.pbbl=tb.bbl,pe.pbbl,NULL))>0,'correct','incorrect') cls,COUNT(DISTINCT pe.pbbl) pn,MAX(pe.pba) pba FROM tb LEFT JOIN pe ON tb.lat=pe.lat AND tb.lon=pe.lon GROUP BY tb.lat,tb.lon HAVING COUNT(DISTINCT pe.pbbl)>0),
ta AS (SELECT t.lat,t.lon,COUNT(DISTINCT t.bbl) tn,COUNT(DISTINCT IFF(pl.BBL IS NOT NULL,t.bbl,NULL)) tm,SUM(COALESCE(pl.BLDGAREA,0)) tba FROM (SELECT DISTINCT lat,lon,bbl FROM tb) t LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$','')=t.bbl GROUP BY t.lat,t.lon),
ap AS (SELECT j.lat,j.lon,COUNT(DISTINCT x.TAPE_SECURITIZATION_SQUARE_FEET) ds,MIN(x.TAPE_SECURITIZATION_SQUARE_FEET) sf FROM (SELECT DISTINCT lb.lat,lb.lon,tb.pk FROM lb JOIN tb ON lb.lat=tb.lat AND lb.lon=tb.lon) j JOIN EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_ANNEX_ISSUANCE_FACT_COMPARISON x ON j.pk=x.PROPERTY_KEY WHERE x.TAPE_SECURITIZATION_SQUARE_FEET>0 GROUP BY j.lat,j.lon),
pm AS (SELECT lb.cls,ap.ds,ap.sf,lb.pba,ta.tba,ta.tn,ta.tm,ap.sf/NULLIF(lb.pba,0) pr,ap.sf/NULLIF(ta.tba,0) tr,ABS(LN(ap.sf/NULLIF(lb.pba,0))) pe,ABS(LN(ap.sf/NULLIF(ta.tba,0))) te FROM ap JOIN lb ON ap.lat=lb.lat AND ap.lon=lb.lon JOIN ta ON ap.lat=ta.lat AND ap.lon=ta.lon),
bd AS (SELECT 'wide_0p50_2p00' band,0.50 lo,2.00 hi UNION ALL SELECT 'medium_0p70_1p40',0.70,1.40 UNION ALL SELECT 'nra_0p78_0p95',0.78,0.95)
SELECT 'ANNEX_TAPE_SECURITIZATION_SQUARE_FEET' src,cls,band,COUNT(*) any_positive,SUM(IFF(ds=1,1,0)) unique_asserted,SUM(IFF(ds=1 AND pba>0,1,0)) pip_denom,SUM(IFF(ds=1 AND tm=tn AND tba>0,1,0)) true_denom,SUM(IFF(ds=1 AND pba>0 AND tm=tn AND tba>0,1,0)) both_denom,SUM(IFF(ds=1 AND pba>0 AND pr BETWEEN lo AND hi,1,0)) pip_in_band,SUM(IFF(ds=1 AND tm=tn AND tba>0 AND tr BETWEEN lo AND hi,1,0)) true_in_band,SUM(IFF(ds=1 AND pba>0 AND tm=tn AND tba>0 AND pr BETWEEN lo AND hi AND tr BETWEEN lo AND hi,1,0)) both_in_band,SUM(IFF(ds=1 AND pba>0 AND tm=tn AND tba>0 AND pr BETWEEN lo AND hi AND NOT tr BETWEEN lo AND hi,1,0)) pip_only,SUM(IFF(ds=1 AND pba>0 AND tm=tn AND tba>0 AND NOT pr BETWEEN lo AND hi AND tr BETWEEN lo AND hi,1,0)) true_only,SUM(IFF(ds=1 AND pba>0 AND tm=tn AND tba>0 AND NOT pr BETWEEN lo AND hi AND NOT tr BETWEEN lo AND hi,1,0)) neither,SUM(IFF(ds=1 AND pba>0 AND tm=tn AND tba>0 AND te<pe,1,0)) true_log_closer,SUM(IFF(ds=1 AND pba>0 AND tm=tn AND tba>0 AND pe<te,1,0)) pip_log_closer,SUM(IFF(ds=1 AND pba>0 AND tm=tn AND tba>0 AND pe=te,1,0)) log_tie FROM pm CROSS JOIN bd GROUP BY cls,band ORDER BY cls,band;
```

## E2 Parcel Attribute Side-by-Side

This is not an asserted-channel score; it records the parcel-side attributes
available for E3/E4 interpretation.

Structured result:

| label | points | PIP area points | true area complete | avg PIP NUMBLDGS | avg true NUMBLDGS | sum PIP NUMBLDGS | sum true NUMBLDGS | PIP yearbuilt points | true yearbuilt complete | min PIP year | max PIP year | min true year | max true year |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| correct | 154 | 154 | 152 | 1.94 | 7.70 | 298 | 1170 | 154 | 150 | 1808 | 2022 | 0 | 2022 |
| incorrect | 79 | 79 | 45 | 1.18 | 5.69 | 93 | 256 | 79 | 45 | 1857 | 2019 | 1896 | 2024 |

Caveat: the correct-sample `min_true_yearbuilt=0` appears because the query's
`MIN` reports over complete true-BBL rows while `true_yearbuilt_complete_points`
counts only rows where the minimum year is positive. Treat `0` as a missing or
sentinel year value, not a construction year.

Exact SQL:

```sql
WITH l AS (SELECT DISTINCT li.LOAN_KEY k,TO_DATE(li.ORIGINATIONDATE) od,ROUND(li.ORIGINALLOANAMOUNT,2) amt FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li ON p.CIK=li.CIK AND p.ASSETNUMBER=li.ASSETNUMBER WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE AND li.ORIGINATIONDATE IS NOT NULL AND li.ORIGINALLOANAMOUNT IS NOT NULL AND MOD(ROUND(li.ORIGINALLOANAMOUNT,2),100000)<>0),
b AS (SELECT DISTINCT li.LOAN_KEY k,CASE TO_VARCHAR(p.COUNTY_FIPS) WHEN '36061' THEN 1 WHEN '36005' THEN 2 WHEN '36047' THEN 3 WHEN '36081' THEN 4 WHEN '36085' THEN 5 END bo FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li ON p.CIK=li.CIK AND p.ASSETNUMBER=li.ASSETNUMBER WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE),
m AS (SELECT DISTINCT DOCUMENT_ID d,TO_DATE(RECORDED_DATETIME) rd,ROUND(DOCUMENT_AMT,2) amt FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT WHERE RELEASE_DT='2026-08-10' AND RECORDED_BOROUGH IN (1,2,3,4,5) AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD') AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000),
c AS (SELECT DISTINCT l.k,m.d FROM l JOIN m ON m.amt=l.amt AND m.rd BETWEEN l.od AND DATEADD(day,45,l.od) JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON m.d=le.DOCUMENT_ID JOIN b ON l.k=b.k AND le.BOROUGH=b.bo WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN (1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL),
cc AS (SELECT k,COUNT(DISTINCT d) n FROM c GROUP BY k),
a AS (SELECT c.k,c.d FROM c JOIN cc USING(k) WHERE n=1),
ab AS (SELECT DISTINCT a.k,TO_VARCHAR(le.BOROUGH)||LPAD(TO_VARCHAR(le.BLOCK),5,'0')||LPAD(TO_VARCHAR(le.LOT),4,'0') bbl FROM a JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON a.d=le.DOCUMENT_ID WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN (1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL),
ps AS (SELECT DISTINCT p.PROPERTY_KEY pk,li.LOAN_KEY k FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li ON p.CIK=li.CIK AND p.ASSETNUMBER=li.ASSETNUMBER WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE),
td AS (SELECT DISTINCT ps.pk,ab.k,ab.bbl,d.LATITUDE lat,d.LONGITUDE lon FROM ab JOIN ps USING(k) JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d ON ps.pk=d.PROPERTY_KEY WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND d.LATITUDE IS NOT NULL AND d.LONGITUDE IS NOT NULL),
pt AS (SELECT LATITUDE lat,LONGITUDE lon FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND LATITUDE IS NOT NULL AND LONGITUDE IS NOT NULL GROUP BY LATITUDE,LONGITUDE),
tb AS (SELECT DISTINCT pt.lat,pt.lon,td.bbl FROM pt JOIN td ON pt.lat=td.lat AND pt.lon=td.lon),
pe AS (SELECT DISTINCT pt.lat,pt.lon,REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$','') pbbl,pl.BLDGAREA pba,pl.NUMBLDGS pnb,pl.YEARBUILT pyb FROM pt JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON ST_CONTAINS(pl.GEOM_GEOG,ST_POINT(pt.lon,pt.lat))),
lb AS (SELECT tb.lat,tb.lon,IFF(COUNT(DISTINCT IFF(pe.pbbl=tb.bbl,pe.pbbl,NULL))>0,'correct','incorrect') cls,COUNT(DISTINCT pe.pbbl) pn,MAX(pe.pba) pba,MAX(pe.pnb) pnb,MAX(pe.pyb) pyb FROM tb LEFT JOIN pe ON tb.lat=pe.lat AND tb.lon=pe.lon GROUP BY tb.lat,tb.lon HAVING COUNT(DISTINCT pe.pbbl)>0),
ta AS (SELECT t.lat,t.lon,COUNT(DISTINCT t.bbl) tn,COUNT(DISTINCT IFF(pl.BBL IS NOT NULL,t.bbl,NULL)) tm,SUM(COALESCE(pl.BLDGAREA,0)) tba,SUM(COALESCE(pl.NUMBLDGS,0)) tnb,MIN(pl.YEARBUILT) tyb_min,MAX(pl.YEARBUILT) tyb_max FROM (SELECT DISTINCT lat,lon,bbl FROM tb) t LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$','')=t.bbl GROUP BY t.lat,t.lon),
pm AS (SELECT lb.cls,lb.pba,lb.pnb,lb.pyb,ta.tn,ta.tm,ta.tba,ta.tnb,ta.tyb_min,ta.tyb_max FROM lb JOIN ta ON lb.lat=ta.lat AND lb.lon=ta.lon)
SELECT cls,COUNT(*) points,SUM(IFF(pba>0,1,0)) pip_area_points,SUM(IFF(tm=tn AND tba>0,1,0)) true_area_complete_points,ROUND(AVG(pnb),2) avg_pip_num_bldgs,ROUND(AVG(IFF(tm=tn,tnb,NULL)),2) avg_true_num_bldgs_complete,SUM(pnb) sum_pip_num_bldgs,SUM(IFF(tm=tn,tnb,NULL)) sum_true_num_bldgs_complete,SUM(IFF(pyb IS NOT NULL AND pyb>0,1,0)) pip_yearbuilt_points,SUM(IFF(tm=tn AND tyb_min IS NOT NULL AND tyb_min>0,1,0)) true_yearbuilt_complete_points,MIN(pyb) min_pip_yearbuilt,MAX(pyb) max_pip_yearbuilt,MIN(IFF(tm=tn,tyb_min,NULL)) min_true_yearbuilt,MAX(IFF(tm=tn,tyb_max,NULL)) max_true_yearbuilt FROM pm GROUP BY cls ORDER BY cls;
```

## Sanity Notes

- E1 class denominator: 79 PIP-covered Gate V2 lot-incorrect points.
- E1 classifier sanity sample: 154 PIP-covered Gate V2 lot-correct points.
- E2 band denominators are not 154/79; they are source-coverage denominators
  after positive asserted SF, unit filtering, uniqueness, and parcel-area
  completeness.
- The first all-in-one E2 query exceeded the CLI request limit and was split
  into source-specific SQL. A `TRY_TO_DOUBLE` cast attempt failed because some
  source columns were already numeric; the cited runs use numeric casts or
  native numeric fields.
- A later compact E1 rerun returned prose-only with no tool trace; it is not
  cited here.
