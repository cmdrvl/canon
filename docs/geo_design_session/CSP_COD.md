# CSP_COD: Tile Resolution as a Proof-Producing Finite Constraint Object

This is the strongest defensible form of the replacement architecture:

The output of a tile resolver is not a best match. It is a finite mathematical
object:

```text
TileResolutionCertificate =
  exact universe
  + named hard constraints
  + proof-producing propagator fixpoint
  + compact representation of all remaining models
  + forced facts, residual alternatives, contradiction cores, and repair sets
```

The closest intellectual lineage is not entity resolution. It is:

- Montanari 1974 and Mackworth 1977 constraint networks
- Waltz 1975 filtering for scene labeling
- Freuder 1978/1982 k-consistency and backtrack-free search
- Régin 1994/1996 matching and flow propagators for global constraints
- Tarski 1955 and Cousot/Cousot 1977 fixpoint semantics
- Bryant 1986 BDDs, Minato 1993 ZDDs, Darwiche 2001 d-DNNF knowledge compilation
- Doyle 1979 and de Kleer 1986 truth maintenance
- Junker 2004 QuickXplain for minimal conflict explanation
- Schiex/Fargier/Verfaillie 1995 valued CSPs and Bistarelli/Montanari/Rossi 1997 semiring CSPs for the soft layer

The categorically different version is this:

> A tile is compiled into a finite, proof-carrying CSP whose residual solution
> set is the deliverable. Resolution, abstention, contradiction, count,
> forced facts, and repair advice are projections of the same exact object.

No candidate proposer exists. Address, geometry, ownership, area, temporal
availability, source exclusivity, building counts, and debt plausibility are all
constraints over the same bounded universe.

## 0. The Formal Object

For one H3 tile plus deterministic halo, define a finite CSP:

```text
N = (X, D, C, E)

X: finite variables
D: finite domains for each variable
C: named constraints with deterministic propagators
E: evidence atoms, each with stable ids and exact values
```

The variables are not merely "which parcel is this row?" That is too small. The
right variable families are:

- `CollateralParcels`: set variable over parcel ids in the tile/hard halo.
- `CollateralBuildings`: set variable over latent building slots.
- `FeatureToBuilding[source_feature]`: source footprint or POI maps to one
  latent building slot or `absent`.
- `BuildingToParcel[building_slot]`: each extant latent building occupies one
  parcel, or a small set only when straddling is explicitly permitted.
- `FeatureToParcel[source_feature]`: derived/inverse view for footprints, POIs,
  and address observations.
- `ParcelSelected[parcel]`: Boolean membership in the collateral set.
- `BuildingSelected[building]`: Boolean membership in the collateral building
  set.
- `NamespaceId[entity, namespace]`: exact external identifier or absent.
- `ExistenceInterval[entity]`: exact temporal interval or bounded unknown.
- `AddressToken[k]`: finite-state parse tokens and normalized exact literals
  from the input string.
- `DebtSupportMode`: optional financial feasibility mode, hard only when the
  rule is contractually crisp.

The domain size ceiling is about 200 physical features, but the solver should
not equate "feature" with "entity." A latent building slot can be observed by
Overture, FEMA, Microsoft, and POI evidence simultaneously. A parcel may have
many legitimate addresses even if the parcel layer stores one representative
`ADDRESS`.

This matters for monotonicity. If a later source is allowed to add brand-new
values to old variables, old residual domains can expand. The monotonic claim is
true only if the universe is represented as a stable finite lattice:

- parcels are the authoritative parcel candidates in tile plus deterministic
  halo;
- buildings are bounded latent slots plus source observations assigned to those
  slots;
- new sources add variables and constraints, and may refine latent slots, but
  old claims are compared by projection onto the old variable set.

With that discipline:

```text
Models(C + new_constraints) subseteq Models(C)
```

So any fact true in all old models remains true in all new models unless the new
model set is empty. An empty model set is not a wrong answer; it is a proof that
the evidence set is mutually inconsistent.

## 1. Strongest Consistency We Can Afford

Plain arc consistency is the floor. The real ceiling is:

1. domain consistency for all applicable global constraints;
2. arc consistency on the binary residue;
3. path consistency on sparse typed components and small dense components;
4. singleton arc consistency on the post-global residual;
5. strong 4-consistency only on small separators/components;
6. exact residual compilation or exact model counting when the compiled state is
   within budget.

The important point: higher local consistency is not the primary weapon.
Dedicated global propagators buy more pruning per CPU cycle than generic
`k`-consistency. Generic strong `k`-consistency becomes intractable quickly;
global filtering, flow, subset-sum DP, and knowledge compilation are where the
ambition belongs.

### 1.1 Cost Model at 200 Features

Let:

```text
n = variables in one connected component
d = maximum domain size
e = binary constraints in the component
W = ceil(d / 64) machine words per bitset domain
```

For the crude dense upper bound:

```text
n = 200
d = 200
W = 4
e = n(n - 1) / 2 = 19,900
```

A binary relation as a dense bit matrix costs:

```text
d * d bits = 40,000 bits = 5,000 bytes
19,900 relations = about 99.5 MB
```

That is large but not scary. It means we can afford bitset relations,
explanation handles, and repeated propagation per valuable tile.

A full arc-consistency sweep checks each value against each neighbor:

```text
e * d * W = 19,900 * 200 * 4 = 15,920,000 word intersections
```

Even several sweeps are cheap in optimized Rust. AC-4 style support counters
are more dangerous: `e * d^2 = 796,000,000` support cells. At 2 bytes per
counter that is already about 1.6 GB before overhead. So the right practical
baseline is bitset AC-3/AC-2001 style "last support" or sparse watched-support
propagation, not naive AC-4 counters.

Path consistency is different. Revising relation `R_ij` through a third
variable `k` needs a boolean matrix composition:

```text
R_ij := R_ij intersect (R_ik o R_kj)
```

With bitsets, one dense composition costs about:

```text
d * d * W = 200 * 200 * 4 = 160,000 word operations
```

All ordered triples at `n = 200` cost:

```text
n^3 * d^2 * W = 8,000,000 * 160,000
                  = 1.28e12 word operations
```

That is not a default per-tile operation. It is a high-value forensic operation
or a sparse-component operation.

But the actual graph should not be dense. Most constraints are typed:

- building-to-parcel containment;
- source-footprint exclusivity;
- address-to-frontage;
- parcel-to-owner;
- parcel/building area sums;
- temporal compatibility;
- identifier equality.

If the average binary degree after global constraints is 12:

```text
oriented triples about n * r * (r - 1) = 200 * 12 * 11 = 26,400
26,400 * 160,000 = 4.224e9 word operations
```

That is expensive but credible for a valuable tile, especially because many
relations are sparse and many domains are much smaller than 200 after global
filtering.

For a dense residual component with `n = 40, d = 50`:

```text
n^3 * d^2 * W = 64,000 * 2,500 * 1 = 160,000,000 word operations
```

That is clearly affordable.

For `n = 50, d = 80`:

```text
W = 2
125,000 * 6,400 * 2 = 1.6e9 word operations
```

This is still plausible for a high-value tile if implemented with bitsets and
good cache behavior.

Singleton arc consistency runs AC after temporarily assigning each remaining
value:

```text
SAC trials = sum_x |D_x|
worst dense = 200 * 200 = 40,000 AC runs
```

Full dense SAC is expensive:

```text
40,000 * 15.9e6 = 6.36e11 word intersections
```

But after global filtering, a realistic residual might be:

```text
n = 120
average d = 20
trials = 2,400
```

If one propagation run costs 1e6 to 1e7 low-level operations, SAC costs
2.4e9 to 2.4e10 operations. That is heavy but defensible for this product:
the tile is small, the value per tile is high, and trials parallelize perfectly.

Strong 4-consistency over the full dense network is not defensible:

```text
number of triples = C(200, 3) = 1,313,400
tuples per triple at d = 200: d^3 = 8,000,000
raw triple relation cells = about 1.05e13
```

Even bit-packed, that is more than a terabyte-scale object before algorithms.
General strong 4-consistency is not the ceiling. Local strong 4-consistency on
small separators is.

For `n = 30, d = 20`:

```text
C(30, 3) * 20^3 = 4,060 * 8,000 = 32,480,000 tuple cells
```

That is fine. So the rule is:

```text
Run strong 4-consistency only where the induced component or separator has
n <= about 30 and effective d <= about 20, or where a specialized global
constraint makes the arity cheap.
```

### 1.2 What Arc Consistency Buys

Arc consistency removes values that have no support in a neighboring variable.
Mackworth's AC-3 is the classic 1977 algorithm; Mohr and Henderson's AC-4
from 1986 made support accounting explicit; Bessiere's AC-6 and AC-2001 family
reduced repeated support search.

In this problem AC catches:

- a parcel candidate outside every possible building containment relation;
- an address token that has no compatible frontage candidate;
- a FEMA footprint that cannot be assigned to any parcel after temporal pruning;
- a namespace id that violates a functional dependency;
- a geometry relation where a point is inside no remaining parcel.

Cheap wrong way:

```text
point-in-polygon, nearest parcel, then SQL join attributes
```

Silent error:

The nearest parcel can have a plausible owner or size while the true parcel
remains unreachable because address was treated as a proposer.

Exact expensive way:

AC never chooses. It deletes only unsupported values and leaves a residual set
when multiple values remain.

### 1.3 What Path Consistency Buys

Path consistency removes pair assignments that cannot be extended through a
third variable. Montanari 1974 and Mackworth 1977 are the key references.

Formally, for every `(a, b)` in relation `R_ij`, and every third variable `k`,
there must exist `c in D_k` such that:

```text
(a, c) in R_ik
(c, b) in R_kj
```

This catches ambiguity that pairwise AC cannot. Example:

```text
P = candidate parcel
B = candidate building
A = address/frontage interpretation
```

AC may keep:

- parcel `P1`, because it has some compatible building;
- building `B7`, because it has some compatible parcel;
- address interpretation `A_first_199`, because some parcel fronts First Ave.

But path consistency can delete `(P1, B7)` if no address interpretation supports
both the parcel frontage and the building/parcel containment relation.

Cases PC separates that AC cannot:

- roadbed geocode says parcel A or B; address frontage only works with A; a
  building candidate only works with B. Each value has pairwise support, but the
  triple has none.
- POI lies in an Overture building; Overture building overlaps a FEMA footprint;
  FEMA footprint is temporally unavailable. Pairwise links survive separately;
  the path through temporal evidence kills the pair.
- owner equality and parcel adjacency each support a lot; no third lot makes
  the selected pair connected and owner-consistent.

Cheap wrong way:

```text
score geometry, score address, multiply scores, pick max
```

Silent error:

The top score can be a combination of mutually incompatible witnesses. PC
deletes combinations, not low scores.

Exact expensive way:

PC proves that a pair has no extension through a named third constraint. The
explanation is structural: "parcel A with building B has no supporting frontage
interpretation."

### 1.4 What Singleton Arc Consistency Buys

Singleton arc consistency, developed in practical form by Debruyne and Bessiere
in the late 1990s, tries each value as a temporary singleton and runs AC/GAC.
If the network becomes inconsistent, the value is deleted.

SAC catches values that are locally supported but globally poisonous.

Examples:

- Selecting parcel `P` is compatible with area, owner, and address separately,
  but when `P` is forced, the remaining building count cannot satisfy
  `NUMBLDGS`.
- A Microsoft footprint can match one of three latent building slots, but
  forcing one slot triggers an all-different Hall violation among Overture
  features.
- A parcel candidate survives PC, but forcing it makes the subset-sum area band
  unreachable.

SAC is the most important generic consistency level for this application
because many defects are "one plausible candidate poisons the rest of the
tile." At 200 features it is expensive but realistic after global filtering.

Cheap wrong way:

```text
take top K candidates and run pairwise sanity checks
```

Silent error:

The wrong candidate passes every local sanity check and only fails after it
forces a building-count or area-sum contradiction several constraints away.

Exact expensive way:

SAC deletes the candidate because the entire constrained universe cannot be
completed with that candidate forced.

### 1.5 What Strong k-Consistency Buys, and the Real Ceiling

Strong `k`-consistency says every consistent assignment to `k - 1` variables can
be extended to any kth variable. Freuder showed when this makes search
backtrack-free. It is powerful but not generally affordable at `n = 200`.

What it buys:

- strong 3-consistency, essentially path consistency on binary networks,
  deletes bad pairs;
- strong 4-consistency deletes bad triples;
- higher levels delete bad partial explanations before search.

Why global strong 4 is not the default:

```text
C(200, 3) * 200^3 = about 1.05e13 triple/value cells
```

That is not a clever tile-scale trick; it is a memory disaster.

Where strong 4 is worth it:

- local separator around one ambiguous corner lot;
- three candidate parcels plus one address interpretation;
- one owner cluster with 10 to 30 lots;
- one exact subset-sum residual with <= 20 effective candidates;
- one connected-component after global propagation with small domains.

The right formulation is adaptive relational consistency:

```text
Do not enforce k-consistency everywhere.
Enforce it on the small scopes that explain the remaining ambiguity.
```

This is also where database theory helps. Acyclic join networks can be solved
by Yannakakis's 1981 semijoin algorithm in linear time in the input and output
size. For cyclic but low-treewidth residuals, bucket elimination or join-tree
clustering has cost:

```text
O(n * d^(w + 1))
```

where `w` is treewidth. Dechter's bucket elimination framework from 1999 is the
right reference.

At `d = 10, w = 4`:

```text
200 * 10^5 = 20,000,000 table entries
```

Fine.

At `d = 20, w = 6`:

```text
200 * 20^7 = 2.56e11 table entries
```

Too much unless the tables are very sparse and compressed.

So the ceiling is:

```text
Full GAC + AC + sparse/component PC + SAC + local strong 4 +
exact low-treewidth compilation/model counting.
```

Not global strong 5. Not generic full k-consistency.

### 1.6 The Best Generic Endgame: Compile the Residual

After propagation, the residual CSP should be compiled when possible into a
deterministic representation of all solutions:

- ROBDD, Bryant 1986: canonical for a fixed variable order.
- ZDD, Minato 1993: better for sparse set families such as possible parcel
  subsets.
- d-DNNF, Darwiche 2001: supports model counting and conditioning in time
  linear in circuit size.
- SDD, Darwiche 2011: deterministic structure with vtrees, often friendlier
  than raw BDDs.

This is not "use a SAT solver to pick a model." It is knowledge compilation:
turn the tile's finite theory into a compact circuit representing all
admissible resolutions.

The circuit gives:

- exact model count;
- forced literals/backbone;
- possible literals;
- projected ambiguity by parcel/building/namespace;
- conditioning on new evidence without recomputing from scratch;
- exact abstention artifact.

Worst-case size can blow up. That does not break the architecture. It means the
certificate has tiers:

```text
Tier 1: propagation fixpoint with reasons
Tier 2: SAC/local-k enhanced fixpoint
Tier 3: compiled residual model set
Tier 4: complete MUS/MCS/MaxCSP diagnostics
```

The decision output must never pretend Tier 1 is Tier 3. The artifact states
which tier was achieved.

## 2. Global Constraints and Dedicated Propagators

Most of the power is here. Encoding these as pairwise checks wastes the
structure.

### 2.1 Source Exclusivity: AllDifferent / AllDifferentExcept0

Rule:

```text
Two footprints from one source are never the same building.
```

Model:

```text
FeatureToBuilding[f] in latent_buildings union {absent}
AllDifferentExcept0(FeatureToBuilding[f] for f in same_source)
```

Named constraint:

- `all_different`, filtered by Régin 1994 using bipartite matching.
- optional `absent` uses `all_different_except_0` or a GCC variant.

Algorithm:

- Build bipartite graph between source features and candidate latent building
  slots.
- Find maximum matching.
- Use alternating paths/SCCs in the residual graph to remove edges that belong
  to no perfect matching.

Complexity:

```text
Hopcroft-Karp: O(E * sqrt(V))
At n = d = 200, E <= 40,000, sqrt(V) about 20
about 800,000 graph steps per all_different
```

What it prunes that pairwise `!=` does not:

Hall sets. If three Overture footprints can only be buildings `{B1, B2, B3}`,
then no other Overture footprint may use `{B1, B2, B3}`. Pairwise inequalities
do not see that until search.

Cheap wrong way:

```text
dedupe footprints by IoU threshold within each source
```

Silent error:

Two adjacent buildings with high overlap due generalized footprints are merged;
or one building split into multiple features is silently counted twice.

Exact expensive way:

Keep all candidates until the matching structure proves that a same-source
co-reference is impossible.

### 2.2 Cardinalities: Global Cardinality Constraint and Flow

Rules:

```text
tile holds approximately N buildings
parcel NUMBLDGS constrains buildings assigned to parcel
selected collateral has k parcels or k buildings when asserted
one POI is in one building
one building is on one parcel
```

Named constraints:

- GCC, Régin 1996.
- cardinality networks for Boolean counts.
- b-matching / feasible flow for assignment with lower and upper capacities.

Algorithm:

- Represent assignments as bipartite flow:

```text
source -> building/feature variables -> parcel/value nodes -> sink
```

- Lower/upper capacities encode min/max counts.
- Edge feasibility is tested through residual reachability or min-cut.
- Flow explanations are min-cut certificates.

Complexity:

For a tile:

```text
V about 400 to 600
E usually 1,000 to 10,000
```

Hopcroft-Karp for unit matching:

```text
O(E * sqrt(V)) roughly 10,000 * 25 = 250,000 graph steps
```

General bounded flow with deterministic Dinic or push-relabel is still small at
this scale. Even an edge-by-edge feasibility pass is often fine after pruning,
though residual-graph algorithms avoid one max-flow per edge.

What it prunes:

- A parcel cannot take building `B` if assigning `B` would force another parcel
  below its required building count.
- A building candidate is deleted if no feasible global assignment of all
  buildings to parcels exists with that edge.
- A source's footprints are forced into a subset of latent building slots.

Cheap wrong way:

```text
count intersecting footprints per parcel after a spatial join
```

Silent error:

The count looks plausible parcel by parcel but no global one-to-one assignment
exists across all sources.

Exact expensive way:

The flow either exists or it does not. A min-cut names the subset of features
and parcels responsible for failure.

### 2.3 Additive Area, Lot Area, Building Area: Subset-Sum / Knapsack DP

Rules:

```text
selected parcels' LOTAREA sum must lie in an exact band
selected parcels' BLDGAREA sum must lie in an exact band
selected buildings' footprint areas or FEMA square feet must lie in a band
loan collateral must plausibly support debt only when the ratio rule is crisp
```

Named constraints:

- linear sum constraint over finite-domain integers;
- subset-sum / knapsack;
- cardinality-constrained subset-sum;
- multi-dimensional knapsack when area and count interact.

Algorithm:

For one integer sum:

```text
reachable[i][s] = whether a subset of first i candidates can reach sum s
```

Use bitset convolution:

```text
bits := bits OR (bits << weight_i)
```

Then run forward/backward DP to test whether each candidate can be included or
excluded while still reaching the allowed interval.

Complexity:

Let `S` be the maximum scaled sum. If square feet are already integer:

```text
O(n * S / word_bits)
```

For `n = 200, S = 500,000`:

```text
200 * 500,000 / 64 = 1,562,500 word shifts/ORs
```

For `S = 5,000,000`:

```text
15,625,000 word operations
```

That is cheap.

For two dimensions, naive DP is `O(n * S1 * S2)` and may explode. The exact
alternatives are:

- Pareto-frontier DP with dominated states removed;
- meet-in-the-middle, Horowitz and Sahni 1974, for 200 candidates only after
  component splitting;
- branch-and-bound with exact interval pruning;
- tree-decomposition if the additive constraint is local to a parcel cluster.

What it prunes:

- A parcel with plausible geometry is deleted if no subset including it can
  meet the asserted collateral area band.
- A parcel is forced if every feasible subset meeting the area band includes it.
- A claimed area can be proven impossible even though many individual parcels
  are "close."

Cheap wrong way:

```text
choose nearest area by greedy add/drop, or compare one parcel BLDGAREA to loan area
```

Silent error:

Large CMBS collateral is often multi-parcel. Greedy area can choose a visually
plausible but globally impossible subset.

Exact expensive way:

The DP proves membership, non-membership, or residual alternatives over all
subsets within the tile component.

### 2.4 Containment: Inverse, Channeling, and b-Matching

Rules:

```text
building sits on exactly one parcel unless straddling is explicitly represented
POI sits in one building
selected building implies selected parcel
selected parcel may require selected buildings depending on collateral mode
```

Named constraints:

- `inverse` constraint between assignment views;
- channeling constraints between Boolean membership and finite-domain
  assignment;
- b-matching / flow;
- table constraints for exact geometric candidate relations.

Algorithm:

Maintain both views:

```text
BuildingToParcel[B] = P
BuildingsOnParcel[P] contains B
ParcelSelected[P] <=> exists selected B with BuildingToParcel[B] = P
```

The inverse/channeling propagator updates both directions. Flow handles
capacity and count feasibility.

Complexity:

The channeling is linear in changed edges. Flow is as above.

What it prunes:

- A building-to-parcel edge is deleted when it would make parcel membership
  impossible.
- A parcel is forced when a forced building can only sit on that parcel.
- A POI's building assignment can force parcel membership without address
  proposing anything.

Cheap wrong way:

```text
spatially join POI point to nearest footprint, then footprint to parcel
```

Silent error:

Interpolated or centroid points in a roadbed attach to the wrong frontage,
and the pipeline never reconsiders the choice.

Exact expensive way:

The containment channel keeps both possibilities until all constraints agree,
then proves any forced assignment.

### 2.5 Non-Overlap and Geometric Feasibility: diffn/geost Where Applicable

Rules:

```text
two physical buildings cannot occupy the same exact space unless they are the
same latent building
two footprints from one source cannot be the same latent building
parcel polygons define containment candidates
```

Named constraints:

- `diffn` for non-overlap of boxes;
- `geost` for geometric objects, Beldiceanu/Carlsson/Poder/Sadek/Truchet 2007;
- clique/at-most-one constraints over incompatibility graphs when geometries
  are fixed;
- all_different for same-source latent building ids.

Implementation stance:

The geometric predicates must be exact. Floating predicates are not acceptable
as semantic inputs. Use one of:

- source integer coordinates if available;
- deterministic decimal-to-rational conversion;
- fixed-scale integer grid with explicit error envelope;
- adaptive exact predicates in the style of Shewchuk 1997, but canonicalized so
  byte output is platform-independent.

For fixed geometries, do not solve a continuous geometry problem. Build exact
candidate/incompatibility graphs once:

```text
contains(point, polygon)
intersects(footprint, parcel)
overlap_area(footprint_a, footprint_b) in exact rational units
```

Then propagate over finite graph edges.

What it prunes:

- Two footprint observations cannot be distinct buildings if exact overlap and
  source rules force co-reference.
- A footprint cannot be assigned to a parcel it does not intersect/lie within
  under the chosen exact topology rule.
- Same-source duplicates are impossible without a threshold.

Cheap wrong way:

```text
IoU > 0.5 means same building; centroid in parcel means containment
```

Silent error:

Large buildings, parking structures, parcel slivers, and generalized footprints
cross thresholds unpredictably.

Exact expensive way:

Geometry contributes named finite relations with exact predicates and declared
uncertainty envelopes.

### 2.6 Address Strings and Frontage: Regular, Table, and Ordering Constraints

Rules:

```text
some member must front First Avenue at number 199
corner lots may have multiple legitimate addresses
parcel layer stores one representative ADDRESS, not the full address set
address string is evidence, not a proposer
```

Named constraints:

- `regular` constraint, Pesant 2004, for finite-state parsing;
- table constraints for token-to-frontage compatibility;
- precedence / lex-chain constraints for ordered address numbers along a street
  side;
- `among` constraints for "at least one selected parcel has frontage matching
  token T."

Algorithm:

1. Parse with a deterministic finite-state transducer, not a probabilistic
   parser.
2. Produce all exact token interpretations:

```text
house_number = 199
street_name = FIRST AVE
unit/suffix/range/corner markers
```

3. Convert parcel geometry and street centerlines/frontages, if available, into
   finite frontage candidates.
4. Enforce:

```text
exists P in CollateralParcels:
    frontage(P, FIRST AVE) and address_number_compatible(P, 199)
```

5. For multiple addresses in the same row, use ordering constraints rather than
   independent token checks.

Complexity:

Regular constraint DP:

```text
O(length * states * token_domain)
```

Tiny relative to geometry.

Ordering constraints are near-linear after sorting frontage positions. Table
constraints use GAC with bitsets.

What it prunes:

- A parcel candidate with plausible geometry is removed if no selected parcel
  can satisfy the address frontage existential.
- A corner lot is retained even if its representative parcel `ADDRESS` does not
  equal the input string.
- Address ranges can force a set of parcels rather than one parcel.

Cheap wrong way:

```text
normalize address string and join to parcel ADDRESS
```

Silent error:

The true lot has a different representative address. The true answer is never
even proposed.

Exact expensive way:

Address is an existential/global constraint over selected parcels, not an index
lookup.

### 2.7 Temporal Feasibility: Interval Algebra and Unary Pruning

Rules:

```text
a 2019 assertion cannot reference a 2021 building
source vintage matters
YEARBUILT constrains existence, but may be coarse
```

Named constraints:

- interval constraints;
- Allen's interval algebra, 1983, where relations are qualitative;
- ORD-Horn tractable subclass, Nebel and Buerckert 1995, when expressive enough;
- simple unary/binary temporal compatibility for most cases.

Algorithm:

Represent every entity/source observation with an exact or interval vintage:

```text
entity_exists_start <= assertion_date <= entity_exists_end
```

Unknowns are wide intervals, not guessed dates. Temporal propagation is mostly
unary and binary, so it is cheap. Path consistency is useful if using qualitative
interval relations.

What it prunes:

- A building first observed/constructed after the loan assertion cannot be the
  collateral building.
- A demolished or replaced structure is not conflated with a current footprint.

Cheap wrong way:

```text
use latest footprint layer for all historical assertions
```

Silent error:

The resolver identifies a building that did not exist at origination.

Exact expensive way:

Time is part of the CSP. The model either has a temporally valid entity or it
does not.

### 2.8 Ownership: nvalue, Among, and Equivalence Constraints

Rules:

```text
selected lots often share OWNERNAME
OWNERNAME is noisy and not by itself identity
```

Named constraints:

- equality / equivalence constraints;
- `among` for "selected parcels whose owner is in set S";
- `nvalue` for limiting the number of distinct owner values;
- table constraints over exact owner literals.

Important caveat:

Full domain consistency for `nvalue` can be hard in general. Use it carefully:

- exact equality when owner names are already canonicalized by registry;
- bounds consistency for "at most k owner names";
- table/among constraints when the owner set is small.

What it prunes:

- A parcel candidate can be removed when every feasible collateral set requires
  owner consistency and that parcel's owner cannot participate.
- A multi-lot collateral can remain unresolved across same-owner lots instead
  of being forced to the parcel whose representative address happened to match.

Cheap wrong way:

```text
join exact OWNERNAME and call it same collateral
```

Silent error:

Common LLC names, stale owner records, and subsidiary ownership create false
links. Conversely, one collateral can have legitimate owner-name variation.

Exact expensive way:

Ownership is a constraint with an explicit strength and evidence id. It narrows
when crisp; it ranks or diagnoses when soft.

### 2.9 Identifier Namespaces: Functional Dependencies and Congruence Closure

Rules:

```text
one resolved entity has at most one id in a functional namespace
same hard namespace id implies same entity when namespace semantics say so
conflicting ids are contradictions
```

Named structures:

- equality constraints;
- functional dependencies;
- all_different for injective namespaces;
- congruence closure, Nelson-Oppen 1979, for equality propagation;
- union-find with proof forests for deterministic explanation.

Algorithm:

Maintain equivalence classes of entity variables and identifier literals.
Every union records the named evidence responsible. Every attempted union with
an incompatible namespace id produces a conflict proof.

Complexity:

Union-find is effectively inverse-Ackermann time per operation. Explanation
adds a proof forest but is still tiny at tile scale.

What it prunes:

- A candidate merge between two building slots is deleted if it would give one
  entity two incompatible namespace ids.
- A parcel id can force an external namespace id across all solutions.

Cheap wrong way:

```text
coalesce ids after choosing a parcel
```

Silent error:

The id conflict is discovered too late, or silently overwritten.

Exact expensive way:

Identity semantics are constraints in the same network. A conflict is a proof,
not an exception log.

### 2.10 Connectivity: Graph Connected Constraint, But Be Honest

Rule:

```text
collateral parcels are often contiguous or connected
```

Named constraint:

- graph `connected` global constraint over selected vertices.

Caveat:

Full domain consistency for graph connectivity with optional vertices is
related to Steiner connectivity and can be expensive. The defensible propagator
is a sound polynomial filter:

- if required vertices lie in different components of the possible graph:
  contradiction;
- optional vertices outside every component that can connect required vertices
  are removed;
- articulation points that are mandatory to connect required regions are forced;
- bridges/cut vertices supply explanations.

Complexity:

Linear graph traversals:

```text
O(V + E)
```

per propagation event, with `V <= 200`.

What it prunes:

- A parcel island cannot be part of a single connected collateral unless the
  collateral mode permits discontiguous parcels.
- A bridge parcel can be forced when it is the only way to connect two required
  parcel groups.

Cheap wrong way:

```text
buffer parcels and merge anything touching within epsilon
```

Silent error:

The epsilon creates false contiguity across roads, alleys, or geometry slivers.

Exact expensive way:

Connectivity is a graph property over exact adjacency relations with explicit
mode flags. When it cannot decide, it abstains.

## 3. Getting Explanations for Free

Explanations are not a reporting layer. They are part of propagation.

The right paradigm is proof-producing CP, especially lazy clause generation
and truth-maintenance style labels.

References:

- Doyle 1979, justification-based truth maintenance.
- de Kleer 1986, assumption-based truth maintenance.
- Ohrimenko, Stuckey, and Codish 2009, lazy clause generation for CP.
- Junker 2004, QuickXplain for minimal conflicts.

Every primitive value deletion is an implication:

```text
reason literals + constraint id -> not (X = v)
```

Every contradiction is an empty clause:

```text
reason literals + constraint ids -> false
```

The proof artifact is a DAG. Leaves are evidence atoms:

```text
parcel:1012920026.LOTAREA = 12,500
fema:feature:abc.area_sqft = 78,200
overture:building:def.geometry_hash = ...
input:row:7.address_token.street = FIRST AVE
input:row:7.assertion_date = 2019-06-01
```

Internal nodes are propagator reasons:

```text
all_different Hall set
flow min-cut
subset-sum unreachable interval
path-consistency no third support
temporal interval contradiction
functional dependency conflict
```

Human-readable explanations are then just projections of the proof DAG:

```text
Parcel 1012920001 was removed because:
  - the collateral must include a parcel fronting FIRST AVE at 199;
  - parcels {1012920001, 1012920004} are the only geometry-compatible parcels;
  - 1012920001 has no compatible frontage interval for 199;
  - forcing 1012920001 makes the building-area interval unreachable.
```

The solver-native reasons are better than hand-written reporting because each
global propagator emits its natural certificate:

- `all_different`: Hall set or alternating-path proof that an edge belongs to
  no perfect matching.
- GCC/flow: min-cut proving capacity shortage or edge impossibility.
- subset-sum: forward/backward DP state proving no reachable sum in the
  required band.
- path consistency: no value in third domain supports the pair.
- SAC: singleton assumption plus derived contradiction.
- congruence closure: proof forest showing equality chain and conflicting
  functional ids.
- connectivity: connected-component, bridge, or articulation proof.

This costs determinism only if the implementation lets algorithms choose
arbitrary ties. Do not allow that.

Deterministic rules:

- sort variables, values, constraints, and evidence ids bytewise;
- use deterministic graph traversal order;
- use deterministic matching/flow tie-breaks;
- canonicalize all reason clauses by sorted literal/evidence ids;
- avoid randomized SAT/CP search;
- make any compilation variable order a deterministic function of the tile
  schema and evidence ids.

The propagator schedule does not need to be semantically important. On a finite
lattice, fair chaotic iteration of monotone contracting propagators reaches the
same greatest common fixpoint. For byte-identical proof logs, however, use a
canonical queue order anyway.

## 4. Solver-Native Artifacts That Are the Product

The product is not "the selected parcel." The product is everything exact that
the solver can say about the tile.

### 4.1 Forced Facts / Backbone

Definition:

```text
fact f is forced iff f is true in every model of the hard CSP
```

SAT literature calls this the backbone.

Examples:

- parcel `1012920026` is in every feasible collateral set;
- building slot `B17` is always selected;
- Overture building `o1` and FEMA footprint `f9` are always the same latent
  building;
- namespace id `BBL=1012920026` is forced;
- the collateral has exactly two parcels, even though which second parcel
  remains unresolved.

Operator value:

This is the safest positive output. It is what can be loaded into a registry or
used downstream without pretending ambiguity is resolved.

Cheap wrong way:

```text
highest scoring candidate
```

Silent error:

A high score is not a universal quantifier over all feasible resolutions.

Exact expensive way:

A forced fact is proven across the entire residual model set.

### 4.2 Residual Domains / Structural Abstention

Definition:

```text
projection of all models onto the variables the operator cares about
```

Example:

```text
CollateralParcels in:
  {1012920026}
  {1012920026, 1012920001}
  {1012920026, 1012920004}
```

Or compactly:

```text
forced: 1012920026
one_of: [1012920001, 1012920004]
reason_not_separated: both satisfy frontage, area, temporal, and owner
```

Operator value:

This is not "low confidence." It is the exact ambiguity boundary. It tells the
operator what is known, what remains open, and which evidence would separate it.

Most valuable use:

Residual domains are the bridge between automated resolution and human review.
They turn review from "find the answer" into "choose between these two
mathematically surviving alternatives."

### 4.3 Exact Model Count

Definition:

```text
number of satisfying assignments, or number of distinct projected parcel sets
```

Use #CSP/#SAT over the compiled residual. If compiled to d-DNNF, counting is
linear in circuit size. If using tree decomposition, counting is dynamic
programming over bags.

Operator value:

- `1` means structurally resolved.
- `2` means a binary ambiguity.
- `10,000` means the tile lacks separating evidence.

This is a real ambiguity measure, unlike a confidence score.

Cheap wrong way:

```text
normalize heuristic scores into probabilities
```

Silent error:

The probabilities are calibration theater. They do not know how many exact
solutions remain.

Exact expensive way:

Count the models or state precisely that only a lower-tier residual was
computed.

### 4.4 Minimal Unsatisfiable Subsets

Definition:

```text
MUS = minimal subset of constraints that is itself inconsistent
```

Algorithms:

- deletion-based MUS extraction;
- QuickXplain, Junker 2004;
- hitting-set diagnosis, Reiter 1987;
- MUS/MSS enumeration families such as Liffiton and Sakallah 2008/2013.

Operator value:

This may be the most valuable artifact commercially. An empty domain with a
minimal contradiction core says:

```text
These exact source claims cannot all be true together.
```

Example:

```text
Conflict core:
  input asserted origination date 2019-06-01
  selected address requires building footprint M
  Microsoft footprint M first appears in 2021 source vintage
  FEMA/parcel containment excludes all pre-2019 alternatives
```

That is not a fuzzy failure. It is a defect receipt.

Why it sells:

CMBS and agency data consumers already know sources conflict. What they do not
usually have is a minimal, named, reproducible proof of the conflict that can be
sent back to a vendor, servicer, or reviewer.

### 4.5 Minimal Correction Sets and Best Repairs

Definition:

```text
MCS = minimal set of constraints whose removal restores satisfiability
```

Weighted version:

```text
minimum-cost relaxation under a declared reliability/cost model
```

Operator value:

This is the repair plan:

- "If we relax the asserted square footage band, the tile resolves."
- "If we distrust the interpolated geocode, three parcel sets remain."
- "If we distrust the 2021 footprint date, the building id is forced."

Cheap wrong way:

```text
drop constraints in a fixed order until something resolves
```

Silent error:

The order encodes hidden policy and may discard the most reliable evidence.

Exact expensive way:

Compute minimal correction sets and report the declared cost model. Never
present the repair as hard truth.

### 4.6 Prime Explanations for Forced Facts

Definition:

```text
minimal sufficient evidence subset proving a forced fact
```

This is dual to MUS extraction: instead of "why impossible?", ask "which named
evidence is sufficient to force this?"

Operator value:

Audit-ready positive evidence. It supports registry promotion:

```text
BBL 1012920026 is forced by constraints C1, C4, C9, C12.
```

Cheap wrong way:

```text
dump all input rows and call it provenance
```

Silent error:

The operator cannot tell which facts actually mattered.

Exact expensive way:

Emit minimal or irredundant support sets derived from proof DAGs.

### 4.7 Counterfactual Separation (a Value-of-Information Precursor)

Definition:

For each precisely stated possible observation, compute whether it can separate the
current residual alternatives. Expected value of information additionally needs a
distribution over possible observations, acquisition cost, and decision utility.

Concrete version:

```text
two residual parcel sets differ only on:
  - frontage at 199 FIRST AVE
  - building area band
  - owner equivalence
```

Operator value:

It tells the human what to buy or inspect next: assessor cards, tax maps,
building permits, imagery date, loan docs, or parcel address aliases.

Cheap wrong way:

```text
send all unresolved cases to manual review
```

Silent error:

Reviewers waste time finding which evidence dimension matters.

Exact expensive way:

Use the residual model set to compute the distinguishing constraints directly.

## 5. Where the Frame Breaks

The hard CSP frame works only for crisp constraints. Some evidence is not crisp:

- `BLDGAREA` can be gross, net, rentable, stale, or assessor-specific.
- `LOTAREA` can include easements or geometry artifacts.
- FEMA square footage can be modeled.
- Microsoft footprints lack stable ids.
- geocodes can be interpolated into roads.
- loan balance plausibility is not identity unless tied to an explicit rule.
- source vintages may be approximate.

If these are treated as hard exact facts, the solver will produce beautiful
false contradictions. That is worse than a heuristic.

The principled extension is stratified hard/soft reasoning.

### 5.1 Preserve Guarantees with Typed Hard Envelopes

For numeric evidence, convert a noisy measurement into a hard interval only
when the interval is part of the evidence contract:

```text
asserted_bldgarea in [L, U]
```

Use exact integer arithmetic:

- square feet as integers;
- ratios as rational intervals;
- percentages as numerator/denominator pairs;
- no platform floats in decisions.

A wider interval prunes less but remains sound. This preserves confluence:
the interval constraint is still a monotone domain-shrinking propagator.

The hard rule is:

```text
Only declared envelopes become hard constraints.
Undeclared noise becomes soft preference or diagnostic evidence.
```

### 5.2 Softness Does Not Preserve the Same Monotonicity

For preferences, use a valued CSP or semiring CSP:

- Schiex, Fargier, Verfaillie 1995 valued CSPs.
- Bistarelli, Montanari, Rossi 1997 semiring CSPs.

Each model gets a cost:

```text
cost(model) = lexicographic or semiring aggregate of violated soft constraints
```

Then compute:

- all hard-feasible models;
- minimum soft cost;
- all minimum-cost models;
- forced facts across all minimum-cost models;
- minimal relaxations when hard constraints are inconsistent.

This is deterministic if costs, tie-breaks, and arithmetic are exact.

But it changes the epistemology:

```text
Adding a soft constraint can change the optimum.
```

So the earlier "knowledge only tightens" guarantee does not apply to preferred
answers. It applies to hard model sets. The output must separate:

```text
HARD_FORCED: true in every hard-feasible model
SOFT_PREFERRED: true in every minimum-cost model under declared policy
SOFT_RANKED: ranked alternatives, not facts
```

Never promote `SOFT_PREFERRED` as a canonical identity fact unless the product
contract explicitly allows policy-dependent identity.

### 5.3 When Hard Constraints Conflict

If hard constraints are inconsistent:

1. Emit the MUS or a small irreducible conflict.
2. Compute MCS/minimum-cost repair only as diagnosis.
3. Do not return a resolved identity.

If the conflict involves constraints that should have been soft, the fix is not
to weaken the solver. The fix is to reclassify the evidence contract.

### 5.4 Fallback

The fallback is not fuzzy matching. It is a lower claim class:

```text
hard residual unresolved
soft ranking available
minimal repairs available
human review target available
```

Softness does not destroy determinism. It destroys the right to call the
optimum "the truth." Keep those separate and the architecture remains honest.

## 6. Ambition Test

This section ranks techniques by the gap between the cheap competent competitor
version and the expensive exact version.

### 6.1 Highest Gap: Residual Model Set as Product

Cheap wrong way:

```text
return top candidate plus confidence
```

Why it silently fails:

The confidence score is not tied to the number of feasible alternatives, and it
cannot distinguish "one exact solution" from "many plausible ones."

Exact way:

Compile or otherwise represent all feasible resolutions. Report forced facts,
possible facts, and exact abstention.

What it buys:

The output is a formally characterized object, not a guess. This is the largest
category gap.

Rank: 1.

### 6.2 Highest Gap: MUS/MCS Data-Quality Receipts

Cheap wrong way:

```text
low confidence, unresolved, needs review
```

Why it silently fails:

It hides actual source contradictions and sends reviewers into the tile without
a proof.

Exact way:

Emit minimal unsatisfiable cores and minimal correction sets with named
evidence.

What it buys:

A source-conflict receipt that can be operationalized, billed, and audited.

Rank: 2.

### 6.3 Very High Gap: Global Matching/Flow for Building and Parcel Assignment

Cheap wrong way:

```text
spatial join each footprint independently
```

Why it silently fails:

Independent joins can create a globally impossible assignment with duplicate
buildings, impossible counts, or capacity violations.

Exact way:

Use all_different, GCC, and b-flow domain filtering.

What it buys:

No local assignment survives unless it participates in a feasible global
assignment.

Rank: 3.

### 6.4 Very High Gap: Exact Subset-Sum Area Propagation

Cheap wrong way:

```text
pick parcel or parcel group whose area is closest
```

Why it silently fails:

Closest greedy subsets can be impossible once owner, address, temporal, and
building constraints are included.

Exact way:

Use integer subset-sum/knapsack DP to prove include/exclude/ambiguous status for
each parcel.

What it buys:

Area becomes a global pruning constraint over all parcel subsets.

Rank: 4.

### 6.5 High Gap: SAC After Global Filtering

Cheap wrong way:

```text
pairwise validation of top candidates
```

Why it silently fails:

Some candidates fail only after their consequences propagate through multiple
global constraints.

Exact way:

Force each remaining value, propagate to fixpoint, and delete values that lead
to contradiction.

What it buys:

Eliminates locally plausible but globally poisonous candidates.

Rank: 5.

### 6.6 High Gap: Address as Existential Frontage Constraint

Cheap wrong way:

```text
join normalized address to parcel ADDRESS
```

Why it silently fails:

Large and corner lots have multiple addresses; parcel layers often store one.

Exact way:

Address tokens constrain the selected parcel set existentially and by frontage,
not by representative address equality.

What it buys:

The true answer remains reachable.

Rank: 6.

### 6.7 Medium Gap: Path Consistency

Cheap wrong way:

```text
AC or pairwise filters only
```

Why it silently fails:

Bad pairs survive when each side has support, but no third variable supports the
combination.

Exact way:

Run PC on sparse typed components and small dense residuals.

What it buys:

Deletes pair-level ghosts before search/compilation.

Rank: 7.

### 6.8 Medium Gap: Exact Geometry Predicates

Cheap wrong way:

```text
centroid containment and epsilon buffers
```

Why it silently fails:

Boundary cases, slivers, roadbeds, generalized footprints, and platform float
differences change results.

Exact way:

Use deterministic rational/integer geometry to produce finite relations.

What it buys:

Byte-identical geometry evidence and no epsilon policy hidden in code.

Rank: 8.

This is essential, but by itself it is not enough. A competitor can also buy a
robust geometry library. The category gap appears when exact geometry feeds a
proof-producing CSP instead of a spatial join.

### 6.9 Lower Gap: Simple Unary Temporal Filters

Cheap wrong way:

```text
ignore vintage
```

Why it silently fails:

Historical assertions can resolve to future buildings.

Exact way:

Use existence intervals and temporal compatibility.

What it buys:

Important correctness, but much of it is straightforward once noticed.

Rank: 9 unless combined with MUS receipts.

## 7. The Categorically Best Version

The best version is a proof-carrying tile theory with a compiled residual model
set.

Call it:

```text
Canonical Tile Constraint Certificate (CTCC)
```

It contains:

```text
1. Universe
   exact tile/hard-halo feature ids, latent slots, domains, and coordinate
   representation

2. Evidence atoms
   named immutable source claims with hashes and exact typed values

3. Hard constraint theory
   all crisp constraints, each with a named propagator and declared semantics

4. Fixpoint trace
   deterministic sequence or canonical summary of domain deletions, each with
   a proof reason

5. Residual model representation
   preferred: ROBDD/ZDD/d-DNNF/SDD
   fallback: domains + constraints + proof-producing propagator tier achieved

6. Projections
   forced parcels/buildings/ids
   possible parcels/buildings/ids
   unresolved alternatives
   exact model counts where compiled

7. Contradiction diagnostics
   MUS/MCS/minimum-cost repair sets when model set is empty or soft layers
   conflict

8. Soft layer
   valued/semiring CSP policy, exact costs, minimum-cost residual, never mixed
   with hard truth
```

The formal guarantees:

- Termination: finite domains and contracting propagators.
- Confluence of fixpoint: monotone propagators over a finite lattice, applied
  fairly, reach the same greatest common fixpoint.
- Soundness: every deleted value has a proof from named constraints.
- Monotonic hard knowledge: adding hard constraints shrinks the model set; hard
  forced facts remain forced under projection unless inconsistency is exposed.
- Byte determinism: exact arithmetic, sorted ids, canonical propagator schedule,
  deterministic graph algorithms, no randomness.
- Abstention: a residual projection with cardinality greater than one is a
  first-class answer.
- Contradiction: empty residual model set yields a proof object, not a failure
  code pretending to be uncertainty.
- Completeness tiering: if knowledge compilation succeeds, forced/possible/count
  facts are complete for the finite theory; if not, the artifact declares the
  achieved consistency tier.

This reframes property resolution from:

```text
pipeline of spatial joins and heuristic scores
```

to:

```text
finite model theory for one tile
```

That is the categorical jump.

## 8. Recommended Solver Architecture

### 8.1 Normalize Into Exact Finite Relations

Do this before CSP solving:

- exact geometry candidate relations;
- deterministic address token interpretations;
- temporal intervals;
- integer numeric bands;
- source feature ids and hashes;
- namespace functional-dependency declarations.

No ranking. No candidate proposer. Only finite domains and named relations.

### 8.2 Run Global Propagators First

Order for performance, not semantics:

1. unary temporal and exact geometry filters;
2. all_different / GCC / flow;
3. subset-sum area filters;
4. address regular/frontage constraints;
5. containment inverse/channeling;
6. connectivity filters;
7. binary AC on the residue.

Every propagator emits deletion reasons.

### 8.3 Run Sparse Path Consistency

Run PC where:

- binary graph is sparse enough;
- component size/domain size fits the budget;
- the component is an ambiguity hotspot.

Do not binary-encode every global constraint just to run PC. That is the wrong
direction.

### 8.4 Run SAC

Run SAC after global propagation because it is much cheaper then. Parallelize
singleton trials. Every SAC deletion reason is:

```text
assume X = v -> propagation proof -> contradiction
```

This is highly explainable.

### 8.5 Run Local Strong 4 / Join-Tree DP

Use deterministic component analysis:

- find residual connected components;
- compute deterministic min-fill or min-degree tree decomposition;
- run exact join-tree DP if estimated bag table size is below budget;
- run strong 4 on separators where tuple count is small.

This is where many "impossible only in combination" cases fall.

### 8.6 Compile Residual When Feasible

Compile projected parcel/building/identifier decisions to ZDD/ROBDD/d-DNNF.

Use fixed variable order:

```text
parcel membership vars sorted by parcel id
building membership vars sorted by stable latent id
assignment vars sorted by source then feature id
namespace vars sorted by namespace then id
```

This is not necessarily the smallest circuit, but it is deterministic. A
deterministic min-fill order can be used if its implementation is also fixed
and serialized.

### 8.7 Emit CTCC

The operator-facing output should include:

- `status`: resolved, partial, unresolved, inconsistent;
- `tier`: propagation, sac, local_k, compiled;
- `forced`: facts true in all hard models;
- `residual`: compact alternatives/projections;
- `count`: exact count when available;
- `explanations`: proof DAG refs for each forced/deleted fact;
- `conflicts`: MUS/MCS when inconsistent;
- `soft`: separate ranked/min-cost diagnostics if configured;
- `universe`: explicit statement of tile and halo bounds.

## 9. Honest Limits

There are limits that must be visible in the product.

### 9.1 Universe-Bound Limit

All proofs are relative to the bounded universe. If the true parcel is outside
the tile/hard halo, the solver can only prove inconsistency or no model within
that universe. The remedy is deterministic universe expansion, not heuristic
choice.

### 9.2 Source-Contract Limit

If a source field is semantically ambiguous, it cannot be a hard constraint
without an explicit envelope. `BLDGAREA` is the obvious example.

### 9.3 Compilation Blowup

Knowledge compilation has bad worst cases. The solver must report:

```text
compiled = true/false
exact_count_available = true/false
consistency_level = ...
```

No approximate model count should be smuggled in as exact.

### 9.4 Soft Ranking Is Policy, Not Truth

Weighted CSPs are deterministic but policy-dependent. They are useful for
review priority and repair recommendations. They are not canonical resolution
unless the customer contract says so.

## 10. The Most Valuable Product Surface

The highest-value deliverable is not the resolved cases. Competitors can resolve
easy cases.

The highest-value deliverable is:

```text
For every hard case, produce the exact residual alternatives or the minimal
source contradiction that prevents resolution.
```

That is what the industry generally lacks. It turns ambiguous property identity
from a confidence problem into an audit object.

The one-sentence product claim:

```text
Given a bounded tile universe, we compute the exact hard model set of property
identity claims as far as tractable, prove every forced conclusion, and return
the residual or contradiction when the evidence does not determine a unique
answer.
```

That is the ambitious defensible form.
