# RED TEAM: "Analysis by Synthesis with an Attribute Checksum"

**Verdict: it does not survive. Not because it is a bad idea — the parse-forest instinct is
right — but because the two load-bearing mechanisms are both broken, they are broken in the
same direction, and the architecture's own abstention criterion is structurally blind to that
direction. It will not fail loudly. It will issue clean certificates for wrong answers, and the
wrong answers will all be the same kind of wrong: too few parcels, wrong legal object, wrong
frontage.**

Conventions in this document:

- `[M]` marks a fact taken verbatim from the measured-facts brief.
- `[C]` marks a construction of mine. Constructions are assembled only out of `[M]` shapes —
  I invent no data source, no schema, and no failure mode not already witnessed in your data.

---

## 0. The two fastest kills, before any case walkthrough

### 0.1 Your own evidence says geometry was right and the address was wrong. Step 4 demotes geometry.

Four measured examples of geocode-vs-lot disagreement: 1633 Broadway vs lot 1657 Broadway;
1540 Broadway vs lot 1568 Broadway; 330 West 42nd vs lot 334 West 42; 9 West Fordham Road vs
lot 2167 Grand Concourse `[M]`. In every one of those, **the point is on the correct lot and
the stored address is the misleading channel** — that is stated in the brief as the framing of
the sample.

The architecture's response to four-for-four evidence that geometry localized correctly and the
address string did not is Rule 4: *geometry never proposes a candidate.*

You have taken the dense, complete channel (every lot has a polygon; every lot has an owner)
and made it read-only, and promoted the sparse, lossy channel (one address per lot, for lots
that legitimately have many) to sole proposer. This is backwards on the evidence you collected
to justify it. The stated reason — "the geocode is frequently useless" — is a claim about a
*bimodal* channel: usually good enough for point-in-polygon, occasionally 1.8 km off `[M]`. The
correct response to a bimodal channel is a mode detector (is `accuracy_type` ROOFTOP or
interpolated or `place`? does the PIP lot share a street or a number-neighborhood with any
reading?). Blanket demotion is not a mode detector. It throws away the good mode to defend
against the bad one, and leaves the system with nothing that can localize.

Everything in §1 is a consequence of this one decision.

### 0.2 It is not a code. It is a coincidence being asked to do the work of a code — and under-rated codes miscorrect *confidently*.

The parity-bit analogy is the rhetorical spine of the proposal, so kill it on its own terms.
Error-correcting codes work because four things hold:

| ECC requires | Here |
|---|---|
| The code is *designed* | The attributes were recorded by unrelated parties for unrelated purposes |
| Redundancy is a *known function* of the message | gross↔NRA is a distribution, not a function `[M]`: 10–20%, and that is only the office case |
| The noise model is known | The residual is a mixture of parse error + measurement convention + temporal drift + source error, all unmodeled |
| Rate exceeds noise | ~4 bits of usable checksum (§2.1) against 10⁴–10⁸ enumerated readings |

And the failure mode of a code asked to correct beyond its distance is not graceful degradation.
It is **confident miscorrection to the wrong codeword**, indistinguishable at the receiver from
correct decoding. That is precisely the failure you asked me to find. The architecture does not
merely risk it; the architecture's headline property — "if exactly one reading reconciles, that
is the answer" — is the definition of it.

---

## 1. THE KILL SHOTS

### KILL SHOT 1 — The representative-address trap: the truth is not reachable from the string

**This is the worst one because it is the most common one, and because the correct answer is
physically underneath the evidence the whole time.**

Row `[C]`, built on the measured 9 West Fordham case `[M]`:

```
address_string : "9 West Fordham Road, Bronx, NY"
asserted_size  : 58,000 sf
asserted_type  : Multifamily
asserted_year  : 1928
geocode        : lands inside the lot whose parcel ADDRESS is "2167 GRAND CONCOURSE"  [M]
```

**Step 1 — PARSE FOREST.** One reading. Maybe two with a directional-alias production. This is
a *clean* string: no hyphen, no separator, no a/k/a. The forest machinery contributes nothing
and the enumeration is trivially complete.

**Step 2 — GROUND.** Look up `9 / WEST FORDHAM ROAD`. The lot it is actually on stores
`2167 GRAND CONCOURSE` `[M]`. **The correct lot cannot be reached by any address-keyed lookup
generated from this string, under any normalizer, ever** — the stored representative address is
on a *different street*. So grounding either:

- (a) returns nothing → abstain. Safe, useless, and this is the flagship-asset case; or
- (b) returns the neighbouring lot that *does* store a West Fordham Road address, because you
  loosened grounding to survive the `WEST 42 STREET` vs `WEST 42ND STREET` ordinal problem `[M]`
  and every other normalization you were forced into.

(b) is what a deployed system does, because (a) abstains on so much of the book that nobody
ships it.

**Step 3 — CHECKSUM.** The neighbour is a 1920s Bronx apartment house `[C]`: `BLDGAREA` 64,000,
`YEARBUILT` 1927, `BLDGCLASS` C1. Against asserted 58,000 NRA / 1928 / Multifamily:

- size: +10.3% over asserted NRA — **dead centre of your own measured 10–20% gross-over-NRA
  band** `[M]`. Not merely inside tolerance: this is the *best possible* checksum score.
- year: 1927 vs 1928, inside any band.
- class: C1 ↔ Multifamily, exact.
- contiguity: single lot, vacuously true.
- frontage coverage: the reading names one street, the lot fronts it, 100%.

Unanimous pass.

**Step 4 — GEOMETRIC CORROBORATION.** And here is the trap closing. Point-in-polygon says the
geocode is **not** on the winning lot; it is on 2167 Grand Concourse. Geometry *disconfirms*.
The architecture now has exactly two options:

1. Let geometry veto → then geometry is proposing by elimination, Rule 4 is dead, and you have
   an undocumented arbitration rule between a sparse-address channel and a geometry channel,
   which is the entire system;
2. Treat it as advisory → certificate issues, with a footnote a downstream consumer reads as
   *"the geocode is bad, we already knew that."*

**The architecture has pre-committed to (2)** by declaring the geocode frequently useless and
by writing Rule 4. It has trained itself, by design, to discard the single observation that
would have caught this. Geometric disconfirmation is uninformative in a system whose prior is
that geometry is garbage.

**Step 5 — CERTIFICATE.** One reading survived. Exactly one. Certificate issued. Wrong BBL,
wrong building, wrong owner, wrong lot area, wrong everything except the street name — and the
correct lot was *in the tile, under the point, the whole time*, and was never a candidate
because no derivation of "9 West Fordham Road" can produce "2167 Grand Concourse".

**Why nothing catches it:** the abstention criterion counts *surviving readings*, never
*whether the truth was representable*. The one thing the system most needs to abstain on — "my
candidate universe may not contain the answer" — is the one thing it structurally cannot
detect. Closed-world assumption plus a uniqueness certificate equals a confident wrong answer
every time the world is bigger than the enumeration. And your measured facts say the world is
bigger than the enumeration for a large, non-random, high-value slice: corner lots, assembled
lots, and every lot where the assessor keyed the other frontage. 1633 vs 1657 Broadway,
1540 vs 1568 Broadway, 330 vs 334 West 42 are all the same shape `[M]` — those are Times
Square and Midtown towers, not edge cases.

---

### KILL SHOT 2 — The marketed address resolves to the billing shell, with a perfect checksum

**This one attacks the checksum directly: it produces Δ ≈ 0, the strongest possible evidence in
the scheme, for the wrong legal object.**

Two measured facts collide. First: the parcel layer's representative address is the assessor's
choice, and for big towers it is not the marketed address — lot stores `1568 BROADWAY` while
the property is marketed and securitized as `1540 Broadway` `[M]`. Second: condo unit billing
BBLs overlap their parent lot in the same source and release; a single point falls inside more
than one lot for 157 five-borough properties, max four `[M]`.

Put those together and you get a systematic steering effect that nobody designed:

> **Marketed and securitized addresses live on condo/vanity billing records. Assessor
> representative addresses live on parent tax lots. The property row always carries the marketed
> address. Therefore exact-match grounding systematically resolves to the billing shell, not the
> real estate.**

Row `[C]`:

```
address_string : "1540 Broadway, New York, NY"
asserted_size  : 1,110,000 sf
```

**Step 1.** One reading.

**Step 2.** Ground `1540 / BROADWAY`. The parent tax lot stores `1568 BROADWAY` → no match. The
retail condo billing BBL stores `1540 BROADWAY` → **match**. Exactly one candidate set: `{retail
condo BBL}`.

**Step 3.** Now the checksum, and the outcome depends entirely on an undocumented property of
the source: does the release apportion `BLDGAREA` across unit BBLs, or repeat the whole-building
figure on each?

- If it *repeats* — and repeating is common, because these records are billing artifacts, not
  measurements — then `BLDGAREA` on the unit BBL is the tower's full area. **Δ ≈ 0. A perfect
  score.** The checksum does not merely fail to catch the error; it awards the error its highest
  possible confidence.
- If it *apportions*, Δ is enormous, the reading is vetoed, zero readings survive, abstain.

So the difference between "confidently wrong" and "safely silent" is a per-release convention of
a third-party parcel extract, which is nowhere in the design, nowhere in the certificate, and
will change without notice. That is not determinism; that is determinism conditional on an
unpinned oracle.

**Step 4.** Point-in-polygon confirms — the point is inside the condo BBL *and* the parent, and
the architecture has no concept of *level* (billing lot vs. tax lot vs. condo declaration) with
which to prefer one. It cannot acquire one without a jurisdiction-specific numbering rule table
(NYC's 75xx condo unit convention), which is a hand-maintained constant, which is §5.

**Step 5.** One reading, unanimous corroboration, Δ ≈ 0. Certificate. The answer is a retail
condo billing shell reported as an office tower.

**The general principle behind this, which is worse than the instance:**

The asserted attributes are *not an independent channel*. CMBS/agency annex data is assembled by
vendors who geocode the address string and join to an assessor record to populate size, year,
and type. When that join used the same address string you are now decoding, **the parity bits
were computed from a copy of the message, including its corruption.** A parity bit computed over
the corrupted message does not detect corruption. It *certifies* it — and it certifies it with
Δ = 0, the scheme's strongest signal.

This is unverifiable from the data you have, because the size field carries no provenance. Which
means: **you cannot currently distinguish "the checksum confirmed the reading" from "the
checksum is a tautology on this row."** Until you can, every Δ ≈ 0 is ambiguous between best
evidence and worst failure. That is a fatal property for a mechanism whose entire job is to
be evidence.

---

### KILL SHOT 3 — 305 East 72nd Street: the corroborators actively rank the wrong answer first

**This one is worst in dollar terms: it silently drops a third of the collateral, and every
single check prefers the drop.**

Measured `[M]`:

```
string : "305 East 72nd Street, A/K/A 301-305 East 72nd Street, A/K/A 1392-1396 2nd Avenue,
          A/K/A 1398-1402 2nd Avenue, A/K/A 300-302 East 73rd Street"
lot A  : OWNERNAME "305/72 CONDOMINIUM", LOTAREA 37,800, BLDGAREA 194,949,
         NUMBLDGS 2, YEARBUILT 1961
lot B  : adjacent condo BBL, "1355 1 AVENUE", BLDGAREA 102,719, YEARBUILT 2009,
         on a fourth frontage NOT NAMED in the string
```

Row `[C]`: asserted 172,000 sf, asserted year 1961, asserted 2 buildings.
Truth `[C]`: the collateral is A **and** B — a through-block site where a 2009 tower was added
to a 1961 building and both were financed together. Combined gross 297,668.

**Step 1.** The a/k/a chain is genuinely ambiguous and the grammar must keep both readings:
`A/K/A` as *coreference* (all five denote one thing) and as *enumeration* (five constituents).
Fine. But note what neither reading can contain: **1st Avenue is never named.** No production in
any US-addressing grammar emits "and also the adjacent lot the borrower omitted." The truth is
not in the forest. Again.

**Step 2.** Coreference readings collapse to `{A}`. Enumeration readings expand toward `{A}` plus
whatever the 2nd Avenue and East 73rd ranges ground to — which, given one-address-per-lot, is
mostly nothing, because A already *is* the lot behind all those frontages and it stores only one
of them. So the grounded survivors are dominated by `{A}`.

**Step 3.** Now watch every corroborator vote for the wrong answer:

| Check | `{A}` (wrong) | `{A,B}` (truth) |
|---|---|---|
| size vs 172,000 NRA | 194,949 = **+13.3%**, dead centre of the measured 10–20% band `[M]` | 297,668 = **+73%**, rejected |
| YEARBUILT | 1961, exact match | range [1961, 2009] — either "suspiciously wide" or, if the check is equality, rejected |
| NUMBLDGS | 2, exact match | 3+, rejected |
| BLDGCLASS mix | homogeneous | mixed 1961/2009, penalized |
| contiguity | vacuous, passes | passes |
| frontage coverage | 3 named frontages, 3 covered = **100%** | still 100%; unnamed frontage is invisible by construction |

**The scoring function is maximized at the wrong answer.** This is not "the checksum failed to
discriminate." This is the checksum, plus the year check, plus the building count, plus the
frontage metric, all independently and correctly computing that the under-inclusive set is the
better explanation of the row's assertions. And they are *right* — the row's assertions describe
the 1961 building, because the loan file's description was frozen at underwriting and the scope
of the assertion is not the scope of the collateral.

**The checksum compares magnitudes. The error is a scope error. Magnitudes cannot detect scope
errors.**

**Step 4.** PIP confirms `{A}`. Geometry is forbidden from proposing B `[proposal Rule 4]`.
`OWNERNAME` — "305/72 CONDOMINIUM", sitting right there in your schema, an *actually independent*
channel not derived from the address string, which would match the affiliate that owns B — is
never consulted.

**Step 5.** Exactly one reading reconciles. Certificate. 102,719 sf — 34.5% of the asset — is
silently absent, and the certificate's list of passing scores reads as a proof of correctness.

---

### RUNNER-UP — The tile is an unlisted Step 0 that does the most consequential filtering in the pipeline

Rule 4 says geometry never proposes. But the tile *is* geometry, it is derived from the geocode,
and it proposes the **entire candidate universe**. The purity rule is violated before Step 1
runs, by a step that is not in the list of five and not in the certificate.

Concretely, using the measured chimera: a parser produced street `W 49th St` at ROOFTOP
confidence for "241/249 West 74th Street", 1.8 km away `[M]`. If the tile is centred on that
geocode, the correct West 74th lots are not in the tile, no correct reading can ground, and the
only readings that can ground are the ones consistent with a location 1.8 km from the property.
Whether that yields zero survivors (abstain) or one (certificate) is luck of the draw on street
naming — and "241 West 74th" against a tile of West 49th features will produce exactly one
survivor whenever the tile happens to straddle a numbered-street grid, which in Manhattan it
always does.

Two consequences:

1. **"Abstention is structural, not a threshold" is false.** The count of surviving readings is
   computed over a universe fixed by a radius (or a *k*), which is a threshold, and which
   determines whether the truth is in the universe at all. Structural abstention abstains on
   ambiguity *within* the tile. It never abstains on the tile being wrong. There is no
   "none of the above" hypothesis anywhere in the design.
2. **Confidence tracks parse difficulty, not answer correctness.** The system abstains on
   monster strings (where the parse is hard) and certifies on clean strings (where the parse is
   trivial). But the clean strings are exactly where Kill Shot 1 lives — the failure mode of
   "1633 Broadway" is not parse ambiguity, it is that the lot stores 1657 `[M]`. So the
   confidence signal is measuring a quantity uncorrelated with the thing it is reported as.
   That is textbook miscalibration, and it is baked into the definition of the certificate.

---

## 2. AUTOPSY OF THE CHECKSUM

### 2.1 The bit budget: ~4 bits of code against a 2⁴⁰ message space

Take the acceptance window. To admit the true answer you must cover the measured 10–20% NRA/gross
gap `[M]`, so a half-width of w ≈ 0.12 is the *minimum* honest setting, and that is already
optimistic (it assumes office convention, no temporal drift, no pro-forma, no portfolio total).
Accepted sums span N(1−w) to N(1+w), a multiplicative width of 1.12/0.88 = 1.27. Plausible
totals for CMBS/agency collateral span roughly 5,000 to 2,000,000 sf, a range of 400×.

```
distinguishable size bins = ln(400) / ln(1.27) = 5.99 / 0.239 ≈ 25
usable information         = log2(25) ≈ 4.6 bits
```

At a more realistic w = 0.25 (covering multifamily, retail GLA, and a bit of drift) it is
**3.5 bits.** At w = 0.35 it is **3.0 bits.**

Now the other attributes, and the crucial word is *conditional*. `YEARBUILT` looks like 3–4 bits
in the abstract, but conditioned on the tile it is near zero — a New York block is homogeneous in
age, and the competing readings are lots on the same block. `BLDGCLASS` mix: same, ~1–2
conditional bits. `NUMBLDGS`: 1.5 bits nominal, frequently null or unreliable. Contiguity: near
1.0 for the true answer *and* for every plausible false one, so ~0 bits. Frontage coverage:
computed against the streets the string names, so it is ≈1.0 by construction for any reading
that grounds — ~0 bits.

**Total conditional information: 6–9 bits, generously.**

Isolating one reading out of 10⁴ needs 13.3 bits. Out of 10⁸ (see §4) needs 26.6 bits. So:

> The checksum can only isolate a unique answer once grounding has already reduced the candidate
> count to about 2⁷ ≈ 128 — and in that regime the survivors are near-neighbours on the same
> block whose attributes are maximally correlated, so the conditional bit count is even lower
> than 6.

**The checksum's discriminating power is anti-correlated with the difficulty of the case.** Where
readings differ wildly, grounding already killed the bad ones and the checksum is redundant.
Where readings differ subtly — include the corner annex or not, parent lot or unit BBLs, garage
or no garage — the sums differ by less than the measurement noise. It separates only what was
already separated. It is doing almost none of the work the proposal credits it with, and all of
the work it does is in the regime where it is least reliable.

By the architecture's own numbers this is self-refuting: 10⁴ readings × 2⁻¹² ≈ 2.4 expected
spurious survivors. The design *predicts* that it should abstain on essentially every case it
was built for. Either it abstains always (safe, useless) or somebody widens something to make it
produce answers (fatal). There is no third setting.

### 2.2 "Additive and therefore unforgiving" is exactly backwards

Addition is commutative and lossy: it is a hash with a one-dimensional output over a
combinatorial message space. The multiset of individual areas `{194949, 102719}` carries vastly
more information than the scalar `297668`. **The architecture deliberately projects away the
signal that would discriminate (which lots, individually, at what sizes) and retains the one
projection that cannot.**

Worse, additivity means error accumulates *in a known direction* rather than averaging out. The
10–20% gross-over-NRA gap `[M]` is common-mode across the lots of one property — same measurement
convention, same appraiser — so summing k lots does not shrink the relative error by √k. It
preserves it. The tolerance band must therefore stay proportionally as wide at k=10 as at k=1,
while the *separation* between adjacent readings shrinks as 1/k.

### 2.3 The exact k at which it dies

Discrimination requires that any wrong grounded reading differ from the right one by more than
the full band width 2w·N. For k roughly-equal lots, adding or dropping one shifts the sum by
1/k of the total.

```
w = 0.12  →  discrimination fails when 1/k < 0.24  →  k ≥ 5
w = 0.25  →  fails when 1/k < 0.50                →  k ≥ 3
```

And for *unequal* lots — the normal case — it fails at **k = 2** whenever the smaller constituent
is under 2w of the total, i.e. under ~24% at w=0.12. In New York that describes almost every
corner taxpayer, parking garage, vacant lot, and low-rise annex.

> **Working range of the checksum: k = 1, and gross type errors. It fails at k ≥ 2 for unequal
> lots and universally at k ≥ 5.**

Meanwhile the multi-address separator population — '/' 297 rows, ',' 129, '&' 93, ' AND ' 44,
' A/K/A ' 9, plus bare whitespace, roughly 7–8% of the book `[M]` — *is* the k ≥ 2 population.
**The checksum's domain of validity excludes the population it was built for.** The single-lot
cases where it works are the cases that did not need it.

### 2.4 Four independent mechanisms, all biasing the same direction: drop collateral

1. **NRA < gross** `[M]`. The correct set overshoots the asserted number. Any reading that
   grounds to fewer or smaller lots lands closer. A lot whose gross is 13–15% of the total is the
   lot that will be dropped — and in NYC that is the corner taxpayer, the garage, the annex, i.e.
   precisely the constituents most likely to sit at a different address on a different street,
   i.e. precisely what the parse forest exists to disambiguate.
2. **A/K/A coreference beats enumeration.** Coreference readings collapse to one lot and sum
   smaller; enumeration readings expand and sum larger. Since the target is below gross,
   coreference wins systematically. Assemblages collapse to their largest constituent.
3. **Zero- and null-area parcels are free.** Vacant lots, parking, air-rights parcels, new
   construction, and many condo unit BBLs carry `BLDGAREA` = 0 or null. A lot contributing 0 can
   be added or dropped with *zero* effect on the checksum, so any two readings differing only in
   zero-area lots are **exactly tied** — and the tie-break (§5) decides. The checksum is
   structurally blind to exactly the parcels most likely to be contested.
4. **Full grounding penalizes long readings geometrically.** See §3.2: a k-address reading
   survives with probability p^k. Short readings win. Short means fewer parcels.

All four point the same way. For a CMBS/agency collateral application, **under-reporting the
collateral is the worst possible directional bias**, and it is the one the design manufactures
four separate ways.

### 2.5 When the asserted size is not measuring what you think

Non-exhaustive, all real in this asset class: NRA vs gross `[M]`; retail GLA; multifamily rows
that assert unit count rather than area; hotel rows that assert keys; self-storage net rentable;
as-stabilized / pro-forma square footage from an appraisal that has not been built yet;
whole-complex figures against single-phase collateral; BOMA 2017 vs a 1961 certificate of
occupancy; and **staleness** — the row's size is frozen at securitization while the parcel layer
is current. The measured 305 East 72nd case is literally this: a 1961 building with a 2009
neighbour, where the assessor's figures moved and the loan file's did not `[M]`.

The checksum reads the entire residual as evidence of *parse* corruption. The residual is
actually a mixture of parse error + measurement convention + temporal drift + source error. **You
cannot decode a channel whose noise you have mismodeled**; you will decode confidently to the
wrong codeword, which is §0.2 again, now with named noise sources.

### 2.6 The one part of the checksum that survives

There is exactly one threshold-free, direction-correct, physically-grounded test in the whole
scheme:

```
gross floor area of a candidate set MUST be >= asserted net rentable area
```

That is a **one-sided veto** requiring no tuned constant, no tolerance, and no float. It kills
sets that are physically too small to contain the asserted rentable area. Keep it. It is
genuinely good, and note that it fires on Kill Shot 1's variant where the neighbour lot's gross
(52,000) is below the asserted NRA (58,000) — the two-sided ±12% band is precisely what lets
that through.

**Never let the checksum rank.** The moment you sort survivors by |Δ|, all four biases in §2.4
activate. Veto, do not select.

---

## 3. AUTOPSY OF GROUNDING

### 3.1 The claim "chimera parses die for free" is false, and its converse is fatal

The proposal's Step 2 defence assumes the grounding oracle answers *"is this a real address?"*
It actually answers *"is this the one string the assessor happened to key for this lot?"* Those
predicates are wildly different, and the gap runs in both directions:

**Chimeras do not die.** The observed failure is a *street-level* chimera: "241/249 West 74th
Street" → street `W 49th St` at ROOFTOP confidence, 1.8 km away `[M]`. W 49th Street is a real
street with real lots and real addresses. That chimera grounds beautifully. The "grounds to
nothing" defence works only against chimeric *numbers on the correct street*, and the observed
failure mode is chimeric *streets*. The second measured chimera, "47-19/47-27 a/k/a 47-27 Little
Neck Parkway" → house number "47-10" `[M]`, is a chimeric grid number on the correct street —
and 47-10 Little Neck Parkway is almost certainly a real address that grounds fine.

**Correct parses die.** "9 West Fordham Road" is correct and does not ground, because the lot
stores "2167 GRAND CONCOURSE" `[M]`. "1633 Broadway" is correct and does not ground, because the
lot stores "1657 BROADWAY" `[M]`. "330 WEST 42ND STREET" does not even survive exact byte
matching against the stored "334 WEST 42 STREET" `[M]` — note that the stored string also drops
the ordinal suffix, so you fail on *two* independent grounds.

> **Grounding recall and chimera rejection are the same knob, turned in opposite directions.**
> Tight grounding: correct readings fail, and you get biased answers or blanket abstention.
> Loose grounding (ranges, nearest-number, street aliases, ordinal stripping — all of which you
> are forced into by the ordinal problem alone): chimeras ground, and "they die for free"
> evaporates, leaving the checksum's 4 bits to do all the discrimination.
>
> There is no setting of this knob that satisfies both claims. The proposal's two headline
> efficiencies are each quietly assuming the other one is carrying the load.

### 3.2 Grounding failure produces biased answers, not abstentions — and the bias is quantifiable

This is the question you asked most directly, so here is the arithmetic.

Let p = P(a single correct atomic address exactly matches the stored representative address of
its lot). Your four hand-picked examples are 0-for-4, but those were selected as discrepancies;
call p ≈ 0.7 for a general row, and materially lower for CMBS/agency collateral specifically,
which skews to large lots, corner lots, and assemblages — exactly the lots with many legitimate
addresses and one stored one.

A reading with k atomic addresses fully grounds with probability **p^k**.

```
k=1   p^k = 0.70
k=2   p^k = 0.49
k=4   p^k = 0.24
k=7   p^k = 0.08
```

For the 7–8% of the book with multi-address separators `[M]`, **the correct reading fails to
fully ground roughly three times out of four at k=4** — and whichever shorter or alternative
reading *does* fully ground wins by default.

This forces a choice, and both branches are bad:

- **Require full grounding** → readings are penalized geometrically in their length → the system
  systematically prefers the shortest reading → for a five-a/k/a assemblage it reliably returns
  one lot. This is §2.4 mechanism 4, and it is a mathematical certainty of the design, not a
  contingency.
- **Allow partial grounding** → you must score partial grounding → what fraction of atomic
  addresses must ground, and how is a partially-grounded reading ranked against a
  fully-grounded shorter one? That is a tuned threshold and a tuned weight, in the decision path,
  §5.

Either way: **grounding failure on the correct reading does not produce an abstention. It
produces a certificate for a surviving wrong reading**, because abstention is defined as
*count of survivors*, and killing the truth *reduces* the count toward 1. The abstention
mechanism is inverted with respect to this failure: the more thoroughly the truth is destroyed,
the more confident the output.

That sentence is the single most damaging thing in this document. **Destroying the correct
reading increases the system's reported confidence.**

---

## 4. TRACTABILITY: the 10⁴ estimate is off by four to five orders of magnitude

Take your own measured monster `[M]`:

```
"95-38, 95-40 to 95-44, 96-42 to 96-70 & 95-56 to 95-60 Queens Boulevard,
 63-73 to 63-79 Saunders Street,
 94-14 to 94-24 and 95-11 to 95-19 63rd Drive"
```

Count the ambiguity sources honestly:

| Source | Multiplicity | Count | Factor |
|---|---|---|---|
| Hyphen: Queens grid-separator vs range-separator | ×2 | 13 hyphenated tokens | 2¹³ = 8,192 |
| "to" semantics: endpoints only / all grid numbers / same-parity run | ×3 | 6 occurrences | 3⁶ = 729 |
| Street-name scope: which number-groups attach to which of 3 street names | — | 7 groups, 3 streets | 15 (monotone) to 2,187 (free) |
| Jurisdiction parameter, when the borough is not determined | ×5 | — | 5 |
| Conjunction attachment for "&" / " AND " inside range lists | ×2 | 2 | 4 |
| Street alias sets (63rd Dr / 63 Dr / 63rd Drive; Queens Blvd main vs service road) | ×2–3 | 3 | ~8 |

Conservative product, taking the *monotone* street-attachment count:

```
8,192 × 729 × 15 × 5 × 4 × 8  ≈  1.4 × 10^10
```

Drop the alias and conjunction terms entirely and you still get **4.5 × 10⁸**. The claim was
10² to 10⁴ for the worst observed string. **You are four to six orders of magnitude out on a
string you have already measured.**

Note also that the "hyphen is atomic in Queens" simplification is false in Queens itself: your
own monster string proves Queens uses hyphenated *grid* numbers inside "to"-delimited *ranges*,
so the grammar must keep both hyphen productions live in Queens, which is where the 2¹³ comes
from. The jurisdiction parameterization does not collapse the ambiguity; it only relabels it.

### 4.1 The deeper problem: Earley's guarantee buys you nothing here

Earley/GLR is O(n³) to *build a shared packed parse forest*. The forest is polynomial. **Its set
of yields is exponential.** And the architecture requires the yields, not the forest:

- Step 2 grounds "each reading";
- Step 3 scores "each candidate set";
- Step 5 counts the survivors.

All three are operations on enumerated yields. The tractability is claimed on the parser's
complexity and paid on the enumeration's complexity. This is the oldest mistake in parsing
literature and it is load-bearing here.

Can you push the checks into the forest and prune locally? **Grounding, yes** — it is a local
predicate on an atomic-address node, and this is the right engineering move. **The checksum, no**
— a sum is a global property of a complete reading, not of a parse node. And you cannot factor it
into a dynamic program either, because readings *share lots* (the grid reading and the range
reading of "95-40 to 95-44" both include the lot at 95-42), so contributions are not disjoint and
you would need inclusion–exclusion over an exponential family.

Formally, what you are asking for is:

> **approximate subset-sum restricted to the yield language of a context-free grammar.**

Subset-sum alone has a pseudopolynomial DP. CFG parsing alone is cubic. Their intersection has no
efficient algorithm in general, and the tolerance window makes it the *approximate* variant,
which is where even the pseudopolynomial trick starts to smear.

> **The only sound pruner for the forest is grounding — which §3 showed is the broken one. The
> mechanism you would want to prune with (the checksum) is exactly the one that cannot be
> evaluated without full enumeration. The two headline ideas are computationally incompatible.**

### 4.2 The abstention output is unbounded

10⁸ readings × 2⁻⁴·⁶ bits of size checksum ≈ **10⁷ readings pass the size window**. "Abstain and
name them" is not an output; it is an unprintable certificate. And the fix — cap the named
alternatives at K, or cap the forest at N readings by some traversal order — is a hidden
threshold that silently decides which truths are representable (§5).

---

## 5. DETERMINISM AND EXPLAINABILITY: the constants inventory

The design claims no floats, no thresholds, no tuning. Here is what it actually requires.

### 5.1 The tie-break/abstention contradiction is a live bug in the spec

The proposal states both:

- "ties broken by a specified total order", and
- "abstention is structural: it is the count of readings that survive."

**These cannot both be operative.** If you break ties, you always have one survivor and never
abstain. If you abstain on count > 1, the total order is never consulted. The fact that both
appear means there is an *undocumented rule for when to tie-break versus when to abstain* — and
that rule is the entire system's precision/recall operating point. It is the most important
constant in the design and it is not written down.

Worse, whatever the total order is, it encodes a preference:

- "fewest lots" → encodes under-inclusion, joining the four biases in §2.4 as a fifth;
- "lowest BBL" → encodes a spatial bias toward low block numbers;
- "leftmost derivation" → encodes the grammar author's production ordering as a substantive
  claim about New York real estate.

**A tie-break that decides real cases is a threshold with a formal-sounding name.** And §2.4.3
guarantees it decides real cases: any two readings differing only in zero-area parcels are
exactly tied, always.

### 5.2 Floats are already in the decision path, and they break byte-identical cross-platform determinism outright

Step 4 requires contiguity, point-in-polygon, and frontage coverage.

- **Contiguity** is a topological predicate on polygons with slivers, requiring a snapping
  tolerance. Different GEOS/JTS versions and different platform floating-point produce different
  adjacency answers on near-degenerate shared edges. That is a direct violation of *"byte-identical
  determinism forever across platforms."*
- **Point-in-polygon** on a boundary case needs exact predicates; naive implementations are
  platform-dependent. Your condo case guarantees boundary cases: unit BBLs share edges with their
  parents exactly `[M]`.
- **Frontage coverage** is a ratio of lengths. Float, and it needs a threshold to become a
  decision.

The escape is to precompute adjacency and PIP into a frozen integer graph — at which point
adjacency is a build artifact that goes stale independently of the parcel release, and you have
traded a determinism violation for a silent staleness violation.

Meanwhile the "integers" you are proud of are not native: the parcel layer stores BBL as a string
with a trailing `.0` `[M]`, which proves the pipeline passes through a float64 stage. `BLDGAREA`
comes through the same stage. Float→int requires a documented rounding rule, and null-vs-zero
requires another. Two more constants.

**Net: the no-floats property survives only in the one place it does not matter (adding integers)
and is violated everywhere it does.**

### 5.3 The normalizer is an unacknowledged fitted model

Exact byte match after ASCII-trim is dead on arrival: "WEST 42ND STREET" vs stored "WEST 42
STREET" `[M]`, "1355 1 AVENUE" vs "1355 1ST AVENUE" `[M]`. So you need ordinal stripping,
directional expansion, suffix abbreviation tables, saint/street disambiguation, apostrophes,
numeric-street handling, and borough-specific street alias sets.

Every rule is a constant. The table is large, hand-authored, jurisdiction-specific, and sits
**inside the decision path**. It will be tuned against observed outcomes, with no held-out set,
because there is no labeled ground truth (§6). That makes it a fitted model with no validation —
the exact thing the "no neural networks, no embeddings" constraint was written to prevent,
smuggled in as a lookup table because tables feel deterministic. **Determinism is not the same
property as not-being-fitted.** You have preserved the former and lost the latter, and the latter
is the one that protects you from silent overfitting to the sample you inspected.

### 5.4 The other constants, briefly

| Constant | Where | Why it is not avoidable |
|---|---|---|
| Size tolerance w | Step 3 | Must cover the 10–20% NRA/gross band `[M]`; and the *correct* w differs by property type — office ≠ multifamily ≠ retail ≠ hotel — so it becomes a table indexed by asserted type, **which is itself a checksum input. Circular.** |
| Tile radius / k | Unlisted Step 0 | Determines the closed world; determines whether truth is representable at all |
| Grounding match predicate | Step 2 | §3.1: one knob, two contradictory jobs |
| Partial-grounding score | Step 2 | Required unless you accept the p^k length bias |
| Condo level rule (75xx et al.) | Step 2/3 | Only way to distinguish billing lot from tax lot; hand-maintained per jurisdiction |
| Year-band width, "renovated" handling | Step 3 | Rows say "1961 (renovated 2004)" |
| Forest cap K | Steps 1/5 | Otherwise §4.2 output is unprintable |
| Sentinel list: VARIOUS / DEFEASED / N/A | Step 1 | 4,245 identities `[M]`; without it, "N/A" parses as house number + street |

### 5.5 "Deterministic forever" is false in the only sense a consumer cares about

Determinism across platforms is achievable. **Determinism across source releases is not.** All
five sources version. NYC lots merge and apportion continuously; condo declarations create new
BBLs; the representative `ADDRESS` gets re-keyed. So the same input row will produce a different
certificate next quarter. A certificate is only meaningful relative to a pinned tuple of source
versions, and the proposal does not carry version pins in the certificate. Without them,
"byte-identical determinism forever" is true of the code and false of the system.

---

## 6. WHAT IS MISSING ENTIRELY

Named, in rough order of damage.

1. **Time.** No temporal model anywhere. BBL is not a stable identifier across releases. A 2019
   loan describes 2019 lots against a 2026 parcel snapshot. `YEARBUILT` does not move after a gut
   renovation or a vertical addition, but `BLDGAREA` does, on a lag. The architecture attributes
   the entire attribute residual to *parse corruption* when a large share of it is *temporal
   drift*. Every mismatch is assigned to the wrong cause. This is a mismodeled noise channel and
   it is why §0.2's miscorrection is not hypothetical.

2. **Legal interest versus physical footprint.** The collateral may be a leasehold, a fee position
   in a condo, a ground lease, transferred development rights, or a building on land that is not
   collateral. "Which parcels does it sit on" is sometimes not the right question, and there is no
   type in the architecture that can express "the answer is not a set of parcels." Your own
   measured example is a condominium `[M]` — the case is not exotic, it is the first one you
   sampled.

3. **Provenance on the asserted attributes.** Without knowing whether the size field was measured
   or joined from an assessor record via a geocode, you cannot distinguish an independent parity
   bit from a copy of the message (Kill Shot 2). This is the single missing field that determines
   whether the central idea is sound.

4. **The identity channels you already have and do not use.** `OWNERNAME` (measured: "305/72
   CONDOMINIUM" `[M]`), borrower name, property name, prior loan history, and — decisively — the
   ALTA survey / legal description in the loan documents, which *states the answer exactly* in
   lot-and-block or metes and bounds. The architecture is decoding the one field that does not
   contain the answer while ignoring the field that does. `OWNERNAME` in particular is a genuinely
   *independent* channel: it is not derived from the address string, and matching it across
   adjacent parcels would have caught Kill Shot 3 immediately.

5. **Non-NYC.** Every fact in the brief is New York. The grammar is billed as "US addressing."
   Outside NYC there is frequently no parcel layer with `BLDGAREA` at all, or one per county with
   a different schema, or township-range-section descriptions, or unincorporated areas with no
   street address. Jurisdiction-parameterized productions imply ~3,000 county parameterizations,
   each hand-authored, each untestable. And *agency* multifamily is heavily suburban Sunbelt:
   garden apartment complexes of 30 buildings under one marketed address, spanning 22 parcels,
   none of which store that address. The p^k problem there is not 0.7; it is near zero.

6. **Portfolio rows.** The '/' (297) and ',' (129) separators `[M]` may be enumerating *properties
   in different cities*, not frontages of one site. The architecture assumes one site and one tile.
   A cross-collateralized portfolio row will produce a confident single-site answer.

7. **Source disagreement as signal.** Overture, FEMA USA Structures, and Microsoft GlobalML will
   disagree on footprint count and area for the same building. The architecture pools them into
   one tile rather than treating them as independent witnesses whose *disagreement* is itself the
   strongest available abstention trigger. You have three witnesses and you are averaging them
   instead of cross-examining them.

8. **Nulls and zeros.** `BLDGAREA`, `LOTAREA`, `NUMBLDGS`, `YEARBUILT` are null or zero on a real
   share of lots. Integer arithmetic silently coerces null to 0, and a candidate set of nulls sums
   to 0, which either matches nothing or (with a bad tolerance shape) matches everything. §2.4.3.

9. **No oracle, no calibration, silent failures.** There is no labeled ground truth set. You cannot
   measure the certificate's precision. An architecture whose headline output is a *certificate of
   uniqueness*, with no way to validate the certificate, and whose errors are by construction
   indistinguishable from successes, is unfalsifiable in deployment. Combined with "abstention is
   structural," you cannot even tune the abstention rate honestly, because you cannot measure what
   abstention buys you.

10. **The escalation economics.** Abstention is only a first-class output if something consumes it.
    If 30% abstain and nobody reviews, the system's effective output is the 70% — including every
    confident wrong answer in §1.

---

## 7. WHAT SURVIVES, AND WHAT IT MUST BE REDUCED TO

I tried to kill all of it. Four things survive, and one of them is the whole point.

### 7.1 Keep: enumerate, do not commit

Refusing to pick a parse is the right instinct and it is the genuinely good idea in the proposal.
**Keep it, but move it down a level:** enumerate a bounded set of *candidate lot sets*, not
readings. The lot set is the thing you actually care about, the space is far smaller, and it
admits candidates the string cannot generate (which is the fix for Kill Shots 1 and 3).

### 7.2 Invert: geometry and ownership PROPOSE; the address string CONFIRMS

This is the one change I would insist on.

The parcel layer's `ADDRESS` is **sparse** — one per lot `[M]` — and its geometry and `OWNERNAME`
are **dense and complete**. Use the dense channel to propose and the sparse channel to check.
The proposal does it exactly backwards.

Concretely, the proposer becomes:

```
seed        = point-in-polygon( geocode )                      # all lots containing the point
closure     = seed ∪ { lots contiguous to seed with matching OWNERNAME or affiliate }
candidates  = all subsets of closure that pass the one-sided veto
```

That closure would have proposed the 1355 1st Avenue lot in Kill Shot 3 — the one the string
never names. Nothing else can.

Then the address string is a *scorer*, not a generator: for each candidate lot set, how many of
the string's atomic addresses are explained by some frontage of some member? Explaining an
address is evidence; failing to explain it is a reason to abstain, not a reason to eliminate.

The geocode is still untrusted — but you handle that with a **mode detector**, not blanket
demotion: `accuracy_type` (ROOFTOP vs interpolated vs `place` `[M]`), whether the PIP lot's
street appears anywhere in the string, and whether the PIP lot's number is within a plausible
range of the parsed number. When the detector says "bad mode," abstain — that is the honest
abstention this design is missing, and it is the one that would have fired on the 1.8 km chimera.

### 7.3 Acquire the missing input. This is the real recommendation.

Most of the parse forest exists to compensate for a missing data source. **You do not have an
address-point layer, and you need one.**

For New York that is PAD / Geosupport: the city's own authoritative address→BBL map, which
already contains *every* legal address for a lot including all frontages and all a/k/a's, already
encodes the Queens grid semantics correctly, already knows the hyphen rules per borough, and
already knows that 9 West Fordham Road and 2167 Grand Concourse are the same lot `[M]`. It is
deterministic, explainable, integer-keyed, has no neural network in it, and is maintained by the
jurisdiction that defines the answer.

With it:

- Kill Shot 1 dies outright — the correct lot becomes reachable from the string.
- Kill Shot 2 dies — the parent/unit relationship is explicit, not inferred from a numbering
  convention.
- §3's p^k bias collapses, because p goes to ≈1.
- §4's forest collapses, because the range and grid semantics are resolved by lookup rather than
  by enumerating all grammatical readings and hoping the checksum sorts it out.

Outside NYC the analogue is the county address-point file, or the National Address Database.
Coverage is imperfect, which is fine: **imperfect coverage produces honest abstentions; a
sparse representative-address field produces confident wrong answers.**

The honest summary is uncomfortable: **the problem here is a missing input, not a missing
algorithm.** Once you have the address authority, you need a normalizer and a lookup, not an
Earley parser over a jurisdiction-parameterized CFG. The grammar is an elaborate reconstruction
of information that is available for download.

### 7.4 Keep the checksum, reduced to a one-sided veto

```
KEEP:    reject any candidate set whose summed gross floor area < asserted net rentable area
         (physically impossible; no tolerance; no constant; no float)

DELETE:  ranking survivors by |Δ|
DELETE:  the two-sided tolerance band
DELETE:  year-built and building-count as selectors
```

The veto is free and correct. The selector is where all five under-inclusion biases live, where
the 4-bit budget is spent pretending to be 26 bits, and where the copied-parity attack lands. A
mechanism that can only veto cannot confidently miscorrect.

### 7.5 Redefine abstention: coverage, not count

Add an explicit representability test and abstain on it regardless of survivor count:

- the PIP lot's stored `ADDRESS` names a street the string does not name → **abstain** (Kill Shot 1);
- any lot in the closure is contiguous, co-owned, and unnamed by the string → **abstain**
  (Kill Shot 3);
- the point falls inside more than one lot and no level rule distinguishes them → **abstain**
  (Kill Shot 2, and the 157 measured cases `[M]`);
- the three footprint sources disagree materially on building count → **abstain**;
- the geocode mode detector says "bad mode" → **abstain**;
- the address authority was unavailable for this jurisdiction → **abstain**.

And make the certificate name **what it did not have**, not only what passed: which frontages
went unexplained, which lots in the tile went unexplained, which source versions were pinned,
whether the size field's provenance is known. A certificate that lists only passing scores is a
confidence generator, not evidence.

### 7.6 The constraint that was imposed on the wrong half of the system

Determinism is a property you need on the **acceptance** function. It is not a property you need
on the **proposal** function — you can propose candidates by any means at all, including means
you would never let near a decision, and remain fully deterministic and fully explainable as
long as the accept/reject step is deterministic and the proposal is logged.

By forbidding geometry from proposing *and* forbidding anything learned from the pipeline, you
left the grammar as the sole proposer — and the grammar is provably unable to propose the truth
for the entire 9-West-Fordham class, which is a large, high-value, non-random slice of the book.
**You spent your determinism budget on the half of the system that did not need it, and it cost
you the ability to represent the answer.**

---

## 8. THE THREE MEASUREMENTS THAT SETTLE THIS IN A DAY

You can kill or save this design yourself with three counts over data you already have. You have
not run them, and every claim in the proposal depends on them.

1. **Exact-grounding ceiling.** Over all five-borough rows: what fraction of rows have a parsed
   `house_number + street` that exactly matches (after ASCII-trim) the `ADDRESS` of the lot
   containing the geocode point? *This number is the hard ceiling on Step 2's recall for correct
   readings.* If it is below ~0.9, §3.2's p^k bias dominates the system and the architecture
   systematically returns short readings. My prediction, from your four-for-four sample and the
   `WEST 42 STREET` ordinal artifact: **it is well below 0.9, and below 0.6 for lots over 20,000 sf.**

2. **The empirical tolerance.** Plot the distribution of `BLDGAREA(PIP lot) / asserted_size`. Its
   spread *is* w. If the interquartile range is wider than ~0.25, §2.1 says the checksum carries
   under 4 bits and cannot select among more than ~16 candidates — which means it can never be
   the deciding mechanism it is billed as. Also check the *tails*: if the distribution is
   multi-modal (office vs multifamily vs hotel-keys vs pro-forma), a single w is indefensible and
   a per-type w is circular (§5.4).

3. **The Kill Shot 1 rate.** What fraction of rows have a PIP lot whose stored `ADDRESS` street is
   **not** named anywhere in the address string? Every one of those is a row where the truth is
   unreachable from the string and the system will either abstain or certify a neighbour. That
   fraction is the direct exposure to the worst failure in this document.

If (1) is high, (2) is tight and unimodal, and (3) is near zero, I am wrong and the design is
viable. I do not believe any of the three will come back that way, because your own measured
facts are four consecutive counterexamples to (1) and (3), and a stated 10–20% systematic gap
against (2).

---

## 9. ONE-PARAGRAPH VERDICT

The parse forest is the good idea; keep it and move it down to lot sets. Everything built on top
of it is inverted. The checksum is a ~4-bit code being asked to isolate one codeword out of 10⁴
to 10⁸, its noise model is a mixture of four unmodeled processes attributed entirely to one, its
error is directional and its direction is *drop collateral*, and its best possible score — Δ = 0 —
is produced most reliably by the case where the parity bits were copied from the corrupted
message. Grounding cannot reject chimeras and admit correct readings with the same knob, and its
failures raise reported confidence rather than lowering it, because abstention counts survivors
rather than testing representability. The forest is four to six orders of magnitude larger than
claimed on a string you have already measured, and the checksum is precisely the constraint that
cannot be pushed into the forest to prune it. Determinism is violated by the geometry step, and
"no tuned constants" is violated by at least eight. And the whole edifice exists to reconstruct,
from a grammar, information that the jurisdiction publishes as a file. **Buy the address-point
layer, let geometry and ownership propose, let the string confirm, let the size be a one-sided
physical veto and never a ranker, and abstain on coverage rather than on count.** What is left
after that is small, correct, and shippable — and it is not analysis by synthesis.
