import json, collections
exec(open('overlay_build2.py').read().split("# calibration on truth")[0])
d=json.load(open('roll_agg.json')); roll={}
for r in d['rows']:
    for item in r['LOTS'].split('^'):
        f=item.split('~')
        if len(f)<8: continue
        bbl,owner,units,gsf,condo,apt,cls,mkt=f[:8]
        roll[bbl]={'owner':owner,'units':units,'gross_sqft':gsf,'condo':condo,'apt':apt,'cls':cls,'mkt':mkt}
json.dump(roll,open('roll.json','w')); print("roll lots:",len(roll),"blocks:",d['row_count'])
st=collections.Counter(); lot_n=lot_ok=0; unit_n=unit_ok=0
for sid in req:
    s=subs[sid]; bor={p['norm'] for p in byDoc.get(s['document_id'],[])}
    if not bor: continue
    t_in=[t for t in s['truth_parcels'] if t in roll]
    st['truth_lots_in_roll']+=len(t_in); st['truth_lots']+=len(s['truth_parcels'])
    if not t_in: st['subject_no_roll_truth']+=1; continue
    kinds=[match(roll[t]['owner'],bor) for t in t_in]; ok=sum(k in('exact','token') for k in kinds); lot_n+=len(kinds); lot_ok+=ok
    is_unit=[1001<=int(t[6:])<7500 for t in t_in]
    unit_n+=sum(is_unit); unit_ok+=sum(1 for k,u in zip(kinds,is_unit) if u and k in('exact','token'))
    st['subject_all_match' if ok==len(kinds) else ('subject_some_match' if ok else 'subject_no_match')]+=1
print(dict(st)); print("roll owner match on truth lots:",lot_ok,"/",lot_n,"| on condo unit lots:",unit_ok,"/",unit_n)
for t in ('3061261001','3061261002','3061261003','3061267501','3060450024','3060450025'): print(t, roll.get(t))
