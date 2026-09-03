import json, os
names=[('eval_base.json','PAD'),('eval_mcp.json','+size+geo'),('eval_owner.json','PAD+owner(hard)'),('eval_all.json','all, owner hard'),('eval_soft.json','all, owner soft+floor'),('eval_roll.json','roll universe+owner'),('eval_roll2.json','roll, exact owner+gsf band')]
subs={}
for s in json.load(open('subjects.json')): subs.setdefault(s['subject_id'],s)
bridge={r['sid']:r for r in json.load(open('condo_bridge_pad.json'))['rows']}
def truth_for(cid,roll_mode):
    if roll_mode: return set(subs[cid]['truth_parcels'])
    return set(bridge[cid]['bridged_truth']) if cid in bridge else set(subs[cid]['truth_parcels'])
print("%-24s %8s %8s %9s %8s %8s %10s %10s"%("stack","resolved","correct","ambiguous","conflict","budget","truth_excl","<=16 sets"))
for fn,label in names:
    if not os.path.exists(fn) or os.path.getsize(fn)==0: print("%-24s pending"%label); continue
    d=json.load(open(fn))
    if d.get('outcome')=='REFUSAL': print("%-24s REFUSAL %s"%(label,d['refusal']['message'][:60])); continue
    cs=d['cases']; roll_mode='roll' in fn
    res=[c for c in cs if c['status']=='resolved']
    correct=sum(1 for c in res if set(c['hard_forced']['parcels'])==truth_for(c['case_id'],roll_mode))
    small=sum(1 for c in cs if c.get('residual_model_count') is not None and c['residual_model_count']<=16)
    print("%-24s %8d %8d %9d %8d %8d %10d %10d"%(label,len(res),correct,sum(c['status']=='ambiguous' for c in cs),sum(c['status']=='conflict' for c in cs),sum(c['status']=='component_budget_fallback' for c in cs),sum(1 for c in cs if c.get('truth_model_in_residual') is False),small))
