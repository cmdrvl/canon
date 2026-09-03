import json, re, collections, blake3
exec(open('overlay_build2.py').read().split("# calibration on truth")[0])  # reuse norm/tokens/match/attrs/parties/byDoc/subs/req
fp=json.load(open('footprints.json')); props=json.load(open('nyc_props.json'))
bridge={r['sid']:r for r in json.load(open('condo_bridge_pad.json'))['rows']}
def b3(o): return blake3.blake3(json.dumps(o,sort_keys=True,separators=(',',':')).encode()).hexdigest()
# footprint calibration (bridged truth)
cal=[]; 
for sid,cand in req.items():
    s=subs[sid]; truth=bridge[sid]['bridged_truth'] if sid in bridge else s['truth_parcels']
    ps=[p for p in props if p['LOAN_KEY']==s['loan_key'] and p['RECORDED_BOROUGH']==s['legal_borough']]
    if not truth or not ps or any(t not in fp for t in truth): continue
    n=ps[0]['LOAN_COUNTY_PROPERTY_COUNT']; bins=sum(fp[t]['bins'] for t in truth)
    cal.append({'subject_id':sid,'property_count':n,'truth_buildings':bins,'holds': bins>=max(0,n-1)})
fcal={'population_id':'h7-d1-residuals-2026-09-03','method':'sum(NYC Building Footprints active BIN count over truth lots, latest release) >= LOAN_COUNTY_PROPERTY_COUNT - 1','rows':cal,'coverage':{'n':len(cal),'holds':sum(r['holds'] for r in cal)}}
json.dump(fcal,open('calibration_footprint.json','w'),indent=1); fh=b3(fcal); print("footprint calibration:",fcal['coverage'])
fcontract={'id':'rho.footprint.building_count_floor','version':'1.0.0','source_dataset':'EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT_x_LOAN_ISSUANCE_PROPERTY','source_release':'footprints-latest_lip-current','source_lineage_ids':sorted(['EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT:latest','EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:current']),'method_id':'active-bin-count-floor','method_version':'1.0.0','claim_role':'attribute_observation','basis':{'kind':'empirical_calibration','population_id':fcal['population_id'],'calibration_blake3':fh,'falsification_rule_id':'truth-buildings-below-floor','admissible_hard_band':True}}
ocal=json.load(open('calibration_owner.json')); oh=b3(ocal)
ocontract={'id':'rho.owner.taxroll_borrower_match_soft','version':'1.0.0','source_dataset':'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_PLUTO_LOT_VINTAGES_x_STG_GEO_NYC_ACRIS_PARTIES','source_release':'26v1_acris-latest','source_lineage_ids':sorted(['EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_PLUTO_LOT_VINTAGES:26v1','EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:latest']),'method_id':'taxroll-owner-borrower-name-preference','method_version':'1.0.0','claim_role':'stable_identity_anchor','basis':{'kind':'empirical_calibration','population_id':ocal['population_id'],'calibration_blake3':oh,'falsification_rule_id':'truth-lot-owner-mismatch'}}
overlays=[]; nf=no=0
for sid,cand in req.items():
    s=subs[sid]; obs=[]; contracts=[]
    ps=[p for p in props if p['LOAN_KEY']==s['loan_key'] and p['RECORDED_BOROUGH']==s['legal_borough']]
    if ps and all(c in fp for c in cand):
        n=ps[0]['LOAN_COUNTY_PROPERTY_COUNT']; floor=max(0,n-1)
        if floor>0:
            recs=[{'source_record_id':'EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:%s:county_count'%s['loan_key'],'source_vintage':'current','record_blake3':b3({'loan':s['loan_key'],'n':n})}]
            recs+=[{'source_record_id':'EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT:latest:%s'%c,'source_vintage':'latest','record_blake3':b3(fp[c])} for c in cand]
            obs.append({'id':'obs.footprint.building_count_floor:%s'%sid,'contract_id':fcontract['id'],'source_records':recs,'observation':{'kind':'integer_sum_band','level':'parcel','measure':{'semantic_id':'footprints.active_bin_count','unit':'buildings','value_origin':'source_asserted'},'values':[{'id':c,'value':fp[c]['bins']} for c in cand],'min':floor,'max':10**9}})
            contracts.append(fcontract); nf+=1
    pl=byDoc.get(s['document_id'],[]); bor={p['norm'] for p in pl}
    if bor:
        matched=[c for c in cand if c in attrs and match(attrs[c]['owner'],bor) in ('exact','token')]
        if matched:
            recs={r['source_record_id']:r for r in [{'source_record_id':'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:%s:%s'%(p['doc'],p['norm'].replace(' ','_')),'source_vintage':str(p['release']),'record_blake3':b3(p)} for p in pl]}
            recs=list(recs.values())+[{'source_record_id':'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_PLUTO_LOT_VINTAGES:26v1:%s'%c,'source_vintage':'26v1_2026-05-01','record_blake3':b3(attrs[c])} for c in matched]
            for c in matched:
                obs.append({'id':'obs.owner.taxroll_borrower_match_soft:%s:%s'%(sid,c),'contract_id':ocontract['id'],'source_records':recs,'observation':{'kind':'prefer_member','member':{'level':'parcel','id':c},'cost_if_absent':2}})
            contracts.append(ocontract); no+=1
    if obs: overlays.append({'case_id':sid,'contracts':contracts,'observations':obs})
json.dump({'version':'canon_geo_population_evidence_stack_request.v0','case_overlays':overlays,'max_overlay_cases':70,'max_overlay_observations':2000},open('overlay_soft.json','w'))
print("overlay cases:",len(overlays),"footprint floors:",nf,"owner soft cases:",no)
