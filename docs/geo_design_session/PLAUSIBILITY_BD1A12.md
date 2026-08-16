# bd-1a12 Geocode Plausibility Measurement

Agent: PearlSparrow  
Bead: `bd-1a12`  
Date: 2026-08-16  
Access: Loom SQL only, `cmdrvl --json orchestrator query --tenant salt --raw`; numbers below come from `tool_responses[*].structuredContent`.

## Finding

Tile-local evidence is real, but the measured discriminators do **not** cleanly separate the Gate V2 labeled PIP population.

The labeled denominator is the Gate V2 geometry-PIP covered truth set from Appendix H.6: 233 points, split into 154 lot-correct and 79 lot-incorrect PIP answers. On that set:

| discriminator | fire means | denominator | fire correct | fire incorrect | no-fire correct | no-fire incorrect | readout |
|---|---|---:|---:|---:|---:|---:|---|
| street_present | some parsed street at the point appears in the r9+k1 MapPLUTO tile | 233 | 153 | 78 | 1 | 1 | no separation |
| number_in_range | some parsed house number falls in observed same-street PIP-block range | 233 | 114 | 51 | 40 | 28 | weak separation |
| street_present_and_number_in_range | both above | 233 | 114 | 51 | 40 | 28 | same as number range because street is saturated |
| house_number_agree_with_pip_lot | parsed street+number equals PIP lot address | 233 | 123 | 52 | 31 | 27 | weak separation |
| nyc_footprint_majority_on_pip_lot | NYC footprint majority-linked to PIP lot | 233 | 153 | 79 | 1 | 0 | saturated |
| fema_structure_majority_on_pip_lot | FEMA structure majority-linked to PIP lot | 233 | 63 | 28 | 91 | 51 | non-separating |
| depth_lt_3m | point is within 3m of PIP lot boundary | 233 | 74 | 51 | 80 | 28 | catches wrongs but false-refutes many rights |
| parity_match_where_range_derivable | odd/even parity matches observed side where derivable | 233 | 68 | 25 | 86 | 54 | no independent power; prior strict blockface run found zero parity mismatches where derivable |

The best refuting signal on the labeled set is boundary-shallow or number-out-of-range, but both are too blunt:

| refuter | wrong points refuted | correct points falsely refuted |
|---|---:|---:|
| not number_in_range | 28 / 79 = 35.44% | 40 / 154 = 25.97% |
| depth_lt_3m | 51 / 79 = 64.56% | 74 / 154 = 48.05% |
| no FEMA structure | 51 / 79 = 64.56% | 91 / 154 = 59.09% |
| no NYC footprint | 0 / 79 = 0.00% | 1 / 154 = 0.65% |

This is evidence against a simple deterministic cascade rule like "accept if the tile has a footprint and the street exists." The facts either fire on nearly everything or punish too many known-correct points.

## P1 Street Presence

Definition: point-grain, distinct `(LATITUDE, LONGITUDE)`. A point is street-supported when **any** parsed `STREET` attached to that point appears on any MapPLUTO parcel address whose centroid falls in the point's H3 r9 cell plus k-ring 1. MapPLUTO `H3_R8` is integer, so the query uses `H3_INT_TO_STRING(m.H3_R8)` and recomputes parcel centroid r9.

Appendix E normalizer used in SQL: uppercase, punctuation to spaces, directionals and suffixes normalized, numeric ordinal suffixes stripped, spelled ordinals 1-12 normalized.

Full 4,076-point universe:

| accuracy_type | denominator | computable | no parsed street | street present | street absent / refuted | refute rate |
|---|---:|---:|---:|---:|---:|---:|
| ALL | 4,076 | 4,046 | 30 | 404 | 3,642 | 90.01% |
| intersection | 19 | 19 | 0 | 2 | 17 | 89.47% |
| mixed | 87 | 87 | 0 | 6 | 81 | 93.10% |
| nearest_rooftop_match | 344 | 344 | 0 | 30 | 314 | 91.28% |
| place | 30 | 0 | 30 | 0 | 0 | n/a |
| range_interpolation | 315 | 315 | 0 | 35 | 280 | 88.89% |
| rooftop | 3,216 | 3,216 | 0 | 322 | 2,894 | 89.99% |
| street_center | 65 | 65 | 0 | 9 | 56 | 86.15% |

On the Gate V2 labeled set:

| outcome | denominator | street present | street absent / refuted | refute rate |
|---|---:|---:|---:|---:|
| lot_correct | 154 | 153 | 1 | 0.65% |
| lot_incorrect | 79 | 78 | 1 | 1.27% |

Interpretation: street presence is not a discriminator on the labeled truth set, and street absence would impose a catastrophic abstention rate on the full universe under this centroid-r9 tile definition.

P1 SQL:

```sql
WITH r AS (SELECT LATITUDE lat,LONGITUDE lon,ACCURACY_TYPE,STREET FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED WHERE COUNTY_FIPS IN('36005','36047','36061','36081','36085') AND LATITUDE IS NOT NULL AND LONGITUDE IS NOT NULL),
pts AS (SELECT lat,lon,IFF(COUNT(DISTINCT ACCURACY_TYPE)=1,MIN(ACCURACY_TYPE),'mixed') acc FROM r GROUP BY lat,lon),
h AS (SELECT pts.lat,pts.lon,acc,f.value::STRING cell FROM pts,LATERAL FLATTEN(INPUT=>H3_GRID_DISK(H3_POINT_TO_CELL_STRING(ST_POINT(lon,lat),9),1)) f),
p AS (SELECT DISTINCT h.lat,h.lon,m.ADDRESS FROM h JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT m ON H3_INT_TO_STRING(m.H3_R8)=H3_CELL_TO_PARENT(h.cell,8) AND H3_POINT_TO_CELL_STRING(m.CENTROID_GEOG,9)=h.cell),
s AS (SELECT 'i' typ,lat,lon,STREET raw_street FROM r UNION ALL SELECT 'p',lat,lon,REGEXP_REPLACE(ADDRESS,'^\\s*[0-9]+[A-Z]?(\\s*[-/]\\s*[0-9]+[A-Z]?)?\\s+','') FROM p),
tok AS (SELECT typ,lat,lon,raw_street,t.index,DECODE(t.value,'NORTH','N','SOUTH','S','EAST','E','WEST','W','STREET','ST','AVENUE','AVE','ROAD','RD','BOULEVARD','BLVD','PLACE','PL','DRIVE','DR','LANE','LN','COURT','CT','PARKWAY','PKWY','HIGHWAY','HWY','TERRACE','TER','CIRCLE','CIR','EXPRESSWAY','EXPY','PLAZA','PLZ','FIRST','1','SECOND','2','THIRD','3','FOURTH','4','FIFTH','5','SIXTH','6','SEVENTH','7','EIGHTH','8','NINTH','9','TENTH','10','ELEVENTH','11','TWELFTH','12',REGEXP_REPLACE(t.value,'^([0-9]+)(ST|ND|RD|TH)$','\\1')) v FROM s,LATERAL SPLIT_TO_TABLE(REGEXP_REPLACE(UPPER(COALESCE(raw_street,'')),'[^A-Z0-9]+',' '),' ') t WHERE t.value<>''),
n AS (SELECT typ,lat,lon,LISTAGG(v,' ') WITHIN GROUP(ORDER BY index) st FROM tok GROUP BY typ,lat,lon,raw_street),
f AS (SELECT pts.acc,pts.lat,pts.lon,COUNT(DISTINCT i.st) input_streets,MAX(IFF(i.st=p.st,1,0)) present FROM pts LEFT JOIN n i ON pts.lat=i.lat AND pts.lon=i.lon AND i.typ='i' LEFT JOIN n p ON pts.lat=p.lat AND pts.lon=p.lon AND p.typ='p' GROUP BY pts.acc,pts.lat,pts.lon),
ag AS (SELECT acc,COUNT(*) denominator,SUM(IFF(input_streets=0,1,0)) no_parsed_street,SUM(IFF(input_streets>0 AND present=1,1,0)) street_present,SUM(IFF(input_streets>0 AND COALESCE(present,0)=0,1,0)) street_absent_refute FROM f GROUP BY acc UNION ALL SELECT 'ALL',COUNT(*),SUM(IFF(input_streets=0,1,0)),SUM(IFF(input_streets>0 AND present=1,1,0)),SUM(IFF(input_streets>0 AND COALESCE(present,0)=0,1,0)) FROM f)
SELECT acc accuracy_type,denominator,denominator-no_parsed_street computable,no_parsed_street,street_present,street_absent_refute,ROUND(street_absent_refute/NULLIF(denominator-no_parsed_street,0)*100,2) refute_rate_pct FROM ag ORDER BY IFF(acc='ALL',0,1),acc;
```

## P2 House-Number Range

Definition: on the Gate V2 labeled, PIP-covered point set, use the same point-grain "any parsed input row at the point" semantics. A point fires when a parsed house number falls inside the min/max observed MapPLUTO address number range for the same normalized street on the predicted PIP block.

| outcome | denominator | in range | out of range / no range | out-of-range rate |
|---|---:|---:|---:|---:|
| lot_correct | 154 | 114 | 40 | 25.97% |
| lot_incorrect | 79 | 51 | 28 | 35.44% |

This is the best cheap address-channel split, but it is only a weak discriminator: it catches 28 / 79 wrong answers while falsely refuting 40 / 154 correct answers.

Combined street/range SQL used the Gate V2 operating point from Appendix H.6: exact cents, non-round amount, legal borough agreement, `[0,+45]` offset, unique-or-discard after filters. The exact submitted SQL is minified to stay under Loom's 10k message limit; its structured rows were:

| rule | denominator | fire correct | fire incorrect | no-fire correct | no-fire incorrect |
|---|---:|---:|---:|---:|---:|
| number_in_range | 233 | 114 | 51 | 40 | 28 |
| street_present | 233 | 153 | 78 | 1 | 1 |
| street_present_and_number_in_range | 233 | 114 | 51 | 40 | 28 |

## P3 Discriminator Panel

All panel rows use the same 233-point labeled denominator unless noted.

| discriminator | denominator | fire correct | fire incorrect | no-fire correct | no-fire incorrect |
|---|---:|---:|---:|---:|---:|
| street_present | 233 | 153 | 78 | 1 | 1 |
| number_in_range | 233 | 114 | 51 | 40 | 28 |
| street_present_and_number_in_range | 233 | 114 | 51 | 40 | 28 |
| house_number_agree_with_pip_lot | 233 | 123 | 52 | 31 | 27 |
| nyc_footprint_majority_on_pip_lot | 233 | 153 | 79 | 1 | 0 |
| fema_structure_majority_on_pip_lot | 233 | 63 | 28 | 91 | 51 |
| depth_lt_1m | 233 | 1 | 2 | 153 | 77 |
| depth_lt_3m | 233 | 74 | 51 | 80 | 28 |
| depth_lt_5m | 233 | 121 | 68 | 33 | 11 |
| parity_match_where_range_derivable | 233 | 68 | 25 | 86 | 54 |

Depth rows come from the earlier successful structured boundary-distance query in this same bead session. Snowflake lacks `ST_BOUNDARY` and `ST_EXTERIORRING`; the successful query used `ST_ASGEOJSON` exterior-ring expansion into line segments and took the minimum `ST_DISTANCE(point, segment)`.

Selected exact feature SQL, NYC footprint:

```sql
WITH l AS(SELECT DISTINCT l.LOAN_KEY k,CAST(l.ORIGINATIONDATE AS DATE)o,ROUND(l.ORIGINALLOANAMOUNT,2)a,IFF(MOD(ROUND(l.ORIGINALLOANAMOUNT,2),100000)=0,1,0)r FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER WHERE p.COUNTY_FIPS IN('36005','36047','36061','36081','36085')AND p.HAS_LOAN=TRUE AND l.ORIGINATIONDATE IS NOT NULL AND l.ORIGINALLOANAMOUNT IS NOT NULL),
b AS(SELECT DISTINCT l.LOAN_KEY k,CASE TO_VARCHAR(p.COUNTY_FIPS)WHEN'36061'THEN 1 WHEN'36005'THEN 2 WHEN'36047'THEN 3 WHEN'36081'THEN 4 WHEN'36085'THEN 5 END b FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER WHERE p.COUNTY_FIPS IN('36005','36047','36061','36081','36085')AND p.HAS_LOAN=TRUE),
d AS(SELECT DISTINCT DOCUMENT_ID d,CAST(RECORDED_DATETIME AS DATE)t,ROUND(DOCUMENT_AMT,2)a FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT WHERE RELEASE_DT='2026-08-10'AND RECORDED_BOROUGH IN(1,2,3,4,5)AND DOC_TYPE IN('MTGE','M&CON','CMTG','SMTG','MMTG','SPRD')AND DOCUMENT_AMT BETWEEN 505879.44 AND 135000000 AND CAST(RECORDED_DATETIME AS DATE)BETWEEN(SELECT MIN(o)FROM l)AND DATEADD(day,45,(SELECT MAX(o)FROM l))),
x AS(SELECT l.k,d.d FROM l JOIN d ON l.r=0 AND d.a=l.a AND d.t BETWEEN l.o AND DATEADD(day,45,l.o)),
c AS(SELECT DISTINCT x.k,x.d FROM x JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT e ON x.d=e.DOCUMENT_ID JOIN b ON x.k=b.k AND e.BOROUGH=b.b WHERE e.RELEASE_DT='2026-08-10'AND e.BOROUGH IN(1,2,3,4,5)AND e.BLOCK IS NOT NULL AND e.LOT IS NOT NULL),q AS(SELECT k,COUNT(DISTINCT d)n FROM c GROUP BY k),a AS(SELECT c.k,c.d FROM c JOIN q USING(k)WHERE n=1),
z AS(SELECT DISTINCT a.k,TO_VARCHAR(e.BOROUGH)||LPAD(TO_VARCHAR(e.BLOCK),5,'0')||LPAD(TO_VARCHAR(e.LOT),4,'0') bb FROM a JOIN EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT e ON a.d=e.DOCUMENT_ID WHERE e.RELEASE_DT='2026-08-10'AND e.BOROUGH IN(1,2,3,4,5)AND e.BLOCK IS NOT NULL AND e.LOT IS NOT NULL),
s AS(SELECT DISTINCT p.PROPERTY_KEY pk,l.LOAN_KEY k FROM EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE l ON p.CIK=l.CIK AND p.ASSETNUMBER=l.ASSETNUMBER WHERE p.COUNTY_FIPS IN('36005','36047','36061','36081','36085')AND p.HAS_LOAN=TRUE),
t AS(SELECT DISTINCT d.LATITUDE la,d.LONGITUDE lo,z.bb FROM z JOIN s USING(k)JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d ON s.pk=d.PROPERTY_KEY WHERE d.COUNTY_FIPS IN('36005','36047','36061','36081','36085')AND d.LATITUDE IS NOT NULL AND d.LONGITUDE IS NOT NULL),
g AS(SELECT DISTINCT LATITUDE la,LONGITUDE lo FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED WHERE COUNTY_FIPS IN('36005','36047','36061','36081','36085')AND LATITUDE IS NOT NULL AND LONGITUDE IS NOT NULL),
w AS(SELECT g.la,g.lo,REGEXP_REPLACE(TO_VARCHAR(m.BBL),'\\.0$','')pb,m.GEOM_GEOG geom,ST_XMIN(m.GEOM_GEOG) xmin,ST_XMAX(m.GEOM_GEOG) xmax,ST_YMIN(m.GEOM_GEOG) ymin,ST_YMAX(m.GEOM_GEOG) ymax FROM g JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT m ON ST_CONTAINS(m.GEOM_GEOG,ST_POINT(g.lo,g.la))),
m0 AS(SELECT t.la,t.lo,IFF(COUNT(DISTINCT IFF(w.pb=t.bb,w.pb,NULL))>0,'C','I')o FROM t JOIN w ON t.la=w.la AND t.lo=w.lo GROUP BY t.la,t.lo HAVING COUNT(DISTINCT w.pb)>0),
m AS(SELECT m0.la,m0.lo,m0.o,w.geom,w.xmin,w.xmax,w.ymin,w.ymax FROM m0 JOIN w USING(la,lo) QUALIFY ROW_NUMBER()OVER(PARTITION BY m0.la,m0.lo ORDER BY w.pb)=1),
h AS(SELECT m.la,m.lo,f.value::STRING ce,H3_CELL_TO_PARENT(f.value::STRING,8) p8 FROM m,LATERAL FLATTEN(INPUT=>H3_GRID_DISK(H3_POINT_TO_CELL_STRING(ST_POINT(lo,la),9),1))f),
ny AS(SELECT DISTINCT m.la,m.lo,1 fire FROM m JOIN h USING(la,lo) JOIN EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT b ON b.H3_R8=h.p8 AND b.IS_ACTIVE_FOOTPRINT=TRUE AND b.BBOX_XMAX>=m.xmin AND b.BBOX_XMIN<=m.xmax AND b.BBOX_YMAX>=m.ymin AND b.BBOX_YMIN<=m.ymax AND ST_INTERSECTS(b.GEOM_GEOG,m.geom) AND ST_AREA(ST_INTERSECTION(b.GEOM_GEOG,m.geom))/NULLIF(ST_AREA(b.GEOM_GEOG),0)>0.5),
f AS(SELECT m.o,COALESCE(ny.fire,0)fire FROM m LEFT JOIN ny USING(la,lo))
SELECT 'nyc_footprint_majority_on_pip_lot' rule,COUNT(*) denominator,SUM(IFF(fire=1 AND o='C',1,0)) fire_correct,SUM(IFF(fire=1 AND o='I',1,0)) fire_incorrect,SUM(IFF(fire=0 AND o='C',1,0)) nofire_correct,SUM(IFF(fire=0 AND o='I',1,0)) nofire_incorrect FROM f;
```

FEMA exact SQL is the same Gate/PIP-geometry CTE with the final source CTE changed to `EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT`, `STATE_FIPS='36'`, `H3_R8=h.p8`, bbox overlap, `ST_INTERSECTS`, and majority overlap by structure area. It returned `fire_correct=63`, `fire_incorrect=28`, `nofire_correct=91`, `nofire_incorrect=51`.

## P4 241/249 W 74th Street Proof Case

The proof case carries two Geocodio rows for the same source address. The tile check reproduces the founding example: the range-interpolated W 74 point is supported, and the rooftop W 49 point is refuted. I measured both the geocoder-parsed street and the original asserted source street.

| property_name | address | lat | lon | point_r9 | accuracy_type | score | parsed street | parsed present | asserted street | asserted present | source | asof |
|---|---|---:|---:|---|---|---:|---|---:|---|---:|---|---|
| Alfie Arms Corp. | 241/249 West 74th Street | 40.7777640 | -73.9755480 | 892a1008bb3ffff | range_interpolation | 1.00 | W 74 ST | 1 | W 74 ST | 1 | TIGER/Line(R) dataset from the US Census Bureau | 2025-01-01 |
| Alfie Arms Corp | 241/249 West 74th Street | 40.7615050 | -73.9859030 | 892a100d64fffff | rooftop | 0.92 | W 49 ST | 0 | W 74 ST | 0 | City of New York | 2026-08-01 |

This proof case survives, but it is not representative of the labeled Gate V2 set. It is a case where tile-local evidence refutes the bad point cleanly; the aggregate labeled population does not show the same clean separation.

P4 SQL:

```sql
WITH r AS (
 SELECT PROPERTY_NAME,PROPERTY_ADDRESS,LATITUDE,LONGITUDE,ACCURACY_TYPE,ACCURACY_SCORE,NUMBER,STREET,SOURCE,ASOF,CENSUS_YEAR,COUNTY_FIPS
 FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
 WHERE PROPERTY_NAME ILIKE 'Alfie Arms Corp%'
   AND COUNTY_FIPS='36061'
   AND LATITUDE IN (40.7777640,40.7615050)
   AND LONGITUDE IN (-73.9755480,-73.9859030)
), c AS (
 SELECT r.*,f.value::STRING cell
 FROM r,LATERAL FLATTEN(INPUT=>H3_GRID_DISK(H3_POINT_TO_CELL_STRING(ST_POINT(LONGITUDE,LATITUDE),9),1)) f
), p AS (
 SELECT DISTINCT c.LATITUDE,c.LONGITUDE,m.ADDRESS
 FROM c JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT m
   ON H3_INT_TO_STRING(m.H3_R8)=H3_CELL_TO_PARENT(c.cell,8)
  AND H3_POINT_TO_CELL_STRING(m.CENTROID_GEOG,9)=c.cell
), src AS (
 SELECT 'parsed' typ,LATITUDE,LONGITUDE,STREET raw_street FROM r
 UNION ALL
 SELECT 'asserted',LATITUDE,LONGITUDE,REGEXP_REPLACE(PROPERTY_ADDRESS,'^\\s*[0-9]+[A-Z]?(\\s*[-/]\\s*[0-9]+[A-Z]?)?\\s+','') FROM r
 UNION ALL
 SELECT 'pluto',LATITUDE,LONGITUDE,REGEXP_REPLACE(ADDRESS,'^\\s*[0-9]+[A-Z]?(\\s*[-/]\\s*[0-9]+[A-Z]?)?\\s+','') FROM p
), tok AS (
 SELECT typ,LATITUDE,LONGITUDE,raw_street,t.index,
  DECODE(t.value,'NORTH','N','SOUTH','S','EAST','E','WEST','W','STREET','ST','AVENUE','AVE','ROAD','RD','BOULEVARD','BLVD','PLACE','PL','DRIVE','DR','LANE','LN','COURT','CT','PARKWAY','PKWY','HIGHWAY','HWY','TERRACE','TER','CIRCLE','CIR','EXPRESSWAY','EXPY','PLAZA','PLZ','FIRST','1','SECOND','2','THIRD','3','FOURTH','4','FIFTH','5','SIXTH','6','SEVENTH','7','EIGHTH','8','NINTH','9','TENTH','10','ELEVENTH','11','TWELFTH','12',REGEXP_REPLACE(t.value,'^([0-9]+)(ST|ND|RD|TH)$','\\1')) v
 FROM src,LATERAL SPLIT_TO_TABLE(REGEXP_REPLACE(UPPER(COALESCE(raw_street,'')),'[^A-Z0-9]+',' '),' ') t
 WHERE t.value<>''
), n AS (
 SELECT typ,LATITUDE,LONGITUDE,raw_street,LISTAGG(v,' ') WITHIN GROUP (ORDER BY index) norm_street
 FROM tok GROUP BY typ,LATITUDE,LONGITUDE,raw_street
), pl AS (SELECT DISTINCT LATITUDE,LONGITUDE,norm_street FROM n WHERE typ='pluto'),
flags AS (
 SELECT n.typ,n.LATITUDE,n.LONGITUDE,n.raw_street,n.norm_street,COUNT(DISTINCT pl.norm_street) matching_tile_streets
 FROM n LEFT JOIN pl ON n.LATITUDE=pl.LATITUDE AND n.LONGITUDE=pl.LONGITUDE AND n.norm_street=pl.norm_street
 WHERE n.typ IN ('parsed','asserted')
 GROUP BY n.typ,n.LATITUDE,n.LONGITUDE,n.raw_street,n.norm_street
)
SELECT r.PROPERTY_NAME,r.PROPERTY_ADDRESS,r.LATITUDE,r.LONGITUDE,H3_POINT_TO_CELL_STRING(ST_POINT(r.LONGITUDE,r.LATITUDE),9) point_r9,r.ACCURACY_TYPE,r.ACCURACY_SCORE,r.NUMBER,r.STREET,r.SOURCE,r.ASOF,r.CENSUS_YEAR,
       MAX(IFF(f.typ='parsed',f.norm_street,NULL)) parsed_norm_street,MAX(IFF(f.typ='parsed',IFF(f.matching_tile_streets>0,1,0),NULL)) parsed_street_present,
       MAX(IFF(f.typ='asserted',f.norm_street,NULL)) asserted_norm_street,MAX(IFF(f.typ='asserted',IFF(f.matching_tile_streets>0,1,0),NULL)) asserted_street_present,
       MAX(IFF(f.typ='parsed',f.matching_tile_streets,NULL)) parsed_matching_tile_street_count,
       MAX(IFF(f.typ='asserted',f.matching_tile_streets,NULL)) asserted_matching_tile_street_count
FROM r JOIN flags f ON r.LATITUDE=f.LATITUDE AND r.LONGITUDE=f.LONGITUDE
GROUP BY r.PROPERTY_NAME,r.PROPERTY_ADDRESS,r.LATITUDE,r.LONGITUDE,r.ACCURACY_TYPE,r.ACCURACY_SCORE,r.NUMBER,r.STREET,r.SOURCE,r.ASOF,r.CENSUS_YEAR
ORDER BY r.LATITUDE DESC;
```

## P5 Operational Abstention

Applying the best cheap refuter available at full-universe grain, street absence, would abstain/refute 3,642 / 4,046 computable points = 90.01%. In the key nearest-rooftop tier it would refute 314 / 344 = 91.28%.

That is not an operationally usable abstention gate. The aggregate labeled panel says the same thing from the other direction: street absence catches only 1 / 79 wrong PIP answers in the Gate V2 truth set.

## Failed Queries And Corrections

Negative results are recorded because several failure modes are directly relevant to future implementation:

| query | structured result |
|---|---|
| all-feature combined discriminator query | Loom validation failed: message exceeded 10,000 characters |
| first proof-case query | SQL parse failed: `IFF` called with four arguments |
| second proof-case query | SQL compilation failed: `LISTAGG ... ORDER BY p.ADDRESS` invalid in that aggregate context |
| first refreshed P1 full-universe query | SQL compilation failed: `RAW_STREET` not carried into token CTE |
| first NYC+FEMA query | SQL compilation failed: `MIN` does not support `GEOGRAPHY` |
| corrected NYC+FEMA combined query | Snowflake timeout |
| `ST_BOUNDARY` probe | unknown function |
| `ST_EXTERIORRING` probe | unknown function |
| `PARSE_JSON(ST_ASGEOJSON(...))` boundary probe | invalid argument because `ST_ASGEOJSON` already returned an object |

