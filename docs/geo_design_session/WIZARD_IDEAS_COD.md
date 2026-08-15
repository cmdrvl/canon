# WIZARD_IDEAS_COD

Forgotten exact techniques for canon geo. This is deliberately not a feature
roadmap. It is a search for old machinery from other fields that maps cleanly
onto CRE building / parcel / POI resolution under canon's constraints:
deterministic replay, named evidence, first-class abstention, tile-bounded
exhaustion, and fixed-point integer geometry.

## The 30-Technique Longlist

These are the candidates I generated before winnowing. The top eight are
expanded below.

1. Patterson difference-vector autocorrelation, crystallography, 1934. Keep.
2. Groth triangle matching / Astrometry.net quad hashing, astronomy, 1986/2010. Keep.
3. Geometric hashing, classical computer vision, 1988. Folded into #2.
4. Maximum common subgraph via association graph and maximum clique, chemoinformatics / graph matching, 1973. Keep.
5. Scene-labeling relaxation, arc consistency, path consistency, classical AI / vision, 1976-1977. Keep.
6. Generalized Hough / Radon frontage accumulator, computer vision, 1972/1981. Keep.
7. Exact cover / Algorithm X / dancing links, combinatorial search, 2000 over older exact-cover search. Keep.
8. Phase-only correlation / ambiguity-function matching, signal processing / radar / image registration, 1975. Keep.
9. Partial Hausdorff distance, classical computer vision, 1992-1993. Kept as part of final #8.
10. Polygon turning-function metric, computational geometry, 1991. Kept with #9 as a boundary-signature operator.
11. Shape contexts, pre-deep-learning object recognition, 2000/2002. Strong but later and less exact than #9.
12. Blum medial-axis skeletons, biological shape / vision, 1967. Good for merged/split building footprints but fragile under minor boundary edits.
13. Hu moment invariants, pattern recognition, 1962. Good cheap rejector, weak explainability.
14. Delaunay / Gabriel / relative-neighborhood graph fingerprints plus WL color refinement, computational geometry / graph isomorphism, 1934/1968. Useful inside #3 as a graph construction/fingerprint.
15. Orthogonal/generalized Procrustes alignment, psychometrics / shape analysis, 1966/1975. Useful after #2/#3 proposes correspondences, not enough alone.
16. Hungarian/Jonker-Volgenant assignment, operations research, 1955. Already scoped in the ambition lane; exact but less novel here.
17. Min-cost flow / max-flow min-cut, operations research, 1956. Good for integral one-to-many assignment, weaker than exact cover for assemblages.
18. SAT/DPLL exact model enumeration, theorem proving, 1960/1962. Good substrate for #5; less domain-specific as the headline.
19. Allen interval algebra, temporal reasoning, 1983. Strong for as-of containment and address ranges, but not the core scene matcher.
20. Formal concept analysis / Galois lattices, data analysis, 1982. Useful for explaining equivalence classes, weak on geometry.
21. Capture-recapture estimators, ecology/survey sampling, late 1800s/1960s. Useful for coverage denominators, not resolution.
22. Error-correcting syndrome decoding, coding theory, 1950s. Tempting for address strings, but too address-centric.
23. Phylogenetic maximum parsimony, systematics, 1960s. Possible for temporal/vintage explanations, weak for same-tile matching.
24. Bundle adjustment / block adjustment, photogrammetry, 1950s. Good for source registration, float-heavy and overkill at tile scale.
25. Kabsch alignment, structural chemistry, 1976. Useful after correspondences; not a correspondence finder.
26. Contact-map alignment, structural bioinformatics, 1970s-1990s. Similar to maximum common subgraph; #3 is cleaner.
27. Mathematical morphology hit-or-miss transforms, image analysis, 1960s. Good footprint prefilter; weak on identities.
28. Radar ambiguity functions, radar/sonar, 1950s. Conceptually same as #8; #8 is the cleaner image-registration form.
29. De Bruijn graph assembly, genome assembly, 1989/1995. Structural analogy is partial observations assembled into a graph, but CRE tiles lack a sequence order.
30. Over-identification restrictions / Sargan-Hansen style tests, econometrics, 1958/1982. Good vocabulary for "channels disagree"; #5 gives a sharper operator.

## 1. Patterson Difference-Vector Autocorrelation

Technique and source: Patterson function / Patterson map, crystallography and
diffraction. A. L. Patterson, "A Fourier Series Method for the Determination of
the Components of Interatomic Distances in Crystals", 1934. The crystallography
problem is: recover an unlabeled atomic arrangement from diffraction intensities
that encode pairwise interatomic vectors, not atom IDs.

Reference anchor: https://doi.org/10.1103/PhysRev.46.372

Structural isomorphism:

- Atom -> building centroid, footprint skeleton node, POI point, or parcel representative point.
- Interatomic vector -> tile-local fixed-point vector between two features.
- Patterson peak -> repeated relative displacement in a source's physical scene.
- Phase problem / unlabeled atoms -> no shared IDs across Overture, FEMA, Microsoft, MapPLUTO, and client rows.
- Crystal ambiguity -> dense Manhattan symmetries where several explanations fit.

Concrete operator specification:

- Inputs: one tile, one entity level at a time, fixed-point metric coordinates, source name, vintage, feature weights, optional per-feature channels such as class bucket, height bucket, area bucket, and level.
- Build for each source a signed integer multiset of pair vectors `(dx_mm, dy_mm, channel_pair)`. For `n` features this is `n * (n - 1)` directed vectors; keep both directions so the spectrum is centrosymmetric and explainable.
- Quantize vector bins by a declared tolerance, for example `vector_bin_mm = 1000` for centroid-level constellation support. Exact predicates still run later; this operator is a support generator.
- Compare two sources by histogram intersection over vector bins and channel-pair bins. Then vote candidate point correspondences from vector agreements: if `(a_i - a_j)` and `(b_p - b_q)` share a bin, increment support for `a_i -> b_p`, `a_j -> b_q` and the reversed orientation when allowed.
- Output evidence:
  - `operator_id = "patterson_vector_spectrum"`
  - `source_pair`
  - `matched_vector_bins`
  - `supporting_feature_pairs`
  - candidate correspondences with `supporting_vectors`, `unique_partners`, `channel_agreement`, `second_best_support`, and `margin`
  - tile-level `spectrum_entropy` and `symmetry_warning`
- Default decision thresholds:
  - source-pair registration support requires at least 6 agreeing directed vectors using at least 4 unique features per source;
  - a feature correspondence is support evidence when its support is at least 3 independent vectors and its support is at least 2x the next candidate for that feature;
  - if the top two transform/vector-support peaks tie within one vector or the spectrum entropy is below the configured floor, emit abstention, not a match.

Determinism and exactness:

- Do not FFT first. At 200 nodes, direct pair enumeration is 39,800 directed vectors per source, which is cheaper and exact.
- Coordinates are tile-local fixed-point integers. Vector differences and bin assignment are integer operations.
- Histogram order is sorted lexicographically by `(dx_bin, dy_bin, channel_pair)`, so byte output is stable.
- If a later fast convolution path is wanted, use an integer number-theoretic transform, not platform FFT, and keep direct enumeration as the reference.

Cost at 200 nodes:

- Per source: `O(n^2)` vector generation, about 40k vectors.
- Pairwise source comparison: linear in sorted histogram length plus votes. With six sources, 15 source pairs. This is trivial at tile scale.

What it sees that PIP/string cannot:

- It sees "this is the same block" even when individual features are unlabeled, shifted, missing, duplicated, or address-hostile.
- It would catch the dense urban block where every candidate building is inside one parcel. Point-in-polygon cannot discriminate after the parcel step; the vector spectrum can say which building participates in the same relative arrangement as FEMA/Microsoft/Overture and which is only co-located on the lot.
- It also gives a tile-level refutation for the 1.8 km wrong rooftop geocode: the wrong tile's vector spectrum around "W 49th St" should not share the expected local constellation with the true W 74th candidate or its asserted attribute context.

How it fails and abstains:

- Repetitive grids, rows of near-identical buildings, and symmetric campuses create many equal vector peaks.
- Major redevelopment across vintages can make the true spectra disagree.
- Abstention signal: multiple vector-spectrum peaks with small margin; many-to-many correspondence supports; low entropy/repetitive-bin warning; or insufficient common-vector mass.

Honest argument against it:

- It is a global support operator, not a final entity matcher. It can tell you that two sources describe the same local arrangement and nominate correspondences, but it does not by itself know parcel/unit/property semantics. In Manhattan grids it may mostly produce principled abstention unless combined with level-specific attributes.

## 2. Astrometric Asterism Hashing

Technique and source: Groth triangle matching for two-dimensional coordinate
lists, later made industrial-strength by Astrometry.net quads. Original field:
astronomy. Original problem: match a photograph's detected stars to a catalog
without knowing translation, rotation, scale, inversion, or sometimes even
rough pointing.

Key names and dates: Groth 1986; Lang, Hogg, Mierle, Blanton, Roweis 2010
Astrometry.net.

Reference anchors:

- https://adsabs.harvard.edu/pdf/1986AJ.....91.1244G
- https://arxiv.org/abs/0910.2233

Structural isomorphism:

- Star in image -> feature point in a source layer.
- Catalog star -> feature point in another source layer or pre-resolved tile anchor.
- Asterism triangle/quad -> invariant local building/POI/parcel constellation.
- Unknown telescope pointing -> unknown source offset, missing features, vendor generalization, and no shared identifier.
- False star / dropout -> source-specific features, demolished buildings, Microsoft features with no stable ID.

Concrete operator specification:

- Inputs: per-level feature representatives in a tile, source pair, optional type/channel constraints, and maximum local neighborhood size.
- For each source, choose stable feature representatives:
  - building: fixed representative point plus optional major footprint vertices/skeleton nodes;
  - POI: POI point;
  - parcel: representative point, but parcel-to-parcel only unless the strategy explicitly asks for containment relations.
- Enumerate local triangles or quads. For quads, choose the most separated pair as the local basis and encode the other two points as rational fixed-point coordinates in that basis. For triangles, encode sorted squared side lengths and orientation.
- Use only local neighborhoods: for each feature, its `k = 8` nearest same-level neighbors by exact squared distance; enumerate triangles/quads inside that neighborhood. This keeps repeated Manhattan grids from dominating.
- Hash invariant codes into bins with declared integer tolerances. Match codes across sources. Each matched asterism votes for point correspondences and for a similarity/affine transform hypothesis.
- Output evidence:
  - `operator_id = "asterism_hash"`
  - matched triangle/quad ids
  - feature correspondence votes
  - transform hypothesis in fixed-point rational form
  - inlier count under exact distance residual
  - `false_positive_guard`: count of equally good asterism explanations
- Thresholds:
  - at least 2 independent asterism matches sharing a consistent transform;
  - at least 4 unique features in the inlier set;
  - transform residual <= declared source-pair tolerance;
  - best transform inlier count exceeds runner-up by at least 2 features or 25 percent, whichever is larger.

Determinism and exactness:

- Squared distances, triangle orientation, basis selection, and residuals are integer operations in the local metric frame.
- Ratio comparisons use cross-multiplied integers, not floats.
- Enumeration order is lexicographic by source id and feature id.
- No RANSAC. Hypotheses are exhaustively enumerated from local asterisms.

Cost at 200 nodes:

- Raw all-triangle enumeration is `C(200,3) = 1,313,400` per source, still not insane, but unnecessary.
- With 8-neighbor local asterisms, roughly `200 * C(8,2) = 5,600` triangles or `200 * C(8,3) = 11,200` quads per source.
- Cross-source hash matching is linear in sorted hash buckets.

What it sees that PIP/string cannot:

- It sees the same physical scene despite missing IDs, shifted coordinates, and source-specific false/missing features.
- It is directly applicable to Microsoft GlobalML, which has no stable ID. The operator can attach Microsoft footprints to Overture/FEMA anchors by local constellation rather than inventing aliases.
- It catches the measured "address channel actively misleads" corner-building cases: two rows can have completely different addresses but sit in the same matched asterism neighborhood.

How it fails and abstains:

- A perfect Manhattan lattice can produce many indistinguishable triangles/quads.
- If a source has too few common features in the tile, no transform has enough inliers.
- If the true difference is non-rigid, a local asterism may support only part of the tile.
- Abstention is natural: no hash peak, multiple equal transform peaks, or transform inliers below support.

Honest argument against it:

- The original astrometry problem tolerates global similarity transforms. CRE tiles are often already in a common coordinate frame, so this may look like overkill. Its value is not transforming coordinates; its value is generating ID-free correspondence evidence under dropout and contamination. If the worked corpus shows source coordinates are already precise and dropout is small, maximum common subgraph may dominate it.

## 3. Maximum Common Subgraph via Association Graph Maximum Clique

Technique and source: maximum common subgraph and subgraph isomorphism. Original
fields: chemical structure matching, graph algorithms, classical pattern
recognition. Key names: Sussenguth 1965 chemical structure matching, Levi 1973
maximal common subgraphs, Ullmann 1976 subgraph isomorphism, Bron-Kerbosch 1973
maximal clique enumeration.

Reference anchors:

- https://link.springer.com/article/10.1007/BF02575586
- https://dl.acm.org/doi/10.1145/321921.321925
- https://research.tue.nl/en/publications/algorithm-457-finding-all-cliques-of-an-undirected-graph-h/

Structural isomorphism:

- Molecule graph -> source-specific tile graph.
- Atom -> building, POI, parcel, or client assertion node.
- Bond -> exact relation: near-neighbor, contains, part_of, frontage-adjacent, same parcel, same H3 anchor, or attribute-compatible relation.
- Maximum common subgraph -> largest mutually consistent cross-source interpretation of the same physical scene.
- Association graph vertex -> possible correspondence `(feature_a, feature_b)`.
- Association graph clique -> set of correspondences that can all be true together.

Concrete operator specification:

- Inputs: two source-layer graphs at the same level, plus optional relation edges across levels for validation.
- Build a graph per source:
  - vertices: features with level, source, vintage, class bucket, area bucket, height bucket, year bucket;
  - edges: integer-labeled relations such as `distance_band`, `bearing_octant`, `touches`, `overlaps`, `contains`, `frontage_same`, `frontage_opposite`, and `same_parcel`.
- Generate candidate correspondence vertices `(a_i, b_j)` only when unary predicates are compatible: same level, compatible class/area/height if present, and no hard cannot-link.
- Add association-graph edges between `(a_i, b_j)` and `(a_k, b_l)` when:
  - `i != k` and `j != l`;
  - the relation label between `a_i,a_k` is compatible with the relation label between `b_j,b_l`;
  - all hard constraints, including within-source mutual exclusion where declared, are respected.
- Enumerate maximum or maximal weighted cliques. A clique is a globally coherent same-level cross-source matching.
- Output evidence:
  - `operator_id = "association_graph_mcs"`
  - clique id, members, total integer score
  - compatibility edges supporting each correspondence
  - rejected correspondences and first violated relation
  - all maximum cliques when tied
  - per-feature domain after clique enumeration
- Thresholds:
  - auto-support only if one maximum clique exceeds the next by at least one high-confidence correspondence or by configured score margin;
  - cliques with fewer than 3 correspondences are support-only;
  - multiple maximum cliques with different assignments for a feature produce abstention for that feature.

Determinism and exactness:

- The association graph is finite and integer-labeled.
- Clique enumeration is exact. Use deterministic pivot and vertex ordering by stable surface ids.
- Weights are integer ScoreUnits. Hard cannot-link is infeasible, not a large negative score.
- No floating convergence, no randomization.

Cost at 200 nodes:

- Worst-case exponential, but tile graphs are sparse after unary compatibility and H3/level blocking.
- If 40 Overture buildings and 45 FEMA buildings yield 200 plausible candidate pairs, the association graph is small. Exact clique enumeration is acceptable and can enumerate all ties, which is the point.
- For full 200x200 impossible candidate grids, this operator should refuse on candidate budget rather than degrade.

What it sees that PIP/string cannot:

- It sees global scene consistency. Greedy PIP can pick one parcel/building independently; MCS asks whether all picks form a mutually consistent block.
- It catches the one-point-inside-two-parcels condo case by allowing the parcel graph to represent `part_of` overlap rather than treating within-source parcel overlap as a hard mutual exclusion. The answer becomes "two compatible parcel interpretations with a typed relation", not "nearest/first polygon wins".
- It catches dense one-parcel/many-building false merges because the building-level graph and parcel-level graph are separate. Same parcel is not enough to merge buildings when their graph neighborhoods differ.

How it fails and abstains:

- It can prefer the largest common subgraph and ignore a small but important real substructure unless scoring penalizes unexplained high-confidence assertions.
- Symmetric campuses and rowhouses can produce multiple maximum cliques.
- Fragmented sources can create many partial cliques.
- Abstention signal: multiple maximum cliques, low clique coverage, incompatible relation edges, or candidate-budget refusal.

Honest argument against it:

- This is powerful but can become a black box if the association graph is not surfaced. The only reason it fits canon is that every clique edge is a named compatibility predicate. Without that discipline, "maximum clique says so" is no better than a learned score.

## 4. Scene-Labeling Constraint Propagation

Technique and source: Waltz-style line labeling, Rosenfeld-Hummel-Zucker
relaxation labeling, Mackworth arc/path consistency. Original field: classical
AI and computer vision. Original problem: objects or line segments in a scene
have ambiguous labels; local relations prune impossible interpretations before
search.

Key dates: Rosenfeld, Hummel, Zucker 1976; Mackworth 1977.

Reference anchors:

- https://web.engr.oregonstate.edu/~sinisa/courses/OSU/CS556/literature/RelaxationLabeling.pdf
- https://www.cs.ubc.ca/~mack/Publications/AI77.pdf

Structural isomorphism:

- Line segment with possible labels -> client assertion or source feature with possible tile entities.
- Vertex compatibility table -> containment, same-level, attribute, frontage, address-set, temporal, and source-vintage constraints.
- Label eliminated by inconsistency -> impossible geocode/address/parcel interpretation.
- Multiple surviving labels -> honest abstention.
- Empty domain -> input refuted by tile.

Concrete operator specification:

- Inputs:
  - variables: client rows, geocode candidates, parsed address members, source features needing attachment;
  - finite domains: candidate POIs/buildings/parcels/properties within the tile and halo;
  - constraints: typed binary/n-ary relations.
- Constraints include:
  - `same_level_same_as_only`: cross-level cannot be same_as;
  - `poi_within_building`, `building_on_parcel`, `property_is_set`;
  - `address_member_present`;
  - `house_number_on_frontage_interval`;
  - `attribute_tuple_rarity`;
  - `source_mutual_exclusion` where declared true;
  - `as_of_overlap`;
  - `coverage_present`.
- Run deterministic node, arc, and path consistency to a fixed point. If needed, enumerate all solutions with DPLL/branch-and-bound after propagation.
- Output evidence:
  - `operator_id = "scene_constraint_consistency"`
  - initial domain sizes
  - each eliminated candidate with exact constraint id and witness values
  - final domains
  - `empty_domain_refutation`, `singleton_resolution`, or `multi_domain_abstention`
- Thresholds:
  - no score threshold for hard constraints;
  - soft constraints are represented as optional support facts, not eliminators, unless the strategy marks them as hard and provenance supports that.

Determinism and exactness:

- Finite domains, sorted ids, exact integer predicates.
- Fixed-point iteration order is deterministic; output logs eliminations sorted by constraint id and candidate id.
- Completeness is explicit: arc/path consistency is a pruning pass; exact search is required if the command claims uniqueness.

Cost at 200 nodes:

- With domain size `d` and constraints `e`, arc consistency is small at tile scale. Even path consistency is acceptable for hundreds of variables when domains are preblocked.
- Full model enumeration can be exponential, but the tile is bounded and budgets can refuse rather than silently guess.

What it sees that PIP/string cannot:

- It catches parsers that synthesize an address appearing in no source. The parsed address member has an empty `address_member_present` domain even if string similarity finds a plausible nearby street.
- It catches the 1.8 km wrong rooftop geocode as an empty or weak domain for asserted street and attributes in the wrong tile.
- It catches the hardest unparsed rows as "no geocode/address domain available" rather than letting them disappear from coverage metrics.

How it fails and abstains:

- Local consistency can leave a globally impossible assignment unless exact search follows.
- If constraints are weak or key layers are absent, many domains survive.
- Abstention signal: non-singleton domains after consistency/search, empty coverage domains, or multiple exact satisfying assignments.

Honest argument against it:

- It is not a matcher by itself; it is a correctness shell around candidate evidence. If the upstream candidate generators are weak, it will mostly abstain. That is acceptable for canon, but it means this is the best "honesty engine", not necessarily the highest-recall engine.

## 5. Generalized Hough / Radon Frontage Accumulator

Technique and source: Hough transform and generalized Hough transform, classical
computer vision. Original problem: detect lines, curves, and arbitrary shapes
by letting local evidence vote in parameter space.

Key dates: Hough patent 1962; Duda and Hart 1972; Ballard generalized Hough
transform 1981.

Reference anchors:

- https://doi.org/10.1145/361237.361242
- https://doi.org/10.1016/0031-3203(81)90009-1

Structural isomorphism:

- Edge pixel -> parcel/building boundary segment or observed address point on a frontage.
- Line/shape parameter -> street frontage, side of street, address-number coordinate, corner/through-block shape.
- Accumulator peak -> a coherent address frontage or multi-frontage property hypothesis.
- Spurious edge -> parser artifact, alias address, Queens grid hyphenate, or unrelated frontage.

Concrete operator specification:

- Inputs: tile polygons, optional street centerlines/address-set source, parsed address members, house numbers, source address strings, fixed-point boundary segments.
- Build frontage candidates:
  - detect boundary segments adjacent to street centerlines or inferred street-bearing clusters;
  - assign each segment `(street_key, side, segment_id, number_interval_if_known)`;
  - for each parsed address member, vote into `(street_key, side, house_number, frontage_interval)`.
- Range and multi-address fields become sets of votes, not a single normalized string.
- Use a discrete accumulator:
  - `street_key`
  - side/parity class
  - integer house-number coordinate
  - interval start/end
  - frontage group id for corner/through-block unions
- Output evidence:
  - `operator_id = "frontage_hough"`
  - accumulator peaks
  - address tokens voting for each peak
  - boundary/frontage segments supporting each peak
  - parser tokens that do not vote anywhere
  - `chimera_parse` when house number and street name vote to different frontages
- Thresholds:
  - address member is supported when its `(number, street)` vote lands on a frontage peak with at least one observed source address or boundary/frontage witness;
  - range assemblage support requires contiguous interval support on one or more declared frontages;
  - conflicting equal peaks across frontages produce abstention.

Determinism and exactness:

- Hough bins are explicit integer parameters, not floating bins.
- Line/frontage equations in the tile-local metric frame use fixed-point coefficients reduced by gcd.
- Vote accumulation is integer addition in sorted bins.
- No random sampling; all candidate segments and address members vote.

Cost at 200 nodes:

- Boundary segments per tile may be in the low thousands. Accumulator updates are linear in segments plus parsed address members.
- Multi-frontage interval enumeration can grow, but the accumulator exposes that count directly.

What it sees that PIP/string cannot:

- It sees that "199 E 12th St" is a chimera when the number votes to First Avenue and the street votes to East 12th Street.
- It sees that "100-105 Broadway" is an interval over frontage members, not a point in one parcel.
- It avoids destroying the 756 Queens hyphenated grid addresses: a hyphenated house number only becomes a range if the frontage/parity convention supports it.

How it fails and abstains:

- Without an address-set or street-centerline source, the accumulator is weaker and may only detect contradictions, not resolve.
- Corner and through-block properties produce multiple legitimate peaks.
- Informal building names with no house numbers do not participate.
- Abstention signal: no frontage peak, equal incompatible peaks, or parser tokens split across frontages.

Honest argument against it:

- This is still partly address machinery, and canon geo's measured failures prove the address channel can lie. Its correct role is to refute and structure address evidence, not to override geometry or attribute anchors.

## 6. Exact Cover for Property Assemblages

Technique and source: exact cover / Algorithm X / dancing links. Original field:
combinatorial search and exact-cover puzzles/tilings. Knuth's DLX paper is 2000,
but the underlying exact-cover and 0-1 integer search idea is older. Original
problem: enumerate all subsets of rows that satisfy each required column exactly
once, with optional columns allowed.

Reference anchor: https://arxiv.org/abs/cs/0011047

Structural isomorphism:

- Board square to cover -> asserted property obligation: address interval slot, required frontage, area band, type, as-of membership, contiguity component.
- Polyomino placement -> candidate member subset: one building, one parcel, one building-over-many-parcels, or multi-building campus member.
- Exact cover solution -> one property membership set.
- Multiple exact covers -> indistinguishable assemblages; abstain.

Concrete operator specification:

- Inputs: one client property assertion, resolved tile members at POI/building/parcel levels, typed relations, address/frontage evidence, area/type/year measures, and declared measure semantics such as NRA versus gross.
- Build candidate rows:
  - single building;
  - building plus containing parcels;
  - contiguous parcel interval on a frontage;
  - union of intervals across frontages for corner/through-block;
  - campus/garden-apartment connected components.
- Build columns:
  - primary columns: required address members/frontage intervals, required entity-level membership slots when known, as-of interval, hard type constraints if observed and trusted;
  - secondary columns: optional aliases, soft contiguity, area tolerance buckets, modelled occupancy, source coverage.
- Enumerate all covers with deterministic Algorithm X over sorted columns. For non-exact numeric area, add bounded subset-sum columns representing integer area residual bands.
- Output evidence:
  - `operator_id = "property_exact_cover"`
  - all feasible covers, not just winner
  - chosen cover if unique
  - uncovered primary columns
  - conflicting rows
  - area residual, measure conversion used, frontage intervals, contiguity witnesses
  - margin to second-best cover
- Thresholds:
  - unique exact cover with all primary columns covered -> strong support;
  - no cover -> abstain/refute depending on coverage;
  - multiple covers with equal primary satisfaction and close residual -> abstain;
  - modelled-only type evidence cannot be a primary eliminator.

Determinism and exactness:

- Algorithm X is deterministic when the next-column heuristic has stable tie-breaking.
- All row/column incidence is Boolean or integer-banded.
- Area residuals are integer square feet or square millimetres after declared conversion.
- Enumerating all covers is critical: the abstention signal is the solution count and margin.

Cost at 200 nodes:

- Not `2^200` if candidates are generated from frontage/type/contiguity filters.
- Case 100-105 Broadway shape should have single- or low-double-digit member candidates after frontage and type filtering.
- Worst-case exact-cover explosion is a valid refusal because it means the evidence does not identify the property.

What it sees that PIP/string cannot:

- It catches the measured range-address failure where "100-105 Broadway" is five properties on five lots but point-in-polygon returns one lot and looks successful.
- It also catches dense multifamily/campus properties where the answer is a set spanning parcels/buildings and no single BBL can express it.

How it fails and abstains:

- Condos and air rights break additivity; the correct member may be a fraction of a parcel or building.
- If the source record's size measure is undeclared, subset-sum can be misleading.
- Through-block/corner frontages can create many covers.
- Abstention signal: multiple covers, no exact cover, fractional/condo detector, or enumeration-budget refusal.

Honest argument against it:

- This does not solve source-to-source building resolution. It solves the property layer once lower-level members and relations exist. It is still top-eight because PROPERTY is the client's actual question and exact cover is the cleanest old formulation of "property is a set".

## 7. Phase-Only Correlation / Tile Ambiguity Function

Technique and source: phase correlation and matched filtering, signal
processing, radar/sonar, image registration. Original problem: estimate the
translation/pose between two noisy images or signals by cross-correlation in a
frequency or accumulator domain.

Key names and dates: Kuglin and Hines phase correlation, 1975; Reddy and
Chatterji FFT-based translation/rotation/scale image registration, 1996.

Reference anchors:

- https://www.scienceopen.com/document?vid=250786ec-dc88-4779-af17-887241e8aa0e
- https://dev.ipol.im/~reyotero/bib/bib_all/1996_Reddy_Chatterji_fft_based_trans_rot_scale_invar_registr.pdf

Structural isomorphism:

- Image intensity -> tile raster channel: footprint occupancy, edge occupancy, centroid impulses, POI category impulses, class/height/area channels.
- Image translation -> source offset, geocode displacement, or wrong candidate tile.
- Cross-correlation peak -> displacement/pose at which two source observations explain each other.
- Flat or multi-peak correlation -> tile is repetitive or evidence is insufficient.

Concrete operator specification:

- Inputs: two source layers or one client assertion tile and candidate neighboring tiles; fixed local metric frame; raster resolution such as 1 m or 2 m; channel definitions.
- Rasterize decision-fidelity geometry into integer grids:
  - centroid impulse channel;
  - building footprint occupancy;
  - boundary/edge occupancy;
  - class/height/area binned channels.
- Compute cross-correlation for a bounded displacement window. Prefer direct integer convolution or NTT for exactness; if FFT is used, it is only a proposal path and direct integer correlation verifies the peak.
- Output evidence:
  - `operator_id = "phase_correlation_tile"`
  - top displacement peaks and integer scores
  - per-channel contribution to each peak
  - features/cells supporting the peak
  - displacement vector in millimetres
  - peak-to-sidelobe ratio
- Thresholds:
  - support when top peak has peak-to-sidelobe ratio >= 2 and at least two independent channels support the same displacement;
  - if top peak and runner-up differ by less than a configured integer margin, abstain;
  - no peak in bounded window -> geocode/tile refuted or coverage absent depending on coverage predicate.

Determinism and exactness:

- Rasterization is fixed-point and deterministic.
- Direct correlation is exact integer arithmetic. NTT is also exact when modulus and transform size are pinned.
- Platform FFT is not acceptable as the decision path because of float-order nondeterminism.
- The operator emits a peak as evidence, not a verdict.

Cost at 200 nodes:

- A 1 km tile at 2 m resolution is a 500x500 grid. A handful of sparse channels can be correlated directly over a bounded window, or densely via NTT/FFT proposal plus exact verification.
- At tile scale, even direct sparse correlation over occupied cells is often cheaper than transform setup.

What it sees that PIP/string cannot:

- It sees systematic displacement and wrong-tile geocodes. For the 1.8 km ROOFTOP error, the wrong candidate tile has no street/feature/channel correlation with the asserted property context, while an expanding bounded search can reveal a strong peak near the true W 74th neighborhood.
- It also detects vendor layer offsets before entity scoring, preventing a cascade of false mismatches.

How it fails and abstains:

- Pure translation is too simple for non-rigid source differences.
- Rectangular Manhattan blocks create periodic sidelobes.
- Rasterization can blur small geometry unless verified by vector predicates.
- Abstention signal: low peak-to-sidelobe ratio, multiple equal peaks, single-channel-only peak, or peak unsupported by exact vector features.

Honest argument against it:

- This is the most "Fischer-Paterson-looking" idea mathematically, but not the best final matcher. It loses vector fidelity and produces displacement evidence, not entity correspondences. Its honest role is as a deterministic tile/geocode/source-registration operator feeding the graph and CSP operators.

## 8. Partial Hausdorff plus Turning-Function Boundary Signatures

Technique and source: classical shape matching in computer vision and
computational geometry. Original problem: compare model and image shapes under
translation, rotation, scale, noise, and partial occlusion.

Key names and dates: Arkin, Chew, Huttenlocher, Kedem, Mitchell polygon
turning-function metric, 1991; Huttenlocher, Klanderman, Rucklidge Hausdorff
image matching, 1992/1993.

Reference anchors:

- https://www.cs.cornell.edu/~dph/papers/ACHKM-TPAMI-91.pdf
- https://doi.org/10.1109/34.232073

Structural isomorphism:

- Model shape -> one source footprint.
- Image shape -> another source footprint or client geometry.
- Occlusion / missing edges -> vendor generalization, clipped parcels, partial building footprint, merged/split footprint.
- Shape distance -> named geometric support or rejection independent of address strings.

Concrete operator specification:

- Inputs: two same-level polygons or multipolygons in the tile-local metric frame, decision-fidelity rings, optional transform from asterism/Patterson/correlation operator.
- Compute boundary signatures:
  - directed partial Hausdorff distance from A boundary samples to B and B to A;
  - percentile Hausdorff, such as 90th and 95th percentile, to tolerate minor extra vertices;
  - turning function distance for closed polygon outline, using cumulative arclength and turn angle represented as rational/integer steps;
  - area ratio and bounding-box relation as cheap filters;
  - unmatched boundary arcs as explainable residuals.
- Output evidence:
  - `operator_id = "boundary_signature_match"`
  - directed distances in millimetres
  - percentile distances
  - turning-function distance
  - unmatched arcs and their lengths
  - area-ratio interval
  - final support/reject/abstain classification
- Thresholds:
  - strong footprint support if bidirectional 90th percentile Hausdorff <= source-pair tolerance and area-ratio interval lies within strategy bounds;
  - reject if minimum directed distance exceeds hard separation or if turning-function distance exceeds shape threshold;
  - abstain when one direction is good and the other bad, which indicates containment, clipping, or merged/split geometry rather than same_as.

Determinism and exactness:

- Boundary samples are generated by integer arclength stepping in the local metric frame.
- Segment distances use exact integer squared distances and rational comparison.
- Turning functions are step functions over integer edge lengths and exact signed turns. Comparing step functions can be exact over rational intervals.
- Polygon intersection for IoU is the only hard exactness caveat; use interval bounds and abstain if a threshold is straddled.

Cost at 200 nodes:

- Pair cost is roughly `O(v_a * v_b)` for exact segment distances if implemented directly, with cheap bounding boxes first. At tile scale and after candidate blocking, this is fine.
- Turning-function comparison for polygons with `m,n` edges is near-linear after normalization.

What it sees that PIP/string cannot:

- It sees that two corner addresses can describe the same physical building. Address string disagreement says "different"; boundary signature says the footprint is the same.
- It sees when one tax lot carrying five legitimate addresses is not a building identity: one parcel boundary may contain or intersect several building boundary signatures, producing containment/part_of evidence rather than same_as.

How it fails and abstains:

- Many city buildings are rectangles of similar size; shape alone can be indistinguishable.
- Vendor generalization can erase facade details.
- Merged footprints and split footprints create one-directional matches.
- Abstention signal: asymmetric directed Hausdorff, multiple equally close shapes, clipped geometry, or threshold-straddling IoU interval.

Honest argument against it:

- This is closest to what a geospatial engineer might already know, so it is less "hidden mathematics" than Patterson or asterism hashing. Its value is not novelty; it is replacing crude PIP/buffer with a named, exact, explainable shape-distance operator that can say "same footprint", "contains", "clipped", or "indistinguishable" rather than just nearest.

## Ranking Rationale

1. Patterson vector spectra are the closest structural analogue to the Fischer-Paterson move: treat the whole tile as a signal whose pairwise differences are the invariant.
2. Asterism hashing is the cleanest "no shared identifier, same physical scene" import and gives correspondences, not just similarity.
3. Maximum common subgraph turns local pair evidence into global consistency and all-solution abstention.
4. Constraint propagation is the honesty engine: it turns contradictions into named refutations instead of bad joins.
5. Frontage Hough is the old computer-vision answer to multi-address/range structure.
6. Exact cover is the right old combinatorial form for PROPERTY-as-set.
7. Phase correlation is powerful for geocode/source registration, but weaker as final identity evidence.
8. Boundary signatures are necessary and exact, but less surprising and less global.

The most promising composite is not one algorithm. It is:

1. Patterson/asterism operators produce ID-free same-scene and correspondence evidence.
2. Boundary signatures validate same-level geometry.
3. Association-graph MCS enforces global same-level consistency.
4. Scene-labeling CSP carries containment, source coverage, address/frontage, and temporal constraints.
5. Exact cover constitutes document-scoped PROPERTY sets only after lower levels are resolved or abstained.

That composition preserves canon's doctrine: every step emits named scores and witnesses, every ambiguity remains enumerable, and no technique has to pretend that a ranking is a decision.
