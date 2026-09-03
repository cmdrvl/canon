import json, sys, collections
def load(p):
    d=json.load(open(p)); return {c['case_id']:c for c in d['cases']}, d['summary']
base,bs=load('eval_base.json')
have_mcp=len(sys.argv)>1
if have_mcp: mcp,ms=load(sys.argv[1])
def tot(s,k): return sum(tp.get(k,0) for tp in s['truth_planes'])
keys=['resolved_cases','ambiguous_cases','conflict_cases','false_merge_cases','solver_truth_exclusion_cases','empirical_falsification_eligible_cases','residual_count_exact_cases','residual_count_saturated_cases']
print("metric | base" + (" | mcp" if have_mcp else ""))
for k in keys: print(k, tot(bs,k), tot(ms,k) if have_mcp else "")
for k in bs['truth_planes'][0].keys():
    if 'falsif' in k or 'truth_model' in k or 'backbone' in k: print(" ", k, tot(bs,k), tot(ms,k) if have_mcp else "")
if have_mcp:
    moved=collections.Counter(); rows=[]
    for cid,b in base.items():
        m=mcp[cid]
        rows.append((cid[-12:], b['status'], m['status'], b.get('residual_model_count'), m.get('residual_model_count'), len(b['hard_forced']['parcels']), len(m['hard_forced']['parcels']), b['false_merge'], m['false_merge'], m['hard_constraint_observations'], m['soft_preference_observations'], m['truth_members_in_universe'], m['truth_members']))
        moved[(b['status'],m['status'])]+=1
    print("status transitions:",dict(moved))
    print("case | b_status m_status | b_models m_models | b_forced m_forced | b_fm m_fm | hard soft | truth_in_U/truth")
    for r in sorted(rows,key=lambda r:(r[2],r[4] if r[4] is not None else 1e30)): print(r)
