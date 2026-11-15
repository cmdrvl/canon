# IDs and evidence: what gets built around Canon

> **Status**: Decision record, 2026-11-15. Nothing here is implemented and
> none of it is Canon work. Written at the operator's request so the decisions
> are not lost before they become beads in the right repos.
> **Retire when**: each item below has a bead in the repo that owns it. Replace
> the item with that bead's id as it is filed; delete this file when all have
> moved.
> **Canon's own work** is the bead graph under `bd-vdcw` (evidence sessions).
> This file is only about what sits around it.

## The picture

The Evidence Machine runs bounded batches. Each batch fetches rows, Canon
resolves identity, `decoding` settles values, and the result is committed to a
registry with the evidence that justified it. Once that loop is operating,
three things become possible, and they are what this file records:

1. **IDs** that agents and people can use anywhere, hosted at cmdrvl.com.
2. **Evidence** behind any outcome, sealed and addressable, and later stored
   and indexed in S3.
3. **Agent-facing services** that make the registry easy to use correctly and
   let agents improve it.

## Who owns what

| Piece | Owner |
|---|---|
| Identity decisions, registries, exact lookup, evidence closures for identity | Canon (`bd-vdcw`) |
| Orchestration, bounded batches, fetching through the MCP servers, S3 sync of registries, packing | Evidence Machine |
| Value convergence and its explanation | `decoding` |
| Sealing and pinning | `pack`, `lock`, `hashbytes` |
| Tracing a datum back to its source | Evidence MCP |
| ID pages and registry hosting | cmdrvl.com |
| Data products | the separate sites (dealcharts, privatecredit.watch) |
| Selling evidence | out of scope for this build; a separate future product (see D8) |

## Decisions

### D1. The Evidence Machine is the orchestrator

It works in bounded batches, including a batch about one entity ("do
BlackRock": query for like names, pull a few thousand rows, resolve, commit),
resumed later by adding more. It fetches through the Data MCP; very large pulls
wait for a bulk export. It syncs registry files with S3, which is the source of
truth, and publishes only if the registry is still the version the batch
started from. Canon stays local and offline.

### D2. cmdrvl.com is about the IDs

One URL per ID. The page returns machine-readable JSON: names, keys (CIK, LEI,
FIGI and the rest), kind, status and successor if retired, units and parents,
what the entity is not, and a reference to the evidence. Data stays on the
separate sites and points back to the ID. Canon writes the per-entity JSON; the
site hosts it.

### D3. Agent-friendly IDs

The agent-friendly id will become the primary id (decided). A new registry
mints it as primary from day one. A published registry keeps its ids for now
(`ORG-012` stays), gains the new id as one more key on each entity, and
migrates later: the new id becomes primary and the old one a retired id that
still resolves to it.

- Form: `<prefix>_` plus exactly 12 characters: a 10-character body and two
  check characters. For example `org_7k3m9q2xh43z`, `per_4f9k2m7q8xvj` (both
  carry valid check characters).
- Prefixes: `org_` organisation, `per_` person, `plc_` place, `deal_` deal,
  `fund_` fund, `ins_` instrument.
- Alphabet: `0123456789abcdefghjkmnpqrstvwxyz` (no i, l, o or u). Written
  lowercase, accepted in any case, never purely numeric, nothing mutable
  inside (no name, ticker or date).
- Check characters so a mistyped or invented id fails before lookup. The exact
  algorithm and test vectors are in bead `bd-vdcw.50`.
- Derived deterministically from the founding evidence, so replays and two
  agents minting the same new entity get the same id.
- Optional advisory label, `org_7k3m9q2xh43z~blackrock-inc`: ignored for
  identity, flagged when it does not match. Catches a real id attached to the
  wrong entity.
- Pinned citable form, `<id>@<registry version>#<closure hash>`, which one
  agent can hand to another and which can be checked.

Not measured: that this format reduces agent copying errors. Cheap to test by
having a model copy a few thousand ids in each format.

### D4. Evidence is packed, sealed, stored and indexed

- **Three layers.** The Evidence Machine proves what was fetched; Canon proves
  who something is; `decoding` proves what is true about it. The Evidence MCP
  can trace any fetched datum back to its source, so a pack can run from a
  decision to the bytes of the filing.
- **Canon's closure composes.** It holds Canon's decision and the records it
  decided on, and references upstream artifacts by id and receipt digest.
  Assignments handed to `decoding` carry the closure hash.
- **Sealing** uses `pack` and `lock`.
- **Local first.** Closures and sealed packs are written to a local directory
  to start. Persisting packs to S3 is a later bead, and the S3 layout is not
  decided. The leaning is a dedicated evidence home, not beside the raw
  filing: one outcome often rests on several filings, and packs carry a
  sensitivity. Bucket names already in docs: `cmdrvl-receipt-packs`,
  `cmdrvl-databooks`. Their contents were not checked.
- **Index, later.** One row per outcome: what it is, its subject, closure
  hash, pack id and location, sensitivity, sealed time. Files first, a
  Snowflake table after.
- **Two granularities.** A coarse pack per commit and a fine pack per outcome.
  The per-commit closure lists the per-outcome hashes, so they agree. They
  would be priced separately if evidence is sold.
- **Sensitivity gates sale.** A pack is as sensitive as its most sensitive
  input; an input with no stated sensitivity is not public.

### D5. "How did you know they are both BlackRock?"

A site can state a link and back it. Canon returns the chain of committed
outcomes between two things, each hop labelled identity or relation with its
evidence hash; the packs for those hops are the answer. It never invents a hop.

### D6. Agent-facing services

| Idea | Home |
|---|---|
| Lookup misses become tomorrow's batch | Canon records the miss; the Evidence Machine schedules |
| Every answer says what the entity is not | Canon data; MCP servers and ID pages show it |
| Entity cards sized to a token budget | Canon export; site and MCP serve |
| What-if before asserting evidence | Canon |
| Registry as a repo: sessions as branches, review as a pull request | Canon guide wording |
| ID pattern in tool input schemas | MCP servers, from Canon's manifest |
| Agents contribute evidence, get a ticket, build a track record | a service in front of the Evidence Machine |
| Standing queries: tell me when this name resolves or this id changes | a service over registry versions |

### D7. Registries

People, places and things. One registry per kind, mapped in configuration:
organisations (the existing `firms` registry, `ORG-*` today), people (new:
registry `people`, ids `per_…`), places (Geo), and things by kind (deals,
instruments, funds). Roles are relations, not registries. A group inside a
firm is its own entity with a part-of link. `controlling-class-holders` folds
into organisations: its holder vehicles become units linked to their
sponsors. The existing publish chain stays: export, S3 version, pin, MCP
servers resolve.

### D8. Selling evidence is out of scope for this build

A paid evidence API was discussed as a possible product. It is separate work,
perhaps through the Stripe MCP or x402 so agents can pay on their own. Nothing
in this build depends on it. What this build does is make the evidence exist,
be sealed, and be addressable per outcome.

## Work to file, by home

**Evidence Machine**
- Drive Canon's session commands as plan nodes; review node over Canon's
  review export and import.
- Registry sync with S3 and publish with an expected-base check.
- Bounded batches, including name-like queries; drop rows that already resolve
  before a later batch; act on Canon's frontier and recorded misses.
- Stitch Canon, `decoding` and fetch receipts into one closure per answer and
  seal with `pack`, to a local directory first. `pack seal` needs a fixed
  creation time for a stable pack id and an outcome anchor starting
  `cmdrvl://`; Canon's outcome references are in that form (`bd-vdcw.44`).
- Supply the second source for the first end-to-end test: EDGAR registrant
  names by CIK for the BDC universe (`bd-vdcw.11`), and the sponsor rows as a
  receipted artifact (`bd-vdcw.7`).
- Turn recorded lookup misses (`bd-vdcw.56`) into the next bounded batch.
- Write corrections files from review decisions (`bd-vdcw.52`, `.53`).
- Later: persist sealed packs to S3 and write the index.

**Canon beads that produce what the rest consumes**: registries by kind and
the id scheme `bd-vdcw.50`; commit hand-off and pinned references
`bd-vdcw.55`; evidence closures and link-evidence `bd-vdcw.44`; entity cards
`bd-vdcw.57`; frontier `bd-vdcw.23`; units and role scope `bd-vdcw.54`.
- Decide the bulk-read path for very large pulls (its bead `em-1vp.7`).

**evidence-protocol**
- Publish the identity edge and identity mesh contracts, or retire them in
  favour of Canon's closure and link-evidence shapes.
- A field for the identity evidence a value outcome rests on.

**decoding and the cmdrvl-soi driver**
- Read Canon's assignments file, including the closure hash.
- Identity vocabulary is not needed: identity stays in Canon.

**MCP servers (mcp-server-base and the servers)**
- Treat retired-id aliases as exact only.
- Accept a role scope on resolve.
- Treat agent-friendly id aliases as exact only.
- Return distinct counterparts, units and parents with a resolution, read
  from the `entity_context` table Canon adds to the search index.
- Carry the id pattern in tool schemas.

**Data MCP**
- An export-to-file result mode for large reads.

**cmdrvl.com**
- One URL per id serving Canon's entity card.
- Registry and id documentation for agents.

**Data pipeline**
- Evidence index as a Snowflake table.
- Decide whether the Reg AB Airflow registry sync stays or moves to the
  Evidence Machine.

**cmdrvl-context**
- Update the serving note: S3 is the source of truth, registries per kind,
  people registry, sponsors in scope.

## Open decisions

- S3 layout for packs and the index key (deferred by the operator).
- When published registries migrate to the agent-friendly id as primary.
- Whether the served `firms` registry is renamed once it holds all
  organisations.

## Not verified

The Evidence Machine's sealer and ledger code were not read. The two bucket
names come from docs and could not be listed. Tool behaviour of `pack` and
`lock` is from their help text.
