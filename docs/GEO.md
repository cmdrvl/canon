# canon geo

`canon geo` is built only with the `geo` feature. Released binaries include it; a
plain `cargo build` does not. Moved here from the README.

### Canon Geo: supported geographic associations for agents

Canon Geo's product question is **“Which geographic entity does this evidence
identify, at what grain, and why should I trust that association?”** An agent may
start with property hints, an address to corroborate, or source evidence it has
already collected. Landed national and local datasets available through MCP give
the agent multiple ways to connect names, addresses, identifiers, and geometry.
The agent obtains and pins relevant evidence; Canon's deterministic run stays offline.

The [Cornerstone experiment](../scripts/geo_measurements/fixtures/cornerstone_national_2026-09-17/README.md)
shows this value concretely. Adding supplied Foursquare and Overture evidence
changes 2,983 tied parcel candidates into one preferred parcel association.
At building grain, two buildings tie because the evidence points occupy different
roofs. Those conclusions belong together in the agent's answer: supported parcel
association, building alternatives, and unproven complete property extent.

The target interface presents each claim with its supporting and conflicting
evidence, source lineage and vintage, acceptance policy, and next useful evidence.
Solver exactness and hard-forced identity remain separate guarantees. Current
runs can retain a useful soft-ranked result while reporting `ABSTAINED`; the
experimental national adapter and manually assembled runs do not yet implement
general source-neutral acceptance. Reviewed registry promotion and exact replay
keep their existing boundaries.

The target has two first-class entry points into one inquiry: corroborate a known
address, or discover likely addresses and physical entities from a region plus a few
qualifying attributes. Both are in development; the delivery sequence is the
[agent-complete inquiry roadmap](PLAN_CANON_GEO_AGENT_COMPLETE_INQUIRY.md).

For Geo work, read the [agent operating architecture](CANON_GEO_AGENT_ARCHITECTURE.md)
and then the [mathematical and empirical plan](PLAN_CANON_GEO.md). The operating
order is question/profile/inventory -> bounded tile+halo -> candidate reach -> rho
admission -> incidence components -> exact residuals -> reconciliation -> separately
reported coverage/reach/solver/truth/cost planes. Source instances belong in adapters and
regional inventories; Canon core does not require a named vendor or, architecturally, a
parcel layer.

The Geo planner and target run view reuse Canon's shared project
manifest/lock/plan/run/receipt substrate; they are not a second orchestration engine.
`canon geo plan` now ships as an offline/read-only planner over one validated
`canon.project.plan.v1` DAG. `canon geo run` now ships as the bounded offline run
projection over that DAG. Explicit local bindings supply home-cell rows, the tile-work
request, warehouse rows, and prospective separation/next-evidence inputs. It runs
materialize-home-cells -> tile-work -> materialize-evidence ->
compile-evidence -> propagate -> solve -> explain -> separate-residual -> next-evidence
through the shared project runner and emits `canon_geo_run.v0`
over `canon.project.run.v2` receipts. Without required local inputs, it can still return a
typed `WAITING_FOR_INPUT` run. It resumes validated completed outputs, refuses undeclared
commands or compile/solve input overrides, and treats `--satisfy` as receipt/explicit-byte
consistency validation only, not plan mutation or replanning. To advance inventory from an
already acquired live receipt, `canon geo replan-from-acquisition` validates one receipt
against explicit local artifacts, atomically writes a separate
`canon_geo_regional_inventory_advancement.v0` sidecar, and emits a new
base-inventory-bound `canon_geo_plan.v0`.

`canon geo capabilities --emit json` ships as a deterministic, offline description of the
compiled Geo control contracts. Use `canon geo plan --question <QUESTION.json>
--capabilities <CAPABILITIES.json> --inventory <INVENTORY.json> --profile <PROFILE.json>
--budget <BUDGET.json>` to emit `canon_geo_plan.v0`. The current composition profile is
parcel/building only; candidate reach remains independently unverified unless the inputs
name a reference that proves it.

Start an agent session with `canon geo capabilities --guide`. It emits a separate,
deterministic `canon_geo_agent_guide.v0` JSON envelope containing installed input
schemas (including local reference targets), task guidance and a complete synthetic
descriptive plan → run → inspect → add-evidence example using `answer_scope: current_answer`
and three bindings, with no hypothetical future-action files. It also includes an optional
valid full-workflow extension that reuses the seven shared stages. No repository checkout or
network schema lookup is needed. Write the example's `write_files` values using
its declared JSON encoding, create its listed directories, then invoke its
`steps[].argv` with `canon` from that directory. The example is an
uncalibrated workflow demonstration, not live identity proof. Read the answer output handle
for support, alternatives, preferences, acceptance and further-work state; the ordinary
answer stage does not claim knowledge of its own final run history.

The guide also supplies a synthetic point-to-parcel example. Bind
`canon_geo_point_parcel_request.v0` to `geo.parcel.materialize_evidence:rows`
through `geo run`. Supply the complete bounded parcel inventory, source decimal
point and polygon coordinates, a common frame, point-to-target relationship,
source hashes and transform accuracy (`null` when unknown). Canon admits each
geometry and computes native containment, including holes and multipolygons;
missing or refused geometry remains an explicit unknown. The retained
`canon_geo_point_parcel_relations.v0` observation accounts for every declared
point/parcel pair. Interior points supply one soft preference per parcel and
source family; repeated points do not multiply that family's weight. Boundary,
exterior and unknown results do not eliminate candidates or establish accepted
identity or complete property extent. The warehouse-only materialization leaf
does not accept this input family; use the guide's plan/run/inspect sequence.

The current descriptive profile accepts a bounded inventory and source-backed
name, address, property type, unit count and construction year. Area, stories,
near-road relations and a facts-only `--inquiry` interface remain unavailable.
Do not put square footage in unit count or treat an article's street name as parcel
membership. Acquisition and source interpretation happen outside Canon. The guide
is documentation; obtain the planning capability artifact **without** `--guide`.

Drive Geo from the primary surface: `geo capabilities`, `geo plan`, `geo run`,
`geo replan-from-acquisition`, `geo evaluate`, `geo inspect`, and `geo ledger` today. The eleven stage leaves and three measurement adapters remain
independently callable and machine-described via `canon --describe`, but they are stage and
harness surfaces — reach for them for Demo 0, single-stage debugging, and tests, not as the
normal way to ask Geo a question. See [Geo command surface](CLI_REFERENCE.md#geo-command-surface).

Geo run is not live acquisition or live proof: acquisition remains external, exactness is
representation-relative, candidate reach is upstream, immutable cross-release same-workdir
reuse is not guaranteed, E5/live scale proof is not shipped, `geo inspect` reads stored run
artifacts only, and `geo ledger` currently ships `build`, `card`, `collision`,
`exposure`, and `validate`.
