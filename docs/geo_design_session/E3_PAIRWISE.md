# E3 Pairwise Candidate Test (bd-2qjj)

Measurement agent: PearlSparrow. Data access was SQL-via-Loom only. ACRIS is
used only to label truth and is not used as an evidence row.

## Result

The Gate V2 labeled geometry baseline rebuild reconciles with Appendix H.6:

| truth points | PIP-covered | lot-correct | lot-incorrect |
|---:|---:|---:|---:|
| 242 | 233 | 154 | 79 |

The pairwise test is mostly blocked by candidate reach, not by scoring. For the
79 lot-incorrect PIP points, I selected the deterministic PIP lot as the
minimum PIP BBL and selected the true lot as the nearest ACRIS truth BBL that is
present in MapPLUTO. The denominator then splits:

| scope | failure points |
|---|---:|
| all lot-incorrect PIP failures | 79 |
| selected true BBL present in MapPLUTO | 48 |
| selected true BBL centroid in point r9+k1 tile | 7 |

Interpretation: 72 / 79 failures are out of scope for a tile-local pairwise
solver under this candidate definition: 31 lack a MapPLUTO-present selected
true BBL and 41 have the selected true BBL present but outside the point r9+k1
tile. The foldable E3 headline should therefore be read two ways:

| denominator | joint true wins | joint ties | joint PIP wins | true-win rate |
|---|---:|---:|---:|---:|
| all 79 failures, counting out-of-scope as not solved | 0 | n/a | n/a | 0.00% |
| 7 tile-addressable failures only | 0 | 0 | 7 | 0.00% |

On the 7 tile-addressable failures, the simple vote across measured non-ACRIS
rows scored 11-13 rows per pair and ranked the wrong PIP lot above the true lot
on every pair. Two pairs had at least one individual true-winning row, but the
joint vote still went to PIP.

## Tile-Addressable Row Panel

This is the citable in-scope panel over the 7 failures where the selected true
lot is actually inside the point r9+k1 candidate tile.

| evidence row | pairs | true wins | ties | PIP wins |
|---|---:|---:|---:|---:|
| geocode contains point | 7 | 0 | 0 | 7 |
| geocode distance lower | 7 | 0 | 0 | 7 |
| strict address street | 7 | 0 | 6 | 1 |
| strict address number exact | 7 | 0 | 6 | 1 |
| strict address number in range | 7 | 0 | 6 | 1 |
| asserted SQFT band | 4 computable | 1 | 2 | 1 |
| asserted SQFT closest center | 4 computable | 2 | 0 | 2 |
| NYC footprint BBL-link exists | 7 | 0 | 7 | 0 |
| NYC footprint BBL-link count matches `NUMBLDGS` | 7 | 0 | 7 | 0 |
| NYC footprint BBL-link count closer to `NUMBLDGS` | 7 | 0 | 7 | 0 |
| FEMA majority structure exists | 7 | 0 | 5 | 2 |
| FEMA majority count matches `NUMBLDGS` | 7 | 0 | 3 | 4 |
| FEMA majority count closer to `NUMBLDGS` | 7 | 0 | 3 | 4 |

Definitions and limits:

- Geometry rows compare true vs PIP lot by point containment and lower
  `ST_DISTANCE(point, lot)`.
- Address rows use grammar-lite normalization: strip leading house number,
  uppercase, collapse non-alphanumeric characters. This is intentionally marked
  strict and does not claim the full Appendix E normalizer.
- SQFT uses median `PROPERTY_PERIOD_FACT.SIZE` where `SIZE_MEASURE='SQFT'`.
  Band is `0.78 * BLDGAREA <= asserted_sqft <= 0.95 * BLDGAREA`; closest-center
  uses distance to `0.865 * BLDGAREA`.
- NYC footprint rows use the landed `NYC_BUILDING_FOOTPRINTS_HOT` BBL link
  (`MAPPLUTO_BBL`, falling back to `BBL` / `BASE_BBL`) because the recomputed
  majority-overlap all-pair query timed out. Do not cite these rows as
  recomputed geometry-majority footprint assignment.
- FEMA rows use recomputed majority overlap over the 7 tile-addressable pairs
  only, with direct centroid r8 disks and `STATE_FIPS='36'`.
- `NYC_DCP_MAPPLUTO_HOT` has no units field in the schema probe; units
  consistency was unavailable from landed MapPLUTO.

## Wider Directional Rows

The 48 MapPLUTO-present failure pairs are useful as a directional panel, but
only 7 are tile-local. These rows should not be interpreted as solvable by a
tile-local cascade when the true lot is outside the tile.

| scope | evidence row | pairs | true wins | ties | PIP wins |
|---|---|---:|---:|---:|---:|
| all computable | geocode contains point | 48 | 0 | 0 | 48 |
| all computable | geocode distance lower | 48 | 0 | 0 | 48 |
| all computable | strict address street | 47 computable | 0 | 46 | 1 |
| all computable | strict address number exact | 45 computable | 1 | 17 | 27 |
| all computable | strict address number in range | 45 computable | 1 | 20 | 24 |
| all computable | asserted SQFT band | 14 computable | 1 | 10 | 3 |
| all computable | asserted SQFT closest center | 14 computable | 3 | 0 | 11 |
| all computable | NYC footprint BBL-link exists | 48 | 0 | 46 | 2 |
| all computable | NYC footprint BBL-link count matches `NUMBLDGS` | 48 | 1 | 45 | 2 |
| all computable | NYC footprint BBL-link count closer to `NUMBLDGS` | 48 | 1 | 45 | 2 |

The all-computable geometry, strict-address, and SQFT rows came from structured
Loom responses before the bounded `VALUES` retry. The NYC all-computable rows
came from the successful 48-row `VALUES` query.

## Control Arm

Control construction: tier-matched correct-point sample where possible, with a
deterministic same-tile non-truth neighbor selected by BBL hash. The matched
sample has 76 controls: failures exist in `place` and `street_center`, but there
are no correct points in those tiers, so those two failure tiers cannot be
matched.

| evidence row | pairs | true/PIP wins | ties | neighbor wins |
|---|---:|---:|---:|---:|
| control geocode contains | 76 | 76 | 0 | 0 |
| control geocode distance lower | 76 | 76 | 0 | 0 |

This falsification arm says the neighbor construction itself is not inverted:
when PIP is the true lot, geometry ranks it above a same-tile neighbor 100% of
the time. I did not complete a full control joint vote over all non-geocode rows.

## Failed And Discarded Queries

| query shape | structured outcome | disposition |
|---|---|---|
| first field discovery | `tool_responses=[]` | discarded; reran with structured result |
| broad pair-scope with control + neighbor | Snowflake canceled `000604` | narrowed |
| first footprint/FEMA pairwise query | Snowflake `GEOGRAPHY` in `GROUP BY` error `092102` | fixed by separate tile flag |
| corrected all-pair recomputed NYC+FEMA majority query | Snowflake canceled `000604` | narrowed |
| all-pair NYC BBL-link query with full Gate CTE | Snowflake canceled `000604` | replaced by returned 48-row `VALUES` CTE |
| all-48 joint query | Loom validation failed: message over 10k chars | narrowed to 7 tile-addressable pairs |
| first tile joint query | `tool_responses=[]` and contradicted citable geometry sign | discarded |

## Citable SQL

The Gate V2 labeling CTE is the exact Appendix H.6 geometry score CTE from
`docs/geo_design_session/GROUNDTRUTH_ACRIS_BD179B.md`, with the operating point:
exact cents, non-round amounts only, legal-borough agreement, offset
`[0,+45]`, unique-or-discard after those filters.

The bounded tile-addressable pair set used in the final citable joint query is:

```sql
WITH pair(la,lo,acc,tc,tbb,pbb,tile,tnb,pnb,tba,pba) AS (
  SELECT * FROM VALUES
  (40.6928550,-73.9332360,'nearest_rooftop_match',26,'3016070068','3016030074',1,1.0,1.0,4400,1401),
  (40.7171670,-73.8184820,'rooftop',3,'4066330001','4066340024',1,20.0,2.0,151869,1344),
  (40.7362040,-73.9979040,'nearest_rooftop_match',1,'1006070038','1006080039',1,1.0,1.0,62772,364679),
  (40.7626840,-73.7954350,'nearest_rooftop_match',6,'4053020097','4053017501',1,3.0,1.0,14580,16753),
  (40.7787040,-73.8964370,'rooftop',4,'4007910123','4008020060',1,1.0,1.0,4468,5352),
  (40.8659680,-73.9265150,'nearest_rooftop_match',1,'1022370075','1022370001',1,1.0,1.0,21100,136060),
  (40.8734400,-73.9077550,'rooftop',4,'1022150700','2032450060',1,1.0,1.0,66754,156474)
)
```

Final joint-vote SQL, returning `PAIRS=7`, `JOINT_TRUE_WINS=0`,
`JOINT_TIES=0`, `JOINT_PIP_WINS=7`, `ANY_SINGLE_TRUE_DECISIVE=2`,
`MIN_ROWS_SCORED=11`, `MAX_ROWS_SCORED=13`:

```sql
WITH pair(la,lo,acc,tc,tbb,pbb,tile,tnb,pnb,tba,pba) AS (
  SELECT * FROM VALUES
  (40.6928550,-73.9332360,'nearest_rooftop_match',26,'3016070068','3016030074',1,1.0,1.0,4400,1401),
  (40.7171670,-73.8184820,'rooftop',3,'4066330001','4066340024',1,20.0,2.0,151869,1344),
  (40.7362040,-73.9979040,'nearest_rooftop_match',1,'1006070038','1006080039',1,1.0,1.0,62772,364679),
  (40.7626840,-73.7954350,'nearest_rooftop_match',6,'4053020097','4053017501',1,3.0,1.0,14580,16753),
  (40.7787040,-73.8964370,'rooftop',4,'4007910123','4008020060',1,1.0,1.0,4468,5352),
  (40.8659680,-73.9265150,'nearest_rooftop_match',1,'1022370075','1022370001',1,1.0,1.0,21100,136060),
  (40.8734400,-73.9077550,'rooftop',4,'1022150700','2032450060',1,1.0,1.0,66754,156474)
),
need AS (SELECT tbb bb FROM pair UNION SELECT pbb FROM pair),
mp AS (
  SELECT REGEXP_REPLACE(TO_VARCHAR(BBL),'\\.0$','') bb,
         ADDRESS, GEOM_GEOG geom, CENTROID_GEOG cent,
         BBOX_XMIN xmin, BBOX_XMAX xmax, BBOX_YMIN ymin, BBOX_YMAX ymax
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
  WHERE REGEXP_REPLACE(TO_VARCHAR(BBL),'\\.0$','') IN (SELECT bb FROM need)
),
w AS (
  SELECT p.*, mt.ADDRESS taddr, mp.ADDRESS paddr,
         mt.geom tg, mp.geom pg, mt.cent tcent, mp.cent pcent,
         mt.xmin txmin, mt.xmax txmax, mt.ymin tymin, mt.ymax tymax,
         mp.xmin pxmin, mp.xmax pxmax, mp.ymin pymin, mp.ymax pymax,
         ST_POINT(p.lo,p.la) ptg
  FROM pair p
  JOIN mp mt ON p.tbb = mt.bb
  JOIN mp ON p.pbb = mp.bb
),
sf AS (
  SELECT pair.la, pair.lo, MEDIAN(p.SIZE) sqft
  FROM pair
  JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_DIM d
    ON pair.la = d.LATITUDE AND pair.lo = d.LONGITUDE
  JOIN EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT p
    ON d.PROPERTY_KEY = p.PROPERTY_KEY
  WHERE d.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
    AND p.SIZE_MEASURE = 'SQFT'
    AND p.SIZE IS NOT NULL
  GROUP BY pair.la, pair.lo
),
inp AS (
  SELECT pair.la, pair.lo,
         TRY_TO_NUMBER(REGEXP_SUBSTR(g.NUMBER,'[0-9]+')) hn,
         REGEXP_REPLACE(UPPER(COALESCE(g.STREET,'')),'[^A-Z0-9]+',' ') ist
  FROM pair
  JOIN EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED g
    ON pair.la = g.LATITUDE AND pair.lo = g.LONGITUDE
  WHERE g.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
),
cand AS (
  SELECT w.la,w.lo,'T' sd,w.tbb bb,w.taddr addr,
         REGEXP_REPLACE(UPPER(REGEXP_REPLACE(w.taddr,'^\\s*[0-9]+[A-Z]?(\\s*[-/]\\s*[0-9]+[A-Z]?)?\\s+','')),'[^A-Z0-9]+',' ') st,
         TRY_TO_NUMBER(REGEXP_SUBSTR(w.taddr,'^[0-9]+')) num
  FROM w
  UNION ALL
  SELECT w.la,w.lo,'P',w.pbb,w.paddr,
         REGEXP_REPLACE(UPPER(REGEXP_REPLACE(w.paddr,'^\\s*[0-9]+[A-Z]?(\\s*[-/]\\s*[0-9]+[A-Z]?)?\\s+','')),'[^A-Z0-9]+',' '),
         TRY_TO_NUMBER(REGEXP_SUBSTR(w.paddr,'^[0-9]+'))
  FROM w
),
blk AS (SELECT DISTINCT SUBSTR(bb,1,6) blk, st FROM cand),
rng AS (
  SELECT SUBSTR(REGEXP_REPLACE(TO_VARCHAR(BBL),'\\.0$',''),1,6) blk,
         REGEXP_REPLACE(UPPER(REGEXP_REPLACE(ADDRESS,'^\\s*[0-9]+[A-Z]?(\\s*[-/]\\s*[0-9]+[A-Z]?)?\\s+','')),'[^A-Z0-9]+',' ') st,
         MIN(TRY_TO_NUMBER(REGEXP_SUBSTR(ADDRESS,'^[0-9]+'))) lohn,
         MAX(TRY_TO_NUMBER(REGEXP_SUBSTR(ADDRESS,'^[0-9]+'))) hihn
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
  WHERE SUBSTR(REGEXP_REPLACE(TO_VARCHAR(BBL),'\\.0$',''),1,6) IN (SELECT blk FROM blk)
  GROUP BY 1,2
),
addr AS (
  SELECT cand.la,cand.lo,cand.sd,
         MAX(IFF(inp.ist=cand.st,1,0)) street,
         MAX(IFF(inp.ist=cand.st AND inp.hn=cand.num,1,0)) exact,
         MAX(IFF(inp.ist=cand.st AND inp.hn BETWEEN rng.lohn AND rng.hihn,1,0)) inrange
  FROM cand
  LEFT JOIN inp ON cand.la=inp.la AND cand.lo=inp.lo
  LEFT JOIN rng ON SUBSTR(cand.bb,1,6)=rng.blk AND cand.st=rng.st
  GROUP BY cand.la,cand.lo,cand.sd
),
ny AS (
  SELECT COALESCE(NULLIF(REGEXP_REPLACE(TO_VARCHAR(MAPPLUTO_BBL),'\\.0$',''),''),
                  NULLIF(REGEXP_REPLACE(TO_VARCHAR(BBL),'\\.0$',''),''),
                  NULLIF(REGEXP_REPLACE(TO_VARCHAR(BASE_BBL),'\\.0$',''),'')) bb,
         COUNT(DISTINCT OBJECTID) n
  FROM EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT
  WHERE COALESCE(NULLIF(REGEXP_REPLACE(TO_VARCHAR(MAPPLUTO_BBL),'\\.0$',''),''),
                 NULLIF(REGEXP_REPLACE(TO_VARCHAR(BBL),'\\.0$',''),''),
                 NULLIF(REGEXP_REPLACE(TO_VARCHAR(BASE_BBL),'\\.0$',''),'')) IN (SELECT bb FROM need)
  GROUP BY 1
),
cl AS (
  SELECT la,lo,'T' sd,tbb bb,tg geom,tcent cent,txmin xmin,txmax xmax,tymin ymin,tymax ymax,tnb nb FROM w
  UNION ALL
  SELECT la,lo,'P',pbb,pg,pcent,pxmin,pxmax,pymin,pymax,pnb FROM w
),
ch AS (
  SELECT cl.la,cl.lo,cl.sd,f.value::STRING c8
  FROM cl, LATERAL FLATTEN(INPUT=>H3_GRID_DISK(H3_POINT_TO_CELL_STRING(cl.cent,8),1)) f
),
fm AS (
  SELECT cl.la,cl.lo,cl.sd,
         COUNT(DISTINCT COALESCE(fs.PROVIDER_FEATURE_ID,TO_VARCHAR(fs.OBJECTID))) n
  FROM cl
  JOIN ch ON cl.la=ch.la AND cl.lo=ch.lo AND cl.sd=ch.sd
  JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT fs
    ON fs.STATE_FIPS='36'
   AND fs.H3_R8=ch.c8
   AND fs.BBOX_XMAX>=cl.xmin AND fs.BBOX_XMIN<=cl.xmax
   AND fs.BBOX_YMAX>=cl.ymin AND fs.BBOX_YMIN<=cl.ymax
   AND ST_INTERSECTS(fs.GEOM_GEOG,cl.geom)
   AND ST_AREA(ST_INTERSECTION(fs.GEOM_GEOG,cl.geom))/NULLIF(ST_AREA(fs.GEOM_GEOG),0)>0.5
  GROUP BY cl.la,cl.lo,cl.sd
),
wide AS (
  SELECT w.*, sf.sqft, COALESCE(nt.n,0) tny, COALESCE(np.n,0) pny,
         COALESCE(ft.n,0) tfm, COALESCE(fp.n,0) pfm,
         at.street tstreet, ap.street pstreet, at.exact texact, ap.exact pexact,
         at.inrange tinrange, ap.inrange pinrange
  FROM w
  LEFT JOIN sf USING(la,lo)
  LEFT JOIN ny nt ON w.tbb=nt.bb
  LEFT JOIN ny np ON w.pbb=np.bb
  LEFT JOIN fm ft ON w.la=ft.la AND w.lo=ft.lo AND ft.sd='T'
  LEFT JOIN fm fp ON w.la=fp.la AND w.lo=fp.lo AND fp.sd='P'
  LEFT JOIN addr at ON w.la=at.la AND w.lo=at.lo AND at.sd='T'
  LEFT JOIN addr ap ON w.la=ap.la AND w.lo=ap.lo AND ap.sd='P'
),
feat AS (
  SELECT la,lo,IFF(ST_DISTANCE(ptg,tg)<ST_DISTANCE(ptg,pg),1,IFF(ST_DISTANCE(ptg,tg)=ST_DISTANCE(ptg,pg),0,-1)) r FROM wide
  UNION ALL SELECT la,lo,IFF(ST_CONTAINS(tg,ptg) AND NOT ST_CONTAINS(pg,ptg),1,IFF(ST_CONTAINS(tg,ptg)=ST_CONTAINS(pg,ptg),0,-1)) FROM wide
  UNION ALL SELECT la,lo,IFF(tstreet>pstreet,1,IFF(tstreet=pstreet,0,-1)) FROM wide
  UNION ALL SELECT la,lo,IFF(texact>pexact,1,IFF(texact=pexact,0,-1)) FROM wide
  UNION ALL SELECT la,lo,IFF(tinrange>pinrange,1,IFF(tinrange=pinrange,0,-1)) FROM wide
  UNION ALL SELECT la,lo,IFF(sqft BETWEEN 0.78*tba AND 0.95*tba,1,0)-IFF(sqft BETWEEN 0.78*pba AND 0.95*pba,1,0) FROM wide WHERE sqft IS NOT NULL
  UNION ALL SELECT la,lo,IFF(ABS(sqft-0.865*tba)<ABS(sqft-0.865*pba),1,IFF(ABS(sqft-0.865*tba)=ABS(sqft-0.865*pba),0,-1)) FROM wide WHERE sqft IS NOT NULL
  UNION ALL SELECT la,lo,IFF(tny>0,1,0)-IFF(pny>0,1,0) FROM wide
  UNION ALL SELECT la,lo,IFF(ROUND(tnb)=tny,1,0)-IFF(ROUND(pnb)=pny,1,0) FROM wide
  UNION ALL SELECT la,lo,IFF(ABS(ROUND(tnb)-tny)<ABS(ROUND(pnb)-pny),1,IFF(ABS(ROUND(tnb)-tny)=ABS(ROUND(pnb)-pny),0,-1)) FROM wide
  UNION ALL SELECT la,lo,IFF(tfm>0,1,0)-IFF(pfm>0,1,0) FROM wide
  UNION ALL SELECT la,lo,IFF(ROUND(tnb)=tfm,1,0)-IFF(ROUND(pnb)=pfm,1,0) FROM wide
  UNION ALL SELECT la,lo,IFF(ABS(ROUND(tnb)-tfm)<ABS(ROUND(pnb)-pfm),1,IFF(ABS(ROUND(tnb)-tfm)=ABS(ROUND(pnb)-pfm),0,-1)) FROM wide
),
j AS (
  SELECT la,lo,SUM(r) vote,COUNT(*) rows_scored,
         SUM(IFF(r>0,1,0)) rows_true,
         SUM(IFF(r=0,1,0)) rows_tie,
         SUM(IFF(r<0,1,0)) rows_pip
  FROM feat
  GROUP BY la,lo
)
SELECT COUNT(*) pairs,
       SUM(IFF(vote>0,1,0)) joint_true_wins,
       SUM(IFF(vote=0,1,0)) joint_ties,
       SUM(IFF(vote<0,1,0)) joint_pip_wins,
       SUM(IFF(rows_true>0,1,0)) any_single_true_decisive,
       MIN(rows_scored) min_rows_scored,
       MAX(rows_scored) max_rows_scored
FROM j;
```

The control-arm SQL used the same Gate V2 CTE, then selected correct points up
to each failure-tier count, chose a deterministic non-truth neighbor from the
point r9+k1 tile, and scored only geocode containment/distance. Its structured
result returned `76` controls and `76/76` true-PIP wins for both geometry rows.
