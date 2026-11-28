# canon command reference

Every command and flag in the released binary, with exit codes, refusal codes and
the JSON output shape. The [README](../README.md) describes the core; this file
is the complete list. A test (`tests/contract_parity.rs`) fails if a compiled
command or flag is missing from the first block below.

## CLI Reference

```bash
canon <INPUT> --registry <REGISTRY> --column <COLUMN> [--scope DIMENSION=VALUE]... [--emit json|csv | --json] [--canon-column <NAME>] [--map-out <PATH>] [--inbox-out <FILE> --inbox-project <P> --inbox-run <R> --inbox-seen-at <RFC3339> [--inbox-raw-ref]] [--max-rows <N>] [--max-bytes <N>]
canon doctor [--json]
canon doctor capabilities [--json]
canon doctor triage [--json]
canon doctor robot-docs
canon capabilities [--json]
canon robot-docs guide
canon doctor health [--json]          # deprecated alias of `canon doctor [--json]`
canon doctor --robot-triage           # deprecated alias of `canon doctor triage --json`
canon --robot-triage                  # deprecated alias of `canon doctor triage --json`
canon temporal compile --request <REQUEST.json> [--emit json|summary] [--max-bytes <N>] [--no-witness]
canon temporal relation finalize --facts <FACTS.json> [--emit json|summary] [--max-bytes <N>] [--no-witness]
canon package pack --root <DIR> --package <package.json> --out <ARCHIVE>
canon package inspect <ARCHIVE> [--emit json|summary]
canon package verify <ARCHIVE> [--emit json|summary]
canon package unpack <ARCHIVE> --target <EMPTY_DIR> [--emit json|summary]
canon package push --archive <ARCHIVE> --registry <OCI_BASE_URL> --repository <REPOSITORY> [--tag <TAG>] [--emit json|summary]
canon package pull --registry <OCI_BASE_URL> --repository <REPOSITORY> --cache <DIR> (--digest <sha256:...>|--tag <TAG>) [--emit json|summary]
canon project init <DIR> [--project-id <ID>] [--mapping-profile <REF>] [--emit json|summary]
canon project validate <DIR> [--manifest <PATH>] [--emit json|summary]
canon project describe <DIR> [--manifest <PATH>] [--emit json|summary]
canon project lock refresh --manifest <MANIFEST> --out <LOCK> [--emit json|summary]
canon project plan --manifest <MANIFEST> --lock <LOCK> [--out <PLAN>] [--cache-hit <NODE>...] [--emit json|summary]
canon project run [--plan <PLAN>] [--manifest <MANIFEST>] [--lock <LOCK>] [--node <NODE>...] [--workspace <DIR>] [--work-dir <DIR>] [--max-parallelism <N>] [--allow-network] [--allow-mutation-gates] [--emit json|summary]
# Geo — primary surface (the commands used in the course of business)
canon geo capabilities [--guide] [--emit json]
canon geo plan --question <QUESTION.json> --capabilities <CAPABILITIES.json> --inventory <INVENTORY.json> --profile <PROFILE.json> --budget <BUDGET.json>
canon geo run --plan <PLAN.json> --work-dir <DIR> [--input <NODE_ID:BINDING_ID=PATH>...] [--satisfy <REQUEST_ID=RECEIPT.json>...]
canon geo replan-from-acquisition --base-plan <PLAN.json> --base-inventory <INVENTORY.json> --question <QUESTION.json> --capabilities <CAPABILITIES.json> --profile <PROFILE.json> --budget <BUDGET.json> --satisfy <REQUEST_ID=RECEIPT.json> --local-artifact <LOCAL_ARTIFACT_ID=PATH>... [--result <DIGEST_ID=PATH>...] --advancement-out <ADVANCEMENT.json>
canon geo evaluate --population <POPULATION.json> [--truth <DEED_TRUTH.json> --truth-plane deed_grain_instrument] [--artifact-dir <DIR>] [--e4-assessment-out <ASSESSMENT.json>] [--e4-before-assessment <BEFORE.json> --e4-rescore-out <COMPARISON.json>]
canon geo inspect --run <DIR> [--component <ID>] [--compare <OTHER_RUN>] [--recommend-next] [--emit json|summary]
canon geo ledger

# Geo — ledger subcommands (implemented under `geo ledger`, not extra primary verbs)
canon geo ledger build --seed <SEED.json> --composition <ARTIFACT_ID=COMPOSITION.json> --evidence <ARTIFACT_ID=EVIDENCE.json>
canon geo ledger card --subject-id <SUBJECT_ID> --context <CONTEXT.json> --ortho-pin <PIN.json> [--composition <COMPOSITION.json> --evidence <EVIDENCE.json> --geometry <GEOMETRY.json>] [--explanation <EXPLANATION.json>] [--explicit]
canon geo ledger collision --ledgers <LEDGER.json> <LEDGER.json> [--pari-passu <DECLARATIONS.json>] [--adjacency <PARCEL_TO_BLOCK.json>]
canon geo ledger exposure --ledger <LEDGER.json> --advisory <ADVISORY.json> --geometry <GEOMETRY.json> --archive <ARCHIVE.json>
canon geo ledger validate --ledger <LEDGER.json>

# Geo — stage leaves (driven by `geo run` and Demo 0; independently callable, hidden from top-level help)
canon geo link-sources --request <REQUEST.json> --rows-out <ROWS.csv>
canon geo materialize-home-cells --rows <ROWS.json>
canon geo tile-work --request <REQUEST.json>
canon geo reconcile-tiles --request <REQUEST.json>
canon geo materialize-geometry --request <REQUEST.json>
canon geo materialize-warehouse-geometry --rows <ROWS.json>
canon geo materialize-evidence --rows <ROWS.json>
canon geo materialize-address-evidence --request <REQUEST.json>
canon geo compile-evidence --request <REQUEST.json>
canon geo stack-evidence --population <POPULATION.json> --overlay <OVERLAY.json>
canon geo solve --request <REQUEST.json>

# Geo — measurement adapters (bounded profile adapters; moving under scripts/geo_measurements/)
canon geo materialize-h7-population --rows <ROWS.json>
canon geo materialize-h7-staging-batch --batch <BATCH.json>
canon geo materialize-h7-pip-block-batch --batch <BATCH.json>

canon inbox list --inbox <INBOX.json> [--policy <POLICY.json>] [--limit <N>] [--cursor <CURSOR>] [--event-kind <KIND>...] [--reason-code <REASON>...] [--field-role <ROLE>...] [--partition <KEY>...] [--emit json|summary]
canon inbox show --inbox <INBOX.json> --event-key <KEY> [--policy <POLICY.json>] [--emit json|summary]
canon inbox explain --inbox <INBOX.json> --event-key <KEY> [--policy <POLICY.json>] [--emit json|summary]
canon inbox stats --inbox <INBOX.json> [--policy <POLICY.json>] [--emit json|summary]
canon inbox export-review --inbox <INBOX.json> [--out <REVIEW.json>] [--policy <POLICY.json>] [--limit <N>] [--cursor <CURSOR>] [--event-kind <KIND>...] [--reason-code <REASON>...] [--field-role <ROLE>...] [--partition <KEY>...] [--emit json|summary]
canon inbox apply-review --inbox <INBOX.json> --review <REVIEW.json> --expected-inbox-hash <HASH> --out <GROUPS.json> [--emit json|summary]
canon inbox plan-entity --inbox <INBOX.json> --expected-inbox-hash <HASH> --out <REQUEST.json> [--policy <POLICY.json>] [--event-key <KEY>...] [--limit <N>] [--mode cluster|link] [--emit json|summary]
canon registry build --source <SOURCE> --seed <SEED> --seed-column <COLUMN> --output <DIR> --version <VER> [--incremental] [--max-rows <N>] [--max-bytes <N>] [--batch-size <N>] [--rate-limit-ms <MS>] [--provider-config <KEY=VALUE>] [--json]
canon registry init --registry <DIR> --registry-id <ID> --prefix <PREFIX> [--zero-pad <N>] [--description <TEXT>] [--emit json|summary | --json]
canon registry export --format dbt-seed|search-index|key-crosswalk|entity-card|link-evidence (--registry <REGISTRY>|--registries <MAP>) [--out <PATH>] [--from <A> --to <B>] [--namespace <CONTEXT>] [--source-file <FILE>...] [--canonical-type <TYPE>...] [--rule-id-prefix <PREFIX>...] [--canonical-iri-prefix <PREFIX>] [--schema-out <schema.yml>] [--anti-collapse-test-out <test.sql>] [--id <ID>...] [--max-bytes <N>] [--parquet] [--emit json|summary | --json]
canon registry providers [--emit json|summary | --json]
canon registry provider-schema <PROVIDER> [--emit json|summary | --json]
canon registry next-id [PREFIX] --registry <DIR> [--zero-pad <N>] [--emit plain|json | --json]
canon registry check-id <ID> [--registry <DIR> | --registries <MAP_YAML>] [--emit json|summary | --json]
canon registry identity --registry <DIR> [--ref <REF>] [--emit json|summary | --json]
canon registry add-entry --registry <DIR> --alias-file <FILE> --canonical-id <ID> --input <INPUT> --rule-id <RULE> [--canonical-type <TYPE>] [--scope DIMENSION=VALUE]... [--bump patch|minor|major | --next-version <VER>] [--no-lint] [--emit json|plain | --json]
canon registry add-entries --registry <DIR> --from <FILE.jsonl> [--bump patch|minor|major | --next-version <VER>] [--no-lint] [--emit json|plain | --json]
canon registry mint --registry <DIR> [--canonical-id <ID> | --prefix <PREFIX>] --canonical-type <TYPE> --with-alias <FILE=INPUT:RULE_ID>... [--scope DIMENSION=VALUE]... [--bump patch|minor|major | --next-version <VER>] [--create-alias-file] [--no-lint] [--emit json|plain | --json]
canon registry mint-batch --registry <DIR> [--prefix <PREFIX>] --canonical-type <TYPE> --from <FILE.jsonl> [--bump patch|minor|major | --next-version <VER>] [--no-lint] [--emit json|plain | --json]
canon registry default-id-scheme --registry <DIR> --prefix <PREFIX> [--zero-pad <N>] [--strict] [--bump patch|minor|major | --next-version <VER>] [--emit json|plain | --json]
canon registry scope-policy --registry <DIR> (--optional-dimension <DIM>... | --remove-dimension <DIM>...) [--bump patch|minor|major | --next-version <VER>] [--emit json|plain | --json]
canon registry diff --old <OLD_REGISTRY> --new <NEW_REGISTRY> [--emit json|summary | --json]
canon registry audit <SEED> --registry <REGISTRY> --column <COLUMN> [--emit json|summary | --json] [--max-rows <N>] [--max-bytes <N>]
canon registry correct --registry <DIR> --from <CORRECTIONS.jsonl> (--next-version <VER>|--bump patch|minor|major) [--rules <RULES.yaml>] [--expect-base <HASH>] [--dry-run] [--emit json|summary | --json]
canon registry lint <REGISTRY> [--profile standard|org|strategy|auto] [--emit json|summary | --json]
canon strategy profile <INPUT> [--emit json|summary] [--max-rows <N>] [--max-bytes <N>]
canon strategy audit --schema <PROFILE.json> --script <SCRIPT> --suite <DIR> [--emit json|summary]
canon strategy resolve --registry <DIR> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> [--emit json|summary]
canon strategy register --registry <DIR> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> --script <SCRIPT> --script-id <ID> --language <LANG> --grade operator-attested|proof-attested --next-version <VER> [--operator <ID> --reason <TEXT> --attested-at <RFC3339>] [--verify <VERIFY.json> --assess <ASSESS.json> --airlock <AIRLOCK.json>] [--rule-id <RULE>] [--emit json|summary] [--no-witness]
canon strategy update --registry <DIR> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> --script <SCRIPT> --script-id <ID> --language <LANG> --next-version <VER> [--operator <ID> --reason <TEXT> --attested-at <RFC3339>] [--verify <VERIFY.json> --assess <ASSESS.json> --airlock <AIRLOCK.json>] [--emit json|summary] [--no-witness]
canon strategy deprecate --registry <DIR> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> --operator <ID> --reason <TEXT> --next-version <VER> [--attested-at <RFC3339>] [--emit json|summary] [--no-witness]
canon strategy promote --registry <DIR> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> --verify <VERIFY.json> --assess <ASSESS.json> --airlock <AIRLOCK.json> --next-version <VER> [--emit json|summary] [--no-witness]
canon strategy list --registry <DIR> [--key-type schema|task] [--grade operator-attested|proof-attested] [--status active|deprecated] [--emit json|summary]
canon strategy explain --registry <DIR> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> [--emit json|summary]
canon strategy diff --old <OLD_DIR> --new <NEW_DIR> [--emit json|summary]
canon entity run <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--cache-mode enabled|disabled] [--suite <DIR>] [--emit json|summary] [--no-witness]
canon entity session init --goal <GOAL> (--registry <REGISTRY_DIR> | --registries <MAP_YAML>) --work-dir <DIR> [--entity-kind <KIND>] [--registry-id <ID>] [--prefix <PREFIX>] [--zero-pad <N>] [--emit json|summary]
canon entity session ingest <SESSION> <OBSERVATIONS.jsonl|-> [--expect-digest blake3:<HEX>] [--artifact <ARTIFACT.json> --receipt <RECEIPT.json>] [--rows-field <FIELD> --column-map <MAP.yaml>] [--emit json|summary]
canon entity session assert <SESSION> <EVIDENCE.jsonl|-> [--expect-digest blake3:<HEX>] [--artifact <ARTIFACT.json> --receipt <RECEIPT.json>] [--rows-field <FIELD> --column-map <MAP.yaml>] [--emit json|summary]
canon entity session solve <SESSION> --rules <RULES.yaml> [--what-if <EXTRA.jsonl>] [--emit json|summary]
canon entity session rules init [--out <RULES.yaml>] [--emit json|summary]
canon entity session rules validate <RULES.yaml> [--emit json|summary]
canon entity session commit <SESSION> (--next-version <VER>|--bump patch|minor|major) [--expect-base <HASH>] [--emit json|summary]
canon entity session status <SESSION> [--rules <RULES.yaml>] [--emit json|summary]
canon entity session explain <SESSION> <OBS_ID|SURFACE|cluster:CLUSTER_ID> [--emit json|summary]
canon entity session frontier <SESSION> [--seed-entity <SURFACE_OR_ID>] [--shard <INDEX/TOTAL>] [--limit <N>] [--emit json|summary]
canon entity session frontier claim <SESSION> --worker <ID> --limit <N> --ttl-secs <SECONDS> [--emit json|summary]
canon entity session frontier release <SESSION> --claim-id <CLAIM_ID> [--emit json|summary]
canon entity session next <SESSION> [--seed-entity <SURFACE_OR_ID>]
canon entity session export <SESSION> --format evidence-closure --out <DIR> [--outcome <OUTCOME_REF>] [--emit json|summary]
canon entity session derive <SESSION> --profile <PROFILE.yaml> --strategy <STRATEGY.yaml> [--no-registry-candidates] [--emit json|summary]
canon entity session sample <SESSION> [--stratum tier_a|tier_b|tier_c|standalone|neighbourhood] [--count <N>] [--seed <SEED>] [--entity-type <TYPE>] [--blind] [--seed-entity <SURFACE_OR_ID>] [--score <LABELS.jsonl>] [--emit jsonl|json|summary]
canon entity link <REFERENCE> <TARGET> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--suite <DIR>] [--gold <JSONL>] [--write-back] [--emit json|summary] [--cache-mode enabled|disabled] [--max-candidates <N>] [--max-rows <N>] [--max-bytes <N>] [--no-witness]
canon entity calibrate sweep <RESULT|EVIDENCE> --gold <GOLD.jsonl> --strategy <STRATEGY.yaml> [--emit json|summary]
canon entity calibrate em <EVIDENCE.jsonl> --strategy <STRATEGY.yaml> [--emit json|summary]
canon entity prepare <ROWS> --profile <PROFILE> --registry <DIR> --work-dir <DIR>
canon entity index build <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--emit json|summary]
canon entity block <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--emit jsonl|summary]
canon entity block preflight <ROWS> --profile <PROFILE> --strategy <YAML> [--sample-pct <N>] [--work-dir <DIR>] [--emit json|summary]
canon entity candidate-recall --manifest <MANIFEST.json> --candidates <CANDIDATES.jsonl> --diagnostics <DIAGNOSTICS.json> --exact-bucket-count <N> [--emit json|summary]
canon entity evidence <ROWS> [--profile <PROFILE>] --strategy <YAML> --candidates <JSONL> --registry <DIR> [--work-dir <DIR>] [--emit jsonl|summary]
canon entity solve <ROWS> [--profile <PROFILE>] --strategy <YAML> --evidence <JSONL> --registry <DIR> [--work-dir <DIR>] [--emit json|summary]
canon entity audit <RESULT.json> --suite <DIR> [--emit json|summary]
canon entity promote <RESULT.json> --audit <AUDIT.json> --registry <DIR> --next-version <VER> [--emit json|summary]
canon entity apply <RESULT.json> --rows <ROWS> --registry <DIR> [--column <COL>] [--output <PATH>] [--work-dir <DIR>] [--require-full-resolution|--allow-partial-output] [--emit json|summary]
canon entity review export <RESULT.json> [--artifact queue|native-review] [--group-by signature] [--emit json|csv|html] [--include resolved|escrow|contradictions|all]
canon entity review import <REVIEW.json|csv> (--registry <DIR> --next-version <VER> | --session <SESSION_DIR> --reviewer <REVIEWER>) [--audit <AUDIT.json>] [--source-review <NATIVE_REVIEW.json>] [--emit json|summary]
canon entity explain <RESULT.json> --row <ROW_ID>|--surface-id <SURFACE_ID>|--canon-id <CANON_ID>|--escrow-id <ESCROW_ID> [--emit json|summary]
canon entity profile list [--emit json|summary]
canon entity profile init <PROFILE> --output <PATH>
```

Entity profiles may declare their candidate retrieval in `blocking.operators`:
exact views, anchors, composite keys, and similarity restricted by shared
attributes. Retrieval does not authorize a merge. Strategies control budgets;
generation refuses at the first actual crossing. The strategy owns candidate
budgets under `block:` (for example, `block.max_candidates_per_run`); the
profile's `blocking:` section owns retrieval operators, not budgets. Implicit
per-operator and per-run defaults scale with the prepared surface count, while
explicit strategy values remain authoritative and bounded by the absolute cap.
Budget refusals name the exact strategy key, file role, and a pasteable YAML
fragment. Sampled run preflight is advisory.
See the [profile blocking reference](PLAN_ENTITY_WORKBENCH.md#strategy-extensions)
for parameters, equality authority, and artifact-version changes.

#### Geo command surface

For a short terminal walkthrough, run:

```bash
bash scripts/geo_demo/cornerstone.sh
```

Press Enter through three evidence steps: Foursquare parcel support, Overture
corroboration, and the remaining building ambiguity. The default reads
hash-verified retained results from the
[Cornerstone experiment](../scripts/geo_measurements/fixtures/cornerstone_national_2026-09-17/README.md)
immediately; it does not fetch live MCP data or execute the solver. Add `--auto`
to print all steps without prompts, or `--recompute` to run native evidence
materialization, compilation, and solving over the retained adapter rows.
Use `--step 1`, `--step 2`, or `--step 3` to visit just one step.
Recomputation can take several minutes per parcel solve; it does not rerun
geometry adaptation or the full project DAG. `--work-dir DIR` retains the inputs
and summaries in a new or empty directory. Default readback requires Bash, jq,
tar, and `sha256sum` or `shasum`; recomputation builds the checkout with Cargo
unless `CANON_BIN` names an executable. The walkthrough preserves the distinction
between a preferred candidate and accepted identity. Live acquisition still
requires an externally connected agent and the experimental adapter.

Geo commands are declared in three tiers, and `canon --describe` carries the tier on
every row:

| Tier | Commands | Who runs it |
|------|----------|-------------|
| **primary** | `geo capabilities`, `geo plan`, `geo run`, `geo replan-from-acquisition`, `geo evaluate`, `geo inspect`, and `geo ledger` | Agents and operators in the course of business. Listed first in `--help`, here, and in `canon --describe`. |
| **leaf** | `link-sources`, `materialize-home-cells`, `tile-work`, `reconcile-tiles`, `materialize-geometry`, `materialize-warehouse-geometry`, `materialize-evidence`, `materialize-address-evidence`, `compile-evidence`, `stack-evidence`, `solve` | The stages `geo run` executes. Kept independently callable for Demo 0, single-stage debugging, and tests; hidden from top-level `--help` and still machine-described. |
| **measurement** | `materialize-h7-population`, `materialize-h7-staging-batch`, `materialize-h7-pip-block-batch` | Bounded profile adapters for the measurement harness. They move under `scripts/geo_measurements/` and the `canon_geo_measurements` binary; they are not a regional engine. |

`geo inspect` and the `geo ledger` family are on the primary surface in
[`docs/PLAN_CANON_GEO.md`](PLAN_CANON_GEO.md) §19.3. `geo ledger build`
and `geo ledger validate` ship today as local artifact operations under `geo ledger`;
`geo ledger exposure` joins a pinned wind-radii advisory to exact ledger building
geometry as a read-only downstream artifact, and `geo ledger card` emits a
redacted visual evidence-card projection by default from stored solve, evidence,
geometry, explanation, and tile-pin bytes; full card output requires `--explicit`.
`geo inspect` ships as a read-only stored-run
inspection command. `geo ledger collision` ships as a read-only cross-deal
collision and adjacency-concentration report over retained ledger ids. No
shipped command was removed or renamed to reach this shape: the tiers change ordering
and visibility, not availability.

##### Primary

**Single-address status (2026-09-16):** an address is a standalone Geo input; a CMBS
deal or loan is optional context. The current address-membership leaf accepts NYC/PAD
inputs supplied by the caller. Nationwide geocoding and footprint acquisition through
external tools are not an automated Canon address-to-building result. The
[four-address baseline](../scripts/geo_measurements/README.md#2026-09-16-four-address-bank-2019-bnk18-baseline)
records the current boundary and replayable NYC evidence. Source-neutral address
membership and the integrated plan/run journey remain open.

`canon geo plan --question --capabilities --inventory --profile --budget` is a
deterministic offline planner. It emits `canon_geo_plan.v0`, a Geo semantic overlay over one
validated `canon.project.plan.v1` DAG. It plans the current parcel/building composition
profile only: omitted/default `parcel` still requires a parcel universe, explicit
`building` can produce a parcel-free building plan, and unsupported grains stay separately
typed instead of poisoning supported grains. Missing local source inputs become typed
discovery/acquisition requests when the required release or as-of selector is present;
otherwise they remain explicit discovery gaps. The command does not execute work, acquire
data, or prove candidate reach without an independent reference.

`canon geo run --plan --work-dir` is a bounded offline run over the Geo plan's single
validated project DAG. It delegates to the shared project runner and registered internal
Geo executor. Set `question.answer_scope` to `current_answer` for eight stages:
materialize home cells, build the bounded tile section, materialize evidence, compile
evidence, propagate constraints, solve, explain, and project `canon_geo_answer.v0`.
Only the home-cell, tile-work and materialize-evidence input bindings are required.
`full_workflow` adds separation and next evidence before the final answer, for ten
stages; those stages require explicit valid prospective-observation/action inputs.
Omitting scope preserves the legacy nine-stage plan without an answer node.
Detectably invalid inputs stop at preflight before execution. Additional registered
stages can extend that DAG. `--input` is
optional at invocation because a blocked plan or missing local artifacts can still produce
a typed `WAITING_FOR_INPUT` run with exact next actions. Missing and invalid inputs
have distinct blocker kinds. Supply actions identify a concrete `input_path`:
create or repair that JSON using the named schema, then execute the supplied public
command unchanged. When supplied, public inputs are
local exogenous leaf artifacts only, including home-cell rows, tile-work requests, warehouse
rows, and prospective-observation/action inputs. Each is bound by artifact id, media/contract,
byte count, and canonical BLAKE3 digest.
Compile-evidence and solve are fed by declared dependency outputs, not by external
overrides. `--satisfy` validates consistency between acquisition receipts and explicit
input bytes only; it does not mutate the plan, clear acquisition blockers, or replan.

The experimental `descriptive_asset_single_member_v0` profile accepts a
`canon_geo_descriptive_asset_request.v0` at the materialization node's `rows`
binding. A pinned bounded inventory supplies the candidates; property type,
unit count, and original construction year pass through declared rho policies.
Source attributes cannot be labeled logical relaxations: hard bands require
empirical calibration, while an explicit `uncalibrated` basis permits diagnostic
or soft evidence. This profile supports declared calibrated bands, diagnostic
policies, and explicit soft weights; conditional support-mask policies are not
supported. Known agreements may rank alternatives; missing values neither
exclude nor earn support. Supplied names and optional street addresses are soft
only, with explicit exact or ASCII case/whitespace comparison. Street suffixes,
directionals, and aliases are not silently rewritten. Acquisition stays external.
This profile asks for one parcel or one building; it does not prove address
membership, complete collateral, or a building assemblage. Target geocode and
target geometry are not accepted. The six-field client profile is unchanged.
The existing bounded search retains residual models and soft rankings when they
fit `max_materialized_models`, including components wider than a machine word.
See the [first addressless experiment plan](PLAN_CANON_GEO_FIRST_ADDRESSLESS_EXPERIMENT.md),
[retained NXRT experiment](../scripts/geo_measurements/fixtures/addressless_nxrt_2026-09-17/README.md),
[Courtney Cove hints/address comparison](../scripts/geo_measurements/fixtures/addressless_courtney_2026-09-17/README.md),
and [measurement runner](../scripts/geo_measurements/addressless_asset.py).

##### Stage leaves

`canon geo link-sources --request` makes the existing N-source materializer reachable
without changing the two-tape `canon entity link` contract. The request declares three or
more named CSV sources, exactly one client-book target, at least one bounded reference,
optional peers, a complete-by-default comparison graph, and explicit pair budgets.
`canonical_reference` is refused on this Geo surface: no parcel or footprint vendor wins
globally by role. The command atomically publishes the merged rows at `--rows-out` and
emits `canon_entity_multisource_link.v1` on stdout with per-source hashes, the merged-row
hash, pair-count diagnostics, anchor-conflict abstentions, and a path-independent semantic
artifact hash suitable for an upstream artifact reference. Roles and source count are
provenance; they do not become evidence weights or independent constraints.

`canon geo materialize-home-cells --rows` derives deterministic H3 blocking/ownership cells
offline from release-bound, fixed-decimal EPSG:4326 representative points using h3o. Each
row binds the point to a source snapshot, record, geometry SHA-256, representative-point
method, and optional transform execution/definition pair. The artifact reports claimed
warehouse-cell matches and mismatches rather than treating a claim as authority, and
retains the cells reached by nine deterministic corner, axis, and center probes of a
declared coordinate perturbation envelope. That sampled sensitivity set and its minimum
covering halo expose blocking-boundary fragility; they do not exhaustively cover the
continuous envelope and are not additional identity evidence. The digest is a typed
binding, not a recomputation: geometry bytes are intentionally absent from this artifact.
One source name cannot mix snapshots, point methods, or transform executions, preventing
temporal evidence from becoming a timeless blocking claim. H3 remains an index, never a
substitute for exact geometric predicates.

`canon geo tile-work --request` turns explicit H3 home-cell assignments, including the
`tile_work_features` projection emitted by `materialize-home-cells`, into a
budgeted center-plus-`k`-ring work unit. The artifact records the complete ordered work
cell set and classifies every supplied feature as center or halo. Mixed resolutions,
duplicate source features, features outside the declared work cells, and cell/feature
budget overruns refuse. Geometry contracts remain authoritative for spatial predicates,
and even verified home-cell assignment cannot prove candidate recall: reach still requires
comparison against a complete bounded reference. Declared budgets are additionally capped
by fixed schema/kernel ceilings. A work unit is an envelope for incidence factorization and
small exact residuals; it is not an instruction to solve every feature in the tile
monolithically.

`canon geo reconcile-tiles --request` consumes independently solved decision batches,
including the exact canonical work unit supplied to each solver, and emits one decision
per canonical member set plus a BLAKE3 receipt for every input work unit. A proposal member
must occur in that work unit with the same home cell. The owner is the numerically smallest
member home cell, independent of source names, input order, or worker completion order. A
missing owner work unit, a proposal seen only from halo work units, an unavailable member,
or different payload digests for the same members is a typed refusal; conflicts are never
silently merged. The reconciler checks payload digests, not the payload semantics, which
remain the local solver's responsibility.

`canon geo solve --request` accepts either a bare
`canon_geo_composition_request.v0` request or the complete
`canon_geo_evidence_compilation.v0` artifact emitted by `compile-evidence`. Solving the
compilation artifact preserves its content digest in the composition output.

`canon geo materialize-geometry --request` converts fixed-decimal source coordinates
through a versioned per-tile integer affine frame into canonical millimetre geometry.
It emits point, polygon, or multipolygon feature values with explicit CRS/frame metadata,
bbox, vertex count, exact snap-loss accounting, and a separate projection-error envelope.
Canonical normalization makes ring direction/start and hole/polygon input ordering
byte-invariant. Invalid topology, non-finite or over-precision coordinates, mixed CRS,
antimeridian crossing, overflow, and declared vertex/byte-budget excess are typed refusals;
decision geometry is never simplified or truncated to fit.

`canon geo materialize-warehouse-geometry --rows` is the release-pinned source-plane
bridge. It recomputes each base64 ISO-WKB SHA-256, refuses mixed source releases,
archives, CRS/SRID values, geometry contracts, or transform executions, and decodes only
2D point, polygon, and multipolygon values. For planar sources it uses the supplied stable,
versioned source origin and exact source-unit-to-millimetre ratio to construct a bounded
local frame. Reusing that origin is mandatory: deriving it from the current row bounds
would move earlier canonical coordinates when new evidence accretes. The artifact reports
WKB-float to fixed-decimal admission loss separately from fixed-decimal to integer-
millimetre snapping; the exact source-plane affine translation declares zero projection
error. Transform ids retain linkage to the WGS84 interoperability sibling, but that
operation is not reapplied to decision geometry.

`canon geo materialize-evidence --rows` is the offline bridge from exported relational
rows to `canon_geo_evidence_request.v0`. Its input contract is
`canon_geo_warehouse_rows.v0`: parcel rows declare the candidate grain,
building/parcel rows declare candidate incidence (a null parcel is an explicit
no-containment marker), and evidence rows group immutable source records under one typed
rho observation. The command rejects duplicate grains and conflicting rows, sorts keyed
collections, validates the result through the same compiler used by `compile-evidence`,
and writes only to stdout. It performs no warehouse acquisition, and multiple source
records remain provenance for one observation rather than independent constraint weight.

`canon geo stack-evidence --population --overlay` accretes contracts and observations onto
named cases in an already bounded `canon_geo_population_request.v0`. The overlay contract
cannot carry truth, candidate members, solver profile, or solver budgets. It may optionally
bind the exact pre-stack evidence digest for optimistic concurrency; the emitted
`canon_geo_population_evidence_stack.v0` retains both inputs, the resulting population,
per-case before/after digests, and hard/soft/diagnostic admission counts so the whole step
can be replay-validated. Exact observation reuse is idempotent. Contract drift, observation
ID drift, and the same semantic observation renamed under another ID are refusals, which
prevents source or preference-count inflation. A validated stack artifact can be passed
directly to another `stack-evidence` invocation or to `canon geo evaluate`. More hard
evidence narrows the feasible set and can make it empty; soft evidence changes objective
cost only; diagnostic evidence changes neither feasibility nor cost. Source-record count is
reported solely as provenance volume.

`canon geo materialize-address-evidence --request` is the single-call offline bridge
from an address parse request plus a PAD address set to an address/PAD parcel
observation bundle. The request contract is
`canon_geo_address_parcel_evidence_request.v0`, which carries the parse request,
PAD set, and `canon_geo_address_parcel_bridge_request.v0`; the emitted
`canon_geo_address_parcel_evidence_bundle.v0` preserves the parse forest, PAD
membership, and `canon_geo_address_parcel_bridge.v0`. Supported ambiguous readings
union their parcel candidates. Readings without PAD source-member support stay
diagnostic, so the command may abstain instead of fabricating an empty hard
constraint. When the bridge emits `bridge.observation`, that observation still must
be wrapped by a separately authored `canon_geo_evidence_request.v0` with an explicit
candidate universe and matching rho `contracts` before `compile-evidence` can admit
it. The bundle itself is not a compiler-ready evidence request. Each source-record
association carries a Canon-recomputed BLAKE3 of the normalized PAD member payload,
so an id-only binding cannot silently change the lot/address content. That integrity
link does not authenticate upstream PAD bytes or replace an acquisition receipt. The
command performs no live acquisition and makes no empirical truth claim.

##### Measurement adapters

`canon geo materialize-h7-staging-batch --batch` is the bounded NYC/H.7 profile adapter
for the staging-derived source-record payload contract. It validates batch guards,
denominators, release pins, source-record bytes, and profile truth planes before delegating
to the generic H.7 population materializer. It is not the generic regional engine and does
not turn derived payload rows or fixtures into live source proof.

`canon geo materialize-h7-pip-block-batch --batch` is the bounded offline adapter for the
warehouse H.7 candidate-block export. It validates the complete observed snapshot,
release-row and source-plane denominators, exact MapPLUTO candidate locators, truth-plane
separation, and a BLAKE3 digest over the typed rows before emitting the generic H.7
population. Observed rows remain diagnostic evidence unless a separately cited live
acquisition satisfies the stricter query-receipt contract.

Open Geo limits remain: acquisition stays outside Canon's deterministic offline run,
exactness is representation-relative to admitted candidates and contracts, candidate reach
is an upstream proof obligation, immutable cross-release reuse in the same work directory is
not guaranteed, E5/live scale proof is not shipped, `geo inspect` reads stored run
artifacts only, and `geo ledger` currently ships `build`, `card`, `collision`,
`exposure`, and `validate`; `card` is redacted by default and emits the full
artifact only with `--explicit`.

The [2026-09-16 standalone-address baseline](../scripts/geo_measurements/README.md#2026-09-16-four-address-bank-2019-bnk18-baseline)
and [command-usability gap plan](PLAN_CANON_GEO.md#1612-command-usability-measured-baseline-and-completion-criteria)
separate the historical baseline from the working target. Brooklyn demonstrated admitted
parcel/building inclusion and exact local solving before the legacy run failed on empty
prospective evidence. Explicit `current_answer` now completes without prospective inputs;
legacy/full-workflow invalid inputs stop at preflight. `geo inspect` now includes bounded
`current_answers`, execution status/phase, blockers and the run's retained `next_actions`.
Answer findings preserve soft support, hard conclusions, acceptance and reach limits;
totals and truncation flags link to the full verified answer artifact. Discovery actions
remain visible before any solve, with their full request in the original plan's
`external_requests`. Inspection reads stored artifacts without executing stages.
Complete inspection/comparison/cost acceptance and automated acquisition/assembly remain
later milestones. These workflows still accept
manually assembled evidence, and retained replays establish integration rather than live truth.

### Arguments

| Argument | Description |
|----------|-------------|
| `<INPUT>` | CSV, JSONL, Parquet or XLSX file. Format detected by extension (`.csv`, `.tsv`, `.jsonl`, `.ndjson`, `.parquet`, `.xlsx`). Use `-` for stdin (JSONL only). The same table gives the same mapping from CSV, Parquet or XLSX. |

### Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--registry <PATH>` | string | *(required)* | Registry directory (versioned). |
| `--column <COLUMN>` | string | *(required)* | Column containing IDs to resolve. |
| `--sheet <NAME\|INDEX>` | string | *(none)* | Worksheet of a `.xlsx` input, by name or 0-based index. Needed when the workbook has more than one visible sheet. |
| `--scope DIMENSION=VALUE` | string | *(none)* | Exact lookup scope binding; repeatable. Use matching scope for scoped registry entries. |
| `--emit <json\|csv>` | string | `json` | Output mode. `csv` requires CSV input. |
| `--canon-column <NAME>` | string | `<COLUMN>__canon` | Name of the appended canonical column. Only with `--emit csv`. |
| `--map-out <PATH>` | string | *(none)* | Write JSON mapping artifact to file. Only with `--emit csv`. |
| `--max-rows <N>` | integer | *(none)* | Refuse if input exceeds N data rows. |
| `--max-bytes <N>` | integer | *(none)* | Refuse if input exceeds N bytes. |
| `--no-witness` | flag | `false` | Suppress witness ledger append. |
| `--explicit` | flag | `false` | Show input and canonical_id values verbatim in JSON output. By default they are masked as `[REDACTED]` for zero-retention safety and the envelope reports `"redacted": true`. |
| `--plain-json-values` | flag | `false` | With `--explicit`, emit UTF-8 JSON `input`/`canonical_id` values without the `u8:` prefix and add `*_encoding` metadata. Non-text/control-byte values remain hex encoded. |
| `--version` | flag | | Print version and exit. |
| `--describe` | flag | | Emit `operator.json` to stdout and exit. |
| `--schema` | flag | | Print JSON Schema for the mapping artifact and exit. |

### Config Footprint

By default, `canon` appends witness records to `~/.cmdrvl/state/witness/witness.jsonl`. `EPISTEMIC_WITNESS` remains an explicit operator override; override paths are used as provided and are not migrated.

On first default witness use, `canon` copy-migrates an existing legacy `~/.epistemic/witness.jsonl` or `.epistemic/witness.jsonl` ledger into the canonical path. It never deletes or moves the legacy file. Migration and deprecation notices are path-only JSONL records under `~/.cmdrvl/migrations/applied.jsonl` and `~/.cmdrvl/notices/deprecated-paths.jsonl`; file contents and secret values are not recorded.

### Subcommands

| Subcommand | Description |
|------------|-------------|
| `doctor [--json]` | Report compiled manifest and read-only contract health (`cmdrvl.doctor.v1`, mode `health`). `doctor health [--json]` is a deprecated alias. |
| `doctor capabilities [--json]` | Describe doctor commands, exit codes, side effects, and fixers (`cmdrvl.doctor.v1`, mode `capabilities`; `contract` is the operator.v1 document). |
| `doctor triage [--json]` | Health with `next_actions` for an agent (`cmdrvl.doctor.v1`, mode `triage`). `doctor --robot-triage` and `--robot-triage` are deprecated aliases. |
| `doctor robot-docs` | Emit concise machine-oriented usage notes. |
| `capabilities [--json]` | With `--json`, the operator.v1 contract, byte for byte the document `--describe` prints. |
| `robot-docs guide` | Agent-oriented markdown guide to the read-only discovery surfaces. |
| `package pack --root <DIR> --package <package.json> --out <ARCHIVE>` | Create a deterministic local package archive from canonical package bytes. |
| `package inspect <ARCHIVE> [--emit json\|summary]` | Inspect package inventory and metadata without writing files. |
| `package verify <ARCHIVE> [--emit json\|summary]` | Verify package archive digests and semantic package contracts. |
| `package unpack <ARCHIVE> --target <EMPTY_DIR> [--emit json\|summary]` | Unpack a verified archive into an existing empty target directory. |
| `package push --archive <ARCHIVE> --registry <OCI_BASE_URL> --repository <REPOSITORY> [--tag <TAG>] [--emit json\|summary]` | Publish a verified local package archive to an OCI registry by immutable digest. |
| `package pull --registry <OCI_BASE_URL> --repository <REPOSITORY> --cache <DIR> (--digest <sha256:...>\|--tag <TAG>) [--emit json\|summary]` | Pull and verify a package from an OCI registry into an external content cache. |
| `project init <DIR> [--project-id <ID>] [--mapping-profile <REF>] [--emit json\|summary]` | Create a minimal neutral project manifest in an explicit empty or missing directory. |
| `project validate <DIR> [--manifest <PATH>] [--emit json\|summary]` | Validate a project manifest and report deterministic diagnostics. |
| `project describe <DIR> [--manifest <PATH>] [--emit json\|summary]` | Describe project capabilities, state flags, side effects, and next commands. |
| `project lock refresh --manifest <MANIFEST> --out <LOCK> [--emit json\|summary]` | Hash actual declared local source bytes and atomically refresh a `canon.project.lock.v1` artifact. |
| `project plan --manifest <MANIFEST> --lock <LOCK> [--out <PLAN>] [--cache-hit <NODE>...] [--emit json\|summary]` | Emit the native validated `canon.project.plan.v1` artifact and executable `project run --node` next commands. |
| `project run [--plan <PLAN>] [--manifest <MANIFEST>] [--lock <LOCK>] [--node <NODE>...] [--workspace <DIR>] [--work-dir <DIR>] [--max-parallelism <N>] [--allow-network] [--allow-mutation-gates] [--emit json\|summary]` | Validate a project plan, reuse valid `canon.project.run.v2` receipts, and execute supported pending nodes through registered internal offline executors. Unknown or unsafe executor declarations still refuse before publication. |
| `geo capabilities [--emit json]` | *(primary)* Emit the compiled offline Geo capability contracts, including each command's surface tier. Deterministic and read-only. |
| `geo plan --question <QUESTION.json> --capabilities <CAPABILITIES.json> --inventory <INVENTORY.json> --profile <PROFILE.json> --budget <BUDGET.json>` | *(primary)* Compile a deterministic offline `canon_geo_plan.v0` over one validated `canon.project.plan.v1` DAG. Missing local sources become typed discovery/acquisition requests or explicit discovery gaps; the command executes no work and acquires nothing. |
| `geo evaluate --population <POPULATION.json> [--truth <DEED_TRUTH.json> --truth-plane deed_grain_instrument] [--artifact-dir <DIR>] [--e4-assessment-out <ASSESSMENT.json>] [--e4-before-assessment <BEFORE.json> --e4-rescore-out <COMPARISON.json>]` | *(primary)* Evaluate a bounded population request and report coverage, reach, rho, solver, truth, and cost as separate planes. This is the E4/E5 gate instrument; optional deed truth binds only Unique rows for truth scoring and reports discarded/no-match rows as unbound, while assessment and rescore comparison sidecars are owned outputs. |
| `geo inspect --run <DIR> [--component <ID>] [--compare <OTHER_RUN>] [--recommend-next] [--emit json\|summary]` | *(primary)* Inspect a stored Geo run manifest, receipts, and content-addressed output artifacts without rerunning acquisition, materialization, compilation, solving, or evaluation. Missing question-specific artifacts are typed as unanswerable; missing or corrupt referenced artifacts refuse. |
| `geo ledger build --seed <SEED.json> --composition <ARTIFACT_ID=COMPOSITION.json> --evidence <ARTIFACT_ID=EVIDENCE.json>` | *(ledger subcommand)* Build a local collateral ledger from strict seed rows and bound solve/evidence artifacts. |
| `geo ledger card --subject-id <SUBJECT_ID> --context <CONTEXT.json> --ortho-pin <PIN.json> [--composition <COMPOSITION.json> --evidence <EVIDENCE.json> --geometry <GEOMETRY.json>] [--explanation <EXPLANATION.json>] [--explicit]` | *(ledger subcommand)* Emit a digest-linked `canon_geo_redacted_artifact.v0` projection by default from stored solve, evidence, retained geometry, explanation, and one pinned ortho tile artifact; full `canon_geo_evidence_card.v0` output requires `--explicit`. |
| `geo ledger collision --ledgers <LEDGER.json> <LEDGER.json> [--pari-passu <DECLARATIONS.json>] [--adjacency <PARCEL_TO_BLOCK.json>]` | *(ledger subcommand)* Emit a `canon_geo_cross_deal.v0` artifact reporting every parcel/building shared across accessions and any multi-accession adjacency block, keeping declared pari passu collisions as labeled rows. |
| `geo ledger exposure --ledger <LEDGER.json> --advisory <ADVISORY.json> --geometry <GEOMETRY.json> --archive <ARCHIVE.json>` | *(ledger subcommand)* Join a pinned wind-radii advisory to exact ledger building geometry and emit a `canon_geo_event_exposure.v0` artifact. |
| `geo ledger validate --ledger <LEDGER.json>` | *(ledger subcommand)* Validate a local collateral ledger artifact with the shipped D3 ledger validator. |
| `geo run --plan <PLAN.json> --work-dir <DIR> [--input <NODE_ID:BINDING_ID=PATH>...] [--satisfy <REQUEST_ID=RECEIPT.json>...]` | *(primary)* Execute or preflight a bounded offline Geo DAG through registered executors and the shared project runner, using local exogenous leaf inputs. The default composition plan has nine stages. It resumes validated completed outputs, refuses undeclared commands or compile/solve input overrides, and emits a `canon_geo_run.v0` projection over `canon.project.run.v2` receipts. `--satisfy` checks receipt/explicit-byte consistency only; it does not mutate the plan, clear acquisition blockers, or replan. |
| `geo replan-from-acquisition --base-plan <PLAN.json> --base-inventory <INVENTORY.json> --question <QUESTION.json> --capabilities <CAPABILITIES.json> --profile <PROFILE.json> --budget <BUDGET.json> --satisfy <REQUEST_ID=RECEIPT.json> --local-artifact <LOCAL_ARTIFACT_ID=PATH>... [--result <DIGEST_ID=PATH>...] --advancement-out <ADVANCEMENT.json>` | *(primary)* Validate one live, complete, positive, nontruncated, full-region acquisition receipt against exact local artifact bytes, atomically publish a separate `canon_geo_regional_inventory_advancement.v0` sidecar, and emit a new base-inventory-bound `canon_geo_plan.v0` on stdout. It never performs acquisition or mutates the old plan or inventory. |
| `geo <stage leaf>` and `geo materialize-h7-*` | Stage-leaf and measurement-tier commands. They stay independently callable for Demo 0, debugging, and tests but are not part of the primary surface — see [Geo command surface](#geo-command-surface) for the full tier table and per-command contracts. |
| `inbox list --inbox <INBOX.json> [--policy <POLICY.json>] [--limit <N>] [--cursor <CURSOR>] [--event-kind <KIND>...] [--reason-code <REASON>...] [--field-role <ROLE>...] [--partition <KEY>...] [--emit json\|summary]` | List ranked unresolved inbox items with deterministic pagination and typed filters. |
| `inbox show --inbox <INBOX.json> --event-key <KEY> [--policy <POLICY.json>] [--emit json\|summary]` | Show one unresolved inbox item and its next commands. |
| `inbox explain --inbox <INBOX.json> --event-key <KEY> [--policy <POLICY.json>] [--emit json\|summary]` | Explain one inbox item's priority score components and provenance. |
| `inbox stats --inbox <INBOX.json> [--policy <POLICY.json>] [--emit json\|summary]` | Summarize inbox counts and ranking coverage. |
| `inbox export-review --inbox <INBOX.json> [--out <REVIEW.json>] [--policy <POLICY.json>] [--limit <N>] [--cursor <CURSOR>] [--event-kind <KIND>...] [--reason-code <REASON>...] [--field-role <ROLE>...] [--partition <KEY>...] [--emit json\|summary]` | Export a stable review queue for selected unresolved inbox items. |
| `inbox apply-review --inbox <INBOX.json> --review <REVIEW.json> --expected-inbox-hash <HASH> --out <GROUPS.json> [--emit json\|summary]` | Apply explicit review decisions into a grouped unresolved artifact. |
| `inbox plan-entity --inbox <INBOX.json> --expected-inbox-hash <HASH> --out <REQUEST.json> [--policy <POLICY.json>] [--event-key <KEY>...] [--limit <N>] [--mode cluster\|link] [--emit json\|summary]` | Plan a bounded entity workbench request without deciding identity. |
| `registry build --source <NAME> --seed <PATH> --seed-column <COLUMN> --output <DIR> --version <VER>` | Materialize a standard canon registry directory from a provider-backed seed corpus, with optional repeatable `--provider-config key=value` overrides such as OpenFIGI `id_type`, `base_url`, `api_key`, or mapping filters like `exchCode=US`. |
| `registry export --format dbt-seed\|search-index\|key-crosswalk\|entity-card --registry <DIR> --out <PATH> [--namespace <CONTEXT>] [--source-file <FILE>...] [--canonical-type <TYPE>...] [--rule-id-prefix <PREFIX>...] [--canonical-iri-prefix <PREFIX>] [--schema-out <schema.yml>] [--anti-collapse-test-out <test.sql>] [--parquet] [--id <ID>...] [--max-bytes <N>]` | Export a versioned registry as a deterministic dbt seed CSV (or Parquet with `--parquet`), self-describing SQLite search index, key crosswalk CSV (or Parquet), or one JSON entity card per ID, with format-specific filters and budgets. |
| `registry providers [--emit json\|summary]` | List the registry build providers available for materialization, with their seed-column support and a pointer to each provider's schema command. |
| `registry provider-schema <PROVIDER> [--emit json\|summary]` | Emit one provider's machine-readable `--provider-config` option contract — keys, types, enum values, secret flags, env fallbacks, defaults, mutual exclusions, interval encoding, and examples. Deterministic and offline; never contacts the provider. |
| `registry identity --registry <DIR> [--ref <REF>] [--emit json\|summary]` | Print the hand-off identity of a registry directory (id, version, package digest, `content_hash` over registry.json, mapping files, `_anchors`, `_escrow`, `_build.json`, `relations.jsonl`, `corrections.jsonl`, per-file hashes) so a driver can read the base after a sync; with `--ref '[<registry id>:]<canonical id>@<registry version>#<closure hash>'` report `id_status` (`current`, `superseded`, `unknown`), `resolves_to`, `version_match` and `closure_found`. Reads only; canon uploads nothing. `entity session commit --expect-base <content_hash>` refuses `base_moved` before any write when the directory differs. The commit receipt carries, per `registries[]` entry, `base` and `new` (`version`, `content_hash`, `package_digest`), `changed_files`, `closures` (`outcome_ref`, `closure_hash`, `ref`), `commit_closure_hash` and the `export_command` that builds the search index; a repeat or a nothing-to-commit reports equal `base` and `new` and empty `changed_files` and `closures`. |
| `registry check-id <ID> [--registry <DIR> \| --registries <MAP_YAML>] [--emit json\|summary]` | Verify an agent-friendly id (`org_7k3m9q2xh43z`, optional `~label`): length, alphabet, kind prefix and the two check symbols. A bad id is refused with `E_BAD_ID` and `detail.reason` of `bad_length`, `bad_symbol`, `unknown_kind` or `bad_check`; a good id returns `{kind, id, resolves_to, label}` where `label` is `match`, `mismatch` or `absent` and a mismatch is not a refusal. |
| `registry next-id [PREFIX] --registry <DIR> [--zero-pad <N>] [--emit plain\|json]` | Read the existing canonical IDs for a self-authored namespace and suggest the next deterministic ID. Uses `registry.json.default_id_scheme` when `PREFIX` is omitted. |
| `registry add-entry --registry <DIR> --alias-file <FILE> --canonical-id <ID> --input <INPUT> --rule-id <RULE> [--canonical-type <TYPE>] [--scope DIMENSION=VALUE]...` | Append one exact alias entry to an existing root mapping file, bump the registry version, update `entry_count`, and run standard lint unless `--no-lint` is set. Scoped duplicate inputs are allowed only across distinct scopes. |
| `registry add-entries --registry <DIR> --from <FILE.jsonl> [--bump patch\|minor\|major \| --next-version <VER>] [--no-lint] [--emit json\|plain]` | Attach a JSONL batch of exact aliases to existing canonical IDs in one versioned transaction; unknown IDs, malformed lines, duplicate inputs, and cross-incumbent collisions refuse atomically. |
| `registry mint --registry <DIR> [--canonical-id <ID>\|--prefix <PREFIX>] --canonical-type <TYPE> --with-alias <FILE=INPUT:RULE_ID>... [--scope DIMENSION=VALUE]...` | Mint one self-authored canonical ID and one or more starting aliases in a single versioned write. Without `--canonical-id`, allocates via `next-id`; scoped aliases require a matching lookup scope. `--create-alias-file` creates a missing root alias file (starting as `[]`) inside the same transactional write, so a `registry.json`-only directory can be bootstrapped natively; a refused or failed mint removes the file it created. |
| `registry default-id-scheme --registry <DIR> --prefix <PREFIX> [--zero-pad <N>] [--strict]` | Persist the registry's default self-authored ID convention in `registry.json` so `next-id` and `mint` can allocate without a prefix argument. |
| `registry scope-policy --registry <DIR> --optional-dimension role [--next-version <VER>]` | Declare a scope dimension optional in `registry.json` `scope_policy`: a bare lookup resolves the unscoped entry and `--scope role=R` returns the entry scoped exactly so, else the unscoped one. Without the policy a registry holding any scoped entry still refuses a bare lookup (`E_SCOPE_REQUIRED`). |
| `registry diff --old <PATH> --new <PATH> [--emit json\|summary]` | Compare two versions of the same registry ID and report added, removed, changed, and unchanged effective mappings. |
| `registry audit <SEED> --registry <PATH> --column <COLUMN> [--emit json\|summary]` | Audit a seed corpus against a registry and emit resolved/unresolved entries plus aggregate canonical-target and rule-hit counts. |
| `registry lint <DIR> [--profile standard\|org\|strategy\|auto] [--emit json\|summary]` | Preflight standard mapping, entity-sidecar, or strategy registry health with severity-tagged findings. |
| `strategy profile <INPUT> [--emit json\|summary] [--max-rows <N>] [--max-bytes <N>]` | Derive a deterministic schema/profile artifact from CSV, TSV, JSONL, or NDJSON for `strategy resolve` and `strategy register`. |
| `strategy audit --schema <JSON> --script <PATH> --suite <DIR> [--emit json\|summary]` | Run a frozen script against deterministic fixture expectations and emit a `canon_strategy_audit.v0` proof artifact. |
| `strategy resolve --registry <DIR> (--schema <JSON>\|--task <TASK>) --skill <PATH>\|--skill-hash <HASH>` | Resolve a schema or exact task key plus skill hash to a frozen champion script. Schema keys can return EXACT/COMPATIBLE/PARTIAL/UNRESOLVED; task keys return EXACT or UNRESOLVED only. |
| `strategy register --registry <DIR> (--schema <JSON>\|--task <TASK>) --skill <PATH>\|--skill-hash <HASH> --script <PATH> --script-id <ID> --language <LANG> --grade operator-attested\|proof-attested --next-version <VER>` | Register a v1 typed strategy entry. Operator-attested entries need operator/reason and no proof artifacts; proof-attested entries require verify/assess/airlock. |
| `strategy update --registry <DIR> (--schema <JSON>\|--task <TASK>) --skill <PATH>\|--skill-hash <HASH> --script <PATH> --script-id <ID> --language <LANG> --next-version <VER>` | Update an active champion in place with a version bump. |
| `strategy deprecate --registry <DIR> (--schema <JSON>\|--task <TASK>) --skill <PATH>\|--skill-hash <HASH> --operator <ID> --reason <TEXT> --next-version <VER>` | Mark an active champion deprecated without deleting history. |
| `strategy promote --registry <DIR> (--schema <JSON>\|--task <TASK>) --skill <PATH>\|--skill-hash <HASH> --verify <JSON> --assess <JSON> --airlock <JSON> --next-version <VER>` | Promote an operator-attested champion to proof-attested. |
| `strategy list --registry <DIR> [--key-type schema\|task] [--grade operator-attested\|proof-attested] [--status active\|deprecated] [--emit json\|summary]` | Inspect mixed schema/task strategy registries, provenance, grade, status, source file, and entry order. |
| `strategy explain --registry <DIR> (--schema <JSON>\|--task <TASK>) --skill <PATH>\|--skill-hash <HASH> [--emit json\|summary]` | Explain active and ignored entries for one strategy key. |
| `strategy diff --old <DIR> --new <DIR> [--emit json\|summary]` | Compare frozen-script strategy registry versions by typed key plus skill hash, including grade/status/attestation changes. |
| `entity run <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--cache-mode enabled\|disabled] [--suite <DIR>] [--emit json\|summary]` | Run the cluster-mode artifact pipeline (prepare -> index -> block -> evidence -> solve, optional audit). Cache mode defaults to enabled, and native cache receipts are recorded in the run artifact. |
| `entity link <REFERENCE> <TARGET> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--suite <DIR>] [--gold <JSONL>] [--write-back] [--emit json\|summary] [--cache-mode enabled\|disabled] [--max-candidates <N>] [--max-rows <N>] [--max-bytes <N>] [--no-witness]` | Run link mode for aligning two row sets through the same typed request and artifact path as project mode, with optional suite/gold scoring. Profile and work-dir are required for successful execution even though generated syntax shows them bracketed; omissions write nothing. `--write-back` currently refuses before work-dir or registry mutation, so accepted knowledge flows through review, audit, promote, and apply. Cache mode defaults to enabled, and native cache receipts are inherited from the nested run. |
| `entity calibrate sweep <RESULT\|EVIDENCE> --gold <GOLD.jsonl> --strategy <STRATEGY.yaml> [--emit json\|summary]` | Sweep deterministic integer threshold tuples against gold labels and emit `canon.entity.calibrate_sweep.v0`. The report maximizes auto-accept subject to frozen `canon.entity.quality.v1` gates, emits a YAML threshold fragment for manual application, and never writes registry or strategy files. |
| `entity calibrate em <EVIDENCE.jsonl> --strategy <STRATEGY.yaml> [--emit json\|summary]` | Estimate m/u probabilities from aggregated support-evidence agreement patterns and emit `canon.entity.calibrate_em.v0`. The report uses integer fixed-point EM, suppresses weights for constant, zero-cell, or non-converged operators, emits a proposed `support_scores` YAML fragment for manual review, and never writes registry or strategy files. |
| `entity prepare <ROWS> --profile <PROFILE> --registry <DIR> --work-dir <DIR>` | Validate and project profile-mapped observations for artifact-backed entity preparation. |
| `entity index build <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--emit json\|summary]` | Build deterministic index artifacts for a work directory. |
| `entity block <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--emit jsonl\|summary]` | Generate candidate neighborhoods via blocking operators. |
| `entity block preflight <ROWS> --profile <PROFILE> --strategy <YAML> [--sample-pct <N>] [--work-dir <DIR>] [--emit json\|summary]` | Estimate candidate cardinality and worst-block skew before running the block stage. |
| `entity candidate-recall --manifest <MANIFEST.json> --candidates <CANDIDATES.jsonl> --diagnostics <DIAGNOSTICS.json> --exact-bucket-count <N> [--emit json\|summary]` | Evaluate candidate retrieval recall against sealed public must-link labels. |
| `entity evidence <ROWS> [--profile <PROFILE>] --strategy <YAML> --candidates <JSONL> --registry <DIR> [--work-dir <DIR>] [--emit jsonl\|summary]` | Score typed evidence for blocked candidate pairs. Relationship evidence remains a relation hint unless a separate equality fact or support lane justifies equivalence. |
| `entity solve <ROWS> [--profile <PROFILE>] --strategy <YAML> --evidence <JSONL> --registry <DIR> [--work-dir <DIR>] [--emit json\|summary]` | Solve deterministic identity assignments from evidence artifacts. |
| `entity audit <RESULT> --suite <DIR> [--emit json\|summary]` | Validate a solve/run artifact against a frozen evaluation suite. |
| `entity promote <RESULT> --audit <JSON> --registry <DIR> --next-version <VER> [--emit json\|summary]` | Write audited results into registry aliases and escrow sidecars. |
| `entity apply <RESULT> --rows <ROWS> --registry <DIR> [--column <COL>] [--output <PATH>] [--work-dir <DIR>] [--require-full-resolution\|--allow-partial-output] [--emit json\|summary]` | Replay accepted assignments from a solve or run artifact onto input rows without changing the registry. |
| `entity review export <RESULT> [--artifact queue\|native-review] [--group-by signature] [--emit json\|csv\|html] [--include resolved\|escrow\|contradictions\|all]` | By default, produce the unchanged review queue contract. With `--artifact native-review`, emit `canon_entity_native_review.v0` as JSON, CSV, or offline HTML; `--group-by signature` presents native items by deterministic evidence signature without changing decision authority. |
| `entity review import <REVIEW> (--registry <DIR> --next-version <VER> \| --session <SESSION_DIR> --reviewer <REVIEWER>) [--audit <JSON>] [--source-review <NATIVE_REVIEW.json>] [--emit json\|summary]` | Import default queue decisions, or with `--source-review` import native decisions, including JSON group decisions expanded against the source review artifact; session imports use the manifest-bound registry and return decisions as evidence. |
| `entity explain <RESULT> --row <ID>\|--surface-id <ID>\|--canon-id <ID>\|--escrow-id <ID> [--emit json\|summary]` | Proof trace for one row, prepared surface, canonical entity, or escrow entity. |
| `entity profile list [--emit json\|summary]` | List built-in entity profile templates. |
| `entity profile init <PROFILE> --output <PATH>` | Write a built-in entity profile template to disk. |

### Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success: RESOLVED, healthy report, or successful report/write |
| `1` | Domain outcome needs inspection: PARTIAL, UNRESOLVED, unhealthy contract health, or another command-specific failed gate |
| `2` | REFUSAL or CLI error |

`canon registry diff`, `canon registry audit`, `canon registry export`, and `canon registry lint` exit `0` when the report succeeds and `2` on refusal. Lint findings are represented inside `canon_registry_lint.v0` rather than via exit status. `canon registry build`, `registry next-id`, `registry check-id`, `registry add-entry`, `registry add-entries`, `registry mint`, and `registry default-id-scheme` exit `0` when their report or write succeeds and `2` on refusal. Provider failures from `registry build` are preserved in the JSON report and warned on stderr. `add-entry`, `add-entries`, and `mint` restore the original files if their post-write lint gate finds errors.

`canon strategy profile`, `canon strategy register`, `canon strategy update`, `canon strategy deprecate`, `canon strategy promote`, `canon strategy list`, `canon strategy explain`, and `canon strategy diff` exit `0` when their reports or writes succeed and `2` on refusal. `canon strategy audit` exits `0` when all fixtures pass, `1` when deterministic fixture checks fail, and `2` on refusal. `canon strategy resolve` exits `0` for a schema EXACT/COMPATIBLE or task EXACT match, `1` for schema PARTIAL/UNRESOLVED or task UNRESOLVED, and `2` on refusal.

`canon entity link` exits `0` when every target record is matched, `1` when any target record is unmatched or ambiguous, and `2` on refusal. In `summary` mode, refusal JSON is written to stderr.

`canon entity calibrate sweep` exits `0` when it emits a structurally valid JSON
or summary report, including `recommendation.status: blocked` when no threshold
tuple satisfies `canon.entity.quality.v1`. It exits `2` for malformed
result/gold/strategy inputs or gold pairs absent from the scored artifact.

`canon entity calibrate em` exits `0` when it emits a structurally valid JSON or
summary report, including `recommendation.status: blocked` when no operator has
safe proposed support-score weights. It exits `2` for malformed evidence,
strategy, or sealed acceptance/holdout inputs. The command is read-only and
does not mutate strategies, registries, work directories, or witness ledgers.

Every `canon doctor ... --json` report follows the CMD+RVL family contract
`cmdrvl.doctor.v1`: `schema_version`, `mode` (`health`, `capabilities` or
`triage`), `tool`, `tool_version`, `healthy`, and `findings[]` (`id`, `severity`
of `info`, `warning` or `error`, `message`, and a `fix` on a failing finding).
`healthy` is false exactly when a finding has severity `error`. The capabilities
report adds `contract`, the operator.v1 document; the triage report adds
`next_actions`. `canon doctor`, `canon doctor capabilities` and `canon doctor
triage` exit `0` when compiled contract parity is healthy and `1` when the
report is emitted but unhealthy. `canon doctor robot-docs` exits `0` when its
read-only text is emitted. All doctor forms exit `2` for CLI usage errors such
as unsupported `--fix`, never for an unhealthy report. The old per-tool schema
ids `canon.doctor.health.v1`, `canon.doctor.capabilities.v1` and
`canon.doctor.triage.v1` are retired in favour of `schema_version`, and the old
`contract` string is now `read_only_contract`; the other detail fields (`ok`,
`checks`, `summary`, `contract_parity`, ...) are kept alongside.

`canon doctor health`, `canon doctor --robot-triage` and `canon --robot-triage`
are deprecated aliases that print the same documents as `canon doctor --json`
and `canon doctor triage --json`. `canon capabilities --json` prints the
operator.v1 contract, the same document as `canon --describe`, and
`canon robot-docs guide` prints an agent-oriented markdown guide to these
discovery surfaces. None of them has a `--fix` surface.

### Output Routing

| `--emit` | stdout | Mapping artifact | Use case |
|----------|--------|------------------|----------|
| `json` (default) | JSON mapping object | IS stdout | Audit, pack, inspection |
| `csv` | Canonicalized CSV | `--map-out` sidecar | Pipeline stage |

---


## Refusal Codes

Every refusal includes the error code, a concrete message, and a recovery path.

| Code | Meaning | Next Step |
|------|---------|-----------|
| `E_IO` | Can't read input or registry | Check paths and permissions |
| `E_ENCODING` | Unsupported text encoding | Convert/re-export as UTF-8 |
| `E_CSV_PARSE` | CSV parse failure | Re-export as standard CSV |
| `E_BAD_REGISTRY` | Registry format invalid | Fix `registry.json` or mapping files |
| `E_SCOPE_REQUIRED` | Scoped registry lookup without a query scope | Rerun with `--scope <SCOPE>` after selecting the intended registry scope |
| `E_COLUMN_NOT_FOUND` | `--column` doesn't exist in input | Check column name |
| `E_PARSE` | Can't parse input or unrecognized extension | Use `.csv`, `.tsv`, `.jsonl`, or `.ndjson` |
| `E_EMPTY_INPUT` | No processable data | Check input file |
| `E_TOO_LARGE` | Exceeds `--max-rows` or `--max-bytes` | Increase limits or reduce input |
| `E_EMIT_FORMAT` | `--emit csv` with JSONL input | Use `--emit json` or provide CSV input |
| `E_COLUMN_EXISTS` | Canonical column name already in header | Choose a different `--canon-column` |
| `E_PACKAGE_NONCANONICAL` | Package JSON bytes are not canonical compact JSON | Rewrite package JSON as sorted-key compact UTF-8 bytes |
| `E_PACKAGE_CONTRACT` | Package fields, digests, paths, or archive constraints violate the package contract | Fix package metadata or package-root contents |
| `E_ENTITY_PROFILE` | Entity profile is unknown, missing, or semantically invalid | Fix the profile or pass a valid profile path |
| `E_ENTITY_STRATEGY` | Entity strategy YAML is malformed or references unsupported operators | Fix the strategy file |
| `E_ENTITY_INPUT_CONTRACT` | Entity input rows violate the active profile contract | Check required fields and side-field JSON |
| `E_ENTITY_SURFACE_ID_COLLISION` | Distinct prepared surfaces produced the same surface ID | Inspect the profile projection and source rows |
| `E_ENTITY_PATCH_CONFLICT` | Entity alias, distinctness, or relation patches contradict each other or the registry | Fix the conflicting patch input |
| `E_ENTITY_REGISTRY_SNAPSHOT` | Entity registry snapshot does not match the consumed artifact | Re-run against the current registry |
| `E_ENTITY_CACHE_MISMATCH` | Entity cache artifact hashes do not match the current run | Rebuild or bypass the cache |
| `E_ENTITY_INDEX_LIMIT` | Entity index posting, bucket, or top-k limits were exceeded | Tighten blocking or raise the relevant limit |
| `E_ENTITY_CANDIDATE_BUDGET` | Entity candidate budget was exceeded before bounded candidate emission | Tighten filters or raise the candidate budget |
| `E_ENTITY_ARTIFACT_CONTRACT` | Entity artifact has the wrong version, profile, strategy, registry, or hash | Rebuild the artifact chain |
| `E_GEO_COMMAND_UNAVAILABLE` | Geo primary command is planned but not implemented in this build | Use `canon geo capabilities --emit json` |
| `E_ENTITY_CANNOT_LINK_OVERRIDE` | Entity merge request conflicts with a hard cannot-link fact | Remove or justify the conflicting merge request |
| `E_ENTITY_REVIEW_IMPORT` | Entity review import is malformed, stale, or references unknown items | Regenerate the review artifact and decisions |
| `E_ENTITY_AUDIT_GATE` | Entity audit artifact is missing, stale, or failed required gates | Re-run audit and address failed gates |
| `E_ENTITY_APPLY_UNRESOLVED` | Entity apply requires full resolution but unresolved surfaces remain | Use a complete result or allow partial output |
| `E_ENTITY_IO_BUDGET` | Entity row, byte, artifact, memory, or work-dir budget was exceeded | Increase limits or reduce inputs |
| `E_BAD_STRATEGY` | Resolve strategy YAML is malformed or invalid | Fix the strategy file |
| `E_TOO_MANY_CANDIDATES` | Resolve candidate filters left too many candidates | Tighten filters or raise `--max-candidates` |
| `E_EMPTY_TAPE` | Resolve reference or target tape has no processable records | Provide non-empty tapes |
| `E_INCOMPATIBLE_TAPES` | Resolve strategy leaves no comparable fields | Fix strategy field mappings |
| `E_BAD_ID` | `registry check-id` id is malformed or fails its check symbols (`detail.reason`: `bad_length`, `bad_symbol`, `unknown_kind`, `bad_check`) | Copy the id again from the registry or session receipt |

---

## Troubleshooting

### "E_COLUMN_NOT_FOUND" but the column exists

Column names are matched exactly (byte-for-byte after ASCII-trim). Check for invisible characters, BOM artifacts, or case mismatches. The refusal message lists available columns.

### "E_BAD_REGISTRY" on a registry that looks fine

All `.json` files in the registry directory except `registry.json` and `_build.json` must be valid mapping files. Check for stray JSON files, malformed entries, or missing required fields (`input`, `canonical_id`, `canonical_type`, `rule_id`).

### Unresolved entries that should match

v0 matching is exact byte match after ASCII-trim only. No case normalization, no punctuation stripping. Check that the registry contains the exact variant present in your input. Use `jq` to inspect unresolved entries:

```bash
canon tape.csv --registry registries/cusip-isin/ --column cusip \
  | jq '.unresolved[] | .input'
```

### Large registries are slow on first use

`canon` builds a SQLite derived index (`_index.sqlite`) on first use. Subsequent runs use the cached index. The build is logged to stderr.

---

## JSON output reference

<details>
<summary><strong>JSON Output Reference</strong></summary>

A single JSON object on stdout. This is the default output and the format used for `--map-out` in CSV mode.

```jsonc
{
  "version": "canon.v0",
  "outcome": "PARTIAL",                   // "RESOLVED" | "PARTIAL" | "UNRESOLVED" | "REFUSAL"
  "registry": {
    "id": "cusip-isin",
    "version": "3.2.1",
    "source": "registries/cusip-isin/"     // path as provided via --registry
  },
  "summary": {
    "total": 4183,                         // unique input values processed
    "resolved": 4150,
    "unresolved": 33
  },
  "redacted": true,                        // present on RESOLVED/PARTIAL/UNRESOLVED; true when values are masked. Re-run with --explicit to reveal. Absent on REFUSAL.
  "mappings": [                            // one per resolved unique input
    {
      "input": "u8:037833100",
      "canonical_id": "u8:AAPL",
      "canonical_type": "ticker",
      "rule_id": "CUSIP_TO_TICKER",
      "confidence": "deterministic"        // v0: always "deterministic"
    }
  ],
  "unresolved": [                          // one per unresolved unique input
    {
      "input": "u8:UNKNOWN123",            // null for special reasons (empty_value, null_value, etc.)
      "reason": "no matching rule"
    }
  ],
  "refusal": null                          // null unless REFUSAL
  // When REFUSAL:
  // "refusal": {
  //   "code": "E_COLUMN_NOT_FOUND",
  //   "message": "Column 'cusip' not found in input file",
  //   "detail": { "column": "cusip", "available_columns": [...] },
  //   "next_command": "canon ... --column security_id"
  // }
}
```

### Identifier Encoding (JSON)

Input values and canonical IDs in JSON use unambiguous encoding:
- `u8:<string>` — valid UTF-8 with no ASCII control bytes
- `hex:<hex-bytes>` — anything else

This lossless encoding remains the default for `--explicit` JSON audit
artifacts. For CLI and human workflows that want ordinary strings, pass
`--explicit --plain-json-values`; valid UTF-8 values are emitted without the
`u8:` prefix and each value gets `input_encoding` / `canonical_id_encoding`
metadata; unresolved non-null inputs use `input_encoding`. Values that cannot
be represented safely as text still use `hex:`.

CSV output uses raw values (no encoding prefix).

### Invariant

`summary.total == summary.resolved + summary.unresolved`. Every unique input value is classified as exactly one of resolved or unresolved.

### Confidence Values

- `"deterministic"` — exact match in versioned registry, fully reproducible
- `"suggested"` — probabilistic match, not auto-accepted (v1)

### Unresolved Reasons

| Reason | Trigger |
|--------|---------|
| `"no matching rule"` | Non-empty value had no exact match |
| `"empty_value"` | Value was empty after ASCII-trim |
| `"missing_field"` | JSONL object missing the `--column` field |
| `"null_value"` | JSONL field was JSON `null` |
| `"non_scalar_value"` | JSONL field was an object or array |

Special reasons (`empty_value`, `null_value`, `missing_field`, `non_scalar_value`) produce at most one unresolved entry each, with `input: null`.
