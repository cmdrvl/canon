# Data center recovery experiment: Step 0 baseline and Step 3 Pryor trace (2026-09-28)

Record for the protocol in [`docs/RESPONSE_CANON_GEO_DATACENTER_SITE_TEST.md`](../../../../docs/RESPONSE_CANON_GEO_DATACENTER_SITE_TEST.md), responding to
[`docs/BRIEF_CANON_GEO_DATACENTER_SITE_TEST.md`](../../../../docs/BRIEF_CANON_GEO_DATACENTER_SITE_TEST.md). Field-test evidence:
[`docs/geo_datacenter_site_test_2026-09-28/`](../../../../docs/geo_datacenter_site_test_2026-09-28/README.md). Related beads: `bd-dzdy1` (cmdrvl-curves),
`bd-2omi` (replay harness; comment only, no status change). **No code was changed, no gate is claimed, and no accuracy is measured.**

## Status against the protocol
| Step | State |
|---|---|
| 0. Pin and reproduce the failure | **Done.** Reproducible with no drift (below) |
| 1. Freeze question, cases, independent evaluation | Not started. Candidate cases are listed in the trace §8; label independence is not checked |
| 2. Freeze the raw neighborhood | Not started. The Pryor trace needs a re-freeze without the size and radius filters |
| 3. Evidence-to-answer trace | **Done for Pryor, but not blind** (analyst had seen results and the official point). A fresh reviewer must repeat it before it justifies a patch |
| 4 to 7 | Not started |

## Files
| File | What it is |
|---|---|
| `step0_baseline_record.json` | Frozen revision, binary versions, environment, input digest, budgets, the response's Appendix A1 output, and replays of `canon geo solve` on the pinned 0.14.0 build and the installed 0.13.0, compared with the historical manifest |
| `step0_baseline.py` | Recorder. Extracts Appendix A1 verbatim from the response and runs it; changes nothing |
| `step3_pryor_trace.md` | The evidence-to-answer trace for Pryor: hypotheses, per-observation trace, first broken link, root-cause decision, missing observations |
| `step3_snapshot_trace.py`, `step3_snapshot_trace.json` | Standard-library helper and its output: per-building evidence table and source ablation of the current objective |

## Step 0 result
- Revision `0d54287d9beb9e10f0212866dcb8c631b3c6e3ca` in a clean detached worktree (0 status lines); `canon 0.14.0` built with `cargo build --locked` (4m20s);
  rustc/cargo 1.98.0-nightly; Python 3.14.7.
- Input: `pryor.evidence_compilation.json` sha256 `ef13086e7e3b92dd...`, 10 building candidates, 0 hard constraints, 17 soft preferences, budgets
  `max_assignments` 65536 and `max_materialized_models` 16384.
- Appendix A1 (independent, no Canon): 1,023 nonempty selections, minimum cost 0, **1 optimal selection: the full universe**.
- Appendix A2: solve exit 0, 406,430 bytes, sha256 `a8584ef52d222118...`, **byte-identical to the historical manifest** on both 0.14.0 (pinned) and 0.13.0
  (installed). Status ambiguous, 1,023 residual models, rank 1 = 10 buildings at cost 0, the only model at that cost. The 0.14.0 release commit did not touch
  `src/geo` or `docs`.
- Gate met: current behavior reproducible; no drift to explain.

## Step 3 result (see the trace for the full argument)
- For every subset of the three retained sources, the optimal selections are exactly the supersets of the union of what the sources assert (none: 1,023
  ties; all three: 1). A source cannot remove a candidate; it only forces its own members in.
- First broken link: **link 2 (translation) then link 3 (formulation)**. Relations and consistency observations became inclusion rewards; an
  inclusion-only objective cannot turn them into a choice among site interpretations. Link 1 (missing candidates or sources) is undetermined from the
  committed snapshot.
- Restorable distinctions: competing-occupant co-location has zero effect today (5 of 10 candidates are closer to a different named business than to the
  Google point, all treated the same); existence agreement between footprint sources is counted as membership support; site alternatives are not variables.
- The retained evidence supports "entrance complex" and "hall campus" equally; the correct output for this snapshot is retained ambiguity with named missing
  observations, not a winner.

## Reproduce
```bash
git worktree add --detach <dir> <revision>          # clean checkout; do not touch another agent's tree
cd <dir> && cargo build --locked --bin canon
python3 scripts/geo_measurements/fixtures/datacenter_recovery_2026-09-28/step0_baseline.py <dir> <dir>/target/debug/canon > step0_baseline_record.json
python3 scripts/geo_measurements/fixtures/datacenter_recovery_2026-09-28/step3_snapshot_trace.py <dir> > step3_snapshot_trace.json
```

## Limits
Analyst contamination on Step 3; the retained snapshot is incomplete (smaller buildings, the full named-place list, source `sources` metadata are not in the
committed files); one site; Pryor cannot score building-level association because its only official label is one facility point.
