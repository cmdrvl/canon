# canon agent-ergonomics audit

Measurement record for bd-c98n: how well `canon` serves an AI agent as its
primary user, scored with the `agent-ergonomics-and-intuitiveness-maximization-for-cli-tools`
skill (rubric 1.0.0). Start at [`audit/HANDOFF.md`](audit/HANDOFF.md).

This was a scoped, single-scorer pass over canon's core runtime surface
(lookup, bare invocation, `--describe`/`--schema`/`--version`, and the doctor
surfaces). It is not a full ten-phase run, and `HANDOFF.md` says which phases
were not run.

| File | What it holds |
|------|---------------|
| `audit/manifest.json` | pass number, target commit, artifact paths |
| `audit/surface_inventory.jsonl` | the 13 surfaces scored, with source lines |
| `audit/agent_surfaces.jsonl` | scores per surface and dimension, pass 1 (before) and pass 2 (after), with evidence |
| `audit/intent_inference_corpus.jsonl` | 17 agent-plausible invocations with before and after outcomes |
| `audit/recommendations.jsonl` | R-001 (applied by bd-c98n) and R-002 to R-007 (deferred, each with a reason) |
| `audit/applied_changes.jsonl` | the change R-001 landed, with commit and regression test |
| `audit/scorecard_pass_1.md`, `audit/uplift_diff.md` | the pass 1 scorecard and the pass 1 to pass 2 diff |
| `audit/HANDOFF.md` | what was done, what was not, what to do next |

Regression tests for the applied change live in `tests/cli_robot_surfaces.rs`
(they run against the real `canon` binary).

To check the workspace: `bash <SKILL>/scripts/validate_pass.sh agent_ergonomics_audit`
and `bash <SKILL>/tools/validate_scorecard.sh agent_ergonomics_audit/audit/agent_surfaces.jsonl`,
where `<SKILL>` is the skill's folder.
