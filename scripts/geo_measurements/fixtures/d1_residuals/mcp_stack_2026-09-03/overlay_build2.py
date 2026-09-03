import json, re, collections, blake3
def b3(o): return blake3.blake3(json.dumps(o,sort_keys=True,separators=(',',':')).encode()).hexdigest()
def norm(s): return re.sub(r'[^A-Z0-9 ]',' ',(s or '').upper()).strip()
STOP={'LLC','INC','CORP','L','P','LP','THE','OF','CO','LTD','OWNER','OWNERS','REALTY','ASSOCIATES','HOLDINGS','COMPANY','PROPERTY','PROPERTIES','TENANTS','APARTMENT','APARTMENTS'}
def tokens(s): return [t for t in norm(s).split() if t not in STOP]
def match(owner,borrowers):
    o=norm(owner)
    if not o: return 'no_owner'
    if o in borrowers: return 'exact'
    ot=set(tokens(owner))
    for b in borrowers:
        bt=set(tokens(b))
        if ot and bt and (ot<=bt or bt<=ot or len(ot&bt)>=2): return 'token'
    return 'none'
attrs=json.load(open('lot_attrs.json')); parties=json.load(open('parties.json'))
byDoc=collections.defaultdict(list)
for p in parties: byDoc[p['doc']].append(p)
subs={}; 
for s in json.load(open('subjects.json')): subs.setdefault(s['subject_id'],s)
req={c['id']:c['evidence']['universe']['parcels'] for c in json.load(open('/Users/zac/Source/cmdrvl/canon/scripts/geo_measurements/fixtures/d1_residuals/h7_population_request.json'))['cases']}
# calibration on truth (lot grain and subject grain)
cal_rows=[]; lot_n=lot_ok=0
for sid,cand in req.items():
    s=subs[sid]; bor={p['norm'] for p in byDoc.get(s['document_id'],[])}
    if not bor: continue
    t_attr=[t for t in s['truth_parcels'] if t in attrs]
    if not t_attr: continue
    kinds=[match(attrs[t]['owner'],bor) for t in t_attr]
    ok=sum(k in ('exact','token') for k in kinds); lot_n+=len(kinds); lot_ok+=ok
    cal_rows.append({'subject_id':sid,'truth_lots_with_owner':len(kinds),'truth_lots_matching':ok,'kinds':kinds})
calib={'population_id':'h7-d1-residuals-2026-09-03','method':'MapPLUTO/PLUTO 26v1 OWNERNAME vs ACRIS party_type 1 PARTY_NAME_NORM for the subject document; exact norm equality or stop-word-stripped token containment / >=2 shared tokens','rule':'a lot whose tax owner matches no borrower is excluded (owner_mismatch sum == 0); applied only when at least one candidate lot matches','rows':cal_rows,'coverage':{'subjects':len(cal_rows),'subjects_all_truth_match':sum(r['truth_lots_matching']==r['truth_lots_with_owner'] for r in cal_rows),'truth_lots':lot_n,'truth_lots_matching':lot_ok}}
json.dump(calib,open('calibration_owner.json','w'),indent=1); ch=b3(calib)
print("owner calibration:",calib['coverage'],ch[:12])
contract={'id':'rho.owner.taxroll_borrower_match','version':'1.0.0','source_dataset':'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_PLUTO_LOT_VINTAGES_x_STG_GEO_NYC_ACRIS_PARTIES','source_release':'26v1_acris-latest','source_lineage_ids':sorted(['EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_PLUTO_LOT_VINTAGES:26v1','EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:latest']),'method_id':'taxroll-owner-borrower-name-exclusion','method_version':'1.0.0','claim_role':'stable_identity_anchor','basis':{'kind':'empirical_calibration','population_id':calib['population_id'],'calibration_blake3':ch,'falsification_rule_id':'truth-lot-owner-mismatch','admissible_hard_band':True}}
overlays=[]; applied=0; skipped=collections.Counter()
for sid,cand in req.items():
    s=subs[sid]; ps=byDoc.get(s['document_id'],[]); bor={p['norm'] for p in ps}
    if not bor: skipped['no_borrower']+=1; continue
    vals=[]; kept=0
    for c in cand:
        m=match(attrs.get(c,{}).get('owner'),bor) if c in attrs else 'no_owner'
        mism=0 if m in ('exact','token') else 1
        kept+= (mism==0); vals.append({'id':c,'value':mism})
    if kept==0: skipped['no_candidate_matches']+=1; continue
    recs=[{'source_record_id':'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:%s:%s'%(p['doc'],p['norm'].replace(' ','_')),'source_vintage':str(p['release']),'record_blake3':b3(p)} for p in ps]
    recs={r['source_record_id']:r for r in recs}; recs=list(recs.values())
    recs+=[{'source_record_id':'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_PLUTO_LOT_VINTAGES:26v1:%s'%c,'source_vintage':'26v1_2026-05-01','record_blake3':b3(attrs.get(c,{}))} for c in cand]
    overlays.append({'case_id':sid,'contracts':[contract],'observations':[{'id':'obs.owner.taxroll_borrower_match:%s'%sid,'contract_id':contract['id'],'source_records':recs,'observation':{'kind':'integer_sum_band','level':'parcel','measure':{'semantic_id':'taxroll.owner_mismatch','unit':'lots','value_origin':'source_asserted'},'values':vals,'min':0,'max':0}}]}); applied+=1
json.dump({'version':'canon_geo_population_evidence_stack_request.v0','case_overlays':overlays,'max_overlay_cases':70,'max_overlay_observations':500},open('overlay_owner.json','w'))
print("owner overlay cases:",applied,"skipped:",dict(skipped))
