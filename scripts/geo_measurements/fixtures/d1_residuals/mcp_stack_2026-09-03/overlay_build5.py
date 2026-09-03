import json, collections, blake3
exec(open('overlay_build2.py').read().split("# calibration on truth")[0])
def b3(o): return blake3.blake3(json.dumps(o,sort_keys=True,separators=(',',':')).encode()).hexdigest()
roll=json.load(open('roll.json')); props=json.load(open('nyc_props.json')); sizes=json.load(open('sizes.json'))
pip={r['subject_id']:r for r in json.load(open('pip_scored.json'))}
pop={c['id']:c for c in json.load(open('population_roll.json'))['cases']}
retained={c['id']:c for c in json.load(open('/Users/zac/Source/cmdrvl/canon/scripts/geo_measurements/fixtures/d1_residuals/d1_population_evidence_stack.json'))['population']['cases']}
geo_c=json.load(open('overlay_mcp.json'))['case_overlays'][0]['contracts'][-1]
def gsf(x):
    v=roll.get(x,{}).get('gross_sqft'); 
    try: return int(float(v)) if v not in (None,'') else None
    except: return None
# --- calibrations on truth
exact_rows=[]; en=eo=0; band_rows=[]
for sid,c in pop.items():
    s=subs[sid]; bor={p['norm'] for p in byDoc.get(s['document_id'],[])}
    t_in=[t for t in s['truth_parcels'] if t in roll]
    if bor and t_in:
        ks=[match(roll[t]['owner'],bor) for t in t_in]; ex=sum(k=='exact' for k in ks); en+=len(ks); eo+=ex
        exact_rows.append({'subject_id':sid,'truth_lots':len(ks),'exact':ex,'token':sum(k=='token' for k in ks)})
    ps=[p for p in props if p['LOAN_KEY']==s['loan_key'] and p['RECORDED_BOROUGH']==s['legal_borough']]
    sq=[sizes[p['PROPERTY_KEY']]['size'] for p in ps if sizes.get(p['PROPERTY_KEY'],{}).get('measure')=='SQFT' and (sizes[p['PROPERTY_KEY']]['size'] or 0)>=500]
    if sq and t_in and len(t_in)==len(s['truth_parcels']) and all(gsf(t) is not None for t in t_in):
        a=sum(sq); tb=sum(gsf(t) for t in t_in); band_rows.append({'subject_id':sid,'asserted':a,'truth_gross':tb,'ratio':round(tb/a,3)})
ratios=sorted(r['ratio'] for r in band_rows); print("roll gsf/asserted ratios:",ratios)
LO,HI=0.7,1.6
cov=sum(LO<=r<=HI for r in ratios)
ecal={'population_id':'h7-d1-residuals-2026-09-03-roll','method':'exact equality of normalized assessment-roll OWNER and ACRIS party_type 1 name','rows':exact_rows,'coverage':{'subjects':len(exact_rows),'truth_lots':en,'exact':eo,'subjects_all_exact':sum(r['exact']==r['truth_lots'] for r in exact_rows)}}
bcal={'population_id':'h7-d1-residuals-2026-09-03-roll','method':'sum(assessment roll GROSS_SQFT over truth lots)/sum(ABS-EE SIZE sqft>=500)','band':[LO,HI],'rows':band_rows,'coverage':{'n':len(band_rows),'in_band':cov}}
json.dump(ecal,open('calibration_roll_owner_exact.json','w'),indent=1); json.dump(bcal,open('calibration_roll_gsf_band.json','w'),indent=1)
print("exact owner calibration:",ecal['coverage']); print("gsf band calibration:",bcal['coverage'])
ROLL_LIN=sorted(['EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:latest','EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.WRGL_NYC_OPENDATA_PROPERTY_VALUATION_AND_ASSESSMENT_DATA_TAX_CLASSES_1_2_3_4__STRUCTURED:FY2026P3'])
own_hard={'id':'rho.owner.assessment_roll_exact_match','version':'1.0.0','source_dataset':'EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION_FY2026P3_x_ACRIS_PARTIES','source_release':'FY2026P3_acris-latest','source_lineage_ids':ROLL_LIN,'method_id':'assessment-roll-owner-exact-exclusion','method_version':'1.0.0','claim_role':'stable_identity_anchor','basis':{'kind':'empirical_calibration','population_id':ecal['population_id'],'calibration_blake3':b3(ecal),'falsification_rule_id':'truth-lot-owner-not-exact','admissible_hard_band':True}}
own_soft={'id':'rho.owner.assessment_roll_affiliate_preference','version':'1.0.0','source_dataset':'EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION_FY2026P3_x_ACRIS_PARTIES','source_release':'FY2026P3_acris-latest','source_lineage_ids':ROLL_LIN,'method_id':'assessment-roll-owner-token-preference','method_version':'1.0.0','claim_role':'stable_identity_anchor','basis':{'kind':'empirical_calibration','population_id':ecal['population_id'],'calibration_blake3':b3(ecal),'falsification_rule_id':'truth-lot-owner-mismatch'}}
band_c={'id':'rho.size.assessment_roll_gross_sqft_band','version':'1.0.0','source_dataset':'EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION_FY2026P3_x_PROPERTY_PERIOD_FACT','source_release':'FY2026P3_ppf-latest','source_lineage_ids':sorted(['EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.WRGL_NYC_OPENDATA_PROPERTY_VALUATION_AND_ASSESSMENT_DATA_TAX_CLASSES_1_2_3_4__STRUCTURED:FY2026P3','EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT:latest_reporting_period']),'method_id':'asserted-sqft-roll-gross-sum-band','method_version':'1.0.0','claim_role':'attribute_observation','basis':{'kind':'empirical_calibration','population_id':bcal['population_id'],'calibration_blake3':b3(bcal),'falsification_rule_id':'truth-gross-sum-outside-band','admissible_hard_band':True}}
overlays=[]; n=collections.Counter()
for sid,c in pop.items():
    s=subs[sid]; cand=c['evidence']['universe']['parcels']; cs=set(cand); obs=[]; contracts=[]
    rc=retained[sid]['evidence']
    obs+=rc['observations']; contracts+=[ct for ct in rc['contracts'] if ct not in contracts]; n['pad']+=len(rc['observations'])
    pl=byDoc.get(s['document_id'],[]); bor={p['norm'] for p in pl}
    if bor:
        kinds={x:(match(roll[x]['owner'],bor) if x in roll else 'no_owner') for x in cand}
        ex=[x for x,k in kinds.items() if k=='exact']; tok=[x for x,k in kinds.items() if k=='token']
        recs={r['source_record_id']:r for r in [{'source_record_id':'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:%s:%s'%(p['doc'],p['norm'].replace(' ','_')),'source_vintage':str(p['release']),'record_blake3':b3(p)} for p in pl]}; recs=list(recs.values())
        if ex:
            r2=recs+[{'source_record_id':'EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:%s'%x,'source_vintage':'FY2026P3','record_blake3':b3(roll.get(x,{}))} for x in cand]
            obs.append({'id':'obs.owner.assessment_roll_exact_match:%s'%sid,'contract_id':own_hard['id'],'source_records':r2,'observation':{'kind':'integer_sum_band','level':'parcel','measure':{'semantic_id':'assessment_roll.owner_not_exact','unit':'lots','value_origin':'source_asserted'},'values':[{'id':x,'value':0 if x in ex else 1} for x in cand],'min':0,'max':0}}); contracts.append(own_hard); n['exact_hard']+=1
        if tok:
            r3=recs+[{'source_record_id':'EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:%s'%x,'source_vintage':'FY2026P3','record_blake3':b3(roll.get(x,{}))} for x in tok]
            for x in tok: obs.append({'id':'obs.owner.assessment_roll_affiliate_preference:%s:%s'%(sid,x),'contract_id':own_soft['id'],'source_records':r3,'observation':{'kind':'prefer_member','member':{'level':'parcel','id':x},'cost_if_absent':1}})
            contracts.append(own_soft); n['affiliate_soft']+=1
    ps=[p for p in props if p['LOAN_KEY']==s['loan_key'] and p['RECORDED_BOROUGH']==s['legal_borough']]
    sq=[(p['PROPERTY_KEY'],sizes[p['PROPERTY_KEY']]) for p in ps if sizes.get(p['PROPERTY_KEY'],{}).get('measure')=='SQFT' and (sizes[p['PROPERTY_KEY']]['size'] or 0)>=500]
    if sq and all(gsf(x) is not None for x in cand):
        a=sum(v['size'] for _,v in sq)
        r4=[{'source_record_id':'EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT:%s'%k,'source_vintage':'latest_reporting_period','record_blake3':b3(v)} for k,v in sq]+[{'source_record_id':'EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:gsf:%s'%x,'source_vintage':'FY2026P3','record_blake3':b3(roll[x])} for x in cand]
        obs.append({'id':'obs.size.assessment_roll_gross_sqft_band:%s'%sid,'contract_id':band_c['id'],'source_records':r4,'observation':{'kind':'integer_sum_band','level':'parcel','measure':{'semantic_id':'assessment_roll.gross_sqft','unit':'sqft','value_origin':'source_asserted'},'values':[{'id':x,'value':gsf(x)} for x in cand],'min':int(LO*a),'max':int(HI*a)+1}}); contracts.append(band_c); n['gsf_band']+=1
    hits=[h for h in pip.get(sid,{}).get('hits',[]) if h in cs]
    if hits and ps:
        r5=[{'source_record_id':'EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:%s:%s'%(p['LOAN_KEY'],p['PROPERTY_KEY']),'source_vintage':'current','record_blake3':b3({k:p[k] for k in ('LATITUDE','LONGITUDE','PROPERTY_KEY')})} for p in ps]
        for h in hits: obs.append({'id':'obs.address.geocode.parcel_containment:%s:%s'%(sid,h),'contract_id':geo_c['id'],'source_records':r5,'observation':{'kind':'prefer_member','member':{'level':'parcel','id':h},'cost_if_absent':1}})
        contracts.append(geo_c); n['geo']+=1
    if obs: overlays.append({'case_id':sid,'contracts':contracts,'observations':obs})
json.dump({'version':'canon_geo_population_evidence_stack_request.v0','case_overlays':overlays,'max_overlay_cases':70,'max_overlay_observations':3000},open('overlay_roll2.json','w'))
print("overlay cases:",len(overlays),dict(n))
