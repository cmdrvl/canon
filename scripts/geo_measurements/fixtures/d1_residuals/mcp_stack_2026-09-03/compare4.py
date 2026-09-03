import json, collections
names=['eval_base.json','eval_mcp.json','eval_owner.json','eval_all.json']; labels=['PAD','PAD+size+geo','PAD+owner','all']
runs=[]
for n in names:
    try: d=json.load(open(n)); runs.append({c['case_id']:c for c in d['cases']})
    except Exception as e: runs.append(None)
def tot(run,pred): return sum(1 for c in run.values() if pred(c))
print("metric | "+" | ".join(labels))
for k,pred in [('resolved',lambda c:c['status']=='resolved'),('ambiguous',lambda c:c['status']=='ambiguous'),('conflict',lambda c:c['status']=='conflict'),('budget_fallback',lambda c:c['status']=='component_budget_fallback'),('false_merge',lambda c:c['false_merge']),('truth_excluded',lambda c:c.get('truth_model_in_residual') is False),('forced_parcels_true',lambda c:c['backbone_true_positive_members']),('forced_parcels_false',lambda c:c['backbone_false_positive_members'])]:
    print(k, " | ".join(str(sum(pred(c) for c in r.values()) if isinstance(pred(next(iter(r.values()))),bool) else sum(pred(c) for c in r.values())) if r else '-' for r in runs))
base=runs[0]
for i,r in enumerate(runs[1:],1):
    if not r: continue
    small=sum(1 for cid,c in r.items() if c.get('residual_model_count') is not None and c['residual_model_count']<=16)
    print(labels[i],"cases with <=16 feasible sets:",small)
