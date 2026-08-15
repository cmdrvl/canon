# WIZARD_SCORES_CC_ON_COD

Scoring COD's eight against the rubric. Ordered by score, not by COD's ranking.

| Rank | COD # | Technique | Score |
|---|---|---|---|
| 1 | 2 | Astrometric asterism hashing | **810** |
| 2 | 4 | Scene-labeling constraint propagation | **785** |
| 3 | 3 | Maximum common subgraph / association-graph max clique | **780** |
| 4 | 1 | Patterson difference-vector autocorrelation | **650** |
| 5 | 6 | Exact cover for property assemblages | **555** |
| 6 | 5 | Generalized Hough / Radon frontage accumulator | **465** |
| 7 | 8 | Partial Hausdorff + turning-function signatures | **430** |
| 8 | 7 | Phase-only correlation / tile ambiguity function | **330** |

---

## 810 — #2 Astrometric asterism hashing (Groth 1986 / Lang et al. 2010)

The isomorphism is exact, not metaphorical: two coordinate lists of one scene, unknown
similarity transform, spurious detections in both, missed detections in both. That is our noise
model term for term, and I can restate it in one sentence without hand-waving. It catches the
1.8 km ROOFTOP geocode decisively — that point is an outlier to a transform supported by sixty
others, which point-in-polygon cannot see *by construction* because a perfectly good polygon
contains it 1.8 km away.

**Two gaps the proposal does not close.** (a) It never says what replaces RANSAC in the
consensus step. The technique admits deterministic variants (Siegel 1982 repeated median;
exact max-depth cell in an arrangement of integer transform rectangles), but the proposal
doesn't name one, and per the rubric that is unaddressed hand-waving on constraint 2. (b) Cost
is asserted, not computed. C(200,4) = 64,684,950 quads; C(200,3) = 1,313,400 triangles. The
triangle form is genuinely free; the quad form is not, unless restricted to k-nearest
neighbours — 200 × C(8,3) = 11,200 — which the proposal does not state.

- **For:** it is the only item here that recovers the *systematic* inter-source offset, and doing
  so collapses the match gate from ~15 m to ~3 m, an ~25× reduction in chance-match area — about
  4.5 bits of free evidence before any other operator runs.
- **Against:** one similarity transform per tile is right for footprint layers and wrong for
  geocodes, whose error is per-street-segment interpolation. Fit one transform to that
  heterogeneous field and you either fit nothing or fit the majority and silently misregister a
  sub-block — the worst possible failure mode, because it is confident.

---

## 785 — #4 Scene-labeling constraint propagation (Waltz / Montanari / Mackworth, 1972–77)

**The item I most wish I had led with.** I had this machinery in hand and folded it in as a
sub-component rather than making it a headline; COD made it a headline and gave it the right
job. The mapping is precise — feature IS variable, candidate partner set IS domain,
compatibility predicate IS constraint — and it scores well on the three rubric criteria that
usually get fudged, simultaneously.

Its determinism story is the best on either list and COD undersells it: the arc-consistent
closure is the *unique greatest fixpoint*, so AC-3's output is independent of propagation order,
independent of the order rows arrive, and reached in finite steps with no convergence tolerance.
That is a stronger guarantee than "we pinned a seed." Explainability is native: an eliminated
candidate carries a witness ("b was ruled out for a because no value in c's domain is
consistent"). Abstention is native: post-closure domain size > 1, with the survivors enumerated.
It also catches the synthesized-address failure directly — a constraint requiring the house
number to appear in *some* source for that segment refutes the invention with a named witness.

- **For:** it is the only proposal that turns a contradiction into a named refutation rather than
  a low score, which is exactly what constraints 2 and 3 are asking for.
- **Against:** arc consistency alone is *weak* — on real instances it prunes domains but rarely to
  singletons, so you need search, and search reintroduces ordering dependence unless you
  enumerate all solutions. Worse, our evidence is mostly soft, and hard constraints are what
  prune; soft-constraint propagation is a different and much messier animal that COD does not
  address. Also, COD leans on "O(n³) is free" — path consistency is O(n³d³), which at d=5 is
  125× the n³ figure, ~10¹¹ operations. Not free.

---

## 780 — #3 Maximum common subgraph via association-graph max clique (Levi 1973 / Bron–Kerbosch 1973)

Real isomorphism, stated precisely: candidate pair IS association node, pairwise distance
agreement IS edge, maximum common substructure IS maximum clique. Fully integer-exact — the
edge predicate is |d² − d²| against an integer tolerance, plus an i64 orientation determinant
sign — and Bron–Kerbosch under a canonical vertex order has fixed output. All-maximal-cliques
enumeration is a genuine abstention certificate, and COD correctly identifies that. Not standard
practice in geospatial; correctly novel.

It catches something nothing else here does: Microsoft's ID-less footprints matched by *relative*
structure, where a polygon 6 m from its twin still resolves because its five neighbours are
displaced identically. Nearest-neighbour swaps it with a closer wrong building.

- **For:** it decides jointly rather than pairwise, which is the single structural fix to the
  per-feature error that PIP embodies.
- **Against:** COD gives no cost bound, and there is a real one. Max clique is NP-hard; on a
  gated graph it is fine, but the gate has to be *tight*, and the gate is only tight after
  registration. So #3 is silently dependent on #2 and cannot be ranked independently of it. Also
  *maximum* common structure is frequently the wrong objective — a smaller clique with far better
  attribute agreement is usually right, and the weighted fix destroys the clean all-cliques
  abstention semantics COD is selling.

---

## 650 — #1 Patterson difference-vector autocorrelation (Patterson 1934)

The invariance claim is real and precise: the interatomic vector set is origin-free exactly as
the inter-feature difference set is georeference-free. At 200 nodes the direct integer
computation is 40,000 subtractions with no FFT, no raster, no float — the cleanest constraint-5
fit on the list. But **ranking it #1 is a judgment error, and COD's own rationale gives it away**:
"closest structural analogue to the Fischer-Paterson move." That is selecting for resemblance to
the prompt's worked example, not for decision value.

It emits a tile-level statistic and no correspondence. Explainability is weak — a peak height is
not a decomposable per-match score. It catches no named measured failure directly. And it is
largely subsumed by #2, which returns the offset *and* the correspondence. COD also omits the
technique's single best gift: **homometry** (Patterson 1944), the formal 1944 name for two
distinct structures with identical vector sets — which is precisely the calibrated-ambiguity
certificate the rubric prizes, and which names the repetitive-townhouse-row case where every
matcher is otherwise confidently wrong. Leaving that out of a Patterson pitch is leaving the
best part on the table.

- **For:** fifty lines, runs before anything else, and vetoes a whole tile when a portfolio has
  been mis-georeferenced en bloc — a failure that currently produces 150 individually plausible
  wrong matches and no alarm.
- **Against:** it is a guard and a seed, not a resolver, and peak-picking on a binned histogram
  reintroduces exactly the quantisation parameter that an exact arrangement sweep would avoid.
  It also breaks structurally against a source that merges adjacent buildings into one polygon,
  which Microsoft's extraction does routinely.

---

## 555 — #6 Exact cover / Algorithm X (Knuth 2000, over older exact-cover search)

Legitimate mapping — a loan's collateral set partitioning its named members with no gap and no
double-count is a genuine exact cover, and DLX with min-remaining-values column choice is
deterministic, exact, and enumerates *all* covers, so multiple covers is a clean abstention. The
cover is literally the explanation card. All good.

The problem is that it operates on the layer where the problem is easiest. The prompt is explicit
that PROPERTY is a set over *resolved* members; exact cover presupposes the resolution it does
not perform, and a loan names perhaps 1–20 parcels. It is sound plumbing at the top of the stack,
not the forgotten edge the question asked for.

- **For:** the only item that treats PROPERTY-as-set with the right formalism instead of a join,
  and enumerating all valid covers is honest by construction.
- **Against:** it does no resolution work, so its value is bounded by whatever the layers below it
  hand up; it inherits their errors and cannot detect them. If members resolve wrongly, exact
  cover will find a clean, confident, wrong cover.

---

## 465 — #5 Generalized Hough / Radon frontage accumulator (Hough 1962 / Ballard 1981)

**Half real, half metaphor, and COD does not distinguish the halves.** Detecting a collinear
frontage direction by voting in (ρ,θ) is a legitimate exact Hough use. But the claim that
"frontage Hough is the old computer-vision answer to multi-address/range structure" is overreach:
Hough finds *shapes by voting*; range-address resolution is an *ordering-and-interval* problem.
Voting into a 1D accumulator along a street is a histogram wearing a costume, and it has no
mechanism to enforce that "100-105" occupies *consecutive* slots or that odd numbers stay on one
side.

The right tool for that failure is the consecutive-ones property with PQ-trees (Booth–Lueker
1976) or interval algebra — and COD *has* interval algebra at longlist #19 and demotes it as "not
the core scene matcher." That is the mis-ranking: #19 maps to the measured failures better than
#5 does. C1P/seriation appears nowhere on the 30 at all.

Also low novelty: Hough line extraction is thoroughly standard in remote sensing.

- **For:** frontage direction estimation is genuinely useful, cheap, and integer-exact with a
  fixed bin grid.
- **Against:** it does not catch any measured failure by a mechanism I can restate, its headline
  claim about range addresses is unsupported, and binned peak-picking is the classic fragility —
  one true peak split across two bins is a silent miss.

---

## 430 — #8 Partial Hausdorff + turning-function boundary signatures (Huttenlocher et al. 1993 / Arkin et al. 1991)

**Mathematics error, called out explicitly.** COD's rationale says "boundary signatures are
necessary and exact." Turning-function metrics are *not* integer-exact: they require turning
angles (arctangents of integer ratios, irrational) and a minimisation over continuous rotation
and starting-point offset. There is a combinatorial substitute — the sign sequence of exact
integer cross products — but that is a different and much weaker descriptor, not Arkin's metric.
The exactness claim as written is wrong.

Partial Hausdorff is sound and COD undersells it by omitting Rucklidge's branch-and-bound over
transform space (1996), which is the exhaustive deterministic search that would have answered the
RANSAC ban head-on. As given, "partial Hausdorff distance" is just a robust shape score.

Lowest novelty on the list: footprint shape comparison is standard map-conflation practice, and
exact symmetric-difference area on integer polygons already does the job. Catches none of the
seven measured failures — every one of them is about addresses, geocodes, or containment
ambiguity, not footprint outline.

- **For:** the partial (k-th largest, not max) Hausdorff form degrades gracefully under partial
  overlap and digitisation differences, which plain IoU does not, and it is a legitimately useful
  component.
- **Against:** it is a validator for matches that other operators propose, contributes nothing to
  finding them, and is the one item here a competitor is already doing.

---

## 330 — #7 Phase-only correlation / tile ambiguity function (Kuglin & Hines 1975)

The registration problem is real, but this is the wrong instrument under these constraints and
COD does not engage with why. POC requires rasterising point and polygon sets to a grid, then an
FFT — which means floating point, accumulation order, platform-dependent FFT libraries, and
interpolative sub-pixel peak estimation. That is a direct, unaddressed collision with constraint
1. Byte-identical-forever across platforms through an FFT is achievable only by pinning your own
fixed-point transform, and the proposal says nothing about the cost of that.

**It is also strictly dominated by COD's own #1.** At 200 nodes the exact integer difference-vector
histogram computes the same translation estimate in 40,000 integer operations with no raster and
no float. Two of eight slots spent on one idea, and the float-heavy variant kept. COD's longlist
even flags radar ambiguity functions (#28) as "conceptually same as #8" — it de-duplicated within
the correlation family once, then failed to notice the larger duplication with Patterson.

Novelty is also low: phase correlation for image registration is thoroughly standard in remote
sensing.

- **For:** phase-only weighting genuinely is robust to broadband intensity differences, which
  matters when two sources have very different feature completeness.
- **Against:** it violates the hardest constraint, is redundant with a better item in the same
  proposal, and is already standard practice in the adjacent field.

---

## Explicit call-outs

**Already standard practice, therefore not the forgotten edge:** phase-only correlation (#7,
remote-sensing registration); Hough line extraction (#5); Hausdorff and turning-function shape
comparison (#8, map conflation); Procrustes/Kabsch (longlist #15, #25); Hungarian assignment
(longlist #16 — COD flags this itself, correctly).

**Mathematics wrong or misstated:** (a) turning functions are not integer-exact, contra #8's
"necessary and exact"; (b) quad enumeration cost is asserted, not computed — C(200,4) = 64.7M,
free only under a kNN restriction COD does not state; (c) path consistency is O(n³d³), not O(n³),
so #4's tractability claim needs the d³ factor spelled out; (d) #1's write-up omits homometry,
which is the technique's actual ambiguity certificate.

**Structural gaps in the whole set of eight.** Two large ones. First, **nothing calibrates against
local feature density** — a 5 m coincidence in Midtown and in Staten Island are scored
identically, and the astronomy likelihood-ratio/reliability family (Sutherland & Saunders 1992)
and radar OS-CFAR (Rohling 1983) appear nowhere on the 30. That is the missing calibration
engine, and it is the thing that makes reliability collapse automatically when two candidates
compete — the one point inside two legitimate parcels. Second, **nothing measures falsifiability**:
COD has bundle adjustment at longlist #24 and dismisses it as "float-heavy and overkill," which
is backwards — the valuable part of adjustment theory is Baarda's 1968 redundancy numbers and
minimal detectable bias, it is not float-heavy in exact rationals, and it is the only way to know
which of the 95.5% of answers PIP already produces were ever checkable.

**Coverage against the measured failures is thin where the failures are thickest.** Four of the
seven are address/ordering pathologies (synthesized address, five addresses on one lot, range
equals five properties, Queens hyphens). COD's eight address these through frontage Hough (weak)
and exact cover (partial). Interval algebra was identified and demoted; consecutive-ones /
PQ-trees / seriation never appear.

**Genuinely good, and I will say so plainly:** scene-labeling constraint propagation as the
honesty engine. I had the same machinery and buried it as a sub-component of the clique work;
COD promoted it and was right to. The order-independent canonical fixpoint of arc consistency is
a better determinism argument than anything I wrote, and "turns contradictions into named
refutations instead of bad joins" is the correct one-line statement of what this system needs to
do.
