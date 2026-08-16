# bd-q5k2 Imagery And Elevation Source Research

Access date for all sources and live checks: 2026-08-16.

Scope: primary-source research for Appendix A.5 imagery/elevation candidates. This file does not register catalog providers or channels; that step was explicitly out of scope for this bead until the metadata-v2 CLI surface exists.

Observer jobs from Appendix A.4:

- BC: building count per parcel.
- FP: self-generated footprints.
- CD: construction-state change detection.

Verdict vocabulary:

- Survivor: catalog-worthy without a special contract, subject to normal source ingest work.
- Conditional: technically useful but blocked by licensing, authentication, vintage, or use-case constraints.
- Rejected: do not use as a canon source for pinned evidence unless a future bead changes the disqualifying fact.

## Survivors Table

| Source | License posture | Verified access mechanics | Resolution / sensor | Vintage / cadence | Fitness for BC / FP / CD | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| NYS / NYC orthoimagery | NYS open-data posture is permissive; NYC map tiles are CC BY 4.0. Not federal public domain. | NYS direct borough ZIP downloads and ArcGIS REST services; NYC TMS/XYZ/WMTS rendered tiles. Manhattan 2024 ZIP supported byte ranges and had `ETag: "99825b89-636d49d29e880"`. | NYS latest service: natural color at about 12 in; source imagery 4-band at 12 or 6 in. NYC tile prose: evolved to 6 in full true orthos. | NYS statewide program since 2000, goal 4-5 year cycle. NYC borough downloads verified for 2006, 2008, 2010, 2012, 2014, 2016, 2018, 2020, 2022, 2024; NYS page lists NYC counties in 2026 and 2024. | BC strong, FP strong, CD strong at biennial NYC cadence. Best first source for NYC. | Survivor |
| USGS 3DEP LiDAR | U.S. Government public domain; USGS requests credit. | Public `s3://usgs-lidar-public/` EPT bucket, public STAC at `usgs-lidar-stac`, byte-range LAZ shards. Raw LAZ `s3://usgs-lidar/` is Requester Pays. | LiDAR point cloud. 3DEP minimum QL2: ANPD >=2 points/m2 and ANPS <=0.70 m. NYC EPT schema includes XYZ, intensity, classification, GPS time, RGB. | AWS registry says national 3DEP acquisition over 8-year periods, updated periodically. Live NYC EPT exists as `NY_NewYorkCity` with 4,755,025,996 points. TNM product lookup at Times Square returned `NY_CMPG_2013`, published 2015-04-14. | BC strong from roof/building clusters, FP strong through classical geometry, CD weak for current NYC because of vintage. The "no model to characterize" claim does not hold for LiDAR; deterministic extraction errors can be characterized from point density, classification, returns, and comparison to footprints. | Survivor with NYC vintage caveat |
| NAIP | USDA page: U.S. Public Domain. AWS registry: Public Domain with Attribution. | AWS buckets `naip-analytic`, `naip-source`, `naip-visualization` are Requester Pays and anonymous listing failed in this harness. Planetary Computer STAC and Azure COG mirror were anonymous; sample NY COG supported HTTP 206 range. | Aerial optical. AWS: 30-100 cm; `naip-analytic` RGB+NIR MRF, `naip-source` raw RGB+NIR GeoTIFF, `naip-visualization` RGB COG. USDA/data.gov: historical 1 m/2 m; 2018 standard 60 cm with 30 cm option; 2025 half 60 cm/half 30 cm. | Appendix A.5 discrepancy resolved: live PC STAC collection extent is 2010-2023 globally; AWS registry now says 2010-2023 for most products. NYC/NY bbox live query found NY entries in 2011, 2013, 2015, 2017, 2019, 2021, and 2022, not every year. | BC moderate, FP moderate, CD moderate. Leaf-on canopy and 0.6-1 m pixels limit parcel-scale footprint work in NYC. Good national fallback. | Survivor |
| NOAA Emergency Response Imagery | NODD open data; attribution requested; no endorsement allowed. | Public S3 bucket `s3://noaa-eri-pds/` in `us-east-1`, no AWS account required. Live 2025 event GeoTIFF supported byte ranges and had S3 ETag. | Emergency aerial imagery, high-resolution digital cameras and other remote sensing. Digital Coast lists 0.3-0.5 m. | Event-driven, manually updated. Live bucket listed event prefixes through 2026; Digital Coast says 2003-present. | BC/FP only inside event AOIs and not as authoritative mapping data; CD strong for disaster/post-event condition. | Survivor, event-specific |
| Sentinel-2 | Copernicus Sentinel legal notice grants free, full, open access with attribution. | CDSE STAC search anonymous at `https://stac.dataspace.copernicus.eu/v1`; OData/S3 downloads require account/token. S2 sample Product URL returned 401 without auth. | Optical MSI, 13 bands: 10 m, 20 m, 60 m. | CDSE collection extent verified as 2015-06-27 to open-ended; NYC live STAC returned August 2026 items. Mission revisit 5 days at equator. | BC rejected, FP rejected, CD strong for coarse land/use change and construction activity proxies. | Conditional survivor for change only |
| Sentinel-1 | Same Copernicus legal posture as Sentinel-2. | CDSE STAC search anonymous; COG_SAFE assets are exposed as `s3://eodata/...` and Product downloads require token. | C-band SAR; modes down to 5 m, wide coverage, dual polarization. Live sample was IW GRD COG with VV/VH assets. | CDSE collection extent verified as 2014-10-04 to open-ended; NYC live STAC returned August 2026 items. | BC rejected, FP rejected, CD conditional for all-weather coarse change and activity signals. SAR speckle/layover need explicit error characterization. | Conditional survivor for all-weather change only |
| USGS HRO | USGS public domain where USGS-produced. | EarthExplorer search/preview/download; USGS Imagery Only tile service. No public STAC/COG source listing found for HRO in this pass. | HRO is 1 m or finer; USGS Imagery Only service varies 6 in to 1 m but is primarily NAIP in CONUS. | HRO page says 2000-2016. Imagery service says data refreshed June 2024 but mainly NAIP 2017-2021 for CONUS. | BC/FP/CD weaker than NYS/NAIP because HRO is legacy and access is not a clean source-pinnable catalog. | Rejected as separate NYC source; conditional historical fallback |
| Maxar / Vantor | Licensed commercial imagery; internal use permitted by standard internal license, but no redistribution/public hosting of source imagery. | Vantor Hub / APIs / tasking under account and contract. No public bucket/STAC. | WorldView 2D up to 30 cm; Vivid Mosaic 15 cm HD/30 cm HD basemaps; archive dates to 1999. | Up to 15 revisits/day and large daily capacity are advertised for the constellation; tasking and archive are paid. | BC strong, FP strong, CD strong if contract allows internal pinned evidence. | Conditional, licensed-no-redistribution |
| Planet | Licensed commercial imagery. Planet Insights terms require Platform/API serving for Planet Data unless separate agreement says otherwise. | Planet APIs/UI/Integrations behind account, plan, and quotas. No unauthenticated source catalog for commercial imagery. | PlanetScope ARPS: 3 m COGs near-daily from 2017-present. SkySat: 50 cm ortho, RGB/NIR/pan, up to 10x daily revisit. | PlanetScope near-daily; SkySat archive dates to 2014 and 50 cm after 2020 per Planet tasking page. | PlanetScope BC/FP rejected, CD strong. SkySat BC/FP moderate/strong, CD strong if licensed. | Conditional; default terms reject local multi-user caching |
| Nearmap | Licensed commercial aerial imagery. No-charge terms forbid bulk/mass imagery databases and restrict caching. Paid product terms may allow limited exports/offline use during term. | Nearmap apps/APIs/WMS/exports behind subscription. Public pages do not expose a source bucket/STAC. | Vertical imagery GSD 4.4-7 cm per product page; help page says current generation 5.5 cm vertical imagery. | U.S. captures began in 2014 per Nearmap help; frequently refreshed metro archives. | BC strong, FP strong, CD strong if paid license permits internal pinned use. | Conditional, licensed/no-redistribution |
| Vexcel | Website terms prohibit download/storage/commercial use without written consent; product license not public on the checked pages. | Vexcel Data Program behind commercial access. Public website only. | Aerial: oblique and true-ortho urban 7.5 cm or better; wide-area ortho 15-20 cm or better. | 40+ countries/territories; cadence not contractually verified from public terms. | BC strong, FP strong, CD strong if licensed. | Conditional, needs written/product license |
| Airbus OneAtlas | Licensed commercial imagery. Standard EULA allows internal store/copy/process/share; source-product redistribution is restricted. | OneAtlas account/API; archive preview does not provide full-resolution imagery without account/license. | Optical 30 cm, 50 cm, 1.5 m; Pleiades Neo 30 cm/HD15. Radar archive 25 cm to 40 m. | Radar archive from 2007; optical archive/tasking paid or subscription. | BC/FP strong at 30 cm/HD15, CD strong if licensed; radar useful for all-weather change, not footprints. | Conditional, licensed-no-redistribution |
| Google Maps / Earth basemaps | Proprietary service. Terms forbid scraping/caching and derived content such as digitized building outlines. | API key/billing service, not source imagery. | Rendered basemap tiles/imagery only. | Continuously updated service, not a pinnable dataset. | Fails all three jobs as evidence source because deriving footprints/building outlines is expressly forbidden. | Rejected |
| Mapbox Satellite / basemaps | Proprietary service; TOS gives limited, non-transferable, non-sublicensable service license. Offline SDK docs prohibit redistributing downloaded offline maps. | Access-token raster tile API; cache-control TTLs, SDK disk cache/offline regions. | Rendered raster tiles, including `mapbox.satellite` JPEG tiles. | Service-updated; not a source imagery archive. | Fails as a canon evidence source because tiles cannot be bundled/redistributed as pinned source data. | Rejected |
| Esri World Imagery / basemaps | Proprietary Esri/third-party service/data. Esri terms say services are not public domain unless specified. | ArcGIS Online / Location Platform token/account. Offline basemaps allowed only through Esri Content Packages in licensed ArcGIS Runtime/Desktop contexts. | Rendered basemap/data service, not source COGs. | Service-updated; terms and third-party licensors can change. | Fails as general canon source; conditional only inside licensed Esri applications, not for canonical pinned evidence. | Rejected for canon source registry |

## NAIP

Primary URLs accessed 2026-08-16:

- AWS Open Data Registry: https://registry.opendata.aws/naip/
- USDA Ag Data Commons: https://agdatacommons.nal.usda.gov/articles/dataset/NAIP_Digital_Ortho_Photo_Image_Geospatial_Data_Presentation_Form_remote-sensing_image/24664908
- Data.gov NAIP record: https://catalog.data.gov/dataset/national-agriculture-imagery-program-naip-imagery
- Planetary Computer STAC collection: https://planetarycomputer.microsoft.com/api/stac/v1/collections/naip

License posture:

- USDA Ag Data Commons lists the license as "U.S. Public Domain".
- AWS lists "Public Domain with Attribution".
- Planetary Computer STAC returned `license: proprietary`; I treat that as host metadata, not the governing producer license, because the item provider is USDA Farm Service Agency and the USDA/AWS primary license pages are public-domain/attribution.

Access mechanics verified live:

```text
GET https://planetarycomputer.microsoft.com/api/stac/v1/collections/naip
id=naip
extent temporal interval=2010-01-01T00:00:00Z to 2023-12-31T00:00:00Z
license=proprietary
summaries include eo:bands and gsd
```

Direct AWS bucket listing attempts against `naip-visualization`, `naip-source`, and `naip-analytic` returned S3 AccessDenied for anonymous users because the buckets are Requester Pays. The registry itself lists the buckets and access form:

```text
naip-analytic: aws s3 ls --request-payer requester s3://naip-analytic/
naip-source: aws s3 ls --request-payer requester s3://naip-source/
naip-visualization: aws s3 ls --request-payer requester s3://naip-visualization/
```

The anonymous Azure COG mirror on Planetary Computer was range-readable:

```text
URL: https://naipeuwest.blob.core.windows.net/naip/v002/ny/2022/ny_060cm_2022/40074/m_4007439_nw_18_060_20221007.tif
HEAD: HTTP 200, Content-Length 284862096, Last-Modified Tue, 12 Dec 2023 01:59:26 GMT, ETag 0x8DBFAB5F80D0477
Range 0-15: HTTP 206, Content-Range bytes 0-15/284862096
```

Vintage/cadence and Appendix A.5 discrepancy:

- AWS now says the catalog includes 2010-2023 for most products.
- Live Planetary Computer STAC agrees on global collection extent: 2010-2023.
- Live NYC/NY bbox query with `naip:state = ny` returned years 2011, 2013, 2015, 2017, 2019, 2021, and 2022. It returned no NY items for 2010, 2012, 2014, 2016, 2018, 2020, 2023, 2024, 2025, or 2026 in the checked NYC bbox.

Representative NYC/NY live query result:

```text
bbox [-74.3,40.45,-73.65,40.95], state=ny, limit 1 per year
2011: ny_m_4007327_nw_18_1_20110710_20111114, gsd=1.0
2013: ny_m_4007439_nw_18_1_20130622_20130729, gsd=1.0
2015: ny_m_4007319_nw_18_.5_20150624_20151109, gsd=0.5
2017: ny_m_4007309_sw_18_1_20170910_20171207, gsd=1.0
2019: ny_m_4007439_nw_18_060_20190917_20191209, gsd=0.6
2021: ny_m_4007327_ne_18_060_20211106, gsd=0.6
2022: ny_m_4007439_nw_18_060_20221007, gsd=0.6
```

Fitness:

- BC: moderate. At 0.6-1 m, larger structures are countable, but parcel-level roofs under tree canopy and attached/tiny structures are weak.
- FP: moderate/weak for NYC. It can seed candidates, not authoritative footprints.
- CD: moderate. Nonannual state cadence and leaf-on conditions limit construction-state inference.

Verdict: Survivor as a national fallback and broad optical context source, not the leading NYC source.

## USGS 3DEP LiDAR

Primary URLs accessed 2026-08-16:

- AWS Open Data Registry: https://registry.opendata.aws/usgs-lidar/
- USGS copyright/credits: https://www.usgs.gov/information-policies-and-instructions/copyrights-and-credits
- USGS LiDAR base specification: https://www.usgs.gov/ngp-standards-and-specifications/lidar-base-specification-collection-requirements
- Public EPT bucket: https://usgs-lidar-public.s3-us-west-2.amazonaws.com/
- Public STAC bucket: https://usgs-lidar-stac.s3-us-west-2.amazonaws.com/
- TNM API product search: https://tnmaccess.nationalmap.gov/api/v1/products

License posture:

- AWS lists the dataset as U.S. Government Public Domain.
- USGS states that USGS-produced data and information are in the U.S. Public Domain and asks for credit.

Access mechanics verified live:

```text
GET https://usgs-lidar-public.s3-us-west-2.amazonaws.com/?list-type=2&delimiter=/&max-keys=20
Result: public XML listing returned. Anonymous access works.

GET https://usgs-lidar.s3.amazonaws.com/?list-type=2&delimiter=/&max-keys=20
Result: AccessDenied for anonymous users. Raw LAZ bucket is Requester Pays.
```

NYC coverage from the public index:

```text
GET https://usgs-lidar-public.s3-us-west-2.amazonaws.com/?list-type=2&delimiter=/&prefix=NY&max-keys=20
NY prefixes included NY_NewYorkCity/

GET https://usgs-lidar-stac.s3-us-west-2.amazonaws.com/ept/NY_NewYorkCity.json
id=NY_NewYorkCity
bbox=[-74.27908752614985,40.48656118526126,-73.69518259147216,40.928858058234376]
pc:count=4755025996
asset ept.json=https://s3-us-west-2.amazonaws.com/usgs-lidar-public/NY_NewYorkCity/ept.json
```

NYC EPT metadata and range-readable shard:

```text
GET https://usgs-lidar-public.s3-us-west-2.amazonaws.com/NY_NewYorkCity/entwine.json
numPoints=4755025996
schema includes X, Y, Z, Intensity, ReturnNumber, NumberOfReturns, Classification, GpsTime, Red, Green, Blue

URL: https://usgs-lidar-public.s3-us-west-2.amazonaws.com/NY_NewYorkCity/ept-data/0-0-0-0-1.laz
HEAD: HTTP 200, Content-Length 12986, Last-Modified Wed, 23 Jan 2019 15:29:22 GMT, ETag "48efda5514ba090c3db5598f0fd9d134", Accept-Ranges bytes
Range 0-15: HTTP 206, Content-Range bytes 0-15/12986
```

NYC vintage check:

```text
GET https://tnmaccess.nationalmap.gov/api/v1/products?bbox=-73.986,40.756,-73.984,40.758&datasets=Lidar%20Point%20Cloud%20(LPC)&max=20
total=1
title=USGS Lidar Point Cloud NY_New_York_CMGP_SANDY_LiDAR_15 18TWL850120
publicationDate=2015-04-14
source path includes NY_CMPG_2013
downloadURL=https://rockyweb.usgs.gov/vdelivery/Datasets/Staged/Elevation/LPC/Projects/NY_New_York_CMGP_SANDY_LiDAR_15/NY_CMPG_2013/LAZ/USGS_LPC_NY_New_York_CMGP_SANDY_LiDAR_15_18TWL850120.laz
```

The raw TNM LAZ URL above was also range-readable:

```text
HEAD: HTTP 200, Content-Length 43529383, Last-Modified Sun, 02 Jul 2023 06:56:27 GMT, ETag "64a11f9b-29834a7", Accept-Ranges bytes
Range 0-15: HTTP 206
```

A broad five-borough TNM product query timed out with `{"message": "Endpoint request timed out"}`. I therefore record the public EPT/STAC coverage plus the narrow Times Square product lookup as the reliable live vintage evidence.

Resolution/sensor:

- 3DEP LiDAR point cloud, not imagery.
- USGS 2025 LiDAR base specification says the minimum acceptable 3DEP collection quality is QL2, with ANPD >=2 points/m2 and ANPS <=0.70 m.
- Collection conditions prefer leaf-off vegetation.

Fitness and the "no model to characterize" claim:

- BC: strong when vintage matches the target date. Roof clusters and above-ground returns are directly observed.
- FP: strong as a geometry extractor. Classical deterministic extraction can use height normalization, planar roof segmentation, connected components, and point classifications. Error can be characterized by pulse density, point spacing, classification coverage, scan voids, and comparison against NYC/NYS footprints.
- CD: weak for current NYC on the checked product because the live TNM product is Sandy-era/2013 with a 2015 publication date. Strong for historical state-of-structure around that vintage.

Verdict: Survivor, especially for roof geometry and floor/height checks. For current NYC construction state, use only after a per-AOI vintage check.

## Sentinel-2

Primary URLs accessed 2026-08-16:

- Copernicus Sentinel legal notice: https://sentinels.copernicus.eu/documents/247904/690755/Sentinel_Data_Legal_Notice
- CDSE Sentinel-2 page: https://dataspace.copernicus.eu/data-collections/copernicus-sentinel-missions/sentinel-2
- CDSE quotas: https://documentation.dataspace.copernicus.eu/Quotas.html
- CDSE STAC collection: https://stac.dataspace.copernicus.eu/v1/collections/sentinel-2-l2a

License posture:

- The legal notice grants "free, full and open access" and allows reproduction, distribution, communication, adaptation, modification, and combination.
- Required notices are `Copernicus Sentinel data [Year]` or `Contains modified Copernicus Sentinel data [Year]`.

Access mechanics verified live:

```text
GET https://stac.dataspace.copernicus.eu/v1/collections/sentinel-2-l2a
id=sentinel-2-l2a
license=other
extent=2015-06-27T10:25:31Z to null
summaries: platform sentinel-2a/sentinel-2b/sentinel-2c, gsd [10]
```

NYC recent STAC search:

```text
POST /v1/search
collections=["sentinel-2-l2a"]
bbox=[-74.05,40.68,-73.90,40.88]
datetime=2026-01-01/2026-08-16
sortby=datetime desc

Returned five recent items, including:
S2C_MSIL2A_20260814T153811... cloud 88.69
S2B_MSIL2A_20260812T154809... cloud 0.04
S2A_MSIL2A_20260809T155711... cloud 13.25
```

Representative asset:

```text
Item S2B_MSIL2A_20260812T154809_N0512_R054_T18TWL_20260812T194239
B04_10m asset: s3://eodata/Sentinel-2/MSI/L2A/2026/08/12/.../T18TWL_20260812T154809_B04_10m.jp2
Product URL: https://download.dataspace.copernicus.eu/odata/v1/Products(730736a6-1b13-4af1-9797-4f3a011656ca)/$value
Unauthenticated range GET returned HTTP 401.
```

CDSE quotas for general users:

- S3/OData/STAC: 2000 requests/minute.
- Bandwidth per connection: 20 MB/s.
- Concurrent connections: 4.
- Monthly transfer: 12 TB over rolling 30 days, then throttled.
- Token active for 10 minutes and refreshable within 60 minutes.

Resolution/vintage:

- Sentinel-2 has 13 optical bands: four at 10 m, six at 20 m, three at 60 m.
- The CDSE page lists Level 1C worldwide July 2015-present and Level 2A Europe July 2015-present.
- Revisit is 5 days at the equator for the mission configuration.

Fitness:

- BC: rejected. 10 m pixels are too coarse for parcel building counts.
- FP: rejected. Not a building-footprint sensor.
- CD: survivor/conditional. Good for coarse land cover, cleared lots, large construction sites, roof-area changes, and cloud-filtered time series. Not enough alone for exact parcel adjudication.

Verdict: Conditional survivor for change detection only.

## Sentinel-1

Primary URLs accessed 2026-08-16:

- Copernicus Sentinel legal notice: https://sentinels.copernicus.eu/documents/247904/690755/Sentinel_Data_Legal_Notice
- CDSE Sentinel-1 page: https://dataspace.copernicus.eu/data-collections/copernicus-sentinel-missions/sentinel-1
- CDSE quotas: https://documentation.dataspace.copernicus.eu/Quotas.html
- CDSE STAC collection: https://stac.dataspace.copernicus.eu/v1/collections/sentinel-1-grd

License posture:

- Same Copernicus Sentinel legal notice as Sentinel-2: free/full/open access with required attribution notices for original or modified data.

Access mechanics verified live:

```text
GET https://stac.dataspace.copernicus.eu/v1/collections/sentinel-1-grd
id=sentinel-1-grd
license=other
extent=2014-10-04T03:12:47Z to null
summaries: platform sentinel-1a/sentinel-1b/sentinel-1c/sentinel-1d, modes IW/EW/SM, polarizations VV/VH/HH/HV
```

NYC recent STAC search:

```text
POST /v1/search
collections=["sentinel-1-grd"]
bbox=[-74.05,40.68,-73.90,40.88]
datetime=2026-01-01/2026-08-16
sortby=datetime desc

Returned five recent items, including:
S1D_IW_GRDH_1SDV_20260812T225038..._COG with VV/VH assets
S1D_IW_GRDH_1SDV_20260812T225013..._COG with VV/VH assets
S1D...20260806...
```

Representative asset:

```text
Item S1D_IW_GRDH_1SDV_20260812T225038..._COG
vv asset: s3://eodata/Sentinel-1/SAR/IW_GRDH_1S-COG/2026/08/12/.../s1d-iw-grd-vv-...-cog.tiff
Product URL: https://download.dataspace.copernicus.eu/odata/v1/Products(8123a558-0ccb-431c-84f7-a79daa5764a6)/$value
Unauthenticated range GET returned HTTP 401.
```

Resolution/vintage:

- C-band SAR, all-weather/day-night imaging.
- CDSE page states resolution down to 5 m and coverage up to 400 km.
- GRD and COG_SAFE GRD are worldwide from October 2014-present.

Fitness:

- BC: rejected for exact parcel building count.
- FP: rejected for building footprint generation.
- CD: conditional survivor. Useful for all-weather coarse change, flood/structural disruption context, and large site activity, but SAR layover, shadowing, speckle, incidence-angle effects, and urban double-bounce need explicit error handling.

Verdict: Conditional survivor for all-weather change signals.

## NYS And NYC Orthoimagery

Primary URLs accessed 2026-08-16:

- NYS orthoimagery overview: https://gis.ny.gov/orthoimagery
- NYS NYC borough downloads: https://gis.ny.gov/new-york-city-orthoimagery-downloads
- NYS Latest ArcGIS service: https://orthos.its.ny.gov/arcgis/rest/services/wms/Latest/MapServer?f=pjson
- NYS Open Data Handbook PDF: https://data.ny.gov/download/77gx-ii52/application/pdf
- NYC map tiles: https://maps.nyc.gov/tiles/
- NYC Open Data Terms: https://opendata.cityofnewyork.us/overview/#termsofuse

License posture:

- NYS open data handbook says users may use government information "as you wish" and that the state does not require public attribution restrictions.
- NYC map tiles page says each map tile set is licensed under CC BY 4.0.
- NYC Open Data terms add warranty/accuracy disclaimers and point to NYC.gov terms plus agency-specific terms.

Access mechanics verified live:

```text
GET https://orthos.its.ny.gov/arcgis/rest/services/wms/Latest/MapServer?f=pjson
serviceDescription: combination of 2022, 2023, 2024, 2025 imagery
view: natural color, approximately 12 inch resolution
source: 4-band at 12 or 6 inches
copyrightText: NYS ITS Geospatial Services
singleFusedMapCache=false
```

NYS Manhattan 2024 ZIP:

```text
URL: https://gisdata.ny.gov/ortho/nysdop12/new_york_city/spcs/zips/boro_manhattan_sp24.zip
HEAD: HTTP 200, Content-Length 2575457161, Last-Modified Thu, 05 Jun 2025 15:17:38 GMT, ETag "99825b89-636d49d29e880", Accept-Ranges bytes
Range 0-15: HTTP 206, Content-Range bytes 0-15/2575457161
```

NYC tile service sample:

```text
URL: https://maps.nyc.gov/xyz/1.0.0/photo/2018/18/77197/98517.png8
HEAD/GET: HTTP 200, Content-Type image/png, Last-Modified Wed, 30 Jan 2019 22:47:32 GMT, Access-Control-Allow-Origin *
```

Vintage/cadence:

- NYS program has produced orthoimagery since 2000 and aims for a statewide 4-5 year cycle.
- NYS page lists all counties as having multiple years of coverage.
- NYC borough direct downloads exist for 2006, 2008, 2010, 2012, 2014, 2016, 2018, 2020, 2022, and 2024.
- NYC tile page says NYC orthophotography has been captured biennially since 2004, with ad hoc 1996 captures before that.

Fitness:

- BC: strong. 6-12 in true/natural-color ortho is adequate for visible roof/building counts.
- FP: strong for self-generated footprints, especially with true-ortho vintages and parcel constraints.
- CD: strong at the NYC biennial cadence. Not event-real-time, but better than NAIP for NYC.

Verdict: Survivor and recommended first NYC optical source. For pinned evidence, prefer NYS downloadable source ZIPs or ArcGIS service exports over transient rendered NYC tiles.

## USGS High Resolution Orthoimagery

Primary URLs accessed 2026-08-16:

- HRO product page: https://www.usgs.gov/centers/eros/science/usgs-eros-archive-aerial-photography-high-resolution-orthoimagery-hro
- USGS Imagery Only service JSON: https://basemap.nationalmap.gov/arcgis/rest/services/USGSImageryOnly/MapServer?f=pjson
- USGS copyright/credits: https://www.usgs.gov/information-policies-and-instructions/copyrights-and-credits

License posture:

- HRO page marks sample media public domain and USGS policy says USGS-produced data are U.S. Public Domain.
- Some HRO content was purchased from private vendors or partners, so per-dataset metadata still matters.

Access mechanics verified live:

- HRO product page points users to EarthExplorer for search, preview, and download under Aerial Imagery.
- The USGS Imagery Only live service returned a tiled basemap, not a source imagery catalog.

Live service facts:

```text
GET https://basemap.nationalmap.gov/arcgis/rest/services/USGSImageryOnly/MapServer?f=pjson
serviceDescription: tile cache basemap visible to 1:9,028
resolution: USGS digital orthoimage may vary from 6 inches to 1 meter
majority CONUS source: NAIP
download note: National Map download client allows free public-domain 1 m JP2 for CONUS
copyrightText: USDA, USGS The National Map: Orthoimagery. Data refreshed June, 2024.
```

Vintage/resolution:

- HRO product page says 1 m or finer from across the U.S. for 2000-2016.
- USGS Imagery Only is a composite basemap, mostly NAIP for CONUS, not a distinct current HRO source.

Fitness:

- BC/FP/CD: technically possible on individual HRO products, but inferior to NYS/NYC ortho for NYC and harder to pin as a clean current source.

Verdict: Rejected as a separate first-class source for canon geo. It remains a conditional historical fallback if a specific EarthExplorer product is needed and can be pinned.

## NOAA Emergency Response Imagery

Primary URLs accessed 2026-08-16:

- AWS Open Data Registry: https://registry.opendata.aws/noaa-eri/
- NOAA Digital Coast ERI: https://coast.noaa.gov/digitalcoast/data/emergency.html
- Public bucket: https://noaa-eri-pds.s3.amazonaws.com/

License posture:

- AWS registry says NOAA data disseminated through NODD are open to the public and "can be used as desired".
- Attribution is requested for unaltered data; users may not imply NOAA endorsement or affiliation.
- Digital Coast warns the rapid response product is not intended for mapping, charting, or navigation.

Access mechanics verified live:

```text
GET https://noaa-eri-pds.s3.amazonaws.com/?list-type=2&delimiter=/&max-keys=20
Result: public XML listing returned.

aws s3 ls --no-sign-request s3://noaa-eri-pds/
Result: listed event prefixes through 2026, including 2026_Midwest_Flood/ and 2026_Pre_Event/.
```

Representative event object:

```text
URL: https://noaa-eri-pds.s3.amazonaws.com/2025_Hurricane_Melissa/20251031a_RGB/20251031aC0774200w175145n.tif
HEAD: HTTP 200, Content-Length 66478040, Last-Modified Fri, 31 Oct 2025 21:08:45 GMT, ETag "6d4204be50505f5562c0c981ad502950-8", Accept-Ranges bytes
Range 0-15: HTTP 206, Content-Range bytes 0-15/66478040
```

Resolution/vintage:

- Digital Coast lists 0.3-0.5 m and 2003-present.
- AWS says updates are manual when needed.
- AWS says GeoTIFF and COG imagery, with COG format available for recent events. The tested TIFF had byte-range support; I did not independently inspect its internal COG layout.

Fitness:

- BC: conditional within event AOI only. The source disclaimer says visual damage analysis, not mapping.
- FP: conditional/weak. Good for post-event visible structure checks, not authoritative footprint extraction.
- CD: strong for post-disaster change state and damage-context evidence.

Verdict: Survivor, event-specific.

## Commercial Tier

### Maxar / Vantor

Primary URLs accessed 2026-08-16:

- Vantor Hub product page: https://vantor.com/product/platform/hub/
- WorldView product page: https://vantor.com/product/worldview/
- Discover imagery docs: https://xpress-docs.maxar.com/About/About_imagery.htm
- Maxar Internal Use License PDF: https://maxar-marketing.s3.amazonaws.com/files/legal/FORM_WW0023F_InternalUseLicense_ver4-21-21.pdf

License posture:

- The internal-use license grants limited internal use and derivative rights, but restricts sublicensing and public availability of products/derivatives. It requires deletion/destruction on termination.
- Load-bearing posture: licensed-no-redistribution for source imagery. Public canon artifacts should not embed pixels unless the customer contract expressly permits it.

Access mechanics:

- Contracted platform/API/tasking; no public bucket or anonymous STAC found.
- Vantor Hub advertises archive and tasking access.

Resolution/vintage:

- Vantor Hub says 20-plus-year archive and more than 6 billion sq km at 30 cm.
- WorldView page says 10 satellites, about 7 million sq km daily capacity, 3.5+ million sq km/day at 30 cm, and up to 15 revisits/day.
- Vivid Mosaic basemaps include 15 cm HD and 30 cm HD.

Fitness:

- BC/FP/CD all strong if licensed and if the acquisition date is suitable.
- Key risk is legal, not sensor quality.

Verdict: Conditional survivor under a license that allows internal pinned evidence.

### Planet

Primary URLs accessed 2026-08-16:

- Planet terms: https://www.planet.com/terms-of-use/
- PlanetScope docs: https://docs.planet.com/data/imagery/planetscope/
- Analysis-Ready PlanetScope tech spec: https://docs.planet.com/data/imagery/arps/techspec/
- SkySat docs: https://docs.planet.com/data/imagery/skysat/
- Planet tasking page: https://www.planet.com/products/high-resolution-satellite-imagery/

License posture:

- Planet Insights terms say Planet Data use is a limited, nontransferable, nonexclusive, non-sublicensable, revocable license.
- They also say Planet Data must be served directly via API and "may not rely on caching" for multi-end-user distribution, absent a separate agreement.

Access mechanics:

- Account/API/UI/integrations; public docs, but commercial data require plan/license and processing units.
- No anonymous commercial source listing was available.

Resolution/vintage:

- ARPS PlanetScope: near-daily, 3 m, four-band surface reflectance COGs, producible from 2017-present, 48-hour latency.
- SkySat: 15 satellites, up to 10x daily revisit, 50 cm ortho, RGB/NIR/panchromatic, archive/tasking via Planet.
- Planet tasking page says SkySat archive dates back to 2014, with 50 cm spatial resolution for images collected on/after 2020-06-30 and 72 cm before that date.

Fitness:

- PlanetScope: BC/FP rejected; CD strong at daily cadence.
- SkySat: BC/FP moderate to strong, CD strong, subject to licensing and tasking/availability.

Verdict: Conditional. Default online terms do not support canon-style local source-pixel caching for multi-user evidence unless a separate written license grants that right.

### Nearmap

Primary URLs accessed 2026-08-16:

- Product-specific terms: https://www.nearmap.com/legal/product-specific-terms
- Website terms: https://www.nearmap.com/legal/website-terms-of-use
- Imagery product page: https://www.nearmap.com/products/imagery
- Vertical imagery page: https://www.nearmap.com/products/imagery/vertical
- Nearmap content help: https://help.nearmap.com/kb/articles/105-about-nearmap-content

License posture:

- No-charge products may not be used to create a database of images for resale/distribution, and may not be pre-fetched/cached/stored except limited temporary performance caching.
- Product-specific terms include paid-product export/offline allowances in some products, but use is subscription- and product-specific, and deletion can be required after termination.

Access mechanics:

- Subscription apps/APIs/WMS/exports; no public bucket or STAC.
- Static imagery display may be limited to Level 20/6 inch imagery resolution under specific web-map product terms.

Resolution/vintage:

- Product page says vertical imagery GSD is 4.4-7 cm per pixel.
- Help page says current camera system captures vertical imagery at 5.5 cm GSD and U.S. captures began in 2014.
- Public pages say metro areas are refreshed frequently, but the exact NYC cadence needs a subscription-side coverage query.

Fitness:

- BC strong, FP strong, CD strong if export/offline evidence rights are licensed.
- The blocker is legal/pinning terms, not pixel quality.

Verdict: Conditional licensed source; not usable from no-charge terms for canon source evidence.

### Vexcel

Primary URLs accessed 2026-08-16:

- Vexcel Terms of Use: https://vexceldata.com/tou/
- L3Harris Vexcel Data Program imagery products: https://www.l3harris.com/all-capabilities/vexcel-data-program-imagery-products

License posture:

- Website terms say users may not modify/copy/distribute/sell/license site information without express written consent.
- The license grants personal site access only and says users may not download other than page caching without written consent.
- Public product license terms for source imagery were not found in this pass; that is a capability-gap finding for procurement.

Access mechanics:

- Commercial Vexcel Data Program; no public source bucket/STAC found.

Resolution/vintage:

- L3Harris page says Vexcel publishes imagery in 40+ countries/territories.
- Oblique and TrueOrtho urban collections: 7.5 cm or better.
- Wide-area ortho: 15-20 cm or better.
- Public cadence/current NYC availability was not verified from a live coverage surface.

Fitness:

- BC strong, FP strong, CD strong if licensed with cache/export rights.

Verdict: Conditional; needs explicit written/product license before cataloging.

### Airbus OneAtlas

Primary URLs accessed 2026-08-16:

- License page: https://space-solutions.airbus.com/legal/licences/
- Standard EULA PDF: https://storage.googleapis.com/p-ssp-iep-prod-8ff-strapi-uploads/Standard_End_User_Licence_Agreement_November_2025_e87587e010/Standard_End_User_Licence_Agreement_November_2025_e87587e010.pdf
- OneAtlas ordering/access page: https://space-solutions.airbus.com/imagery/how-to-order-imagery-and-data/
- Airbus terms: https://space-solutions.airbus.com/legal/terms-and-conditions/

License posture:

- License page summarizes Standard internal rights as "View, copy, process, share internally".
- Standard EULA text grants internal rights to use/store/access/copy/share/process product, and allows derivative-work distribution, but restricts stand-alone extract redistribution and imagery whose geolocation has been enhanced.
- Termination for breach requires permanent deletion/destruction of product/VAP/extract copies.

Access mechanics:

- OneAtlas account/API/subscription or pay-per-order.
- Archive preview requires no account but does not provide full-resolution imagery.

Resolution/vintage:

- Radar archive from 2007, 25 cm to 40 m, daylight/weather independent.
- Optical Living Library: 30 cm, 50 cm, and 1.5 m; users can stream/download full-resolution imagery by subscription.
- Pleiades Neo archive/tasking: 30 cm and HD15.

Fitness:

- BC/FP strong for 30 cm and HD15 optical if licensed; radar not a footprint source but useful for all-weather change.
- CD strong with archive/tasking access.

Verdict: Conditional licensed source; suitable only when the license permits internal pinned evidence and attribution/output constraints are respected.

## Basemap Terms Check

### Google Maps / Earth

Primary URLs accessed 2026-08-16:

- Google Maps Platform terms: https://cloud.google.com/maps-platform/terms
- Google Maps service-specific terms: https://cloud.google.com/maps-platform/terms/maps-service-terms

Disqualifying facts:

- Google Maps Platform terms prohibit scraping, pre-fetching, storing, resharing, rehosting, and bulk downloading Google Maps Content.
- Terms state "No Caching" except as expressly permitted.
- Terms explicitly forbid creating content from Google Maps Content, including tracing or digitizing building outlines from the satellite basemap and creating 3D building models from 45-degree imagery.

Verdict: Rejected. Google basemaps are forbidden as a source for building count, footprint extraction, or construction-state evidence in canon.

### Mapbox

Primary URLs accessed 2026-08-16:

- Mapbox TOS: https://www.mapbox.com/legal/tos
- Raster Tiles API: https://docs.mapbox.com/api/maps/raster-tiles/
- Maps API caching: https://docs.mapbox.com/help/dive-deeper/api-caching/
- Offline SDK concepts: https://docs.mapbox.com/android/maps/guides/offline/concepts/

Disqualifying facts:

- TOS grants a limited, non-transferable, non-sublicensable, revocable service license.
- Raster tiles require an access token and are billed by requests.
- Raster tile responses have a device cache TTL of 12 hours and CDN TTL of 5 minutes.
- Offline SDK docs say downloaded offline maps may not be redistributed, bundled, or preloaded; users must retrieve Mapbox data from Mapbox servers.

Verdict: Rejected for canon evidence. Mapbox Satellite can be a UI basemap, not a pinned source dataset.

### Esri

Primary URLs accessed 2026-08-16:

- Esri Web Site and Service Terms: https://www.esri.com/en-us/legal/terms/web-site-service
- Esri Master Agreement page: https://www.esri.com/en-us/legal/terms/full-master-agreement
- Esri Master Agreement PDF: https://assets.esri.com/content/dam/esrisites/en-us/media/legal/ma-full/ma-full.pdf
- Esri current click-through/product-specific PDF surfaced from the legal page: https://assets.esri.com/content/dam/esrisites/en-us/media/legal/ma-translations/english.pdf
- ArcGIS Static Basemap Tiles docs: https://developers.arcgis.com/documentation/glossary/arcgis-static-basemap-tiles-service/
- Basemap usage model docs: https://developers.arcgis.com/documentation/glossary/basemap-usage-model/
- Offline maps docs: https://doc.esri.com/en/arcgis-enterprise/latest/share/take-maps-offline.html

Disqualifying facts:

- Esri web/service terms say services are not in the public domain unless specified, and third-party data/imagery may have separate license restrictions.
- Esri Data terms allow Online Services basemaps offline through Esri Content Packages for licensed ArcGIS Runtime/Desktop, but state that customers may not otherwise scrape, download, or store Data.
- Esri terms also prohibit using Data outside Software/Online Services to train machine systems/models/algorithms.
- ArcGIS basemap services require ArcGIS Location Platform or ArcGIS Online accounts and access tokens; usage is measured by tiles or sessions.

Verdict: Rejected for the canon source registry. Conditional only for a licensed ArcGIS application workflow, not for source-pinned evidence artifacts.

## Overall Disposition

Catalog-worthy public/open survivors:

- NYS/NYC orthoimagery for NYC optical building evidence.
- USGS 3DEP LiDAR for geometry/height evidence, with per-AOI vintage checks.
- NAIP as a national optical fallback and non-NYC baseline.
- NOAA ERI for disaster/event-specific construction-state evidence.

Conditional public/open survivors:

- Sentinel-2 for coarse optical change detection only.
- Sentinel-1 for coarse all-weather SAR change detection only.

Rejected or conditional non-public sources:

- USGS HRO is rejected as a separate current NYC source because it is legacy and not a clean public STAC/COG source in this pass.
- Commercial sources are technically valuable but must be treated as licensed, non-redistributable, internal-only evidence unless a specific contract says otherwise.
- Google, Mapbox, and Esri basemaps are not acceptable canon source-pixel evidence. They may be display layers only where their terms allow.
