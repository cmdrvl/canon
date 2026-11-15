# canon entity — Evidence Sessions

> **Status**: Draft for operator review. Nothing here is implemented.
> **Created**: 2026-11-15
> **Context**: `canon entity` was meant to let agents gather evidence about
> entities, build a graph from it, and turn what the graph proves into registry
> mappings, with a human queue for the long tail. A diagnosis on 2026-11-15
> (below) found that the solver half of that exists and the intake half does
> not. This plan describes the missing half.
> **Boundary**: Exact runtime lookup is unchanged. Everything here is
> build-time workbench behaviour under
> [`IDENTITY_ARCHITECTURE.md`](IDENTITY_ARCHITECTURE.md).

---

## Goal (operator statement, 2026-11-15)

One or more agents with a goal and a lot of data (a database, the CMD+RVL MCP
servers, files) run a **session**. In the session they:

1. acquire observations of entities from several sources;
2. gather evidence that two observations are, or are not, the same entity;
3. let deterministic graph rules decide what that evidence proves;
4. create or update registries from what was proved;
5. leave the long tail in a human review queue.

The priority is resolving **keys** (CIK, ticker, LEI and similar) and, on
evidence, spellings of names. Deterministic, not guessing. There is no single
standing graph: each session builds its own, and the registry is what persists.

Reference use case: de-duplicate the CMBS sponsors and guarantors extracted
from Annex A (`WRGL_EDGAR_CMBS_DEALS__FWP_SPONSORS_GUARANTORS_ENTITY`, 150,883
rows; 3,705 distinct sponsor company spellings), acquired through the
`cmdrvl-data` MCP server.

---

## Diagnosis: what was measured

All runs used real data and are reproducible from the commands in the appendix.
Canon 0.14.0 (installed) unless marked; the key test was repeated on 0.15.0
(`fbeb731`) with the same outcome.

| # | Test | Result |
|---|------|--------|
| D1 | Built-in `regab_firm_identity` profile on 13,998 BDC borrower names (54 funds, from the `cmdrvl-soi` gold sets) | Two refusals, then a 2-minute run: 408,891 candidate pairs, **0 entities, 0 review groups** |
| D2 | 40-line normalise-and-group script on the same rows | 1,229 groups spanning more than one fund; 1,074 groups joining different spellings |
| D3 | Same profile with every executed support operator enabled, 3-fund subset (1,264 rows) | 51 groups, none accepted; about 35 correct, about 10 clearly wrong (`Mode Purchaser` / `BGI Purchaser` / `GSV Purchaser`) |
| D4 | 11-row key test: different spellings sharing a CIK or LEI | Linked correctly, including transitively (CIK-only row and LEI-only row joined through a row carrying both). **All ended in escrow** |
| D5 | Same test: two rows agreeing on instrument type, maturity, rate and par, no shared key | No candidate, no evidence |
| D6 | Same test: identical name, different CIK | Grouped in one component with zero cannot-link hits |
| D7 | Sponsor use case through the MCP servers | Stopped before any Canon run: no path from an MCP result to Canon input except through the agent's context |

What these show:

- **The graph mechanics work** (D4). Components, transitive key linking,
  cannot-link and splitting are reusable.
- **Nothing proved becomes a mapping** (D4). The default new-ID policy is
  escrow-only. Acceptance needs a promotion policy plus a frozen labelled audit
  suite, the same gate a name guess faces. The `bdc-issuers` registry in
  `cmdrvl-soi` has had `entry_count: 0` since 2026-03-29.
- **Structural agreement is not evidence** (D5). Attributes exist only as
  vetoes (`attribute_conflict`). `PLAN_STRUCTURAL_RESOLUTION.md` describes
  this as `canon resolve`, "Implemented v0"; that command is not in the CLI.
- **There is no intake for evidence an agent finds.** Input is one row file
  per run. An asserted pair has to be disguised as a shared key column, which
  is what the `cmdrvl-soi` pilot did (`issuer_evidence_key`).
- **Name handling is thin** (D1–D3). The legal-suffix table has seven terms
  (no Inc, Corp, LP). There is no handling of `dba`, co-borrower lists or
  share-class tails. Of 33 operators the profile validator accepts, 9 execute.
- **The front door is hostile** (D1): no command creates an empty registry, one
  unnormalisable row refuses the whole run, and the default budget refuses
  above roughly 1,000 surfaces with the override undocumented.

One further observation shaped the design. A warehouse query for "same deal,
same loan, different filing, different sponsor spelling" returned 33 pairs.
About half are true aliases (`Peregrine Reality Partners` / `Peregrine Realty
Partners`). The rest are loan renumbering between filings, fragments from
upstream name splitting (`LP`, `Jr`), and real relationships that are not
identity (`Simon Property Group` / `Washington Prime Group Inc`). **A single
evidence edge is not identity.** Edges need a type, and acceptance needs
corroboration.

---

## Design

### 1. Session

A session is a work directory holding everything acquired and asserted toward
one goal. It is append-only and content-addressed so a solve can be replayed.

```text
canon entity session init   --goal "dedupe CMBS sponsors" --registry REG --work-dir S
canon entity session ingest  S <observations.jsonl | ->   [--receipt R.json]
canon entity session assert  S <evidence.jsonl | ->
canon entity session solve   S --rules rules.yaml
canon entity session status  S
canon entity session commit  S --next-version X.Y.Z
```

`init` creates the registry if it does not exist. `ingest` and `assert` may be
called any number of times, by more than one agent, in any order. `-` reads
JSONL from stdin.

Open question for the operator: whether the session reuses the `canon project`
workspace substrate, as Geo inquiries do, or is its own directory layout.

### 2. Observations

One JSON object per line. An observation is one sighting of an entity in one
source. Keys and attributes are optional; provenance is not.

```json
{"obs_id": "fwp:0001234567-25-000123:loan7:sponsor:1",
 "entity_type": "org",
 "surface": "Omninet Capital, LLC",
 "keys": {"cik": null, "lei": null},
 "attrs": {"role": "sponsor"},
 "context": {"deal_cik": "1771234", "loan_id": "7", "filing_id": "0001234567-25-000123"},
 "source": {"channel": "cmdrvl-data", "table": "DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_DEALS__FWP_SPONSORS_GUARANTORS_ENTITY",
            "result_digest": "sha256:…", "row": 412}}
```

A malformed or unnormalisable observation is rejected individually and
reported; it does not refuse the batch (fixes the D1 behaviour).

### 3. Evidence

One JSON object per line. An evidence record is a typed, sourced assertion
about two observations (or an observation and a registry entry). Agents, SQL
queries and Canon's own operators all produce the same record.

| Type | Meaning | Can support a merge |
|------|---------|---------------------|
| `same_key` | Both carry the same value in a declared key namespace | Yes |
| `same_slot` | Both fill the same role on the same underlying thing (same loan, same position) per a stated join | Yes, with corroboration |
| `attribute_agreement` | Declared attributes agree within stated tolerance | Yes, with corroboration |
| `name_equivalent` | Names are equal after declared, listed normalisation steps | Yes, with corroboration |
| `asserted_alias` | A cited document states the alias (`dba`, "formerly known as") | Yes |
| `distinct` | Hard cannot-link, with reason | Never; vetoes |
| `related` | Parent, successor, affiliate | Never; recorded as a relation |

Each record carries `basis` (the values compared and the rule applied),
`source` (as for observations) and `asserted_by`. `related` exists so that
Simon / Washington Prime has somewhere to go that is not a merge.

Canon's existing support and cannot-link operators become producers of these
records. The existing `protected_token_conflict` and `anchor_conflict` emit
`distinct`.

### 4. Acquisition

Canon does not call the network during a solve. This follows the Provider SDK
rule ("hidden network access is always a conformance failure") and the Geo
acquisition contract (`bd-1b0g`: "Canon never invokes/imports the executor").

Three paths, cheapest first:

1. **JSONL file or stdin.** Anything that can write the observation shape can
   feed a session. This is the contract; the other two paths produce it.
2. **Acquisition executor.** A script outside the solve
   (`scripts/entity_acquisition/`) takes a recipe (server, SQL or tool call,
   column-to-field map), runs it against an MCP server, pages the full result
   to disk, and writes the observation JSONL plus a receipt holding the query,
   the server's `result_digest` / `verifiable_id` for each page, row counts and
   a completeness flag. The agent runs one command; rows never enter its
   context. `session ingest --receipt` validates digests and row counts.
3. **`registry build` provider.** The OpenFIGI provider is the precedent for a
   networked, maintenance-only materialiser. A `cmdrvl-mcp` provider is the
   right shape only for seeding a registry from an authoritative key table. It
   is not the session acquisition path.

Open question: the `cmdrvl-data` MCP tools cap and page results inside the
tool response. The executor either speaks MCP itself and pages with
`fetch_result`, or the data server grows an export-to-file result mode. The
second is simpler for every consumer and belongs to the data server's repo.

### 5. Rules and acceptance

`solve` builds the graph from all evidence, computes components, and applies a
declared rule table. The table is data, not code, and is hashed into the solve
artifact.

| Tier | Condition on a component | Outcome |
|------|--------------------------|---------|
| Veto | Any `distinct` edge between members | `contradiction`; never accepted |
| A | Every member joined by `same_key` in a namespace the rules mark authoritative, or by `asserted_alias` | Accept |
| B | Members joined by at least two evidence types with different source ancestry | Accept |
| C | Anything else with positive evidence | Review queue, grouped by evidence signature |
| — | No positive evidence | Standalone; listed, not queued |

Notes:

- Tier A removes the D4 failure: a shared verified key is a mapping.
- "Different source ancestry" matters. Two filings of one deal are one origin;
  `decoding` already carries this concept and the session should use the same
  definition rather than invent one.
- A conflicting key inside an exact-name bucket must raise `distinct` (D6).
- Accepted components are written by `commit` as registry entries, with every
  key value and every accepted spelling as aliases, and a version bump. The
  commit records which tier and which evidence accepted each entry, so a later
  audit can sample by tier.
- Human review stays, as an audit of tier A/B samples and as the decision
  maker for tier C. It stops being a gate in front of proved mappings.

### 6. Agent surface

An agent mid-session needs three questions answered without reading artifacts:

```text
canon entity session status  S            # counts by tier, top unresolved by frequency
canon entity session explain S <obs_id>   # its component, the evidence, what would move it up a tier
canon entity session next    S            # evidence that would resolve the most tier-C items
```

`next` is what makes this a loop: it tells the agent which acquisition or
assertion is worth doing, in the same spirit as Geo's next-evidence planning.

---

## Not in this plan

- Probabilistic scoring (Fellegi-Sunter, EM). It conflicts with "not
  guessing". It may later rank the tier-C queue; it never accepts.
- A standing cross-session evidence store. The registry is the memory.
- Changes to exact lookup.
- Name parsing (legal forms, `dba`, co-borrowers, share classes). It is needed
  and is its own bead; `name_equivalent` evidence is only as good as it.

---

## Proof this works

One real loop on the reference use case, re-executed by a verifier other than
the implementer:

1. Acquire all sponsor and guarantor company observations through the executor
   (receipt shows completeness).
2. Assert `same_slot` evidence from the warehouse and `name_equivalent`
   evidence from Canon.
3. Solve; commit; a non-empty `cmbs-sponsors` registry exists.
4. A labelled sample of at least 200 accepted pairs and 200 queued pairs,
   drawn before the solve and labelled by the operator, gives precision of
   tier A and tier B and the share of true aliases left in tier C.

Denominator: all distinct company spellings acquired. Countermetric: false
merges in the accepted sample. A run that accepts nothing is a failure of this
plan, not a safe outcome.

---

## Appendix: reproduction

Inputs and run directories from the diagnosis are retained in the diagnosing
session's scratch directory, which is not durable. To regenerate:

```bash
# D1–D3: borrower names from cmdrvl-soi/gold_sets/*.goldset.v*.json -> rows.csv
#        (columns source_row_id, field_name, org_name, dataset)
canon entity profile init regab_firm_identity --output p.yaml
canon entity run rows.csv --profile p.yaml --strategy p.yaml --registry REG --work-dir wd --emit summary
# raise budgets in the strategy file under `block:` (not `blocking:`):
#   block: {max_candidates_per_surface: 200, max_candidates_per_operator: 5000000, max_candidates_per_run: 5000000}

# D4–D6: the rows below; profile with identity views on cik and lei,
#        exact_view blocking on both, anchor_match support and
#        anchor_conflict cannot-link on both, solve.hard_cannot_link: split_direct.
```

```csv
source_row_id,dataset,org_name,cik,lei,instrument_type,maturity,rate,par
r1,fundA,"Medline Borrower, LP",111,,,,,
r2,fundB,"Mozart Borrower LP (dba Medline)",111,,,,,
r3,fundA,Acme Holdings Inc,,LEI0000000000000001,,,,
r4,fundB,Acme Hldgs,,LEI0000000000000001,,,,
r5,fundA,Zeta Corp,222,,,,,
r6,fundB,Zeta Corporation,,LEI0000000000000002,,,,
r7,fundC,ZC Inc,222,LEI0000000000000002,,,,
r8,fundA,"Project Falcon Buyer, Inc.",,,First lien term loan,2030-06-30,9.75,12500000
r9,fundB,Falcon Parent Holdings (dba SkyWorks),,,First lien term loan,2030-06-30,9.75,12500000
r10,fundA,Apex LLC,333,,,,,
r11,fundB,Apex LLC,444,,,,,
```

These rows are invented for the test; they are not real entities or keys.

Not independently verified: every number above was produced and read by one
agent in one session.
