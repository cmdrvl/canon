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

## Doctor alignment with spine-rules v2: not done, and why

bd-c98n asks to "align `src/doctor.rs` with `spine-rules` v2". That could not be
carried out, because no doctor specification named v2 was found:

- `spine-rules` (`/Users/zacharyruiz/Source/cmdrvl/spine-rules`, main at
  `00430f4`, crate 0.2.0, tag v0.1.0) contains the golden-rules test suite,
  three ast-grep rules (exit-code range, no `HashMap` in output, witness
  append-only), and from 0.2.0 the `composite_key` and `pipeline` modules. It has
  no doctor, capabilities or triage rule, and no mention of `read_only_doctor`.
- canon's `tests/golden_rules.rs` already runs the spine-rules golden-rules suite
  and passes. `Cargo.lock` pins spine-rules 0.1.0 at `76c87d8`; main is 0.2.0.
  The 0.2.0 additions are key-tuple and pipeline checks, not doctor checks, and
  canon has no consumer for them, so the pin was not bumped.
- The doctor contract that exists is `cmdrvl.read_only_doctor.v1`, which canon
  and `shape`, `rvl`, `verify`, `assess`, `pack` and `lock` all declare. Their
  actual output is not one shape. Top-level keys of `capabilities --json`,
  observed from the installed binaries: `shape` and `rvl` carry
  `contract_version`, `schema_version`, `agent_surfaces`, `data_paths`,
  `detectors`, `env_vars`, `fix_mode`, `online_default`; `pack` and `lock` carry
  `contract`, `schema_version`, `output_contract`, `network`, `fix_mode`;
  `assess` and canon carry `contract`, `schema`, `config_footprint`, `fixers`,
  `read_only`, `side_effects`. canon already matches the `assess` layout.
  `--robot-triage` varies the same way (`summary` and `findings` in
  `shape`/`rvl`; `health` and `recommended_actions` in `pack`/`lock`; `checks`,
  `score` and `recommended_next_steps` in `assess` and canon).

Guessing a target among those three layouts would change the
`canon.doctor.capabilities.v1` payload for existing consumers on no evidence, so
nothing was changed. What is needed is the name of the document or bead that
defines "v2". If it is the shape/rvl layout, the gaps in canon are
`env_vars`, `agent_surfaces` (canon's `top_level_aliases` is the nearest
equivalent), `fix_mode` and `online_default`; `env_vars` and `error_kinds` are
already filed as R-005.

NO_COLOR, `TERM=dumb` and non-tty handling, the other half of that bullet, is
covered by `discovery_output_ignores_no_color_and_dumb_terminals`; canon emits no
color, so the requirement holds by absence, and the test pins it.

## Next

0. Name the document that defines "spine-rules v2" for the doctor (see the section
   above); until then the alignment half of bd-c98n is open.
1. After plan A lands: re-run `cli_robot_surfaces` in the core profile and
   `command_contract`, `contract_parity`, `canon_v1_contract_inventory` with
   `--all-features`.
2. R-006: point `canon --help` at the three discovery commands (`after_help`).
3. R-003: make `canon doctor --fix` and `canon capabilities --fix` say there is
   no repair surface and point at `canon --robot-triage`.
4. R-002 and R-007 belong together and need a decision from whoever owns the
   intent-inference contract: `src/lib.rs` and the Intent inference bullet in
   `docs/PLAN_CANON.md` pin plain edit distance 1 (no adjacent-swap hints); the
   skill counts a swap as one edit.
5. R-004 waits on bd-ko97 (global `--json`).
6. R-005 waits on a spine-rules decision on the capabilities schema.
7. Whoever owns bd-j7u0 should look at the three failures above.

## Process note

The repo's `.git/hooks/pre-commit` runs `br sync --flush-only` and
`git add .beads/issues.jsonl`, so every commit here also carries the flushed
bead state. `ca15ba4` includes `.beads/issues.jsonl` (57 lines added, 57 removed) with other
agents' bead updates for that reason.
