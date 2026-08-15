# GT_ATTACK_CC — adversarial review of the tile analysis

Attacking the claims as stated. I did not open the extract; every objection below
is either internal to the reported claims or points at a computation the analysis
itself says it had the inputs for.

---

## (a) The refusal is wrong-scope, and it cost the whole deliverable

The refusal is *literally* correct and *analytically* lazy. "No polygons" rules out
polygon containment. It does not rule out geometric measurement. The analysis had
points and lengths, which is enough for at least three things it did not do:

1. **The component-size-vs-radius sweep.** Parcel centroids and footprint centroids
   define a bipartite graph at any threshold *r*. Sweep *r* from 0 to 150m, report
   component-size distribution as a function of *r*. That curve is the *entire* test
   of the architecture's "6–20 variables after geometric filtering" claim: either
   there is a stable plateau where components land in 6–20, or the distribution
   jumps from singletons to tens with no usable *r*, and the claim is dead. This is
   computable from centroid floats alone. It was not run. Everything else in the
   report is a side dish.

2. **Centroid-to-boundary lower bounds from area.** Equal-area disc radius
   `sqrt(A/π)` is a hard floor on a parcel's maximum centroid-to-boundary distance
   — no shape of area *A* has a smaller max radius than the circle. A 20,000 ft²
   lot cannot have all its extent inside 24m of its centroid. That is a bound, not
   invented geometry.

3. **Half-diagonal from LotFront/LotDepth.** The analysis says it has length floats.
   Manhattan lots are overwhelmingly rectangular; `0.5·sqrt(front² + depth²)` is a
   near-exact centroid-to-corner distance. A standard 20×100 ft lot gives ~15.5m.
   A 200×100 ft assemblage gives ~34m.

That third number is the tell. It means the reported 31.58m offset is not evidence
that 25m is "too small in general" — it is evidence that 1014477501 is an *outsized
assemblage lot*, which is exactly consistent with the a/k/a ranges and with
"full assemblage extent = not singleton." So the correct refutation of the 25m
filter is far worse for the filter than the one given: the filter is fine for the
80% of lots nobody needs help with, and fails **specifically and silently on the
large assemblage parcels that are the hard cases**. A per-parcel half-diagonal
histogram over all 100 parcels turns an anecdote into a failure *rate* stratified
by lot size. It had the inputs. It reported one counterexample instead.

Verdict: honest refusal, wrong boundary, and the one decisive measurement went
unmade because of it.

---

## (b) The bridge measurement isn't merely non-validating — it's vacuous

The caveat is right and too weak. Measuring components through MAPPLUTO_BBL is
measuring the component structure of an equivalence relation whose classes are
defined by the key you are trying to resolve. "Mean 2.92, max 5" is a fact about
Manhattan building stock — footprints per tax lot — not a fact about a resolution
architecture. It supports the exact-compilation budget by exactly zero.

Worse, publishing it as "meets the budget (only because…)" is how a caveat gets
stripped in the next document that cites the number. The honest cell is
**not applicable**, not a number plus an asterisk.

Second flattery it did not name: if MAPPLUTO_BBL exists, the footprint→parcel edge
needs no constraint propagation at all. It's a deterministic join. An architecture
that "passes" here passes by not being exercised.

Third, it asserted the NYC-specificity caveat without paying the one cheap check
that would have quantified it: **the null / stale rate of MAPPLUTO_BBL across its
own 93 rows.** Footprint BBLs go stale through subdivision and merger. A stale
bridge is worse than an absent one — components look clean and are wrong — and on
a tile whose answer is an assemblage lot, merger-induced staleness is precisely the
failure mode in play.

---

## (c) Missed entirely

- **The tile is centred on the answer.** The 150m window is drawn around the
  rooftop geocode of the correct building. Every statistic reported — component
  sizes, the 2-slot bijection, "strong singleton" — is conditioned on already
  having a good geocode. Production tiles are centred on bad geocodes and
  ambiguous addresses. n=1, and the 1 is favourable.
- **It demolished one unprincipled radius and adopted another.** 25m is attacked;
  150m is treated as ground truth defining "the tile." Same defect, opposite
  treatment.
- **It had the discriminative signal and didn't use it.** Nearest footprint 18.56m,
  second 49.03m. A geocode-to-*footprint* distance with a gap ratio is a far better
  filter than geocode-to-centroid at any constant. The right recommendation was
  never "raise the constant" — it was "change the metric, or normalise distance by
  lot half-diagonal." It refuted a filter and proposed no replacement.
- **Precision theatre.** 31.58m and 18.56m to the centimetre, from WGS84 centroid
  floats and a rooftop geocode whose own positional error is metres. The entire
  25m refutation rests on a 6.58m margin sitting inside its own unquantified error
  bar. The conclusion survives (for the half-diagonal reason above), but the stated
  reason does not.
- **"Building" is not one category.** NYC footprints include garages, sheds, and
  vestibules. The "2 footprints, 2 slots, 2 bijections" result is a mapping between
  two differently-defined notions of building, so the bijection is weaker evidence
  than it reads as. This also sharpens its own five-two-building-parcels point.
- **Edge effect stated, not bounded.** It flagged that footprint centroids can fall
  outside a parcel-centroid window. It did not count how many of the 93 do, which
  is the only version of that flag anyone can act on.

---

## (d) The abstain holds, but it is mislabelled — and "strong" is unsupported

Abstaining is defensible; the label is incoherent with the rest of the output. It
calls 1014477501 a strong singleton *and* abstains on canonical identity — but the
canonical identity **is** the BBL. What is actually missing is the *warrant*: that
the a/k/a address ranges attach to that lot. So this is an abstain on the
address→parcel link, not on parcel identity. Naming it correctly costs nothing and
makes the highest-value-missing-fact ask (PAD/Geosupport) follow directly instead
of arriving as a separate assertion.

The real over-caution is elsewhere: a SINGLETON/ABSTAIN pair discards ranking
information the analysis demonstrably holds. And **"strong" is backed by no
reported number.** It gives the distance to the *second footprint on the same
parcel* (49.03m). It never gives the margin to the **runner-up parcel** — the only
quantity that could make "strong" mean anything. A four-tier stratification whose
top tier rests on an unquantified adjective is the same sin it accuses the
architecture of.

---

## (e) The single most important thing it got wrong

**It declined to produce the component-size-vs-radius curve** — the one artifact
that could confirm or kill the architecture's 6–20-variable claim, fully computable
from the centroids and distances it already had, and the thing that would have made
the 25m refutation a quantified failure rate stratified by lot size rather than a
single anecdote. The refusal in claim 1 was not epistemic caution; it was a scope
error that emptied the deliverable, and the identifier-bridge number was then
offered in its place as if it filled the hole. It doesn't.

Runner-up: every number reported is conditioned on a window drawn around the
correct answer.
