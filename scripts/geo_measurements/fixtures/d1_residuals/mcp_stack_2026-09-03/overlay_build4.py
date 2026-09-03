import json, collections, blake3
exec(open('overlay_build2.py').read().split("# calibration on truth")[0])
def b3(o): return blake3.blake3(json.dumps(o,sort_keys=True,separators=(',',':')).encode()).hexdigest()
roll=json.load(open('roll.json')); props=json.load(open('nyc_props.json')); sizes=json.load(open('sizes.json'))
pip={r['subject_id']:r for r in json.load(open('pip_scored.json'))}
retained=json.load(open('/Users/zac/Source/cmdrvl/canon/scripts/geo_measurements/fixtures/d1_residuals/d1_population_evidence_stack.json'))
ret_cases={c['id']:c for c in retained['population']['cases']}
popreq=json.load(open('/Users/zac/Source/cmdrvl/canon/scripts/geo_measurements/fixtures/d1_residuals/h7_population_request.json'))
by_block=collections.defaultdict(list)
for b in roll: by_block[b[:6]].append(b)
# --- population v2: universe = PLUTO universe ∪ roll lots on the same blocks; truth unchanged
new_cases=[]; growth=[]
for c in popreq['cases']:
    u=set(c['evidence']['universe']['parcels']); blocks={p[:6] for p in u}
    u2=sorted(u | {b for blk in blocks for b in by_block[blk]})
    growth.append((len(u),len(u2)))
    c2=json.loads(json.dumps(c)); c2['evidence']['universe']['parcels']=u2; c2['evidence']['contracts']=[]; c2['evidence']['observations']=[]
    new_cases.append(c2)
pop2=dict(popreq); pop2['cases']=new_cases
json.dump(pop2,open('population_roll.json','w'))
print("universe growth (min/median/max):",min(g[1]/g[0] for g in growth),sorted(g[1]/g[0] for g in growth)[len(growth)//2],max(g[1]/g[0] for g in growth),"| max universe:",max(g[1] for g in growth))
# --- calibration: roll owner vs borrower on truth
cal=[]; ln=lo=0
for c in popreq['cases']:
    sid=c['id']; s=subs[sid]; bor={p['norm'] for p in byDoc.get(s['document_id'],[])}
    t_in=[t for t in s['truth_parcels'] if t in roll]
    if not bor or not t_in: continue
    kinds=[match(roll[t]['owner'],bor) for t in t_in]; ok=sum(k in('exact','token') for k in kinds); ln+=len(kinds); lo+=ok
    cal.append({'subject_id':sid,'truth_lots':len(kinds),'matching':ok})
rcal={'population_id':'h7-d1-residuals-2026-09-03-roll','method':'DOF assessment roll FY2026 final (PERIOD 3) OWNER vs ACRIS party_type 1 names; exact norm equality or stop-word-stripped token containment / >=2 shared tokens; unit-lot grain','rows':cal,'coverage':{'subjects':len(cal),'subjects_all_match':sum(r['matching']==r['truth_lots'] for r in cal),'truth_lots':ln,'truth_lots_matching':lo}}
json.dump(rcal,open('calibration_roll_owner.json','w'),indent=1); rh=b3(rcal); print("roll owner calibration:",rcal['coverage'])
LIN=sorted(['EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:latest','EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.WRGL_NYC_OPENDATA_PROPERTY_VALUATION_AND_ASSESSMENT_DATA_TAX_CLASSES_1_2_3_4__STRUCTURED:FY2026P3'])
own_c={'id':'rho.owner.assessment_roll_borrower_match','version':'1.0.0','source_dataset':'EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION_FY2026P3_x_ACRIS_PARTIES','source_release':'FY2026P3_acris-latest','source_lineage_ids':LIN,'method_id':'assessment-roll-owner-borrower-exclusion','method_version':'1.0.0','claim_role':'stable_identity_anchor','basis':{'kind':'empirical_calibration','population_id':rcal['population_id'],'calibration_blake3':rh,'falsification_rule_id':'truth-lot-owner-mismatch','admissible_hard_band':True}}
pad_c=None; geo_c=json.load(open('overlay_mcp.json'))['case_overlays'][0]['contracts'][-1]
overlays=[]; n_pad=n_own=n_geo=0
for c in new_cases:
    sid=c['id']; s=subs[sid]; cand=c['evidence']['universe']['parcels']; obs=[]; contracts=[]
    rc=ret_cases[sid]['evidence']
    for o in rc['observations']:
        obs.append(o); n_pad+=1
    for ct in rc['contracts']:
        if ct not in contracts: contracts.append(ct)
    bor={p['norm'] for p in byDoc.get(s['document_id'],[])}; pl=byDoc.get(s['document_id'],[])
    if bor:
        vals=[]; kept=0
        for x in cand:
            m=match(roll[x]['owner'],bor) if x in roll else 'no_owner'; mism=0 if m in('exact','token') else 1; kept+=(mism==0); vals.append({'id':x,'value':mism})
        if kept:
            recs={r['source_record_id']:r for r in [{'source_record_id':'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:%s:%s'%(p['doc'],p['norm'].replace(' ','_')),'source_vintage':str(p['release']),'record_blake3':b3(p)} for p in pl]}
            recs=list(recs.values())+[{'source_record_id':'EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:%s'%x,'source_vintage':'FY2026P3','record_blake3':b3(roll.get(x,{}))} for x in cand]
            obs.append({'id':'obs.owner.assessment_roll_borrower_match:%s'%sid,'contract_id':own_c['id'],'source_records':recs,'observation':{'kind':'integer_sum_band','level':'parcel','measure':{'semantic_id':'assessment_roll.owner_mismatch','unit':'lots','value_origin':'source_asserted'},'values':vals,'min':0,'max':0}}); contracts.append(own_c); n_own+=1
    hits=[h for h in pip.get(sid,{}).get('hits',[]) if h in set(cand)]
    ps=[p for p in props if p['LOAN_KEY']==s['loan_key'] and p['RECORDED_BOROUGH']==s['legal_borough']]
    if hits and ps:
        recs=[{'source_record_id':'EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:%s:%s'%(p['LOAN_KEY'],p['PROPERTY_KEY']),'source_vintage':'current','record_blake3':b3({k:p[k] for k in ('LATITUDE','LONGITUDE','PROPERTY_KEY')})} for p in ps]
        for h in hits: obs.append({'id':'obs.address.geocode.parcel_containment:%s:%s'%(sid,h),'contract_id':geo_c['id'],'source_records':recs,'observation':{'kind':'prefer_member','member':{'level':'parcel','id':h},'cost_if_absent':1}})
        contracts.append(geo_c); n_geo+=1
    if obs: overlays.append({'case_id':sid,'contracts':contracts,'observations':obs})
json.dump({'version':'canon_geo_population_evidence_stack_request.v0','case_overlays':overlays,'max_overlay_cases':70,'max_overlay_observations':2000},open('overlay_roll.json','w'))
print("overlay cases:",len(overlays),"pad obs:",n_pad,"owner:",n_own,"geo:",n_geo)
