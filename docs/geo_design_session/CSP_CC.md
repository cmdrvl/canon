# CSP_CC — The Tile as a Compiled Constraint Object

**Status:** design argument, not a plan. Constraint-propagation frame taken as settled.
**Thesis in one sentence:** the deliverable is not a resolved row, it is a *canonical compiled representation of the tile's entire solution space*, from which the resolved row, the residual, the count, the blame set, the repair set, and the value-of-information are all derived views — and whose canonicity, monotonicity, and confluence are theorems about the data structure rather than disciplines about the code.

---

## 0. The claim, stated so it can be attacked

Every competitor ships a point estimate with a score. The score is uncalibrated, the tie-breaking is row-order-dependent, the geometry is double-precision and therefore platform-dependent, and next year's rerun silently overwrites this year's answer with no way to tell refinement from regression.

We ship an object `T` with the following properties, each of which is provable, not asserted:

| Property | Mechanism | Authority |
|---|---|---|
| The propagation fixpoint is unique regardless of application order | propagators are monotone, contracting, correct functions on a finite lattice | Tarski 1955; Cousot & Cousot 1977; Apt 1999 |
| The compiled form is *canonical* — same tile, same bytes, on any machine | reduced ordered decision diagram under a fixed variable order derived from the H3 index | Bryant 1986; Darwiche 2011 (SDD) |
| Adding a source can only refine, never contradict | `Models(T ∧ c) ⊆ Models(T)` — set inclusion, and *checkable* by an entailment test between last year's diagram and this year's | Darwiche & Marquis 2002 (polytime CE/EN queries on d-DNNF/SDD) |
| Abstention is the residual, not a threshold | the answer is the model set; a singleton is a decision, a doubleton is an honest doubleton | — |
| An empty model set is a *proof of source defect*, with a minimal ordered blame set | MUS via preference-ordered QuickXplain; repairs via minimal hitting-set duality | Reiter 1987; Junker 2004; Liffiton & Sakallah 2008 |
| Every conclusion is explainable by naming evidence | explanation is the minimal environment supporting the conclusion, computed on demand | de Kleer 1986 (ATMS); Junker 2004 |
| The whole run is independently machine-checkable | pseudo-Boolean proof log covering global-constraint propagation and symmetry breaking | Gocht, McCreesh & Nordström 2022 (VeriPB) |

Note the lineage. This is not a new idea; it is a 1970s idea nobody applied here. Waltz (1972/75) resolved ambiguous scene labelings from local constraints. Montanari's founding paper (1974) is literally titled *"Networks of constraints: fundamental properties and applications to picture processing."* Rosenfeld, Hummel & Zucker (1976) is *"Scene labeling by relaxation operations."* **A tile is a Waltz scene.** Two hundred noisy local observations, no global identifier, and a set of physical laws that admit only a few consistent global interpretations. That is the problem those papers solved, and the industry currently attacks it with a spatial join and a trigram index.

---

## 1. The model, because everything downstream depends on it

### 1.1 The soundness discipline (ρ)

This is the single most important design rule, and it is what makes a hard-constraint frame survive noisy sources. It is also, structurally, the answer to §5.

> **Every source attribute is admitted to the solver only through a declared, versioned relaxation operator ρ that maps the raw value to the *weakest constraint the source can actually support*.**

| Raw evidence | Naive (unsound) reading | ρ-image (sound) |
|---|---|---|
| Geocode point `g`, match type `interpolated` | "the property is at `g`" | "the subject's footprint intersects the disc of radius `r(interpolated)=150 m` about `g`" — nearly vacuous, which is *correct*, and is exactly why roadbed geocodes stop poisoning us |
| Geocode point `g`, match type `rooftop` | same | `r(rooftop)=8 m` — sharp, and legitimately so |
| Parcel `ADDRESS = "355 E 12 ST"` | "this lot's address is 355 E 12 St" | "355 E 12 St is *one of* this lot's addresses" — a membership, never a functional equality |
| Query address `199 First Ave` | "match a lot whose ADDRESS = 199 First Ave" | "*some* member of the collateral set fronts First Avenue at house number 199" — an existential over the set variable |
| `BLDGAREA = 214,300` | "GLA is 214,300" | gross above-grade `∈ [214300, 214300]`; net rentable `∈ [⌊0.78·x⌋, ⌈0.95·x⌉]` for `BLDGCLASS ∈ O*`, a separate declared relation |
| `OWNERNAME` equal after normalization | "same owner" | "these lots *may* form an assemblage" (permits, never forbids) |
| `OWNERNAME` different | "different owner ⟹ not assembled" | **no constraint at all** |
| FEMA county coverage 92% for structures >450 sf | unused | `gcc` lower bound: at least `⌈0.80·K⌉` slots carry a FEMA observation (0.80 = declared conservative floor) |

Two consequences fall out immediately:

1. **Every noisy channel is admitted only in its sound direction.** Address evidence never excludes; it only requires existence. Ownership never separates; it only permits. Geocodes never locate; they only bound. This is why neither channel needs to be a "proposer" — the red team's dilemma was an artifact of using noisy evidence in its *unsound* direction and then needing a second channel to clean up.

2. **Theorem (trivial, and the whole business).** If every ρ is sound — the true world satisfies `ρ(v)` whenever the source reports `v` — then the true assignment is in the model set. Therefore:

   > **An empty model set is a proof that at least one source violated its own published error model.**

   Not "the sources disagree." Not "the join failed." A *proof*, attributable to a minimal set of source records, that a specific vendor's declared tolerance was breached on a specific parcel. That is a falsifiable claim you can put in an email to Overture, FEMA, or a servicer. Nobody else in this market can send that email.

### 1.2 Geometry is integers or it is not deterministic

Every coordinate is projected into a **per-tile fixed local integer frame** and snapped to millimetres in `i64`. The projection constants for each H3 cell are precomputed once and shipped as versioned data, so at decision time the projection is a table lookup plus an exact integer affine map. **No transcendental function, no floating-point value, and no `f64` comparison appears anywhere in the decision path.**

Arithmetic check: a 1 km tile spans ~10⁶ mm, so coordinates fit in ~2×10⁶. Shoelace terms are ~4×10¹²; summed over a 10³-vertex polygon, ~4×10¹⁵ — comfortably inside `i64`, and we carry `i128` for headroom. Orientation predicates are exact `i128` determinants. No adaptive-precision filter (Shewchuk 1997) is needed because we never leave the integers.

*Cheap wrong way:* `ST_Contains` in PostGIS/GEOS double precision. *Silent error:* a footprint straddling a lot line by 3 cm is assigned to lot A on x86 and lot B on ARM, and to a third answer after a GEOS point release. In a portfolio of 40,000 loans that is a handful of silently different answers per rerun and no mechanism to detect them. *What exact buys:* byte-identity across platforms and decades, by construction rather than by testing.

### 1.3 Variables

Canonical total order `≺` on all features: `(source_rank, source_native_id_bytes)` with `source_rank` from a versioned table. Everything downstream — variable order, diagram order, report order, tie-breaks — derives from `≺`. There is no hash-map iteration in any order-sensitive path.

**Latent layer.**
- Parcels `P` are *given* (≈25 per tile). Their geometry is treated as survey ground truth; their *attributes* are assertions and go through ρ like anything else.
- Latent buildings `B = {b₁…b_K}`, `K = Σ_p NUMBLDGS(p)` where present, else the per-component max footprint count across sources, plus ⌈0.2K⌉ slack slots governed by an `atmost`. `K ≈ 60–80`.

**Variables.**

```
X_f  ∈ B ∪ {⊥}       for each observed footprint f      (Overture, FEMA, MS)      ~180
Y_q  ∈ B ∪ {⊥}       for each POI q                                                ~40
Pb_b ∈ P ∪ {∅}       parcel each slot sits on                                       ~80
A_b  ∈ [a_lo,a_hi]   integer footprint area, whole sq ft                            ~80
Fl_b ∈ [1,120]       floor count                                                    ~80
Lo_ℓ,Hi_ℓ ∈ ℤ        address-range endpoints per lot per block face                 ~50
Coll ⊆ P             the collateral parcel set            (ROBDD set variable)
QB   ⊆ B             the collateral building set          (ROBDD set variable)
```

`n_fd ≈ 260` finite-domain variables (domain-consistent propagation), `n_int ≈ 210` integer variables (bounds(ℤ) consistency, per Schulte & Stuckey 2005's criterion for when bounds and domain propagation coincide — for our monotone sum/knapsack structures they largely do, and where they don't we say so).

Domain size: `d_max = K ≈ 80` before geometric filtering; `d_typ ≈ 8` after. Carry both.

**Symmetry.** The slots `b₁…b_K` are interchangeable. `K!` symmetry destroys model counting outright (every solution appears `K!` times) and must be broken *completely and soundly*, not canonicalised after the fact. Two mechanisms, channelled together (Cheng, Choi, Lee & Wu 1999):

- **Representative encoding** for canonicity: a latent building is *identified with* the `≺`-least observation in its cluster. No anonymous slots, no symmetry, but weak propagation.
- **Slot encoding** for the strong global propagators, with **value precedence** (`precede`, Law & Lee 2004; Walsh 2006) breaking value interchangeability completely at GAC in O(nd).

*Cheap wrong way:* cluster, then sort the clusters and call them 1..k. *Silent error:* the count is wrong by a factor of `K!/(orbit size)`, so "3 candidates" and "3 million relabellings of 1 candidate" are indistinguishable, and any downstream ambiguity measure is noise.

---

## 2. Q1 — The strongest consistency we can actually afford

### 2.1 The ladder, with arithmetic

All figures at `n_fd = 260`, `e ≈ 4,500` binary constraints (each variable geometrically coupled to ~30 neighbours, not the full 33,670-edge clique), `d = 8` post-AC, `d = 80` pre-AC. Assume 3×10⁸ constraint checks/sec — deliberately conservative for Rust over 64-bit bitset domains.

| Level | Algorithm | Complexity | Ops here | Wall clock |
|---|---|---|---|---|
| Node consistency | trivial | O(nd) | 2.1×10³ | <1 µs |
| **Arc consistency** | AC-2001 / AC-3.1 (Bessière, Régin, Yap & Zhang 2005) | O(ed²) | 2.9×10⁵ @ d=8; 2.9×10⁷ @ d=80 | 1–100 ms |
| **GAC on globals** | Régin 1994/1996 (see §3) | see §3 table | <10⁶ per pass | ~10 ms to fixpoint |
| Restricted / max-RPC | Berlandier 1995; Debruyne & Bessière 1997 | O(end³) worst | ~10⁸ | ~0.3 s |
| **Singleton arc consistency** | SAC-Opt (Bessière & Debruyne 2005), O(end³) time / O(end) space; SAC-SDS for practice | O(end³) | 4,500 × 260 × 512 = **6.0×10⁸** | **0.3–2 s** |
| Path consistency, whole tile | PC-2001 (Bessière et al. 2005) | O(n³d³) | 260³ × 512 = **9.0×10⁹** | **30 s+** |
| Path consistency, *post-decomposition* | PC-2001 on components, n≈20, d≈5 | O(n³d³) per component | 10⁶ × 15 comps = **1.5×10⁷** | **~50 ms** |
| Strong 4-consistency, tile | Freuder 1978 | O(n⁴d⁴) | 260⁴ × 8⁴ = 1.9×10¹⁴ | dead |
| Strong 4-consistency, components | — | 20⁴ × 5⁴ × 15 = 1.5×10⁹ | ~5 s | affordable but **pointless** — see below |
| **Exact compilation, components** | reduced MDD / SDD, canonical | O(diagram size) | width ≤ 2¹²; 10³–10⁵ nodes/comp | **~0.2 s total** |

PC space is not the constraint people assume: at `d ≤ 64` a binary relation is a `d`-word bitmask, so the whole tile's relation table is `n²/2 × d²` bits = 33,670 × 64 bits ≈ 270 KB at d=8, or 27 MB at d=80. Storable. Composition `R_ik ∘ R_kj` is `d` word-ORs per row, so the nominal `d³` is really `d²` word-ops — a 64× constant-factor win that people forget exists. It is still 30 s on the raw tile because the `n³` dominates.

### 2.2 The real answer: the ceiling is not a consistency level

**Decomposition is free, and it is a byproduct we already pay for.** Régin's `alldifferent` GAC computes the strongly connected components of the value graph (Tarjan 1972) as an intrinsic step. Those SCCs *are* the tile's decomposition — a footprint can only be a slot within ~25 m, so the value graph is already a near-disjoint union of blocks. We do not need a separate tree-decomposition heuristic; the propagator hands us the components for free.

Typical component after slot-level geometric filtering: **6–20 variables, d ≤ 8**. Tail to ~40 on a dense assemblage.

On a component of that size, **exact compilation of the entire solution set is cheaper than path consistency on the tile, and it subsumes k-consistency for every k simultaneously.** A component's MDD state for a per-source `alldifferent` over ≤ 8 reachable slots is a subset of used slots: ≤ 2⁸ = 256 states per layer, 20 layers → ≤ 5,120 nodes. Milliseconds.

So the ladder is climbed only until exactness becomes cheaper than approximation, and at 200 features **that crossover is at k = 3**. The architecture is:

```
NC  →  AC-2001 + GAC on globals  (≈10 ms, tile-wide)
    →  SAC                        (≈0.3 s, tile-wide)   ← the level that earns its keep
    →  decompose (free, from Régin/Tarjan)
    →  exact MDD/SDD compilation per component (≈0.2 s)  ← subsumes all k-consistency
    →  PC on components, for the minimal network only     (≈50 ms) ← explanation artifact, not pruning
```

**Tile budget: ~0.5 s.** A spatial join is ~1 ms. We spend 500×, and that is the entire commercial thesis. At 10⁶ tiles that is ~140 CPU-hours, embarrassingly parallel — a few hundred dollars of compute for a national pass.

### 2.3 What each level buys that the level below cannot

This is the part people hand-wave. Concretely:

**AC over pairwise `≠` cannot see Hall sets.** Six MS GlobalML footprints, five geometrically admissible slots. Pairwise disequality with AC finds nothing — every value still has a support. Régin's GAC finds the wipeout immediately, because Hall's theorem (1935) violations are exactly what the SCC decomposition of the value graph detects. *This is a proof that MS over-segmented a roof ridge*, emitted for free.

**SAC buys the eliminations that require an assignment plus a numeric constraint.** Assume the collateral is lot A. Propagate. The knapsack propagator on `Σ_{b: Pb_b = A} A_b · Fl_b` cannot reach the asserted 214,300 sf even using every compatible footprint at maximum plausible floor count → wipeout → **lot A is eliminated with no threshold and no search.** Plain AC never sees this, because the sum is only violated *in combination with* the assignment, and AC is a unary domain filter. This case is common: it is the mechanism that kills undersized candidate lots that a geocode radius happens to touch.

**PC and SAC are incomparable as domain filters** (Debruyne & Bessière, *Domain filtering consistencies*, JAIR 14, 2001, which gives the full lattice `AC ≺ RPC ≺ maxRPC ≺ SAC` with PC sitting orthogonally as a *relation*-filtering consistency). PC's distinctive product is not extra domain pruning — it is the pairwise relation itself: *"if the collateral is lot A then the FEMA structure must be `f3`."* SAC can never represent that, because SAC only prunes unary domains.

**Therefore PC is demoted from a pruning technique to an explanation technique.** Post-decomposition, post-SAC, PC-2001 on components produces an approximation to Montanari's (1974) **minimal network** — the network whose binary relations are exactly the projections of the solution set. That is the human-readable pairwise summary of the residual. We run it, but for the report, not for the answer.

**Strong 4-consistency is affordable on components and worthless there,** because for the same ~5 s you can compile the component exactly and get every k at once, plus the count, plus the backbone. This is the honest verdict the ambition test demands: *rank k-consistency for k ≥ 4 down to zero.*

**Freuder's 1982 theorem gives us a per-tile certificate.** If a component's constraint graph has width `w` under the canonical ordering and the network is strongly `(w+1)`-consistent, search is backtrack-free — the propagation fixpoint *is* the solution set. We compute `w` per component and report it. A tile can therefore carry the line *"solved backtrack-free at width 2"*, which is a mathematical statement about that tile, not a QA note.

---

## 3. Q2 — Global constraints, their real propagators, and what they prune

Each row: the domain rule, the named global constraint it is an instance of, the algorithm, the complexity, and — the part that matters — **what it prunes that the naive pairwise encoding cannot.**

### 3.1 Exclusivity → `alldifferent_except_0`

- **Algorithm:** Régin, *A filtering algorithm for constraints of difference in CSPs*, AAAI 1994. Maximum bipartite matching by Hopcroft–Karp (1973) at O(m√n), then Tarjan SCC (1972) at O(m) to mark edges belonging to no maximum matching (Berge 1970 / Costa 1994: an edge is in some maximum matching iff it lies on an even alternating cycle or an even alternating path from a free vertex).
- **Here:** m = Σ|Dᵢ| ≈ 60 × 8 = 480 per source, √n ≈ 8 → ~4×10³ ops initial, O(m) incremental. Free.
- **Prunes what naive cannot:** all Hall-set violations. The pairwise `≠` decomposition is strictly weaker and provably so. The 6-footprints-into-5-slots contradiction above is invisible to it.
- **Gap vs. cheap way:** *large.* The cheap way is IoU > 0.5 pairwise dedup, whose silent error is transitive: A~B, B~C, A≁C produces clusters that depend on processing order, and over-segmented roofs get merged, deflating the building count with no signal.

### 3.2 Cardinality (`NUMBLDGS`, coverage rates) → `gcc`

- **Algorithm:** Régin, *Generalized arc consistency for global cardinality constraint*, AAAI 1996. Flow-based; GAC in O(n²d).
- **Here:** 175² × 8 ≈ 2.4×10⁵. Free.
- **Prunes what naive cannot:** **`gcc` propagates backwards from the count to the assignments.** If parcel `p` must host exactly 3 buildings, 4 slots are geometrically compatible, and one is forced onto `q`, `gcc` immediately forces the remaining 3 onto `p`. A naive count-and-compare check can only *validate* at the end, and its warning is discarded downstream. **`gcc` turns `NUMBLDGS` from a validation check into a generator of forced assignments.** That single sentence is most of the value of this section.
- Coverage rates enter here too: a declared per-county FEMA coverage floor becomes a `gcc` lower bound on how many slots must carry a FEMA observation.

### 3.3 Distinct building count → `nvalue` / `atmost_nvalue`

- **Algorithm:** Pachet & Roy 1999; Bessière, Hebrard, Hnich, Kızıltan & Walsh, *Filtering algorithms for the NValue constraint*, Constraints 11(4), 2006. Exact GAC is NP-hard; the standard filtering uses a maximum-independent-set lower bound on the "cannot-be-same" graph.
- **Here:** we do the NP-hard thing. Bron–Kerbosch (1973) with pivoting on a 20-node sparse geometric graph, or a 2²⁰ = 10⁶ bitmask DP. Milliseconds.
- **This is the thesis in miniature:** `nvalue` is NP-hard to propagate exactly, therefore every production CP system uses a bound, therefore nobody gets its full pruning. At 200 features the exact version is free. *We take the intractable branch because our universe is bounded.*

### 3.4 Additive size → `knapsack` / `bin_packing`, not "sum with tolerance"

- **Algorithm:** Trick, *A dynamic programming approach for consistency and propagation for knapsack constraints*, Annals of OR 118, 2003 — DP achieving **domain consistency** on an integer linear sum. Plus Shaw, *A constraint for bin packing*, CP 2004, for the packing-into-parcels view.
- **Complexity:** pseudo-polynomial O(n·C). With areas in 100-sq-ft units and `BLDGAREA ≈ 214,300` → C = 2,143, n = 8 slots → **1.7×10⁴ ops.** Free. Even in whole square feet it is 1.7×10⁶ — still free.
- **Prunes what naive cannot:** the naive form is `|Σ − BLDGAREA| / BLDGAREA < 0.15`, evaluated *after* an assignment is chosen. It is a threshold, it hides the gross/net and below-grade conventions, and it cannot prune. Trick's DP prunes: it removes from `A_b`'s domain every value that participates in no completion reaching the declared band. Under SAC it eliminates whole parcels (§2.3).
- **Gap:** *large.* And note the honest framing: the band is not a threshold, because a threshold *selects* and a band *restricts*. A wrong band produces an empty domain, which is detectable. A wrong threshold produces a wrong answer, which is not.

### 3.5 Non-overlap → `geost` / `diffn`

- **Algorithm:** Beldiceanu & Carlsson, *Sweep as a generic pruning technique applied to the non-overlapping rectangles constraint*, CP 2001; Beldiceanu, Carlsson, Poder, Sadek & Truchet, *A generic geometrical constraint kernel in space and time for handling polymorphic k-dimensional objects*, CP 2007.
- **Here:** trivial at 80 slots on integer coordinates.
- **Gap: honestly, medium-to-small.** A snapped-integer spatial join gets most of non-overlap on its own. **Rank this item down.** Its value is not standalone; it is that non-overlap plus `nvalue` plus `gcc` jointly force building counts that none of them force alone. Include it as an input to the cardinality reasoning, do not sell it.

### 3.6 Address along a block face → `disjunctive` scheduling on the house-number axis

This is the item that directly dissolves the failure that killed both prior architectures, and I think it is the single strongest technical idea in this document.

The parcel layer stores *one* address per lot. Corner lots and large lots have many. So the true answer is unreachable from string matching — that was the red team's finding, and it is correct **for per-row string matching**. It is not correct for the block face as a whole.

**Physical law:** along one side of one street between two cross-streets, house numbers increase monotonically in the along-street direction and share parity. That is not a heuristic; it is how municipal address grids are defined, and it is verifiable per municipality.

**Model.** For block face `F`, order the fronting lots by exact integer projection of their frontage onto the street centreline. Each lot ℓ gets integer interval variables `[Lo_ℓ, Hi_ℓ]` with:

```
Hi_ℓ  <  Lo_{ℓ+1}                            (strict, disjoint, order fixed by geometry)
Lo_ℓ ≡ Hi_ℓ ≡ p (mod 2)                      (block-face parity)
Lo_ℓ ≤ a ≤ Hi_ℓ      for every asserted address a on ℓ   (anchors, from any source)
⋃[Lo_ℓ,Hi_ℓ] ⊆ [block range]                 (from the street network's address range attributes)
∃! ℓ : Lo_ℓ ≤ a* ≤ Hi_ℓ    or   a* ∉ F       (the query address)
```

**This is a unary-resource scheduling problem.** The block face is a machine, the lots are non-overlapping tasks on the address axis with fixed precedences. So the propagator is **edge-finding**: Carlier & Pinson (1989, *Management Science* 35(2)) for the original job-shop algorithm, Vilím (2004, CPAIOR) for the Θ-tree O(n log n) form, Baptiste, Le Pape & Nuijten (*Constraint-Based Scheduling*, 2001) for the full treatment.

At 20 lots per face: ~90 operations. **Free.**

**What it buys, concretely.** The query is `199 First Avenue`. The true lot's `ADDRESS` field reads `355 East 12th Street` — a corner lot indexed on the cross street. No string match exists. But the lot at 195 First Ave and the lot at 203 First Ave are both anchored by Overture POIs, and our lot lies strictly between them in the frontage order. Edge-finding **proves** `199 ∈ [Lo, Hi]` for that lot. When the anchors do not bracket tightly, it returns the honest bracket — two lots — and the residual says so.

*Cheap wrong way:* TIGER-style linear interpolation between the segment's from/to address range. That is what every geocoder on earth does. *Silent error:* interpolation assumes uniform lot widths. Every block containing an assemblage violates that, and in dense urban fabric the interpolated point lands on the wrong lot at a rate somewhere in the 15–30% range **with no signal that it did** — the geocoder returns the same match-type string either way. *What exact buys:* a proof or an honest bracket, never a confident wrong point.

**Transplanting 1989 job-shop scheduling theory onto street addressing is exactly the kind of move the brief asks for: old, exact, well understood, and structurally unfashionable.** Rank: **highest gap of any item here.**

### 3.7 Address string parsing → `regular`

- **Algorithm:** Pesant, *A regular language membership constraint for finite sequences of variables*, CP 2004. GAC by DFA unfolding into a layered graph, O(n·|Q|·|Σ|). The unfolding *is* an MDD, so it composes with everything else in §5.
- **Prunes what naive cannot:** the naive way is `libpostal` — a CRF, i.e. a statistical model, i.e. nondeterministic across versions and uninterpretable — which **picks one parse**. Silent error: `"199 First Avenue, Unit 3B, a/k/a 355 East 12th Street"` gets one parse, the `a/k/a` is discarded, and the true answer is destroyed before the solver ever runs. With `regular` over a declared, versioned token grammar, **all parses stay alive as a domain** and the *other* constraints kill the wrong ones. Alternation handles `a/k/a` natively.
- **Gap: large.** And it removes the last statistical component from the decision path.

### 3.8 Temporal → Allen interval algebra / STP

- **Algorithm:** Allen, *Maintaining knowledge about temporal intervals*, CACM 26(11), 1983. Full IA satisfiability is NP-complete (Vilain & Kautz 1986), but our constraints (built-before, observed-after, demolished-before, securitised-at) are **pointizable**, hence in the ORD-Horn maximal tractable subclass (Nebel & Bürckert, JACM 42(1), 1995), where **path consistency decides satisfiability**. Most of it is a Simple Temporal Problem (Dechter, Meiri & Pearl, *Temporal constraint networks*, AI 49, 1991), solved exactly by Floyd–Warshall, which also yields the **minimal network** directly.
- **Here:** ~100 temporal entities → 10⁶ ops. Free.
- **What it buys:** MS GlobalML footprint from 2021 imagery, FEMA structure from 2019, parcel `YEARBUILT` 2020. A spatial join merges all three into one building. The temporal network **proves** the 2019 FEMA record cannot denote the same physical structure as the post-2020 construction, so the tile contains a **demolition-and-rebuild event** — which means the collateral described in the 2019 offering document no longer exists. That is a five-alarm finding for a CMBS analyst and it falls out for free from a 1983 paper.
- *Cheap wrong way:* `WHERE year_built <= 2019`. *Silent error:* it filters rows instead of detecting events, so the rebuild is invisible.
- **Gap: large, and dramatic.**

### 3.9 Set variables (`Coll`, assemblages) → ROBDD set domains

- **Algorithm:** Hawkins, Lagoon & Stuckey, *Solving set constraint satisfaction problems using ROBDDs*, JAIR 24, 2005 — **domain**-consistent set propagation, versus the set-bounds consistency of Gervet (1997) / Puget's Conjunto (1992).
- **Why this one:** it makes the set-variable store and the compiled solution space *the same technology*. The whole tile is decision diagrams end to end, so the artifact is homogeneous and the canonicity argument (Bryant 1986) covers everything.

### 3.10 Slot symmetry → `precede`; ordering → `lex_chain`

- Law & Lee, *Global constraints for integer and set value precedence*, CP 2004; Walsh, *Symmetry breaking using value precedence*, CP 2006 — GAC in O(nd), and it breaks value interchangeability **completely**, which post-hoc canonicalisation does not.
- Carlsson & Beldiceanu 2002 for `lex_chain` where lexicographic ordering is the right symmetry breaker.
- Crawford, Ginsberg, Luks & Roy, KR 1996, for the general theory.

### 3.11 Summary of one propagation pass

| Constraint | Global | Complexity | Ops here |
|---|---|---|---|
| exclusivity | `alldifferent_except_0` | O(m√n) init, O(m) incr | 4×10³ |
| counts, coverage | `gcc` | O(n²d) | 2.4×10⁵ |
| distinct buildings | `nvalue` (exact) | NP-hard; 2²⁰ DP here | ≤10⁶ |
| area | `knapsack` (Trick DP) | O(n·C) | 1.7×10⁴ |
| non-overlap | `geost` sweep | O(n log n)–O(n²) | ~10³ |
| address chain | `disjunctive` edge-finding | O(n log n) | ~90 |
| address parse | `regular` | O(n·|Q|·|Σ|) | ~10⁴ |
| temporal | STP / ORD-Horn PC | O(n³) | 10⁶ |
| symmetry | `precede` | O(nd) | 6.4×10² |
| **total per pass** | | | **< 3×10⁶** |

Hundreds of passes to fixpoint still lands under 10⁸. AC+GAC fixpoint: **~10 ms**.

---

## 4. Q3 — Explanations as a byproduct, and what determinism they cost

### 4.1 The paradigm

There are three real candidates and they have different cost profiles.

**(a) ATMS — de Kleer, *An assumption-based truth maintenance system*, AI 28(2), 1986.** Every derived datum carries a **label**: the set of minimal environments (sets of assumptions) under which it holds. Explanation is not a reporting layer; it *is* the data structure. Combined with de Kleer & Williams, *Diagnosing multiple faults*, AI 32(1), 1987 (GDE), you get diagnosis for free too.

*Honest cost:* labels are antichains of assumption sets and can blow up exponentially. With ~200 source records as assumptions per tile this is a real risk, not a theoretical one. **So do not run a full ATMS eagerly.**

**(b) QuickXplain — Junker, AAAI 2004.** Compute a *preferred* minimal explanation on demand, in **O(k log(n/k))** consistency checks for an explanation of size `k` from `n` constraints. At n ≈ 60 tile constraints and k ≈ 3: `3·log₂(20) ≈ 13` solver calls. Each call is ~10 ms against an already-propagated store. **~130 ms per explanation request, paid only when an operator clicks.** This is the right engineering answer and it is fully deterministic given a fixed constraint order — which the source-reliability ordering supplies.

**(c) Lazy Clause Generation — Ohrimenko, Stuckey & Codish, *Propagation via lazy clause generation*, Constraints 14(3), 2009.** Propagators explain themselves in clauses; the solver's resolution derivation is the proof. Layer a certificate on top with **VeriPB** (Gocht, McCreesh & Nordström, *An auditable constraint programming solver*, CP 2022; Bogaerts, Gocht, McCreesh & Nordström on certified symmetry and dominance breaking), which can certify global-constraint propagation *and* symmetry breaking — the two things a naive DRAT log cannot.

### 4.2 The architecture

- **Answer layer:** compile. No search, therefore no learned clauses, therefore no order-dependence at all.
- **Explanation layer:** QuickXplain on demand, ordered by declared source reliability. The artifact is a minimal set of named source records — *"lots 1012920026 and 1012920001 are separated by exactly: {FEMA structure `f3` SQMETERS = 3,240; MapPLUTO `NUMBLDGS(1012920026) = 2`; the First Avenue block-face anchor at 195}"*. That maps to human-readable prose by templating over named evidence atoms, and the map is total because every constraint carries its provenance by construction.
- **Certificate layer:** VeriPB proof log, emitted for the full run, independently checkable by a third party who does not trust our code.

### 4.3 The honest determinism cost

**Zero at the answer level, conditionally zero at the proof level.**

The fixpoint is confluent by Apt (1999) — monotone, contracting, correct propagators on a finite lattice have a unique greatest fixpoint (Tarski 1955; Cousot & Cousot 1977). Order-independence of the *answer* is a theorem and costs nothing.

The *proof artifact* is a different matter. Where we search rather than compile — large components exceeding the width budget — CDCL learned clauses depend on branching order, restart policy, and activity scores. Byte-identical proofs then require: canonical branching order from `≺`, restarts driven only by a deterministic counter (never wall clock), no PRNG without a fixed seed, and no propagator that reads external mutable state. Those are all achievable; they cost some performance and they must be enforced, because a single `HashMap` iteration in a propagator silently destroys the guarantee.

Note the precondition people skip: **confluence requires every propagator to be a monotone function of the domain store.** Any propagator using randomised rounding, sampling, or an early-exit budget is non-monotone and voids the theorem. That is a hard rule on our propagator library, not advice.

---

## 5. Q4 — The solver-native artifacts, and which one is actually the product

A constraint reasoner computes far more than a solution. Compiling to a **d-DNNF / SDD / reduced MDD** (Darwiche, *Decomposable negation normal form*, JACM 48(4), 2001; Darwiche & Marquis, *A knowledge compilation map*, JAIR 17, 2002; Darwiche, *SDD*, IJCAI 2011; Andersen, Hadžić, Hooker & Tiedemann, *A constraint store based on multivalued decision diagrams*, CP 2007; Bergman, Cire, van Hoeve & Hooker, *Decision Diagrams for Optimization*, Springer 2016) makes all of the following **linear or polynomial in the diagram size**.

| Artifact | Computation | Operator product |
|---|---|---|
| **Backbone** — values in every solution | one traversal | "Regardless of how the ambiguity resolves, this loan touches BBL 1012920026, GERS `08f2a3…`, and total collateral GLA ≥ 412,000 sf." **Lets a downstream system act on partial resolution.** |
| **Exact model count** | one bottom-up pass | A *calibration-free* ambiguity measure. Not a confidence score — a count. 1 = decided, 3 = three named alternatives, 0 = proof of source defect. |
| **Residual enumeration** | polynomial delay | The full alternative set, streamable. For pure matching sub-problems, Uno (ISAAC 1997) gives O(V) delay per matching; exact match counts come from Ryser's formula (1963) at O(2ⁿn) — n=12 → 4.9×10⁴ ops, free, though the permanent is #P-complete in general (Valiant 1979). *Intractable at scale, free at 200.* |
| **MUS** — minimal blame | QuickXplain, O(k log(n/k)) checks | "These five sources cannot all be right, and here is the smallest set that proves it, ordered so the least-trusted source is named first." |
| **MCS** — minimal repair | minimal hitting sets of the MUSes (Reiter, *A theory of diagnosis from first principles*, AI 32(1), 1987); enumeration via CAMUS (Liffiton & Sakallah, JAR 40, 2008) or MARCO (Liffiton, Previti, Malik & Marques-Silva, Constraints 21, 2016) | "Retract *either* {FEMA `f3` SQMETERS} *or* {MapPLUTO `NUMBLDGS`} and the tile becomes consistent. Nothing smaller works." **A repair recommendation, not an error message.** |
| **Value of information** | count reduction under each hypothetical new fact | "Buy the certificate-of-occupancy date from this vendor and 61% of your residual ambiguity across the portfolio collapses." Exact, because counting is exact. |
| **Minimal network** (Montanari 1974) | PC on the residual component | The human-readable pairwise summary: *"if lot A then FEMA `f3`; if lot B then FEMA `f7` and the POI is a tenant not the owner."* |
| **Certified refinement** | entailment test between last year's diagram and this year's — polytime on SDDs sharing a vtree | "Here is a machine-checkable proof that our 2027 answer *refines* our 2026 answer and contradicts nothing." |

### The ranking, committed

**Contractual output (build first): the pair (backbone, exact count).** Everything else derives from the same compiled form, so this is nearly free once the compiler exists, and it is the pair that goes in the SLA. It is what converts abstention from a failure into a deliverable: a consumer can safely act on the backbone and safely defer on the residual, and both are precise.

**Highest-margin single artifact: the ordered MCS lattice.** I rank this #1 overall and I will defend it. Backbone can be *approximated* — a competitor with a good probabilistic model can produce a "high confidence subset" that is usually right, and "usually right" sells. **MCS has no approximation.** There is no statistical proxy for "the minimal set of retractions that restores consistency." It is also the only artifact with a buyer *other than* the person who asked the question — the data vendor, the trustee, the risk committee — and the only one that **improves the input corpus rather than consuming it**, so it compounds.

**Compounding moat: value of information.** Once counting is exact, VOI is exact, and exact VOI turns data acquisition from a procurement guess into an optimisation. Over three years this is the thing that makes the corpus asymmetric.

**Regulatory: the certified refinement proof.** In CMBS specifically, "we can hand the trustee a proof that our restatement is a refinement and not a revision" is worth more than it sounds.

---

## 6. Q5 — Where the frame breaks, answered honestly

The brief asks whether admitting softness destroys confluence and determinism. **It does, for one specific and very popular kind of softness, and there is a real theorem that says exactly which.**

### 6.1 The theorem

Semiring-based CSP (Bistarelli, Montanari & Rossi, *Semiring-based constraint satisfaction and optimization*, JACM 44(2), 1997) generalises both hard and soft constraints. The relevant result:

> **Soft constraint propagation is confluent and reaches a unique fixpoint iff the semiring's combination operator × is idempotent (a × a = a).**

- **Fuzzy / possibilistic CSP** — semiring `⟨[0,1], max, min⟩`. `min` is idempotent. **Confluent. Unique fixpoint. Safe.**
- **Weighted CSP** — semiring `⟨ℕ ∪ {∞}, min, +⟩`. `+` is *not* idempotent. Soft arc consistency (Cooper & Schiex, *Arc consistency for soft constraints*, AI 154, 2004; Larrosa & Schiex, *Solving weighted CSP by maintaining arc consistency*, AI 159, 2004) requires equivalence-preserving transformations, and **the fixpoint reached depends on the order in which they are applied.**

So: **weights destroy confluence; idempotent softness does not.** That is the precise, citable line, and it means the answer to "can we just add reliability weights?" is **no, and here is the paper.**

### 6.2 What we do instead

Softness lives in **three** places, none of which is the solver:

**(1) In ρ — the declared, versioned, falsifiable relaxation (§1.1).** `BLDGAREA` is gross; the asserted figure may be net. That is not modelled as fuzziness, it is modelled as two hard relations plus a declared band `[0.78, 0.95]` for `BLDGCLASS ∈ O*`. The band is a *modelling commitment* with a version number and a citation, and the crucial distinction is:

> **A threshold selects. A band restricts.** A wrong threshold silently produces a wrong answer. A wrong band produces an empty model set — which is a detected, attributable, reportable failure. **The system audits its own error models.**

The price is real and I will name it: **wider bands mean larger residuals.** We resolve fewer tiles to a singleton than a competitor willing to guess. That price is paid in abstention, which is a first-class output, and it is the correct trade for this asset class.

**(2) In the presentation ranking.** Genuine preferences — "prefer the interpretation with a plausible loan-to-value," "prefer the larger of two candidate assemblages" — are applied to the **already-enumerated finite residual**, as a sort with a canonical total order and canonical tie-breaking. Sorting a finite enumerated set is confluent by construction. The solver never sees the preference.

> **Rule: preferences rank; constraints prune. Never mix.**

**(3) Reliability, which is not a weight.** Source reliability enters in exactly two sound places: it sets the *width* of that source's ρ band, and it supplies the *preference order* handed to QuickXplain, which determines which MUS the operator is shown first. **Reliability never weights a decision. It widens a band and orders a report.**

### 6.3 The one legitimate escape hatch

If we ever need genuinely soft *pruning*, the licensed form is **fuzzy/possibilistic CSP over a finite, totally ordered, integer scale of named trust levels** — not floats. Idempotency gives confluence (Bistarelli–Montanari–Rossi), integer levels give byte-determinism, and the α-cut family is *nested and monotone in α*, so it is another accretion-friendly object rather than a threshold. Fargier, Lang & Schiex (1993) on leximin refinements is where you would go if max-min is too coarse — but note leximin is **not** idempotent, so that refinement costs the confluence theorem. Take it only with eyes open.

### 6.4 The break, stated plainly

The frame breaks if you want a single best answer under conflicting evidence with source-specific weights. There is no confluent, deterministic way to do that. **The fallback is not a weighted optimum; it is MCS enumeration.** Enumerate all maximal satisfiable subsets and present them. If there is a unique **cardinality**-minimal correction, that is a canonical answer with no weights involved. If there is a tie, report the tie — abstention again. **Cardinality-minimal is confluent; weight-minimal is not.** That rule is the whole of our over-constrained policy.

---

## 7. Q6 — The ambition test

| # | Item | Cheap wrong way | Silent error it produces | What exact buys | Gap |
|---|---|---|---|---|---|
| 1 | **Block-face address chain** (§3.6) | TIGER linear interpolation between segment endpoints | Assumes uniform lot widths; on assemblage blocks lands on the wrong lot at a high rate with an *identical* match-type string either way | Proof of lot, or an honest two-lot bracket. Reaches answers unreachable from any string match, killing the red team's core objection | **Highest** |
| 2 | **MUS/MCS** (§5) | "row failed validation" | No attribution, so the same defective vendor record poisons every rerun forever | Minimal, preference-ordered blame plus a minimal repair. No statistical proxy exists | **Highest** |
| 3 | **Exact count + backbone** (§5) | "return top-1 with a score" | The score is uncalibrated and hides that candidate #2 was equally good; downstream consumers cannot distinguish decided from ambiguous | A count, and a set of facts true in every solution — actionable under partial resolution | **Highest** |
| 4 | **Integer exact geometry** (§1.2) | `ST_Contains`, double precision | Boundary-straddling footprints resolve differently on ARM vs x86 and across GEOS point releases; reruns differ with no detection | Byte-identity across platforms and decades, by construction | **High** |
| 5 | **`regular` address parsing** (§3.7) | libpostal CRF picks one parse | The `a/k/a` clause is discarded before the solver runs; the true answer is destroyed upstream and invisibly | All parses survive as a domain; other constraints kill the wrong ones. Removes the last statistical component from the decision path | **High** |
| 6 | **Temporal network** (§3.8) | `WHERE year_built <= 2019` | Filters rows instead of detecting events; a demolition-and-rebuild between vintages is merged into one building | Proves vintage inconsistency → detects that the collateral in the offering document no longer exists | **High** |
| 7 | **`gcc` on NUMBLDGS** (§3.2) | count-and-warn | The warning is discarded; the count never forces the correct assignment | Turns a count into a generator of forced assignments | **High** |
| 8 | **GAC `alldifferent`** (§3.1) | pairwise IoU > 0.5 dedup | Non-transitive merges depend on row order; over-segmented roofs silently merge and deflate building count | Hall-set detection → *proof* that a source over-segmented | **High** |
| 9 | **Knapsack DP** (§3.4) | `|Σ−BLDGAREA|/BLDGAREA < 0.15` | A threshold, applied after the fact, that hides gross/net and below-grade conventions and cannot prune | Domain-consistent pruning under a declared falsifiable band; under SAC it eliminates whole parcels | **Medium-high** |
| 10 | **Canonical compilation (Bryant)** (§8) | "we run the same code, so we get the same answer" | Untrue: hash iteration order, float, and library versions all leak. Discovered years later, in an audit | Canonicity is a property of the reduced ordered diagram, not of our discipline | **High** |
| 11 | **Exact `nvalue`** (§3.3) | independent-set lower bound (what every CP system ships) | Under-prunes; some contradictions never surface | Exact NP-hard propagation, free at 20 nodes | **Medium** |
| 12 | **SAC** (§2.3) | AC only | Misses every elimination requiring an assignment *plus* a numeric constraint — the common undersized-lot case | Threshold-free, search-free parcel elimination | **Medium** |
| 13 | **Path consistency as pruning** | nobody does it | — | **Almost nothing over SAC + compilation. Ranked down.** Its real value is the minimal network for explanation, and it is only affordable *after* decomposition | **Low as pruning; medium as explanation** |
| 14 | **`geost` non-overlap** (§3.5) | `ST_Intersects` with a buffer | Topology noise, CRS/precision divergence | A snapped-integer spatial join gets most of this. **Ranked down.** Value is as an input to cardinality reasoning, not standalone | **Low** |
| 15 | **Loan-balance plausibility** | ignore it | — | Genuinely soft and usually weak. **Ranked down.** But when it fires it fires hard: a $60M loan cannot be collateralised by a 4,000 sf lot with a two-storey walkup, and that is a proof | **Low, occasionally decisive** |
| 16 | **Cohomological framing** (§8.4) | — | — | Beautiful, real, and the *correct name* for tile-boundary gluing failure. But the algorithm remains "compile and conjoin." **Ranked down as engineering, up as vocabulary.** Do not sell it | **Low as engineering** |

Four items ranked down, one to near-zero. That is the honest shape of the answer.

---

## 8. Q7 — The formally characterized object

### 8.1 The Tile Certificate

The resolution of a tile is not a pipeline output. It is an object `T(h, S, V)` — H3 index `h`, source snapshot `S`, model version `V` — with these components:

```
T.evidence     canonically ordered, content-addressed set of source atoms + their ρ-images
T.diagram      reduced ordered MDD/SDD over the tile's variables under the
               vtree canonically derived from h.  ← THE OBJECT
T.views        backbone | exact count | residual domains | minimal network (arity-2 projection)
T.blame        if unsatisfiable: the complete MUS lattice and its MCS dual,
               ordered by declared source reliability
T.proof        VeriPB log certifying T.diagram against T.evidence
T.width        per-component induced width w* and achieved consistency level k;
               "backtrack-free at width w" where k ≥ w+1  (Freuder 1982)
T.hash         BLAKE3 of the canonical serialisation of the above
```

### 8.2 The properties, as theorems

1. **Canonicity.** For a fixed variable order, the reduced ordered decision diagram of a Boolean/finite-domain function is unique (Bryant 1986; Darwiche 2011 for SDDs with a fixed vtree). Since the order derives deterministically from `h` and `≺`, **`T.hash` is a function of `(h, S, V)` alone.** Two runs, two continents, two architectures, one hash. Byte-identical determinism is a property of the data structure, not an engineering aspiration.

2. **Monotone refinement.** Adding a source conjoins constraints, so `Models(T ∧ c) ⊆ Models(T)`. Set inclusion. And it is *checkable*: entailment between two SDDs over a shared vtree is polytime (Darwiche & Marquis 2002). **We can emit a machine-checkable proof that this year's tile refines last year's.** The accretion claim stops being a promise and becomes a certificate.

3. **Confluence.** Apt (1999), grounded in Tarski (1955). Conditional on every propagator being monotone, contracting, and correct — which is a checkable property of our propagator library, and which rules out anything sampled, randomised, or budget-truncated.

4. **Structural abstention with a two-sided bracket.** When a component exceeds the width budget, we do not silently degrade. We build **both** a relaxed and a restricted diagram (Bergman, Cire, van Hoeve & Hooker 2016) and report the bracket:

   | | Guarantee |
   |---|---|
   | Relaxed (width-capped over-approximation, `S ⊆ R`) | A value on no path of `R` is in no solution → **sound pruning**. A value on every path of `R` is in every solution → **sound backbone** (a subset of the true backbone; never a false one). `R = ∅ ⟹ S = ∅` → **sound infeasibility proof**. `|R| ≥ |S|` → **upper bound on count**. |
   | Restricted (width-capped under-approximation, `S′ ⊆ S`) | Any path is a real solution → **sound satisfiability**. `|S′| ≤ |S|` → **lower bound on count**. |

   Every guarantee points the same direction: we may under-claim resolution, never over-claim. The gap between the two is a **certified measure of what we do not know**, and the tile is flagged width-limited. No silent caps, ever.

5. **Compositionality across tile boundaries.** A property straddling a tile edge composes by conjoining the two certificates on shared boundary variables. Conjunction of SDDs over a shared vtree is polytime, so **tiles compose without recomputation.** The atlas is built once and queried by conjunction.

### 8.3 What this makes categorically different

Everyone in this market computes a *row*. We compute a *canonical object with a hash whose queries include the row*.

The row is one derived view (the backbone, when the count is 1). But so are: the honest alternative set, the exact ambiguity count, the minimal blame set when the sources contradict, the minimal repair, the exact value of the next field we could buy, and a proof that next year's answer refines this year's. **None of those are additional features. They are all `O(|T.diagram|)` traversals of the same object.** Once the object exists, they are free; without it, each one is a separate research project that nobody will fund.

That is the categorical claim: **we are not resolving addresses better. We are producing a different kind of artifact, and address resolution is a query against it.**

### 8.4 The honest footnote on the boundary problem

There is a real subtlety in §8.2(5) worth naming precisely, because it has a name. A collection of tiles can be pairwise consistent on every overlap and yet admit no global assignment. That is not a bug; it is exactly the sheaf-theoretic notion of **contextuality**, and the obstruction is a Čech cohomology class (Abramsky & Brandenburger, *The sheaf-theoretic structure of non-locality and contextuality*, New J. Physics 13, 2011; Abramsky, Barbosa, Kishida, Lal & Mansfield, *Contextuality, cohomology and paradox*, CSL 2015; and the CSP connection explicitly in Abramsky, Gottlob & Kolaitis, *Robust constraint satisfaction and local hidden variables in quantum mechanics*, IJCAI 2013). A tile that is arc-consistent, path-consistent, and globally unsatisfiable is a contextual scenario, and the cohomological obstruction is a certificate of that fact.

I include this because it is the *correct definition* of the local-agreement-without-global-agreement failure and having the right definition matters. I rank it down as engineering because the algorithm is still "compile and conjoin," and I would not put it in a sales deck.

---

## 9. Build order, and what I would cut

**Build, in this order:**

1. Integer exact geometry with per-H3 shipped projection constants. Nothing else is deterministic without it, and it is a week.
2. The ρ registry: per-source, per-attribute, versioned relaxation operators with citations. This is the intellectual core of the product and it is mostly writing, not coding.
3. AC-2001 + Régin `alldifferent`/`gcc` + Trick knapsack DP + `precede`. Gets the fixpoint and, for free, the decomposition.
4. **The block-face address chain with Vilím edge-finding.** Highest-gap item; do it early because it is what makes the demo unanswerable.
5. Per-component MDD compilation → backbone + exact count. The contractual output.
6. QuickXplain on demand, ordered by reliability. The explanation layer.
7. MUS/MCS enumeration (MARCO). The highest-margin artifact.
8. SAC. Cheap, and it earns its 0.3 s.
9. VeriPB proof emission. Do this before a regulator asks, not after.
10. Value-of-information over the compiled forms. The compounding moat.

**Cut or defer:** strong k-consistency for k ≥ 4 (subsumed by compilation, ranked to zero); path consistency as a pruning technique (keep only post-decomposition, for the minimal network); `geost` as a headline (fold it in as an input to cardinality reasoning); the cohomological framing (keep the vocabulary, ship no code for it); any form of weighted CSP (it costs the confluence theorem — see §6.1 — and there is no version of this product worth that trade).

---

## Bibliography

**Foundations of constraint propagation.** Waltz, *Understanding line drawings of scenes with shadows*, in Winston (ed.), *The Psychology of Computer Vision*, 1975 (thesis 1972). Montanari, *Networks of constraints: fundamental properties and applications to picture processing*, Information Sciences 7(2):95–132, 1974. Rosenfeld, Hummel & Zucker, *Scene labeling by relaxation operations*, IEEE Trans. SMC-6(6):420–433, 1976. Mackworth, *Consistency in networks of relations*, AI 8(1):99–118, 1977. Freuder, *Synthesizing constraint expressions*, CACM 21(11):958–966, 1978. Freuder, *A sufficient condition for backtrack-free search*, JACM 29(1):24–32, 1982. Tarski, *A lattice-theoretical fixpoint theorem and its applications*, Pacific J. Math 5(2):285–309, 1955. Cousot & Cousot, POPL 1977. Apt, *The essence of constraint propagation*, TCS 221(1–2):179–210, 1999.

**Consistency algorithms.** Mohr & Henderson, *Arc and path consistency revisited*, AI 28(2):225–233, 1986. Han & Lee, AI 36(1):125–130, 1988 (PC-4). Bessière, *Arc-consistency and arc-consistency again*, AI 65(1):179–190, 1994 (AC-6). Chmeiss & Jégou, IJAIT 7(2), 1998 (PC-8). Bessière, Régin, Yap & Zhang, *An optimal coarse-grained arc consistency algorithm*, AI 165(2):165–185, 2005. Berlandier, IEEE CAIA 1995 (RPC). Freuder & Elfe, AAAI 1996 (NIC). Debruyne & Bessière, IJCAI 1997 (SAC-1); *Domain filtering consistencies*, JAIR 14:205–230, 2001. Bessière & Debruyne, *Optimal and suboptimal singleton arc consistency algorithms*, IJCAI 2005.

**Structure.** Dechter & Pearl, *Network-based heuristics for constraint-satisfaction problems*, AI 34(1):1–38, 1987. Gottlob, Leone & Scarcello, *Hypertree decompositions and tractable queries*, JCSS 64(3):579–627, 2002.

**Global constraints.** Régin, AAAI 1994 (`alldifferent`); AAAI 1996 (`gcc`). Hall, J. London Math. Soc., 1935. Hopcroft & Karp, SIAM J. Comput. 2(4):225–231, 1973. Tarjan, SIAM J. Comput. 1(2):146–160, 1972. Pachet & Roy, CP 1999; Bessière, Hebrard, Hnich, Kızıltan & Walsh, *Filtering algorithms for the NValue constraint*, Constraints 11(4), 2006. Trick, *A dynamic programming approach for consistency and propagation for knapsack constraints*, Annals of OR 118:73–84, 2003. Shaw, *A constraint for bin packing*, CP 2004. Beldiceanu & Carlsson, CP 2001; Beldiceanu, Carlsson, Poder, Sadek & Truchet, CP 2007 (`geost`). Pesant, *A regular language membership constraint for finite sequences of variables*, CP 2004. Carlsson & Beldiceanu, `lex_chain`, SICS T2002:18, 2002. Law & Lee, CP 2004; Walsh, CP 2006 (value precedence). Crawford, Ginsberg, Luks & Roy, KR 1996. Cheng, Choi, Lee & Wu, Constraints 4(2), 1999 (channelling). Gervet, Constraints 1(3):191–244, 1997; Hawkins, Lagoon & Stuckey, JAIR 24:109–156, 2005 (ROBDD set domains). Schulte & Stuckey, TOPLAS 27(3), 2005.

**Scheduling, transplanted.** Carlier & Pinson, *An algorithm for solving the job-shop problem*, Management Science 35(2):164–176, 1989. Baptiste, Le Pape & Nuijten, *Constraint-Based Scheduling*, Kluwer, 2001. Vilím, *O(n log n) filtering algorithms for unary resource constraint*, CPAIOR 2004.

**Temporal.** Allen, CACM 26(11):832–843, 1983. Vilain & Kautz, AAAI 1986. van Beek & Cohen, Computational Intelligence 6:132–144, 1990. Dechter, Meiri & Pearl, *Temporal constraint networks*, AI 49:61–95, 1991. Nebel & Bürckert, JACM 42(1):43–66, 1995 (ORD-Horn).

**Explanation and diagnosis.** de Kleer, *An assumption-based TMS*, AI 28(2):127–162, 1986. Reiter, *A theory of diagnosis from first principles*, AI 32(1):57–95, 1987. de Kleer & Williams, *Diagnosing multiple faults*, AI 32(1):97–130, 1987. Jussien & Ouis, *User-friendly explanations for constraint programming*, 2001. Junker, *QuickXplain*, AAAI 2004. Liffiton & Sakallah, JAR 40(1):1–33, 2008 (CAMUS). Liffiton, Previti, Malik & Marques-Silva, Constraints 21(2):223–250, 2016 (MARCO). Ohrimenko, Stuckey & Codish, *Propagation via lazy clause generation*, Constraints 14(3):357–391, 2009. Gocht, McCreesh & Nordström, *An auditable constraint programming solver*, CP 2022; Bogaerts, Gocht, McCreesh & Nordström, certified symmetry and dominance breaking, 2023.

**Compilation and counting.** Bryant, *Graph-based algorithms for Boolean function manipulation*, IEEE ToC C-35(8):677–691, 1986. Minato, DAC 1993 (ZDD). Darwiche, *Decomposable negation normal form*, JACM 48(4):608–647, 2001. Darwiche & Marquis, *A knowledge compilation map*, JAIR 17:229–264, 2002. Darwiche, *SDD*, IJCAI 2011. Andersen, Hadžić, Hooker & Tiedemann, CP 2007. Bergman, Cire, van Hoeve & Hooker, *Decision Diagrams for Optimization*, Springer 2016. Ryser, *Combinatorial Mathematics*, Carus Monograph 14, 1963. Valiant, *The complexity of computing the permanent*, TCS 8(2):189–201, 1979. Uno, ISAAC 1997. Bron & Kerbosch, CACM 16(9):575–577, 1973. Monasson, Zecchina, Kirkpatrick, Selman & Troyansky, Nature 400:133–137, 1999 (backbone).

**Softness.** Schiex, Fargier & Verfaillie, *Valued constraint satisfaction problems*, IJCAI 1995. Bistarelli, Montanari & Rossi, *Semiring-based constraint satisfaction and optimization*, JACM 44(2):201–236, 1997. Fargier, Lang & Schiex, EUFIT 1993. Cooper & Schiex, *Arc consistency for soft constraints*, AI 154(1–2):199–227, 2004. Larrosa & Schiex, AI 159(1–2):1–26, 2004. Petit, Régin & Bessière, soft global constraints, CP 2001.

**Framing.** Abramsky & Brandenburger, New J. Physics 13:113036, 2011. Abramsky, Gottlob & Kolaitis, IJCAI 2013. Abramsky, Barbosa, Kishida, Lal & Mansfield, CSL 2015. Shewchuk, Discrete & Computational Geometry 18:305–363, 1997.
