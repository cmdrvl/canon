# bd-179b ACRIS Ground Truth

Date: 2026-08-16

Agent: PearlSparrow

Original G1-G6 scope: five-borough CMBS geocode scope from
`EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED` with
`COUNTY_FIPS in ('36005','36047','36061','36081','36085')`. Controlling G7
scope: the raw filed-county universe declared in G7.1 and G7.6.

Data access discipline: every cited number below came from
`cmdrvl orchestrator query --tenant salt --timeout 300 --raw` and returned
`tool_responses[*].structuredContent`. Loom prose is not cited.

## Status

**Controlling update, 2026-08-28:** G7 supersedes Gate V2 for truth admission. A
provenance audit found that the earlier bridge's `RECORDED_BOROUGH` was derived
from geocoded `COUNTY_FIPS`, so the earlier claim of a wholly
address-channel-independent borough gate was too strong. G7 rebuilds the gate
from raw filed `PROPERTYCOUNTY`, adds the lender/party discriminator as a
separate truth plane, and keeps reach separate from precision. The earlier
sections remain as historical measurements and sensitivity evidence; they are
not release truth claims.

Historical G1-G6 status: ACRIS Source 1 is usable as a small
address-independent foothold, but the final
contamination probe shows the exact-cents +/-30 unique gate is not clean enough
for a headline precision estimate across the full accepted set. The operating
gate is exact cents on origination amount and a +/-30 day recording-date window,
with uniqueness required per CMBS loan and ambiguous loans discarded.

Raw result before contamination decomposition:

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

Final contamination probe: the full-block mismatch population has the signature
of unique-but-wrong amount/date matches. At accepted-loan grain, geometry PIP
covered 513 / 523 loans and scored 135 / 513 = 26.32% lot-grade precision.
Correct PIP hits had 0 negative recording offsets; full-block mismatches had
165 / 356 negative offsets. ACRIS legal borough disagreed with every property
borough in 203 full-block mismatches, and both recorded and legal borough
disagreed in 113. Non-round amounts scored 66 / 119 = 55.46%, while 100k/1m
round amounts scored 69 / 394 = 17.51% and 1m multiples alone scored 19 / 241
= 7.88%. Treat the raw 29.48% point-grain precision headline as contaminated,
not a clean baseline-failure estimate.

Gate V2 refined result: exact cents, `[0,+45]` recording offset, legal-borough
agreement, and non-round amount only. It accepts 166 loans from the same 3,040
loan denominator; 48 non-round loans are ambiguous after filters, 451 non-round
loans have no match, and 2,375 round-amount loans are explicitly excluded.
Against this refined truth set, geometry PIP scores 154 / 233 = 66.09% lot grade
and 169 / 233 = 72.53% block grade, with 242 / 4,076 = 5.94% truth coverage.
The naive address-key baseline scores 63 / 93 = 67.74% lot grade and 71 / 93 =
76.34% block grade, with 328 / 5,269 = 6.23% truth coverage and only 28.35%
coverage on those truth keys. Nearest-rooftop geometry PIP scores 15 / 29 =
51.72% lot grade and 18 / 29 = 62.07% block grade; the address baseline fires
on 0 / 44 nearest-rooftop truth keys.

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

## Contamination Probe: Unique Amount-Date Matches

This probe answers the final fork for the 370 point-grain full-block
mismatches: real baseline failure versus truth-set contamination from a
unique-but-wrong amount/date match. The diagnostic is at accepted-loan grain over
the 523 ACRIS accepts, not the earlier point grain. Geometry PIP is scored as:
correct if any MapPLUTO BBL predicted for any attached property point equals any
ACRIS BBL accepted for the loan; block-match/lot-mismatch if no lot matches but
at least one predicted `borough+block` matches; full-block mismatch otherwise.

Accepted-loan geometry PIP denominator:

| accepted loans | PIP covered | lot correct | lot precision | block correct | block upper | block-match lot-mismatch | full-block mismatch | no PIP prediction |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 523 | 513 | 135 | 26.32% | 157 | 30.60% | 22 | 356 | 10 |

### 1. Recording-Offset Distribution

`offset_days = RECORDED_DATE - ORIGINATIONDATE`. The correct population clusters
on non-negative offsets only. The full-block mismatch population spans the full
+/-30-day window, including 165 negative offsets, which is the expected signature
of amount/date collision contamination.

Summary:

| scored_outcome | scored_detail | loans | negative offsets | zero offsets | positive offsets | min offset | max offset | avg offset | median offset |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| correct | lot_correct | 135 | 0 | 0 | 135 | 2 | 30 | 13.06 | 12.000 |
| incorrect | block_match_lot_mismatch | 22 | 0 | 1 | 21 | 0 | 30 | 11.82 | 12.000 |
| incorrect | full_block_mismatch | 356 | 165 | 8 | 183 | -30 | 30 | 0.42 | 1.500 |
| no_pip_prediction | no_pip_prediction | 10 | 3 | 0 | 7 | -25 | 23 | 4.00 | 6.500 |

Histogram:

| scored_detail | bucket | loans | negative offsets | zero offsets | positive offsets |
|---|---:|---:|---:|---:|---:|
| lot_correct | 1..7 | 26 | 0 | 0 | 26 |
| lot_correct | 8..14 | 62 | 0 | 0 | 62 |
| lot_correct | 15..30 | 47 | 0 | 0 | 47 |
| block_match_lot_mismatch | 0 | 1 | 0 | 1 | 0 |
| block_match_lot_mismatch | 1..7 | 4 | 0 | 0 | 4 |
| block_match_lot_mismatch | 8..14 | 10 | 0 | 0 | 10 |
| block_match_lot_mismatch | 15..30 | 7 | 0 | 0 | 7 |
| full_block_mismatch | -30..-15 | 98 | 98 | 0 | 0 |
| full_block_mismatch | -14..-8 | 38 | 38 | 0 | 0 |
| full_block_mismatch | -7..-1 | 29 | 29 | 0 | 0 |
| full_block_mismatch | 0 | 8 | 0 | 8 | 0 |
| full_block_mismatch | 1..7 | 44 | 0 | 0 | 44 |
| full_block_mismatch | 8..14 | 39 | 0 | 0 | 39 |
| full_block_mismatch | 15..30 | 100 | 0 | 0 | 100 |
| no_pip_prediction | -30..-15 | 1 | 1 | 0 | 0 |
| no_pip_prediction | -14..-8 | 2 | 2 | 0 | 0 |
| no_pip_prediction | 1..7 | 2 | 0 | 0 | 2 |
| no_pip_prediction | 8..14 | 2 | 0 | 0 | 2 |
| no_pip_prediction | 15..30 | 3 | 0 | 0 | 3 |

```sql
WITH ls AS (
 SELECT DISTINCT l.LOAN_KEY k,CAST(l.ORIGINATIONDATE AS DATE) od,ROUND(l.ORIGINALLOANAMOUNT,2) amt
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE AND l.ORIGINATIONDATE IS NOT NULL AND l.ORIGINALLOANAMOUNT IS NOT NULL
), ad AS (
 SELECT DISTINCT DOCUMENT_ID doc,RECORDED_BOROUGH rb,CAST(RECORDED_DATETIME AS DATE) rd,ROUND(DOCUMENT_AMT,2) amt
 FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT
 WHERE RELEASE_DT='2026-08-10' AND RECORDED_BOROUGH IN (1,2,3,4,5) AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD') AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000
   AND CAST(RECORDED_DATETIME AS DATE) BETWEEN DATEADD(day,-30,(SELECT MIN(od) FROM ls)) AND DATEADD(day,30,(SELECT MAX(od) FROM ls))
), ca AS (
 SELECT ls.k,ad.doc,ad.rb,ad.rd,ls.od,ls.amt,DATEDIFF(day,ls.od,ad.rd) off
 FROM ls JOIN ad ON ad.amt=ls.amt AND ad.rd BETWEEN DATEADD(day,-30,ls.od) AND DATEADD(day,30,ls.od)
), cn AS (SELECT k,COUNT(DISTINCT doc)n FROM ca GROUP BY k), ac AS (SELECT ca.* FROM ca JOIN cn USING(k) WHERE n=1),
ab AS (
 SELECT DISTINCT ac.k,TO_VARCHAR(l.BOROUGH)||LPAD(TO_VARCHAR(l.BLOCK),5,'0')||LPAD(TO_VARCHAR(l.LOT),4,'0') bbl,TO_VARCHAR(l.BOROUGH)||LPAD(TO_VARCHAR(l.BLOCK),5,'0') blk,l.BOROUGH lb
 FROM ac JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT l ON ac.doc=l.DOCUMENT_ID
 WHERE l.RELEASE_DT='2026-08-10' AND l.BOROUGH IN (1,2,3,4,5) AND l.BLOCK IS NOT NULL AND l.LOT IS NOT NULL
), ps AS (
 SELECT DISTINCT p.PROPERTY_KEY pk,l.LOAN_KEY k,CASE TO_VARCHAR(p.COUNTY_FIPS) WHEN '36061' THEN 1 WHEN '36005' THEN 2 WHEN '36047' THEN 3 WHEN '36081' THEN 4 WHEN '36085' THEN 5 END pb
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE
), gp AS (
 SELECT DISTINCT ps.k,ps.pk,d.LATITUDE lat,d.LONGITUDE lon
 FROM ps JOIN ac USING(k) JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d ON ps.pk=d.PROPERTY_KEY
 WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND d.LATITUDE IS NOT NULL AND d.LONGITUDE IS NOT NULL
), pe AS (
 SELECT DISTINCT gp.k,REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$','') pbbl,SUBSTR(REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$',''),1,6) pblk
 FROM gp JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON ST_CONTAINS(pl.GEOM_GEOG,ST_POINT(gp.lon,gp.lat))
), lo AS (
 SELECT ac.k,ac.doc,ac.rb,ac.rd,ac.od,ac.amt,ac.off,COUNT(DISTINCT pe.pbbl) pred_bbls,COUNT(DISTINCT IFF(pe.pbbl=ab.bbl,pe.pbbl,NULL)) lot_hits,COUNT(DISTINCT IFF(pe.pblk=ab.blk,pe.pblk,NULL)) block_hits
 FROM ac JOIN ab USING(k) LEFT JOIN pe USING(k)
 GROUP BY ac.k,ac.doc,ac.rb,ac.rd,ac.od,ac.amt,ac.off
), o AS (
 SELECT *,CASE WHEN pred_bbls=0 THEN 'no_pip_prediction' WHEN lot_hits>0 THEN 'correct' ELSE 'incorrect' END scored_outcome,
          CASE WHEN pred_bbls=0 THEN 'no_pip_prediction' WHEN lot_hits>0 THEN 'lot_correct' WHEN block_hits>0 THEN 'block_match_lot_mismatch' ELSE 'full_block_mismatch' END scored_detail,
          CASE WHEN off BETWEEN -30 AND -15 THEN '-30..-15' WHEN off BETWEEN -14 AND -8 THEN '-14..-8' WHEN off BETWEEN -7 AND -1 THEN '-7..-1' WHEN off=0 THEN '0' WHEN off BETWEEN 1 AND 7 THEN '1..7' WHEN off BETWEEN 8 AND 14 THEN '8..14' WHEN off BETWEEN 15 AND 30 THEN '15..30' ELSE 'outside' END bucket,
          CASE WHEN off BETWEEN -30 AND -15 THEN 1 WHEN off BETWEEN -14 AND -8 THEN 2 WHEN off BETWEEN -7 AND -1 THEN 3 WHEN off=0 THEN 4 WHEN off BETWEEN 1 AND 7 THEN 5 WHEN off BETWEEN 8 AND 14 THEN 6 WHEN off BETWEEN 15 AND 30 THEN 7 ELSE 8 END bucket_ord
 FROM lo
)
SELECT 'summary' row_type,scored_outcome,scored_detail,NULL bucket,COUNT(*) loans,SUM(IFF(off<0,1,0)) negative_offsets,SUM(IFF(off=0,1,0)) zero_offsets,SUM(IFF(off>0,1,0)) positive_offsets,MIN(off) min_offset,MAX(off) max_offset,ROUND(AVG(off),2) avg_offset,MEDIAN(off) median_offset
FROM o GROUP BY scored_outcome,scored_detail
UNION ALL
SELECT 'histogram',scored_outcome,scored_detail,bucket,COUNT(*),SUM(IFF(off<0,1,0)),SUM(IFF(off=0,1,0)),SUM(IFF(off>0,1,0)),MIN(off),MAX(off),ROUND(AVG(off),2),MEDIAN(off)
FROM o GROUP BY scored_outcome,scored_detail,bucket,bucket_ord
ORDER BY scored_outcome,scored_detail,row_type DESC,bucket;
```

### 2. Borough Agreement

`RECORDED_BOROUGH` is not clean enough by itself: 68 lot-correct accepts have a
recorded-borough mismatch while their ACRIS legal borough agrees. The stronger
contamination indicator is therefore legal-borough disagreement, with the strict
near-certain bucket requiring both recorded and legal borough to disagree with
every property county attached to the CMBS loan.

| scored_outcome | scored_detail | loans | recorded borough agrees any property | recorded borough disagrees all properties | legal borough agrees any property | legal borough disagrees all properties | recorded and legal disagree all properties |
|---|---|---:|---:|---:|---:|---:|---:|
| correct | lot_correct | 135 | 67 | 68 | 135 | 0 | 0 |
| incorrect | block_match_lot_mismatch | 22 | 14 | 8 | 22 | 0 | 0 |
| incorrect | full_block_mismatch | 356 | 205 | 151 | 153 | 203 | 113 |
| no_pip_prediction | no_pip_prediction | 10 | 4 | 6 | 5 | 5 | 2 |

The strict near-certain contamination bucket is 113 full-block mismatches, with
0 lot-correct and 0 same-block/lot-mismatch accepts in that bucket.

```sql
WITH ls AS (
 SELECT DISTINCT l.LOAN_KEY k,CAST(l.ORIGINATIONDATE AS DATE) od,ROUND(l.ORIGINALLOANAMOUNT,2) amt
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE AND l.ORIGINATIONDATE IS NOT NULL AND l.ORIGINALLOANAMOUNT IS NOT NULL
), ad AS (
 SELECT DISTINCT DOCUMENT_ID doc,RECORDED_BOROUGH rb,CAST(RECORDED_DATETIME AS DATE) rd,ROUND(DOCUMENT_AMT,2) amt
 FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT
 WHERE RELEASE_DT='2026-08-10' AND RECORDED_BOROUGH IN (1,2,3,4,5) AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD') AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000
   AND CAST(RECORDED_DATETIME AS DATE) BETWEEN DATEADD(day,-30,(SELECT MIN(od) FROM ls)) AND DATEADD(day,30,(SELECT MAX(od) FROM ls))
), ca AS (
 SELECT ls.k,ad.doc,ad.rb,ad.rd,ls.od,ls.amt,DATEDIFF(day,ls.od,ad.rd) off
 FROM ls JOIN ad ON ad.amt=ls.amt AND ad.rd BETWEEN DATEADD(day,-30,ls.od) AND DATEADD(day,30,ls.od)
), cn AS (SELECT k,COUNT(DISTINCT doc)n FROM ca GROUP BY k), ac AS (SELECT ca.* FROM ca JOIN cn USING(k) WHERE n=1),
ab AS (
 SELECT DISTINCT ac.k,TO_VARCHAR(l.BOROUGH)||LPAD(TO_VARCHAR(l.BLOCK),5,'0')||LPAD(TO_VARCHAR(l.LOT),4,'0') bbl,TO_VARCHAR(l.BOROUGH)||LPAD(TO_VARCHAR(l.BLOCK),5,'0') blk,l.BOROUGH lb
 FROM ac JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT l ON ac.doc=l.DOCUMENT_ID
 WHERE l.RELEASE_DT='2026-08-10' AND l.BOROUGH IN (1,2,3,4,5) AND l.BLOCK IS NOT NULL AND l.LOT IS NOT NULL
), ps AS (
 SELECT DISTINCT p.PROPERTY_KEY pk,l.LOAN_KEY k,CASE TO_VARCHAR(p.COUNTY_FIPS) WHEN '36061' THEN 1 WHEN '36005' THEN 2 WHEN '36047' THEN 3 WHEN '36081' THEN 4 WHEN '36085' THEN 5 END pb
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE
), gp AS (
 SELECT DISTINCT ps.k,ps.pk,d.LATITUDE lat,d.LONGITUDE lon
 FROM ps JOIN ac USING(k) JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d ON ps.pk=d.PROPERTY_KEY
 WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND d.LATITUDE IS NOT NULL AND d.LONGITUDE IS NOT NULL
), pe AS (
 SELECT DISTINCT gp.k,REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$','') pbbl,SUBSTR(REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$',''),1,6) pblk
 FROM gp JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON ST_CONTAINS(pl.GEOM_GEOG,ST_POINT(gp.lon,gp.lat))
), lo AS (
 SELECT ac.k,ac.doc,ac.rb,COUNT(DISTINCT pe.pbbl) pred_bbls,COUNT(DISTINCT IFF(pe.pbbl=ab.bbl,pe.pbbl,NULL)) lot_hits,COUNT(DISTINCT IFF(pe.pblk=ab.blk,pe.pblk,NULL)) block_hits,
        COUNT(DISTINCT ps.pb) property_boroughs,COUNT(DISTINCT ab.lb) legal_boroughs,MAX(IFF(ps.pb=ac.rb,1,0)) recorded_borough_match,MAX(IFF(ps.pb=ab.lb,1,0)) legal_borough_match
 FROM ac JOIN ab USING(k) JOIN ps USING(k) LEFT JOIN pe USING(k)
 GROUP BY ac.k,ac.doc,ac.rb
), o AS (
 SELECT *,CASE WHEN pred_bbls=0 THEN 'no_pip_prediction' WHEN lot_hits>0 THEN 'correct' ELSE 'incorrect' END scored_outcome,
          CASE WHEN pred_bbls=0 THEN 'no_pip_prediction' WHEN lot_hits>0 THEN 'lot_correct' WHEN block_hits>0 THEN 'block_match_lot_mismatch' ELSE 'full_block_mismatch' END scored_detail
 FROM lo
)
SELECT scored_outcome,scored_detail,COUNT(*) loans,SUM(IFF(recorded_borough_match=1,1,0)) recorded_borough_agree_any_property,SUM(IFF(recorded_borough_match=0,1,0)) recorded_borough_disagree_all_properties,SUM(IFF(legal_borough_match=1,1,0)) legal_borough_agree_any_property,SUM(IFF(legal_borough_match=0,1,0)) legal_borough_disagree_all_properties,SUM(IFF(recorded_borough_match=0 AND legal_borough_match=0,1,0)) recorded_and_legal_disagree_all_properties,MIN(property_boroughs) min_property_borough_count,MAX(property_boroughs) max_property_borough_count,MIN(legal_boroughs) min_legal_borough_count,MAX(legal_boroughs) max_legal_borough_count
FROM o
GROUP BY scored_outcome,scored_detail
ORDER BY scored_outcome,scored_detail;
```

### 3. Amount Roundness

Round amounts load most of the contamination. Non-round loans have 55.46%
accepted-loan PIP lot precision; 100k/1m-round loans have 17.51%, and 1m
multiples alone have 7.88%.

| grain | amount_class | accepted loans | PIP covered | lot correct | lot precision | block correct | block upper | block-match lot-mismatch | full-block mismatch | no PIP prediction | negative offsets |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| all | ALL | 523 | 513 | 135 | 26.32% | 157 | 30.60% | 22 | 356 | 10 | 168 |
| binary_100k | non_round_100k | 123 | 119 | 66 | 55.46% | 72 | 60.50% | 6 | 47 | 4 | 23 |
| binary_100k | round_100k_or_1m | 400 | 394 | 69 | 17.51% | 85 | 21.57% | 16 | 309 | 6 | 145 |
| fine | multiple_100k_not_1m | 156 | 153 | 50 | 32.68% | 54 | 35.29% | 4 | 99 | 3 | 55 |
| fine | multiple_1m | 244 | 241 | 19 | 7.88% | 31 | 12.86% | 12 | 210 | 3 | 90 |
| fine | non_round_100k | 123 | 119 | 66 | 55.46% | 72 | 60.50% | 6 | 47 | 4 | 23 |

Interpretation: the original exact-cents +/-30 unique gate is useful as a
Source-1 foothold, but it is not clean enough for a headline precision estimate
on round-dollar loans. The strongest ACRIS-only precision diagnostic is the
non-round subset; the full accepted set is materially contaminated by amount
collisions, especially whole-million loans.

```sql
WITH ls AS (
 SELECT DISTINCT l.LOAN_KEY k,CAST(l.ORIGINATIONDATE AS DATE) od,ROUND(l.ORIGINALLOANAMOUNT,2) amt
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE AND l.ORIGINATIONDATE IS NOT NULL AND l.ORIGINALLOANAMOUNT IS NOT NULL
), ad AS (
 SELECT DISTINCT DOCUMENT_ID doc,RECORDED_BOROUGH rb,CAST(RECORDED_DATETIME AS DATE) rd,ROUND(DOCUMENT_AMT,2) amt
 FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT
 WHERE RELEASE_DT='2026-08-10' AND RECORDED_BOROUGH IN (1,2,3,4,5) AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD') AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000
   AND CAST(RECORDED_DATETIME AS DATE) BETWEEN DATEADD(day,-30,(SELECT MIN(od) FROM ls)) AND DATEADD(day,30,(SELECT MAX(od) FROM ls))
), ca AS (
 SELECT ls.k,ad.doc,ad.rb,ad.rd,ls.od,ls.amt,DATEDIFF(day,ls.od,ad.rd) off
 FROM ls JOIN ad ON ad.amt=ls.amt AND ad.rd BETWEEN DATEADD(day,-30,ls.od) AND DATEADD(day,30,ls.od)
), cn AS (SELECT k,COUNT(DISTINCT doc)n FROM ca GROUP BY k), ac AS (SELECT ca.* FROM ca JOIN cn USING(k) WHERE n=1),
ab AS (
 SELECT DISTINCT ac.k,TO_VARCHAR(l.BOROUGH)||LPAD(TO_VARCHAR(l.BLOCK),5,'0')||LPAD(TO_VARCHAR(l.LOT),4,'0') bbl,TO_VARCHAR(l.BOROUGH)||LPAD(TO_VARCHAR(l.BLOCK),5,'0') blk,l.BOROUGH lb
 FROM ac JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT l ON ac.doc=l.DOCUMENT_ID
 WHERE l.RELEASE_DT='2026-08-10' AND l.BOROUGH IN (1,2,3,4,5) AND l.BLOCK IS NOT NULL AND l.LOT IS NOT NULL
), ps AS (
 SELECT DISTINCT p.PROPERTY_KEY pk,l.LOAN_KEY k
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE
), gp AS (
 SELECT DISTINCT ps.k,ps.pk,d.LATITUDE lat,d.LONGITUDE lon
 FROM ps JOIN ac USING(k) JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d ON ps.pk=d.PROPERTY_KEY
 WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND d.LATITUDE IS NOT NULL AND d.LONGITUDE IS NOT NULL
), pe AS (
 SELECT DISTINCT gp.k,REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$','') pbbl,SUBSTR(REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$',''),1,6) pblk
 FROM gp JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON ST_CONTAINS(pl.GEOM_GEOG,ST_POINT(gp.lon,gp.lat))
), lo AS (
 SELECT ac.k,ac.amt,ac.off,COUNT(DISTINCT pe.pbbl) pred_bbls,COUNT(DISTINCT IFF(pe.pbbl=ab.bbl,pe.pbbl,NULL)) lot_hits,COUNT(DISTINCT IFF(pe.pblk=ab.blk,pe.pblk,NULL)) block_hits
 FROM ac JOIN ab USING(k) LEFT JOIN pe USING(k)
 GROUP BY ac.k,ac.amt,ac.off
), o AS (
 SELECT *,IFF(pred_bbls>0,1,0) covered,IFF(lot_hits>0,1,0) lot_correct,IFF(block_hits>0,1,0) block_correct,
          CASE WHEN MOD(amt,1000000)=0 THEN 'multiple_1m' WHEN MOD(amt,100000)=0 THEN 'multiple_100k_not_1m' ELSE 'non_round_100k' END fine_roundness,
          IFF(MOD(amt,100000)=0,'round_100k_or_1m','non_round_100k') binary_roundness
 FROM lo
), r AS (
 SELECT 'fine' grain,fine_roundness amount_class,* FROM o
 UNION ALL SELECT 'binary_100k',binary_roundness,* FROM o
 UNION ALL SELECT 'all','ALL',* FROM o
)
SELECT grain,amount_class,COUNT(*) accepted_loans,SUM(covered) pip_covered,SUM(lot_correct) lot_correct,ROUND(SUM(lot_correct)/NULLIF(SUM(covered),0)*100,2) lot_precision_pct,SUM(block_correct) block_correct,ROUND(SUM(block_correct)/NULLIF(SUM(covered),0)*100,2) block_upper_pct,SUM(IFF(covered=1 AND lot_correct=0 AND block_correct=1,1,0)) block_match_lot_mismatch,SUM(IFF(covered=1 AND block_correct=0,1,0)) full_block_mismatch,SUM(IFF(covered=0,1,0)) no_pip_prediction,SUM(IFF(off<0,1,0)) negative_offsets,MIN(amt) min_amount,MAX(amt) max_amount
FROM r
GROUP BY grain,amount_class
ORDER BY CASE grain WHEN 'all' THEN 1 WHEN 'binary_100k' THEN 2 ELSE 3 END, amount_class;
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

## Gate V2: Refined ACRIS Truth Re-score

Gate v2 recomputes candidate matches from scratch and applies the contamination
filters before the uniqueness gate:

- Recording offset must be non-negative: `RECORDED_DATE - ORIGINATIONDATE`
  between 0 and the tested upper window.
- At least one ACRIS legal borough must agree with at least one property county
  borough attached to the CMBS loan.
- Roundness rule: drop every loan whose amount is an exact multiple of 100,000.
  `LOAN_ISSUANCE.ORIGINATORNAME` and ACRIS `PARTY_TYPE` / `NAME` are landed,
  but I did not admit round-dollar loans via free-text token overlap. The
  conservative operating rule keeps the truth set address-independent and avoids
  replacing amount/date contamination with unvalidated lender-name heuristics.
- Unique-or-discard is applied after those filters.

Operating point selected: exact cents, non-round amount, legal-borough match,
offset window `[0, +45]`. The `[0, +60]` window finds more candidates but fewer
accepted loans because ambiguity increases.

### V2 Field Discovery

| table | relevant columns |
|---|---|
| `PROPERTY_MART.LOAN_ISSUANCE` | `ORIGINATORNAME`, `ORIGINATIONDATE`, `ORIGINALLOANAMOUNT` |
| `SOURCE.NYC_ACRIS_REAL_PROPERTY_PARTIES_EXT` | `DOCUMENT_ID`, `PARTY_TYPE`, `NAME` |

```sql
SELECT 'LOAN_ISSUANCE' table_name,column_name,data_type,ordinal_position
FROM EDGAR_DB.INFORMATION_SCHEMA.COLUMNS
WHERE table_schema='PROPERTY_MART' AND table_name='LOAN_ISSUANCE'
  AND (column_name ILIKE '%ORIGIN%' OR column_name ILIKE '%LENDER%' OR column_name ILIKE '%MORT%' OR column_name ILIKE '%SELLER%' OR column_name ILIKE '%SPONSOR%' OR column_name ILIKE '%BANK%' OR column_name ILIKE '%TRUST%')
UNION ALL
SELECT 'ACRIS_PARTIES' table_name,column_name,data_type,ordinal_position
FROM EDGAR_DB.INFORMATION_SCHEMA.COLUMNS
WHERE table_schema='SOURCE' AND table_name='NYC_ACRIS_REAL_PROPERTY_PARTIES_EXT'
  AND (column_name ILIKE '%PART%' OR column_name ILIKE '%NAME%' OR column_name ILIKE '%TYPE%' OR column_name ILIKE '%DOC%' OR column_name ILIKE '%RELEASE%')
ORDER BY table_name,ordinal_position;
```

### Gate V2 Sensitivity

The denominator remains the same 3,040 five-borough CMBS loans with amount and
origination date. Under the conservative roundness rule, 665 are non-round and
2,375 are excluded as exact 100k/1m-round amounts. The v2 operating point
reconciles as:

`166 accepts + 48 ambiguous discards + 451 no-match non-round loans + 2,375 round-excluded loans = 3,040`.

| offset window | total loans | non-round eligible | round excluded | candidate matches | loans with candidate | accepts | ambiguous discards | no match | reconcile total |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 30 | 3,040 | 665 | 2,375 | 250 | 189 | 149 | 40 | 476 | 3,040 |
| 45 | 3,040 | 665 | 2,375 | 303 | 214 | 166 | 48 | 451 | 3,040 |
| 60 | 3,040 | 665 | 2,375 | 348 | 223 | 161 | 62 | 442 | 3,040 |

```sql
WITH w AS (SELECT 30 win UNION ALL SELECT 45 UNION ALL SELECT 60),
ls AS (
 SELECT DISTINCT l.LOAN_KEY k,CAST(l.ORIGINATIONDATE AS DATE) od,ROUND(l.ORIGINALLOANAMOUNT,2) amt,IFF(MOD(ROUND(l.ORIGINALLOANAMOUNT,2),100000)=0,1,0) is_round
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE AND l.ORIGINATIONDATE IS NOT NULL AND l.ORIGINALLOANAMOUNT IS NOT NULL
), st AS (SELECT COUNT(*) total_loans,SUM(IFF(is_round=0,1,0)) nonround_eligible,SUM(is_round) round_excluded FROM ls),
lb AS (
 SELECT DISTINCT l.LOAN_KEY k,CASE TO_VARCHAR(p.COUNTY_FIPS) WHEN '36061' THEN 1 WHEN '36005' THEN 2 WHEN '36047' THEN 3 WHEN '36081' THEN 4 WHEN '36085' THEN 5 END boro
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE
), ad AS (
 SELECT DISTINCT DOCUMENT_ID doc,CAST(RECORDED_DATETIME AS DATE) rd,ROUND(DOCUMENT_AMT,2) amt
 FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT
 WHERE RELEASE_DT='2026-08-10' AND RECORDED_BOROUGH IN (1,2,3,4,5) AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD') AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000
   AND CAST(RECORDED_DATETIME AS DATE) BETWEEN (SELECT MIN(od) FROM ls) AND DATEADD(day,60,(SELECT MAX(od) FROM ls))
), raw AS (
 SELECT w.win,ls.k,ad.doc FROM w JOIN ls ON ls.is_round=0 JOIN ad ON ad.amt=ls.amt AND ad.rd BETWEEN ls.od AND DATEADD(day,w.win,ls.od)
), cand AS (
 SELECT DISTINCT raw.win,raw.k,raw.doc
 FROM raw JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON raw.doc=le.DOCUMENT_ID JOIN lb ON raw.k=lb.k AND le.BOROUGH=lb.boro
 WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN (1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL
), cc AS (SELECT win,k,COUNT(DISTINCT doc) docs FROM cand GROUP BY win,k)
SELECT w.win offset_window_days,st.total_loans,st.nonround_eligible,st.round_excluded,COUNT(DISTINCT cand.k||':'||cand.doc) candidate_matches,COUNT(DISTINCT cand.k) loans_with_candidate,COUNT(DISTINCT IFF(cc.docs=1,cc.k,NULL)) accepts,COUNT(DISTINCT IFF(cc.docs>1,cc.k,NULL)) ambiguous_discards,st.nonround_eligible-COUNT(DISTINCT cand.k) no_match,COUNT(DISTINCT IFF(cc.docs=1,cc.k,NULL))+COUNT(DISTINCT IFF(cc.docs>1,cc.k,NULL))+(st.nonround_eligible-COUNT(DISTINCT cand.k))+st.round_excluded reconcile_total
FROM w CROSS JOIN st LEFT JOIN cand ON w.win=cand.win LEFT JOIN cc ON cand.win=cc.win AND cand.k=cc.k
GROUP BY w.win,st.total_loans,st.nonround_eligible,st.round_excluded
ORDER BY w.win;
```

### Gate V2 Baseline Re-score

Truth coverage stays low because the v2 gate intentionally trades coverage for
cleanliness:

- Geometry point grain: 242 / 4,076 = 5.94% truth coverage.
- Address-key grain: 328 / 5,269 = 6.23% truth coverage.

Geometry PIP, lot and block grade:

| accuracy_type | universe | truth units | truth coverage | covered | coverage on truth | lot correct | lot precision | block correct | block upper | block-match lot-mismatch | full-block mismatch |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ALL | 4,076 | 242 | 5.94% | 233 | 96.28% | 154 | 66.09% | 169 | 72.53% | 15 | 64 |
| intersection | 19 | 1 | 5.26% | 0 | 0.00% | 0 | n/a | 0 | n/a | 0 | 0 |
| mixed | 87 | 7 | 8.05% | 7 | 100.00% | 6 | 85.71% | 6 | 85.71% | 0 | 1 |
| nearest_rooftop_match | 344 | 29 | 8.43% | 29 | 100.00% | 15 | 51.72% | 18 | 62.07% | 3 | 11 |
| place | 30 | 1 | 3.33% | 1 | 100.00% | 0 | 0.00% | 0 | 0.00% | 0 | 1 |
| range_interpolation | 315 | 9 | 2.86% | 2 | 22.22% | 2 | 100.00% | 2 | 100.00% | 0 | 0 |
| rooftop | 3,216 | 193 | 6.00% | 192 | 99.48% | 131 | 68.23% | 143 | 74.48% | 12 | 49 |
| street_center | 65 | 2 | 3.08% | 2 | 100.00% | 0 | 0.00% | 0 | 0.00% | 0 | 2 |

Naive address-key, lot and block grade:

| accuracy_type | universe | truth units | truth coverage | covered | coverage on truth | lot correct | lot precision | block correct | block upper | block-match lot-mismatch | full-block mismatch |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ALL | 5,269 | 328 | 6.23% | 93 | 28.35% | 63 | 67.74% | 71 | 76.34% | 8 | 22 |
| intersection | 20 | 1 | 5.00% | 0 | 0.00% | 0 | n/a | 0 | n/a | 0 | 0 |
| mixed | 47 | 5 | 10.64% | 0 | 0.00% | 0 | n/a | 0 | n/a | 0 | 0 |
| nearest_rooftop_match | 593 | 44 | 7.42% | 0 | 0.00% | 0 | n/a | 0 | n/a | 0 | 0 |
| place | 43 | 1 | 2.33% | 0 | 0.00% | 0 | n/a | 0 | n/a | 0 | 0 |
| range_interpolation | 340 | 9 | 2.65% | 2 | 22.22% | 2 | 100.00% | 2 | 100.00% | 0 | 0 |
| rooftop | 4,160 | 266 | 6.39% | 91 | 34.21% | 61 | 67.03% | 69 | 75.82% | 8 | 22 |
| street_center | 66 | 2 | 3.03% | 0 | 0.00% | 0 | n/a | 0 | n/a | 0 | 0 |

Key interpretation: after v2 filtering, the geometry baseline is still covered
on 96.28% of truth points and scores 66.09% lot-grade / 72.53% block-grade. The
nearest-rooftop silent-error tier improves from the contaminated 23.19% lot
grade to 51.72% lot grade, but still leaves 11 full-block mismatches among 29
covered truth points. The address baseline is precise on the few units it
covers, but covers only 28.35% of truth keys and fires on 0 / 44 nearest-rooftop
truth keys.

Geometry score SQL:

```sql
WITH ls AS (
 SELECT DISTINCT l.LOAN_KEY k,CAST(l.ORIGINATIONDATE AS DATE) od,ROUND(l.ORIGINALLOANAMOUNT,2) amt,IFF(MOD(ROUND(l.ORIGINALLOANAMOUNT,2),100000)=0,1,0) is_round
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE AND l.ORIGINATIONDATE IS NOT NULL AND l.ORIGINALLOANAMOUNT IS NOT NULL
), lb AS (
 SELECT DISTINCT l.LOAN_KEY k,CASE TO_VARCHAR(p.COUNTY_FIPS) WHEN '36061' THEN 1 WHEN '36005' THEN 2 WHEN '36047' THEN 3 WHEN '36081' THEN 4 WHEN '36085' THEN 5 END boro
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE
), ad AS (
 SELECT DISTINCT DOCUMENT_ID doc,CAST(RECORDED_DATETIME AS DATE) rd,ROUND(DOCUMENT_AMT,2) amt
 FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT
 WHERE RELEASE_DT='2026-08-10' AND RECORDED_BOROUGH IN (1,2,3,4,5) AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD') AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000
   AND CAST(RECORDED_DATETIME AS DATE) BETWEEN (SELECT MIN(od) FROM ls) AND DATEADD(day,45,(SELECT MAX(od) FROM ls))
), raw AS (
 SELECT ls.k,ad.doc FROM ls JOIN ad ON ls.is_round=0 AND ad.amt=ls.amt AND ad.rd BETWEEN ls.od AND DATEADD(day,45,ls.od)
), cand AS (
 SELECT DISTINCT raw.k,raw.doc FROM raw JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON raw.doc=le.DOCUMENT_ID JOIN lb ON raw.k=lb.k AND le.BOROUGH=lb.boro
 WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN (1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL
), cc AS (SELECT k,COUNT(DISTINCT doc) docs FROM cand GROUP BY k), ac AS (SELECT cand.k,cand.doc FROM cand JOIN cc USING(k) WHERE docs=1),
ab AS (
 SELECT DISTINCT ac.k,TO_VARCHAR(le.BOROUGH)||LPAD(TO_VARCHAR(le.BLOCK),5,'0')||LPAD(TO_VARCHAR(le.LOT),4,'0') bbl,TO_VARCHAR(le.BOROUGH)||LPAD(TO_VARCHAR(le.BLOCK),5,'0') blk
 FROM ac JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON ac.doc=le.DOCUMENT_ID
 WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN (1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL
), ps AS (
 SELECT DISTINCT p.PROPERTY_KEY pk,l.LOAN_KEY k FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE
), td AS (
 SELECT DISTINCT ps.pk,ab.k,ab.bbl,ab.blk,d.LATITUDE lat,d.LONGITUDE lon FROM ab JOIN ps USING(k) JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d ON ps.pk=d.PROPERTY_KEY
 WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND d.LATITUDE IS NOT NULL AND d.LONGITUDE IS NOT NULL
), r AS (SELECT * FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')),
pts AS (SELECT LATITUDE lat,LONGITUDE lon,IFF(COUNT(DISTINCT ACCURACY_TYPE)=1,MIN(ACCURACY_TYPE),'mixed') acc FROM r WHERE LATITUDE IS NOT NULL AND LONGITUDE IS NOT NULL GROUP BY LATITUDE,LONGITUDE),
u AS (SELECT acc,COUNT(*) universe FROM pts GROUP BY acc UNION ALL SELECT 'ALL',COUNT(*) FROM pts),
tb AS (SELECT DISTINCT pts.lat,pts.lon,pts.acc,td.bbl,td.blk FROM pts JOIN td ON pts.lat=td.lat AND pts.lon=td.lon),
pe AS (SELECT DISTINCT pts.lat,pts.lon,REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$','') pbbl,SUBSTR(REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$',''),1,6) pblk FROM pts JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON ST_CONTAINS(pl.GEOM_GEOG,ST_POINT(pts.lon,pts.lat))),
e AS (
 SELECT tb.lat,tb.lon,tb.acc,COUNT(DISTINCT tb.bbl) truth_bbls,COUNT(DISTINCT tb.blk) truth_blocks,COUNT(DISTINCT pe.pbbl) pred_bbls,COUNT(DISTINCT IFF(pe.pbbl=tb.bbl,pe.pbbl,NULL)) lot_hits,COUNT(DISTINCT IFF(pe.pblk=tb.blk,pe.pblk,NULL)) block_hits
 FROM tb LEFT JOIN pe ON tb.lat=pe.lat AND tb.lon=pe.lon GROUP BY tb.lat,tb.lon,tb.acc
), ag AS (
 SELECT acc,COUNT(*) truth_units,SUM(IFF(pred_bbls>0,1,0)) covered,SUM(IFF(pred_bbls>0 AND lot_hits>0,1,0)) lot_correct,SUM(IFF(pred_bbls>0 AND block_hits>0,1,0)) block_correct,SUM(IFF(pred_bbls>0 AND lot_hits=0 AND block_hits>0,1,0)) block_match_lot_mismatch,SUM(IFF(pred_bbls>0 AND block_hits=0,1,0)) full_block_mismatch,SUM(IFF(pred_bbls>1,1,0)) multi_predict,SUM(truth_bbls) truth_bbl_edges,SUM(truth_blocks) truth_block_edges FROM e GROUP BY acc
 UNION ALL SELECT 'ALL',COUNT(*),SUM(IFF(pred_bbls>0,1,0)),SUM(IFF(pred_bbls>0 AND lot_hits>0,1,0)),SUM(IFF(pred_bbls>0 AND block_hits>0,1,0)),SUM(IFF(pred_bbls>0 AND lot_hits=0 AND block_hits>0,1,0)),SUM(IFF(pred_bbls>0 AND block_hits=0,1,0)),SUM(IFF(pred_bbls>1,1,0)),SUM(truth_bbls),SUM(truth_blocks) FROM e
)
SELECT 'geometry_pip_point' baseline,ag.acc accuracy_type,u.universe universe_units,truth_units,ROUND(truth_units/NULLIF(u.universe,0)*100,2) truth_coverage_pct,covered,ROUND(covered/NULLIF(truth_units,0)*100,2) coverage_on_truth_pct,lot_correct,ROUND(lot_correct/NULLIF(covered,0)*100,2) lot_precision_pct,block_correct,ROUND(block_correct/NULLIF(covered,0)*100,2) block_upper_pct,block_match_lot_mismatch,full_block_mismatch,multi_predict,truth_bbl_edges,truth_block_edges
FROM ag JOIN u ON ag.acc=u.acc ORDER BY IFF(ag.acc='ALL',0,1),ag.acc;
```

Address score SQL: same gate CTEs and address-key normalization as G5, with
block-grade columns added.

```sql
WITH ls AS (
 SELECT DISTINCT l.LOAN_KEY k,CAST(l.ORIGINATIONDATE AS DATE) od,ROUND(l.ORIGINALLOANAMOUNT,2) amt,IFF(MOD(ROUND(l.ORIGINALLOANAMOUNT,2),100000)=0,1,0) is_round
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE AND l.ORIGINATIONDATE IS NOT NULL AND l.ORIGINALLOANAMOUNT IS NOT NULL
), lb AS (
 SELECT DISTINCT l.LOAN_KEY k,CASE TO_VARCHAR(p.COUNTY_FIPS) WHEN '36061' THEN 1 WHEN '36005' THEN 2 WHEN '36047' THEN 3 WHEN '36081' THEN 4 WHEN '36085' THEN 5 END boro
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE
), ad AS (
 SELECT DISTINCT DOCUMENT_ID doc,CAST(RECORDED_DATETIME AS DATE) rd,ROUND(DOCUMENT_AMT,2) amt
 FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT
 WHERE RELEASE_DT='2026-08-10' AND RECORDED_BOROUGH IN (1,2,3,4,5) AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD') AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000
   AND CAST(RECORDED_DATETIME AS DATE) BETWEEN (SELECT MIN(od) FROM ls) AND DATEADD(day,45,(SELECT MAX(od) FROM ls))
), raw AS (
 SELECT ls.k,ad.doc FROM ls JOIN ad ON ls.is_round=0 AND ad.amt=ls.amt AND ad.rd BETWEEN ls.od AND DATEADD(day,45,ls.od)
), cand AS (
 SELECT DISTINCT raw.k,raw.doc FROM raw JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON raw.doc=le.DOCUMENT_ID JOIN lb ON raw.k=lb.k AND le.BOROUGH=lb.boro
 WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN (1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL
), cc AS (SELECT k,COUNT(DISTINCT doc) docs FROM cand GROUP BY k), ac AS (SELECT cand.k,cand.doc FROM cand JOIN cc USING(k) WHERE docs=1),
ab AS (
 SELECT DISTINCT ac.k,TO_VARCHAR(le.BOROUGH)||LPAD(TO_VARCHAR(le.BLOCK),5,'0')||LPAD(TO_VARCHAR(le.LOT),4,'0') bbl,TO_VARCHAR(le.BOROUGH)||LPAD(TO_VARCHAR(le.BLOCK),5,'0') blk
 FROM ac JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON ac.doc=le.DOCUMENT_ID
 WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN (1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL
), ps AS (
 SELECT DISTINCT p.PROPERTY_KEY pk,l.LOAN_KEY k FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE
), td AS (
 SELECT DISTINCT ps.pk,ab.k,ab.bbl,ab.blk,d.LATITUDE lat,d.LONGITUDE lon FROM ab JOIN ps USING(k) JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d ON ps.pk=d.PROPERTY_KEY
 WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND d.LATITUDE IS NOT NULL AND d.LONGITUDE IS NOT NULL
), r AS (SELECT * FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')),
ak AS (
 SELECT PROPERTY_ADDRESS,COUNTY_FIPS,CASE COUNTY_FIPS WHEN '36005' THEN 'BX' WHEN '36047' THEN 'BK' WHEN '36061' THEN 'MN' WHEN '36081' THEN 'QN' WHEN '36085' THEN 'SI' END boro,IFF(COUNT(DISTINCT ACCURACY_TYPE)=1,MIN(ACCURACY_TYPE),'mixed') acc,REGEXP_REPLACE(UPPER(TRIM(PROPERTY_ADDRESS)),'[^A-Z0-9]','') norm
 FROM r GROUP BY PROPERTY_ADDRESS,COUNTY_FIPS
), u AS (SELECT acc,COUNT(*) universe FROM ak GROUP BY acc UNION ALL SELECT 'ALL',COUNT(*) FROM ak),
tb AS (
 SELECT DISTINCT ak.PROPERTY_ADDRESS,ak.COUNTY_FIPS,ak.acc,td.bbl,td.blk FROM ak JOIN r ON ak.PROPERTY_ADDRESS=r.PROPERTY_ADDRESS AND ak.COUNTY_FIPS=r.COUNTY_FIPS JOIN td ON r.LATITUDE=td.lat AND r.LONGITUDE=td.lon
), pa AS (
 SELECT DISTINCT BOROUGH boro,REGEXP_REPLACE(TO_VARCHAR(BBL),'\\.0$','') pbbl,SUBSTR(REGEXP_REPLACE(TO_VARCHAR(BBL),'\\.0$',''),1,6) pblk,REGEXP_REPLACE(UPPER(TRIM(ADDRESS)),'[^A-Z0-9]','') norm FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
), pe AS (SELECT DISTINCT ak.PROPERTY_ADDRESS,ak.COUNTY_FIPS,pa.pbbl,pa.pblk FROM ak JOIN pa ON pa.boro=ak.boro AND pa.norm=ak.norm),
e AS (
 SELECT tb.PROPERTY_ADDRESS,tb.COUNTY_FIPS,tb.acc,COUNT(DISTINCT tb.bbl) truth_bbls,COUNT(DISTINCT tb.blk) truth_blocks,COUNT(DISTINCT pe.pbbl) pred_bbls,COUNT(DISTINCT IFF(pe.pbbl=tb.bbl,pe.pbbl,NULL)) lot_hits,COUNT(DISTINCT IFF(pe.pblk=tb.blk,pe.pblk,NULL)) block_hits
 FROM tb LEFT JOIN pe ON tb.PROPERTY_ADDRESS=pe.PROPERTY_ADDRESS AND tb.COUNTY_FIPS=pe.COUNTY_FIPS GROUP BY tb.PROPERTY_ADDRESS,tb.COUNTY_FIPS,tb.acc
), ag AS (
 SELECT acc,COUNT(*) truth_units,SUM(IFF(pred_bbls>0,1,0)) covered,SUM(IFF(pred_bbls>0 AND lot_hits>0,1,0)) lot_correct,SUM(IFF(pred_bbls>0 AND block_hits>0,1,0)) block_correct,SUM(IFF(pred_bbls>0 AND lot_hits=0 AND block_hits>0,1,0)) block_match_lot_mismatch,SUM(IFF(pred_bbls>0 AND block_hits=0,1,0)) full_block_mismatch,SUM(IFF(pred_bbls>1,1,0)) multi_predict,SUM(truth_bbls) truth_bbl_edges,SUM(truth_blocks) truth_block_edges FROM e GROUP BY acc
 UNION ALL SELECT 'ALL',COUNT(*),SUM(IFF(pred_bbls>0,1,0)),SUM(IFF(pred_bbls>0 AND lot_hits>0,1,0)),SUM(IFF(pred_bbls>0 AND block_hits>0,1,0)),SUM(IFF(pred_bbls>0 AND lot_hits=0 AND block_hits>0,1,0)),SUM(IFF(pred_bbls>0 AND block_hits=0,1,0)),SUM(IFF(pred_bbls>1,1,0)),SUM(truth_bbls),SUM(truth_blocks) FROM e
)
SELECT 'naive_address_key' baseline,ag.acc accuracy_type,u.universe universe_units,truth_units,ROUND(truth_units/NULLIF(u.universe,0)*100,2) truth_coverage_pct,covered,ROUND(covered/NULLIF(truth_units,0)*100,2) coverage_on_truth_pct,lot_correct,ROUND(lot_correct/NULLIF(covered,0)*100,2) lot_precision_pct,block_correct,ROUND(block_correct/NULLIF(covered,0)*100,2) block_upper_pct,block_match_lot_mismatch,full_block_mismatch,multi_predict,truth_bbl_edges,truth_block_edges
FROM ag JOIN u ON ag.acc=u.acc ORDER BY IFF(ag.acc='ALL',0,1),ag.acc;
```

### Gate V2 G6 Assemblage Split

Gate v2 accepts 166 loans: 138 one-BBL accepts and 28 multi-BBL accepts. The
previous 125-loan invisible-assemblage population shrinks to 25 multi-BBL loans
where all attached property keys PIP to exactly one lot. Of those 25, 10 are
condo-signature by the ACRIS unit-lot heuristic and 15 are non-condo; the
non-condo subset splits into 8 multi-block and 7 same-block multi-lot cases.

Distribution:

| grain | ACRIS class | PIP class | units | loans | sum ACRIS BBLs | max ACRIS BBLs | property-key edges |
|---|---|---|---:|---:|---:|---:|---:|
| loan_grain | acris_multi_bbl | all_properties_pip_one | 25 | 25 | 108 | 16 | 43 |
| loan_grain | acris_multi_bbl | all_properties_pip_zero | 1 | 1 | 3 | 3 | 1 |
| loan_grain | acris_multi_bbl | mixed_pip_one_zero | 2 | 2 | 28 | 26 | 29 |
| loan_grain | acris_one_bbl | all_properties_pip_one | 133 | 133 | 133 | 1 | 188 |
| loan_grain | acris_one_bbl | all_properties_pip_zero | 2 | 2 | 2 | 1 | 2 |
| loan_grain | acris_one_bbl | mixed_pip_one_zero | 3 | 3 | 3 | 1 | 7 |
| property_key_grain | acris_multi_bbl | pip_one_lot | 69 | 27 | 844 | 26 | n/a |
| property_key_grain | acris_multi_bbl | pip_zero_lots | 4 | 3 | 57 | 26 | n/a |
| property_key_grain | acris_one_bbl | pip_one_lot | 192 | 136 | 192 | 1 | n/a |
| property_key_grain | acris_one_bbl | pip_zero_lots | 5 | 5 | 5 | 1 | n/a |

Condo/block split:

| ACRIS class | PIP class | condo class | block class | loans | sum ACRIS BBLs | max ACRIS BBLs | property-key edges |
|---|---|---|---|---:|---:|---:|---:|
| acris_multi_bbl | all_properties_pip_one | condo_signature | multi_acris_block | 4 | 10 | 4 | 6 |
| acris_multi_bbl | all_properties_pip_one | condo_signature | one_acris_block | 6 | 39 | 16 | 7 |
| acris_multi_bbl | all_properties_pip_one | non_condo_signature | multi_acris_block | 8 | 40 | 10 | 22 |
| acris_multi_bbl | all_properties_pip_one | non_condo_signature | one_acris_block | 7 | 19 | 4 | 8 |
| acris_multi_bbl | all_properties_pip_zero | non_condo_signature | one_acris_block | 1 | 3 | 3 | 1 |
| acris_multi_bbl | mixed_pip_one_zero | non_condo_signature | multi_acris_block | 1 | 26 | 26 | 27 |
| acris_multi_bbl | mixed_pip_one_zero | non_condo_signature | one_acris_block | 1 | 2 | 2 | 2 |
| acris_one_bbl | all_properties_pip_one | condo_signature | one_acris_block | 19 | 19 | 1 | 27 |
| acris_one_bbl | all_properties_pip_one | non_condo_signature | one_acris_block | 114 | 114 | 1 | 161 |
| acris_one_bbl | all_properties_pip_zero | non_condo_signature | one_acris_block | 2 | 2 | 1 | 2 |
| acris_one_bbl | mixed_pip_one_zero | condo_signature | one_acris_block | 2 | 2 | 1 | 4 |
| acris_one_bbl | mixed_pip_one_zero | non_condo_signature | one_acris_block | 1 | 1 | 1 | 3 |

G6 v2 SQL:

```sql
WITH ls AS (
 SELECT DISTINCT l.LOAN_KEY k,CAST(l.ORIGINATIONDATE AS DATE) od,ROUND(l.ORIGINALLOANAMOUNT,2) amt,IFF(MOD(ROUND(l.ORIGINALLOANAMOUNT,2),100000)=0,1,0) is_round
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE AND l.ORIGINATIONDATE IS NOT NULL AND l.ORIGINALLOANAMOUNT IS NOT NULL
), lb AS (
 SELECT DISTINCT l.LOAN_KEY k,CASE TO_VARCHAR(p.COUNTY_FIPS) WHEN '36061' THEN 1 WHEN '36005' THEN 2 WHEN '36047' THEN 3 WHEN '36081' THEN 4 WHEN '36085' THEN 5 END boro
 FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE
), ad AS (
 SELECT DISTINCT DOCUMENT_ID doc,CAST(RECORDED_DATETIME AS DATE) rd,ROUND(DOCUMENT_AMT,2) amt
 FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT
 WHERE RELEASE_DT='2026-08-10' AND RECORDED_BOROUGH IN (1,2,3,4,5) AND DOC_TYPE IN ('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD') AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000
   AND CAST(RECORDED_DATETIME AS DATE) BETWEEN (SELECT MIN(od) FROM ls) AND DATEADD(day,45,(SELECT MAX(od) FROM ls))
), raw AS (
 SELECT ls.k,ad.doc FROM ls JOIN ad ON ls.is_round=0 AND ad.amt=ls.amt AND ad.rd BETWEEN ls.od AND DATEADD(day,45,ls.od)
), cand AS (
 SELECT DISTINCT raw.k,raw.doc FROM raw JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON raw.doc=le.DOCUMENT_ID JOIN lb ON raw.k=lb.k AND le.BOROUGH=lb.boro
 WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN (1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL
), cc AS (SELECT k,COUNT(DISTINCT doc) docs FROM cand GROUP BY k), ac AS (SELECT cand.k,cand.doc FROM cand JOIN cc USING(k) WHERE docs=1),
ab AS (
 SELECT DISTINCT ac.k,ac.doc,TO_VARCHAR(le.BOROUGH)||LPAD(TO_VARCHAR(le.BLOCK),5,'0')||LPAD(TO_VARCHAR(le.LOT),4,'0') bbl,TO_VARCHAR(le.BOROUGH)||LPAD(TO_VARCHAR(le.BLOCK),5,'0') blk,IFF(TRY_TO_NUMBER(le.LOT) BETWEEN 1001 AND 6999,1,0) condo
 FROM ac JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON ac.doc=le.DOCUMENT_ID
 WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN (1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL
), lc AS (SELECT k,COUNT(DISTINCT bbl) acris_bbls,COUNT(DISTINCT blk) acris_blocks,MAX(condo) condo_sig FROM ab GROUP BY k),
ps AS (
 SELECT DISTINCT p.PROPERTY_KEY pk,l.LOAN_KEY k FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER
 WHERE p.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE
), gp AS (
 SELECT DISTINCT ps.pk,ps.k,lc.acris_bbls,lc.acris_blocks,lc.condo_sig,d.LATITUDE lat,d.LONGITUDE lon FROM ps JOIN lc USING(k) JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d ON ps.pk=d.PROPERTY_KEY
 WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085') AND d.LATITUDE IS NOT NULL AND d.LONGITUDE IS NOT NULL
), pp AS (
 SELECT gp.pk,gp.k,gp.acris_bbls,gp.acris_blocks,gp.condo_sig,COUNT(DISTINCT REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$','')) pip_lots
 FROM gp LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON ST_CONTAINS(pl.GEOM_GEOG,ST_POINT(gp.lon,gp.lat)) GROUP BY gp.pk,gp.k,gp.acris_bbls,gp.acris_blocks,gp.condo_sig
), lr AS (
 SELECT k,MAX(acris_bbls) acris_bbls,MAX(acris_blocks) acris_blocks,MAX(condo_sig) condo_sig,COUNT(DISTINCT pk) property_keys,SUM(IFF(pip_lots=0,1,0)) pip_zero,SUM(IFF(pip_lots=1,1,0)) pip_one,SUM(IFF(pip_lots>1,1,0)) pip_multi FROM pp GROUP BY k
), loan_class AS (
 SELECT k,acris_bbls,acris_blocks,condo_sig,property_keys,CASE WHEN pip_multi>0 THEN 'any_property_pip_multi' WHEN pip_one>0 AND pip_zero=0 THEN 'all_properties_pip_one' WHEN pip_one>0 AND pip_zero>0 THEN 'mixed_pip_one_zero' ELSE 'all_properties_pip_zero' END pip_class FROM lr
)
SELECT 'distribution' section,'property_key_grain' grain,IFF(acris_bbls=1,'acris_one_bbl','acris_multi_bbl') acris_class,CASE WHEN pip_lots=0 THEN 'pip_zero_lots' WHEN pip_lots=1 THEN 'pip_one_lot' ELSE 'pip_multi_lot' END pip_class,NULL condo_class,NULL block_class,COUNT(*) units,COUNT(DISTINCT k) loans,SUM(acris_bbls) sum_acris_bbls,MAX(acris_bbls) max_acris_bbls,NULL property_key_edges
FROM pp GROUP BY 1,2,3,4,5,6
UNION ALL
SELECT 'distribution','loan_grain',IFF(acris_bbls=1,'acris_one_bbl','acris_multi_bbl'),pip_class,NULL,NULL,COUNT(*),COUNT(DISTINCT k),SUM(acris_bbls),MAX(acris_bbls),SUM(property_keys) FROM loan_class GROUP BY 1,2,3,4,5,6
UNION ALL
SELECT 'condo_block_split','loan_grain',IFF(acris_bbls=1,'acris_one_bbl','acris_multi_bbl'),pip_class,IFF(condo_sig=1,'condo_signature','non_condo_signature'),IFF(acris_blocks=1,'one_acris_block','multi_acris_block'),COUNT(*),COUNT(DISTINCT k),SUM(acris_bbls),MAX(acris_bbls),SUM(property_keys) FROM loan_class GROUP BY 1,2,3,4,5,6
ORDER BY section,grain,acris_class,pip_class,condo_class,block_class;
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
- A first Gate V2 sensitivity attempt returned no
  `tool_responses[*].structuredContent`; it is discarded and no numbers from it
  are cited. The shorter sensitivity query recorded in the Gate V2 section
  returned structured results and is the cited source.

## G7: 2026-08-28 Filed-County Lender/Party Rebuild

This section is the controlling ACRIS measurement. It was executed against the
live warehouse through the repaired `cmdrvl-data` MCP path. Every source table
was discovered and described before measurement SQL; every cited measurement
returned a nonzero structured result. Reveal catalog discovery supplied table
keys and lineage, while the data MCP supplied live schema and query results.

### G7.1 Why Gate V2 Had To Be Rebuilt

The earlier Gate V2 used the borough carried by
`PROPERTY_MART.LOAN_ISSUANCE_PROPERTY`. Reveal provenance receipt
`049e10cc99e1e15b` resolved that field to:

```sql
CASE county_fips
  WHEN 36061 THEN 1
  WHEN 36005 THEN 2
  WHEN 36047 THEN 3
  WHEN 36081 THEN 4
  WHEN 36085 THEN 5
END
```

That `COUNTY_FIPS` is geocoder-derived. The gate still did not compare address
strings, but its borough restriction could suppress cross-borough geocoder
errors. It was therefore not independent enough to carry the truth claim made
in H.6. The correction is not to discard that run: it remains a useful
sensitivity plane. The correction is to stop admitting it as controlling truth.

Reveal provenance receipt `e441595de4d416a3` resolved
`PROPERTY_PERIOD_FACT.PROPERTYCOUNTY` as the raw filed property county from the
selected latest property-period snapshot. G7 now also requires raw filed
`PROPERTYSTATE = 'NY'` from the same property-period source before the county
mapping can admit a loan. Fresh control query
`01c6bd17-0821-a0dc-006c-c703088c2796` (197 ms, 7 rows) reproduced the
2,974-loan universe exactly under `PROPERTYSTATE = 'NY'`: 653 non-round and
2,321 round. The earlier raw county-only 3,016 diagnostic contained 42
same-named-county cross-state extras: GA 29, CA 6, VA 5, NC 3, null 3, and NA 1,
with overlap across state buckets. G7 admits only this state-guarded mapping:

```text
NEW YORK | MANHATTAN | NY061  -> ACRIS borough 1
BRONX                         -> ACRIS borough 2
KINGS | BROOKLYN             -> ACRIS borough 3
QUEENS                        -> ACRIS borough 4
RICHMOND                      -> ACRIS borough 5
anything else or missing      -> abstain
```

No geocoder county, parsed address, MapPLUTO address, or address-normalization
key enters truth admission. Geocoded points enter only later, as the prediction
being scored.

Mixed-state loans admitted by the raw NY filed rows are scoped as
`nyc_filed_collateral_slice`, not as full national collateral truth. A legal
residual that materializes H.7 rows must preserve that scope explicitly rather
than silently treating the NYC ACRIS subset as the whole loan.

Fresh originator availability control
`01c6bd19-0821-9afc-006c-c703088c0936` (313 ms, 2 rows), with lineage receipts
`1385b1fd64bf266f` for raw ABS-EE `ORIGINATORNAME` inheritance and
`dbd7d7dbc84727b2` for `ORIGINATOR_MATCH_TEXT` at source commit
`e7c8989527cd2fed84749226bf807bf8a0c83fa4`, reports originator text on
653/653 non-round and 2,317/2,321 round loans, with 0/4 absent and no
ambiguities. This conflicts with the archived G7 availability figures
(605/653 and 2,173/2,321). It is retained as an open empirical discrepancy.
Until the bounded ACRIS candidate/legal residual is rerun, the historical
149-loan round exact-lender acceptance is retained evidence, not freshly
reproduced acceptance.

Fresh bounded round candidate aggregation
`01c6bd25-0821-a0dc-006c-c703088c27be` (42,031 ms, nonzero array row) reported
2,317 round loans with exact originator text, 311 candidate loans, and 439
loan-document pairs, versus archived G7 2,173 / 182 / 277. Cached identical
repeat `01c6bd26-0821-9afc-006c-c703088c095a` is not independent. The bounded
aggregate-to-flatten legal continuation
`01c6bd28-0821-a0dc-006c-c703088c27c6` hit deterministic client cancellation
000604/57014 at 45,044 ms. It is discarded/cancelled, not evidence; no fresh
round legal counts are admissible from that attempt.

### G7.2 Pinned Sources And Live Controls

| source | pin | live control |
|---|---|---:|
| ACRIS master | `RELEASE_DT = 2026-08-10` | 17,065,090 rows |
| ACRIS legals | `RELEASE_DT = 2026-08-10` | 22,727,180 rows |
| ACRIS parties | `RELEASE_DT = 2026-08-10` | 46,540,137 rows |
| loan-property bridge | build `3aed6660-ce1c-46a9-aeb2-7296c134ce8f` | 51,496 rows |
| MapPLUTO 26v1 | `2026-05-01`, `shoreline_clipped` | 856,614 distinct BBLs |
| MapPLUTO 26v2 | `2026-08-01`, `shoreline_clipped` | 856,687 distinct BBLs |

The bridge build was observed on 2026-08-28. The exact catalog keys used for
discovery were:

```text
snowflake://edgar_db.source/nyc_acris_real_property_master_ext
snowflake://edgar_db.source/nyc_acris_real_property_legals_ext
snowflake://edgar_db.source/nyc_acris_real_property_parties_ext
snowflake://edgar_db.property_mart/loan_issuance
snowflake://edgar_db.property_mart/loan_issuance_property
snowflake://edgar_db.property_mart/property_period_fact
```

Reveal lineage showed that `LOAN_ISSUANCE_PROPERTY` depends on
`LOAN_ISSUANCE`, `PROPERTY_DIM`, and `PROPERTY_PERIOD_FACT`. Reveal did not
reliably resolve the ACRIS control-code table or MapPLUTO by exact search, so
those were verified through live table listing and description rather than
silently inferred from catalog search.

### G7.3 Declared Matching Semantics

ACRIS `DOCUMENT_AMT` is a floating-point source field. Both sides are therefore
compared as integer cents:

```sql
ROUND(value * 100, 0)::NUMBER(38,0)
```

This is exact arithmetic relative to the declared cents quantization. It is not
a claim that the floating source representation, the recorded instrument, or
world truth is exact.

The non-round plane uses exact cents, origination-to-recording offset `[0,+45]`
days, filed-county/legal-borough agreement, and unique-or-discard after all
filters. It excludes amounts divisible by $100,000.

The round plane requires a second discriminator before legal confirmation:
exact equality between `LOAN_ISSUANCE.ORIGINATORNAME` and the ACRIS lender party
name after applying the same narrow transform to both:

```sql
TRIM(REGEXP_REPLACE(UPPER(name), '[^A-Z0-9 ]', ' '))
```

This transform deliberately does **not** collapse internal whitespace, strip
corporate suffixes, compare token sets, use containment, or make a fuzzy match.
Those may recover candidates, but they cannot be admitted as truth until their
error rate is separately measured.

The ACRIS control-code table establishes the lender party role by document type:

| document types | lender `PARTY_TYPE` |
|---|---:|
| `CMTG`, `M&CON`, `MTGE`, `SMTG`, `SPRD` | 2 |
| `MMTG` | 1 |

An exact lender hit is a second discriminator, not a second independent source:
the party row and mortgage row belong to the same ACRIS record. Source count is
never substituted for independent information.

### G7.4 Bounded Query Shape

The first all-in-one amount/date/borough/party/legal join was cancelled after
45.16 seconds. A second monolithic formulation was also cancelled. Neither was
repeated after the failure mode became deterministic.

The successful formulation follows the system's intended mathematics:

1. Pin the 2,974-loan filed-county section and form exact amount/date/party
   candidates.
2. Aggregate that small relation into a single array row so the MCP returns the
   complete candidate set rather than truncating rows.
3. Bind the returned candidates as an explicit `VALUES` residual and join only
   that residual to ACRIS legals.
4. Require legal-borough agreement, then count candidate documents at loan grain
   and accept only one-document loans.
5. Attach accepted document BBL sets to collateral only within the same filed
   borough, then score the separately pinned MapPLUTO releases.

The essential residual shape is:

```sql
WITH residual(loan_key, document_id, filed_borough, plane) AS (
  SELECT * FROM VALUES
    -- literal rows returned by the bounded candidate stage
), legal_edges AS (
  SELECT DISTINCT r.loan_key, r.document_id, r.plane,
         l.borough, l.block, l.lot
  FROM residual r
  JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT l
    ON l.document_id = r.document_id
   AND l.release_dt = '2026-08-10'
   AND l.borough = r.filed_borough
)
SELECT ... FROM legal_edges;
```

This is a bounded section followed by a small exact residual. It is not evidence
for, and must never be described as, a monolithic national or 500k-candidate
solve.

The MCP success envelope did not expose Snowflake query IDs. They were recovered
from `EDGAR_DB.INFORMATION_SCHEMA.QUERY_HISTORY`, following upstream
`br-1h1l`. The MCP also capped ordinary row fetches at 200 despite a higher
requested limit; the array-row stage was necessary to preserve the denominator.

### G7.5 Query Receipts And Discarded Attempts

| purpose | Snowflake query ID | disposition |
|---|---|---|
| ACRIS release controls | `01c6b399-0821-86f2-006c-c703088970d2` | cited |
| bridge build discovery | `01c6b399-0821-83a1-006c-c7030889494e` | cited |
| initial geocoder-borough scope | `01c6b39b-0821-83a1-006c-c7030889495e` | diagnostic only |
| ACRIS lender role controls | `01c6b39b-0821-83a1-006c-c70308894962` | cited |
| first monolithic join | `01c6b39c-0821-83a1-006c-c7030889496e` | cancelled; no numbers cited |
| second monolithic join | `01c6b3a5-0821-86f2-006c-c70308897116` | cancelled; no numbers cited |
| truncated exact candidate rows | `01c6b3a1-0821-83a1-006c-c70308894982` | discarded |
| legal join over truncated residual | `01c6b3a1-0821-86f2-006c-c70308897102` | discarded |
| corrected diagnostic array | `01c6b3a3-0821-86f2-006c-c7030889710e` | diagnostic only |
| corrected diagnostic legal residual | `01c6b3a3-0821-83a1-006c-c70308894992` | diagnostic only |
| filed-county universe | `01c6b3b2-0821-86f2-006c-c7030889718a` | cited |
| filed exact-lender candidate array | `01c6b3b2-0821-83a1-006c-c703088949f2` | cited |
| filed non-round V2 candidate array | `01c6b3b2-0821-86f2-006c-c7030889718e` | cited |
| filed exact-lender legal residual | `01c6b3b3-0821-83a1-006c-c703088949fa` | cited |
| filed non-round legal residual | `01c6b3b3-0821-86f2-006c-c70308897196` | cited |
| filed-county dual-release score | `01c6b3b3-0821-83a1-006c-c70308894a02` | cited |

The first 200-row legal result produced 85 round accepts. That number is not
cited anywhere because its denominator was silently truncated. The corrected
array/residual run is the only admitted result.

### G7.6 Filed-Country Universe And Truth-Gate Reach

The filed-county universe contains 2,974 distinct loans:

| plane | eligible loans | originator text | no originator text | multi-filed-borough |
|---|---:|---:|---:|---:|
| non-round | 653 | 605 | 48 | 35 |
| round | 2,321 | 2,173 | 148 | 122 |
| total | 2,974 | 2,778 | 196 | 157 |

The non-round V2 gate produced 654 master candidate pairs on 262 loans and 687
candidate borough triples. Legal confirmation retained 313 loan-document pairs
on 221 candidate loans:

| non-round disposition | loans |
|---|---:|
| amount/date/legal-borough candidate | 262 |
| legal-confirmed candidate | 221 |
| unique accept | 172 |
| ambiguous after all filters | 49 |
| candidate without legal confirmation | 41 |
| no candidate | 391 |
| round, outside plane | 2,321 |
| **reconciled universe** | **2,974** |

The 172 accepts comprise 137 one-BBL and 35 multi-BBL loans, carrying 446 ACRIS
BBL edges. The multi-BBL count is defined here by accepted ACRIS truth
`BBL_COUNT > 1`, after legal acceptance, not by bridge property-key count.
Acceptance reach is 172/653 = **26.34%** of the non-round eligible plane and
172/2,974 = **5.78%** of the full filed-county universe.

Across all amount classes, exact lender matching produced 324 pairs on 229
loans. The round subset contained 277 pairs on 182 loans. Legal confirmation
retained 235 pairs on 179 loans; three exact-lender candidate loans had no legal
confirmation. The final round plane is:

| round exact-lender disposition | loans |
|---|---:|
| originator text available | 2,173 |
| exact lender candidate | 182 |
| legal-confirmed exact lender candidate | 179 |
| unique legal accept | 149 |
| ambiguous legal match | 30 |
| no legal confirmation after exact lender | 3 |
| no exact-lender candidate | 2,139 |

The 149 accepts comprise 135 one-BBL and 14 multi-BBL loans, carrying 353 ACRIS
BBL edges. The multi-BBL count is again accepted ACRIS truth `BBL_COUNT > 1`,
not an upstream bridge multi-property selector. Originator-text reach is
2,173/2,321 = **93.62%**; exact-lender reach is 182/2,321 = **7.84%**; final
acceptance reach is 149/2,321 = **6.42%** and 149/2,974 = **5.01%** of the full
universe.

Because the planes are disjoint, accepted-loan coverage can be added:
321/2,974 = **10.79%**. This is a reach number only. Their scored precision must
not be pooled.

The retained H.7 measurements therefore define a `retained_complete` contract:
35 non-round plus 14 round multi-BBL loan subjects, or 49 unique accepted loans.
A typed artifact may claim that scope only when supplied rows include both
pinned MapPLUTO candidate releases per subject (98 release-run rows), matching
payload row counts, preserved syntactically validated source hashes, and cited
non-fixture SQL-bound receipts. Canon validates source-hash syntax and source/hash-kind
identity uniqueness but does not recompute source bytes in this offline
materializer. The current checked fixture is only `fixture_subset` with 1+1
subjects. The 49 subjects and 98 release rows do not satisfy the frozen E4
target of 79 genuine cases.

### G7.7 Scoring Contract And Separate Association Planes

MapPLUTO scoring uses a bounding-box prefilter followed by exact
`ST_CONTAINS`. The two pinned releases are scored independently. No MapPLUTO
address field enters the prediction.

For each exact geocode point, the latest `ASOF` observation is used. When the
latest observation contains more than one accuracy type, its tier is `mixed`.
Observed as-of dates range from 2025-01-01 through 2026-08-01.

An ACRIS BBL is attached only to collateral rows whose raw filed borough agrees
with the legal borough. Truth is split by the number of property keys attached
within the truth boroughs:

- `single_property`: the loan-level document truth has one collateral property
  association in scope.
- `multi_property`: the document supplies a set of BBLs for a loan with multiple
  property keys. Set-valued overlap is a lenient upper plane; it does not prove
  which BBL belongs to which property key.

That split cannot be pooled. Otherwise every BBL on a multi-property mortgage
would be copied onto every collateral and the measurement would silently award
correctness without resolving loan-to-property incidence.

G7.7's association-plane split is separate from G7.6's multi-BBL population.
The 57 non-round and 51 round multi-property class loans are property-key
association strata on accepted rows; they are not interchangeable with the
35/14 accepted ACRIS multi-BBL truth subjects.

### G7.8 Point-Grain Results

`PIP reached` is candidate reach on the accepted truth slice. Lot and block
precision use `PIP reached` as denominator and remain distinct from truth-gate
acceptance reach in G7.6.

| truth plane | association | class loans | truth points | PIP reached | reach | lot correct | lot precision | block correct | block precision |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| non-round | single | 115 | 104 | 100 | 96.15% | 59 | 59.00% | 67 | 67.00% |
| non-round | multi | 57 | 153 | 148 | 96.73% | 127 | 85.81% | 133 | 89.86% |
| round exact lender | single | 98 | 94 | 93 | 98.94% | 69 | 74.19% | 79 | 84.95% |
| round exact lender | multi | 51 | 99 | 93 | 93.94% | 71 | 76.34% | 82 | 88.17% |

The non-round 59.00%/85.81% single/multi gap is load-bearing evidence that a
pooled score would hide association ambiguity. The round exact-lender plane is
steadier at 74.19%/76.34%, but it is still a selected sensitivity plane, not
independent adjudication.

### G7.9 Property-Key Results

| truth plane | association | property units | geocoded | PIP reached | lot correct | lot precision | block correct | block precision |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| non-round | single | 115 | 113 | 109 | 67 | 61.47% | 75 | 68.81% |
| non-round | multi | 161 | 159 | 153 | 131 | 85.62% | 137 | 89.54% |
| round exact lender | single | 98 | 97 | 96 | 72 | 75.00% | 82 | 85.42% |
| round exact lender | multi | 112 | 110 | 104 | 78 | 75.00% | 90 | 86.54% |

Property-key and exact-point grains answer different questions and are retained
separately. Neither changes the admission reach in G7.6.

### G7.10 Representativeness And Accuracy Strata

Accepted-loan filed-borough distribution is sharply different by plane:

| plane | Manhattan | Bronx | Brooklyn | Queens | multi-borough | total |
|---|---:|---:|---:|---:|---:|---:|
| non-round | 77 | 30 | 26 | 37 | 2 | 172 |
| round exact lender | 146 | 0 | 0 | 3 | 0 | 149 |

The round exact-lender gate improves truth coverage but not representativeness;
98.0% of its accepts are Manhattan-only.

Selected 26v2 point-grain accuracy strata are shown with exact denominators:

| plane / association | accuracy tier | lot correct / reached | block correct / reached |
|---|---|---:|---:|
| non-round / single | nearest_rooftop_match | 9/13 = 69.23% | 9/13 = 69.23% |
| non-round / single | rooftop | 49/85 = 57.65% | 57/85 = 67.06% |
| non-round / multi | nearest_rooftop_match | 10/17 = 58.82% | 13/17 = 76.47% |
| non-round / multi | rooftop | 113/125 = 90.40% | 116/125 = 92.80% |
| round lender / single | nearest_rooftop_match | 10/12 = 83.33% | 11/12 = 91.67% |
| round lender / single | rooftop | 56/76 = 73.68% | 63/76 = 82.89% |
| round lender / multi | nearest_rooftop_match | 5/7 = 71.43% | 5/7 = 71.43% |
| round lender / multi | rooftop | 64/79 = 81.01% | 74/79 = 93.67% |

The remaining range/place/intersection/street cells are small. They are retained
in the query result with their exact denominators and are not generalized from.

### G7.11 Release Sensitivity And Calibrated Conclusion

There were 57 comparable scored strata across MapPLUTO 26v1 and 26v2 and zero
metric differences. This proves equality only for these rows and these
aggregates. It does not prove the releases are globally equal or interchangeable.

What the evidence supports:

- Filed-county admission removes the known geocoder-borough feedback from the
  ACRIS truth gate.
- Exact lender/party equality recovers a round-amount truth plane without
  pretending free-text similarity is exact identity.
- Candidate reach remains low: 26.34% on the non-round plane and 6.42% on the
  round exact-lender plane. This is upstream of solver correctness.
- PIP precision depends strongly on the association plane. The 59.00%
  single-property non-round result and 85.81% multi-property set-valued result
  must not be combined.
- The exact-lender plane is useful corroboration and coverage, not independent
  truth, and it is overwhelmingly Manhattan-selected.
- These measurements grade a geometry-only PIP baseline against document truth.
  They do not establish solver soundness, completeness, confluence, or world
  truth.

What remains open before a release precision claim:

- stratified human adjudication from rendered geometry without reading the
  address channel;
- explicit disagreement reporting between filed ACRIS, address-derived PLUTO,
  and adjudication truth planes;
- validation of any broader lender-name normalization before it can admit
  candidates;
- exact loan-to-property association evidence for multi-property mortgages.

The measurement advances the evidence-stacking ambition precisely because it
does not collapse these unresolved distinctions. It narrows the admissible
truth set, exposes when that set is empty or weak, and leaves each evidence plane
available for later composition without laundering correlation into certainty.
