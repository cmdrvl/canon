# WIZARD_IDEAS_CC

Six techniques from outside geospatial and outside record linkage, for cross-source feature
resolution on a ~200-node H3 tile with no shared identifiers.

---

## 1. Triangle / quad pattern matching on coordinate lists

- **Technique.** Pattern matching of two-dimensional coordinate lists via similarity-invariant
  triangles. Astrometry, 1986. Edward J. Groth (*AJ* 91, 1244); productionised as FOCAS
  (Valdes et al. 1995) and astrometry.net (Lang, Hogg, Mierle, Blanton & Roweis 2010).
- **Mapping.** A star list from one telescope plate, with unknown offset/rotation/scale,
  spurious detections and missed stars, **is** one source's feature list in a tile, with
  unknown systematic georeferencing offset, extra features and missing features.
- **Operator.** For each feature take its 8 nearest neighbours, enumerate the triangles, and
  key each by the exact rational pair (mid²/max², min²/max²) of its squared side lengths —
  translation-, rotation- and scale-invariant, and integer-exact. Matched triangle pairs each
  imply one candidate transform; consensus is taken by *repeated median* (Siegel 1982) or by
  the maximum-depth cell of the arrangement of feasible-transform rectangles, both exhaustive.
- **Deterministic and exact?** Yes. Invariants are ratios of integer squared distances,
  compared by cross-multiplication in i128; consensus is a median (pure selection, no
  accumulation) or an integer sweep. No sampling — this is the deterministic exhaustive
  replacement for the banned RANSAC.
- **Catches.** The ROOFTOP-confidence geocode 1.8 km off. It is an outlier to a transform
  supported by sixty other features at ≤6 m residual. Point-in-polygon cannot see it *by
  construction*: 1.8 km away there is a perfectly good polygon containing it, and PIP returns
  that polygon and scores a success. Secondary win: after registration the match tolerance
  drops from ~15 m to ~3 m, shrinking chance-match area ~25× — about 4.5 bits of free evidence
  before any other operator runs.
- **Against it.** The single-similarity-transform model is right for footprint layers but wrong
  for geocodes, whose error is per-street-segment interpolation, not tile-wide. Fitting one
  transform to a heterogeneous error field either fits nothing or fits the majority and
  silently misregisters a minority sub-block. The fix (per-block, per-segment transforms) burns
  the redundancy that makes the estimate trustworthy in the first place.

---

## 2. Maximum common substructure by max-clique in the modular product graph

- **Technique.** Maximal common subgraph via clique detection in the association/modular
  product graph. Chemoinformatics and early relational vision, 1973–76. G. Levi (*Calcolo* 9);
  Barrow & Burstall (*IPL* 4, 83); enumerated with Bron–Kerbosch (*CACM* 16, 575, 1973).
- **Mapping.** Two molecular graphs with no atom identifiers, where the largest mutually
  consistent atom correspondence defines the shared substructure, **is** two source layers in a
  tile, where the largest mutually consistent feature correspondence defines the shared scene.
- **Operator.** Build an association graph whose nodes are gated candidate pairs (a,b) and
  whose edges join (a,b)–(a′,b′) when the pair is geometrically consistent —
  |d(a,a′)² − d(b,b′)²| within an integer tolerance, plus agreement in the sign of the i64
  orientation determinant. Bron–Kerbosch enumerates *all* maximal cliques; the maximum clique
  is the correspondence, and the intersection of all near-maximum cliques is the **backbone**:
  ship the backbone, abstain on its complement.
- **Deterministic and exact?** Yes. The edge predicate is an integer inequality on squared
  distances and an exact integer determinant sign; Bron–Kerbosch with a canonical vertex order
  is a fixed-output combinatorial enumeration. No arithmetic beyond comparison and counting.
- **Catches.** Microsoft's footprints, which carry no identifier at all. A Microsoft polygon
  6 m from its Overture twin still matches, because its five neighbours are displaced the same
  way — relative structure, no transform estimated, no identifier needed. Nearest-neighbour
  will happily swap it with a closer wrong building. It also converts the "one point inside two
  parcels" case from an arbitrary tie-break into two equal maximal cliques and a principled
  abstention.
- **Against it.** *Maximum* common structure is often the wrong objective — a slightly smaller
  clique with far better attribute agreement is usually the right answer, and the weighted
  formulation loses the clean all-maximal-cliques abstention semantics. It also degenerates
  exactly where it hurts commercially: a regular grid of identical warehouses is highly
  symmetric, produces many equal-size cliques, and abstains on the whole tile. That is correct
  behaviour and an unsellable result.

---

## 3. Baarda reliability theory: data snooping, redundancy numbers, minimal detectable bias

- **Technique.** A testing procedure for geodetic networks — the w-test, partial redundancy
  numbers rᵢ, and internal/external reliability. Geodesy, 1968. W. Baarda, Netherlands Geodetic
  Commission, *Publications on Geodesy* NS 2(5).
- **Mapping.** An over-determined survey network in which each observation is checked to a
  computable degree by the others **is** a tile's set of assertions (this point is in that
  parcel, this footprint matches that footprint, this year-built agrees with that one), each
  checked to a computable degree by the rest.
- **Operator.** Write the assertions as observation equations with an integer/rational design
  matrix, form the redundancy matrix, and take its diagonal: rᵢ = 0 means nothing in the tile
  can contradict assertion i, so its "confidence" is unfalsifiable and must not be shipped as
  one. Emit per assertion the normalised residual wᵢ (data snooping flags the single most
  likely blunder) and the Minimal Detectable Bias — "we would have caught an error above 5.1 m
  here; below that we cannot tell."
- **Deterministic and exact?** Yes, if computed in exact rational arithmetic. The normal matrix
  for a tile is ~40×40 with integer/rational entries; exact inversion and 600 diagonal
  quadratic forms are microseconds and are byte-identical across platforms. Floating-point
  Cholesky would *not* satisfy this; rationals do.
- **Catches.** The 95.5% of the book that point-in-polygon already "answers." Nothing today
  distinguishes the answers the tile could have falsified from the ones it could not. The tax
  lot carrying five legitimate street addresses while the parcel layer stores one is exactly an
  rᵢ ≈ 0 situation: the containment claim is unopposed by any other evidence, and the honest
  output is "uncheckable," not 0.9. Baarda additionally tells you *which* missing observation
  would raise rᵢ the most — a deterministic evidence-acquisition policy.
- **Against it.** It is the heaviest lift here: you must commit to a real observation model, and
  the model immediately becomes the thing everyone argues about. Worse, its central assumption
  is a *single* blunder; with several simultaneous errors the residuals smear and data snooping
  confidently blames an innocent assertion. And containment is an inequality, which does not sit
  cleanly in the linear model — reliability theory for inequality-constrained adjustment is
  much less settled than the textbook case.

---

## 4. Likelihood ratio and reliability for catalogue cross-identification

- **Technique.** LR = q(m)·f(r)/n(m) with the derived *reliability* of each candidate.
  Astronomy, 1992. Sutherland & Saunders (*MNRAS* 259, 413), after Richter 1975. Pair it with
  Ordered-Statistic CFAR (Rohling, *IEEE T-AES* 19(4), 1983) for the gate radius.
- **Mapping.** Deciding whether a radio source and an optical source are the same object, where
  the strength of a positional coincidence depends on the *local surface density* of the
  optical catalogue, **is** deciding whether two features are the same building, where a 5 m
  coincidence in Midtown and a 5 m coincidence in Staten Island are not the same evidence.
- **Operator.** For each candidate pair emit separately named terms — positional (against the
  tile's own registration-residual quantiles), class, size, name — each as an integer count of
  millibits from a pinned log table, then add. Reliability Rel_j = LR_j / (ΣLR_i + (1−Q)) falls
  below 0.5 automatically whenever two candidates compete, so abstention is produced by the
  arithmetic rather than bolted on as a threshold; the OS-CFAR gate radius is the k-th smallest
  nearest-neighbour distance in the tile, an exact integer rank selection, not a constant 10 m.
- **Deterministic and exact?** Yes. Scoring becomes integer addition of millibits — commutative,
  associative, no float accumulation order to depend on. The only approximation is a versioned
  integer log table, which is a pinned constant. The gate is a rank selection over integers.
- **Catches.** The 157 points sitting inside two parcels. Point-in-polygon returns one of them —
  smallest area, or first in file — and records a success. Here the two candidates have similar
  LR, both reliabilities land near 0.5, and the row abstains with a named margin. More broadly
  it is the only thing on this list that makes a score in a dense tile comparable to a score in
  a sparse one, which is what "calibrated" has to mean.
- **Against it.** It imports a prior Q — the fraction of book rows that have a true counterpart
  in the tile at all — that must be justified and is noisy when estimated per tile. It also
  assumes at most one true counterpart, which is false for the range-address and multi-address-
  lot cases, so it will abstain on genuinely one-to-many rows unless explicitly extended. And
  its calibration depends on the positional error model; geocode residuals are heavy-tailed,
  and a Gaussian f(r) is badly wrong in exactly the tail that matters.

---

## 5. PQ-trees and the consecutive-ones property

- **Technique.** Testing the consecutive-ones property in linear time with PQ-trees.
  Theoretical CS and genome physical mapping, 1976. Booth & Lueker (*JCSS* 13, 335), on
  Fulkerson & Gross 1965; forbidden-submatrix characterisation by Tucker 1972. The same
  mathematics is Petrie's 1899 archaeological seriation of undated Egyptian graves.
- **Mapping.** A set of clones each covering a contiguous run of an unknown linear genome
  ordering **is** a set of address assertions each covering a contiguous run of building slots
  along an unknown block-face ordering.
- **Operator.** Order the buildings on a block face by exact integer dot product onto the street
  direction, build the assertion × slot incidence matrix, and run Booth–Lueker. If C1P fails,
  the Tucker forbidden submatrix names precisely which assertions conflict; if it holds, the
  PQ-tree's Q-nodes are the forced orderings and every P-node is an explicit, enumerated set of
  buildings the evidence cannot order — a first-class abstention with its members listed.
- **Deterministic and exact?** Yes, maximally so. Projection ordering is an integer dot product
  comparison with canonical id tie-breaks; Booth–Lueker is a combinatorial algorithm whose only
  arithmetic is counting. Zero floating point anywhere in the operator.
- **Catches.** The three address pathologies that no amount of string similarity can touch,
  because none of the evidence is in the string — it is in the arrangement of buildings on the
  ground. "100-105 Broadway" needs 3–5 consecutive slots and the face has five, so the correct
  output is a 1:5 expansion, not the single lot PIP returns and scores as a success. The 756
  Queens hyphens are adjudicated by *geometry*: under the range parse C1P fails against the
  observed building order, under the grid parse it holds. And five legitimate addresses on one
  tax lot appear natively as five assertions on one slot — a legitimate many-to-one, not four
  failures.
- **Against it.** It needs a street centreline and a defensible block-face partition, neither of
  which is in the five listed sources, and it is silent exactly where it is most needed: a face
  with one or two buildings carries no ordering information at all. Corner buildings belong to
  two faces, curved streets make the projection order unstable, and superblocks with internal
  private roads have no linear order to test. It also only helps where addressing is genuinely
  positional — and the vanity addresses and grid hyphens are precisely the exceptions.

---

## 6. The Patterson function and homometry

- **Technique.** The interatomic-vector (autocorrelation) map recovered from intensities without
  phases. Crystallography, 1934. A. L. Patterson (*Phys. Rev.* 46, 372); the cross-Patterson
  translation function from Rossmann & Blow 1962; the ambiguity theory from Patterson's own 1944
  paper on homometric structures.
- **Mapping.** The set of all interatomic difference vectors, which is invariant to the unknown
  phase and to the origin choice, **is** the set of all inter-feature difference vectors in a
  source, which is invariant to that source's unknown georeferencing offset.
- **Operator.** Compute the multiset of all 200² integer difference vectors within source A,
  within source B, and across A→B. The within-source spectra can be compared *before any
  correspondence exists* — a translation-invariant answer to "do these two sources even describe
  the same scene?" — and the peak of the cross spectrum is the registration offset, obtained
  with no correspondence hypothesis and no outlier rejection.
- **Deterministic and exact?** Yes, and this is the cleanest case on the list: integer
  subtraction and histogram counting only. No division, no accumulation order dependence
  (counts commute). Note this *is* the Fischer–Paterson convolution — but at n=200 the direct
  O(n²) integer sum is 40,000 operations, so we get the correlation without ever touching the
  floating-point FFT. The tile bound buys exactness.
- **Catches.** Wholesale mis-georeferencing — a lat/lon swap, a datum error, a portfolio shifted
  en bloc — which currently produces 150 individually plausible wrong matches and no alarm.
  The Patterson overlap collapses and the tile abstains globally, at essentially zero cost,
  before any matcher runs. Its second gift is *homometry*: the 1944 name for two distinct
  structures with identical vector sets, which is the exact condition under which a repetitive
  townhouse row or an identical-warehouse grid is irreducibly ambiguous and every matcher will
  otherwise be confidently wrong.
- **Against it.** It is a global statistic. It tells you the scene matches and roughly by how
  much it is shifted; it never tells you which feature is which, so it can only ever be a guard
  and a seed. It is also largely subsumed by technique #1, which recovers the offset *and* the
  correspondence — if you implement only one, implement #1. It degrades when the two sources
  differ in completeness, and it breaks structurally against a source that merges adjacent
  buildings into single polygons, which Microsoft's extraction does routinely.

---

## Ordering

Best to worst on expected precision gain per unit of implementation risk: **1, 2, 4, 5, 3, 6**.
#1 and #2 are the pair to build first — register, then resolve jointly — because together they
change what every downstream score *means*. #4 makes the scores comparable across tiles. #5 is
the highest value-per-line-of-code and attacks failures nothing else on the list touches. #3 is
the most principled treatment of abstention and the heaviest lift. #6 is fifty lines, runs
first, and can veto a tile.
