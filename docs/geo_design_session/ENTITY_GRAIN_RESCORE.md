# bd-s3i9 Entity-Grain Re-score

Measurement date: 2026-08-16.

Mode: Loom SQL only. Numbers below are from `--raw` structured
`tool_responses[*].structuredContent`, not Loom prose. ACRIS is used only as the
Gate V2 truth instrument.

## Result

The Gate V2 / E1 rebuild reconciles to the known labeled denominator:

| denominator | points |
|---|---:|
| PIP-covered Gate V2 points | 233 |
| E1 correct sanity class | 154 |
| E1 failures | 79 |
| E1 failure partition | 40 + 32 + 2 + 2 + 3 = 79 |

Entity predicate used for M1/M2:

```sql
ledger_correct := lot_hits > 0
entity_correct := ledger_correct OR failure_class = 'condo_representation_residue'
```

The `condo_representation_residue` class is the E1 class with PIP coverage,
ledger BBL miss, ACRIS condo-unit truth signature, and missing MapPLUTO unit
geometry, after the gross-geocode class has already been removed by priority.
That is the L.5 representation bridge: the BBL is an alias projection, and the
unavailable unit-ledger projection does not make the parcel/building answer
wrong. With the landed data, parcel and building grain are the same measurable
predicate because no direct condo-unit-to-BIN crosswalk is landed.

### M1: Ledger vs Parcel vs Building

Structured result:

| class | points | ledger correct | ledger precision | parcel correct | parcel precision | building correct | building precision | entity flips |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| ALL | 233 | 154 | 66.09% | 186 | 79.83% | 186 | 79.83% | 32 |
| no_failure_class | 154 | 154 | 100.00% | 154 | 100.00% | 154 | 100.00% | 0 |
| gross_geocode_error_gt500m | 40 | 0 | 0.00% | 0 | 0.00% | 0 | 0.00% | 0 |
| condo_representation_residue | 32 | 0 | 0.00% | 32 | 100.00% | 32 | 100.00% | 32 |
| assemblage_artifact_neighbor | 2 | 0 | 0.00% | 0 | 0.00% | 0 | 0.00% | 0 |
| adjacent_lot_near_miss | 2 | 0 | 0.00% | 0 | 0.00% | 0 | 0.00% | 0 |
| residual_truth_contamination_suspect | 3 | 0 | 0.00% | 0 | 0.00% | 0 | 0.00% | 0 |

### M2: Precision On Answered

Structured result:

| policy | answered points | ledger correct | ledger precision | parcel/building correct | parcel/building precision | entity flips |
|---|---:|---:|---:|---:|---:|---:|
| answer all, no abstention | 233 | 154 | 66.09% | 186 | 79.83% | 32 |
| abstain gross-for-retry | 193 | 154 | 79.79% | 186 | 96.37% | 32 |
| abstain gross, exclude 3 contamination suspects | 190 | 154 | 81.05% | 186 | 97.89% | 32 |

### M3: Already-Landed Retry Ceiling

Bridge: `WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED` has no `PROPERTY_KEY`, so I used
exact `UPPER(TRIM(PROPERTY_ADDRESS))` plus `COUNTY_FIPS` to find already-landed
alternate geocode rows. Alternates required different coordinates and different
`SOURCE` or `ASOF`. This is a ceiling from landed alternates only; no external
geocoder calls were made.

Structured result:

| bucket | gross points | with geocode key | with alternate point | with different-r9 alternate | with alternate PIP | with alternate PIP truth block | with different-r9 alternate PIP truth block | alternate points | different-r9 alternate points | retry ceiling |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ALL | 40 | 40 | 11 | 0 | 11 | 0 | 0 | 11 | 0 | 0.00% |
| nearest_rooftop_match | 7 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.00% |
| place | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.00% |
| rooftop | 30 | 30 | 11 | 0 | 11 | 0 | 0 | 11 | 0 | 0.00% |
| street_center | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.00% |

Finding: the landed alternate geocode rows do not bound any recovery in this
slice. The retry-loop ceiling from already-landed alternates is `0 / 40 =
0.00%`; actual recovery requires reacquisition, not just selecting another
currently landed row.

## Schema Discovery

Structured schema result for the geocode table returned 44 columns. Relevant to
M3: the table has `PROPERTY_NAME`, `PROPERTY_ADDRESS`, city/state/zip/county
fields, `LATITUDE`, `LONGITUDE`, `ACCURACY_TYPE`, `SOURCE`, and `ASOF`; it does
not expose `PROPERTY_KEY` or `LOAN_KEY`.

Exact SQL:

```sql
SELECT column_name,data_type,ordinal_position
FROM EDGAR_DB.INFORMATION_SCHEMA.COLUMNS
WHERE table_schema='DBT_WRANGLING_EDGAR'
  AND table_name='WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED'
ORDER BY ordinal_position;
```

## Exact SQL

M1/M2 exact SQL:

```sql
WITH l AS(SELECT DISTINCT li.LOAN_KEY k,TO_DATE(li.ORIGINATIONDATE) od,ROUND(li.ORIGINALLOANAMOUNT,2) amt FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li ON p.CIK=li.CIK AND p.ASSETNUMBER=li.ASSETNUMBER WHERE p.COUNTY_FIPS IN('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE AND li.ORIGINATIONDATE IS NOT NULL AND li.ORIGINALLOANAMOUNT IS NOT NULL AND MOD(ROUND(li.ORIGINALLOANAMOUNT,2),100000)<>0),
b AS(SELECT DISTINCT li.LOAN_KEY k,CASE TO_VARCHAR(p.COUNTY_FIPS) WHEN '36061' THEN 1 WHEN '36005' THEN 2 WHEN '36047' THEN 3 WHEN '36081' THEN 4 WHEN '36085' THEN 5 END bo FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li ON p.CIK=li.CIK AND p.ASSETNUMBER=li.ASSETNUMBER WHERE p.COUNTY_FIPS IN('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE),
m AS(SELECT DISTINCT DOCUMENT_ID d,TO_DATE(RECORDED_DATETIME) rd,RECORDED_BOROUGH rb,ROUND(DOCUMENT_AMT,2) amt FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT WHERE RELEASE_DT='2026-08-10' AND RECORDED_BOROUGH IN(1,2,3,4,5) AND DOC_TYPE IN('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD') AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000 AND TO_DATE(RECORDED_DATETIME) BETWEEN (SELECT MIN(od) FROM l) AND DATEADD(day,45,(SELECT MAX(od) FROM l))),
c AS(SELECT DISTINCT l.k,m.d,DATEDIFF(day,l.od,m.rd) off FROM l JOIN m ON m.amt=l.amt AND m.rd BETWEEN l.od AND DATEADD(day,45,l.od) JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON m.d=le.DOCUMENT_ID JOIN b ON l.k=b.k AND le.BOROUGH=b.bo WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN(1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL),
cc AS(SELECT k,COUNT(DISTINCT d) n FROM c GROUP BY k),
a AS(SELECT c.* FROM c JOIN cc USING(k) WHERE n=1),
ab AS(SELECT DISTINCT a.k,a.d,a.off,TO_VARCHAR(le.BOROUGH)||LPAD(TO_VARCHAR(le.BLOCK),5,'0')||LPAD(TO_VARCHAR(le.LOT),4,'0') bbl,TO_VARCHAR(le.BOROUGH)||LPAD(TO_VARCHAR(le.BLOCK),5,'0') blk,IFF(TRY_TO_NUMBER(le.LOT) BETWEEN 1001 AND 6999,1,0) co FROM a JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON a.d=le.DOCUMENT_ID WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN(1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL),
ps AS(SELECT DISTINCT p.PROPERTY_KEY pk,li.LOAN_KEY k FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li ON p.CIK=li.CIK AND p.ASSETNUMBER=li.ASSETNUMBER WHERE p.COUNTY_FIPS IN('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE),
td AS(SELECT DISTINCT ps.pk,ab.k,ab.bbl,ab.blk,ab.co,d.LATITUDE la,d.LONGITUDE lo FROM ab JOIN ps USING(k) JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d ON ps.pk=d.PROPERTY_KEY WHERE d.COUNTY_FIPS IN('36005','36047','36061','36081','36085') AND d.LATITUDE IS NOT NULL AND d.LONGITUDE IS NOT NULL),
pt AS(SELECT g.LATITUDE la,g.LONGITUDE lo,IFF(COUNT(DISTINCT g.ACCURACY_TYPE)=1,MIN(g.ACCURACY_TYPE),'mixed') acc FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED g JOIN (SELECT DISTINCT la,lo FROM td) x ON g.LATITUDE=x.la AND g.LONGITUDE=x.lo WHERE g.COUNTY_FIPS IN('36005','36047','36061','36081','36085') GROUP BY g.LATITUDE,g.LONGITUDE),
tb AS(SELECT DISTINCT pt.la,pt.lo,pt.acc,td.bbl,td.blk,td.co FROM pt JOIN td ON pt.la=td.la AND pt.lo=td.lo),
tp AS(SELECT DISTINCT la,lo,acc FROM tb),
pe AS(SELECT DISTINCT tp.la,tp.lo,REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$','') pb,SUBSTR(REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$',''),1,6) pk,pl.GEOM_GEOG pg FROM tp JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON ST_CONTAINS(pl.GEOM_GEOG,ST_POINT(tp.lo,tp.la))),
tg AS(SELECT tb.la,tb.lo,tb.bbl,pl.GEOM_GEOG tg FROM tb LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$','')=tb.bbl),
e AS(SELECT tb.la,tb.lo,tb.acc,pe.pb,pe.pk,tb.bbl,tb.blk,tb.co,IFF(tg.tg IS NULL,1,0) miss,ST_DISTANCE(pe.pg,tg.tg) pd,ST_DISTANCE(ST_POINT(tb.lo,tb.la),tg.tg) td FROM tb JOIN pe ON tb.la=pe.la AND tb.lo=pe.lo LEFT JOIN tg ON tb.la=tg.la AND tb.lo=tg.lo AND tb.bbl=tg.bbl),
r AS(SELECT la,lo,acc,COUNT(DISTINCT bbl) tbbl,COUNT(DISTINCT pb) pp,COUNT(DISTINCT IFF(pb=bbl,pb,NULL)) lh,COUNT(DISTINCT IFF(pk=blk,pk,NULL)) bh,MAX(co) co,MIN(pd) mpd,MIN(td) mtd,SUM(miss) miss FROM e GROUP BY la,lo,acc),
s AS(SELECT *,CASE WHEN pp>0 AND lh=0 AND COALESCE(mtd,mpd)>500 THEN 'gross_geocode_error_gt500m' WHEN pp>0 AND lh=0 AND co=1 AND miss>0 THEN 'condo_representation_residue' WHEN pp>0 AND lh=0 AND tbbl>1 AND (mpd<=25 OR bh>0) THEN 'assemblage_artifact_neighbor' WHEN pp>0 AND lh=0 AND (mpd<=25 OR bh>0) THEN 'adjacent_lot_near_miss' WHEN pp>0 AND lh=0 THEN 'residual_truth_contamination_suspect' ELSE 'no_failure_class' END fc,IFF(lh>0,1,0) led FROM r WHERE pp>0),
z AS(SELECT *,IFF(fc='condo_representation_residue',1,0) flip,IFF(led=1 OR fc='condo_representation_residue',1,0) ent FROM s),
out AS(SELECT 'M1_BY_CLASS' sec,'ALL' bucket,COUNT(*) pts,SUM(led) ledger,ROUND(100*SUM(led)/COUNT(*),2) ledger_pct,SUM(ent) parcel,ROUND(100*SUM(ent)/COUNT(*),2) parcel_pct,SUM(ent) building,ROUND(100*SUM(ent)/COUNT(*),2) building_pct,SUM(flip) flips FROM z UNION ALL SELECT 'M1_BY_CLASS',fc,COUNT(*),SUM(led),ROUND(100*SUM(led)/COUNT(*),2),SUM(ent),ROUND(100*SUM(ent)/COUNT(*),2),SUM(ent),ROUND(100*SUM(ent)/COUNT(*),2),SUM(flip) FROM z GROUP BY fc UNION ALL SELECT 'M2_POLICY','answer_all_no_abstention',COUNT(*),SUM(led),ROUND(100*SUM(led)/COUNT(*),2),SUM(ent),ROUND(100*SUM(ent)/COUNT(*),2),SUM(ent),ROUND(100*SUM(ent)/COUNT(*),2),SUM(flip) FROM z UNION ALL SELECT 'M2_POLICY','answered_abstain_gross',COUNT(*),SUM(led),ROUND(100*SUM(led)/COUNT(*),2),SUM(ent),ROUND(100*SUM(ent)/COUNT(*),2),SUM(ent),ROUND(100*SUM(ent)/COUNT(*),2),SUM(flip) FROM z WHERE fc<>'gross_geocode_error_gt500m' UNION ALL SELECT 'M2_POLICY','answered_abstain_gross_exclude_3_contam_suspects',COUNT(*),SUM(led),ROUND(100*SUM(led)/COUNT(*),2),SUM(ent),ROUND(100*SUM(ent)/COUNT(*),2),SUM(ent),ROUND(100*SUM(ent)/COUNT(*),2),SUM(flip) FROM z WHERE fc NOT IN('gross_geocode_error_gt500m','residual_truth_contamination_suspect'))
SELECT * FROM out ORDER BY sec,CASE bucket WHEN 'ALL' THEN 0 WHEN 'no_failure_class' THEN 1 WHEN 'gross_geocode_error_gt500m' THEN 2 WHEN 'condo_representation_residue' THEN 3 WHEN 'assemblage_artifact_neighbor' THEN 4 WHEN 'adjacent_lot_near_miss' THEN 5 WHEN 'residual_truth_contamination_suspect' THEN 6 WHEN 'answer_all_no_abstention' THEN 10 WHEN 'answered_abstain_gross' THEN 11 ELSE 12 END;
```

M3 exact SQL:

```sql
WITH l AS(SELECT DISTINCT li.LOAN_KEY k,TO_DATE(li.ORIGINATIONDATE) od,ROUND(li.ORIGINALLOANAMOUNT,2) amt FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li ON p.CIK=li.CIK AND p.ASSETNUMBER=li.ASSETNUMBER WHERE p.COUNTY_FIPS IN('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE AND li.ORIGINATIONDATE IS NOT NULL AND li.ORIGINALLOANAMOUNT IS NOT NULL AND MOD(ROUND(li.ORIGINALLOANAMOUNT,2),100000)<>0),
b AS(SELECT DISTINCT li.LOAN_KEY k,CASE TO_VARCHAR(p.COUNTY_FIPS) WHEN '36061' THEN 1 WHEN '36005' THEN 2 WHEN '36047' THEN 3 WHEN '36081' THEN 4 WHEN '36085' THEN 5 END bo FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li ON p.CIK=li.CIK AND p.ASSETNUMBER=li.ASSETNUMBER WHERE p.COUNTY_FIPS IN('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE),
m AS(SELECT DISTINCT DOCUMENT_ID d,TO_DATE(RECORDED_DATETIME) rd,ROUND(DOCUMENT_AMT,2) amt FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT WHERE RELEASE_DT='2026-08-10' AND RECORDED_BOROUGH IN(1,2,3,4,5) AND DOC_TYPE IN('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD') AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000 AND TO_DATE(RECORDED_DATETIME) BETWEEN (SELECT MIN(od) FROM l) AND DATEADD(day,45,(SELECT MAX(od) FROM l))),
c AS(SELECT DISTINCT l.k,m.d FROM l JOIN m ON m.amt=l.amt AND m.rd BETWEEN l.od AND DATEADD(day,45,l.od) JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON m.d=le.DOCUMENT_ID JOIN b ON l.k=b.k AND le.BOROUGH=b.bo WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN(1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL),
cc AS(SELECT k,COUNT(DISTINCT d) n FROM c GROUP BY k),a AS(SELECT c.* FROM c JOIN cc USING(k) WHERE n=1),
ab AS(SELECT DISTINCT a.k,a.d,TO_VARCHAR(le.BOROUGH)||LPAD(TO_VARCHAR(le.BLOCK),5,'0')||LPAD(TO_VARCHAR(le.LOT),4,'0') bbl,TO_VARCHAR(le.BOROUGH)||LPAD(TO_VARCHAR(le.BLOCK),5,'0') blk,IFF(TRY_TO_NUMBER(le.LOT) BETWEEN 1001 AND 6999,1,0) co FROM a JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT le ON a.d=le.DOCUMENT_ID WHERE le.RELEASE_DT='2026-08-10' AND le.BOROUGH IN(1,2,3,4,5) AND le.BLOCK IS NOT NULL AND le.LOT IS NOT NULL),
ps AS(SELECT DISTINCT p.PROPERTY_KEY pk,li.LOAN_KEY k FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li ON p.CIK=li.CIK AND p.ASSETNUMBER=li.ASSETNUMBER WHERE p.COUNTY_FIPS IN('36005','36047','36061','36081','36085') AND p.HAS_LOAN=TRUE),
td AS(SELECT DISTINCT ps.pk,ab.k,ab.bbl,ab.blk,ab.co,d.LATITUDE la,d.LONGITUDE lo FROM ab JOIN ps USING(k) JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d ON ps.pk=d.PROPERTY_KEY WHERE d.COUNTY_FIPS IN('36005','36047','36061','36081','36085') AND d.LATITUDE IS NOT NULL AND d.LONGITUDE IS NOT NULL),
pt AS(SELECT g.LATITUDE la,g.LONGITUDE lo,IFF(COUNT(DISTINCT g.ACCURACY_TYPE)=1,MIN(g.ACCURACY_TYPE),'mixed') acc FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED g JOIN (SELECT DISTINCT la,lo FROM td) x ON g.LATITUDE=x.la AND g.LONGITUDE=x.lo WHERE g.COUNTY_FIPS IN('36005','36047','36061','36081','36085') GROUP BY g.LATITUDE,g.LONGITUDE),
tb AS(SELECT DISTINCT pt.la,pt.lo,pt.acc,td.bbl,td.blk,td.co FROM pt JOIN td ON pt.la=td.la AND pt.lo=td.lo),tp AS(SELECT DISTINCT la,lo,acc FROM tb),
pe AS(SELECT DISTINCT tp.la,tp.lo,REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$','') pb,SUBSTR(REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$',''),1,6) pk,pl.GEOM_GEOG pg FROM tp JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON ST_CONTAINS(pl.GEOM_GEOG,ST_POINT(tp.lo,tp.la))),
tg AS(SELECT tb.la,tb.lo,tb.bbl,pl.GEOM_GEOG tg FROM tb LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$','')=tb.bbl),
e AS(SELECT tb.la,tb.lo,tb.acc,pe.pb,pe.pk,tb.bbl,tb.blk,tb.co,IFF(tg.tg IS NULL,1,0) miss,ST_DISTANCE(pe.pg,tg.tg) pd,ST_DISTANCE(ST_POINT(tb.lo,tb.la),tg.tg) td FROM tb JOIN pe ON tb.la=pe.la AND tb.lo=pe.lo LEFT JOIN tg ON tb.la=tg.la AND tb.lo=tg.lo AND tb.bbl=tg.bbl),
r AS(SELECT la,lo,acc,COUNT(DISTINCT bbl) tbbl,COUNT(DISTINCT pb) pp,COUNT(DISTINCT IFF(pb=bbl,pb,NULL)) lh,COUNT(DISTINCT IFF(pk=blk,pk,NULL)) bh,MAX(co) co,MIN(pd) mpd,MIN(td) mtd,SUM(miss) miss FROM e GROUP BY la,lo,acc),
s AS(SELECT *,CASE WHEN pp>0 AND lh=0 AND COALESCE(mtd,mpd)>500 THEN 'gross_geocode_error_gt500m' WHEN pp>0 AND lh=0 AND co=1 AND miss>0 THEN 'condo_representation_residue' WHEN pp>0 AND lh=0 AND tbbl>1 AND (mpd<=25 OR bh>0) THEN 'assemblage_artifact_neighbor' WHEN pp>0 AND lh=0 AND (mpd<=25 OR bh>0) THEN 'adjacent_lot_near_miss' WHEN pp>0 AND lh=0 THEN 'residual_truth_contamination_suspect' ELSE 'no_failure_class' END fc FROM r WHERE pp>0),
g AS(SELECT la,lo,acc,H3_POINT_TO_CELL_STRING(ST_POINT(lo,la),9) gc FROM s WHERE fc='gross_geocode_error_gt500m'),
gk AS(SELECT DISTINCT g.la,g.lo,g.acc,UPPER(TRIM(w.PROPERTY_ADDRESS)) addr,w.COUNTY_FIPS cf,COALESCE(TO_VARCHAR(w.SOURCE),'') s0,COALESCE(TO_VARCHAR(w.ASOF),'') a0 FROM g JOIN EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED w ON g.la=w.LATITUDE AND g.lo=w.LONGITUDE WHERE w.COUNTY_FIPS IN('36005','36047','36061','36081','36085') AND w.PROPERTY_ADDRESS IS NOT NULL),
al AS(SELECT DISTINCT gk.la,gk.lo,gk.acc,w.LATITUDE ala,w.LONGITUDE alo,H3_POINT_TO_CELL_STRING(ST_POINT(w.LONGITUDE,w.LATITUDE),9) ac FROM gk JOIN EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED w ON UPPER(TRIM(w.PROPERTY_ADDRESS))=gk.addr AND w.COUNTY_FIPS=gk.cf WHERE w.LATITUDE IS NOT NULL AND w.LONGITUDE IS NOT NULL AND (w.LATITUDE<>gk.la OR w.LONGITUDE<>gk.lo) AND (COALESCE(TO_VARCHAR(w.SOURCE),'')<>gk.s0 OR COALESCE(TO_VARCHAR(w.ASOF),'')<>gk.a0)),
ap AS(SELECT DISTINCT al.la,al.lo,al.ala,al.alo,al.ac,SUBSTR(REGEXP_REPLACE(TO_VARCHAR(pl.BBL),'\\.0$',''),1,6) pblk FROM al JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT pl ON ST_CONTAINS(pl.GEOM_GEOG,ST_POINT(al.alo,al.ala))),
f AS(SELECT g.la,g.lo,g.acc,COUNT(DISTINCT gk.addr) key_count,COUNT(DISTINCT TO_VARCHAR(al.ala)||','||TO_VARCHAR(al.alo)) alt_points,COUNT(DISTINCT IFF(al.ac<>g.gc,TO_VARCHAR(al.ala)||','||TO_VARCHAR(al.alo),NULL)) diff_tile_alt_points,COUNT(DISTINCT TO_VARCHAR(ap.ala)||','||TO_VARCHAR(ap.alo)) alt_points_with_pip,COUNT(DISTINCT IFF(ap.pblk=tb.blk,TO_VARCHAR(ap.ala)||','||TO_VARCHAR(ap.alo),NULL)) alt_points_truth_block_pip,COUNT(DISTINCT IFF(al.ac<>g.gc AND ap.pblk=tb.blk,TO_VARCHAR(ap.ala)||','||TO_VARCHAR(ap.alo),NULL)) diff_tile_alt_points_truth_block_pip FROM g LEFT JOIN gk ON g.la=gk.la AND g.lo=gk.lo LEFT JOIN al ON g.la=al.la AND g.lo=al.lo LEFT JOIN ap ON al.la=ap.la AND al.lo=ap.lo AND al.ala=ap.ala AND al.alo=ap.alo LEFT JOIN tb ON g.la=tb.la AND g.lo=tb.lo GROUP BY g.la,g.lo,g.acc),
o AS(SELECT 'ALL' bucket,COUNT(*) gross_points,SUM(IFF(key_count>0,1,0)) with_geocode_key,SUM(IFF(alt_points>0,1,0)) with_alt_point,SUM(IFF(diff_tile_alt_points>0,1,0)) with_diff_tile_alt,SUM(IFF(alt_points_with_pip>0,1,0)) with_alt_pip,SUM(IFF(alt_points_truth_block_pip>0,1,0)) with_alt_truth_block_pip,SUM(IFF(diff_tile_alt_points_truth_block_pip>0,1,0)) with_diff_tile_alt_truth_block_pip,SUM(alt_points) alt_points,SUM(diff_tile_alt_points) diff_tile_alt_points,SUM(alt_points_truth_block_pip) alt_truth_block_pip_points,SUM(diff_tile_alt_points_truth_block_pip) diff_tile_alt_truth_block_pip_points FROM f UNION ALL SELECT acc,COUNT(*),SUM(IFF(key_count>0,1,0)),SUM(IFF(alt_points>0,1,0)),SUM(IFF(diff_tile_alt_points>0,1,0)),SUM(IFF(alt_points_with_pip>0,1,0)),SUM(IFF(alt_points_truth_block_pip>0,1,0)),SUM(IFF(diff_tile_alt_points_truth_block_pip>0,1,0)),SUM(alt_points),SUM(diff_tile_alt_points),SUM(alt_points_truth_block_pip),SUM(diff_tile_alt_points_truth_block_pip) FROM f GROUP BY acc)
SELECT *,ROUND(100*with_diff_tile_alt_truth_block_pip/NULLIF(gross_points,0),2) retry_ceiling_pct FROM o ORDER BY IFF(bucket='ALL',0,1),bucket;
```

## Failed Or Discarded Queries

| query shape | structured outcome | disposition |
|---|---|---|
| first full M1/M2 query | Snowflake compile error: invalid identifier `LEDGER_CORRECT` | fixed by splitting score aliases into a separate CTE |
| second full M1/M2 query | Loom validation error: message over 10,000 characters | compacted SQL |
| first compact M1/M2 query | Snowflake canceled `000604` timeout | narrowed spatial join to Gate V2 truth points and restored recorded-date filter |
| regex escape probe | no `tool_responses` structured payload | discarded; no cited measurement number |
