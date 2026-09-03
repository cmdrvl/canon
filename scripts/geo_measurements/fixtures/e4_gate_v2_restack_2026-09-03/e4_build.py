import json, collections, blake3
exec(open('overlay_build2.py').read().split("# calibration on truth")[0])   # norm/tokens/match, attrs, parties, byDoc, subs, req(D1)
def b3(o): return blake3.blake3(json.dumps(o,sort_keys=True,separators=(',',':')).encode()).hexdigest()
roll=json.load(open('roll.json')); props=json.load(open('nyc_props.json')); sizes=json.load(open('sizes.json'))
pip={r['subject_id']:r for r in json.load(open('pip_scored.json'))}
retained={c['id']:c for c in json.load(open('/Users/zac/Source/cmdrvl/canon/scripts/geo_measurements/fixtures/d1_residuals/d1_population_evidence_stack.json'))['population']['cases']}
bind={c['case_id']:c for c in json.load(open('e4_case_bindings.json'))['cases']}
e4=json.load(open('/Users/zac/Source/cmdrvl/canon/tests/fixtures/geo/e4_gate_v2_population_request.json'))
subs_list=json.load(open('subjects.json'))
by_truth={}
for s in subs_list: by_truth.setdefault(frozenset(s['truth_parcels']), s)
by_block=collections.defaultdict(list)
for x in roll: by_block[x[:6]].append(x)
ocal=json.load(open('calibration_roll_owner_exact.json')); bcal=json.load(open('calibration_roll_gsf_band.json')); LO,HI=bcal['band']
def gsf(x):
    v=roll.get(x,{}).get('gross_sqft')
    try: return int(float(v)) if v not in (None,'') else None
    except: return None
ROLL_LIN=sorted(['EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:latest','EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.WRGL_NYC_OPENDATA_PROPERTY_VALUATION_AND_ASSESSMENT_DATA_TAX_CLASSES_1_2_3_4__STRUCTURED:FY2026P3'])
own_hard={'id':'rho.owner.assessment_roll_exact_match','version':'1.0.0','source_dataset':'EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION_FY2026P3_x_ACRIS_PARTIES','source_release':'FY2026P3_acris-latest','source_lineage_ids':ROLL_LIN,'method_id':'assessment-roll-owner-exact-exclusion','method_version':'1.0.0','claim_role':'stable_identity_anchor','basis':{'kind':'empirical_calibration','population_id':ocal['population_id'],'calibration_blake3':b3(ocal),'falsification_rule_id':'truth-lot-owner-not-exact','admissible_hard_band':True}}
own_soft=dict(own_hard, id='rho.owner.assessment_roll_affiliate_preference', method_id='assessment-roll-owner-token-preference', basis={'kind':'empirical_calibration','population_id':ocal['population_id'],'calibration_blake3':b3(ocal),'falsification_rule_id':'truth-lot-owner-mismatch'})
band_c={'id':'rho.size.assessment_roll_gross_sqft_band','version':'1.0.0','source_dataset':'EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION_FY2026P3_x_PROPERTY_PERIOD_FACT','source_release':'FY2026P3_ppf-latest','source_lineage_ids':sorted(['EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.WRGL_NYC_OPENDATA_PROPERTY_VALUATION_AND_ASSESSMENT_DATA_TAX_CLASSES_1_2_3_4__STRUCTURED:FY2026P3','EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT:latest_reporting_period']),'method_id':'asserted-sqft-roll-gross-sum-band','method_version':'1.0.0','claim_role':'attribute_observation','basis':{'kind':'empirical_calibration','population_id':bcal['population_id'],'calibration_blake3':b3(bcal),'falsification_rule_id':'truth-gross-sum-outside-band','admissible_hard_band':True}}
geo_c=json.load(open('overlay_mcp.json'))['case_overlays'][0]['contracts'][-1]
pop_base=json.loads(json.dumps(e4)); pop_roll=json.loads(json.dumps(e4))
ov_base=[]; ov_roll=[]; n=collections.Counter()
for cb,cr in zip(pop_base['cases'],pop_roll['cases']):
    cid=cb['id']; s=by_truth.get(frozenset(cb['truth']['parcels']))
    u=set(cb['evidence']['universe']['parcels']); blocks={p[:6] for p in u}
    u2=sorted(u | {x for blk in blocks for x in by_block[blk]}); cr['evidence']['universe']['parcels']=u2
    for c in (cb,cr): c['evidence']['contracts']=[]; c['evidence']['observations']=[]
    obs_b=[]; con_b=[]; obs_r=[]; con_r=[]
    if s and s['subject_id'] in retained:
        rc=retained[s['subject_id']]['evidence']
        pad_obs=[o for o in rc['observations'] if all(m['id'] in u for m in o['observation'].get('members',[]))]
        pad_obs_r=[o for o in rc['observations'] if all(m['id'] in set(u2) for m in o['observation'].get('members',[]))]
        obs_b+=pad_obs; obs_r+=pad_obs_r; n['pad']+=len(pad_obs)
        if pad_obs: con_b+=rc['contracts']
        if pad_obs_r: con_r+=rc['contracts']
    bor=set(bind[cid]['borrower_names_norm']); pl=[p for d in bind[cid]['document_ids'] for p in byDoc.get(d,[])]
    if bor:
        kinds={x:(match(roll[x]['owner'],bor) if x in roll else 'no_owner') for x in u2}
        ex=[x for x,k in kinds.items() if k=='exact']; tok=[x for x,k in kinds.items() if k=='token']
        recs={r['source_record_id']:r for r in [{'source_record_id':'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:%s:%s'%(p['doc'],p['norm'].replace(' ','_')),'source_vintage':str(p['release']),'record_blake3':b3(p)} for p in pl]}; recs=list(recs.values())
        if ex:
            obs_r.append({'id':'obs.owner.assessment_roll_exact_match:%s'%cid,'contract_id':own_hard['id'],'source_records':recs+[{'source_record_id':'EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:%s'%x,'source_vintage':'FY2026P3','record_blake3':b3(roll.get(x,{}))} for x in u2],'observation':{'kind':'integer_sum_band','level':'parcel','measure':{'semantic_id':'assessment_roll.owner_not_exact','unit':'lots','value_origin':'source_asserted'},'values':[{'id':x,'value':0 if x in ex else 1} for x in u2],'min':0,'max':0}}); con_r.append(own_hard); n['exact_hard']+=1
        if tok:
            r3=recs+[{'source_record_id':'EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:%s'%x,'source_vintage':'FY2026P3','record_blake3':b3(roll.get(x,{}))} for x in tok]
            for x in tok: obs_r.append({'id':'obs.owner.assessment_roll_affiliate_preference:%s:%s'%(cid,x),'contract_id':own_soft['id'],'source_records':r3,'observation':{'kind':'prefer_member','member':{'level':'parcel','id':x},'cost_if_absent':1}})
            con_r.append(own_soft); n['affil']+=1
    if s:
        ps=[p for p in props if p['LOAN_KEY']==s['loan_key'] and p['RECORDED_BOROUGH']==s['legal_borough']]
        sq=[(p['PROPERTY_KEY'],sizes[p['PROPERTY_KEY']]) for p in ps if sizes.get(p['PROPERTY_KEY'],{}).get('measure')=='SQFT' and (sizes[p['PROPERTY_KEY']]['size'] or 0)>=500]
        if sq and all(gsf(x) is not None for x in u2):
            a=sum(v['size'] for _,v in sq)
            obs_r.append({'id':'obs.size.assessment_roll_gross_sqft_band:%s'%cid,'contract_id':band_c['id'],'source_records':[{'source_record_id':'EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT:%s'%k,'source_vintage':'latest_reporting_period','record_blake3':b3(v)} for k,v in sq]+[{'source_record_id':'EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:gsf:%s'%x,'source_vintage':'FY2026P3','record_blake3':b3(roll[x])} for x in u2],'observation':{'kind':'integer_sum_band','level':'parcel','measure':{'semantic_id':'assessment_roll.gross_sqft','unit':'sqft','value_origin':'source_asserted'},'values':[{'id':x,'value':gsf(x)} for x in u2],'min':int(LO*a),'max':int(HI*a)+1}}); con_r.append(band_c); n['band']+=1
        hits=[h for h in pip.get(s['subject_id'],{}).get('hits',[]) if h in set(u2)]
        if hits and ps:
            r5=[{'source_record_id':'EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:%s:%s'%(p['LOAN_KEY'],p['PROPERTY_KEY']),'source_vintage':'current','record_blake3':b3({k:p[k] for k in ('LATITUDE','LONGITUDE','PROPERTY_KEY')})} for p in ps]
            for h in hits: obs_r.append({'id':'obs.address.geocode.parcel_containment:%s:%s'%(cid,h),'contract_id':geo_c['id'],'source_records':r5,'observation':{'kind':'prefer_member','member':{'level':'parcel','id':h},'cost_if_absent':1}})
            con_r.append(geo_c); n['geo']+=1
    if obs_b: ov_base.append({'case_id':cid,'contracts':con_b,'observations':obs_b})
    if obs_r: ov_roll.append({'case_id':cid,'contracts':con_r,'observations':obs_r})
json.dump(pop_base,open('e4_pop_base.json','w')); json.dump(pop_roll,open('e4_pop_roll.json','w'))
json.dump({'version':'canon_geo_population_evidence_stack_request.v0','case_overlays':ov_base,'max_overlay_cases':15,'max_overlay_observations':500},open('e4_overlay_base.json','w'))
json.dump({'version':'canon_geo_population_evidence_stack_request.v0','case_overlays':ov_roll,'max_overlay_cases':15,'max_overlay_observations':2000},open('e4_overlay_roll.json','w'))
print("base overlays",len(ov_base),"roll overlays",len(ov_roll),dict(n),"universe growth",[(len(a['evidence']['universe']['parcels']),len(b['evidence']['universe']['parcels'])) for a,b in zip(pop_base['cases'],pop_roll['cases'])])
