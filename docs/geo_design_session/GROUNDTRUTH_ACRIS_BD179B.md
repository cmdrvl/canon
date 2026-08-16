# bd-179b ACRIS Ground Truth

Date: 2026-08-16

Agent: PearlSparrow

Scope: five-borough CMBS geocode scope from
`EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED` with
`COUNTY_FIPS in ('36005','36047','36061','36081','36085')`.

Data access discipline: every cited number below came from
`cmdrvl orchestrator query --tenant salt --timeout 300 --raw` and returned
`tool_responses[*].structuredContent`. Loom prose is not cited.

## Status

ACRIS Source 1 is usable for a small, clean, address-independent truth set.
The operating gate is exact cents on origination amount and a +/-30 day
recording-date window, with uniqueness required per CMBS loan and ambiguous
loans discarded.

Headline result:

- ACRIS truth set: 523 accepted five-borough loans from a 3,040-loan denominator.
- Geometry PIP lot-grade precision on ACRIS-covered baseline points: 166 / 563
  = 29.48%. Block-grade diagnostic upper bound: 193 / 563 = 34.28%.
- Naive address-key lot-grade precision on ACRIS-covered address keys: 67 / 286
  = 23.43%. Block-grade diagnostic upper bound: 82 / 286 = 28.67%.
- Nearest-rooftop geometry PIP lot-grade precision: 16 / 69 = 23.19%.
  Block-grade diagnostic upper bound: 17 / 69 = 24.64%.

The truth set coverage is low and should be reported as low: 582 / 4,076
baseline points = 14.28%, and 864 / 5,269 address-county keys = 16.40%.

Follow-up representation-confound diagnostic: MapPLUTO `26v1` exposes
`BOROUGH`, `BLOCK`, `LOT`, and `BBL`, but no `CONDONO` or other condo metadata
in the landed schema. Condo signature below therefore uses the explicit heuristic
requested by the orchestrator: any ACRIS legal `LOT` between 1001 and 6999.
This shows the lot-grade precision is representation-confounded, but only a
minority of the headline misses are same-block/lot-mismatch cases. Most covered
misses remain full-block mismatches.

## G1: ACRIS Discovery

Initial base-table discovery was a negative result because ACRIS is landed as
external tables, not base tables.

| discovery query | structured result |
|---|---:|
| SOURCE base tables with ACRIS in the name | 0 rows |
| ACRIS HOT/materialized candidates | 0 rows |

```sql
SELECT table_schema, table_name, table_type, row_count
FROM EDGAR_DB.INFORMATION_SCHEMA.TABLES
WHERE table_schema = 'SOURCE'
  AND table_type = 'BASE TABLE'
  AND table_name ILIKE '%ACRIS%'
ORDER BY table_name;
```

```sql
SELECT table_schema, table_name, table_type, row_count, bytes, created, last_altered
FROM EDGAR_DB.INFORMATION_SCHEMA.TABLES
WHERE table_name ILIKE '%ACRIS%HOT%'
   OR table_name ILIKE '%ACRIS%MATERIAL%'
   OR table_name ILIKE '%REAL_PROPERTY%HOT%'
ORDER BY table_schema, table_name;
```

### External Tables

The ACRIS release is present in `EDGAR_DB.SOURCE` as external tables.

| table | table_type | bytes | created | last_altered |
|---|---|---:|---|---|
| `NYC_ACRIS_DOCUMENT_CONTROL_CODES_EXT` | EXTERNAL TABLE | 17,333 | 2026-08-14 00:44:40.815 -0700 | 2026-08-14 03:02:31.856 -0700 |
| `NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT` | EXTERNAL TABLE | 1,979,165,777 | 2026-08-14 00:44:42.129 -0700 | 2026-08-14 03:02:34.326 -0700 |
| `NYC_ACRIS_REAL_PROPERTY_MASTER_EXT` | EXTERNAL TABLE | 1,818,172,412 | 2026-08-14 00:44:41.509 -0700 | 2026-08-14 03:02:33.004 -0700 |
| `NYC_ACRIS_REAL_PROPERTY_META_EXT` | EXTERNAL TABLE | 3,972,868 | 2026-08-14 00:44:44.831 -0700 | 2026-08-14 03:02:41.054 -0700 |
| `NYC_ACRIS_REAL_PROPERTY_PARTIES_EXT` | EXTERNAL TABLE | 4,880,679,438 | 2026-08-14 00:44:42.773 -0700 | 2026-08-14 03:02:35.705 -0700 |
| `NYC_ACRIS_REAL_PROPERTY_REFERENCES_EXT` | EXTERNAL TABLE | 579,839,166 | 2026-08-14 00:44:43.477 -0700 | 2026-08-14 03:02:37.422 -0700 |
| `NYC_ACRIS_REAL_PROPERTY_REMARKS_EXT` | EXTERNAL TABLE | 452,577,551 | 2026-08-14 00:44:44.134 -0700 | 2026-08-14 03:02:38.519 -0700 |

```sql
SELECT table_schema, table_name, table_type, row_count, bytes, created, last_altered, comment
FROM EDGAR_DB.INFORMATION_SCHEMA.TABLES
WHERE table_schema = 'SOURCE'
  AND (
      table_name ILIKE 'NYC_ACRIS%'
      OR table_name ILIKE '%ACRIS%'
      OR table_name IN (
          'NYC_ACRIS_REAL_PROPERTY_MASTER_EXT',
          'NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT',
          'NYC_ACRIS_REAL_PROPERTY_PARTIES_EXT',
          'NYC_ACRIS_DOCUMENT_CONTROL_CODES_EXT',
          'NYC_ACRIS_REFERENCES_EXT',
          'NYC_ACRIS_REMARKS_EXT',
          'NYC_ACRIS_META_EXT'
      )
  )
ORDER BY table_name;
```

### Release And Row Counts

`NYC_ACRIS_REAL_PROPERTY_META_EXT` pins `RELEASE_DT = '2026-08-10'`.

| artifact | row_count | raw_row_count | bytes | source_dataset_id | source_rows_updated_at |
|---|---:|---:|---:|---|---|
| manifest | 100,764,969 | 100,764,969 | 4,227,500 | n/a | n/a |
| document_control_codes normalized | 126 | 126 | 17,333 | `7isb-wh4c` | 2026-08-05 13:49:57 |
| document_control_codes raw | 126 | 126 | 2,051 | `7isb-wh4c` | 2026-08-05 13:49:57 |
| real_property_legals normalized | 22,727,180 | 22,727,180 | 1,979,165,777 | `8h5j-fqxa` | 2026-08-10 13:34:41 |
| real_property_legals raw | 22,727,180 | 22,727,180 | 438,391,262 | `8h5j-fqxa` | 2026-08-10 13:34:41 |
| real_property_master normalized | 17,065,090 | 17,065,090 | 1,818,172,412 | `bnx9-e6tj` | 2026-08-10 13:35:55 |
| real_property_master raw | 17,065,090 | 17,065,090 | 420,079,117 | `bnx9-e6tj` | 2026-08-10 13:35:55 |
| real_property_parties normalized | 46,540,137 | 46,540,137 | 4,880,679,438 | `636b-3b5g` | 2026-08-10 13:34:59 |
| real_property_parties raw | 46,540,137 | 46,540,137 | 1,270,813,529 | `636b-3b5g` | 2026-08-10 13:34:59 |
| real_property_references normalized | 8,699,896 | 8,699,896 | 579,839,166 | `pwkr-dpni` | 2026-08-10 13:35:16 |
| real_property_references raw | 8,699,896 | 8,699,896 | 99,726,208 | `pwkr-dpni` | 2026-08-10 13:35:16 |
| real_property_remarks normalized | 5,732,540 | 5,732,540 | 452,577,551 | `9p4w-7npp` | 2026-08-10 13:35:22 |
| real_property_remarks raw | 5,732,540 | 5,732,540 | 107,162,766 | `9p4w-7npp` | 2026-08-10 13:35:22 |

```sql
SELECT
    release_dt,
    dataset_slug,
    artifact,
    ANY_VALUE(dataset_name) AS dataset_name,
    COUNT(*) AS meta_rows,
    SUM(row_count) AS summed_row_count,
    SUM(raw_row_count) AS summed_raw_row_count,
    SUM(bytes) AS summed_bytes,
    MIN(source_rows_updated_at) AS min_source_rows_updated_at,
    MAX(source_rows_updated_at) AS max_source_rows_updated_at,
    LISTAGG(DISTINCT source_dataset_id, ', ')
        WITHIN GROUP (ORDER BY source_dataset_id) AS source_dataset_ids
FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_META_EXT
WHERE release_dt = '2026-08-10'
GROUP BY 1,2,3
ORDER BY dataset_slug, artifact;
```

### Key Columns

The column sweep returned 155 columns across the seven external tables. Key
columns for this bead:

- Master: `DOCUMENT_ID`, `DOC_TYPE`, `DOCUMENT_AMT`, `DOCUMENT_DATE`,
  `RECORDED_DATETIME`, `RECORDED_BOROUGH`, `CRFN`, `RELEASE_DT`.
- Legals: `DOCUMENT_ID`, `BOROUGH`, `BLOCK`, `LOT`, `BBL`, `STREET_NUMBER`,
  `STREET_NAME`, `UNIT`, `RELEASE_DT`.
- Parties: `DOCUMENT_ID`, `PARTY_TYPE`, `NAME`, address fields, `RELEASE_DT`.
- Control codes: `DOC_TYPE`, `DOC_TYPE_DESCRIPTION`,
  `CLASS_CODE_DESCRIPTION`, party type descriptions, `RELEASE_DT`.

```sql
SELECT
    table_name,
    ordinal_position,
    column_name,
    data_type
FROM EDGAR_DB.INFORMATION_SCHEMA.COLUMNS
WHERE table_schema = 'SOURCE'
  AND table_name IN (
      'NYC_ACRIS_DOCUMENT_CONTROL_CODES_EXT',
      'NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT',
      'NYC_ACRIS_REAL_PROPERTY_MASTER_EXT',
      'NYC_ACRIS_REAL_PROPERTY_META_EXT',
      'NYC_ACRIS_REAL_PROPERTY_PARTIES_EXT',
      'NYC_ACRIS_REAL_PROPERTY_REFERENCES_EXT',
      'NYC_ACRIS_REAL_PROPERTY_REMARKS_EXT'
  )
ORDER BY table_name, ordinal_position;
```

### Mortgage-Class Codes

The control-code query returned 32 mortgage/agreement/assignment-like rows. For
the truth set I used only primary mortgage instruments:
`('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD')`. Releases, satisfactions,
assignments, corrections, and broad agreement rows were excluded to keep the
truth set small and clean.

```sql
SELECT
    release_dt,
    record_type,
    doc_type,
    doc_type_description,
    class_code_description,
    party1_type,
    party2_type,
    party3_type
FROM EDGAR_DB.SOURCE.NYC_ACRIS_DOCUMENT_CONTROL_CODES_EXT
WHERE release_dt = '2026-08-10'
  AND (
      UPPER(COALESCE(doc_type_description,'')) LIKE '%MORT%'
      OR UPPER(COALESCE(class_code_description,'')) LIKE '%MORT%'
      OR UPPER(COALESCE(doc_type_description,'')) LIKE '%AGREEMENT%'
      OR UPPER(COALESCE(class_code_description,'')) LIKE '%AGREEMENT%'
      OR UPPER(COALESCE(doc_type_description,'')) LIKE '%ASSIGN%'
      OR UPPER(COALESCE(class_code_description,'')) LIKE '%ASSIGN%'
  )
ORDER BY doc_type;
```

The bounded ACRIS master candidate count for these codes and the CMBS amount/date
range was:

| doc_type | candidate_master_rows | candidate_documents | min_recorded_date | max_recorded_date | min_document_amt | max_document_amt |
|---|---:|---:|---|---|---:|---:|
| MTGE | 277,522 | 276,940 | 2015-07-01 | 2026-07-31 | 505,894.91 | 135,000,000 |
| M&CON | 11,534 | 11,497 | 2015-07-01 | 2026-07-31 | 505,888.70 | 134,400,000 |
| SPRD | 955 | 945 | 2015-07-01 | 2026-07-22 | 506,039.44 | 134,200,414.53 |
| CMTG | 647 | 636 | 2016-02-02 | 2026-07-31 | 517,300 | 126,075,000 |
| SMTG | 76 | 76 | 2015-07-30 | 2026-04-23 | 519,739 | 109,320,240 |

```sql
SELECT
    doc_type,
    COUNT(*) AS candidate_master_rows,
    COUNT(DISTINCT document_id) AS candidate_documents,
    MIN(CAST(recorded_datetime AS DATE)) AS min_recorded_date,
    MAX(CAST(recorded_datetime AS DATE)) AS max_recorded_date,
    MIN(document_amt) AS min_document_amt,
    MAX(document_amt) AS max_document_amt
FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT
WHERE release_dt = '2026-08-10'
  AND recorded_borough IN (1,2,3,4,5)
  AND doc_type IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD')
  AND document_amt BETWEEN 505879.44 AND 135000000
  AND CAST(recorded_datetime AS DATE) BETWEEN DATEADD(day,-180,'2015-12-28'::DATE)
                                          AND DATEADD(day,180,'2026-08-03'::DATE)
GROUP BY doc_type
ORDER BY candidate_master_rows DESC;
```

### BBL Normalization

ACRIS legals carries both `BBL` and borough/block/lot components. A 10-row sample
from `RELEASE_DT = '2026-08-10'` matched explicit component normalization:
`TO_VARCHAR(BOROUGH) || LPAD(BLOCK, 5, '0') || LPAD(LOT, 4, '0')`.

```sql
SELECT
    release_dt,
    document_id,
    borough,
    block,
    lot,
    bbl AS acris_bbl,
    TO_VARCHAR(borough) || LPAD(TO_VARCHAR(block), 5, '0') || LPAD(TO_VARCHAR(lot), 4, '0') AS normalized_from_components,
    IFF(
        bbl = TO_VARCHAR(borough) || LPAD(TO_VARCHAR(block), 5, '0') || LPAD(TO_VARCHAR(lot), 4, '0'),
        TRUE,
        FALSE
    ) AS component_matches_bbl
FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT
WHERE release_dt = '2026-08-10'
  AND borough IN (1,2,3,4,5)
  AND block IS NOT NULL
  AND lot IS NOT NULL
  AND bbl IS NOT NULL
LIMIT 10;
```

## G2: Address-Free CMBS Loan Bridge

`WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED` has geocode/address fields only; it has no
loan key, amount, origination date, or deal key. The address-free bridge is:

`PROPERTY_MART.PROPERTY_PERIOD_FACT` -> `PROPERTY_MART.LOAN_ISSUANCE` on
`(CIK, ASSETNUMBER)`.

The attempted join on `LOAN_ASSET_KEY = LOAN_KEY` produced zero joined pairs.
The `(CIK, ASSETNUMBER)` bridge produced the usable loan scope:

| measure | count |
|---|---:|
| property-period key rows | 162,766 |
| distinct property keys | 2,941 |
| distinct CIK/ASSETNUMBER pairs | 4,447 |
| distinct loan asset keys | 99 |
| rows missing loan asset key | 0 |
| property-loan pairs joined by loan asset key | 0 |
| property-loan pairs joined by CIK/ASSETNUMBER | 4,248 |
| property-loan pairs joined either key | 4,248 |
| property-loan pairs with amount and date | 4,236 |

```sql
WITH p AS (
    SELECT DISTINCT
        property_key,
        cik,
        assetnumber,
        loan_asset_key,
        filing_id,
        county_fips
    FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT
    WHERE county_fips IN ('36005','36047','36061','36081','36085')
      AND has_loan = TRUE
),
l AS (
    SELECT
        loan_key,
        cik,
        assetnumber,
        first_seen_filing_id,
        originationdate,
        originalloanamount
    FROM EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE
)
SELECT
    COUNT(*) AS property_period_key_rows,
    COUNT(DISTINCT p.property_key) AS distinct_property_keys,
    COUNT(DISTINCT p.cik || '|' || p.assetnumber) AS distinct_cik_asset_pairs,
    COUNT(DISTINCT p.loan_asset_key) AS distinct_loan_asset_keys,
    SUM(IFF(p.loan_asset_key IS NULL, 1, 0)) AS rows_missing_loan_asset_key,
    COUNT(DISTINCT IFF(l_by_key.loan_key IS NOT NULL, p.property_key || '|' || l_by_key.loan_key, NULL)) AS property_loan_pairs_joined_by_loan_asset_key,
    COUNT(DISTINCT IFF(l_by_asset.loan_key IS NOT NULL, p.property_key || '|' || l_by_asset.loan_key, NULL)) AS property_loan_pairs_joined_by_cik_assetnumber,
    COUNT(DISTINCT IFF(COALESCE(l_by_key.loan_key, l_by_asset.loan_key) IS NOT NULL, p.property_key || '|' || COALESCE(l_by_key.loan_key, l_by_asset.loan_key), NULL)) AS property_loan_pairs_joined_either_key,
    COUNT(DISTINCT IFF(l_by_asset.loan_key IS NOT NULL
                       AND l_by_asset.originationdate IS NOT NULL
                       AND l_by_asset.originalloanamount IS NOT NULL,
                       p.property_key || '|' || l_by_asset.loan_key, NULL)) AS property_loan_pairs_with_amount_and_date
FROM p
LEFT JOIN l l_by_key
  ON p.loan_asset_key = l_by_key.loan_key
LEFT JOIN l l_by_asset
  ON p.cik = l_by_asset.cik
 AND p.assetnumber = l_by_asset.assetnumber;
```

The distinct five-borough loan denominator with amount and origination date is
3,040 loans across 4,236 property-loan pairs and 2,669 property keys.

| property_loan_pairs | distinct_loans | distinct_property_keys | min_origination_date | max_origination_date | min_amount | max_amount |
|---:|---:|---:|---|---|---:|---:|
| 4,236 | 3,040 | 2,669 | 2015-12-28 | 2026-08-03 | 505,879.44 | 135,000,000 |

```sql
WITH p AS (
    SELECT DISTINCT property_key, cik, assetnumber
    FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT
    WHERE county_fips IN ('36005','36047','36061','36081','36085')
      AND has_loan = TRUE
),
lp AS (
    SELECT DISTINCT
        p.property_key,
        l.loan_key,
        l.cik,
        l.assetnumber,
        l.originationdate,
        l.originalloanamount
    FROM p
    JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l
      ON p.cik = l.cik
     AND p.assetnumber = l.assetnumber
    WHERE l.originationdate IS NOT NULL
      AND l.originalloanamount IS NOT NULL
)
SELECT
    COUNT(*) AS property_loan_pairs,
    COUNT(DISTINCT loan_key) AS distinct_loans,
    COUNT(DISTINCT property_key) AS distinct_property_keys,
    MIN(originationdate) AS min_originationdate,
    MAX(originationdate) AS max_originationdate,
    MIN(originalloanamount) AS min_originalloanamount,
    MAX(originalloanamount) AS max_originalloanamount,
    COUNT(DISTINCT ROUND(originalloanamount,0)) AS distinct_rounded_amounts,
    COUNT(DISTINCT originationdate) AS distinct_origination_dates
FROM lp;
```

## G3-G4: Truth Set And Sensitivity

Uniqueness gate: accept a CMBS loan only when amount plus date-window yields
exactly one ACRIS document. Ambiguous loans are discarded.

| amount_mode | window_days | loan_denominator | candidate_loan_document_matches | loans_with_any_candidate | unique_accept_loans | ambiguous_discard_loans | no_match_loans | accepted_loans_with_bbl | accepted_loan_bbl_edges | accepted_loans_one_bbl | accepted_loans_multi_bbl | max_bbls |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| exact_cents | 30 | 3,040 | 17,244 | 1,753 | 523 | 1,230 | 1,287 | 523 | 1,695 | 392 | 131 | 172 |
| exact_cents | 90 | 3,040 | 49,127 | 2,192 | 410 | 1,782 | 848 | 410 | 1,678 | 286 | 124 | 172 |
| exact_cents | 180 | 3,040 | 95,717 | 2,406 | 341 | 2,065 | 634 | 341 | 1,850 | 221 | 120 | 560 |
| rounded_dollar | 30 | 3,040 | 17,260 | 1,753 | 523 | 1,230 | 1,287 | 523 | 1,695 | 392 | 131 | 172 |
| rounded_dollar | 90 | 3,040 | 49,164 | 2,192 | 409 | 1,783 | 848 | 409 | 1,676 | 286 | 123 | 172 |
| rounded_dollar | 180 | 3,040 | 95,773 | 2,406 | 341 | 2,065 | 634 | 341 | 1,850 | 221 | 120 | 560 |

Operating point selected: `exact_cents`, +/-30 days. It keeps the strictest
amount equality, maximizes unique accepts in this sensitivity grid, and avoids
the wider-window ambiguity increase. Rounded-dollar matching adds no accepts at
+/-30 days.

```sql
WITH loan_scope AS (
    SELECT DISTINCT
        l.LOAN_KEY,
        l.CIK,
        l.ASSETNUMBER,
        CAST(l.ORIGINATIONDATE AS DATE) AS ORIGINATION_DATE,
        ROUND(l.ORIGINALLOANAMOUNT, 2) AS ORIGINAL_AMOUNT_CENTS,
        ROUND(l.ORIGINALLOANAMOUNT, 0) AS ORIGINAL_AMOUNT_DOLLARS
    FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p
    JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l
      ON p.CIK = l.CIK
     AND p.ASSETNUMBER = l.ASSETNUMBER
    WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND p.HAS_LOAN = TRUE
      AND l.ORIGINATIONDATE IS NOT NULL
      AND l.ORIGINALLOANAMOUNT IS NOT NULL
),
windows AS (
    SELECT 30 AS window_days UNION ALL
    SELECT 90 UNION ALL
    SELECT 180
),
modes AS (
    SELECT 'exact_cents' AS amount_mode UNION ALL
    SELECT 'rounded_dollar'
),
mode_windows AS (
    SELECT amount_mode, window_days
    FROM modes CROSS JOIN windows
),
acris_docs AS (
    SELECT DISTINCT
        DOCUMENT_ID,
        CAST(RECORDED_DATETIME AS DATE) AS RECORDED_DATE,
        ROUND(DOCUMENT_AMT, 2) AS DOCUMENT_AMOUNT_CENTS,
        ROUND(DOCUMENT_AMT, 0) AS DOCUMENT_AMOUNT_DOLLARS
    FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT
    WHERE RELEASE_DT = '2026-08-10'
      AND RECORDED_BOROUGH IN (1,2,3,4,5)
      AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD')
      AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000
      AND CAST(RECORDED_DATETIME AS DATE) BETWEEN DATEADD(day, -180, (SELECT MIN(ORIGINATION_DATE) FROM loan_scope))
                                               AND DATEADD(day,  180, (SELECT MAX(ORIGINATION_DATE) FROM loan_scope))
),
candidates AS (
    SELECT
        'exact_cents' AS amount_mode,
        w.window_days,
        l.LOAN_KEY,
        a.DOCUMENT_ID
    FROM loan_scope l
    JOIN windows w
      ON TRUE
    JOIN acris_docs a
      ON a.DOCUMENT_AMOUNT_CENTS = l.ORIGINAL_AMOUNT_CENTS
     AND a.RECORDED_DATE BETWEEN DATEADD(day, -w.window_days, l.ORIGINATION_DATE)
                             AND DATEADD(day,  w.window_days, l.ORIGINATION_DATE)
    UNION ALL
    SELECT
        'rounded_dollar' AS amount_mode,
        w.window_days,
        l.LOAN_KEY,
        a.DOCUMENT_ID
    FROM loan_scope l
    JOIN windows w
      ON TRUE
    JOIN acris_docs a
      ON a.DOCUMENT_AMOUNT_DOLLARS = l.ORIGINAL_AMOUNT_DOLLARS
     AND a.RECORDED_DATE BETWEEN DATEADD(day, -w.window_days, l.ORIGINATION_DATE)
                             AND DATEADD(day,  w.window_days, l.ORIGINATION_DATE)
),
loan_candidate_counts AS (
    SELECT
        amount_mode,
        window_days,
        LOAN_KEY,
        COUNT(DISTINCT DOCUMENT_ID) AS CANDIDATE_DOCUMENTS
    FROM candidates
    GROUP BY amount_mode, window_days, LOAN_KEY
),
candidate_stats AS (
    SELECT
        amount_mode,
        window_days,
        COUNT(*) AS candidate_loan_document_matches,
        COUNT(DISTINCT LOAN_KEY) AS loans_with_any_candidate,
        COUNT(DISTINCT IFF(CANDIDATE_DOCUMENTS = 1, LOAN_KEY, NULL)) AS unique_accept_loans,
        COUNT(DISTINCT IFF(CANDIDATE_DOCUMENTS > 1, LOAN_KEY, NULL)) AS ambiguous_discard_loans
    FROM candidates
    JOIN loan_candidate_counts USING (amount_mode, window_days, LOAN_KEY)
    GROUP BY amount_mode, window_days
),
accepted AS (
    SELECT DISTINCT
        c.amount_mode,
        c.window_days,
        c.LOAN_KEY,
        c.DOCUMENT_ID
    FROM candidates c
    JOIN loan_candidate_counts lcc
      ON c.amount_mode = lcc.amount_mode
     AND c.window_days = lcc.window_days
     AND c.LOAN_KEY = lcc.LOAN_KEY
    WHERE lcc.CANDIDATE_DOCUMENTS = 1
),
accepted_bbls AS (
    SELECT DISTINCT
        a.amount_mode,
        a.window_days,
        a.LOAN_KEY,
        a.DOCUMENT_ID,
        COALESCE(NULLIF(REGEXP_REPLACE(TO_VARCHAR(l.BBL), '\\.0$', ''), ''),
                 TO_VARCHAR(l.BOROUGH) || LPAD(TO_VARCHAR(l.BLOCK), 5, '0') || LPAD(TO_VARCHAR(l.LOT), 4, '0')) AS ACRIS_BBL
    FROM accepted a
    JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT l
      ON a.DOCUMENT_ID = l.DOCUMENT_ID
    WHERE l.RELEASE_DT = '2026-08-10'
      AND l.BOROUGH IN (1,2,3,4,5)
      AND l.BLOCK IS NOT NULL
      AND l.LOT IS NOT NULL
),
accepted_bbl_counts AS (
    SELECT
        amount_mode,
        window_days,
        LOAN_KEY,
        COUNT(DISTINCT ACRIS_BBL) AS BBL_COUNT
    FROM accepted_bbls
    GROUP BY amount_mode, window_days, LOAN_KEY
),
bbl_stats AS (
    SELECT
        amount_mode,
        window_days,
        COUNT(DISTINCT LOAN_KEY) AS accepted_loans_with_bbl,
        COUNT(*) AS accepted_loan_bbl_edges,
        COUNT(DISTINCT IFF(BBL_COUNT = 1, LOAN_KEY, NULL)) AS accepted_loans_one_bbl,
        COUNT(DISTINCT IFF(BBL_COUNT > 1, LOAN_KEY, NULL)) AS accepted_loans_multi_bbl,
        MAX(BBL_COUNT) AS max_bbls
    FROM accepted_bbls
    JOIN accepted_bbl_counts USING (amount_mode, window_days, LOAN_KEY)
    GROUP BY amount_mode, window_days
)
SELECT
    mw.amount_mode,
    mw.window_days,
    (SELECT COUNT(*) FROM loan_scope) AS loan_denominator,
    COALESCE(cs.candidate_loan_document_matches, 0) AS candidate_loan_document_matches,
    COALESCE(cs.loans_with_any_candidate, 0) AS loans_with_any_candidate,
    COALESCE(cs.unique_accept_loans, 0) AS unique_accept_loans,
    COALESCE(cs.ambiguous_discard_loans, 0) AS ambiguous_discard_loans,
    (SELECT COUNT(*) FROM loan_scope) - COALESCE(cs.loans_with_any_candidate, 0) AS no_match_loans,
    COALESCE(bs.accepted_loans_with_bbl, 0) AS accepted_loans_with_bbl,
    COALESCE(bs.accepted_loan_bbl_edges, 0) AS accepted_loan_bbl_edges,
    COALESCE(bs.accepted_loans_one_bbl, 0) AS accepted_loans_one_bbl,
    COALESCE(bs.accepted_loans_multi_bbl, 0) AS accepted_loans_multi_bbl,
    bs.max_bbls
FROM mode_windows mw
LEFT JOIN candidate_stats cs
  ON mw.amount_mode = cs.amount_mode
 AND mw.window_days = cs.window_days
LEFT JOIN bbl_stats bs
  ON mw.amount_mode = bs.amount_mode
 AND mw.window_days = bs.window_days
ORDER BY mw.amount_mode, mw.window_days;
```

## G5: Precision Against ACRIS Truth

The selected truth set attaches back to the bd-14co baseline grains through
`PROPERTY_PERIOD_FACT` -> `PROPERTY_DIM` and exact geocode point equality, not
through address fields.

Coverage bridge:

| metric | loans | documents | bbls | rows_or_edges |
|---|---:|---:|---:|---:|
| accepted_truth | 523 | 447 | 1,350 | 1,695 |
| truth_property_bbl_edges | 523 | 447 | 1,350 | 2,293 |
| truth_property_geo_edges | 523 | 447 | 1,350 | 2,293 |
| baseline_points_denominator | n/a | n/a | n/a | 4,076 |
| baseline_address_keys_denominator | n/a | n/a | n/a | 5,269 |
| truth_points_exact | 523 | n/a | 1,350 | 582 |
| truth_points_round7 | 523 | n/a | 1,350 | 582 |
| truth_points_round6 | 523 | n/a | 1,350 | 582 |
| truth_address_keys_exact_point | 523 | n/a | 1,350 | 864 |

The exact, 7-decimal, and 6-decimal coordinate joins all return the same 582
truth-covered baseline points.

Address alignment caveat: `PROPERTY_DIM.CANONICAL_ADDRESS` has 585 accepted
truth address keys but only 268 exact overlaps with the wrangled geocode
address-key universe. For the bd-14co naive baseline, I therefore scored the
wrangled address-key baseline by attaching ACRIS truth to address keys through
the exact geocode point, not by joining `CANONICAL_ADDRESS` to
`PROPERTY_ADDRESS`.

| metric | address_keys | overlap_address_keys | edges |
|---|---:|---:|---:|
| truth_dim_address_keys | 585 | n/a | 2,293 |
| truth_dim_address_keys_overlap_wrgl | 585 | 268 | n/a |
| truth_dim_property_keys | 585 | n/a | 2,293 |
| truth_point_address_keys | 864 | n/a | 3,169 |

Coverage bridge source SQL:

```sql
WITH loan_scope AS (
    SELECT DISTINCT
        l.LOAN_KEY,
        l.CIK,
        l.ASSETNUMBER,
        CAST(l.ORIGINATIONDATE AS DATE) AS ORIGINATION_DATE,
        ROUND(l.ORIGINALLOANAMOUNT, 2) AS ORIGINAL_AMOUNT_CENTS
    FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p
    JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l
      ON p.CIK = l.CIK
     AND p.ASSETNUMBER = l.ASSETNUMBER
    WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND p.HAS_LOAN = TRUE
      AND l.ORIGINATIONDATE IS NOT NULL
      AND l.ORIGINALLOANAMOUNT IS NOT NULL
),
acris_docs AS (
    SELECT DISTINCT
        DOCUMENT_ID,
        CAST(RECORDED_DATETIME AS DATE) AS RECORDED_DATE,
        ROUND(DOCUMENT_AMT, 2) AS DOCUMENT_AMOUNT_CENTS
    FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT
    WHERE RELEASE_DT = '2026-08-10'
      AND RECORDED_BOROUGH IN (1,2,3,4,5)
      AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD')
      AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000
      AND CAST(RECORDED_DATETIME AS DATE) BETWEEN DATEADD(day, -30, (SELECT MIN(ORIGINATION_DATE) FROM loan_scope))
                                               AND DATEADD(day,  30, (SELECT MAX(ORIGINATION_DATE) FROM loan_scope))
),
candidates AS (
    SELECT
        l.LOAN_KEY,
        a.DOCUMENT_ID
    FROM loan_scope l
    JOIN acris_docs a
      ON a.DOCUMENT_AMOUNT_CENTS = l.ORIGINAL_AMOUNT_CENTS
     AND a.RECORDED_DATE BETWEEN DATEADD(day, -30, l.ORIGINATION_DATE)
                             AND DATEADD(day,  30, l.ORIGINATION_DATE)
),
loan_candidate_counts AS (
    SELECT
        LOAN_KEY,
        COUNT(DISTINCT DOCUMENT_ID) AS CANDIDATE_DOCUMENTS
    FROM candidates
    GROUP BY LOAN_KEY
),
accepted AS (
    SELECT DISTINCT
        c.LOAN_KEY,
        c.DOCUMENT_ID
    FROM candidates c
    JOIN loan_candidate_counts lcc
      ON c.LOAN_KEY = lcc.LOAN_KEY
    WHERE lcc.CANDIDATE_DOCUMENTS = 1
),
accepted_bbls AS (
    SELECT DISTINCT
        a.LOAN_KEY,
        a.DOCUMENT_ID,
        COALESCE(NULLIF(REGEXP_REPLACE(TO_VARCHAR(l.BBL), '\\.0$', ''), ''),
                 TO_VARCHAR(l.BOROUGH) || LPAD(TO_VARCHAR(l.BLOCK), 5, '0') || LPAD(TO_VARCHAR(l.LOT), 4, '0')) AS ACRIS_BBL
    FROM accepted a
    JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT l
      ON a.DOCUMENT_ID = l.DOCUMENT_ID
    WHERE l.RELEASE_DT = '2026-08-10'
      AND l.BOROUGH IN (1,2,3,4,5)
      AND l.BLOCK IS NOT NULL
      AND l.LOT IS NOT NULL
),
loan_property_scope AS (
    SELECT DISTINCT
        p.PROPERTY_KEY,
        l.LOAN_KEY
    FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p
    JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l
      ON p.CIK = l.CIK
     AND p.ASSETNUMBER = l.ASSETNUMBER
    WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND p.HAS_LOAN = TRUE
),
truth_property_bbl AS (
    SELECT DISTINCT
        ps.PROPERTY_KEY,
        ab.LOAN_KEY,
        ab.DOCUMENT_ID,
        ab.ACRIS_BBL
    FROM accepted_bbls ab
    JOIN loan_property_scope ps
      ON ab.LOAN_KEY = ps.LOAN_KEY
),
truth_property_geo AS (
    SELECT DISTINCT
        t.PROPERTY_KEY,
        t.LOAN_KEY,
        t.DOCUMENT_ID,
        t.ACRIS_BBL,
        d.LATITUDE,
        d.LONGITUDE,
        d.COUNTY_FIPS,
        d.ACCURACY_TYPE
    FROM truth_property_bbl t
    JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d
      ON t.PROPERTY_KEY = d.PROPERTY_KEY
    WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND d.LATITUDE IS NOT NULL
      AND d.LONGITUDE IS NOT NULL
),
scope_rows AS (
    SELECT *
    FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
    WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
),
points AS (
    SELECT
        LATITUDE,
        LONGITUDE,
        IFF(COUNT(DISTINCT ACCURACY_TYPE) = 1, MIN(ACCURACY_TYPE), 'mixed') AS POINT_ACCURACY_TYPE
    FROM scope_rows
    WHERE LATITUDE IS NOT NULL
      AND LONGITUDE IS NOT NULL
    GROUP BY LATITUDE, LONGITUDE
),
address_keys AS (
    SELECT
        PROPERTY_ADDRESS,
        COUNTY_FIPS,
        IFF(COUNT(DISTINCT ACCURACY_TYPE) = 1, MIN(ACCURACY_TYPE), 'mixed') AS ADDRESS_ACCURACY_TYPE
    FROM scope_rows
    GROUP BY PROPERTY_ADDRESS, COUNTY_FIPS
),
truth_point_exact AS (
    SELECT DISTINCT
        p.LATITUDE,
        p.LONGITUDE,
        g.LOAN_KEY,
        g.PROPERTY_KEY,
        g.ACRIS_BBL
    FROM points p
    JOIN truth_property_geo g
      ON p.LATITUDE = g.LATITUDE
     AND p.LONGITUDE = g.LONGITUDE
),
truth_point_round7 AS (
    SELECT DISTINCT
        p.LATITUDE,
        p.LONGITUDE,
        g.LOAN_KEY,
        g.PROPERTY_KEY,
        g.ACRIS_BBL
    FROM points p
    JOIN truth_property_geo g
      ON ROUND(p.LATITUDE, 7) = ROUND(g.LATITUDE, 7)
     AND ROUND(p.LONGITUDE, 7) = ROUND(g.LONGITUDE, 7)
),
truth_point_round6 AS (
    SELECT DISTINCT
        p.LATITUDE,
        p.LONGITUDE,
        g.LOAN_KEY,
        g.PROPERTY_KEY,
        g.ACRIS_BBL
    FROM points p
    JOIN truth_property_geo g
      ON ROUND(p.LATITUDE, 6) = ROUND(g.LATITUDE, 6)
     AND ROUND(p.LONGITUDE, 6) = ROUND(g.LONGITUDE, 6)
),
truth_address_keys AS (
    SELECT DISTINCT
        s.PROPERTY_ADDRESS,
        s.COUNTY_FIPS,
        tp.LOAN_KEY,
        tp.PROPERTY_KEY,
        tp.ACRIS_BBL
    FROM scope_rows s
    JOIN truth_point_exact tp
      ON s.LATITUDE = tp.LATITUDE
     AND s.LONGITUDE = tp.LONGITUDE
)
SELECT 'accepted_truth' AS metric,
       COUNT(DISTINCT ab.LOAN_KEY) AS loans,
       COUNT(DISTINCT ab.DOCUMENT_ID) AS documents,
       COUNT(DISTINCT ab.ACRIS_BBL) AS bbls,
       COUNT(*) AS rows_or_edges
FROM accepted_bbls ab
UNION ALL
SELECT 'truth_property_bbl_edges', COUNT(DISTINCT LOAN_KEY), COUNT(DISTINCT DOCUMENT_ID), COUNT(DISTINCT ACRIS_BBL), COUNT(*)
FROM truth_property_bbl
UNION ALL
SELECT 'truth_property_geo_edges', COUNT(DISTINCT LOAN_KEY), COUNT(DISTINCT DOCUMENT_ID), COUNT(DISTINCT ACRIS_BBL), COUNT(*)
FROM truth_property_geo
UNION ALL
SELECT 'baseline_points_denominator', NULL, NULL, NULL, COUNT(*) FROM points
UNION ALL
SELECT 'baseline_address_keys_denominator', NULL, NULL, NULL, COUNT(*) FROM address_keys
UNION ALL
SELECT 'truth_points_exact', COUNT(DISTINCT LOAN_KEY), NULL, COUNT(DISTINCT ACRIS_BBL), COUNT(DISTINCT TO_VARCHAR(LATITUDE) || '|' || TO_VARCHAR(LONGITUDE)) FROM truth_point_exact
UNION ALL
SELECT 'truth_points_round7', COUNT(DISTINCT LOAN_KEY), NULL, COUNT(DISTINCT ACRIS_BBL), COUNT(DISTINCT TO_VARCHAR(LATITUDE) || '|' || TO_VARCHAR(LONGITUDE)) FROM truth_point_round7
UNION ALL
SELECT 'truth_points_round6', COUNT(DISTINCT LOAN_KEY), NULL, COUNT(DISTINCT ACRIS_BBL), COUNT(DISTINCT TO_VARCHAR(LATITUDE) || '|' || TO_VARCHAR(LONGITUDE)) FROM truth_point_round6
UNION ALL
SELECT 'truth_address_keys_exact_point', COUNT(DISTINCT LOAN_KEY), NULL, COUNT(DISTINCT ACRIS_BBL), COUNT(DISTINCT COALESCE(PROPERTY_ADDRESS,'') || '|' || COALESCE(COUNTY_FIPS,'')) FROM truth_address_keys
ORDER BY metric;
```

Address-alignment source SQL:

```sql
WITH loan_scope AS (
    SELECT DISTINCT
        l.LOAN_KEY,
        l.CIK,
        l.ASSETNUMBER,
        CAST(l.ORIGINATIONDATE AS DATE) AS ORIGINATION_DATE,
        ROUND(l.ORIGINALLOANAMOUNT, 2) AS ORIGINAL_AMOUNT_CENTS
    FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p
    JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l
      ON p.CIK = l.CIK
     AND p.ASSETNUMBER = l.ASSETNUMBER
    WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND p.HAS_LOAN = TRUE
      AND l.ORIGINATIONDATE IS NOT NULL
      AND l.ORIGINALLOANAMOUNT IS NOT NULL
),
acris_docs AS (
    SELECT DISTINCT
        DOCUMENT_ID,
        CAST(RECORDED_DATETIME AS DATE) AS RECORDED_DATE,
        ROUND(DOCUMENT_AMT, 2) AS DOCUMENT_AMOUNT_CENTS
    FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT
    WHERE RELEASE_DT = '2026-08-10'
      AND RECORDED_BOROUGH IN (1,2,3,4,5)
      AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD')
      AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000
      AND CAST(RECORDED_DATETIME AS DATE) BETWEEN DATEADD(day, -30, (SELECT MIN(ORIGINATION_DATE) FROM loan_scope))
                                               AND DATEADD(day,  30, (SELECT MAX(ORIGINATION_DATE) FROM loan_scope))
),
candidates AS (
    SELECT l.LOAN_KEY, a.DOCUMENT_ID
    FROM loan_scope l
    JOIN acris_docs a
      ON a.DOCUMENT_AMOUNT_CENTS = l.ORIGINAL_AMOUNT_CENTS
     AND a.RECORDED_DATE BETWEEN DATEADD(day, -30, l.ORIGINATION_DATE)
                             AND DATEADD(day,  30, l.ORIGINATION_DATE)
),
loan_candidate_counts AS (
    SELECT LOAN_KEY, COUNT(DISTINCT DOCUMENT_ID) AS CANDIDATE_DOCUMENTS
    FROM candidates
    GROUP BY LOAN_KEY
),
accepted AS (
    SELECT DISTINCT c.LOAN_KEY, c.DOCUMENT_ID
    FROM candidates c
    JOIN loan_candidate_counts lcc
      ON c.LOAN_KEY = lcc.LOAN_KEY
    WHERE lcc.CANDIDATE_DOCUMENTS = 1
),
accepted_bbls AS (
    SELECT DISTINCT
        a.LOAN_KEY,
        a.DOCUMENT_ID,
        COALESCE(NULLIF(REGEXP_REPLACE(TO_VARCHAR(l.BBL), '\\.0$', ''), ''),
                 TO_VARCHAR(l.BOROUGH) || LPAD(TO_VARCHAR(l.BLOCK), 5, '0') || LPAD(TO_VARCHAR(l.LOT), 4, '0')) AS ACRIS_BBL
    FROM accepted a
    JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT l
      ON a.DOCUMENT_ID = l.DOCUMENT_ID
    WHERE l.RELEASE_DT = '2026-08-10'
      AND l.BOROUGH IN (1,2,3,4,5)
      AND l.BLOCK IS NOT NULL
      AND l.LOT IS NOT NULL
),
loan_property_scope AS (
    SELECT DISTINCT p.PROPERTY_KEY, l.LOAN_KEY
    FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p
    JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l
      ON p.CIK = l.CIK
     AND p.ASSETNUMBER = l.ASSETNUMBER
    WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND p.HAS_LOAN = TRUE
),
truth_property AS (
    SELECT DISTINCT
        ps.PROPERTY_KEY,
        ab.LOAN_KEY,
        ab.DOCUMENT_ID,
        ab.ACRIS_BBL
    FROM accepted_bbls ab
    JOIN loan_property_scope ps
      ON ab.LOAN_KEY = ps.LOAN_KEY
),
truth_dim AS (
    SELECT DISTINCT
        t.PROPERTY_KEY,
        t.LOAN_KEY,
        t.DOCUMENT_ID,
        t.ACRIS_BBL,
        d.CANONICAL_ADDRESS,
        d.COUNTY_FIPS,
        d.LATITUDE,
        d.LONGITUDE,
        d.ACCURACY_TYPE
    FROM truth_property t
    JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d
      ON t.PROPERTY_KEY = d.PROPERTY_KEY
    WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND d.LATITUDE IS NOT NULL
      AND d.LONGITUDE IS NOT NULL
),
scope_rows AS (
    SELECT *
    FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
    WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
),
wrgl_address_keys AS (
    SELECT DISTINCT PROPERTY_ADDRESS, COUNTY_FIPS
    FROM scope_rows
),
truth_dim_address_keys AS (
    SELECT DISTINCT CANONICAL_ADDRESS AS PROPERTY_ADDRESS, COUNTY_FIPS
    FROM truth_dim
    WHERE CANONICAL_ADDRESS IS NOT NULL
),
truth_dim_address_key_edges AS (
    SELECT DISTINCT CANONICAL_ADDRESS AS PROPERTY_ADDRESS, COUNTY_FIPS, LOAN_KEY, PROPERTY_KEY, ACRIS_BBL
    FROM truth_dim
    WHERE CANONICAL_ADDRESS IS NOT NULL
),
truth_point_address_key_edges AS (
    SELECT DISTINCT s.PROPERTY_ADDRESS, s.COUNTY_FIPS, td.LOAN_KEY, td.PROPERTY_KEY, td.ACRIS_BBL
    FROM scope_rows s
    JOIN truth_dim td
      ON s.LATITUDE = td.LATITUDE
     AND s.LONGITUDE = td.LONGITUDE
)
SELECT 'truth_dim_address_keys' AS metric,
       COUNT(DISTINCT PROPERTY_ADDRESS || '|' || COUNTY_FIPS) AS address_keys,
       NULL AS overlap_address_keys,
       COUNT(*) AS edges
FROM truth_dim_address_key_edges
UNION ALL
SELECT 'truth_dim_address_keys_overlap_wrgl',
       COUNT(DISTINCT t.PROPERTY_ADDRESS || '|' || t.COUNTY_FIPS),
       COUNT(DISTINCT w.PROPERTY_ADDRESS || '|' || w.COUNTY_FIPS),
       NULL
FROM truth_dim_address_keys t
LEFT JOIN wrgl_address_keys w
  ON t.PROPERTY_ADDRESS = w.PROPERTY_ADDRESS
 AND t.COUNTY_FIPS = w.COUNTY_FIPS
UNION ALL
SELECT 'truth_point_address_keys',
       COUNT(DISTINCT PROPERTY_ADDRESS || '|' || COUNTY_FIPS),
       NULL,
       COUNT(*)
FROM truth_point_address_key_edges
UNION ALL
SELECT 'truth_dim_property_keys',
       COUNT(DISTINCT PROPERTY_KEY),
       NULL,
       COUNT(*)
FROM truth_dim
ORDER BY metric;
```

### Geometry-Only PIP Precision

Grain: distinct `(LATITUDE, LONGITUDE)` baseline points. Prediction is the
current MapPLUTO `ST_CONTAINS` BBL. Correct means at least one predicted BBL is
in the ACRIS BBL set attached to that baseline point.

| accuracy_type | universe_points | truth_points | truth_coverage | PIP covered on truth | coverage on truth | correct | precision | multi-predict | truth BBL edges |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ALL | 4,076 | 582 | 14.28% | 563 | 96.74% | 166 | 29.48% | 0 | 2,238 |
| intersection | 19 | 1 | 5.26% | 0 | 0.00% | 0 | n/a | 0 | 2 |
| mixed | 87 | 9 | 10.34% | 9 | 100.00% | 4 | 44.44% | 0 | 36 |
| nearest_rooftop_match | 344 | 69 | 20.06% | 69 | 100.00% | 16 | 23.19% | 0 | 620 |
| place | 30 | 5 | 16.67% | 3 | 60.00% | 0 | 0.00% | 0 | 7 |
| range_interpolation | 315 | 28 | 8.89% | 15 | 53.57% | 2 | 13.33% | 0 | 224 |
| rooftop | 3,216 | 465 | 14.46% | 465 | 100.00% | 144 | 30.97% | 0 | 1,329 |
| street_center | 65 | 5 | 7.69% | 2 | 40.00% | 0 | 0.00% | 0 | 20 |

The nearest-rooftop slice is the silent-error population: geometry covers all 69
ACRIS-covered nearest-rooftop points, but only 16 are correct against ACRIS.
That is 23.19% precision on covered units.

### Naive Address-String Precision

Grain: distinct `(PROPERTY_ADDRESS, COUNTY_FIPS)` baseline address keys.
Prediction is the bd-14co dumb exact address normalization against MapPLUTO
`ADDRESS`, scoped by borough. Correct means at least one predicted BBL is in the
ACRIS BBL set attached to that address key by exact geocode point.

| accuracy_type | universe_keys | truth_keys | truth_coverage | naive covered on truth | coverage on truth | correct | precision | multi-predict | truth BBL edges |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ALL | 5,269 | 864 | 16.40% | 286 | 33.10% | 67 | 23.43% | 0 | 3,061 |
| intersection | 20 | 1 | 5.00% | 0 | 0.00% | 0 | n/a | 0 | 2 |
| mixed | 47 | 10 | 21.28% | 3 | 30.00% | 1 | 33.33% | 0 | 160 |
| nearest_rooftop_match | 593 | 120 | 20.24% | 1 | 0.83% | 1 | 100.00% | 0 | 885 |
| place | 43 | 15 | 34.88% | 0 | 0.00% | 0 | n/a | 0 | 24 |
| range_interpolation | 340 | 32 | 9.41% | 9 | 28.13% | 2 | 22.22% | 0 | 84 |
| rooftop | 4,160 | 681 | 16.37% | 273 | 40.09% | 63 | 23.08% | 0 | 1,874 |
| street_center | 66 | 5 | 7.58% | 0 | 0.00% | 0 | n/a | 0 | 32 |

The nearest-rooftop address-key row has 100% precision only because it fires on
one ACRIS-covered key. Its coverage on the nearest-rooftop truth slice is 1 / 120
= 0.83%.

### Representation Diagnostic: Condo Unit Lots Vs Billing Lots

MapPLUTO `26v1` does not expose condo metadata fields in the landed table. The
schema check found only the following condo/BBL/block/lot-related columns:

| column_name | data_type | ordinal_position |
|---|---|---:|
| BOROUGH_PARTITION | TEXT | 10 |
| BOROUGH | TEXT | 12 |
| BLOCK | TEXT | 13 |
| LOT | TEXT | 14 |
| BBL | TEXT | 15 |
| LOTAREA | NUMBER | 20 |

```sql
SELECT
    column_name,
    data_type,
    ordinal_position
FROM EDGAR_DB.INFORMATION_SCHEMA.COLUMNS
WHERE table_schema = 'SOURCE'
  AND table_name = 'NYC_DCP_MAPPLUTO_HOT'
  AND (
      column_name ILIKE '%CONDO%'
      OR column_name ILIKE '%BBL%'
      OR column_name ILIKE '%BORO%'
      OR column_name ILIKE '%BLOCK%'
      OR column_name ILIKE '%LOT%'
  )
ORDER BY ordinal_position;
```

Therefore the condo signature used below is a heuristic over ACRIS legals:
`TRY_TO_NUMBER(LOT) BETWEEN 1001 AND 6999`.

Block grade is a diagnostic upper bound: a covered unit is block-correct when a
predicted BBL's `borough+block` matches at least one ACRIS truth BBL's
`borough+block`. The original lot-grade scoring is still the strict score; the
block-grade score asks how much of the strict miss population could be
parent/unit or same-building representation mismatch.

#### Geometry PIP: Block Diagnostic By Tier

| accuracy_type | truth_units | covered | lot_correct | lot_precision | block_correct | block_upper | block-match lot-mismatch | full-block mismatch |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| ALL | 582 | 563 | 166 | 29.48% | 193 | 34.28% | 27 | 370 |
| intersection | 1 | 0 | 0 | n/a | 0 | n/a | 0 | 0 |
| mixed | 9 | 9 | 4 | 44.44% | 4 | 44.44% | 0 | 5 |
| nearest_rooftop_match | 69 | 69 | 16 | 23.19% | 17 | 24.64% | 1 | 52 |
| place | 5 | 3 | 0 | 0.00% | 0 | 0.00% | 0 | 3 |
| range_interpolation | 28 | 15 | 2 | 13.33% | 3 | 20.00% | 1 | 12 |
| rooftop | 465 | 465 | 144 | 30.97% | 169 | 36.34% | 25 | 296 |
| street_center | 5 | 2 | 0 | 0.00% | 0 | 0.00% | 0 | 2 |

Geometry PIP condo split:

| accuracy_type | truth_class | truth_units | covered | lot_correct | lot_precision | block_correct | block_upper | block-match lot-mismatch | full-block mismatch |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| ALL | condo_signature | 123 | 115 | 9 | 7.83% | 35 | 30.43% | 26 | 80 |
| ALL | non_condo_signature | 459 | 448 | 157 | 35.04% | 158 | 35.27% | 1 | 290 |
| nearest_rooftop_match | condo_signature | 14 | 14 | 0 | 0.00% | 1 | 7.14% | 1 | 13 |
| nearest_rooftop_match | non_condo_signature | 55 | 55 | 16 | 29.09% | 16 | 29.09% | 0 | 39 |

Interpretation: same-block lot mismatch is real in the condo-signature subset,
but it does not explain the headline. For PIP, 397 covered units were incorrect
at lot grade; only 27 of those were block-match/lot-mismatch. The remaining 370
were full-block mismatches. Nearest-rooftop is even sharper: 53 covered misses,
1 block-match/lot-mismatch, 52 full-block mismatches.

#### Naive Address-Key: Block Diagnostic By Tier

| accuracy_type | truth_units | covered | lot_correct | lot_precision | block_correct | block_upper | block-match lot-mismatch | full-block mismatch |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| ALL | 864 | 286 | 67 | 23.43% | 82 | 28.67% | 15 | 204 |
| intersection | 1 | 0 | 0 | n/a | 0 | n/a | 0 | 0 |
| mixed | 10 | 3 | 1 | 33.33% | 1 | 33.33% | 0 | 2 |
| nearest_rooftop_match | 120 | 1 | 1 | 100.00% | 1 | 100.00% | 0 | 0 |
| place | 15 | 0 | 0 | n/a | 0 | n/a | 0 | 0 |
| range_interpolation | 32 | 9 | 2 | 22.22% | 3 | 33.33% | 1 | 6 |
| rooftop | 681 | 273 | 63 | 23.08% | 77 | 28.21% | 14 | 196 |
| street_center | 5 | 0 | 0 | n/a | 0 | n/a | 0 | 0 |

Naive address-key condo split:

| accuracy_type | truth_class | truth_units | covered | lot_correct | lot_precision | block_correct | block_upper | block-match lot-mismatch | full-block mismatch |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| ALL | condo_signature | 187 | 59 | 7 | 11.86% | 20 | 33.90% | 13 | 39 |
| ALL | non_condo_signature | 677 | 227 | 60 | 26.43% | 62 | 27.31% | 2 | 165 |
| nearest_rooftop_match | condo_signature | 25 | 1 | 1 | 100.00% | 1 | 100.00% | 0 | 0 |
| nearest_rooftop_match | non_condo_signature | 95 | 0 | 0 | n/a | 0 | n/a | 0 | 0 |

For the address baseline, 219 covered units were incorrect at lot grade; 15 were
block-match/lot-mismatch and 204 were full-block mismatches.

#### Geometry Diagnostic SQL

```sql
WITH ls AS (
 SELECT DISTINCT l.LOAN_KEY k, CAST(l.ORIGINATIONDATE AS DATE) od, ROUND(l.ORIGINALLOANAMOUNT,2) amt
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE AND l.ORIGINATIONDATE IS NOT NULL AND l.ORIGINALLOANAMOUNT IS NOT NULL
), ad AS (
 SELECT DISTINCT DOCUMENT_ID doc, CAST(RECORDED_DATETIME AS DATE) rd, ROUND(DOCUMENT_AMT,2) amt
 FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT
 WHERE RELEASE_DT='2026-08-10' AND RECORDED_BOROUGH IN (1,2,3,4,5) AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD') AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000
   AND CAST(RECORDED_DATETIME AS DATE) BETWEEN DATEADD(day,-30,(SELECT MIN(od) FROM ls)) AND DATEADD(day,30,(SELECT MAX(od) FROM ls))
), ca AS (
 SELECT k,doc FROM ls JOIN ad ON ad.amt=ls.amt AND ad.rd BETWEEN DATEADD(day,-30,ls.od) AND DATEADD(day,30,ls.od)
), cn AS (SELECT k,COUNT(DISTINCT doc) n FROM ca GROUP BY k), ac AS (SELECT ca.k,ca.doc FROM ca JOIN cn USING(k) WHERE n=1),
ab AS (
 SELECT DISTINCT ac.k, ac.doc, TO_VARCHAR(l.BOROUGH)||LPAD(TO_VARCHAR(l.BLOCK),5,'0')||LPAD(TO_VARCHAR(l.LOT),4,'0') bbl, TO_VARCHAR(l.BOROUGH)||LPAD(TO_VARCHAR(l.BLOCK),5,'0') blk, IFF(TRY_TO_NUMBER(l.LOT) BETWEEN 1001 AND 6999,1,0) condo
 FROM ac JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT l ON ac.doc=l.DOCUMENT_ID
 WHERE l.RELEASE_DT='2026-08-10' AND l.BOROUGH IN (1,2,3,4,5) AND l.BLOCK IS NOT NULL AND l.LOT IS NOT NULL
), ps AS (
 SELECT DISTINCT p.PROPERTY_KEY pk,l.LOAN_KEY k FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE
), td AS (
 SELECT DISTINCT ps.pk,ab.k,ab.bbl,ab.blk,ab.condo,d.LATITUDE lat,d.LONGITUDE lon
 FROM ab JOIN ps USING(k) JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d ON ps.pk=d.PROPERTY_KEY
 WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND d.LATITUDE IS NOT NULL AND d.LONGITUDE IS NOT NULL
), r AS (
 SELECT * FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
), pts AS (
 SELECT LATITUDE lat,LONGITUDE lon,IFF(COUNT(DISTINCT ACCURACY_TYPE)=1,MIN(ACCURACY_TYPE),'mixed') acc FROM r WHERE LATITUDE IS NOT NULL AND LONGITUDE IS NOT NULL GROUP BY LATITUDE,LONGITUDE
), u AS (SELECT acc,COUNT(*) universe FROM pts GROUP BY acc UNION ALL SELECT 'ALL',COUNT(*) FROM pts),
tb AS (SELECT DISTINCT pts.lat,pts.lon,pts.acc,td.bbl,td.blk,td.condo FROM pts JOIN td ON pts.lat=td.lat AND pts.lon=td.lon),
pe AS (SELECT DISTINCT pts.lat,pts.lon,REGEXP_REPLACE(TO_VARCHAR(p.BBL),'\\.0$','') pbbl,SUBSTR(REGEXP_REPLACE(TO_VARCHAR(p.BBL),'\\.0$',''),1,6) pblk FROM pts JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p ON ST_CONTAINS(p.GEOM_GEOG,ST_POINT(pts.lon,pts.lat))),
e AS (
 SELECT tb.lat,tb.lon,tb.acc,IFF(MAX(tb.condo)=1,'condo_signature','non_condo_signature') cls,COUNT(DISTINCT tb.bbl) truth_bbls,COUNT(DISTINCT tb.blk) truth_blocks,COUNT(DISTINCT pe.pbbl) pred_bbls,COUNT(DISTINCT IFF(pe.pbbl=tb.bbl,pe.pbbl,NULL)) lot_ok,COUNT(DISTINCT IFF(pe.pblk=tb.blk,pe.pblk,NULL)) block_ok
 FROM tb LEFT JOIN pe ON tb.lat=pe.lat AND tb.lon=pe.lon GROUP BY tb.lat,tb.lon,tb.acc
), ag AS (
 SELECT IFF(GROUPING(acc)=1,'ALL',acc) acc,IFF(GROUPING(cls)=1,'ALL_TRUTH_CLASSES',cls) cls,COUNT(*) truth_units,SUM(IFF(pred_bbls>0,1,0)) covered,SUM(IFF(pred_bbls>0 AND lot_ok>0,1,0)) lot_correct,SUM(IFF(pred_bbls>0 AND block_ok>0,1,0)) block_correct,SUM(IFF(pred_bbls>0 AND lot_ok=0 AND block_ok>0,1,0)) block_match_lot_mismatch,SUM(IFF(pred_bbls>0 AND lot_ok=0 AND block_ok=0,1,0)) full_block_mismatch,SUM(IFF(pred_bbls>1,1,0)) multi_predict,SUM(truth_bbls) truth_bbl_edges,SUM(truth_blocks) truth_block_edges
 FROM e GROUP BY GROUPING SETS ((acc,cls),(acc),(cls),())
)
SELECT 'geometry_pip_point' baseline,ag.acc accuracy_type,ag.cls truth_class,u.universe universe_units,truth_units,ROUND(truth_units/NULLIF(u.universe,0)*100,2) truth_coverage_pct,covered,ROUND(covered/NULLIF(truth_units,0)*100,2) coverage_on_truth_pct,lot_correct,ROUND(lot_correct/NULLIF(covered,0)*100,2) lot_precision_pct,block_correct,ROUND(block_correct/NULLIF(covered,0)*100,2) block_precision_upper_pct,block_match_lot_mismatch,full_block_mismatch,multi_predict,truth_bbl_edges,truth_block_edges
FROM ag JOIN u ON ag.acc=u.acc ORDER BY IFF(ag.acc='ALL',0,1),ag.acc,IFF(ag.cls='ALL_TRUTH_CLASSES',0,IFF(ag.cls='condo_signature',1,2));
```

#### Naive Address Diagnostic SQL

```sql
WITH ls AS (
 SELECT DISTINCT l.LOAN_KEY k, CAST(l.ORIGINATIONDATE AS DATE) od, ROUND(l.ORIGINALLOANAMOUNT,2) amt
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE AND l.ORIGINATIONDATE IS NOT NULL AND l.ORIGINALLOANAMOUNT IS NOT NULL
), ad AS (
 SELECT DISTINCT DOCUMENT_ID doc, CAST(RECORDED_DATETIME AS DATE) rd, ROUND(DOCUMENT_AMT,2) amt
 FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT
 WHERE RELEASE_DT='2026-08-10' AND RECORDED_BOROUGH IN (1,2,3,4,5) AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD') AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000
   AND CAST(RECORDED_DATETIME AS DATE) BETWEEN DATEADD(day,-30,(SELECT MIN(od) FROM ls)) AND DATEADD(day,30,(SELECT MAX(od) FROM ls))
), ca AS (
 SELECT k,doc FROM ls JOIN ad ON ad.amt=ls.amt AND ad.rd BETWEEN DATEADD(day,-30,ls.od) AND DATEADD(day,30,ls.od)
), cn AS (SELECT k,COUNT(DISTINCT doc) n FROM ca GROUP BY k), ac AS (SELECT ca.k,ca.doc FROM ca JOIN cn USING(k) WHERE n=1),
ab AS (
 SELECT DISTINCT ac.k, ac.doc, TO_VARCHAR(l.BOROUGH)||LPAD(TO_VARCHAR(l.BLOCK),5,'0')||LPAD(TO_VARCHAR(l.LOT),4,'0') bbl, TO_VARCHAR(l.BOROUGH)||LPAD(TO_VARCHAR(l.BLOCK),5,'0') blk, IFF(TRY_TO_NUMBER(l.LOT) BETWEEN 1001 AND 6999,1,0) condo
 FROM ac JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT l ON ac.doc=l.DOCUMENT_ID
 WHERE l.RELEASE_DT='2026-08-10' AND l.BOROUGH IN (1,2,3,4,5) AND l.BLOCK IS NOT NULL AND l.LOT IS NOT NULL
), ps AS (
 SELECT DISTINCT p.PROPERTY_KEY pk,l.LOAN_KEY k FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE
), td AS (
 SELECT DISTINCT ps.pk,ab.k,ab.bbl,ab.blk,ab.condo,d.LATITUDE lat,d.LONGITUDE lon
 FROM ab JOIN ps USING(k) JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d ON ps.pk=d.PROPERTY_KEY
 WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND d.LATITUDE IS NOT NULL AND d.LONGITUDE IS NOT NULL
), r AS (
 SELECT * FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
), ak AS (
 SELECT PROPERTY_ADDRESS addr,COUNTY_FIPS fips,CASE COUNTY_FIPS WHEN '36005' THEN 'BX' WHEN '36047' THEN 'BK' WHEN '36061' THEN 'MN' WHEN '36081' THEN 'QN' WHEN '36085' THEN 'SI' END bor,IFF(COUNT(DISTINCT ACCURACY_TYPE)=1,MIN(ACCURACY_TYPE),'mixed') acc,REGEXP_REPLACE(UPPER(TRIM(PROPERTY_ADDRESS)),'[^A-Z0-9]','') norm
 FROM r GROUP BY PROPERTY_ADDRESS,COUNTY_FIPS
), u AS (SELECT acc,COUNT(*) universe FROM ak GROUP BY acc UNION ALL SELECT 'ALL',COUNT(*) FROM ak),
tb AS (
 SELECT DISTINCT ak.addr,ak.fips,ak.acc,td.bbl,td.blk,td.condo FROM ak JOIN r ON ak.addr=r.PROPERTY_ADDRESS AND ak.fips=r.COUNTY_FIPS JOIN td ON r.LATITUDE=td.lat AND r.LONGITUDE=td.lon
), pn AS (
 SELECT DISTINCT BOROUGH bor,REGEXP_REPLACE(TO_VARCHAR(BBL),'\\.0$','') pbbl,SUBSTR(REGEXP_REPLACE(TO_VARCHAR(BBL),'\\.0$',''),1,6) pblk,REGEXP_REPLACE(UPPER(TRIM(ADDRESS)),'[^A-Z0-9]','') norm FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
), pe AS (
 SELECT DISTINCT ak.addr,ak.fips,pn.pbbl,pn.pblk FROM ak JOIN pn ON ak.bor=pn.bor AND ak.norm=pn.norm
), e AS (
 SELECT tb.addr,tb.fips,tb.acc,IFF(MAX(tb.condo)=1,'condo_signature','non_condo_signature') cls,COUNT(DISTINCT tb.bbl) truth_bbls,COUNT(DISTINCT tb.blk) truth_blocks,COUNT(DISTINCT pe.pbbl) pred_bbls,COUNT(DISTINCT IFF(pe.pbbl=tb.bbl,pe.pbbl,NULL)) lot_ok,COUNT(DISTINCT IFF(pe.pblk=tb.blk,pe.pblk,NULL)) block_ok
 FROM tb LEFT JOIN pe ON tb.addr=pe.addr AND tb.fips=pe.fips GROUP BY tb.addr,tb.fips,tb.acc
), ag AS (
 SELECT IFF(GROUPING(acc)=1,'ALL',acc) acc,IFF(GROUPING(cls)=1,'ALL_TRUTH_CLASSES',cls) cls,COUNT(*) truth_units,SUM(IFF(pred_bbls>0,1,0)) covered,SUM(IFF(pred_bbls>0 AND lot_ok>0,1,0)) lot_correct,SUM(IFF(pred_bbls>0 AND block_ok>0,1,0)) block_correct,SUM(IFF(pred_bbls>0 AND lot_ok=0 AND block_ok>0,1,0)) block_match_lot_mismatch,SUM(IFF(pred_bbls>0 AND lot_ok=0 AND block_ok=0,1,0)) full_block_mismatch,SUM(IFF(pred_bbls>1,1,0)) multi_predict,SUM(truth_bbls) truth_bbl_edges,SUM(truth_blocks) truth_block_edges
 FROM e GROUP BY GROUPING SETS ((acc,cls),(acc),(cls),())
)
SELECT 'naive_address_key' baseline,ag.acc accuracy_type,ag.cls truth_class,u.universe universe_units,truth_units,ROUND(truth_units/NULLIF(u.universe,0)*100,2) truth_coverage_pct,covered,ROUND(covered/NULLIF(truth_units,0)*100,2) coverage_on_truth_pct,lot_correct,ROUND(lot_correct/NULLIF(covered,0)*100,2) lot_precision_pct,block_correct,ROUND(block_correct/NULLIF(covered,0)*100,2) block_precision_upper_pct,block_match_lot_mismatch,full_block_mismatch,multi_predict,truth_bbl_edges,truth_block_edges
FROM ag JOIN u ON ag.acc=u.acc ORDER BY IFF(ag.acc='ALL',0,1),ag.acc,IFF(ag.cls='ALL_TRUTH_CLASSES',0,IFF(ag.cls='condo_signature',1,2));
```

### G5 Source SQL

```sql
WITH loan_scope AS (
    SELECT DISTINCT
        l.LOAN_KEY,
        l.CIK,
        l.ASSETNUMBER,
        CAST(l.ORIGINATIONDATE AS DATE) AS ORIGINATION_DATE,
        ROUND(l.ORIGINALLOANAMOUNT, 2) AS ORIGINAL_AMOUNT_CENTS
    FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p
    JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l
      ON p.CIK = l.CIK
     AND p.ASSETNUMBER = l.ASSETNUMBER
    WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND p.HAS_LOAN = TRUE
      AND l.ORIGINATIONDATE IS NOT NULL
      AND l.ORIGINALLOANAMOUNT IS NOT NULL
),
acris_docs AS (
    SELECT DISTINCT
        DOCUMENT_ID,
        CAST(RECORDED_DATETIME AS DATE) AS RECORDED_DATE,
        ROUND(DOCUMENT_AMT, 2) AS DOCUMENT_AMOUNT_CENTS
    FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT
    WHERE RELEASE_DT = '2026-08-10'
      AND RECORDED_BOROUGH IN (1,2,3,4,5)
      AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD')
      AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000
      AND CAST(RECORDED_DATETIME AS DATE) BETWEEN DATEADD(day, -30, (SELECT MIN(ORIGINATION_DATE) FROM loan_scope))
                                               AND DATEADD(day,  30, (SELECT MAX(ORIGINATION_DATE) FROM loan_scope))
),
candidates AS (
    SELECT
        l.LOAN_KEY,
        a.DOCUMENT_ID
    FROM loan_scope l
    JOIN acris_docs a
      ON a.DOCUMENT_AMOUNT_CENTS = l.ORIGINAL_AMOUNT_CENTS
     AND a.RECORDED_DATE BETWEEN DATEADD(day, -30, l.ORIGINATION_DATE)
                             AND DATEADD(day,  30, l.ORIGINATION_DATE)
),
loan_candidate_counts AS (
    SELECT
        LOAN_KEY,
        COUNT(DISTINCT DOCUMENT_ID) AS CANDIDATE_DOCUMENTS
    FROM candidates
    GROUP BY LOAN_KEY
),
accepted AS (
    SELECT DISTINCT
        c.LOAN_KEY,
        c.DOCUMENT_ID
    FROM candidates c
    JOIN loan_candidate_counts lcc
      ON c.LOAN_KEY = lcc.LOAN_KEY
    WHERE lcc.CANDIDATE_DOCUMENTS = 1
),
accepted_bbls AS (
    SELECT DISTINCT
        a.LOAN_KEY,
        a.DOCUMENT_ID,
        COALESCE(NULLIF(REGEXP_REPLACE(TO_VARCHAR(l.BBL), '\\.0$', ''), ''),
                 TO_VARCHAR(l.BOROUGH) || LPAD(TO_VARCHAR(l.BLOCK), 5, '0') || LPAD(TO_VARCHAR(l.LOT), 4, '0')) AS ACRIS_BBL
    FROM accepted a
    JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT l
      ON a.DOCUMENT_ID = l.DOCUMENT_ID
    WHERE l.RELEASE_DT = '2026-08-10'
      AND l.BOROUGH IN (1,2,3,4,5)
      AND l.BLOCK IS NOT NULL
      AND l.LOT IS NOT NULL
),
loan_property_scope AS (
    SELECT DISTINCT
        p.PROPERTY_KEY,
        l.LOAN_KEY
    FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p
    JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l
      ON p.CIK = l.CIK
     AND p.ASSETNUMBER = l.ASSETNUMBER
    WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND p.HAS_LOAN = TRUE
),
truth_property_bbl AS (
    SELECT DISTINCT
        ps.PROPERTY_KEY,
        ab.LOAN_KEY,
        ab.DOCUMENT_ID,
        ab.ACRIS_BBL
    FROM accepted_bbls ab
    JOIN loan_property_scope ps
      ON ab.LOAN_KEY = ps.LOAN_KEY
),
truth_dim AS (
    SELECT DISTINCT
        t.PROPERTY_KEY,
        t.LOAN_KEY,
        t.DOCUMENT_ID,
        t.ACRIS_BBL,
        d.LATITUDE,
        d.LONGITUDE,
        d.COUNTY_FIPS,
        d.ACCURACY_TYPE
    FROM truth_property_bbl t
    JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d
      ON t.PROPERTY_KEY = d.PROPERTY_KEY
    WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND d.LATITUDE IS NOT NULL
      AND d.LONGITUDE IS NOT NULL
),
scope_rows AS (
    SELECT *
    FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
    WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
),
points AS (
    SELECT
        LATITUDE,
        LONGITUDE,
        IFF(COUNT(DISTINCT ACCURACY_TYPE) = 1, MIN(ACCURACY_TYPE), 'mixed') AS ACCURACY_TYPE
    FROM scope_rows
    WHERE LATITUDE IS NOT NULL
      AND LONGITUDE IS NOT NULL
    GROUP BY LATITUDE, LONGITUDE
),
point_universe AS (
    SELECT ACCURACY_TYPE, COUNT(*) AS UNIVERSE_UNITS
    FROM points
    GROUP BY ACCURACY_TYPE
    UNION ALL
    SELECT 'ALL', COUNT(*) FROM points
),
truth_point_bbls AS (
    SELECT DISTINCT
        p.LATITUDE,
        p.LONGITUDE,
        p.ACCURACY_TYPE,
        td.ACRIS_BBL
    FROM points p
    JOIN truth_dim td
      ON p.LATITUDE = td.LATITUDE
     AND p.LONGITUDE = td.LONGITUDE
),
pip_edges AS (
    SELECT DISTINCT
        p.LATITUDE,
        p.LONGITUDE,
        REGEXP_REPLACE(TO_VARCHAR(pl.BBL), '\\.0$', '') AS PREDICTED_BBL
    FROM points p
    JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl
      ON ST_CONTAINS(pl.GEOM_GEOG, ST_POINT(p.LONGITUDE, p.LATITUDE))
),
pip_point_eval AS (
    SELECT
        tp.LATITUDE,
        tp.LONGITUDE,
        tp.ACCURACY_TYPE,
        COUNT(DISTINCT tp.ACRIS_BBL) AS TRUTH_BBLS,
        COUNT(DISTINCT pe.PREDICTED_BBL) AS PREDICTED_BBLS,
        COUNT(DISTINCT IFF(pe.PREDICTED_BBL = tp.ACRIS_BBL, pe.PREDICTED_BBL, NULL)) AS CORRECT_BBLS
    FROM truth_point_bbls tp
    LEFT JOIN pip_edges pe
      ON tp.LATITUDE = pe.LATITUDE
     AND tp.LONGITUDE = pe.LONGITUDE
    GROUP BY tp.LATITUDE, tp.LONGITUDE, tp.ACCURACY_TYPE
),
pip_tier AS (
    SELECT
        'geometry_pip_point' AS BASELINE,
        ACCURACY_TYPE,
        COUNT(*) AS TRUTH_UNITS,
        SUM(IFF(PREDICTED_BBLS > 0, 1, 0)) AS BASELINE_COVERED_UNITS,
        SUM(IFF(PREDICTED_BBLS > 0 AND CORRECT_BBLS > 0, 1, 0)) AS CORRECT_UNITS,
        SUM(IFF(PREDICTED_BBLS > 1, 1, 0)) AS MULTI_PREDICT_UNITS,
        SUM(TRUTH_BBLS) AS TRUTH_BBL_EDGES
    FROM pip_point_eval
    GROUP BY ACCURACY_TYPE
    UNION ALL
    SELECT
        'geometry_pip_point',
        'ALL',
        COUNT(*),
        SUM(IFF(PREDICTED_BBLS > 0, 1, 0)),
        SUM(IFF(PREDICTED_BBLS > 0 AND CORRECT_BBLS > 0, 1, 0)),
        SUM(IFF(PREDICTED_BBLS > 1, 1, 0)),
        SUM(TRUTH_BBLS)
    FROM pip_point_eval
),
address_keys AS (
    SELECT
        PROPERTY_ADDRESS,
        COUNTY_FIPS,
        CASE COUNTY_FIPS
            WHEN '36005' THEN 'BX'
            WHEN '36047' THEN 'BK'
            WHEN '36061' THEN 'MN'
            WHEN '36081' THEN 'QN'
            WHEN '36085' THEN 'SI'
        END AS BOROUGH,
        IFF(COUNT(DISTINCT ACCURACY_TYPE) = 1, MIN(ACCURACY_TYPE), 'mixed') AS ACCURACY_TYPE,
        REGEXP_REPLACE(UPPER(TRIM(PROPERTY_ADDRESS)), '[^A-Z0-9]', '') AS NORM_ADDR
    FROM scope_rows
    GROUP BY PROPERTY_ADDRESS, COUNTY_FIPS
),
address_universe AS (
    SELECT ACCURACY_TYPE, COUNT(*) AS UNIVERSE_UNITS
    FROM address_keys
    GROUP BY ACCURACY_TYPE
    UNION ALL
    SELECT 'ALL', COUNT(*) FROM address_keys
),
truth_address_bbls AS (
    SELECT DISTINCT
        ak.PROPERTY_ADDRESS,
        ak.COUNTY_FIPS,
        ak.ACCURACY_TYPE,
        td.ACRIS_BBL
    FROM address_keys ak
    JOIN scope_rows s
      ON ak.PROPERTY_ADDRESS = s.PROPERTY_ADDRESS
     AND ak.COUNTY_FIPS = s.COUNTY_FIPS
    JOIN truth_dim td
      ON s.LATITUDE = td.LATITUDE
     AND s.LONGITUDE = td.LONGITUDE
),
pluto_address_norm AS (
    SELECT DISTINCT
        BOROUGH,
        REGEXP_REPLACE(TO_VARCHAR(BBL), '\\.0$', '') AS PREDICTED_BBL,
        REGEXP_REPLACE(UPPER(TRIM(ADDRESS)), '[^A-Z0-9]', '') AS NORM_ADDR
    FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
),
address_match_edges AS (
    SELECT DISTINCT
        ak.PROPERTY_ADDRESS,
        ak.COUNTY_FIPS,
        p.PREDICTED_BBL
    FROM address_keys ak
    JOIN pluto_address_norm p
      ON p.BOROUGH = ak.BOROUGH
     AND p.NORM_ADDR = ak.NORM_ADDR
),
address_eval AS (
    SELECT
        ta.PROPERTY_ADDRESS,
        ta.COUNTY_FIPS,
        ta.ACCURACY_TYPE,
        COUNT(DISTINCT ta.ACRIS_BBL) AS TRUTH_BBLS,
        COUNT(DISTINCT ame.PREDICTED_BBL) AS PREDICTED_BBLS,
        COUNT(DISTINCT IFF(ame.PREDICTED_BBL = ta.ACRIS_BBL, ame.PREDICTED_BBL, NULL)) AS CORRECT_BBLS
    FROM truth_address_bbls ta
    LEFT JOIN address_match_edges ame
      ON ta.PROPERTY_ADDRESS = ame.PROPERTY_ADDRESS
     AND ta.COUNTY_FIPS = ame.COUNTY_FIPS
    GROUP BY ta.PROPERTY_ADDRESS, ta.COUNTY_FIPS, ta.ACCURACY_TYPE
),
address_tier AS (
    SELECT
        'naive_address_key' AS BASELINE,
        ACCURACY_TYPE,
        COUNT(*) AS TRUTH_UNITS,
        SUM(IFF(PREDICTED_BBLS > 0, 1, 0)) AS BASELINE_COVERED_UNITS,
        SUM(IFF(PREDICTED_BBLS > 0 AND CORRECT_BBLS > 0, 1, 0)) AS CORRECT_UNITS,
        SUM(IFF(PREDICTED_BBLS > 1, 1, 0)) AS MULTI_PREDICT_UNITS,
        SUM(TRUTH_BBLS) AS TRUTH_BBL_EDGES
    FROM address_eval
    GROUP BY ACCURACY_TYPE
    UNION ALL
    SELECT
        'naive_address_key',
        'ALL',
        COUNT(*),
        SUM(IFF(PREDICTED_BBLS > 0, 1, 0)),
        SUM(IFF(PREDICTED_BBLS > 0 AND CORRECT_BBLS > 0, 1, 0)),
        SUM(IFF(PREDICTED_BBLS > 1, 1, 0)),
        SUM(TRUTH_BBLS)
    FROM address_eval
),
combined AS (
    SELECT * FROM pip_tier
    UNION ALL
    SELECT * FROM address_tier
),
universes AS (
    SELECT 'geometry_pip_point' AS BASELINE, ACCURACY_TYPE, UNIVERSE_UNITS FROM point_universe
    UNION ALL
    SELECT 'naive_address_key', ACCURACY_TYPE, UNIVERSE_UNITS FROM address_universe
)
SELECT
    c.BASELINE,
    c.ACCURACY_TYPE,
    u.UNIVERSE_UNITS,
    c.TRUTH_UNITS,
    ROUND(c.TRUTH_UNITS / NULLIF(u.UNIVERSE_UNITS, 0) * 100, 2) AS TRUTH_COVERAGE_PCT,
    c.BASELINE_COVERED_UNITS,
    ROUND(c.BASELINE_COVERED_UNITS / NULLIF(c.TRUTH_UNITS, 0) * 100, 2) AS BASELINE_COVERAGE_ON_TRUTH_PCT,
    c.CORRECT_UNITS,
    ROUND(c.CORRECT_UNITS / NULLIF(c.BASELINE_COVERED_UNITS, 0) * 100, 2) AS PRECISION_ON_COVERED_PCT,
    c.MULTI_PREDICT_UNITS,
    c.TRUTH_BBL_EDGES
FROM combined c
JOIN universes u
  ON c.BASELINE = u.BASELINE
 AND c.ACCURACY_TYPE = u.ACCURACY_TYPE
ORDER BY c.BASELINE, IFF(c.ACCURACY_TYPE = 'ALL', 0, 1), c.ACCURACY_TYPE;
```

## G6 Stretch: Invisible Assemblage

ACRIS multi-BBL accepts are document-asserted collateral sets. The stretch probe
compares accepted loan BBL cardinality against how many MapPLUTO lots the
geocode point itself PIPs into.

At loan grain, 125 multi-BBL accepted loans have all attached property keys PIP
to exactly one lot. Before the condo decomposition these looked like invisible
assemblages to the geocode/PIP baseline. After the decomposition, 46 of the 125
are condo-signature by the ACRIS unit-lot heuristic, and 79 are non-condo
multi-parcel collateral candidates.

| grain | ACRIS class | PIP class | units | distinct loans |
|---|---|---|---:|---:|
| loan_grain | acris_multi_bbl | all_properties_pip_one | 125 | 125 |
| loan_grain | acris_multi_bbl | all_properties_pip_zero | 2 | 2 |
| loan_grain | acris_multi_bbl | mixed_pip_one_zero | 4 | 4 |
| loan_grain | acris_one_bbl | all_properties_pip_one | 379 | 379 |
| loan_grain | acris_one_bbl | all_properties_pip_zero | 8 | 8 |
| loan_grain | acris_one_bbl | mixed_pip_one_zero | 5 | 5 |
| property_key_grain | acris_multi_bbl | pip_one_lot | 183 | 129 |
| property_key_grain | acris_multi_bbl | pip_zero_lots | 7 | 6 |
| property_key_grain | acris_one_bbl | pip_one_lot | 487 | 384 |
| property_key_grain | acris_one_bbl | pip_zero_lots | 13 | 13 |

G6 condo split:

| ACRIS class | PIP class | condo class | block class | loans | sum ACRIS BBLs | max ACRIS BBLs | property-key edges |
|---|---|---|---|---:|---:|---:|---:|
| acris_multi_bbl | all_properties_pip_one | condo_signature | multi_acris_block | 5 | 27 | 8 | 12 |
| acris_multi_bbl | all_properties_pip_one | condo_signature | one_acris_block | 41 | 756 | 172 | 48 |
| acris_multi_bbl | all_properties_pip_one | non_condo_signature | multi_acris_block | 24 | 165 | 56 | 55 |
| acris_multi_bbl | all_properties_pip_one | non_condo_signature | one_acris_block | 55 | 194 | 38 | 62 |
| acris_multi_bbl | all_properties_pip_zero | condo_signature | one_acris_block | 2 | 151 | 149 | 2 |
| acris_multi_bbl | mixed_pip_one_zero | condo_signature | one_acris_block | 2 | 6 | 4 | 6 |
| acris_multi_bbl | mixed_pip_one_zero | non_condo_signature | one_acris_block | 2 | 4 | 2 | 5 |
| acris_one_bbl | all_properties_pip_one | condo_signature | one_acris_block | 46 | 46 | 1 | 57 |
| acris_one_bbl | all_properties_pip_one | non_condo_signature | one_acris_block | 333 | 333 | 1 | 422 |
| acris_one_bbl | all_properties_pip_zero | condo_signature | one_acris_block | 1 | 1 | 1 | 1 |
| acris_one_bbl | all_properties_pip_zero | non_condo_signature | one_acris_block | 7 | 7 | 1 | 7 |
| acris_one_bbl | mixed_pip_one_zero | condo_signature | one_acris_block | 2 | 2 | 1 | 4 |
| acris_one_bbl | mixed_pip_one_zero | non_condo_signature | one_acris_block | 3 | 3 | 1 | 9 |

Summary for the original 125 `acris_multi_bbl/all_properties_pip_one` loans:
46 are condo-signature and 79 are non-condo. Of the 79 non-condo cases, 24 span
multiple ACRIS blocks and 55 are same-block multi-lot collateral. Across all 131
multi-BBL accepted loans, 50 are condo-signature and 81 are non-condo.

G6 condo split SQL:

```sql
WITH ls AS (
 SELECT DISTINCT l.LOAN_KEY k, CAST(l.ORIGINATIONDATE AS DATE) od, ROUND(l.ORIGINALLOANAMOUNT,2) amt
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE AND l.ORIGINATIONDATE IS NOT NULL AND l.ORIGINALLOANAMOUNT IS NOT NULL
), ad AS (
 SELECT DISTINCT DOCUMENT_ID doc, CAST(RECORDED_DATETIME AS DATE) rd, ROUND(DOCUMENT_AMT,2) amt
 FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT
 WHERE RELEASE_DT='2026-08-10' AND RECORDED_BOROUGH IN (1,2,3,4,5) AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD') AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000
   AND CAST(RECORDED_DATETIME AS DATE) BETWEEN DATEADD(day,-30,(SELECT MIN(od) FROM ls)) AND DATEADD(day,30,(SELECT MAX(od) FROM ls))
), ca AS (
 SELECT k,doc FROM ls JOIN ad ON ad.amt=ls.amt AND ad.rd BETWEEN DATEADD(day,-30,ls.od) AND DATEADD(day,30,ls.od)
), cn AS (SELECT k,COUNT(DISTINCT doc) n FROM ca GROUP BY k), ac AS (SELECT ca.k,ca.doc FROM ca JOIN cn USING(k) WHERE n=1),
ab AS (
 SELECT DISTINCT ac.k,ac.doc,TO_VARCHAR(l.BOROUGH)||LPAD(TO_VARCHAR(l.BLOCK),5,'0')||LPAD(TO_VARCHAR(l.LOT),4,'0') bbl,TO_VARCHAR(l.BOROUGH)||LPAD(TO_VARCHAR(l.BLOCK),5,'0') blk,IFF(TRY_TO_NUMBER(l.LOT) BETWEEN 1001 AND 6999,1,0) condo
 FROM ac JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT l ON ac.doc=l.DOCUMENT_ID
 WHERE l.RELEASE_DT='2026-08-10' AND l.BOROUGH IN (1,2,3,4,5) AND l.BLOCK IS NOT NULL AND l.LOT IS NOT NULL
), lc AS (
 SELECT k,COUNT(DISTINCT bbl) acris_bbls,COUNT(DISTINCT blk) acris_blocks,MAX(condo) condo_sig FROM ab GROUP BY k
), ps AS (
 SELECT DISTINCT p.PROPERTY_KEY pk,l.LOAN_KEY k FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE
), gp AS (
 SELECT DISTINCT ps.pk,ps.k,lc.acris_bbls,lc.acris_blocks,lc.condo_sig,d.LATITUDE lat,d.LONGITUDE lon
 FROM ps JOIN lc USING(k) JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d ON ps.pk=d.PROPERTY_KEY
 WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND d.LATITUDE IS NOT NULL AND d.LONGITUDE IS NOT NULL
), pp AS (
 SELECT gp.pk,gp.k,gp.acris_bbls,gp.acris_blocks,gp.condo_sig,COUNT(DISTINCT REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$','')) pip_lots
 FROM gp LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON ST_CONTAINS(pl.GEOM_GEOG,ST_POINT(gp.lon,gp.lat))
 GROUP BY gp.pk,gp.k,gp.acris_bbls,gp.acris_blocks,gp.condo_sig
), lr AS (
 SELECT k,MAX(acris_bbls) acris_bbls,MAX(acris_blocks) acris_blocks,MAX(condo_sig) condo_sig,COUNT(DISTINCT pk) property_keys,SUM(IFF(pip_lots=0,1,0)) pk_pip_zero,SUM(IFF(pip_lots=1,1,0)) pk_pip_one,SUM(IFF(pip_lots>1,1,0)) pk_pip_multi
 FROM pp GROUP BY k
)
SELECT IFF(acris_bbls=1,'acris_one_bbl','acris_multi_bbl') acris_class,
       CASE WHEN pk_pip_multi>0 THEN 'any_property_pip_multi' WHEN pk_pip_one>0 AND pk_pip_zero=0 THEN 'all_properties_pip_one' WHEN pk_pip_one>0 AND pk_pip_zero>0 THEN 'mixed_pip_one_zero' ELSE 'all_properties_pip_zero' END pip_class,
       IFF(condo_sig=1,'condo_signature','non_condo_signature') condo_class,
       CASE WHEN acris_blocks=1 THEN 'one_acris_block' ELSE 'multi_acris_block' END block_class,
       COUNT(*) loans,
       SUM(acris_bbls) sum_acris_bbls,
       MAX(acris_bbls) max_acris_bbls,
       SUM(property_keys) property_key_edges
FROM lr
GROUP BY 1,2,3,4
ORDER BY acris_class,pip_class,condo_class,block_class;
```

```sql
WITH loan_scope AS (
    SELECT DISTINCT
        l.LOAN_KEY,
        l.CIK,
        l.ASSETNUMBER,
        CAST(l.ORIGINATIONDATE AS DATE) AS ORIGINATION_DATE,
        ROUND(l.ORIGINALLOANAMOUNT, 2) AS ORIGINAL_AMOUNT_CENTS
    FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p
    JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l
      ON p.CIK = l.CIK
     AND p.ASSETNUMBER = l.ASSETNUMBER
    WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND p.HAS_LOAN = TRUE
      AND l.ORIGINATIONDATE IS NOT NULL
      AND l.ORIGINALLOANAMOUNT IS NOT NULL
),
acris_docs AS (
    SELECT DISTINCT
        DOCUMENT_ID,
        CAST(RECORDED_DATETIME AS DATE) AS RECORDED_DATE,
        ROUND(DOCUMENT_AMT, 2) AS DOCUMENT_AMOUNT_CENTS
    FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT
    WHERE RELEASE_DT = '2026-08-10'
      AND RECORDED_BOROUGH IN (1,2,3,4,5)
      AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD')
      AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000
      AND CAST(RECORDED_DATETIME AS DATE) BETWEEN DATEADD(day, -30, (SELECT MIN(ORIGINATION_DATE) FROM loan_scope))
                                               AND DATEADD(day,  30, (SELECT MAX(ORIGINATION_DATE) FROM loan_scope))
),
candidates AS (
    SELECT l.LOAN_KEY, a.DOCUMENT_ID
    FROM loan_scope l
    JOIN acris_docs a
      ON a.DOCUMENT_AMOUNT_CENTS = l.ORIGINAL_AMOUNT_CENTS
     AND a.RECORDED_DATE BETWEEN DATEADD(day, -30, l.ORIGINATION_DATE)
                             AND DATEADD(day,  30, l.ORIGINATION_DATE)
),
loan_candidate_counts AS (
    SELECT LOAN_KEY, COUNT(DISTINCT DOCUMENT_ID) AS CANDIDATE_DOCUMENTS
    FROM candidates
    GROUP BY LOAN_KEY
),
accepted AS (
    SELECT DISTINCT c.LOAN_KEY, c.DOCUMENT_ID
    FROM candidates c
    JOIN loan_candidate_counts lcc
      ON c.LOAN_KEY = lcc.LOAN_KEY
    WHERE lcc.CANDIDATE_DOCUMENTS = 1
),
accepted_bbls AS (
    SELECT DISTINCT
        a.LOAN_KEY,
        a.DOCUMENT_ID,
        COALESCE(NULLIF(REGEXP_REPLACE(TO_VARCHAR(l.BBL), '\\.0$', ''), ''),
                 TO_VARCHAR(l.BOROUGH) || LPAD(TO_VARCHAR(l.BLOCK), 5, '0') || LPAD(TO_VARCHAR(l.LOT), 4, '0')) AS ACRIS_BBL
    FROM accepted a
    JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT l
      ON a.DOCUMENT_ID = l.DOCUMENT_ID
    WHERE l.RELEASE_DT = '2026-08-10'
      AND l.BOROUGH IN (1,2,3,4,5)
      AND l.BLOCK IS NOT NULL
      AND l.LOT IS NOT NULL
),
accepted_loan_bbl_counts AS (
    SELECT
        LOAN_KEY,
        COUNT(DISTINCT ACRIS_BBL) AS ACRIS_BBLS
    FROM accepted_bbls
    GROUP BY LOAN_KEY
),
loan_property_scope AS (
    SELECT DISTINCT
        p.PROPERTY_KEY,
        l.LOAN_KEY
    FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p
    JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l
      ON p.CIK = l.CIK
     AND p.ASSETNUMBER = l.ASSETNUMBER
    WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND p.HAS_LOAN = TRUE
),
truth_properties AS (
    SELECT DISTINCT
        ps.PROPERTY_KEY,
        c.LOAN_KEY,
        c.ACRIS_BBLS
    FROM accepted_loan_bbl_counts c
    JOIN loan_property_scope ps
      ON c.LOAN_KEY = ps.LOAN_KEY
),
truth_property_geo AS (
    SELECT DISTINCT
        tp.PROPERTY_KEY,
        tp.LOAN_KEY,
        tp.ACRIS_BBLS,
        d.LATITUDE,
        d.LONGITUDE,
        d.ACCURACY_TYPE
    FROM truth_properties tp
    JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d
      ON tp.PROPERTY_KEY = d.PROPERTY_KEY
    WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND d.LATITUDE IS NOT NULL
      AND d.LONGITUDE IS NOT NULL
),
property_pip AS (
    SELECT
        g.PROPERTY_KEY,
        g.LOAN_KEY,
        g.ACRIS_BBLS,
        COUNT(DISTINCT REGEXP_REPLACE(TO_VARCHAR(pl.BBL), '\\.0$', '')) AS PIP_LOTS
    FROM truth_property_geo g
    LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl
      ON ST_CONTAINS(pl.GEOM_GEOG, ST_POINT(g.LONGITUDE, g.LATITUDE))
    GROUP BY g.PROPERTY_KEY, g.LOAN_KEY, g.ACRIS_BBLS
),
loan_pip_rollup AS (
    SELECT
        LOAN_KEY,
        MAX(ACRIS_BBLS) AS ACRIS_BBLS,
        COUNT(DISTINCT PROPERTY_KEY) AS PROPERTY_KEYS,
        SUM(IFF(PIP_LOTS = 0, 1, 0)) AS PROPERTY_KEYS_PIP_ZERO,
        SUM(IFF(PIP_LOTS = 1, 1, 0)) AS PROPERTY_KEYS_PIP_ONE,
        SUM(IFF(PIP_LOTS > 1, 1, 0)) AS PROPERTY_KEYS_PIP_MULTI
    FROM property_pip
    GROUP BY LOAN_KEY
)
SELECT 'property_key_grain' AS GRAIN,
       IFF(ACRIS_BBLS = 1, 'acris_one_bbl', 'acris_multi_bbl') AS ACRIS_CLASS,
       CASE WHEN PIP_LOTS = 0 THEN 'pip_zero_lots'
            WHEN PIP_LOTS = 1 THEN 'pip_one_lot'
            ELSE 'pip_multi_lot' END AS PIP_CLASS,
       COUNT(*) AS UNITS,
       COUNT(DISTINCT LOAN_KEY) AS DISTINCT_LOANS
FROM property_pip
GROUP BY 1,2,3
UNION ALL
SELECT 'loan_grain',
       IFF(ACRIS_BBLS = 1, 'acris_one_bbl', 'acris_multi_bbl') AS ACRIS_CLASS,
       CASE WHEN PROPERTY_KEYS_PIP_MULTI > 0 THEN 'any_property_pip_multi'
            WHEN PROPERTY_KEYS_PIP_ONE > 0 AND PROPERTY_KEYS_PIP_ZERO = 0 THEN 'all_properties_pip_one'
            WHEN PROPERTY_KEYS_PIP_ONE > 0 AND PROPERTY_KEYS_PIP_ZERO > 0 THEN 'mixed_pip_one_zero'
            ELSE 'all_properties_pip_zero' END AS PIP_CLASS,
       COUNT(*) AS UNITS,
       COUNT(DISTINCT LOAN_KEY) AS DISTINCT_LOANS
FROM loan_pip_rollup
GROUP BY 1,2,3
ORDER BY GRAIN, ACRIS_CLASS, PIP_CLASS;
```

## Negative Results And Corrections

- ACRIS base-table sweeps returned zero rows because the landing is external
  tables. This was a discovery result, not absence of ACRIS.
- No ACRIS HOT/materialized tables were found; all ACRIS work above reads the
  external tables directly.
- A first bridge query used an unqualified `CIK` in a multi-join select and
  failed with `SQL compilation error: ambiguous column name 'CIK'`.
- A first sensitivity aggregation joined candidate stats to accepted BBL edges
  by `(amount_mode, window_days)` and inflated candidate sums through fan-out.
  The corrected sensitivity query separates `candidate_stats` from `bbl_stats`;
  only the corrected table above is cited.
- A first combined representation-diagnostic query for both baselines exceeded
  Loom's 10,000-character message limit before execution. I split it into the
  successful geometry, address, and G6 queries recorded above.
