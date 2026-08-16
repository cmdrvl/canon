# PLAN_CANON_GEO — The Tile as a Compiled Constraint Object

> Status: **proposed architecture**, not yet implemented. Supersedes the resolution
> approach sketched across bd-2kjx and its children. Does not change canon core: runtime
> lookup remains exact registry lookup.
>
> Date: 2026-08-15. Derived from an adversarial multi-model design session (see
> [Provenance](#15-provenance-and-what-is-not-yet-verified) — **the ~50 academic citations
> below have NOT been independently verified**).

---

## 1. The thesis

A tile is a **Waltz scene**.

Roughly 200 noisy local observations of one physical block, from 4–6 sources with no shared
identifier, governed by physical laws that admit only a few globally consistent
interpretations. That is precisely the problem classical constraint reasoning was invented
for:

- Waltz (1972/75) — resolving ambiguous scene labellings from local constraints
- Montanari (1974) — *"Networks of constraints: fundamental properties and applications to
  picture processing"*
- Rosenfeld, Hummel & Zucker (1976) — *"Scene labeling by relaxation operations"*

**The industry attacks this problem with a spatial join and a trigram index.**

We do not ship a point estimate with a score. We ship a compiled object `T` whose
properties are *provable* rather than asserted.

| Property | Mechanism | Authority |
|---|---|---|
| Fixpoint unique regardless of application order | monotone, contracting, correct propagators on a finite lattice | Tarski 1955; Cousot & Cousot 1977; Apt 1999 |
| Compiled form is canonical — same tile, same bytes, any machine | reduced ordered decision diagram, variable order derived from the H3 index | Bryant 1986; Darwiche 2011 (SDD) |
| Adding a source can only refine, never contradict — **and this is checkable** | `Models(T ∧ c) ⊆ Models(T)`, entailment test between last year's diagram and this year's | Darwiche & Marquis 2002 |
| Abstention is the residual, not a threshold | the answer is the model set; a singleton is a decision, a doubleton is an honest doubleton | — |
| Empty model set is a **proof of source defect** with a minimal ordered blame set | MUS via preference-ordered QuickXplain; repairs via hitting-set duality | Reiter 1987; Junker 2004; Liffiton & Sakallah 2008 |
| Every conclusion explainable by naming evidence | minimal environment supporting the conclusion, computed on demand | de Kleer 1986 (ATMS) |
| The whole run is independently machine-checkable | pseudo-Boolean proof log covering global-constraint propagation and symmetry breaking | Gocht, McCreesh & Nordström 2022 (VeriPB) |

---

## 2. Why the previous two architectures failed

Both prior designs picked a channel to **propose** candidates and demoted the other to
**confirm**. Both were red-teamed and destroyed.

**Address-proposes fails.** The parcel layer stores one representative `ADDRESS` per lot,
while large and corner lots legitimately carry many. Measured: geocode `1633 BROADWAY` vs
lot `1657 BROADWAY`; `9 WEST FORDHAM ROAD` vs lot `2167 GRAND CONCOURSE`. The true answer
is frequently **unreachable from the string**, so grounding fails silently and the system
biases toward whichever reading happens to match the stored representative — producing
wrong answers rather than abstentions.

**Geometry-proposes fails.** Interpolated geocodes sit in the roadbed contained by nothing;
one measured case parsed to the wrong street 1.8 km away at ROOFTOP confidence.

**The dilemma was an artifact.** It arose from using noisy evidence in its *unsound
direction* and then needing a second channel to clean up. Fix the direction and the
dilemma dissolves. There is no proposer.

### 2.1 The checksum idea, and why it was demoted

An earlier proposal treated asserted attributes (size, year, count) as **parity bits** on a
parse — pick the reading whose implied physical footprint reconciles. The red team killed
it as a *decider* with an information-theoretic argument:

```
acceptance half-width w ≈ 0.12   (minimum honest; covers the measured NRA/gross gap)
plausible size range 5,000–2,000,000 sf = 400×
distinguishable bins = ln(400)/ln(1.27) ≈ 25
usable information   = log2(25) ≈ 4.6 bits      (3.5 bits at realistic w = 0.25)
```

Other attributes are near-zero **conditional on the tile** — competing readings are lots on
the same block, homogeneous in age and class. Total: **6–9 bits, generously.** Isolating
one reading from 10⁴ needs 13.3.

Worse, the discriminating power is **anti-correlated with case difficulty**: where readings
differ wildly, grounding already killed the bad ones; where they differ subtly, the sums
differ by less than measurement noise. And an error-correcting code asked to correct beyond
its distance does not degrade gracefully — **it confidently miscorrects to the wrong
codeword.**

**Verdict: size is retained as one constraint among many, contributing its ~4 bits. It is
not a decider.** This was an over-promotion, not a wrong idea.

---

## 3. ρ — the soundness discipline

**This is the single most important design rule and it is what makes a hard-constraint
frame survive noisy sources.**

> Every source attribute is admitted to the solver only through a declared, versioned
> relaxation operator **ρ** that maps the raw value to the *weakest constraint the source
> can actually support*.

| Raw evidence | Naive (unsound) reading | ρ-image (sound) |
|---|---|---|
| Geocode `g`, `interpolated` | "the property is at `g`" | "footprint intersects the disc of radius `r=150 m` about `g`" — nearly vacuous, which is **correct** |
| Geocode `g`, `rooftop` | same | `r = 8 m` — sharp, and legitimately so |
| Parcel `ADDRESS = "355 E 12 ST"` | "this lot's address is 355 E 12 St" | "355 E 12 St is *one of* this lot's addresses" — membership, never functional equality |
| Query address `199 First Ave` | "match a lot whose ADDRESS = 199 First Ave" | "*some* member of the collateral set fronts First Avenue at 199" — existential over the set variable |
| `BLDGAREA = 214,300` | "GLA is 214,300" | gross above-grade exact; net rentable `∈ [⌊0.78x⌋, ⌈0.95x⌉]` for `BLDGCLASS ∈ O*`, a separate declared relation |
| `OWNERNAME` equal after normalization | "same owner" | "these lots *may* form an assemblage" — permits, never forbids |
| `OWNERNAME` different | "different owner ⟹ not assembled" | **no constraint at all** |
| FEMA county coverage 92% | unused | `gcc` lower bound: ≥ `⌈0.80·K⌉` slots carry a FEMA observation |

### 3.1 Two consequences

**Every noisy channel is admitted only in its sound direction.** Address evidence never
excludes, it only requires existence. Ownership never separates, it only permits. Geocodes
never locate, they only bound.

**Theorem (trivial, and the whole business).** If every ρ is sound — the true world
satisfies `ρ(v)` whenever the source reports `v` — then the true assignment is in the model
set. Therefore:

> **An empty model set is a proof that at least one source violated its own published error
> model.**

Not "the sources disagree." Not "the join failed." A *proof*, attributable to a minimal set
of source records, that a specific vendor's declared tolerance was breached on a specific
parcel. **That is a falsifiable claim you can put in an email to Overture, FEMA, or a
servicer.**

### 3.2 The band-versus-threshold rule

> **A threshold selects. A band restricts.** A wrong threshold silently produces a wrong
> answer. A wrong band produces an empty model set — a detected, attributable, reportable
> failure. **The system audits its own error models.**

Named price: **wider bands mean larger residuals.** We resolve fewer tiles to a singleton
than a competitor willing to guess. Paid in abstention, which is a first-class output.

---

## 4. Integer geometry

Every coordinate is projected into a **per-tile fixed local integer frame** and snapped to
millimetres in `i64`. Projection constants per H3 cell are precomputed once and shipped as
versioned data, so at decision time projection is a table lookup plus an exact integer
affine map.

**No transcendental function, no floating-point value, and no `f64` comparison appears
anywhere in the decision path.**

Arithmetic: a 1 km tile spans ~10⁶ mm, coordinates ~2×10⁶. Shoelace terms ~4×10¹²; summed
over a 10³-vertex polygon ~4×10¹⁵ — inside `i64`, with `i128` carried for headroom.
Orientation predicates are exact `i128` determinants. **No adaptive-precision filter
(Shewchuk 1997) is needed because we never leave the integers.**

- *Cheap wrong way:* `ST_Contains` in double precision.
- *Silent error:* a footprint straddling a lot line by 3 cm goes to lot A on x86, lot B on
  ARM, and a third answer after a GEOS point release. In 40,000 loans that is a handful of
  silently different answers per rerun with no detection mechanism.
- *What exact buys:* byte-identity across platforms and decades by construction.

---

## 5. The variable model

Canonical total order `≺` on all features: `(source_rank, source_native_id_bytes)`, with
`source_rank` from a versioned table. Variable order, diagram order, report order and
tie-breaks all derive from `≺`. **No hash-map iteration in any order-sensitive path.**

**Latent layer.** Parcels `P` are given (~25/tile); geometry is survey ground truth,
attributes go through ρ. Latent buildings `B = {b₁…b_K}`, `K = Σ_p NUMBLDGS(p)` where
present, else per-component max footprint count across sources, plus `⌈0.2K⌉` slack slots
under an `atmost`. `K ≈ 60–80`.

```
X_f  ∈ B ∪ {⊥}       observed footprint → slot   (Overture, FEMA, MS)   ~180
Y_q  ∈ B ∪ {⊥}       POI → slot                                          ~40
Pb_b ∈ P ∪ {∅}       slot → parcel                                       ~80
A_b  ∈ [a_lo,a_hi]   integer footprint area, whole sq ft                 ~80
Fl_b ∈ [1,120]       floor count                                         ~80
Lo_ℓ,Hi_ℓ ∈ ℤ        address-range endpoints per lot per block face      ~50
Coll ⊆ P             collateral parcel set        (ROBDD set variable)
QB   ⊆ B             collateral building set      (ROBDD set variable)
```

`n_fd ≈ 260` finite-domain, `n_int ≈ 210` integer. `d_max = K ≈ 80` before geometric
filtering; `d_typ ≈ 8` after.

### 5.1 Symmetry must be broken completely and soundly

Slots `b₁…b_K` are interchangeable. `K!` symmetry **destroys model counting outright** —
every solution appears `K!` times. Two mechanisms channelled together (Cheng, Choi, Lee &
Wu 1999):

- **Representative encoding** for canonicity — a latent building is identified with the
  `≺`-least observation in its cluster. No anonymous slots, weak propagation.
- **Slot encoding** for the strong global propagators, with **value precedence**
  (`precede`; Law & Lee 2004; Walsh 2006) breaking value interchangeability completely at
  GAC in O(nd).

- *Cheap wrong way:* cluster, then sort clusters and call them 1..k.
- *Silent error:* the count is wrong by `K!/orbit size`, so "3 candidates" and "3 million
  relabellings of 1 candidate" are indistinguishable and every ambiguity measure is noise.

---

## 6. The consistency ladder, with arithmetic

**The ceiling is not a consistency level.** Régin's `alldifferent` GAC computes strongly
connected components of the value graph (Tarjan 1972) as an intrinsic step — **those SCCs
*are* the tile's decomposition, handed to us free.** No separate tree-decomposition
heuristic. Typical component after geometric filtering: **6–20 variables, d ≤ 8**, tail to
~40 on a dense assemblage.

At that size, **exact compilation of the entire solution set is cheaper than path
consistency on the tile, and subsumes k-consistency for every k simultaneously.** The
crossover is at **k = 3**.

```
NC  →  AC-2001 + GAC on globals   ≈ 10 ms   tile-wide
    →  SAC                        ≈ 0.3 s   tile-wide   ← the level that earns its keep
    →  decompose                  free      from Régin/Tarjan
    →  exact MDD/SDD per component ≈ 0.2 s  ← subsumes all k-consistency at once
    →  PC on components           ≈ 50 ms   ← explanation artifact, NOT pruning
```

**Tile budget ≈ 0.5 s.** A spatial join is ≈ 1 ms.

> **We spend 500×, and that is the entire commercial thesis.**

At 10⁶ tiles: ~140 CPU-hours, embarrassingly parallel — **a few hundred dollars of compute
for a national pass.**

### 6.1 What each level buys that the level below cannot

**AC over pairwise `≠` cannot see Hall sets.** Six MS GlobalML footprints, five
geometrically admissible slots. Pairwise disequality with AC finds nothing — every value
still has support. Régin's GAC finds the wipeout immediately, because Hall's theorem (1935)
violations are exactly what the SCC decomposition detects. **This is a proof that MS
over-segmented a roof ridge, emitted for free.**

**SAC buys eliminations requiring an assignment plus a numeric constraint.** Assume the
collateral is lot A. Propagate. The knapsack propagator on `Σ A_b · Fl_b` cannot reach the
asserted 214,300 sf even using every compatible footprint at maximum plausible floor count
→ wipeout → **lot A eliminated with no threshold and no search.** Plain AC never sees this
because the sum is violated only *in combination with* the assignment.

**PC and SAC are incomparable as domain filters** (Debruyne & Bessière, JAIR 14, 2001; the
lattice is `AC ≺ RPC ≺ maxRPC ≺ SAC` with PC orthogonal as a *relation*-filtering
consistency). PC's distinctive product is the pairwise relation itself — *"if the
collateral is lot A then the FEMA structure must be `f3`"* — which SAC can never represent.

**Therefore PC is demoted from pruning to explanation.** Post-decomposition, post-SAC,
PC-2001 on components approximates Montanari's (1974) **minimal network**: the network
whose binary relations are exactly the projections of the solution set. The human-readable
pairwise summary of the residual. Run for the report, not the answer.

**Strong k-consistency for k ≥ 4 is affordable on components and worthless there** — for
the same ~5 s you can compile the component exactly and get every k at once plus the count
plus the backbone. **Ranked to zero.**

**Freuder's 1982 theorem gives a per-tile certificate.** If a component's constraint graph
has width `w` under the canonical ordering and the network is strongly `(w+1)`-consistent,
search is backtrack-free — the propagation fixpoint *is* the solution set. Compute `w` per
component and report it. A tile carries the line *"solved backtrack-free at width 2"*,
which is a mathematical statement about that tile, not a QA note.

---

## 7. The global constraint catalogue

Domain rules that look like generic pairwise checks are instances of **named global
constraints with polynomial domain-consistent propagators.** Hand-coding them as pairwise
checks discards decades of work and prunes far worse.

| Domain rule | Global constraint | Algorithm / authority |
|---|---|---|
| Within-source exclusivity (two Overture buildings are never the same building) | `alldifferent` / `alldifferent_except_0` | Régin 1994, via Hall's theorem + Tarjan SCC |
| Cardinality priors (`NUMBLDGS`, source coverage rates) | `gcc` (global cardinality) | flow-based |
| Distinct building count | `nvalue` / `atmost_nvalue` | — |
| Additive area — **not** "sum with tolerance" | `knapsack` / `bin_packing` | subset-sum DP with dedicated propagator |
| Parcels do not overlap | `diffn` / `geost` | Beldiceanu et al. |
| Address along a block face | `disjunctive` scheduling on the house-number axis | — |
| **Address string parsing** | `regular` | Pesant, CP 2004 — GAC by DFA unfolding, O(n·\|Q\|·\|Σ\|) |
| Temporal feasibility | Allen interval algebra / STP | Allen 1983; ORD-Horn tractable subclass (Nebel & Bürckert 1995); STP by Floyd–Warshall (Dechter, Meiri & Pearl 1991) |
| Ownership equivalence | `nvalue`, `among`, equivalence constraints | — |
| **Identifier namespaces** | functional dependencies + **congruence closure** | Nelson–Oppen 1979; union-find with proof forests |
| Containment (building on parcel, POI in building) | `inverse`, channelling, b-matching | — |
| Set variables (assemblages, `Coll`, `QB`) | ROBDD set domains | — |
| Slot symmetry / ordering | `precede`, `lex_chain` | Law & Lee 2004; Walsh 2006 |

### 7.1 Three that deserve calling out

**`regular` puts the address grammar inside the solver.** The naive way is libpostal — a
CRF, therefore statistical, therefore nondeterministic across versions and uninterpretable
— which **picks one parse**. Silent error: `"199 First Avenue, Unit 3B, a/k/a 355 East 12th
Street"` gets one parse, the `a/k/a` is discarded, and the true answer is destroyed before
the solver runs. With `regular` over a declared versioned token grammar, **all parses stay
alive as a domain** and the other constraints kill the wrong ones. Alternation handles
`a/k/a` natively. **This removes the last statistical component from the decision path.**

**Allen's interval algebra finds demolitions.** MS footprint from 2021 imagery, FEMA
structure from 2019, parcel `YEARBUILT` 2020. A spatial join merges all three into one
building. The temporal network **proves** the 2019 FEMA record cannot denote the same
physical structure — so the tile contains a demolition-and-rebuild event, meaning **the
collateral described in the 2019 offering document no longer exists.** A five-alarm CMBS
finding, falling out of a 1983 paper. *Cheap wrong way:* `WHERE year_built <= 2019` — it
filters rows instead of detecting events, so the rebuild is invisible.

**Congruence closure makes identity conflicts proofs.** Maintain equivalence classes of
entity variables and identifier literals; every union records the named evidence
responsible; **every attempted union with an incompatible namespace id produces a conflict
proof.** Inverse-Ackermann per operation. *Cheap wrong way:* coalesce ids after choosing a
parcel — the conflict is discovered too late or silently overwritten. **A conflict is a
proof, not an exception log.**

---

## 8. Explanations as a byproduct

Three candidate paradigms with different cost profiles:

**(a) ATMS** (de Kleer 1986; + GDE, de Kleer & Williams 1987). Every derived datum carries
a label: the minimal environments under which it holds. Explanation *is* the data
structure. **Honest cost:** labels are antichains and can blow up exponentially — with ~200
source records as assumptions per tile this is a real risk. **Do not run a full ATMS
eagerly.**

**(b) QuickXplain** (Junker, AAAI 2004). Preferred minimal explanation on demand in
**O(k log(n/k))** consistency checks. At n ≈ 60 tile constraints, k ≈ 3: `3·log₂(20) ≈ 13`
solver calls × ~10 ms = **~130 ms per explanation, paid only when an operator clicks.**
Fully deterministic given a fixed constraint order, which the source-reliability ordering
supplies. **This is the right engineering answer.**

**(c) Lazy Clause Generation** (Ohrimenko, Stuckey & Codish 2009) — propagators explain
themselves in clauses; the resolution derivation is the proof. Certified with **VeriPB**
(Gocht, McCreesh & Nordström, CP 2022), which can certify global-constraint propagation
*and* symmetry breaking, which a naive DRAT log cannot.

### 8.1 The committed architecture

- **Answer layer:** compile. No search → no learned clauses → no order-dependence at all.
- **Explanation layer:** QuickXplain on demand, ordered by declared source reliability.
  Artifact is a minimal set of named source records: *"lots 1012920026 and 1012920001 are
  separated by exactly {FEMA `f3` SQMETERS = 3,240; MapPLUTO `NUMBLDGS` = 2; the First
  Avenue block-face anchor at 195}."* Templates to prose because every constraint carries
  provenance by construction.
- **Certificate layer:** VeriPB proof log for the full run, **independently checkable by a
  third party who does not trust our code.**

### 8.2 The determinism precondition people skip

**Confluence requires every propagator to be a monotone function of the domain store.** Any
propagator using randomised rounding, sampling, or an early-exit budget is non-monotone and
**voids the theorem.** That is a hard rule on the propagator library, not advice.

Where we search rather than compile (components exceeding the width budget), byte-identical
*proofs* additionally require: canonical branching order from `≺`; restarts driven by a
deterministic counter, never wall clock; no PRNG without a fixed seed; no propagator reading
external mutable state. **A single `HashMap` iteration in a propagator silently destroys the
guarantee.**

---

## 9. Solver-native artifacts — the actual product

Compiling to **d-DNNF / SDD / reduced MDD** (Darwiche 2001, 2011; Darwiche & Marquis 2002;
Andersen, Hadžić, Hooker & Tiedemann 2007; Bergman, Cire, van Hoeve & Hooker 2016) makes
all of the following linear or polynomial in diagram size.

| Artifact | Computation | Operator product |
|---|---|---|
| **Backbone** — values in every solution | one traversal | *"Regardless of how the ambiguity resolves, this loan touches BBL 1012920026, GERS `08f2a3…`, and total collateral GLA ≥ 412,000 sf."* **Lets a downstream system act on partial resolution.** |
| **Exact model count** | one bottom-up pass | A *calibration-free* ambiguity measure. Not a confidence score — a count. 1 = decided, 3 = three named alternatives, 0 = proof of source defect. |
| **Residual enumeration** | polynomial delay | The full alternative set, streamable. Uno (1997) gives O(V) delay per matching; exact counts via Ryser (1963) at O(2ⁿn) — n=12 → 4.9×10⁴ ops, free. *#P-complete in general (Valiant 1979); intractable at scale, free at 200.* |
| **MUS** — minimal blame | QuickXplain | *"These five sources cannot all be right, here is the smallest set that proves it, ordered so the least-trusted source is named first."* |
| **MCS** — minimal repair | hitting sets of MUSes (Reiter 1987); enumeration via CAMUS (Liffiton & Sakallah 2008) or MARCO (Liffiton et al. 2016) | *"Retract either {FEMA `f3` SQMETERS} or {MapPLUTO `NUMBLDGS`} and the tile becomes consistent. Nothing smaller works."* **A repair recommendation, not an error message.** |
| **Value of information** | count reduction under each hypothetical new fact | *"Buy the certificate-of-occupancy date from this vendor and 61% of your residual ambiguity across the portfolio collapses."* Exact, because counting is exact. |
| **Minimal network** (Montanari 1974) | PC on the residual component | *"If lot A then FEMA `f3`; if lot B then FEMA `f7` and the POI is a tenant not the owner."* |
| **Certified refinement** | entailment between last year's diagram and this year's — polytime on SDDs sharing a vtree | *"Here is a machine-checkable proof that our 2027 answer refines our 2026 answer and contradicts nothing."* |

### 9.1 The committed ranking

**Contractual output, build first: the pair (backbone, exact count).** Nearly free once the
compiler exists, and it is what goes in the SLA. **It converts abstention from a failure
into a deliverable** — a consumer can safely act on the backbone and safely defer on the
residual, both precisely.

**Highest-margin single artifact: the ordered MCS lattice.** Backbone can be *approximated*
— a competitor with a good probabilistic model can produce a "high confidence subset" that
is usually right, and usually-right sells. **MCS has no approximation.** There is no
statistical proxy for "the minimal set of retractions that restores consistency." It is
also the only artifact with a buyer *other than* the person who asked the question — the
data vendor, the trustee, the risk committee — and **the only one that improves the input
corpus rather than consuming it, so it compounds.**

**Compounding moat: value of information.** Once counting is exact, VoI is exact, and exact
VoI turns data acquisition from a procurement guess into an optimisation. This is the thing
that makes the corpus asymmetric over three years, and it directly answers "which dataset
do we buy next" from real residuals rather than intuition.

**Regulatory: certified refinement.** In CMBS specifically, *"we can hand the trustee a
proof that our restatement is a refinement and not a revision"* is worth more than it
sounds.

---

## 10. Where the frame breaks — answered with a theorem

**Semiring-based CSP** (Bistarelli, Montanari & Rossi, JACM 44(2), 1997):

> **Soft constraint propagation is confluent and reaches a unique fixpoint iff the
> semiring's combination operator × is idempotent (a × a = a).**

- **Fuzzy / possibilistic** — `⟨[0,1], max, min⟩`. `min` is idempotent. **Confluent. Safe.**
- **Weighted** — `⟨ℕ∪{∞}, min, +⟩`. `+` is not idempotent. Soft arc consistency (Cooper &
  Schiex 2004; Larrosa & Schiex 2004) requires equivalence-preserving transformations and
  **the fixpoint depends on the order they are applied.**

**So "can we just add reliability weights?" is answered no, and here is the paper.**

### 10.1 Where softness lives instead — three places, none of them the solver

1. **In ρ** — declared, versioned, falsifiable bands (§3). Gross-vs-NRA is two hard
   relations plus a band `[0.78, 0.95]` for office, with a version number and a citation.
2. **In presentation ranking** — genuine preferences applied to the **already-enumerated
   finite residual**, as a sort with canonical total order and tie-breaking. Sorting a
   finite enumerated set is confluent by construction. **The solver never sees the
   preference.**
3. **Reliability, which is not a weight** — it sets the *width* of a source's ρ band and
   supplies the *preference order* handed to QuickXplain. **Reliability never weights a
   decision. It widens a band and orders a report.**

> **Rule: preferences rank; constraints prune. Never mix.**

### 10.2 The claim-class stratification

If valued/semiring CSP *is* used, it remains deterministic given exact costs and
tie-breaks — but **adding a soft constraint can change the optimum**, so the "knowledge only
tightens" guarantee does **not** extend to preferred answers. Output must therefore
separate:

```
HARD_FORCED     true in every hard-feasible model
SOFT_PREFERRED  true in every minimum-cost model under declared policy
SOFT_RANKED     ranked alternatives, not facts
```

**Never promote `SOFT_PREFERRED` as a canonical identity fact** unless the product contract
explicitly allows policy-dependent identity.

> **Softness does not destroy determinism. It destroys the right to call the optimum "the
> truth." Keep those separate and the architecture remains honest.**

### 10.3 When hard constraints conflict

1. Emit the MUS or a small irreducible conflict.
2. Compute MCS / minimum-cost repair **as diagnosis only**.
3. **Do not return a resolved identity.**

If the conflict involves constraints that should have been soft, **the fix is not to weaken
the solver. The fix is to reclassify the evidence contract.**

**Fallback is not fuzzy matching. It is a lower claim class:** hard residual unresolved;
soft ranking available; minimal repairs available; human review target available.

---

## 11. What this supersedes in the existing geo epic

| Bead | Status under this plan |
|---|---|
| bd-2cbs entity-level model | **Retained and strengthened** — levels become typed variables and channelling constraints |
| bd-16r1 geometry typed value | **Retained** — the per-tile integer frame is exactly this, now with an arithmetic bound |
| bd-3nc7 predicate regime | **Resolved** — integers in a tile-local frame; no adaptive-precision filter needed |
| bd-15ba exact predicates | **Demoted** — Shewchuk becomes a fallback, not the bar |
| bd-2zdz assemblage subset selection | **Superseded** — becomes `knapsack`/`bin_packing` + set variables, not bespoke interval enumeration |
| bd-786w coverage abstention | **Superseded** — abstention is the residual model set; reason codes become MUS/MCS output |
| bd-272d attribute anchoring | **Retained as a constraint**, demoted from decider (see §2.1) |
| bd-1a12 geocode plausibility | **Retained** — becomes ρ radius selection plus an empty-model-set proof |
| bd-1uje / bd-3d8p / bd-1c96 / bd-3h2p / bd-3ul7 ambition lane | **Mostly superseded** — assignment and clustering become global constraints with exact propagators; revisit each against §7 |
| bd-101v visual evidence card | **Retained and easier** — minimal network + MUS are the card's content |
| bd-tccn worked-case corpus | **Retained, now the validation harness** for the propagator library |
| bd-35qg address-set source | **Elevated** — the red team's central recommendation: much of the machinery exists to compensate for a missing address-point layer |

---

## 12. The acquisition finding the red team surfaced

> *"Most of the parse forest exists to compensate for a missing data source. You do not
> have an address-point layer, and you need one."*

NYC PAD / Geosupport contains every legal address per lot including all frontages and
a/k/a's, encodes Queens grid semantics correctly, and already knows that 9 West Fordham
Road and 2167 Grand Concourse are the same lot. Deterministic, integer-keyed, explainable,
no model in it, maintained by the jurisdiction that *defines* the answer.

With it, several hard problems collapse to lookup rather than enumeration. Outside NYC the
analogue is the county address-point file or the National Address Database. **Imperfect
coverage is fine — imperfect coverage produces honest abstentions.**

---

## 13. Cost model and the commercial thesis

```
per tile        ≈ 0.5 s        vs ≈ 1 ms for a spatial join      → 500×
national pass   ≈ 140 CPU-hours at 10⁶ tiles, embarrassingly parallel
                ≈ a few hundred dollars of compute
```

**The moat is not the data.** Overture, FEMA, Microsoft footprints and county parcel data
are public. The moat is being willing to spend 500× per tile running exact combinatorial
methods, because the tile bounds the problem to ~200 nodes and turns globally intractable
techniques into free ones.

Nobody in commercial real estate knows these techniques exist, and nobody who knows these
techniques has looked at a rent roll.

---

## 14. Open questions and risks

1. **Propagator library scope.** How many of §7's global constraints must be implemented
   before the first useful tile? Which have usable Rust implementations and which are new
   work?
2. **Compilation blowup.** Component sizes are *estimated* at 6–20 variables after
   geometric filtering. **This has not been measured on real tiles.** If dense Manhattan
   components run to 60+, exact compilation may not fit the budget. **Measure before
   committing.**
3. **ρ band calibration.** Every band is a modelling commitment. Where do the numbers come
   from, who owns them, and how are they falsified? The 0.78–0.95 office NRA band is
   illustrative, not derived.
4. **VeriPB practicality.** Proof logs for a national pass may be enormous. What is
   actually emitted, retained, and for how long?
5. **Search fallback.** Components exceeding the width budget need search, which
   reintroduces branching-order determinism requirements. How often does this happen?
6. **Does the ROBDD set-variable representation hold** at realistic assemblage sizes?
7. **Interaction with the BYOP boundary.** Layers 0–4 are a pure function of the tile and
   cacheable; where exactly does the compiled object sit relative to client data?

---

## 15. Provenance, and what is NOT yet verified

This plan was produced by an adversarial multi-model design session on 2026-08-14/15:

1. A cross-domain technique search (`WIZARD_IDEAS_CC.md`, `WIZARD_IDEAS_COD.md`)
2. An identifier-authority ambition round (`WIZARD_AMBITION_COD.md`)
3. Cross-model adversarial scoring (`WIZARD_SCORES_*.md`)
4. A red team that **destroyed two prior architectures** (`REDTEAM_CC.md`)
5. This constraint-object round (`CSP_CC.md`, `CSP_COD.md`), where two models converged
   independently on the same formal object

**Independent convergence between two different model families on the same formalism,
global constraints, and artifact set is the strongest evidence this plan carries.**

### What is NOT verified

- **The ~50 academic citations above have not been independently checked.** Authors, dates,
  titles, complexities and theorem statements are as reported by the models. **Verify
  before citing externally or committing engineering to a claimed complexity.**
- All performance numbers (0.5 s/tile, 140 CPU-hours, component sizes, propagation costs)
  are **estimates from the analysis, not measurements.**
- The information-theoretic checksum argument in §2.1 is internally consistent but its
  inputs (400× size range, 10–20% NRA gap) are drawn from a small measured sample.
- Whether usable Rust implementations exist for the named global constraints is **unknown**.

### Session lesson encoded here

Three claims during this session came from model prose rather than returned values, and all
three were wrong. **Take literal values, record the query, verify citations before relying
on them.** This document is a design to be validated, not a set of established facts.

---

## 16. The resolution task, operationally

Added 2026-08-16, operator-approved, after the Appendix K review exposed the gap: the plan
specified the mathematics (§4–§10) and the admission discipline (§3) but never the
operational middle layer. This section closes it.

### 16.1 The query

Input: one CMBS property record — its address string(s) (possibly multi-address, ranges,
a/k/a), its geocode(s) with accuracy tier, its asserted attributes (SF, units, year built
from Annex A / the loan documents), and its loan identity (for document evidence).

Output: the **collateral parcel set** `Coll` and **building set** `QB`, delivered in the
§10.2 claim classes — `HARD_FORCED` facts (backbone), the exact residual model set with
its count, `SOFT_RANKED` alternatives where policy allows, or a typed refusal. A residual
of size >1 is a deliverable, not a failure (§9.1). Case 6's shape is normative: parcel
singleton, building doubleton, both stated.

### 16.2 Candidate enumeration

Candidates are never proposed by a channel (§2: there is no proposer). The candidate
universe is the tile: all parcels in the work unit, decomposed into components by the
canonical geometric predicate (Appendix F/D). `Coll` candidates are subsets of component
parcels, pruned by hard constraints — the knapsack over asserted SF, adjacency,
ownership-permits (never forbids), document-asserted BBL sets. Enumerate within
components (measured sizes: 2–5 typical, parcel-stars to ~71 per Appendix F); the residual
is whatever survives propagation.

### 16.3 The evidence inventory

Every evidence class, its landed NYC instance, its generic analogue, its ρ, what it feeds,
and its measured state. **The class is the architecture; the instance is data onboarding
(Appendix A.6). Canon core never special-cases an instance.** Rows marked UNMEASURED are
open work, not established capability.

| # | Evidence class | NYC instance | Generic analogue | ρ — sound reading | Feeds | State |
|---|---|---|---|---|---|---|
| 1 | Geocode point + tier | `WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED` | any geocoder | disc of tier-dependent radius; never a location, only a bound. `nearest_rooftop_match` is the measured silent-error tier (E.2) | position constraint on candidates | MEASURED: 66% precision alone (H.6) |
| 2 | Address strings (query side) | `PROPERTY_ADDRESS` + parsed fields | any | existential membership — "some member of `Coll` fronts street S near number N"; parses held as a domain via `regular` (§7.1); never exclusion | `Lo/Hi` range vars, membership | MEASURED: 28.89% exact-fire; string normalization is representation-bound (K.2) |
| 3 | Address sets (lot side) | **NOT LANDED — NYC PAD (curves bd-28kn)** | county address points, National Address Database, OpenAddresses | the lot's full legal address set; membership tests against it | address-set membership | UNMEASURED; 3 worked-case receipts (I) |
| 4 | Parcel geometry | `NYC_DCP_MAPPLUTO_HOT` 26v1 | county parcels / Regrid | survey substrate; exact integer predicates (§4) | candidate universe, area-majority anchor | MEASURED (D/F) |
| 5 | Footprints, multi-source | NYC footprints; FEMA structures; MS GlobalML (NY landed); Overture (**0 NY rows — gap**) | same, national | area-majority assignment with geometric denominators only (F); within-source `alldifferent`; cross-source counts via `gcc` | `X_f`, `Pb_b` slots | MEASURED: forest holds 3-source (F); 55% exact count agreement (F.4) |
| 6 | Asserted attributes | Annex A / `PROPERTY_MART` SF, units, year | every deal tape | declared bands (§3: e.g. NRA ∈ [0.78,0.95]·gross for office); `knapsack` over candidate `Coll` | `A_b`, `Fl_b`, knapsack | **LANDED, NEVER JOINED INTO ANY TEST — the first unrun test in §17** |
| 7 | Parcel attributes | `BLDGAREA`, `NUMBLDGS`, units | assessor rolls | observations to check, never denominators (F) | `gcc` counts, area bands | PARTIALLY MEASURED (F.4) |
| 8 | Document evidence | `NYC_ACRIS_*` external tables | county recorder / title plants | recorded collateral BBL sets bound by amount+date+lender with contamination filters (H) | direct `Coll` evidence; also ground truth | MEASURED as truth instrument (H); unused as solver evidence |
| 9 | Imagery / elevation observers | none landed; verified catalog in J (NYS/NYC ortho first, 3DEP, NAIP, NOAA event) | national per J | frozen-weight observers emit typed counts/outlines/floors with characterized regional error (A.2–A.3) | `gcc` checks, an own footprint source, change events | UNMEASURED |
| 10 | POI / tenant | not landed | — | existential presence; tenant ≠ owner | `Y_q` | UNMEASURED |
| 11 | Ownership | `OWNERNAME` | — | permits assemblage, never forbids (§3) | `Coll` permitting | ~0 bits alone by design |
| 12 | Temporal / vintage | per-source dates, `YEARBUILT`, document dates, imagery vintage | — | Allen/STP intervals (§7.1) | demolition/rebuild events | UNMEASURED |

Per §2.1, **every row is worth a few bits at most, by design.** The architecture's claim
is that the bits sum under joint propagation over the candidate set. No row is a decider;
no row should ever be tested as one.

## 17. Evaluation protocol

Added 2026-08-16, operator-approved. The standing rule this plan lacked, stated first:

> **Constraints are never evaluated as unary accept/refute rules. The unit of measurement
> is the joint residual over the candidate set.** Individual-signal saturation and
> false-refutation are expected consequences of §2.1's few-bits premise — measuring them
> (Appendix K) characterizes inputs; it neither confirms nor refutes the architecture.

The dispositive ladder — each stage gates the next, all numbers from structured results
with the query recorded, denominators predeclared, scored against the H.6 baselines on the
coverage/precision plane:

- **E1 — Failure taxonomy.** Classify the 79 labeled Gate V2 failures by cause:
  adjacent-lot near-miss (with distance to true lot), gross geocode error (W 49th class),
  assemblage scoring artifact, condo representation residue, residual truth contamination.
  Bounds achievable headroom per evidence class before anything is built.
- **E2 — Attribute-channel reconciliation.** Join Annex A asserted SF/units/year to the
  labeled set; measure band-consistency of asserted SF against the PIP lot vs the true
  lot. First exercise of inventory row 6.
- **E3 — Pairwise candidate test (the solver stand-in).** For each labeled failure, score
  the true lot against the PIP lot under all landed inventory rows jointly. The headroom
  number is: how often does joint evidence rank the true lot strictly above the wrong
  one? This is the architecture's claim in measurable form.
- **E4 — Joint mini-propagation.** Run the actual constraint set (§5–§7) over the six
  corpus cases (I) and the labeled set: residual sizes, backbone accuracy, abstention
  honesty, per-component cost against Appendix G's sizing.
- **E5 — Genericity gate.** Repeat E1–E4 in one non-NYC county using only generic
  instances (county parcels, NAD/county address points, NAIP or state ortho). No
  NYC-specific code path may be load-bearing.

Kill condition, stated honestly: if E3 — with the attribute channel joined and an
address-set layer landed — cannot rank the true lot above the wrong one on the majority of
E1's addressable failure classes, then the bits do not sum, the 500× premise of §13 fails,
and the architecture survives only as an honest-abstention engine, which is not the
product. That is the falsifiable form of "could this possibly work."

---

# Appendix A — Frozen-weight observers and imagery sources

Added 2026-08-15 after operator correction. **This appendix is an open extension point, not
a commitment.** It records what the architecture does and does not forbid, and exactly what
admission requires.

## A.1 The correction: neural networks are not banned. Nondeterminism is.

The main plan's constraint is byte-identical reproducibility, not an aversion to models. A
model with **frozen, hashed, versioned weights, implemented in pure Rust with controlled
arithmetic** — no platform-variable BLAS, no nondeterministic kernels, no sampling — is
byte-identical forever and satisfies the determinism requirement exactly as well as an
integer orientation predicate does.

CMD+RVL has shipped this pattern three times: **cmdrvl-tabfm**, **FrankenWhisper**, and
**FrankenOCR** (with the `focr` skill on the operator's machine). The
`ai-model-into-rust-mega-fused-hyper-kernel` skill states the target explicitly as
**bit-identical model parity**.

A frozen model's version therefore becomes another pinned input in the artifact chain,
exactly like the registry version, the strategy hash or the tile digest.

## A.2 The rule that keeps it sound: a model is a SOURCE, not a SOLVER

canon's explainability rule — *if you cannot explain a match by pointing at assertion
scores, it does not ship* — constrains **what the model outputs**, not whether one exists.

| Model output | Admissible? |
|---|---|
| "probably the right building, 0.87" | **No.** Opaque scalar, not decomposable, decides nothing legibly |
| "3 structures inside this polygon" | **Yes** — a count, checkable against `NUMBLDGS`, enters as a constraint |
| "roof outline at these coordinates" | **Yes** — a geometry, enters ρ like any other observed footprint |
| "12 floors from facade or shadow" | **Yes** — a number, checkable against `NUMFLOORS` |

> **The model observes. The constraint system decides.** A frozen-weight observer is a
> sixth source with its own ρ band; nothing else in the architecture changes.

## A.3 The admission gate: you must characterize the error before you can write ρ

ρ requires the *weakest constraint the source can support*. For an integer predicate the
error is zero. For an observer it must be **measured**, e.g. *"structure count correct
within ±1 at 94% on NYC-density blocks, degrading to 78% under closed canopy."*

That measurement is real work and it precedes admission. It is also not global: **error
varies by land cover, density, season and imagery vintage**, so the band is regional, not a
single constant. No characterization, no admission.

## A.4 What an observer could supply that no landed source does

- **Building count per parcel** — `NUMBLDGS` is unreliable; this checks it independently
- **A footprint we generate ourselves** — a geometry source we license from nobody
- **Construction state — is there a building here *now*?** The Allen interval constraints in
  §7 need demolition and new-construction observations and currently have **no observation
  source at all**
- **Confirmation of roof-ridge over-segmentation.** Régin's Hall-set violation *proves*
  a source split one building into several; imagery *shows* it. Proof and picture in the
  same evidence card.

## A.5 Imagery and elevation source inventory

**Verified 2026-08-15:**

| Source | Type | Res | License | Access |
|---|---|---|---|---|
| **NAIP** (USDA FPAC-BC) | aerial, RGB+NIR | 1 m 2003–17; **0.6 m from 2018**; 0.3 m coastal option | **Public domain** (US Gov work) | Three public S3 buckets — `naip-visualization` (3-band **COG**), `naip-analytic` (4-band MRF), `naip-source` (raw GeoTIFF). STAC catalogs exist (`stactools-naip`, Planetary Computer). |

**NAIP access pattern — the one to use.** COG plus **HTTP range requests**: fetch only the
byte window covering an H3 cell. No bulk download, no tile server, no API key, no rate
limit. **Pin the S3 object version or ETag and imagery becomes a content-addressed input**
like the tile artifact itself; a byte-range read is deterministic.

**NAIP caveats, all real:**
1. **Vintage varies by state and is not annual** — rotating 2–3 year schedule. Imagery date
   is a per-tile fact and must enter the temporal constraints as an *observation date*,
   never as "now."
2. **Recent coverage needs verifying.** The AWS registry describes 2011–2018 for one bucket
   while STAC catalogs claim 2010–present. Confirm against the live bucket before planning.
3. **Leaf-on growing-season imagery.** Canopy obscures structures in tree-heavy areas —
   irrelevant for dense Manhattan, material for suburban and agency multifamily. This is
   the dominant term in the ρ band and it is seasonal and geographic.

**Not yet verified — candidates to assess (claims below are from general knowledge and
MUST be checked before any commitment):**

| Candidate | Why it might matter |
|---|---|
| **USGS 3DEP LiDAR** | **Possibly better than imagery for this problem.** Height is *measured*, not inferred; building extraction from point clouds is classical geometry rather than ML, so there may be **no model to characterize at all**; distinguishes building from parking lot trivially; yields floor-count estimates directly against `NUMFLOORS`. Public domain, on AWS Open Data. Coverage incomplete and vintage varies widely. **Assess this before assuming imagery is the right sensor.** |
| **Sentinel-1 SAR** (ESA) | Radar, ~10 m, all-weather, free and open. Too coarse for footprints but buildings have distinctive double-bounce signatures — potentially useful for *presence and change detection* rather than geometry. |
| **Sentinel-2** (ESA) | ~10 m optical, free and open. Too coarse for individual buildings; useful for change detection and land cover. |
| **State and municipal orthoimagery** | Several states and NYC specifically fly higher-resolution orthophotos than NAIP, often public. Best resolution available for the proving ground. |
| **USGS High Resolution Orthoimagery (HRO)** | Public domain, higher res than NAIP in covered areas. |
| **NOAA emergency-response imagery** | Post-disaster, high-res, public domain. Narrow but valuable for change events. |
| **Commercial** — Maxar, Planet, Nearmap, Vexcel, Airbus | Higher res and better cadence, **licensed**. Same containment discipline as §10; resolvable-under-licence, not redistributable. |
| **Mapbox / Google / Esri basemaps** | **Avoid.** Terms generally forbid caching and redistribution, which is incompatible with a pinned evidence artifact. |

## A.6 The generalization

> **If the frozen-weight pattern holds, any imagery source is just another source.**

The architecture is indifferent to which sensor produced an observation. Each new source
needs exactly three things and nothing else:

1. a **license posture** — resolvable, redistributable, or neither (§10)
2. a **vintage** per observation, feeding the temporal constraints
3. a **characterized error**, which becomes its ρ band (§A.3)

Everything downstream — propagation, the residual, MUS, model counting, the certificate —
is unchanged. Adding a sensor is a data-onboarding task, not an architectural one. **That
is the property worth protecting.**

---

# Appendix B — MEASURED: the component-size claim is not supported

Added 2026-08-15. **This is the first hard measurement against real data and it falsifies a
load-bearing claim in §6.** Recorded here rather than quietly amended, because the claim was
used to justify the exact-compilation strategy and the 0.5 s/tile budget.

## B.1 What §6 claimed

> "Typical component after slot-level geometric filtering: **6–20 variables**, d ≤ 8. Tail to
> ~40 on a dense assemblage." — and therefore exact compilation is affordable and subsumes
> all k-consistency.

**This was an estimate presented without measurement.**

## B.2 The measurement

Real tile: 100 MapPLUTO parcels + 93 NYC building footprints within 150 m of the 305 E 72nd
rooftop geocode (`docs/geo_design_session/TILE_305_E_72ND.txt`). Bipartite graph, parcel
centroid ↔ footprint centroid, edge where haversine distance ≤ r. Connected components:

```
  r(m)  comps    mean   max   p50   p90  in 6-20  isolated
    10     64    3.02    17     2     7        8        16
    15     51    3.78    25     2     8        7         7
    20     33    5.85    34     2    15        7         4
    25     24    8.04    37     4    16        9         1
    30     12   16.08    49    10    37        5         1
    35      7   27.57    59    31    59        1         1
    40      6   32.17    59    37    59        1         1
    50      3   64.33    77    59    77        0         0
    60      1  193.00   193   193   193        0         0
   150      1  193.00   193   193   193        0         0
```

## B.3 The verdict: there is no usable plateau

The adversarial review predicted the exact test: *"either there is a stable plateau where
components land in 6–20, or the distribution jumps from singletons to tens with no usable r,
and the claim is dead."*

**It is the second one.** The distribution goes from mostly-singletons straight to a giant
component:

- At the most favourable radius (r = 25 m) only **9 of 24** components fall in the 6–20 band,
  and the **maximum is already 37** — above §6's stated ceiling.
- At r = 30 m the mean is 16 but the max is **49**.
- **The tile percolates at r ≈ 60 m** into a single 193-variable component, well inside the
  150 m tile radius.

**Centroid proximity does not decompose this tile at any radius.** The exact-compilation
argument in §6 rests on a decomposition that this filter does not produce.

## B.4 What actually decomposed the tile, and why it does not count

The ground-truth pass measured components via the footprint table's `MAPPLUTO_BBL → BBL`
bridge and got mean 2.92, max 5 — comfortably inside budget. **That number is vacuous, and
must not be cited as support.**

> Measuring components through `MAPPLUTO_BBL` measures the component structure of an
> equivalence relation whose classes are defined by the key being resolved. It is a fact
> about Manhattan building stock — footprints per tax lot — not about a resolution
> architecture. And if `MAPPLUTO_BBL` exists, that edge needs no propagation at all; it is a
> deterministic join. **An architecture that passes here passes by not being exercised.**

The honest entry for the component-size row is **not applicable**, not a number with an
asterisk.

## B.5 The 25 m filter, corrected

§B.2's companion measurement — equal-area disc radius `√(A/π)` per parcel, a hard floor on
centroid-to-boundary distance:

```
  median parcel   8.7 m
  p90            18.6 m
  max            33.4 m   ← BBL 1014477501, LOTAREA 37,800 — the answer parcel
  parcels exceeding 25 m:  4 / 100  =  4%
```

So the earlier refutation ("a 25 m filter deletes the true parcel, which sits 31.58 m from
its centroid") had the right conclusion for the wrong reason. **The correct statement is
worse for the filter:**

> A fixed 25 m centroid radius is adequate for 96% of parcels and fails **specifically and
> silently on the ~4% that are large assemblages** — which are precisely the hard cases this
> product exists to solve. The answer parcel here is the single largest in the tile.

**A fixed radius is the wrong shape of filter.** Any replacement must normalise by parcel
extent — half-diagonal from `LOTFRONT`/`LOTDEPTH`, or the equal-area radius above — rather
than applying a constant.

## B.6 Consequences

1. **§6's consistency ladder and 0.5 s/tile budget are unvalidated.** Do not quote them.
2. **The decomposition mechanism is an open question**, and probably needs real polygon
   containment rather than centroid proximity. That requires a re-pull with
   `ST_AsWKT(GEOM_GEOG)` on both tables.
3. **If no filter decomposes dense tiles**, exact compilation may be affordable only in
   sparser geographies, and dense urban tiles may need a different strategy — a real
   possibility that must be priced before committing.
4. **Scope of this finding: n = 1, and it is the dense extreme.** Midtown-adjacent Manhattan
   is the worst case for percolation. A suburban or agency-multifamily tile will decompose
   far more readily. **The claim is falsified for dense urban, not universally** — measure
   across a stratified tile sample before concluding anything general.

## B.7 The process lesson

The estimate survived a full red team, an adversarial cross-scoring round, and two model
families independently endorsing the architecture. **It was killed by twenty minutes of
arithmetic on data we already had.** No amount of adversarial reasoning substitutes for one
measurement.

---

# Appendix C — MEASURED: the tile is ~7× larger than the plan assumes

Added 2026-08-15, immediately after Appendix B. **This falsifies the single number the entire
commercial thesis rests on.**

## C.1 What the plan and the epic assume

> §6: "About 200 features per tile, so methods that are globally intractable are often FREE
> here… **O(n³) is free. Even O(2ⁿ) over a filtered subset can be free.**"
>
> bd-2kjx: the work unit is "that cell **plus its 6 neighbours** (7 cells, 0.737 km²) so
> boundary buildings resolve."

## C.2 The measurement

Feature density across **all 1,192 H3 r8 cells** covering NYC (r8 ≈ 0.737 km², i.e. exactly
the area of the epic's r9-plus-k-ring work unit):

```
                    parcels per r8 cell
  cells                          1,192
  min                                1
  median                           638
  mean                             719
  p90                            1,587
  p99                            2,103
  max                            2,422
```

Footprints run **1.0–1.6×** the parcel count in dense cells. Worked examples:

```
  882a107707fffff   2,422 parcels   2,657 footprints   =  5,079 features
  882a107631fffff   2,385 parcels   3,499 footprints   =  5,884 features
  882a107733fffff   2,367 parcels   3,644 footprints   =  6,011 features
```

**Median work unit ≈ 638 parcels + ~700 footprints ≈ 1,340 features. Worst ≈ 6,000.**

## C.3 The diagnosis: the sizing was computed for the blocking cell, not the work unit

A single r9 cell is 0.105 km², one seventh of an r8. Median parcels per r9 cell is therefore
≈ 638/7 ≈ 91, plus ~100 footprints ≈ **190 features — which is exactly the plan's "~200."**

> **The "~200 features" figure is correct for ONE r9 cell and wrong by 7× for the r9 +
> k-ring 1 work unit the epic actually specifies.**

The earlier 193-feature observation (Appendix B) came from a 150 m radius disc = 0.071 km²,
about one tenth of an r8 — consistent with, and therefore not contradicting, this result. It
simply measured a much smaller area than the stated work unit.

## C.4 Why this breaks the commercial argument

```
  n =   200    n³ = 8.0e6      free
  n = 1,340    n³ = 2.4e9      seconds, not milliseconds
  n = 6,000    n³ = 2.2e11     minutes per tile
```

The thesis is *"we spend 500× what a spatial join costs, and that is the entire commercial
thesis."* At the measured median that becomes several thousand times, and the ~0.5 s/tile
budget and the ~140 CPU-hour national pass in §13 are **not supported**.

## C.5 Options, none of them free

1. **Work unit = one r9 cell.** Restores ~190 features and the whole cost model, but
   sacrifices the k-ring halo that exists so boundary features resolve. Boundary handling
   would need a different mechanism.
2. **Accept 7× and re-price.** Honest, and it makes the national pass materially more
   expensive; must be re-estimated rather than asserted.
3. **Drop to r10 + k-ring.** r10 is 7× smaller again, so the work unit returns to ~190
   features — at the cost of 7× more tiles and more boundary surface per unit area.
4. **Decompose within the tile.** Only viable if a filter actually decomposes dense tiles,
   which Appendix B shows centroid proximity does not. Depends entirely on bd-3un6.

**Option 3 is the most likely answer** — it preserves the halo argument and restores the
sizing — but it must be measured, not assumed, because boundary-crossing features scale with
perimeter and r10 has considerably more perimeter per unit area.

## C.6 A second live format defect

`H3_R8` is stored as **INTEGER** in `NYC_DCP_MAPPLUTO_HOT` (e.g. `613229552600088575`) and
as **TEXT** in `NYC_BUILDING_FOOTPRINTS_HOT` (e.g. `'882a107707fffff'`).

This was not deduced — it was observed failing. A first query comparing the two directly
returned `FOOTPRINT_COUNT: 0` for every cell, which reads as *"there are no buildings in
this cell"* rather than raising a type error. **A naive join between the two NYC tables on
H3_R8 silently returns nothing.**

That is now the second such defect in the same table pair, alongside `BBL` being
`"1014477501.0"` in MapPLUTO and `"1014470001"` in the footprints table. **Both tables are
NYC municipal sources landed by the same pipeline into the same schema.** Cross-source
normalization is a first-class ingest concern, not a tidy-up.

## C.7 Scope

Measured across **all 1,192 NYC r8 cells**, so unlike Appendix B this is not n=1. It is
still NYC-only, and NYC is the dense extreme — a national distribution will have a far
longer low-density tail. **The sizing must be re-measured per geography before any national
cost estimate is quoted.**

---

# Appendix D — MEASURED: the predicate is load-bearing, and the obvious one fails

Added 2026-08-15. **The first measurement that supports the architecture rather than
falsifying it — conditional on a predicate choice that is not the obvious one.**

## D.1 The question

Appendix B showed centroid proximity does not decompose a dense tile at any radius. But
centroid distance was never the intended filter; the architecture says a footprint belongs
to a slot when it is *geometrically compatible* with a parcel. So: does a real polygon
predicate decompose the tile?

## D.2 The measurement

H3 r8 cell `882a100d8bfffff` — **2,343 MapPLUTO lots, 2,354 NYC building footprints.**
Predicates computed server-side in Snowflake; no geometry shipped. Per footprint, how many
parcels does it match?

```
  predicate                          edges    zero        exactly one   more than one
  A  ST_INTERSECTS                   4,718      17 ( 1%)    240 (10%)    2,097 (89%)
  B  ST_CONTAINS                       179   2,175 (92%)    179 ( 8%)        0 ( 0%)
  C  intersects AND >50% of footprint
     area inside the parcel          1,988     366 (16%)  1,988 (84%)        0 ( 0%)
```

## D.3 What each predicate does

**A — `ST_INTERSECTS` fails outright.** 89% of footprints touch more than one parcel. In a
dense block of contiguous row buildings, a footprint touches the lot lines of its
neighbours, so "intersects" means "is somewhere on this block." Every footprint would chain
several parcels together and **the tile would not decompose at all** — the same failure as
centroid proximity, for a different reason.

**B — `ST_CONTAINS` fails in the opposite direction.** 92% of footprints are contained in
*zero* parcels. Buildings routinely cross lot lines, and the two layers are independently
digitized at different vintages, so strict nesting almost never holds. Only 179 of 2,354
footprints sit entirely inside a lot.

**C — area majority works, and is mathematically clean.** 84% of footprints have exactly one
parcel, and **zero have more than one** — necessarily, since only one parcel can hold more
than half of a footprint's area. The remaining 16% have no majority holder, which is an
honest abstention identifying genuine lot-line straddlers.

## D.4 The decomposition result

Under predicate C each footprint has **at most one** parcel edge, so the compatibility graph
is a **forest**. Components are exactly *one parcel plus the footprints whose area it
majority-holds* — typically 2–3 variables, far inside any compilation budget.

> **Polygon area-majority decomposes the tile. `ST_INTERSECTS` — the predicate a competent
> engineer reaches for first — does not.**

So §6's exact-compilation strategy survives Appendix B's falsification, but *only* with the
right predicate. The choice is not a detail; it is the difference between a forest and a
single connected block.

## D.5 This is ρ working exactly as specified

The three predicates are three readings of the same evidence, and §3's discipline picks the
right one without any tuning:

- `ST_INTERSECTS` is the **unsound** reading — "touches" is not "is on," and admitting it
  asserts more than the geometry supports in the wrong direction.
- `ST_CONTAINS` is **over-strict** — it demands a nesting the two independently-digitized
  layers do not have, so it refuses almost everything.
- **Area majority is the sound relaxation**: the weakest constraint that still says "this
  building is on this lot," and it fails to a named abstention rather than to a guess.

**The 50% threshold is doing no tuning work** — it is the unique fraction that guarantees
at most one match. Any lower value reintroduces multi-parcel ambiguity; any higher value
discards true assignments. It is structural, not calibrated.

## D.6 The honest caveat

The same objection the adversarial review raised about the `MAPPLUTO_BBL` bridge applies
here in weaker form: **if 84% of footprint-to-parcel assignment is decided by a single
deterministic predicate, the constraint machinery is not being exercised at that level.**
Parcel-to-building assignment is largely a solved geometric join.

That is fine, and it should be stated plainly rather than counted as a win: the architecture
earns its keep at the level *above* — which parcels and buildings constitute the asserted
**property** — not at the level of which building sits on which lot. **The 16% with no
majority holder is where the interesting work is**, and it is exactly the lot-line-straddling
assemblage population the product exists to resolve.

## D.8 Stratified check — two strata verified, and it improves at lower density

Re-ran predicate C on a second cell at very different density. Both figures below are from
returned structured query results, not prose.

```
  cell               borough   parcels  footprints   exactly one     zero
  882a100d8bfffff    MN/dense    2,343       2,354   1,988 (84%)   366 (16%)
  882a100f4dfffff    BX            300         291     291 (100%)     0 ( 0%)
```

**The predicate gets cleaner as density falls** — 100% unique assignment with zero orphans in
a 300-lot Bronx cell, against 84%/16% in a 2,343-lot cell. That is the expected direction:
in less dense fabric, buildings sit within their lots rather than straddling lot lines, so
the no-majority population shrinks.

Two strata across a 7.8× density range both produce a forest. **The decomposition property
is not an artifact of one cell.**

**Scope, honestly.** Three further cells (Queens ~1,500, Queens ~700, Manhattan ~41) were
queried and returned no usable structured output. Loom emitted prose for two of them
claiming a "multi-match rate" of ~3% — which the >50% predicate makes *mathematically
impossible*, since only one parcel can hold a majority of a footprint's area. **That prose
is not cited and should not be trusted.** Those strata remain unmeasured. Complete them
under bd-3un6.

## D.7 Consequences

1. **Adopt area-majority as the footprint-to-parcel predicate.** Record `ST_INTERSECTS` as a
   rejected candidate with this measurement, so it is not re-proposed.
2. **§6's decomposition claim is restored** for parcel↔footprint, with components of ~2–3
   rather than the estimated 6–20. Appendix C's tile-sizing problem is *unaffected* and
   still stands.
3. **The 16% no-majority population needs its own path** — it is not an error and must not
   be dropped.
4. Still **n=1 cell**, dense Manhattan. Re-measure across strata per bd-3un6.

---

# Appendix E — MEASURED: both kill-criterion baselines, tier-resolved

Added 2026-08-16 (bd-14co; full tables and exact SQL in
`docs/geo_design_session/BASELINES_BD14CO.md`). **The two numbers the cascade must beat
now exist, and the tier breakdown localizes exactly where the product's work lives.**
All figures from returned structured results against MapPLUTO release **26v1**
(2026-05-01), five-borough CMBS geocode scope, fan-out-aware distinct-point grain.

## E.1 The two baseline points

```
  naive address-string   28.89% coverage (1,522/5,269 address-county keys), zero multi-match,
                         97.70% house-number agreement on the keys where it fires
  geometry-only PIP      94.65% coverage (3,858/4,076 distinct points), zero multi-lot points
                         on 26v1, 71.41% house-number agreement on comparable hits
```

The cascade must pass **above both points on the coverage/precision plane**: materially
more coverage than 28.89% at address-grade precision, and higher precision than
geometry-only near 94.65% coverage. Precision for both baselines remains unmeasured —
scoring against PLUTO address→BBL is circular (the CMBS address is the thing under test);
it is blocked on bd-179b's address-independent ACRIS ground truth.

**Snapshot correction:** the previously recorded 157 multi-lot points do not reproduce on
26v1 — both `ST_CONTAINS` and `ST_INTERSECTS` now return only one-lot hit points. The
condo parent/unit overlap population is a property of a specific MapPLUTO release, not a
stable fact. Do not quote it without pinning its snapshot.

## E.2 The silent-error tier, quantified

`nearest_rooftop_match` (344 points, 8.4% of the tile-scope corpus) is where both
channels fail **on the same population**:

```
  tier                    PIP hit   house-number agree   chimera   address-match fires
  rooftop                  99.91%        78.00%            5.98%        34.13%
  nearest_rooftop_match   100.00%        48.40%           14.53%         1.52%
  range_interpolation      53.02%        13.94%            5.94%        24.12%
```

Geometry is maximally confident exactly where it is least trustworthy (100% hit, less
than half house-number agreement, 2.4× the rooftop chimera rate), and naive address
matching almost never fires there (9/593 keys). A cascade that arbitrates between the
two channels has nothing to arbitrate *with* on this tier — it needs the constraint
frame's independent evidence (footprints, address ranges, parity, temporal). **This is
the population the architecture exists for, and it is now a named, measurable slice.**

## E.3 Fan-out at this grain

95.53% of surrogate property keys hit exactly one lot (6,033/6,315); 0.05% hit two;
4.42% hit zero. The single-point, single-lot case dominates the geocode grain — the
assemblage/multi-parcel problem lives almost entirely below the surface of this table
and must come from documents (bd-179b, bd-1oid), not from geocode fan-out.

---

# Appendix F — MEASURED: decomposition survives stratification and a second footprint source; Appendix D's 16% was a denominator artifact

Added 2026-08-16 (bd-3un6; full tables and exact SQL in
`docs/geo_design_session/STRATA_FEMA_BD3UN6.md`). Six NYC cells now measured across a
~57× density range, plus the first genuine multi-source test using the newly landed
`FEMA_USA_STRUCTURES_HOT` (135.3M rows, both TEXT and INT H3 keys — the C.6 defect class
now has typed companions).

## F.1 Stratified predicate C: the forest holds everywhere, with one new shape

Four new strata (MN 41 parcels, QN 1,502, QN 701, SI 101), all with **zero
multi-matches** — the >50% uniqueness guarantee is structural and held in every cell:

```
  cell        borough  parcels  footprints  exactly-one    zero      max component
  882a1008c7  MN            41          45   41 (91.1%)   4 ( 8.9%)      4
  882a103b6b  QN         1,502       2,007   1,753 (87.3%) 254 (12.7%)   4
  882a100e25  QN           701       1,049   927 (88.4%)  122 (11.6%)    5
  882a106019  SI           101         256   154 (60.2%)  102 (39.8%)   71
```

Three cells decompose into components of ≤5. The Staten Island cell is the new shape:
still a forest, but with **parcel-star components of 39 and 71** — single large parcels
holding many structures (campus/complex fabric). Exact compilation survives, but the
per-component budget must be sized by the largest parcel-star, not by a universal
"2–3 variables."

## F.2 The denominator correction to Appendix D

Appendix D's 84%/16% dense-Manhattan split is reproduced **only** with the source-asserted
`SHAPE_AREA` as the fraction's denominator. The pure geometric predicate
`ST_AREA(intersection)/ST_AREA(footprint)` on the same cell gives **2,332 / 22 / 0**
(99.1% exactly-one, 0.9% no-majority). Of the 366 "no-majority" footprints, 344 resolve
under the literal geometric denominator, 311 are clean two-parcel straddlers by
intersection count, and 300 have ≥99% of their geometric area inside their top two
parcels.

**The 16% population was mostly an artifact of dividing a computed area by an asserted
one.** The same failure appeared independently in FEMA's `SQMETERS` field (2 multi-matches
in Queens where geometry gives 0) and in a units-conversion probe (946 impossible
multi-matches — dimensional error, caught by the multi=0 sanity gate). This is ρ working
as specified, one level down: **asserted source area fields are observations to check,
never denominators to divide by.** Adopt `ST_AREA`-over-`ST_AREA` as the canonical
predicate-C form; the no-majority residual in dense Manhattan is ~1%, not 16%.

## F.3 The multi-source result: FEMA does not re-percolate the tile

Three-layer merged graph (parcels + NYC footprints + FEMA structures), geometric
denominators, three cells:

```
  cell           NYC exact/zero   FEMA exact/zero   merged components  merged max
  BX  882a100f4d   274 / 17         76 /  39            356               19
  MN  882a100d8b  1,988 / 366       88 / 152          2,861                6
  QN  882a103b6b  1,753 / 254     1,078 /  30         1,786                6
```

**The merged graph remains a forest in all three cells.** Adding a second footprint
source refines without re-percolating — measured, not asserted, for the first time. FEMA
coverage is strongly geography-dependent: 97.3% majority-parcel rate in Queens vs 36.7%
in dense Manhattan (FEMA sees only 240 structures where NYC sees 2,354) — FEMA is a
corroborating source in outer-borough fabric and nearly absent in the urban core.

## F.4 Cross-source agreement is real but asymmetric

On the Queens cell, NYC and FEMA agree on per-parcel structure counts for 55.2% of
parcels (829/1,502); where they disagree, NYC sees more on 634 parcels vs FEMA's 39, and
no disagreement exceeds 3. Over-segmentation vs `NUMBLDGS` is negligible (max overage 1).
The dominant disagreement mode is FEMA missing whole runs of 2-structure residential
parcels, not over-segmentation — so Régin-style within-source exclusivity has little to
catch at the footprint level in this fabric, and the `gcc` coverage-rate constraint
(§3's FEMA row) is the right admission form.

## F.5 Consequences

1. **§6's decomposition strategy is affirmed at n=6 cells across boroughs and densities,
   and under multi-source load.** The open sizing question is no longer decomposition —
   it is Appendix C's work-unit arithmetic (r10+k-ring), which remains unmeasured.
2. **Appendix D's predicate is right; its denominator was wrong.** Canonical form is
   geometric-over-geometric. The "16% product population" shrinks to ~1% no-majority
   plus the parcel-star components — the honest hard residual.
3. **Parcel-star components (SI max 71) are the new worst case** for per-component
   compilation budgets; they, not dense Manhattan, bound the component size.
4. All measurements NYC-only; the bead's suburban/agency-multifamily stratum has no
   landed source yet.

---

# Appendix G — MEASURED: no k-ring configuration restores "~200 features"; the cost model must be component-wise

Added 2026-08-16 (bd-152l; full tables and exact SQL in
`docs/geo_design_session/WORKUNIT_SIZING_BD152L.md`). This settles Appendix C's Option 3
with data: measured across every parcel-containing NYC work unit, three sources
(parcels + NYC footprints + FEMA structures), centroid-derived r9/r10 home cells (no
landed table has native r9/r10 columns).

## G.1 The distributions

```
  work unit   centers   median   mean     p90     p99     max    >200      >400
  r9  + k1      6,829    2,274   2,442   4,619   6,103   7,515   94.77%    90.76%
  r10 + k1     39,098      418     421     755   1,011   1,329   75.88%    52.14%
```

r10+k1 cuts the median 5.44× — **and still does not restore ~190.** Three-quarters of
r10 work units exceed 200 features; half exceed 400. The "~200 features, so O(n³) is
free" arithmetic does not hold at any measured k-ring configuration once FEMA is
included and the halo is retained.

## G.2 The boundary tax, measured

Fraction of features whose geometry is not contained in their centroid's home cell:

```
  source           r9        r10
  parcels        15.41%    37.24%
  NYC footprints  8.07%    20.33%
  FEMA           10.86%    26.14%
```

Moving r9→r10 raises boundary pressure ~2.4–2.6× across all three sources — Appendix
C's perimeter warning, quantified. At r10 more than a third of parcels straddle their
home cell. (Direct `H3_COVERAGE_STRINGS` aggregation timed out twice; the containment
predicate above is the recorded geometric substitute.)

## G.3 Verdict on Appendix C's options

**Option 3 is directionally right and numerically insufficient.** No uniform-grid work
unit both preserves the halo and lands near 200 features in NYC. The honest reading,
combining this with Appendix F:

1. **The tile-wide O(n³) framing is the wrong cost model.** Appendix F shows the
   compatibility graph decomposes into parcel-star components (max 71) under the
   canonical geometric predicate — so per-work-unit cost is driven by component-wise
   compilation plus halo reconciliation, not by tile-wide consistency passes over n.
2. **§13's national-pass estimate must be re-derived** from: 39,098 r10 units × NYC-scale
   component distributions, or priced at r9 with in-tile decomposition. Until then, quote
   neither 0.5 s/tile nor 140 CPU-hours.
3. **Boundary reconciliation is a first-class cost** at r10 (20–37% of features), not an
   edge case; bd-2b9d's halo design should assume it.
4. NYC-only, land-biased center universe (parcel-containing cells), all three sources —
   denominators differ from Appendix C's all-1,192-r8-cell, two-source measure by
   construction; both are recorded with their SQL.

---

# Appendix H — MEASURED: the ACRIS truth set exists, caught its own contamination, and puts provisional precision far below coverage

Added 2026-08-16 (bd-179b; full tables and exact SQL in
`docs/geo_design_session/GROUNDTRUTH_ACRIS_BD179B.md`). First address-independent ground
truth for the Appendix E baselines: CMBS loans matched to recorded ACRIS mortgages by
**amount + recording-date window with no address fields anywhere in the join** (bridge:
`PROPERTY_PERIOD_FACT → LOAN_ISSUANCE` on CIK+ASSETNUMBER; 3,040 five-borough loans).

## H.1 The gate, and what it accepted

Operating point exact-cents ± 30 days, unique-or-discard: **523 accepts** (1,230
ambiguous discarded, 1,287 no-match), 392 → one BBL, 131 → 2+ BBLs. Truth coverage of
the baseline grains is honest and low: 582/4,076 points (14.28%), 864/5,269 address keys
(16.40%).

## H.2 The raw headline, and why it must not be quoted bare

Scored any-overlap (lenient): geometry PIP **29.48%** (166/563), naive address **23.43%**
(67/286) on truth-covered units. Condo unit-lot representation explains only ~5pp
(block-grade bounds 34.28% / 28.67%). The dominant failure signature was full-block
mismatch — which is also the signature of a *wrong unique match*. The contamination
probe settled it with three independent discriminators:

```
  signal                     lot-correct accepts        full-block mismatches
  recording offset           0 negative, median +12d    165/356 negative, spans window
  ACRIS legal borough        agrees 135/135             disagrees 203/356 (113 w/ county too)
  amount roundness           non-round: 55.46% precise  $1M multiples: 7.88% precise
```

Real recordings happen days-to-weeks *after* origination in the property's borough;
collisions are uniform in time, cross-borough, and concentrated in round amounts.
**Amount+date uniqueness alone is not a sufficient truth gate.** The raw 29.48% is a
contaminated estimate and the report says so explicitly.

## H.3 What survives as the provisional precision point

On the cleanest measurable slice (non-round amounts, 119 loans): **geometry PIP ≈ 55%
(66/119)** against document truth — with ~95% coverage. The coverage/precision plane the
cascade must beat is therefore provisionally: address-string (28.89% coverage, high
precision when it fires), geometry (≈95% coverage, ≈55% precision on clean truth). The
gap between them is the product. Refine the gate (non-negative offset, legal-borough
agreement, roundness handling or a second discriminator such as lender-name tokens) and
re-score before treating any precision number as final.

## H.4 The assemblage payoff

Of 125 multi-BBL loans invisible to the geocode grain, condo-signature filtering leaves
**79 genuine multi-parcel candidates** (24 spanning multiple blocks). That is the first
measured count of the invisible-assemblage population — the exact population §2's
architecture exists to resolve — subject to the same gate-refinement caveat.

## H.5 The meta-result

The truth set audited itself: declared bands (unique-or-discard) plus independent
consistency checks (offset sign, borough, roundness) converted "suspiciously low
precision" into a *named, attributable defect of the truth gate* rather than a silently
wrong conclusion. This is §3.2's band-versus-threshold argument operating one level up —
the strongest process evidence yet that the architecture's self-auditing claim is real.

## H.6 Gate V2 — the finished kill-criterion plane

Same session, completing H.3's mandate. Operating gate: exact cents, recording offset
**[0,+45] days**, ACRIS legal borough must agree with a property county, all 100k/1M-round
amounts dropped, unique-or-discard applied *after* the filters (candidacy recomputed from
scratch). Sensitivity reconciles to 3,040 loans at every window; [0,+45] accepts 166
(48 ambiguous, 451 no-match, 2,375 round-excluded).

```
                       coverage                precision (lot)   precision (block)
  geometry PIP         94.65% of points        154/233 = 66.09%   169/233 = 72.53%
  naive address        28.89% of keys          63/93  = 67.74%    71/93  = 76.34%
  nearest_rooftop PIP  100% of tier            15/29  = 51.72%    18/29  = 62.07%
  nearest_rooftop addr fires 0/44 truth keys   —                  —
```

**Both baselines sit at ~two-thirds precision against document truth.** The plan's
implicit assumption that address matching is high-precision-when-it-fires is also dead:
67.74% at lot grade. The cascade's target is now concrete — materially exceed ~66–68%
precision at geometry's coverage, and the ~34% of geometry answers that are wrong (worst
on nearest_rooftop, the tier where address evidence never fires) are the addressable
population. G6 on v2 accepts: 25 invisible multi-BBL loans, 15 non-condo.

Caveats that travel with these numbers: v2 truth coverage is small (242/4,076 points,
5.94%) and the round-amount exclusion biases the truth set toward odd-amount loans;
lender-name second-discriminator admission of round amounts is the recorded path to a
larger truth set (originator and party fields are landed).

---

# Appendix I — WORKED: the six-case corpus exists; the code gate is satisfied

Added 2026-08-16 (bd-tccn; index at `docs/geo_design_session/CASES_INDEX_BDTCCN.md`, one
file per case with structured evidence tables and exact SQL). The operator's 2026-08-14
"no code until this exists" gate is met: six cases, query-selected (not hand-curated),
worked end to end from landed data, each forcing a distinct design decision:

| # | Property | Verdict | Decision forced |
|---|---|---|---|
| 1 | 1 Grace Court, BK | singleton, 4 sources converge | the clean floor + ablation control; even the sibling "1 GRACE CT" spelling fails naive normalization |
| 2 | 982 Madison St, BK | singleton via address after geocode abstains; nearest-lot picks the wrong building | tile-bounded proximity; **no snap-to-nearest** |
| 3 | 107–111 N 9th St, BK | three-parcel assemblage | interval semantics; **one BBL is a false answer** |
| 4 | 199–205 First Ave + 349/351 E 12th, MN | six-parcel core; parsed "199 E 12th St" rejected as synthesized | multi-address parsing; **chimera rejection before geocode trust** |
| 5 | 66 Crosby St a/k/a 514 Broadway, MN | singleton despite zero address matches; ACRIS carries both frontages | address disagreement is noise; **a/k/a fields are address sets** |
| 6 | 305 E 72nd St, MN | parcel singleton, building residual {2 BINs} | **entity-level output**; parcel identity cannot answer the product question |

Recurring acquisition finding: cases 4, 5, and 6 each hit the missing address-set layer
(only primary addresses in MapPLUTO) — the bd-35qg/PAD elevation from §12, now with three
worked receipts. Source snapshot from case 1: FEMA NY 5.0M and Microsoft GlobalML NY 5.4M
rows landed; **Overture reports 0 NY rows** — a landing gap to resolve before Overture
appears in any tile. These six artifacts are the seed of the `--suite`/`--gold` evaluation
corpus, the visual evidence card's worked examples (bd-101v), and a showable pre-product
sales artifact.

---

# Appendix J — VERIFIED: Appendix A.5's imagery table, from primary sources

Added 2026-08-16 (bd-q5k2; full dossiers with license quotes, live-check receipts, and
URLs in `docs/geo_design_session/IMAGERY_SOURCES_BDQ5K2.md`; access date 2026-08-16).
Every A.5 candidate dispositioned from agency pages, license text, and live
bucket/STAC/range-request checks — not model recall. What changed from A.5's assumptions:

1. **NYS/NYC orthoimagery is the best first source for NYC, not NAIP** — 6-inch true
   orthos, verified borough downloads for every even year 2006–2024, permissive
   (NYS open / NYC CC BY 4.0), byte-range-capable with ETags. BC/FP/CD all strong at
   biennial cadence.
2. **NAIP's posture changed:** AWS buckets are now Requester-Pays (A.5's "no key, no
   rate limit" no longer holds); the anonymous path is the Planetary Computer COG mirror
   (206-verified). The A.5 vintage discrepancy resolves to **2010–2023**, with NY flown
   only in odd years plus 2022. Demoted to national fallback.
3. **3DEP survives with a sharpened claim:** public EPT bucket live (NYC = 4.75B points),
   but Times Square's product is the **2013/Sandy-era** collection — measured height,
   decade-old. The "no model to characterize" hope is refuted as stated and replaced with
   the defensible version: deterministic extraction whose error is characterizable from
   point density, classification, and footprint comparison.
4. **Sentinel-1/2:** conditional, change-detection only (10 m; BC/FP rejected). **NOAA
   ERI:** survivor for event-scoped change evidence (0.3–0.5 m, bucket live through 2026
   events). **USGS HRO:** rejected as a distinct source (legacy 2000–2016, no clean
   pinnable catalog).
5. **Commercial tier all conditional** — every vendor (Maxar/Vantor, Planet, Nearmap,
   Vexcel, Airbus) permits internal use only under contract-bounded terms; Planet's
   default terms reject local multi-user caching outright. **Basemaps rejected with the
   governing clauses quoted** — Google expressly forbids digitized building outlines;
   Mapbox forbids redistributing offline tiles; Esri offline use exists only inside
   licensed ArcGIS content packages. A.5's "avoid" instinct is now a citation.

Catalog provider/channel registration deliberately deferred (recorded in the report);
the survivors table is the registration-ready input.

---

# Appendix K — MEASURED: cheap tile discriminators do not separate geometry's wrong third; the headroom must come from evidence not yet landed

Added 2026-08-16 (bd-1a12; full tables, exact SQL, failed query shapes, and the
reconciliation diagnostics in `docs/geo_design_session/PLAUSIBILITY_BD1A12.md`). The
sharpest test the plan has faced: Gate V2's labeling (154 known-correct, 79 known-incorrect
PIP answers) made the central premise measurable — can tile-local evidence identify the
~34% of geometry answers that are wrong?

## K.1 The discriminator panel: no clean separation

On the 233 labeled points, every cheap deterministic tile fact either saturates or
false-refutes heavily:

```
  discriminator                    fires correct   fires incorrect   verdict
  NYC footprint on PIP lot           153/154          79/79          saturated
  street match (PIP-anchored)        153/154          78/79          saturated
  house-number in block range        114/154          51/79          catches 35% of wrongs,
                                                                     false-refutes 26% of rights
  house-number agrees with PIP lot   123/154          52/79          weak
  FEMA structure on PIP lot           63/154          28/79          non-separating
  boundary depth < 3 m                74/154          51/79          false-refutes 48% of rights
  parity match (where derivable)      68/154          25/79          no independent power
```

No simple accept/refute rule over these facts beats PIP alone on truth-covered points.
**With currently landed evidence, the deterministic cascade has no measured headroom over
the 66% baseline.** The founding W 74th/W 49th proof case reproduces exactly (the tile
refutes the wrong rooftop point, supports the right interpolated one) — targeted
refutation works; the aggregate rule does not.

## K.2 The strict street-presence predicate is representation-bound

After a definitional reconciliation (the panel's street row and the universe query were
measuring different predicates under one name — caught because the subset and universe
rates could not both be true), the consistent strict form (parsed street matches any
parcel-address street in the centroid-r9+k1 tile) fires on only **10.38% of the full
universe and 15.09% of labeled points** — with the tile join verified non-empty (median
843.5 parcels/tile). The failure is street-*string representation*, not street absence:
MapPLUTO primary-address spellings vs parsed streets defeat a fixed normalizer ~90% of the
time. As a refuter it would abstain on ~90% of everything; operationally unusable — and it
is precisely the gap §7's `regular` address grammar and an address-SET layer (PAD,
bd-35qg) exist to close.

## K.3 What this means for the plan

1. **The kill-criterion gap (66% → better, at ~95% coverage) is not closable with landed
   evidence and simple rules.** The measured paths to headroom: the PAD address-set layer
   (three worked-case receipts in Appendix I), document evidence (ACRIS, per Appendix H),
   and grammar-level address matching (`regular`) — all pre-identified by the plan, now
   with measured justification instead of argument.
2. Trap for all future H3 SQL, measured: stored `H3_R8` equals direct centroid-r8 on
   856,614/856,614 parcels, but `H3_CELL_TO_PARENT(centroid_r9, 8)` disagrees on 61,607
   (7.2%) — H3 child/parent nesting is not spatially exact; never mix the two forms in
   one predicate.
3. The plan's self-auditing pattern fired again, one level up: the apples-to-oranges
   predicate was caught by a subset-vs-universe reconciliation check, not by inspection.
   **Predicate definitions are load-bearing** — the same lesson as Appendices D, F, and H,
   now with its fourth receipt.

## K.4 Coda — what this appendix does and does not falsify (added 2026-08-16, on review)

This appendix's original framing overstated. What K measured is that **each tile signal is
individually weak as a unary accept/refute rule** — which is §2.1's few-bits premise,
measured, not the architecture's thesis, refuted. The plan never claimed any single
discriminator separates; it claims weak constraints **sum under joint propagation over the
candidate set**. That summing test — §17's E3 pairwise candidate test — has not been run,
and K's panel design (independent 2×2s on the predicted answer only) could not have run
it. K stands as: (a) a characterization of the input signals, (b) proof that no cheap
shortcut around the full machinery exists, and (c) the event that forced §16 and §17 into
the plan. The headroom question remains open until E3 answers it.

---

# Appendix L — MEASURED: E1–E3 ran; the failure mass was never a ranking problem

Added 2026-08-16 (bd-3ab6 + bd-2qjj; full tables and exact SQL in
`docs/geo_design_session/E1_E2_TAXONOMY_ATTR.md` and `E3_PAIRWISE.md`). The §17 ladder's
first three stages, run on the Gate V2 labeled set (233 PIP-covered points: 154 correct /
79 incorrect, denominators independently re-reconciled by both agents), interpreted
jointly per the §17 gate.

## L.1 The taxonomy (E1): two classes own 91% of the failures

```
  class                              points   signature
  gross geocode error (>500m)          40     avg 7.1 km, max 23.3 km from true parcel
  condo representation residue         32     PIP-parcel-to-true distance 0.00 m
  assemblage-neighbor artifact          2
  adjacent-lot near miss                2
  residual truth-contamination          3
  (sums to 79; sanity arm: 0/154 correct points classified)
```

The gross class is unambiguous wrong-location input — the true lot is kilometres away and
**not in any tile the point defines**. The condo class is the opposite: **geometry found
the right building** (distance 0.00 m); ACRIS records unit BBLs that MapPLUTO does not
carry as parcels. Neither is a candidate-selection failure. Only 4 points (2+2) are cases
where re-ranking within the tile is the relevant tool.

## L.2 The attribute channel (E2): thin on this proving ground

Inventory row 6's first exercise: filtered to genuinely SF-denominated assertions
(`SIZE_MEASURE='SQFT'`), coverage collapses to 10 correct / 17 incorrect labeled points —
NYC CMBS skews multifamily and asserts UNITS, and MapPLUTO landed no unit-count
comparator. On the handful of comparable rows the band test does not separate (and
mildly favors the wrong lot, n=5). Row 6 is recorded as sparse-here, not dead — its
density is geography- and asset-class-dependent, an E5 question.

## L.3 The pairwise test (E3): blocked by candidate reach, honest on both denominators

```
  all 79 failures (out-of-scope counted as unsolved):  true-lot wins 0/79
  7 tile-addressable failures:                          true wins 0, ties 0, PIP wins 7
  control arm (76 matched correct points):              true lot beats same-tile neighbor 76/76
```

The scope split is the finding: 31/79 selected true BBLs are absent from MapPLUTO
(the condo residues), 41 are present but outside the r9+k1 tile (the gross errors) —
**72/79 failures are unreachable by any tile-local pairwise solver by construction.** Of
the 7 reachable, joint measured non-ACRIS evidence never ranks the true lot first (2/7
had an individual true-winning row; the vote still went to PIP) — and 3 of those 7 are
contamination suspects where "losing" may be correct behavior. The control arm passed:
the scoring machinery correctly prefers the true lot 76/76 when it is the answer. The
method works; there is almost nothing in this failure population for it to fix.

## L.4 The verdict under §17

Read strictly, the kill condition fires **for candidate re-ranking with currently landed
evidence**: the bits do not need to sum because the failures re-ranking could address
barely exist. But the taxonomy dissolves the premise rather than the plan: the measured
path from 66% precision to the mid-90s is

1. **a condo/ledger representation bridge** (32 points, deterministic — billing↔unit BBL
   mapping; the generic class is "ledger representation compilation," squarely canon's
   identity-compiler competence, no solver required);
2. **refutation/abstention on wrong-location input** (40 points — bd-1a12's capability
   with asserted-street semantics, strengthened by an address-set layer; the tile
   proves the answer is absent and abstains rather than answering);
3. **honest residual** on the remaining handful (doubletons, contamination suspects).

This reweights the architecture's role on the point-resolution task from candidate
selection toward **justified abstention and representation compilation — which are §9.1's
own claimed products.** What the labeled set cannot exercise, and therefore remains
genuinely open for the constraint machinery, is the *collateral-composition* question —
which parcels and buildings constitute the property (Appendix I cases 3, 4, 6; the 79
non-condo multi-BBL loans of H.4) — plus E4 (joint propagation, now pointed at
composition rather than point re-ranking) and E5 (the genericity gate).

Ladder status: E1 ✓, E2 ✓ (sparse-here), E3 ✓. E4 and E5 remain, re-aimed by these
results.
