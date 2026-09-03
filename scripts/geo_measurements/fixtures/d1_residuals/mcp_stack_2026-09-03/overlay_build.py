import json, blake3, collections
LO, HI, MIN_ASSERTED = 0.5, 2.6, 500
def b3(obj): return blake3.blake3(json.dumps(obj, sort_keys=True, separators=(',',':')).encode()).hexdigest()
subs=json.load(open('subjects.json')); props=json.load(open('nyc_props.json')); sizes=json.load(open('sizes.json')); parcels=json.load(open('parcels.json'))
pip={r['subject_id']:r for r in json.load(open('pip_scored.json'))}
_req=json.load(open('/Users/zac/Source/cmdrvl/canon/scripts/geo_measurements/fixtures/d1_residuals/h7_population_request.json'))['cases']
solver_ids=[c['id'] for c in _req]
universe={c['id']:c['evidence']['universe']['parcels'] for c in _req}
by_sid={}
for s in subs:
    by_sid.setdefault(s['subject_id'], s)
# ---- calibration record (truth-scored, recorded, hashed) ----
calib_rows=[]
for sid in solver_ids:
    s=by_sid[sid]; boro=s['legal_borough']
    ps=[p for p in props if p['LOAN_KEY']==s['loan_key'] and p['RECORDED_BOROUGH']==boro]
    sq=[(p['PROPERTY_KEY'],sizes[p['PROPERTY_KEY']]['size']) for p in ps if sizes.get(p['PROPERTY_KEY'],{}).get('measure')=='SQFT' and (sizes[p['PROPERTY_KEY']]['size'] or 0)>=MIN_ASSERTED]
    if not sq: continue
    asserted=sum(v for _,v in sq)
    t=s['truth_parcels']
    if all(x in parcels and parcels[x]['bldgarea'] is not None for x in t):
        tb=sum(parcels[x]['bldgarea'] for x in t)
        calib_rows.append({'subject_id':sid,'asserted_sqft':asserted,'truth_bldgarea':tb,'ratio':round(tb/asserted,4),'in_band': LO*asserted<=tb<=HI*asserted})
calibration={'population_id':'h7-d1-residuals-2026-09-03','method':'sum(MapPLUTO 26v1 BLDGAREA over truth parcels) / sum(ABS-EE SIZE where SIZE_MEASURE=SQFT and SIZE>=%d) per solver subject'%MIN_ASSERTED,'band':[LO,HI],'rows':calib_rows,'coverage':{'n':len(calib_rows),'in_band':sum(r['in_band'] for r in calib_rows)}}
pipcal={'population_id':'h7-d1-residuals-2026-09-03','method':'ABS-EE property lat/long ST_CONTAINS MapPLUTO 26v1 parcel, restricted to legal borough','coverage':collections.Counter(r['cls'] for r in pip.values())}
json.dump(calibration,open('calibration_size_band.json','w'),indent=1); json.dump(pipcal,open('calibration_pip.json','w'),indent=1)
cal_hash=b3(calibration); pip_hash=b3(pipcal)
print("size-band calibration: n=%d in_band=%d hash=%s"%(len(calib_rows),calibration['coverage']['in_band'],cal_hash[:12]))
print("pip calibration:",dict(pipcal['coverage']),pip_hash[:12])
size_contract={'id':'rho.size.asserted_sqft_band','version':'1.0.0','source_dataset':'EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT_x_MAPPLUTO_26V1','source_release':'ppf-latest_26v1','source_lineage_ids':sorted(['EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT:latest_reporting_period','EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES:26v1']),'method_id':'asserted-sqft-bldgarea-sum-band','method_version':'1.0.0','claim_role':'attribute_observation','basis':{'kind':'empirical_calibration','population_id':calibration['population_id'],'calibration_blake3':cal_hash,'falsification_rule_id':'truth-bldgarea-sum-outside-band','admissible_hard_band':True}}
pip_contract={'id':'rho.address.geocode.parcel_containment','version':'1.0.0','source_dataset':'EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY_x_MAPPLUTO_26V1','source_release':'lip-current_26v1','source_lineage_ids':sorted(['EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:current','EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES:26v1']),'method_id':'geocode-point-in-parcel','method_version':'1.0.0','claim_role':'stable_identity_anchor','basis':{'kind':'empirical_calibration','population_id':pipcal['population_id'],'calibration_blake3':pip_hash,'falsification_rule_id':'geocode-hit-outside-truth'}}
overlays=[]; nband=npip=0
for sid in solver_ids:
    s=by_sid[sid]; boro=s['legal_borough']; cand=universe[sid]
    obs=[]; contracts=[]
    ps=[p for p in props if p['LOAN_KEY']==s['loan_key'] and p['RECORDED_BOROUGH']==boro]
    sq=[(p['PROPERTY_KEY'],sizes[p['PROPERTY_KEY']]) for p in ps if sizes.get(p['PROPERTY_KEY'],{}).get('measure')=='SQFT' and (sizes[p['PROPERTY_KEY']]['size'] or 0)>=MIN_ASSERTED]
    if sq and all(c in parcels and parcels[c]['bldgarea'] is not None for c in cand):
        asserted=sum(v['size'] for _,v in sq)
        recs=[{'source_record_id':'EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT:%s'%k,'source_vintage':'latest_reporting_period','record_blake3':b3(v)} for k,v in sq]
        recs+=[{'source_record_id':'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES:26v1:%s'%c,'source_vintage':'26v1_2026-05-01','record_blake3':b3(parcels[c])} for c in cand]
        obs.append({'id':'obs.size.asserted_sqft_band:%s'%sid,'contract_id':size_contract['id'],'source_records':recs,'observation':{'kind':'integer_sum_band','level':'parcel','measure':{'semantic_id':'mappluto.bldgarea','unit':'sqft','value_origin':'source_asserted'},'values':[{'id':c,'value':int(parcels[c]['bldgarea'])} for c in cand],'min':int(LO*asserted),'max':int(HI*asserted)+1}})
        contracts.append(size_contract); nband+=1
    hits=[h for h in pip.get(sid,{}).get('hits',[]) if h in set(cand)]
    if hits and ps:
        recs=[{'source_record_id':'EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:%s:%s'%(p['LOAN_KEY'],p['PROPERTY_KEY']),'source_vintage':'current','record_blake3':b3({k:p[k] for k in ('LATITUDE','LONGITUDE','PROPERTY_KEY')})} for p in ps]
        for h in hits:
            obs.append({'id':'obs.address.geocode.parcel_containment:%s:%s'%(sid,h),'contract_id':pip_contract['id'],'source_records':recs,'observation':{'kind':'prefer_member','member':{'level':'parcel','id':h},'cost_if_absent':1}})
        contracts.append(pip_contract); npip+=1
    if obs: overlays.append({'case_id':sid,'contracts':contracts,'observations':obs})
req={'version':'canon_geo_population_evidence_stack_request.v0','case_overlays':overlays,'max_overlay_cases':70,'max_overlay_observations':2000}
json.dump(req,open('overlay_mcp.json','w'))
print("overlay cases:",len(overlays),"size-band cases:",nband,"pip cases:",npip,"observations:",sum(len(o['observations']) for o in overlays))
