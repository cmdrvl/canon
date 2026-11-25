# Pass 1 → Pass 2 Uplift Diff

Generated: 2026-11-25T07:10:29Z

## Per-surface uplift

| surface_id | prior weighted | new weighted | Δ | improved dims | regressed dims |
|------------|----------------|--------------|---|---------------|-----------------|
| flag__doctor__robot-triage | 827 | 827 | +0 | (none) | (none) |
| flag__global__describe | 827 | 827 | +0 | (none) | (none) |
| flag__global__schema | 816 | 816 | +0 | (none) | (none) |
| flag__global__version | 844 | 844 | +0 | (none) | (none) |
| verb__bare-invocation | 768 | 795 | +27 | agent_ergonomics (+50); agent_ease_of_use (+150); self_documentation (+100) | (none) |
| verb__doctor | 731 | 731 | +0 | (none) | (none) |
| verb__doctor__capabilities | 800 | 811 | +11 | agent_ease_of_use (+50); self_documentation (+50) | (none) |
| verb__doctor__health | 805 | 805 | +0 | (none) | (none) |
| verb__doctor__robot-docs | 788 | 788 | +0 | (none) | (none) |
| verb__lookup | 731 | 731 | +0 | (none) | (none) |

**Median uplift across 10 scored surfaces:** 0 pts
**Mean uplift across 10 scored surfaces:** 3 pts
**Median uplift across 2 applied surfaces:** 19 pts (surface_ids_touched of the 1 applied rec(s) of pass 1)

## Added surfaces (in pass 2, with no row in pass 1 or before)

| surface_id | weighted_score |
|------------|----------------|
| flag__global__robot-triage | 795 |
| verb__capabilities | 750 |
| verb__robot-docs__guide | 831 |
