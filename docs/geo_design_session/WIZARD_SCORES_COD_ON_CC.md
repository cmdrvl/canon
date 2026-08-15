# WIZARD_SCORES_COD_ON_CC

## Summary

The other model's strongest idea is correct: property identifier resolution is a
scene-level inference problem, not a per-feature lookup problem. The weak part
is that it sometimes names a field's vocabulary instead of proving a structural
isomorphism. A real technique has to say exactly what the source observations,
invariants, noise, constraints, and abstention certificate are in our problem.

My highest scores go to exact constraint/model enumeration and relative
structure methods: SAT backbone, maximum common subgraph, astrometric pattern
matching, transform-space consensus, PQ-trees for block-face ordering, and
Patterson-style difference spectra. My lowest scores go to methods whose
assumptions are false for property data: CSR nearest-neighbour nulls,
capture-recapture, latent-class accuracy estimation, and loose analogies like
seriation or double-difference relocation when not turned into an exact property
algorithm.

Already standard practice, so not the forgotten edge by itself: robust
estimation, Hough voting, Hungarian assignment, likelihood-ratio linkage,
adaptive gating, exact predicates/snap rounding, range/order address logic, and
cross-validation. They can still be useful components.

Genuinely brilliant and worth stealing: SAT backbone/model enumeration for
"forced stamps only"; PQ-tree/consecutive-ones for address ranges and block-face
abstention; modular-product maximum common subgraph over typed property
evidence; Patterson/homometry vocabulary for exact indistinguishability; and
interval point-in-polygon returning undecided.

## Scores

### 1. Triangle/quad pattern matching on coordinate lists

**Score: 860/1000**

This is a real structural isomorphism when two source layers are noisy point
constellations of the same local scene under an unknown shared transform, with
missing and spurious points. It survives determinism if the implementation uses
canonical enumeration, fixed integer/rational geometry, and stable tie-breaking;
the raw O(n^4) quad count at n=200 is about 1.6 billion, so it only remains
cheap with triangles, k-nearest neighborhoods, or strong labels.

**Strongest argument for:** It directly catches the systematic-offset failure
that point-in-polygon and nearest-neighbour cannot see.

**Strongest argument against:** It resolves geometric point constellations, not
entity levels; without typed constraints it can confidently align the wrong
class of object.

### 2. Repeated-median and Theil-Sen exact robust estimation

**Score: 710/1000**

This is a sound deterministic replacement for RANSAC when the correspondences or
candidate correspondences already exist and the task is to estimate a robust
transform. It does not solve association by itself, and its abstention signal is
mostly residual/tie based rather than a certificate that the identity evidence is
indistinguishable.

**Strongest argument for:** It gives a deterministic high-breakdown transform
estimator without sampling or convergence tolerance.

**Strongest argument against:** It is standard robust statistics and needs a
separate matching engine before it becomes useful.

### 3. Exact max-depth cell in transform-constraint arrangements

**Score: 835/1000**

This is a real isomorphism: each candidate correspondence induces a region in
transform space, and the maximum-depth cell is the deterministic consensus
transform. It gives named support sets and exact ambiguity regions if built with
integer grids or rational intervals, though full arrangements can get expensive
unless candidate generation is pruned.

**Strongest argument for:** It converts "confidence" into a concrete feasible
region plus the evidence rows that support it.

**Strongest argument against:** It is computational-geometry infrastructure,
not a complete property resolver, and exact implementation burden is real.

### 4. Double-difference relocation / common-mode cancellation

**Score: 430/1000**

The principle is useful, but the proposed isomorphism is loose. Seismology's
double differences are differences of arrival-time residuals under a velocity
model; our property problem has heterogeneous geometry, addresses, parcels, and
categorical identifiers, so "relative geometry survives geocoder error" is more
of a slogan than a method.

**Strongest argument for:** It correctly emphasizes that source error is often
shared across a tile and should be cancelled before resolving individual points.

**Strongest argument against:** As stated, it does not define the observations,
the error model, or the exact estimator in our domain.

### 5. Configuration-space obstacles

**Score: 560/1000**

The set-of-transforms framing is real: assertions can define feasible and
forbidden regions in transform space. The phrase "confidence as exact polygon
area" is mathematically sloppy, though; area is not confidence unless a
well-defined prior over transforms has been declared.

**Strongest argument for:** It naturally produces abstention as "more than one
feasible transform remains."

**Strongest argument against:** It is more a formulation than an implementable
resolver, and exact configuration-space geometry is a permanent maintenance
burden.

### 6. Maximum common substructure via max-clique in the modular product

**Score: 910/1000**

This is one of the strongest proposals. A tile can be represented as a typed
relational graph, candidate node matches become modular-product vertices, and a
clique is a mutually consistent correspondence set; maximal cliques and
automorphisms give a direct abstention certificate.

**Strongest argument for:** It explains every accepted match by named unary and
binary constraints, and every runner-up by the constraints it violates or ties.

**Strongest argument against:** Naive candidate-pair products can explode
(200x200 = 40,000 product vertices before edges), so it needs aggressive
type/geometry/address pruning and bitset engineering.

### 7. Interpretation trees with unary/binary pruning

**Score: 660/1000**

This is structurally real but mostly an older search organization for the same
consistent-labelling problem solved more cleanly by graph/SAT formulations. It
can be deterministic and explainable, but the pruning strategy becomes ad hoc
and can be hard to keep complete.

**Strongest argument for:** It is simple to implement as depth-first exact
search over candidate labels with explicit rejection reasons.

**Strongest argument against:** It is not especially novel or robust, and worst
case search can be ugly even at tile scale.

### 8. Geometric hashing with exact rational affine invariants

**Score: 700/1000**

The isomorphism is real for transform-free indexing: basis-relative coordinates
can identify a local pattern without first knowing the transform. The weak part
is "exact rational affine invariants" under noisy coordinates; exact invariants
are brittle unless wrapped in deterministic intervals, and affine invariance may
be the wrong model for many address/parcel distortions.

**Strongest argument for:** It can cheaply generate high-quality candidate
matches before exact graph or SAT verification.

**Strongest argument against:** It is standard computer-vision machinery and is
only safe as an index, not as the stamp authority.

### 9. SAT backbone, model enumeration, minimal unsat cores

**Score: 950/1000**

This is the cleanest match to the determinism and abstention requirements.
Candidate bindings are boolean variables, source assertions are clauses or
small constraints, backbone literals are exactly the stamps forced by all
models, and non-backbone variables are honest abstentions.

**Strongest argument for:** It gives the exact product behavior we want:
"confirmed" means forced by evidence, "conflict" means incompatible evidence,
and "abstain" means multiple models survive.

**Strongest argument against:** A full SAT dependency may be overkill; a small
team should probably start with bounded exact enumeration or a simple DPLL over
tile-pruned candidates before adopting a solver stack.

### 10. Hungarian assignment + Murty k-best + LP dual reduced costs

**Score: 760/1000**

This is sound for one-to-one assignment subproblems and gives deterministic
best/second-best margins with explainable edge costs. The entity-level problem
often is not one-to-one: one parcel can have many addresses, one building can
cross parcels, one facility can span many parcels, and one permit can apply to a
unit or a campus.

**Strongest argument for:** It is cheap, deterministic, easy to test, and gives
clear runner-up explanations.

**Strongest argument against:** It silently encodes the wrong cardinality for
many of the most important property namespaces.

### 11. Weisfeiler-Leman, canonical labelling, automorphism group

**Score: 790/1000**

This is a real way to detect structural indistinguishability: if the evidence
graph has a nontrivial automorphism, some proposed stamps are not forced. It is
less a resolver than an ambiguity/canonicalization layer, and mature libraries
like nauty are a dependency and licensing/portability decision.

**Strongest argument for:** Automorphism is exactly the kind of non-threshold
abstention certificate the product needs.

**Strongest argument against:** It does not by itself score heterogeneous
evidence or generate candidate bindings.

### 12. Baarda data snooping, redundancy numbers, Minimal Detectable Bias

**Score: 590/1000**

This is strong in geodetic least-squares networks, where observations,
variances, and linearized residuals have precise meanings. The mapping to
property identity is only partially real because much of our evidence is
categorical, topological, legal, and many-to-many rather than Gaussian
measurement residuals.

**Strongest argument for:** The idea "is this observation actually checked by
the rest of the network?" is exactly the right question.

**Strongest argument against:** The mathematics is misapplied if we pretend
parcel/address/permit evidence has the same linear stochastic model as a survey
network.

### 13. Likelihood ratio + reliability for catalogue cross-identification

**Score: 675/1000**

The local-background-density idea is real and useful, especially in dense
candidate fields where a 3 m match is not equally meaningful everywhere. But it
is standard record-linkage/cross-identification practice, probabilistic by
nature, and must be discretized and decomposed carefully to satisfy replay and
human explanation.

**Strongest argument for:** It catches "the nearest match is not meaningful
because the local scene is crowded."

**Strongest argument against:** A scalar reliability score can become another
confidence-number black box unless every factor is named and independently
auditable.

### 14. Ordered-statistic CFAR

**Score: 470/1000**

Adaptive gating from the local order statistic is a real radar technique, but
for property identity it mostly gives a better threshold, not a resolver. It
can be deterministic and useful as a guardrail, yet it does not explain a match
beyond local density and it does not solve entity-level ambiguity.

**Strongest argument for:** It prevents a constant search radius from behaving
insanely in sparse rural and dense urban tiles.

**Strongest argument against:** It is just adaptive nearest-neighbour gating,
which is standard practice adjacent and not the edge we are looking for.

### 15. MML / MDL model selection

**Score: 520/1000**

The "merge if it compresses the evidence" framing can be made real, but only
after defining an exact code for geometry, addresses, source errors, births,
deaths, and entity relations. Without that, MDL is a vibe with bits attached,
and the claim "no tuned threshold" is misleading because the coding model
itself carries the tuning.

**Strongest argument for:** It forces a global scene explanation instead of a
pile of independent pairwise matches.

**Strongest argument against:** It is easy to hide subjective weights inside the
code length and then lose the named-score explanation requirement.

### 16. Exact CSR nearest-neighbour null / Fisher exact inference

**Score: 390/1000**

The p-value is exact only under the null model, and complete spatial randomness
is a terrible null for buildings, parcels, and addresses arranged along streets,
zoning, and subdivision geometry. It says how surprising a distance is under a
fictional process, not whether two identifiers name the same property entity.

**Strongest argument for:** It can expose that a small distance is meaningless
in a very dense local field.

**Strongest argument against:** The structural isomorphism fails because the
spatial point process assumption is false in exactly the places we care about.

### 17. PQ-trees and the consecutive-ones property

**Score: 875/1000**

This is excellent for block-face and range-address problems. Addressable
objects along a street face should occupy constrained consecutive positions,
and P/Q nodes give a native notation for "this order is not forced."

**Strongest argument for:** It directly attacks range addresses, multi-address
tax lots, and ambiguous block-face ordering with deterministic explainability.

**Strongest argument against:** It only covers linear order constraints; it does
not solve parcels, buildings, units, or geometry away from the street-face
model.

### 18. Seriation

**Score: 600/1000**

Seriation is a plausible analogy for inferring order when dates or address
numbers are unreliable, but it is weaker than the more precise PQ-tree and rank
aggregation formulations. It can be deterministic if formulated as an exact
optimization, yet the proposal does not specify the invariant beyond "ordering."

**Strongest argument for:** It encourages using neighborhood order rather than
trusting a single parsed address token.

**Strongest argument against:** As stated, it is mostly a metaphor and risks
rediscovering address interpolation with academic decoration.

### 19. Kemeny/Slater rank aggregation by subset DP

**Score: 725/1000**

This is real for reconciling multiple source orderings on a small block face,
and exact subset DP at n <= 20 is genuinely cheap. Condorcet cycles are a good
conflict/abstention signal, but many tiles have more than 20 relevant objects
unless segmented first, and source orderings may be partial rather than total.

**Strongest argument for:** It turns conflicting address/building order evidence
into named pairwise disagreements and exact optimal orders.

**Strongest argument against:** It is a narrow subroutine, not a general
identifier stamping method.

### 20. Patterson function and cross-Patterson translation function

**Score: 830/1000**

The isomorphism is real for translation-invariant point sets: inter-object
difference vectors in one layer correspond to inter-object difference vectors in
another. At n=200 the O(n^2) vector multiset is cheap, and homometric ties give
a natural abstention warning.

**Strongest argument for:** It sees relative constellations even when absolute
coordinates are shifted or georeferencing is wrong.

**Strongest argument against:** Missing/spurious points, rotation/local affine
distortion, repeated grids, and street-regular layouts can create misleading
vector spectra unless verified by a downstream constraint solver.

### 21. Homometry / turnpike ambiguity

**Score: 705/1000**

This is not a standalone resolver, but it is a strong abstention concept:
different configurations can produce the same difference evidence. It is most
valuable as a certificate attached to Patterson-style or order-based methods,
not as a separate product technique.

**Strongest argument for:** It gives a formal name and test target for "the
evidence genuinely cannot distinguish these candidates."

**Strongest argument against:** Knowing an ambiguity exists does not tell us how
to bind identifiers unless another method has already produced candidates.

### 22. Laminar-family / perfect-phylogeny compatibility, 3-gamete test

**Score: 760/1000**

The containment isomorphism is real: parcels, buildings, units, campuses,
permits, and facilities often impose nested-or-disjoint clusters, and crossing
clusters are evidence of a bad entity-level claim. The phylogenetic framing can
overreach, but the laminar-family constraint itself is directly useful.

**Strongest argument for:** It produces crisp contradiction evidence when a
claimed hierarchy cannot be true.

**Strongest argument against:** Real property has legitimate overlaps: air
rights, easements, condos, mixed-use campuses, and facilities spanning parcels
can violate naive laminarity.

### 23. Strict and majority-rule consensus trees / polytomies

**Score: 485/1000**

Polytomy is a good notation for unresolved hierarchy, but this is mostly a
reporting vocabulary unless paired with a precise tree-building method and
source model. Majority-rule consensus is especially dangerous because "most
sources agree" is not authority when the sources share an upstream error.

**Strongest argument for:** It gives a human-legible abstention shape for
unresolved containment.

**Strongest argument against:** It can launder source duplication into false
confidence.

### 24. Snap rounding for exact arrangements

**Score: 660/1000**

This is real, deterministic computational-geometry infrastructure. It helps
avoid platform-dependent slivers and predicate failures, but it is standard
geospatial hygiene rather than a resolver, and snap rounding itself changes
geometry unless the grid policy is part of the evidence contract.

**Strongest argument for:** It is a practical way to make geometry operations
replayable across platforms.

**Strongest argument against:** It does not explain identity, and careless grid
choice can create the very topology errors it is meant to prevent.

### 25. Multiple-systems estimation / capture-recapture log-linear models

**Score: 340/1000**

This is useful for estimating coverage, not for stamping identifiers. The
unobserved-cell analogy is tempting, but Overture, Microsoft, FEMA, local
parcels, and OSM are not independent capture processes, and log-linear fitting
introduces probabilistic/iterative machinery that does not produce named
binding evidence.

**Strongest argument for:** It correctly warns that "three sources agree" is
not three independent votes.

**Strongest argument against:** It estimates missing populations rather than
proving that a specific external identifier names a specific physical entity.

### 26. Hui-Walter latent-class accuracy estimation

**Score: 140/1000**

The other model was right to reject this. The identification conditions are not
credible here, conditional independence is badly violated, and the output would
be source-level accuracy estimates rather than evidence-backed property stamps.

**Strongest argument for:** It asks a useful meta-question: how accurate are
sources when no gold standard exists?

**Strongest argument against:** It fails the structural-isomorphism,
determinism, explainability, and implementation-burden tests for this product.

### 27. Multiple Hypothesis Tracking / 0-1 IP with birth/death

**Score: 690/1000**

The temporal association analogy is real, especially the birth/death term for
vintage differences between source releases. As a standalone proposal it is too
broad and overlaps with graph/SAT/IP assignment, but "objects can appear,
retire, split, merge, and be missed" is essential.

**Strongest argument for:** It is the best explicit treatment of source-vintage
change on the list.

**Strongest argument against:** Classic MHT often relies on probabilistic
scoring, pruning, and tuning, which can become non-replayable and hard to
explain.

### 28. Interval-arithmetic point-in-polygon returning in / out / undecided

**Score: 775/1000**

This is small but extremely sound. It takes the industry's core operator and
makes it deterministic and honest: if numeric precision or boundary conditions
cannot decide, the result is undecided instead of a fake yes/no.

**Strongest argument for:** It directly fixes one measured failure class: a
point inside, on, or near multiple legitimate parcel boundaries.

**Strongest argument against:** It is standard exact/numerical hygiene and does
not solve address parsing, multi-address parcels, systematic geocode shifts, or
entity-level joins.

### 29. Integer chamfer distance transform matching

**Score: 550/1000**

This is a real deterministic shape-matching tool, especially if footprints are
rasterized onto a pinned integer grid. It is weaker than point-pattern and graph
methods because rasterization loses exact topology and the output is a distance
field unless carefully decomposed.

**Strongest argument for:** It is cheap, deterministic, and useful for comparing
building footprint shapes under small shifts.

**Strongest argument against:** It is standard computer vision and has lower
discriminating power than exact vector/graph constraints in dense urban tiles.

### 30. Free-R cross-validation with deterministic leave-one-out

**Score: 705/1000**

As a validation harness, this is good: hold out one assertion, solve the tile,
and ask whether the solver predicts the held-out evidence rather than merely
fitting it. It is not a resolver and does not itself produce stamps, but it is
excellent for detecting overfit rules and demo-path hard-coding.

**Strongest argument for:** At n=200, deterministic leave-one-out is cheap and
gives concrete per-evidence failure reports.

**Strongest argument against:** Cross-validation is standard practice and can
still pass if the held-out evidence shares the same upstream error as the
training evidence.

## Quick Rejects Mentioned Outside The Table

### Persistent homology

**Score: 220/1000**

The stability theorem is real, but the property-identifier scene usually does
not have rich topology; city blocks are dominated by order, containment, and
typed graph relations. It may be deterministic with integer filtrations, but it
does not explain why two identifiers name the same entity.

**Strongest argument for:** It could detect coarse structural differences in
large campus or network-like layouts.

**Strongest argument against:** The structural isomorphism is mostly metaphor
for ordinary parcel/address tiles.

### Combinatorial group testing

**Score: 80/1000**

This fails because group testing assumes we can design pooled tests and observe
their outcomes. In property resolution the evidence arrives from external
sources; our consistency checks are not freely designable tests of hidden
defectives.

**Strongest argument for:** It has the right taste of using aggregate evidence.

**Strongest argument against:** The core experimental model does not map to the
problem.

### Gale-Shapley stable matching

**Score: 260/1000**

If this was meant as a one-to-one preference matcher, it is the wrong default
model for property identity. It always finds a stable matching under declared
preferences, but "stable" is not "true," and many of our relations are
one-to-many, many-to-many, temporal, or hierarchical.

**Strongest argument for:** It is deterministic and simple for narrow cases
where two sources both impose preference lists.

**Strongest argument against:** It tends to emit a match even when evidence
should force abstention.

## Final Ranking By Usefulness

1. SAT backbone / model enumeration - 950
2. Maximum common substructure / modular-product clique - 910
3. PQ-trees / consecutive-ones - 875
4. Triangle/quad astrometric pattern matching - 860
5. Exact transform-space max-depth arrangements - 835
6. Patterson / cross-Patterson difference spectra - 830
7. WL / canonical labelling / automorphism - 790
8. Interval-arithmetic point-in-polygon with undecided - 775
9. Hungarian + Murty + reduced costs - 760
10. Laminar-family compatibility - 760

The main correction to the other model's output: several "FOLD" items are not
lesser versions of the selected items, they are different layers. The best
architecture is not one winning analogy; it is candidate generation by relative
geometry, exact constraint enumeration over typed evidence, and abstention via
backbone/automorphism/multiple-optimum certificates.
