# Handoff: canon agent-ergonomics audit, pass 1 (bd-c98n)

Written 2026-11-25. Target: `canon` at 511f2d4. Branch: `main`. The change landed
as `ca15ba4`.

## What this was

A scoped, single-scorer pass over canon's core runtime surface: lookup, bare
invocation, `--describe` / `--schema` / `--version`, and the doctor surfaces
(13 surfaces). The scores are one scorer's hand-assigned judgments against
rubric 1.0.0, each backed by an observed invocation or a source line. Nobody
else scored them and no scores were reconciled.

## What changed (R-001, the only applied recommendation)

The spine siblings answer `<tool> --robot-triage`, `<tool> capabilities --json`
and `<tool> robot-docs guide`. canon answered all three with a clap usage error,
because the same reports lived only under `canon doctor`. They now work at the
top level and call the same functions as the doctor forms, so the bytes cannot
drift. They are listed in the orientation text, root `--help`, `operator.json`,
the capabilities payload, the new guide, the README and PLAN CLI blocks, and the
v1 contract inventory. The guide decides whether to advertise Geo from the
compiled command tree, so it is right with or without a `geo` feature.

Observed before and after, 17 agent-plausible invocations
(`audit/intent_inference_corpus.jsonl`):

| classification | before | after |
|---|---|---|
| useless error | 13 | 5 |
| useful hint | 3 | 7 |
| inferred and acted on | 1 | 5 |

Scores (`audit/uplift_diff.md`): of the 10 surfaces that existed before, the
median uplift is 0 points and the mean is 3. Two surfaces were touched: bare
invocation +27, `doctor capabilities` +11 (median 19 across those two). Three
surfaces did not exist before and now score 750 to 831. Eight surfaces were not
touched; their pass 2 rows are carried unchanged from pass 1, and their behavior
was re-checked against the post-change binary (same exit codes and bytes, except
the doctor health and triage manifest digest and check counts, which move with
`operator.json`).

## Not done, said plainly

- **The Ambition Bar is not met.** One recommendation applied against a bar of
  at least 5 to 10 changes; 3 dimensions touched; median uplift far below 50.
  The self-prompt round was not run (`ambition_bar_self_prompt_run: false`), and
  the pass is not marked complete (`completed_at: null`).
- Phases not run: archetype classifier, canonical-task simulations, triangulation,
  fresh-eyes rounds, heatmap, playbook, second scorer, CASS mining.
- `canon capabilities --json` is not the skill's Universal Envelope and has no
  `error_kinds` or `env_vars` (R-005, deferred pending a spine-rules decision).
- NO_COLOR, `TERM=dumb` and non-tty handling: canon emits no color at all, so
  there is nothing to honor; the new tests assert byte-identical, ANSI-free
  output under `NO_COLOR`, `TERM=dumb` and `CLICOLOR_FORCE`. That is a test of
  the current behavior, not a feature added.

## Verification

`ca15ba4` was committed from HEAD 511f2d4 plus only these hunks, because the
`geo` feature gate (bd-1pjc plan A) was still uncommitted in the shared tree and
the hunks were three-way merged onto pristine HEAD with no conflicts. The tested
tree is that HEAD-plus-hunks tree, in its own target directory:

- `cargo fmt --check` clean. `cargo clippy --locked --all-targets -- -D warnings`
  exit 0.
- Lib tests `doctor::` and `cli::tests`: 57 pass.
- 16 integration suites, all pass except as listed below: `cli_robot_surfaces` 9,
  `cli_smoke` 82, `command_contract` 5, `contract_parity` 9,
  `canon_v1_contract_inventory` 6, `intent_inference` 4,
  `entity_namespace_migration` 4, `safety_matrix` 22, `golden_rules` 10,
  `inbox_cli` 4, `provider_schema` 7, `geo_control_contracts` 24,
  `extensions_review_policy` 16, `geo_cli` 48, `geo_e4_metric_restatement` 4.
- **Three failures, all reproduced on a pristine `git archive HEAD` with none of
  this change, so none comes from it:** `domain_neutrality` x2 (the `servicer_of`
  term in `src/session/mod.rs` and `src/entity/session/mod.rs`, from bd-j7u0) and
  `entity::canon_v1_operator_journey::..._exact_replay` ("journey must exercise
  every advertised entity leaf").
- `ubs`: `src/doctor.rs` and `tests/cli_robot_surfaces.rs` 0 critical.
  Run on all four changed Rust files together it reports 74 critical in existing
  code (string `==` comparisons, a temp-file nonce, `panic!` in the
  `cli.rs` test module); the sampled lines are far from the changed lines and the
  diff adds no `panic!`, `unreachable!` or `todo!`.
- Not independently verified: the full `cargo test` suite, and the combined tree
  of this change plus the uncommitted plan A beyond the one suite named below.

`bdb43ab` changes how `tests/cli_robot_surfaces.rs` decides whether geo is
compiled. `ca15ba4` probed `canon geo --help`, which exits 0 in a core build
(clap reads `geo` as the input path), so the probe could not tell the profiles
apart. It now reads the binary's own `--describe` contract, which lists geo rows
exactly when geo is compiled (26 rows with geo, 0 in core, counted on built
binaries). `cargo test --locked --test cli_robot_surfaces`: 9 passed on HEAD plus
this change (geo present) and 9 passed on the plan A tree in the core profile
(geo absent), so both outcomes of the check were exercised. The plan A tree with
`--features geo` was still compiling when this was written; its result is in the
bd-c98n comment, not here.

## Doctor alignment with spine-rules (operator.v1 / cmdrvl.doctor.v1): done in 0731c13

An earlier version of this section said the alignment could not be carried out
because no doctor specification named v2 was found. That was wrong for the
spine-rules revision that matters: it looked at spine-rules `00430f4` (crate
0.2.0), which has no doctor rule. The specification is in spine-rules `906feed`
(crate 0.3.1): its `golden_rules_suite!` validates `operator.json` as
`operator.v1` and the doctor output as `cmdrvl.doctor.v1`. airlock `b0180f8` is
the reference alignment, and canon followed it in `0731c13` (bd-c98n stage B).

What changed in canon:

- `operator.json` is `operator.v1`: `tool_version` and `contract_version`
  (1.0.0) replace `version`, `commands` replaces `subcommands`, and `env_vars`,
  `formats` and `side_effects` are declared. Two rows are new: `doctor triage` and
  the aggregate `robot-docs`.
- `canon doctor [--json]`, `canon doctor capabilities --json` and
  `canon doctor triage --json` emit `cmdrvl.doctor.v1` (`schema_version`, `mode`,
  `tool`, `tool_version`, `healthy`, `findings[]`). `doctor capabilities` carries
  the operator.v1 document as `contract`, equal to `canon capabilities --json`
  and `canon --describe`. Exit 0 is healthy, 1 is unhealthy, 2 is a usage error.
- `canon doctor health`, `canon doctor --robot-triage` and `canon --robot-triage`
  still work as deprecated aliases. canon's own fields (`ok`, `score`,
  `read_only`, `checks`, `summary`, `contract_parity`) are kept as additions. The
  old string `contract` is now `read_only_contract`
  (`cmdrvl.read_only_doctor.v1`), because `contract` is reserved for the
  operator.v1 document in capabilities mode.
- The ids `operator.v0` and `canon.doctor.{health,capabilities,triage}.v1` are
  retired from `src/`, `operator.json` and the contract inventory.
- `Cargo.lock` pins spine-rules at 0.3.1 / `906feed`, in the same commit, so
  `tests/golden_rules.rs` (20 tests) stays green.

Evidence is in the bd-c98n comment (commands, per-suite counts, both profiles).
The fields the old text listed as gaps against the shape/rvl layout
(`agent_surfaces`, `fix_mode`, `online_default`) are not among operator.v1's
required fields, so canon does not carry them. `env_vars` is now declared;
`error_kinds` is not, and is still R-005.

The `.jsonl` files in this folder (surface inventory, recommendations, corpus)
are snapshots of the surface as audited and still show the retired ids. They
were not rewritten.

NO_COLOR, `TERM=dumb` and non-tty handling, the other half of that bullet, is
covered by `discovery_output_ignores_no_color_and_dumb_terminals`; canon emits no
color, so the requirement holds by absence, and the test pins it.

## Next

0. Done in `0731c13`: the doctor alignment (see the section above).
1. Done: plan A landed as `acb81f9`, and `cli_robot_surfaces` was re-run in the
   core profile with `command_contract`, `contract_parity` and
   `canon_v1_contract_inventory` under `--all-features` at the `0731c13` tree
   (details in the bd-c98n comment).
2. R-006: point `canon --help` at the three discovery commands (`after_help`).
3. R-003: make `canon doctor --fix` and `canon capabilities --fix` say there is
   no repair surface and point at `canon doctor triage --json` (`canon
   --robot-triage` is now a deprecated alias of it).
4. R-002 and R-007 belong together and need a decision from whoever owns the
   intent-inference contract: `src/lib.rs` and the Intent inference bullet in
   `docs/PLAN_CANON.md` pin plain edit distance 1 (no adjacent-swap hints); the
   skill counts a swap as one edit.
5. R-004 waits on bd-ko97 (global `--json`).
6. R-005: the capabilities schema is decided (operator.v1) and `env_vars` is now
   declared; `error_kinds` is still absent.
7. Whoever owns bd-j7u0 should look at the three failures above.

## Process note

The repo's `.git/hooks/pre-commit` runs `br sync --flush-only` and
`git add .beads/issues.jsonl`, so every commit here also carries the flushed
bead state. `ca15ba4` includes `.beads/issues.jsonl` (57 lines added, 57 removed) with other
agents' bead updates for that reason.
