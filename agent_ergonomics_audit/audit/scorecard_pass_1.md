# Agent Ergonomics Scorecard

Generated: 2026-11-25T05:51:33Z
Source: `audit/agent_surfaces.jsonl` (pass 1)


## Per-surface scores

| surface_id | weighted | intu | ergo | ease | parse | error | intent | safe | det | self | comp | regr |
|------------|----------|------|------|------|-------|-------|--------|------|-----|------|------|------|
| verb__lookup | 731 | 500 | 700 | 500 | 800 | 800 | 700 | 1000 | 900 | 600 | 800 | 750 |
| verb__bare-invocation | 768 | 750 | 750 | 700 | 600 | 800 | 750 | 1000 | 900 | 700 | 900 | 600 |
| flag__global__describe | 827 | 750 | 700 | 700 | 850 | - | - | 1000 | 900 | 800 | 900 | 850 |
| flag__global__schema | 816 | 750 | 700 | 700 | 850 | - | - | 1000 | 900 | 800 | 900 | 750 |
| flag__global__version | 844 | 1000 | 1000 | 750 | 600 | - | - | 1000 | 1000 | 500 | 1000 | 750 |
| verb__doctor | 731 | 750 | 750 | 750 | 800 | 400 | 250 | 1000 | 900 | 750 | 900 | 800 |
| verb__doctor__health | 805 | 800 | 700 | 700 | 850 | - | - | 1000 | 750 | 750 | 900 | 800 |
| verb__doctor__capabilities | 800 | 750 | 750 | 650 | 850 | - | - | 1000 | 850 | 650 | 900 | 800 |
| verb__doctor__robot-docs | 788 | 750 | 700 | 700 | 500 | - | - | 1000 | 1000 | 800 | 900 | 750 |
| flag__doctor__robot-triage | 827 | 750 | 800 | 700 | 850 | - | - | 1000 | 850 | 750 | 900 | 850 |

## Distribution histogram

### Weighted score distribution (per surface)

```
   0- 99 │  (0)
 100-199 │  (0)
 200-299 │  (0)
 300-399 │  (0)
 400-499 │  (0)
 500-599 │  (0)
 600-699 │  (0)
 700-799 │ ████ (4)
 800-899 │ ██████ (6)
 900-999 │  (0)
1000     │  (0)
```

## Below-Polish-Bar surfaces (weighted < 750)

- verb__lookup (weighted: 731)
- verb__doctor (weighted: 731)
