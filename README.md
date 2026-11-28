# canon

![canon: versioned identifier resolution. A painterly dashboard showing 32 email hashes resolving against a Canonical People Registry v3.2. Two of the 32 resolve to canonical person records; thirty are unresolved (structural, not error). The email-hash normalization rule appears as four checked steps: lowercase, trim, drop +suffix, sha256. The footnote reads: canon is operational, not a social-graph mirror.](docs/images/canon.webp)

> *Identity is a registry, not a guess. Zero matches is a finding, not a failure.*

<div align="center">

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**The same entity has five names across three vendors. canon makes them one.**

```bash
brew install cmdrvl/tap/canon
```

</div>

---

The same company appears as a CIK in one file, a legal name in another and a
misspelling in a third. Your pipeline needs those rows to carry one ID, the same
ID every time, and you need to be able to say why.

canon does three things:

1. **Lookup.** Resolve a column of values against a versioned registry by exact
   match. Same input and same registry version give the same bytes out.
2. **Registries.** Keep the accepted names, keys and IDs as plain JSON files with
   a version, so they can be reviewed in git, diffed, corrected and exported.
3. **Evidence sessions.** Let one or more agents collect observations and typed
   evidence about which of them are the same entity. Fixed rules decide what the
   evidence proves, and only what is proved is written to the registry.

```text
observations + evidence -> session -> solve (rules) -> commit -> registry -> exact lookup
     (agents, files)                        |                       |
                                            v                       v
                                   frontier: what is         exports: dbt seed,
                                   still unresolved          search index, crosswalk
```

Nothing is guessed at lookup time. Matching that needs judgement happens in a
session, before the registry changes, and every ID a session creates records the
evidence and rules that produced it.

## Lookup

```bash
$ canon tape.csv --registry registries/cusip-isin/ --column cusip --emit csv
cusip,balance,cusip__canon
037833100,1000000,US0378331005
UNKNOWN99,500000,
594918104,250000,US5949181045
```

Two of three values resolved, so the outcome is `PARTIAL` and the exit code is 1.
The unresolved row keeps an empty canonical column.

Without `--emit csv`, canon prints one JSON object: the mapping artifact.

```bash
$ canon tape.csv --registry registries/cusip-isin/ --column cusip --explicit --plain-json-values
```

```json
{
  "version": "canon.v0",
  "outcome": "PARTIAL",
  "registry": { "id": "cusip-isin", "version": "1.0.0", "source": "registries/cusip-isin/" },
  "summary": { "total": 3, "resolved": 2, "unresolved": 1 },
  "redacted": false,
  "mappings": [
    { "input": "037833100", "input_encoding": "utf8", "canonical_id": "US0378331005", "canonical_id_encoding": "utf8", "canonical_type": "isin", "rule_id": "CUSIP_TO_ISIN", "confidence": "deterministic" },
    { "input": "594918104", "input_encoding": "utf8", "canonical_id": "US5949181045", "canonical_id_encoding": "utf8", "canonical_type": "isin", "rule_id": "CUSIP_TO_ISIN", "confidence": "deterministic" }
  ],
  "unresolved": [
    { "input": "UNKNOWN99", "input_encoding": "utf8", "reason": "no matching rule" }
  ],
  "refusal": null
}
```

By default the JSON masks input values and canonical IDs as `[REDACTED]`;
`--explicit` shows them. Input can be CSV, TSV, JSONL, Parquet or XLSX.
`--emit csv` needs CSV input.

How lookup behaves:

- **Exact match.** Bytes are compared after trimming ASCII whitespace. There is no
  case folding and no punctuation stripping. If a variant should resolve, it is
  an entry in the registry.
- **One answer per unique value.** Values are deduplicated first, so a file with
  500,000 rows and 500 distinct values produces 500 entries.
- **Four outcomes.** Every run ends in exactly one of these:

| Outcome | Meaning | Exit code |
|---------|---------|-----------|
| `RESOLVED` | Every value matched | 0 |
| `PARTIAL` | Some matched, some did not | 1 |
| `UNRESOLVED` | None matched | 1 |
| `REFUSAL` | canon could not run: missing column, bad registry, bad flags | 2 |

A refusal names its code (for example `E_COLUMN_NOT_FOUND`), says what was wrong
and suggests the next command. The codes are listed in the
[command reference](docs/CLI_REFERENCE.md#refusal-codes).

In a pipeline, canon comes before the comparison:

```bash
canon nov.csv --registry registries/cusip-isin/ --column cusip --emit csv > nov.canon.csv
canon dec.csv --registry registries/cusip-isin/ --column cusip --emit csv > dec.canon.csv
rvl nov.canon.csv dec.canon.csv --key cusip__canon
```

## Registries

A registry is a directory:

```text
registries/cusip-isin/
├── registry.json        # id, version, description, entry count
├── cusip-to-isin.json   # mapping file: an array of entries
└── _build.json          # optional build provenance, ignored by lookup
```

Each entry maps one exact input to one canonical ID:

```json
{"input": "037833100", "canonical_id": "US0378331005", "canonical_type": "isin", "rule_id": "CUSIP_TO_ISIN"}
```

Every lookup result carries the registry `id` and `version`, so a result can
always be traced to the registry that produced it. canon builds a SQLite index
beside the files on first use; the JSON files remain the source.

The `canon registry` commands change a registry without hand-editing JSON. The
commands that write keep the version and the entry count in step.

| Task | Commands |
|------|----------|
| Create a registry and choose how IDs are numbered | `registry init`, `default-id-scheme`, `next-id` |
| Add IDs and aliases | `mint`, `mint-batch`, `add-entry`, `add-entries` |
| Fix a mistake: merge two IDs, detach or split an alias, record a successor | `correct` |
| Check before use | `lint`, `audit`, `diff`, `identity`, `check-id` |
| Fill from a provider such as OpenFIGI | `build`, `providers`, `provider-schema` |
| Allow a lookup scope such as role to be optional | `scope-policy` |
| Hand to other systems | `export --format dbt-seed`, `search-index`, `key-crosswalk`, `entity-card` |

```bash
canon registry init --registry registries/people --registry-id people --prefix PPL --zero-pad 3
canon registry mint --registry registries/people --canonical-type person \
  --with-alias 'aliases.json=Jane Doe:MANUAL'
canon registry diff --old registries/people.v1 --new registries/people --emit summary
```

## Evidence sessions

A session is a work directory bound to a registry. Agents append to it; canon
decides. This is the whole loop, run here on four observations from three
files:

```bash
canon entity session init --goal "dedupe sponsors" \
  --registry registries/sponsors --work-dir work/sponsors \
  --registry-id sponsors --prefix ORG --zero-pad 4

canon entity session ingest work/sponsors observations.jsonl
canon entity session assert work/sponsors evidence.jsonl
canon entity session rules init --out rules.yaml
canon entity session solve  work/sponsors --rules rules.yaml
canon entity session commit work/sponsors --bump patch
```

**Observations** are sightings: one name in one source, with any keys it came
with.

```json
{"obs_id":"filing-a:1","entity_type":"organisation","surface":"Acme Capital, LLC","keys":{"cik":"0001234567"},"source":{"channel":"file","file":"filing-a.csv"}}
{"obs_id":"filing-b:7","entity_type":"organisation","surface":"ACME CAPITAL LLC","keys":{"cik":"0001234567"},"source":{"channel":"file","file":"filing-b.csv"}}
{"obs_id":"filing-c:3","entity_type":"organisation","surface":"Acme Capital","source":{"channel":"file","file":"filing-c.csv"}}
{"obs_id":"filing-c:9","entity_type":"organisation","surface":"Birch Street Partners LP","source":{"channel":"file","file":"filing-c.csv"}}
```

**Evidence** is a typed claim about two observations, with its basis and its
source.

```json
{"type":"name_equivalent","left":"filing-a:1","right":"filing-c:3","basis":{"rule":"casefold, strip punctuation and legal suffix","left_value":"Acme Capital, LLC","right_value":"Acme Capital","steps":["casefold","strip_punctuation","strip_legal_suffix"]},"source":{"channel":"script","id":"names:1"},"asserted_by":"script:names"}
{"type":"same_slot","left":"filing-a:1","right":"filing-c:3","basis":{"rule":"same deal and role","left_value":"deal-12:sponsor","right_value":"deal-12:sponsor","steps":["join:deal_id","join:role"]},"source":{"channel":"agent","id":"warehouse:q7"},"asserted_by":"agent:worker-1"}
```

**Rules** say what counts. The table `rules init` writes accepts a shared
authoritative key (CIK, LEI) or an asserted alias on its own (tier A). Weaker
evidence is accepted only when two different types from two independent sources
agree (tier B). A `distinct` claim is a veto. One piece of weak evidence leaves
the pair for review.

After `solve` and `commit`, the session reports (shortened here):

```text
clusters=2 tier_a=0 tier_b=1 review=0 contradiction=0 standalone=1
outcome=committed minted=1 attached=0 skipped=1 registry_version=0.0.1
```

The two filings that share a CIK, and the third spelling that two independent
pieces of evidence tie to them, became one entity, `ORG-0001`. Birch Street
Partners had no evidence, so it was left alone. The registry now answers by
exact lookup:

```bash
$ canon tape.csv --registry registries/sponsors --column sponsor --emit csv
sponsor,balance,sponsor__canon
ACME CAPITAL LLC,1000000,ORG-0001
Acme Capital,250000,ORG-0001
Birch Street Partners LP,400000,
```

Each alias written by a commit records the session, the rules hash, the
evidence IDs and its tier, so "why are these the same?" has an answer on file.

What else a session gives you:

- **Several writers.** `ingest` and `assert` can be run by many processes at
  once on one session. A writer that loses a race retries; no rows are lost.
- **A work queue.** `frontier` lists what is still unresolved and which kinds of
  evidence would settle each item. `frontier claim` hands a worker its own batch
  with a time limit, and `frontier release` gives it back.
- **Bad rows do not stop the batch.** A malformed line is reported with its line
  number and a fix; the rest are stored.
- **Accretion.** Start the next session on the same registry. Clusters that match
  an existing ID attach new spellings to it instead of minting a duplicate.
- **Memory of decisions.** Vetoes and items left for review are kept beside the
  registry, so a later session does not ask the same question again.
- **Evidence canon derives itself.** `derive` runs canon's name and key operators
  over the observations and appends what they find as evidence.
- **Places as well as names.** Parcel, address and footprint evidence types use
  the same solver.

The other session commands are `status`, `explain`, `next`, `export`, `sample`
and `rules validate`. The
[agent guide](docs/ENTITY_WORKBENCH_AGENT_GUIDE.md) covers each step in detail.

### What has been shown, and what has not

- Shown by a test in this repository: four concurrent workers resolving a
  synthetic set of 252 name families and 40 properties through claim, assert,
  solve and commit, graded against its own ground truth, with no ID spanning two
  true entities.
- Not shown: accuracy on real data, behaviour with model-driven workers, or
  performance at 150,000 observations. The workers in the test are scripted.

## Commands

The core of canon is the lookup and two command groups:

| Group | Commands | What it is |
|-------|---------:|------------|
| `canon <INPUT> --registry <DIR> --column <COL>` | 1 | Exact lookup |
| `canon registry ...` | 18 | Create, change, correct, check and export registries |
| `canon entity session ...` | 16 | Evidence sessions |
| `canon doctor`, `capabilities`, `robot-docs` | 8 | Health and machine-readable self-description |

The released binary also contains older and more specialised groups. They work
and are tested, but they are not where new work should start.

| Group | Commands | Status |
|-------|---------:|--------|
| `canon entity run`, `link`, `block`, `solve`, `audit`, `promote`, ... | 19 | The earlier entity pipeline, driven by profiles and YAML strategies. `session derive` reuses its operators. See [docs/ENTITY_PIPELINE.md](docs/ENTITY_PIPELINE.md). |
| `canon geo ...` | 26 | Geographic association workbench. See [docs/GEO.md](docs/GEO.md). |
| `canon strategy ...` | 10 | Registry of frozen scripts keyed by schema or task |
| `canon inbox ...` | 7 | Triage of values a lookup could not resolve (`--inbox-out`) |
| `canon package ...`, `canon project ...` | 12 | Package registries for distribution; project manifests and locks |
| `canon temporal ...` | 2 | Compile dated assertions into a snapshot |

That is 118 subcommands in the released binary, of which 42 are the core. Every
command, flag, exit code and refusal code is in
[docs/CLI_REFERENCE.md](docs/CLI_REFERENCE.md). Shell recipes are in
[docs/SCRIPTING_EXAMPLES.md](docs/SCRIPTING_EXAMPLES.md).

## For agents

```bash
canon --describe            # the operator.v1 contract: every command, flag, exit code and refusal
canon doctor triage --json  # health, with next actions
canon robot-docs guide      # short guide to the discovery commands
```

Most commands take `--emit json` (or `--json`). Refusals are JSON with a `code`,
a `message` and a `next_command`. Exit codes are the same everywhere: 0 success,
1 a result that needs inspection, 2 refusal or usage error.

For the wider toolchain, see the
[Agent Operator Guide](https://github.com/cmdrvl/.github/blob/main/profile/AGENT_PROMPT.md).
Repository rules for agents are in [AGENTS.md](./AGENTS.md).

## Installation

```bash
brew install cmdrvl/tap/canon
```

From source:

```bash
cargo build --release --all-features
./target/release/canon --help
```

`--all-features` builds what the released binary contains. A plain
`cargo build` leaves out `canon geo` and `canon entity link`. The `geo` feature
adds `canon geo`; the `workbench` feature adds `canon entity link`; `geo` turns
`workbench` on.

## Rust API

Rust callers can use `canon::sdk` for exact batch lookup, CSV mapping, package
verification and read-only registry metadata, without spawning the CLI. The CLI
and the SDK share the same parser, registry loader and lookup engine, and
`tests/sdk_conformance.rs` checks that they produce identical bytes.

```rust
use canon::sdk::{ExactMappingRequest, exact_mapping_artifact};
use std::path::PathBuf;

let artifact = exact_mapping_artifact(ExactMappingRequest::v1(
    PathBuf::from("tape.csv"),
    PathBuf::from("registries/cusip-isin"),
    "cusip",
))?;

assert!(artifact.exit_code <= 1);
```

## Limits

| Limit | Detail |
|-------|--------|
| Lookup is exact | No fuzzy or phonetic matching at lookup time, by design. Variants must be registry entries. |
| Keys are stored normalised | A session stores a CIK without leading zeros, so a lookup by key must use the stored form. `registry export --format key-crosswalk` lists them. |
| Registries are flat | Mapping files sit at the registry root; no subdirectories. |
| `--emit csv` needs CSV input | JSONL, Parquet and XLSX input produce JSON only. |
| Sessions are local | canon reads and writes local files. It does not call a database, an MCP server or any network service; the agent or driver does that and hands canon the rows. |

canon does not ship industry knowledge. What is true about your entities lives
in your registries and your session rules.

## Related tools

[`rvl`](https://github.com/cmdrvl/rvl) explains numeric changes between two
files; canon gives it a stable key to align on.
[`shape`](https://github.com/cmdrvl/shape) checks that two files are
structurally comparable before either runs.

## Further reading

| Document | Covers |
|----------|--------|
| [docs/ENTITY_WORKBENCH_AGENT_GUIDE.md](docs/ENTITY_WORKBENCH_AGENT_GUIDE.md) | Running sessions step by step: evidence types, rules, corrections, places |
| [docs/CLI_REFERENCE.md](docs/CLI_REFERENCE.md) | Every command and flag, exit codes, refusal codes, JSON output |
| [docs/LOOKUP_AND_REGISTRIES.md](docs/LOOKUP_AND_REGISTRIES.md) | Outcomes, registry patterns, providers and output modes in detail |
| [docs/IDENTITY_ARCHITECTURE.md](docs/IDENTITY_ARCHITECTURE.md) | Why lookup is exact and where judgement is allowed |
| [docs/PLAN_CANON.md](docs/PLAN_CANON.md) | The lookup specification |
| [CHANGELOG.md](CHANGELOG.md) | What changed in each release |

## Development

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test                  # unit tests and doctests only
cargo test --all-features   # everything, including tests/ (slow)
```

Targets in `tests/` require the `integration` feature; run one with
`cargo test --features integration --test cli_smoke`. The default build is the
core, so `cargo test --features integration` must stay green without `geo` or
`workbench`; CI runs both profiles.

---

*`canon` is part of the open-source toolchain from the [CMD+RVL](https://cmdrvl.com) lineage and AI enablement practice. MIT-licensed. Contributions welcome from any practice or stack.*
