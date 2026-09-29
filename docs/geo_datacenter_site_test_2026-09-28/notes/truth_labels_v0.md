# Truth labels v0 (2026-09-28)

A label is valid only if it comes from an official document independent of any geocoder, Epoch, or our news
pipeline. "Verified" below means I read the document text myself; "agent-reported" means a research subagent's
claim that I have not checked against the source. Only verified labels are scored so far.

## A. Verified official point labels (state air-permit records)

### Google Pryor OK (Epoch: "Google Pryor (North)", 4581 Webb St, Pryor OK 74361)
- Source: Oklahoma DEQ, Evaluation of Permit Application No. 2021-0235-C (M-1), Myall, LLC, "Pryor Data Storage
  Facility (SIC 7374/NAICS 518210)", Facility ID 6417. https://applications.deq.ok.gov/permitspublic/storedpermits/8648.pdf
  (memo dated December 14, 2023 per the agent; I extracted the text and confirmed the lines below).
- Text (extracted locally with pdftotext): "Latitude 36.24250°N, Longitude 95.33020°W" and "facility is on southeast
  corner, 4581 Webb Street".
- Truth point: 36.24250, -95.33020. Address 4581 Webb St confirmed.
- NOT stated by the document: that the operator is Google. That link comes from Epoch (Owner Google) and Foursquare
  (POI "Google" at the same address). The memo also names a separate Myall Main Street Pryor Warehouse (Facility ID 13935).
- Grain: one facility point. It says nothing about which buildings or parcels belong to the campus.

### Meta Los Lunas NM (Epoch: 4250 Messenger Lp, Los Lunas NM 87031)
- Source: NMED Air Quality Bureau public notice, Permit No. 7026-M5 (TEMPO Agency Interest 37303), Greater Kudu, LLC.
  https://www.env.nm.gov/air-quality/wp-content/uploads/sites/2/2021/03/AQBP-Public-Notice-7026M5.pdf
- Text (extracted locally): "Greater Kudu, LLC at 4250 Messenger Loop NW, Los Lunas, NM ... The exact location of the
  facility is at latitude and longitude decimal degrees: 34.828611, -106.781389 Datum: NAD83. This facility is located
  within the Village of Los Lunas ..."
- Truth point: 34.828611, -106.781389 (NAD83; treated as WGS84 for a 1,500 m scoring radius, the shift is under 2 m).
- NOT stated: that Greater Kudu is Meta. That comes from Epoch (Owner Meta). Notice dated around 2021 (preliminary
  intent to issue on or before June 13, 2021 per the agent).
- Grain: one facility point.

## B. Official parcel classification (verified from our landed Franklin County Auditor data)
See `truth_candidates_franklin_auditor.md`: 18 parcels the Auditor classes "data center". Independent of geocoders;
owner names are not attributed to operators unless the record says so.

## C. Agent-reported, NOT yet verified (do not score against these)
| Site | Claim | Strength |
|---|---|---|
| Anthropic-Amazon New Carlisle | IDEM draft permit T141-47750-00642: "Amazon Data Services, Inc., located at 55001 Larrison Blvd., New Carlisle, IN 46552"; no coordinates or parcels | medium, address only |
| OpenAI Stargate Abilene | Texas Comptroller Ch. 312 abatement lists 10 Taylor CAD accounts under Lancium Abilene LLC; no street address, so no tie to 5502 Spinks Rd | low, owner-level assemblage |
| Abilene (possible same campus) | TCEQ notice for Crusoe "Longhorn Data Center Buildings 9 and 10" at 615 FM 2404, marker -99.77722, 32.50792 | not tied to 5502 Spinks; do not score |
| Google New Albany | City of New Albany project page lists "Google 7 ... 1101 Beech Road" (read through a summarizer) | low |
| AWS New Albany | same page: AWS buildings "generally located at Miller and Beech"; 13360 not printed | low |
| Meta Prometheus | same page lists Meta at 1500 Beech Rd, 13385 Green Chapel Rd, 4409-4417 Clover Valley Rd; NOT 1 Community Cir | low, but suggests Epoch's address may differ from the city's |
| Google Columbus, Google Lancaster | no official record found | none |
| Colossus 1 / 2 | no official document read (Shelby County Health Dept permit 01156-01PC not retrievable) | none |

## D. Findings about label acquisition itself
- Web fetch tools cannot read most county assessor searches (interactive/403) or large planning PDFs. State air-permit
  notices are the most reliable official source: they print a facility address and often coordinates.
- A data-center coordinate from a state air permit is a distinct evidence channel worth harvesting at scale (ODEQ,
  NMED, IDEM, TCEQ, others). It found an anchor for a site the Census geocoder cannot place (Los Lunas).
