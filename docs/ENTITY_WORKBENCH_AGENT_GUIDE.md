# canon entity — agent guide

How an agent (or operator) takes a CSV of names to a reviewed registry
proposal with `canon entity`, without reading
[`PLAN_ENTITY_WORKBENCH.md`](PLAN_ENTITY_WORKBENCH.md). The plan is the
specification; this is the how-to. If they disagree, the plan wins.

Boundary reminder ([`IDENTITY_ARCHITECTURE.md`](IDENTITY_ARCHITECTURE.md)):
the workbench produces *proposals*. Runtime lookup stays exact-match against a
reviewed, versioned registry. Nothing below loosens that.

## Start an evidence session

For a new registry, initialize the session and registry together:

```bash
canon entity session init \
  --goal "dedupe BDC borrowers" \
  --registry registries/organisations \
  --work-dir work/bdc-session \
  --registry-id organisations \
  --prefix ORG \
  --zero-pad 6
```

This creates `registry.json`, an empty `aliases.json`, and an append-only
session manifest under `work/bdc-session/.canon/entity/publications/v1`.
`canon registry lint registries/organisations` and
`canon registry next-id --registry registries/organisations` are useful
follow-up checks; an empty registry correctly leaves exact lookup unresolved.
Repeat the same command to get an idempotent `already_initialised` result.
Changing the goal or registry id for an existing work directory is refused;
use a new `--work-dir`. If the registry already exists, `--registry-id`
and `--prefix` are only needed when you want to check the requested registry id.
The next session steps are `canon entity session ingest` for observations and
`canon entity session assert` for typed evidence.

## Ingest observations

Append one observation JSON object per line to the session. The file may be
replaced with `-` to read JSONL from stdin:

```bash
canon entity session ingest work/bdc-session observations.jsonl --emit json
cat observations.jsonl | canon entity session ingest work/bdc-session - --emit json
```

When an Evidence Machine hands Canon a payload, verify the raw bytes before
projection. The artifact and receipt are local JSON documents; Canon does not
call the Evidence Machine or any other network service:

```bash
canon entity session ingest work/bdc-session sponsors.payload.json \
  --rows-field rows --column-map sponsors.map.yaml \
  --artifact sponsors.artifact.json --receipt sponsors.receipt.json --emit json
canon entity session ingest work/bdc-session observations.jsonl \
  --expect-digest blake3:<64-hex-digits> --emit json
```

The observation map names `obs_id`, `entity_type`, `surface`, optional `keys`,
`attrs`, and `context` columns, plus a `source` map. It may declare
`origin_template: "deal:{deal_cik}"` and `primary_key: [obs_id]`; a row's own
`row_hash` is retained, otherwise the declared primary-key values are retained
as `source.row_key`. The verified artifact adds `artifact_id`, `result_digest`,
`sensitivity`, and (when supplied) `receipt_digest`; payload `catalog_key` and
the source-column map are retained verbatim.

Each row requires `obs_id`, `entity_type`, `surface`, and
`source.channel`. `keys`, `attrs`, and `context` are optional objects whose
values must be scalar or null. Accepted rows are stored in deterministic,
content-addressed observation shards. A malformed row is reported with its
line, rejection code, and `fix`, while other rows are still appended; exit 1
means at least one row was rejected. Repeat an exact ingest to receive
`accepted=0` and an `already_present` count without creating a new generation.
Ingest trims only ASCII whitespace around required text fields; a surface such
as `` `tick` `` is retained as-is.

## Assert evidence

Append typed evidence assertions to the same session, from a JSONL file or
stdin:

```bash
canon entity session assert work/bdc-session evidence.jsonl --emit json
cat evidence.jsonl | canon entity session assert work/bdc-session - --emit json
```

Evidence Machine query results can use the same verified artifact/receipt path.
For a rows payload, the assertion map names `type`, `left`, `right`,
`asserted_by`, `basis`, and `source` columns. `--expect-digest` alone is also
valid and binds the verified payload digest into evidence provenance. A missing
map column, a byte mismatch, an unknown artifact schema, or a non-complete
receipt refuses before the session stream is changed.

Short assertion rows compile into the existing `canon.evidence.v1` record:

```json
{"type":"name_equivalent","left":"obs-a","right":"obs-b",
 "basis":{"rule":"casefold+strip_punct","left_value":"A, LLC","right_value":"A LLC","steps":["casefold","strip_punctuation"]},
 "source":{"channel":"script","id":"goldset:1","locator":"row:4","content_hash":"blake3:<64 hex>"},
 "asserted_by":"script:goldset"}
```

The supported types are `same_key`, `same_slot`, `attribute_agreement`,
`name_equivalent`, and `asserted_alias` (positive pair support), `distinct`
(a hard cannot-link requiring `basis.authority`), and `related` (a contextual
pair relation requiring `basis.relation` and `basis.subject`). `asserted_alias`
requires a locator and content hash for non-human asserters. Complete
`canon.evidence.v1` records are accepted as well. Rows are canonicalized and
stored in the `entity-session-evidence` publication stream; malformed rows are
reported individually with line, code, message, and fix while valid rows are
still appended. Exit 1 means at least one row was rejected.

## Solve a session

Use a declared rule table to classify exact observation nodes as tier A, tier B,
review, standalone, or contradiction:

```bash
canon entity session rules init --out session-rules.yaml --emit json
canon entity session rules validate session-rules.yaml --emit json
canon entity session solve work/bdc-session --rules session-rules.yaml --emit json
```

The solve reads the committed observation and evidence generations, performs
exact registry lookup for incumbents, and publishes `solve/solve.json` in the
`entity-session-solve` stream. A shared authoritative key or asserted alias is
tier A. Tier B needs the declared number of distinct eligible evidence types
and independent source lineages; missing corroboration stays review. `distinct`
is a contradiction, `related` is reported as context and never joins a
cluster, and a node with no positive evidence remains standalone. Solve exit 0
means the deterministic artifact completed even when it reports review or
contradiction outcomes.

The minimum rule table shape is:

```yaml
version: canon.entity.session.rules.v1
keys:
  cik: {authoritative: true, single_valued: true}
  lei: {authoritative: true, single_valued: true}
  ticker: {authoritative: false, single_valued: false}
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
  required_evidence_types: []
standalone:
  mint_when_authoritative_key: true
guards:
  max_cluster_surfaces: 50
incumbent_attach:
  name_equivalent_steps: [casefold, strip_punctuation, collapse_whitespace]
```

## Commit accepted session clusters

Commit only accepted tier A/B clusters into the registry bound by the session:

```bash
canon entity session commit work/bdc-session --next-version 0.1.0 --emit json
# or: canon entity session commit work/bdc-session --bump patch --emit summary
```

The commit is a bounded local registry transaction. It derives the canonical
type from the session kind, mints one ID per accepted cluster, writes exact
surface and authoritative-key aliases, and publishes `commit/commit.json` in
the `entity-session-commit` stream. Review, contradiction, standalone, and
incumbent clusters are reported in `skipped`; no human gate is required.
Every minted row carries a stable `outcome_ref` and `closure_hash`. A changed
observation, evidence, solve, or registry snapshot is refused as stale before
any registry write. Repeating the same solve against its committed version is
an `already_committed` no-op.

To accrete into an existing registry, initialize the next session with the
same `--registry` directory rather than seeding a second copy:

```bash
canon entity session init --goal "day two aliases" \
  --registry registries/organisations/ --work-dir work/day-two \
  --entity-kind organisation
canon entity session solve work/day-two --rules rules.yaml
canon entity session commit work/day-two --bump patch --emit json
```

An accepted cluster with one incumbent ID attaches only new exact surfaces to
that ID and reports them under `attached[]`; it does not mint a duplicate.

### Registry memory: known review and vetoes

Session commits keep review memory beside the exact lookup aliases in the
registry. The `_escrow/pending.jsonl` sidecar records tier-C reviews and open
contradictions; a later solve reports a matching item as `known_pending` with
its stable `escrow_id` and increments `review_known` instead of presenting it
as a new question. A later accepted A/B cluster or a decided relation clears
the matching pending item in the same commit.

Reviewed `distinct` assertions write canonical `CannotLinkFact` rows to
`_escrow/cannot_link.jsonl`. Future solves load those rows as hard anti-merge
constraints: a positive edge that crosses a veto is reported as
`contradiction` with reason `registry_cannot_link`, and commit skips it without
minting or attaching either side. Both sidecars are deterministic JSONL and
are checked by the normal registry lint command; they are memory, not runtime
fuzzy matching, and do not change exact lookup.

### Keys in the registry

Commit writes each accepted cluster's authoritative key values as exact aliases
of its ID, one mapping file per namespace: the registries map's `key_file`, or
`keys.<namespace>.json` when the map names none (created when absent). Each
entry has rule id `SESSION_KEY_<NAMESPACE>` and an `external_global_id`
namespace, so plain lookup of a CIK or LEI returns the canonical ID. Namespaces
the rule table does not mark authoritative are written only when listed:

```yaml
commit:
  key_namespaces: [ticker]
```

A row whose surface is already an ID's name and which carries an authoritative
key is accepted as `incumbent_keyed`: the existing ID gains the key and no
spelling or ID is added. A key value that already means a different ID (another
namespace's value, or a name) is skipped as `key_alias_collision`; the skip's
`detail` names both entries, the rest of the cluster is still written, and
nothing is shadowed. A name that belongs to one ID and a key that belongs to
another is an `incumbent_conflict` and writes nothing.

```bash
canon registry export --registry registries/organisations/ \
  --format key-crosswalk --out crosswalk.csv
```

The crosswalk has one row per canonical ID that holds a key and one column per
key namespace present; several values of one namespace are joined with `;` in
sorted order. It lists only keys committed through a session.

To give an agent bounded context for one ID, export entity cards from the same
registry:

```bash
canon registry export --registry registries/organisations/ \
  --format entity-card --out cards/ --id ORG-001 --max-bytes 4096
```

Each `<id>.json` card carries the exact registry identity, display names and
keys, part-of and unit relations, reviewed distinct counterparts, retired
successor information, and founding evidence when present. `--max-bytes`
records every dropped field in `truncated`; it never drops the ID, kind,
registry, status, successor, primary name, authoritative keys, part-of, or
evidence.
Registry aliases can also be attached in a separate atomic JSONL batch:

```bash
canon registry add-entries --registry registries/organisations/ \
  --from aliases.jsonl --bump patch --emit json
```

Each `aliases.jsonl` line has `canonical_id`, `alias_file`, `input`, and
`rule_id`. Unknown IDs and aliases owned by another ID refuse the whole batch;
aliases already owned by the requested ID are no-op skips.

## Handing a commit to the publisher

`session commit` writes the registry directory; canon never uploads it. The receipt tells the driver exactly what to publish. Read it per registry, under `registries[]`:

- `base` and `new`: `{version, content_hash, package_digest}` before and after the commit. Keep `new.content_hash`; pass the previous one back as `--expect-base` on the next commit, and a directory that moved underneath you refuses with `base_moved` before anything is written.
- `changed_files`: the files whose bytes differ, each with `change` (`added`, `modified`, `removed`) and both hashes. Sidecar-only changes (`_escrow/`, `_anchors/`, `relations.jsonl`) appear here too; upload these, not the whole tree.
- `closures`: one `{outcome_ref, closure_hash, ref}` per entry the commit wrote. Store `ref` (`firms:ORG-002@1.0.1#blake3:...`) next to the downstream record; `canon registry identity --ref <ref>` later says whether it is `current` or `superseded`.
- `export_command`: the argv that builds the served `<version>/search.sqlite` for the new version.

A repeat of the same commit, or one with `nothing_to_commit`, shows `base` equal to `new` and empty `changed_files` and `closures`: there is nothing to publish.

## Where does the session stand

One call answers it, without opening any artifact file:

```bash
canon entity session status work/bdc-session --emit json
# add --rules rules.yaml to also detect rule drift against the recorded solve
```

`canon.entity.session.status.v1` reports the session and its registry, the log
(observations, observation shards, unreceipted observations, evidence by type), the recorded solve
(`present`, `stale` with `stale_reasons` of `new_observations`, `new_evidence`,
`rules_changed` or `registry_changed`, and the tier counts exactly as the solve
recorded them), the commit state (last version, minted IDs, aliases attached,
accepted clusters still uncommitted), the 20 `top_unresolved` review pairs and
standalone surfaces ranked by how often they were observed, and one
`next_command`: the solve when it is stale, the commit when accepted clusters
are uncommitted, otherwise `null` (nothing pending). It only ever names a
command that exists.

A session with no solve is a normal answer (`solve.present: false`, exit 0). A
directory that is not a session is a refusal (exit 2). Status computes and
prints; it never writes a file, and the counts restate the solve, not a quality
measure. Rule drift is only reported when `--rules` names the file, because the
session does not store the rules path. Rejected rows from a past ingest are not
persisted, so status does not list them.

## Explain a session result

Use the recorded solve and evidence streams to inspect one observation, exact
surface, or cluster without opening artifact files:

```bash
canon entity session explain work/bdc-session r8 --emit json
canon entity session explain work/bdc-session 'surface:Medline Borrower, LP' --emit summary
canon entity session explain work/bdc-session 'cluster:<DIGEST>' --emit json
```

`canon.entity.session.explain.v1` reports the target node, its observations,
lineages, keys, cluster outcome and tier, every touching evidence edge with its
asserted-by and basis fields, contradictions, relations, and concrete
`to_move_up` conditions in rule-table terms. A stale solve is marked
`stale:true` and does not present its old cluster as current. Use
`canon entity session solve <SESSION> --rules <RULES.yaml> --what-if <EXTRA.jsonl>`
to evaluate additional observation/assertion rows in an isolated copy; it
returns only changed edge states and rejected rows and never changes the source
session.

## What is still unresolved: the frontier

`session frontier` states what the recorded solve still leaves open, as data. It
does not choose the next action; a driver does that.

```bash
canon entity session frontier work/bdc-session --emit json
canon entity session frontier work/bdc-session --seed-entity 'Beacon Sponsor LLC'
canon entity session next work/bdc-session        # the same data, one line per fact
```

`canon.entity.session.frontier.v1` is bound to `solve_hash` and refuses
(`stale_solve`, exit 2) when observations or evidence changed after the solve.
It lists:

- `coverage`: per key family (`name` or a key namespace) and source, how many
  distinct surfaces were `attempted`, `supported` (in an accepted or
  resolved-existing cluster), `conflicted` (in a contradiction) or `unresolved`.
- `unresolved_keys`: authoritative key values on one surface with no partner,
  and key values inside a contradiction.
- `components`: review items grouped by what is missing. `needs_independent_origin`
  (a second origin; two observations whose origins overlap count as one),
  `needs_second_evidence_type`, `needs_authoritative_key` (a required type is
  absent; `settles_with` names the authoritative namespaces) and `needs_human`
  (contradictions and oversized clusters). Each group reports `items`,
  `surfaces`, `observation_count` and `settles_with`, the evidence types that
  would move its items up. `settles_with` is conditional on that evidence
  existing; an empty frontier does not say the residue is irreducible.
- `terminal`: standalone surfaces with no positive evidence and dangling evidence.
- `history_requests`: for a review pair `same_slot` could settle, the prior
  observations it would need: the shared context (for example deal, loan, role)
  and the earliest as-of date as `before`. Fetch them however you like and ingest
  them as ordinary rows.

`--seed-entity` restricts all of this to one entity's neighbourhood (its cluster,
the surfaces linked by `part_of`, and role-marked surfaces that name it) and adds
a `seed` block: authoritative namespaces still missing per entity, spellings
still in review, units with no `part_of` evidence, and the history needed.
Nothing here names a command; rejected ingest rows are not persisted, so they
are not part of the frontier.

## The evidence behind an outcome

`session export --format evidence-closure` writes the records a committed outcome
rests on, as local files `pack seal` can seal. Canon seals, uploads and indexes
nothing.

```bash
canon entity session export work/bdc-session --format evidence-closure --out out/closures
canon entity session export work/bdc-session --format evidence-closure --out out/one --outcome cmdrvl://canon/outcome/organisations/minted_id/<HASH>
pack seal out/closures            # one pack per commit
pack seal out/closures/<HEX>      # one pack per outcome
```

Each outcome gets `out/closures/<64 hex>/` with `closure.json` (the stored
manifest; its blake3 is the `closure_hash` written on the alias entry, relation
line or cannot-link record), `observations.jsonl` (the stored records plus their
`source_handles`; a handle the observation does not carry is `null`),
`evidence.jsonl` and `rules.yaml`. `out/closures/commit.json` is the stored
closure set. The JSON on stdout gives each closure's `sensitivity`: `public` only
when every input states `public`; an input with no stated sensitivity counts as
`unstated_inputs` and is never treated as public. A closure shows what canon
decided and on which records, not that the upstream source told the truth.

## How are these two linked?

```bash
canon registry export --registry REG --format link-evidence --from 'Fund Alpha Name' --to 'Adviser Spelling Inc'
canon registry export --registries registries.yaml --format link-evidence --from key:cik:10 --to 'Deal One'
```

The answer is the shortest chain of committed outcomes between the two, read from
the registry directories alone (delete the session directories and it is the
same). Alias, key and superseded-id entries are `identity` hops; a
`relations.jsonl` line is a `relation` hop with its relation name and is never
called identity. Each hop carries the `outcome_ref` and `closure_hash` it was
written under: export that closure with `session export` from the session that
wrote it, or look it up in a sealed pack. A hop that predates sessions has
`closure_hash: null`. Two look-alike names with no committed chain return
`chain: null, reason: no_committed_chain`; nothing is inferred from name
similarity. Across registries (`--registries`) only a relation line whose object
names another registry's id crosses between them.

## Corrections: merging two IDs that are one entity

A session never merges two existing IDs (a cluster with two incumbents is an
`incumbent_conflict` contradiction). A reviewer decides a merge, writes it as a
line, and `registry correct` applies it as a new registry version:

```bash
echo '{"op":"merge","survivor":"ORG-001","retired":"ORG-002","reviewer":"human:zac","evidence_ids":["evidence:blake3:..."],"reason":"same legal entity, CIK 1234567 on both"}' > merge.jsonl
canon registry correct --registry REG --from merge.jsonl --next-version 1.1.0 --rules rules.yaml --dry-run
canon registry correct --registry REG --from merge.jsonl --next-version 1.1.0 --rules rules.yaml --expect-base blake3:...
```

The retired ID keeps resolving, exactly, to the survivor: `canon rows.csv
--registry REG --column id` returns the survivor with rule_id
`SUPERSEDED_ID_ALIAS`. It is not found by search (`superseded-id` in the search
index) and is never minted again. Every re-pointed entry keeps the
`session_authority` it had; the correction adds its own closure
(`corrections.jsonl`, and the `SUPERSEDED_ID_ALIAS` entry), so
`registry export --format link-evidence` from the retired ID to a survivor
spelling returns a chain that includes it. A merge across a cannot-link, or
(with `--rules`) of two IDs with different authoritative keys, is refused;
without `--rules` the output says `key_check: not_run`. Rerunning the same file
is `already_applied`.

### Detach, split and removing a cannot-link

The same file and command take three more reviewer-written ops:

```jsonl
{"op":"detach","canonical_id":"ORG-000412","inputs":["Newmark Realty Capital"],"keys":[{"namespace":"cik","value":"1234567"}],"reviewer":"human:zac","evidence_ids":["..."],"reason":"..."}
{"op":"split","canonical_id":"ORG-000412","move":{"inputs":["Apex Capital LLC"],"keys":[{"namespace":"cik","value":"444"}]},"reviewer":"human:zac","evidence_ids":["..."],"reason":"two CIKs, two entities"}
{"op":"remove_cannot_link","left":"ORG-000412","right":"ORG-000977","reviewer":"human:zac","evidence_ids":["..."],"reason":"..."}
```

A **detach** takes a wrong spelling or key off the id; exact lookup of it is
unresolved afterwards, and registry memory holds a cannot-link against the id so
a later session does not re-attach it (it reports `registry_cannot_link`). A
**split** mints a new id for the moved entries and records a cannot-link between
the two ids; relations stay on the old id and are reported for you to move by
hand. Both refuse to take the last entry. **remove_cannot_link** is the only way
a cannot-link leaves memory (for example before a merge of two ids a split
separated). Every kept and moved entry keeps the `session_authority` it had;
the correction line carries its own closure and, for a detach, the closure of
each entry it removed, so earlier evidence stays reachable from
`corrections.jsonl`, which is append-only. Nothing in a session ever detaches or
splits on its own.

## Evidence Canon derives itself

`session derive` runs Canon's own entity operators over the observations and
appends their findings as evidence, so a session does not need a script to
assert name evidence:

```bash
canon entity session derive work/bdc-session --profile profile.yaml --strategy strategy.yaml
canon entity session solve work/bdc-session --rules rules.yaml
```

Which operator emits which evidence type:

| Operator in the profile | Evidence type | Counts toward tier A or B |
|---|---|---|
| `exact_view` on a normalised view | `name_equivalent` (`basis.steps` lists the view's steps) | yes (tier B) |
| `anchor_match`, `isin_cusip_arithmetic` | `same_key` | yes (tier A) |
| `anchor_conflict`, `attribute_conflict`, `protected_token_conflict` | `distinct` | vetoes a join |
| `string_similarity`, `tfidf_cosine`, `two_token_reversal`, `token_overlap` | `name_similar` | never |
| `date_transposed_digits` | `value_similar` | never |

**Day two.** A new session whose rows share nothing with the first session's
still finds its spellings: derive also puts the registry's aliases in the rows
file (`origin: registry`, so the profile lists `origin` in
`prepare.provenance_fields`) and an `exact_view` hit between a new surface and a
registry alias is evidence to that alias, so the commit attaches the new
spelling to the first session's ID instead of minting a second one. Registry
rows are candidates only for the session's surfaces: a registry of 50,000
unrelated aliases adds work to prepare and index it, but no registry-to-registry
pair is generated or scored (`registry_candidates` in the output says so, and
`records_digest` is unchanged). `--no-registry-candidates` turns it off.
The rule table decides what is the same spelling: an `exact_view` hit to a
registry alias attaches only when the two spellings are equal after
`incumbent_attach.name_equivalent_steps` in the rules file (the template from
`session rules init` ships `[casefold, strip_punctuation, collapse_whitespace]`).
A rules file without that section still gets the evidence, but solve treats it
as a review item that needs a second lineage, which a registry alias cannot
bring, and nothing attaches.

Similar is not equal: `Mode Purchaser` and `BGI Purchaser` produce
`name_similar` only and are never accepted, whatever the lineages. The profile
must put `source_row_id` in `prepare.provenance_fields` and use
`surface_key_fields` that make a surface of one observation (or a name view),
and it must declare `cannot_link` and `relation_hints` (the validator requires
them; derive runs no relation hint and lists them). The output lists records by
type and operator, the observations prepare rejected (by obs_id), and how many
hits crossed entity types. Running derive again adds nothing.

## The loop

```text
rows.csv -> [profile] -> entity run -> review export -> human decision
         -> review import -> audit -> promote -> apply
```

| Step | Command | Output |
|------|---------|--------|
| Pick a profile | `canon entity profile list` / `profile init <ID> --output p.yaml` | a profile YAML to edit |
| Run | `canon entity run rows.csv --profile p.yaml --strategy p.yaml --registry REG --work-dir wd --emit summary` | `wd/run/run.json`, `wd/solve/solve.json`, … |
| Export the queue | `canon entity review export wd/run/run.json --include all --emit json` (or `--emit csv`) | `canon_entity_review.v1` queue |
| Decide | fill each item's `decision`; items left blank are not imported | decided queue |
| Import | `canon entity review import queue.json --registry REG --next-version X.Y.Z` | new registry version |
| Audit | `canon entity audit wd/run/run.json --suite SUITE --emit json > audit.json` | audit artifact (see "Audit suites and gold labels") |
| Promote | `canon entity promote wd/run/run.json --audit audit.json --registry REG --next-version X.Y.Z` | registry knowledge for the audited run; refuses an audit that does not certify this exact run |
| Apply | `canon entity apply wd/run/run.json --rows rows.csv --registry REG --column COL --out out.csv [--allow-partial-output]` | rows with canonical IDs appended; refuses before writing if any row stays unresolved unless `--allow-partial-output` (then exit 1) |

Notes from running this loop:

- `review export` accepts the run (`run.json`) or its solve (`solve.json`).
  Exporting the run reads the items from the solve the run hash-binds and keeps
  the queue bound to the run, which is the artifact `audit` and `promote`
  certify. A solve that does not match the run's recorded hash is refused.
- `--artifact native-review` accepts `canon_entity_link.v1`, `canon_entity_run.v1`,
  `canon_entity_solve.v1`, and legacy v0 run/solve artifacts. A v1 run export
  follows its single hash-bound solve stage and binds the native review to the
  run hash; the exact solve version and hash stay in the review's upstream
  references. A direct solve export binds to that solve hash. The native review
  envelope remains `canon_entity_native_review.v0`.
- Import native decisions with `canon entity review import decisions.json
  --registry REG --next-version X.Y.Z --source-review review.json`. Cluster
  assignment requires an explicit `target_canonical_id`; import returns proposal
  patches and does not write registry truth.
- Alias proposals take `accept_alias` or `reject_alias` as their `decision`.
- A component whose positive-support edges contain a hard cannot-link is
  reported as state `contradiction` (reason `hard_cannot_link_constraint`),
  never merged and never promotable. By default the solver does not split it.
  To resolve numbered or lettered siblings deterministically, opt in with
  `solve: {hard_cannot_link: split_direct}` (see "Solver options" below).

## What the agent decides vs. what a human must

- Agent: extract observations, choose and author the profile, run the
  workbench, read evidence and explain output, prepare the decided queue for
  review.
- Human: new canonical IDs default to human review. Only an explicit
  `promotion.new_ids: auto_accept` strategy, backed by a passing frozen labeled
  audit, may stand in for that. No policy may accept a component containing a
  hard cannot-link.
- Nobody: matching at lookup time. It stays exact.

## Audit suites and gold labels

There are two different things called "suite"; know which one you need.

**1. The suite directory given to `entity audit --suite DIR`.** For a
`canon_entity_run.v1` / `canon_entity_solve.v1` artifact the directory may hold
a `manifest.json`:

```json
{"id": "my-suite", "version": "v1",
 "gates": [{"gate_id": "G01", "label": "artifact continuity", "passed": true,
            "expected": "v1_self_hash_valid", "actual": "v1_self_hash_valid"}]}
```

`gate_id` must be one of `G01`..`G15`, unique, with a non-empty label; a gate
with `"passed": false` makes the audit refuse with `E_ENTITY_AUDIT_GATE`. A
directory with no `manifest.json` (or an empty `gates` list) gets the two
default gates `G01` artifact continuity and `G14` promotion preflight. **These
`passed` flags are asserted by whoever wrote the manifest; canon does not
compute them.** Treat such an audit as proof that the artifact chain is intact
(the audit certifies the exact run hash that `promote` then checks), not as
evidence that the matches are correct. Correctness evidence comes from gold
labels, below.

**2. The frozen policy suite, which *is* computed.** It exists only when the
strategy declares `promotion: {new_ids: auto_accept, auto_accept: {...}}`.
`audit.suite` in that policy names a directory holding `policy_suite.json`,
and the audit re-runs the same strategy on that corpus and scores what the
policy would accept against your labels. Nothing in the file can claim a
precision; every input is pinned by `blake3:` hash and a mismatch refuses
(`suite_input_hash_mismatch`).

```text
SUITE/
  policy_suite.json   rows, rows_hash, profile, profile_hash, registry,
                      registry_snapshot_hash, gold, gold_hash,
                      gold_provenance, work_dir   (unknown keys are refused)
  gold.json           JSON array: [{"surface_id": "...", "entity_id": "..."}, ...]
```

- `gold.json` labels *surfaces with an entity*, not pairs: surfaces that share
  an `entity_id` are the same entity, different ids are distinct. It must cover
  exactly the prepared surface universe of the suite corpus (get the ids from
  `wd/prepare/surfaces.jsonl` after one run of the same rows and profile), have
  at least 3 labels and 2 entities, and at least one entity with more than one
  surface (so both positive and negative pairs exist).
- `gold_provenance` must be non-empty and states who labeled it. Operator
  labels are not replaceable by agent or model labels for a real-data claim.
- Hashes are `canon::witness::hash_bytes`, i.e. `blake3:` + hex of the file
  bytes (`b3sum` hex with that prefix). Any edit to rows, profile, registry
  snapshot or gold needs a new manifest.
- The policy passes only if the computed pair and component precision reach
  `min_pair_precision` / `min_component_precision`. A fixture-built suite
  (`tests/entity_profile_instrument.rs`,
  `declared_policy_runs_a_frozen_audit_then_promotes_exact_aliases`) proves the
  mechanics, not quality; label real samples before claiming a precision.

### Gold JSONL for `entity calibrate sweep`

`canon entity calibrate sweep RESULT --gold gold.jsonl --strategy strat.yaml`
reads one JSON object per line (a JSON array, or an object with a `gold`,
`labels`, `pairs` or `records` array, also works). Pair labels, not entity
labels:

```jsonl
{"left_surface_id": "s_aaa", "right_surface_id": "s_bbb", "label": "same_entity"}
{"left_surface_id": "s_aaa", "right_surface_id": "s_ccc", "label": "distinct_entity", "critical": true}
```

- Accepted labels: `same`/`same_entity`/`match`/`must_link`/`positive` and
  `distinct`/`distinct_entity`/`non_match`/`cannot_link`/`negative`.
  Anything else is refused.
- `critical: true` marks a false merge that must never happen; any accepted
  critical false merge fails the tuple.
- The sweep re-scores the run's own evidence pairs at candidate thresholds and
  recommends one only if a tuple meets `canon.entity.quality.v1`: precision
  at least 99.50% **and** recall at least 98.00% with zero critical false
  merges. Otherwise the status is `blocked` (`no threshold tuple satisfied
  canon.entity.quality.v1`) and no threshold is suggested.
- Deriving pair gold from entity gold: for every unordered pair of labeled
  surfaces, emit `same_entity` if their `entity_id` is equal, else
  `distinct_entity`. Label the sibling pairs that matter (RR vs RR H, Fund II
  vs Fund IV) explicitly as `distinct_entity`; they are the cases a score
  alone cannot separate.
- What a profile needs to pass: score-only support cannot separate suffix
  siblings from spelling variants, so a profile that reaches the gate needs
  anti-merge evidence for them: `protected_token_conflict` for numbered
  siblings, `attribute_conflict` / `anchor_conflict` where rows carry an
  attribute or registry-grade anchor. Check the operator catalog: only
  operators marked "yes" affect the sweep.

Quickest honest loop for a new profile: run once, copy surface ids from
`wd/prepare/surfaces.jsonl`, write `gold.json` (policy suite) or the pair
JSONL (sweep), run `calibrate sweep`, add anti-merge operators until it stops
reporting `blocked`, then pin the suite and run `entity audit`.

## Solver options (`solve:` in the strategy file)

Both settings are opt-in; without a `solve:` section the solver behaves exactly
as described above. They live in the file passed as `--strategy` (not in the
profile).

```yaml
strategy_id: my_strategy
strategy_version: 0.1.0
solve:
  hard_cannot_link: split_direct   # default: contradiction
  standalone_surfaces: include     # default: omit
```

**`hard_cannot_link: split_direct`.** Support edges whose two endpoints are
*directly* hard-cannot-linked are cut, and components are recomputed. "Sub III"
and "Sub IV" stop sharing a component; spelling variants of one vehicle
("Sub III LLC" / "Sub III, L.L.C.") stay merged because no hard cannot-link sits
between them. Every surface that lost an edge is kept as its own component, so
nothing disappears. A conflict that survives the cut (A~B and B~C supported,
A and C cannot-linked, no direct A~C support edge) stays a `contradiction` for a
human. The cut edges stay visible in `evidence/evidence.jsonl` (both the support
and the cannot-link hits), and `summary.counts.diagnostics_split_direct_cut_edges`
records how many were cut. Hyperedges from exact buckets are never cut.

**`standalone_surfaces: include`.** By default a surface with no support edge
and no incumbent ID is absent from the solve: it is not an entity and not a
review item. With `include` it becomes a one-surface component in state
`escrow`, reason `standalone_surface_no_support`, and is counted in
`summary.counts.diagnostics_standalone_surfaces`.

## Minting IDs for entities that matched nothing

`entity run` never mints an ID for a lone surface, and that is by design: the
only new-ID route is `promotion.new_ids: auto_accept`, which refuses any policy
that allows a one-surface component or zero positive support, and requires a
passing frozen audit. An ID for an entity with no matching evidence is an
operator-accepted fact, so it goes through the registry maintenance commands:

1. Run with `standalone_surfaces: include` (and `split_direct` if you have
   sibling vehicles) to list every surface that still needs an ID.
2. Review the list, then write one JSON object per line:
   `{"with_alias": ["aliases.json=Acme Fund II, LLC:MANUAL"], "canonical_type": "vehicle"}`
   (`with_alias` is the same `FILE=INPUT:RULE_ID` spec as `registry mint`;
   `canonical_type` is optional per line).
3. `canon registry mint-batch --registry REG --canonical-type vehicle --from batch.jsonl`

`mint-batch` allocates sequential IDs from the registry's `default_id_scheme`
(or `--prefix`), writes every alias, and bumps the version **once**. It is
atomic: any refusal (a duplicate input inside the batch or already in the
registry, a bad alias file, a lint error) leaves the registry byte-identical.
`--emit plain` prints one new ID per line.

For a later cut whose aliases belong to IDs already in the registry, use
`registry add-entries` instead of minting a second ID:

```jsonl
{"canonical_id":"ORG-000012","alias_file":"aliases.json","input":"Acme Holdings Ltd.","rule_id":"SESSION_TIER_B"}
```

```bash
canon registry add-entries --registry REG --from attach.jsonl --bump patch
```

The batch infers the incumbent canonical type, attaches all new aliases in one
versioned write, skips aliases already owned by that ID, and refuses unknown
IDs or cross-incumbent collisions without partial writes.

## Recording lookup misses for the next cut

Plain exact lookup can record what it did not find, so a driver can turn misses
into its next bounded batch. Canon records; it does not schedule or fetch.

```bash
canon rows.csv --registry REG --column name \
  --inbox-out misses.inbox.json --inbox-project P --inbox-run R1 \
  --inbox-seen-at 2026-11-16T00:00:00Z [--inbox-raw-ref]
```

Each unresolved value becomes one `canon.unresolved.inbox.v1` item (one item per
event key; a value missed again in a later run, with another `--inbox-run`,
gains an occurrence). The file is created or merged and replaced atomically;
stdout gains an `inbox{path,items_added,items_total,artifact_content_hash}`
block and nothing else changes (stderr in `--emit csv` mode). Canon reads no
clock, so `--inbox-seen-at` is required. The missed string is never stored:
`--inbox-raw-ref` adds an external reference to the input file, column and input
hash, and the driver reads the string from there. List what was recorded with
`canon inbox list --inbox misses.inbox.json`.

## Authoring a profile

Start from `canon entity profile init <built-in> --output p.yaml`. A profile
must declare non-empty `normalized_views`, `prepare`, `patch_namespaces`, and
all three evidence lanes (`support`, `cannot_link`, `relation_hints`); a
missing field is refused with `E_ENTITY_PROFILE` listing `missing` plus the
`supported_operators` per lane.

**Who owns what.** `entity run` takes its evidence lanes from the *profile*.
`--strategy` is read for its id, version, budgets and content hash; an
`evidence:` block in a separate strategy file has no effect. (The shipped
fixtures pass the profile file as the strategy, which is why that works there.)

### Candidate retrieval budgets

Candidate budgets belong to the strategy file under `block:`, not to the
profile's `blocking:` retrieval section:

```yaml
block:
  max_candidates_per_surface: 100
  max_candidates_per_operator: 25000
  max_candidates_per_run: 25000
  max_exact_bucket_size: 10000
```

The default per-operator and per-run limits scale from the per-surface limit
and the prepared surface count. An explicit value remains authoritative, but
the absolute safety ceiling still applies. If a run crosses a limit before
writing candidates, the refusal names the exact key (for example,
`block.max_candidates_per_run`), says that it belongs in the strategy file,
and includes a ready-to-paste `block:` fragment. A budget under `blocking:` or
in the profile file is reported as misplaced rather than ignored.

### Operator catalog

Every operator the profile validator accepts, and whether `canon entity run`
executes it. **"declared only" means the validator accepts it but `run` does
nothing with it** — `entity run --emit summary` prints a warning naming them.
Relation hints are never executed per operator: `run` emits one generic review
hint per retained candidate.

<!-- operator-catalog:start -->
| Lane | Operator | Executed by `entity run` |
|------|----------|--------------------------|
| support | exact_view | yes |
| support | anchor_match | yes |
| support | isin_cusip_arithmetic | yes |
| support | token_overlap | yes |
| support | string_similarity | yes |
| support | tfidf_cosine | yes |
| support | date_transposed_digits | yes |
| support | two_token_reversal | yes |
| cannot_link | anchor_conflict | yes |
| cannot_link | attribute_conflict | yes |
| cannot_link | protected_token_conflict | yes |
| relation_hints | related_brand_family | declared only |
| relation_hints | possible_successor_predecessor | declared only |
| relation_hints | same_parent_or_sponsor | declared only |
| relation_hints | division_of | declared only |
| relation_hints | parent_subsidiary_context | declared only |
| relation_hints | cross_profile_alignment | declared only |
| relation_hints | context_alignment | declared only |
| relation_hints | segment_alignment | declared only |
| relation_hints | issued_by | declared only |
| relation_hints | superseded_by | declared only |
<!-- operator-catalog:end -->

`tests/entity/run_review_handoff.rs` fails if this table and the code disagree.
Generic operators have a `view:` and `params:`; the Reg-AB-flavored ones
(`division_of` and `parent_subsidiary_context`) remain relation-hint
declarations; `entity run` emits one generic review hint for retained
candidates rather than executing each relation operator separately.

A support operator with no threshold (`string_similarity`/`tfidf_cosine`
without `min_score_units`) is disabled by design and produces no evidence.

### `protected_token_conflict`

Keeps numbered siblings apart ("Fund II" vs "Fund IV", "Sub III" vs "Sub IV").
Required: `view`. Optional params:

- `protect` — comma list of `digits`, `roman`, `none`. Default `digits,roman`.
  `digits` protects any token containing a digit. `roman` protects series
  numerals written with only i/v/x, two letters or longer, up to xxxix.
  Single letters and other Roman letters are not protected (initials and words
  like "mix" are common); list them in `protected_tokens` if you need them.
- `protected_tokens` — extra comma-separated tokens to protect.
- `shared_context_view` — optional declared normalized view. Exact, non-empty
  equality on both surfaces can scope a protected-token conflict even when
  their unprotected whitespace tokens are disjoint. Missing, blank or different
  values supply no such scope; an unknown view refuses. This is conflict
  evidence only, never identity support or acceptance authority.
- `score_units` — `0` disables the operator.

By default, a pair is cut (hard cannot-link) only when its protected-token sets differ
**and** the two names still share an unprotected token. "Acme Coffee" vs
"Acme Coffee Shop" is not cut; unrelated names are not made distinct by a
stray digit.

With `shared_context_view`, a differing protected-token set can also cut the
pair within that explicitly shared context. Tokenization is unchanged: for
example, `Example 2 Company` and `Example2Company` have different whole protected
tokens and are conservatively cut when they share that context, despite the
same numeral. This option does not establish numbered-name recall or split
condensed identifiers into inferred words.

## Enrichment: where agent-sourced facts live

Decision (bd-6znd): enrichment stays **upstream** of the workbench. The
workbench has no "agent sources facts" step and no enrichment-row contract.

- Add the facts as extra columns on the rows, with sibling provenance columns
  (`<fact>_source`, `<fact>_retrieved_at`, `<fact>_receipt`).
- List the provenance columns in the profile's `prepare.provenance_fields` so
  they ride along on the prepared surface for review.
- Map an agent-sourced fact to `context_fields` (support, never a hard cut) by
  default. Map it to `anchor_fields` only when its source is registry-grade;
  an anchor conflict is a hard cannot-link.

Revisit only if a second use case needs a shared enrichment shape.
