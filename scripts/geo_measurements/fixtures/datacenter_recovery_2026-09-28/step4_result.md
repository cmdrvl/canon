# Step 4 result: opposition on entity-level evidence (2026-09-29)

Pre-registered in `step4_preregistration.md` (commit `222714e`) before any run. Code: branch `geo-prefer-absent-diagnostic` (`bd162fe`), not merged. Replay outputs: `step4_replay_{PRYOR,DEV-1,H1-P}.json`.
Evaluator scoring (numbers only, report sealed, sha256 `a32dd1862c5ebcff...`) used the same label-derived site definition as the reach report; the site boundary is inferred, and the H1-P label discrepancy is unreconciled. Diagnostic; no accuracy claim.

One deviation, before any scoring: the first run's kernel cross-check disagreed on all-opposed subsets because I had not accounted for the kernel's non-empty-selection requirement; only the check formula changed (rules, weights, readout unchanged). After the fix the separable readout agrees with the kernel's rank 1 on all 4 subsets in each of the 3 cases, baseline and patched.

## Results
| | Pryor | DEV-1 | H1-P |
|---|---|---|---|
| Entities in universe (1,300 m, no size filter) | 134 | 2,734 | 287 |
| Support: partition / module / cross-layer | 16 / 0 / 62 | 41 / 0 / 2,500 | 9 / 5 / 225 |
| Opposed entities (competing named place inside a footprint) | 14 | 120 | 22 |
| Baseline core / upper | 72 / 134 | 2,531 / 2,734 | 228 / 287 |
| Patched core / upper | 58 / 120 | 2,414 / 2,615 | 207 / 265 |
| Site entities | n/a | 43 | 5 |
| Site entities in upper, baseline -> patched | n/a | 43 -> 43 | 5 -> 5 |
| Site entities excluded by opposition | n/a | 0 | 0 |
| Entities excluded by opposition that are non-site | n/a | 119 of 119 | 22 of 22 |
| Non-site share of upper, baseline -> patched | n/a | 0.9843 -> 0.9836 | 0.9826 -> 0.9811 |
| Non-site count in upper | n/a | 2,691 -> 2,572 | 282 -> 260 |
| Site built-area share reached (core / upper) | n/a | 0.988 / 1.000 | 1.000 / 1.000 |

The kernel's own solve at these sizes is `ambiguous` with the residual count saturated and no ranked models, in both variants; the separable readout is what is reported.

## Reading against the pre-registered decision rule
- **Outcome 3 (suppresses truth): not triggered.** Opposition removed 0 site entities and 141 non-site entities; every opposed entity that opposition excluded was non-site.
- **Outcome 2 (marginal effect): this is the result.** The non-site share of the answer moved by less than 0.2 percentage points in both labeled cases (still about 98% non-site), and 96% of DEV-1's universe remains in the answer. Pryor stays ambiguous (62 neutral ties remain), as intended.
- **Why the effect is small:** the opposition rule fires on 4 to 8% of the universe, and the support side is nearly non-discriminating: the lineage-collapsed cross-layer rule supports 91% of DEV-1 and 78% of H1-P (any entity with two declared upstreams), and the module rule finds nothing in two cases. Opposition can only subtract from that.
- **What this does and does not show:** the patch works as designed (directional, evidence-attached, never prunes, no truth lost) and the mechanism is not the bottleneck. The bottleneck is that the retained evidence describes the whole neighborhood similarly. By the pre-registered rule I did not add weights, widen opposition, or change the support rules after seeing this.

## Open
- Full `cargo test` did not complete (disk full); verified on the branch: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `geo_composition` (44), `geo_evidence_compilation` (36). Other test binaries (`geo_schemas`, `geo_materialize`, `geo_run` and the rest) are unverified.
- Two labeled cases, one rule set, one weight.
- I know one H1-P label fact from an earlier evaluator reply.
