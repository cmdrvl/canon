# Decision: port ER techniques natively; retire the concrete matcher adapters

> 2026-07-10 (operator + assessment). Supersedes the "ship OpenRefine/Splink/Dedupe adapters"
> shape of P09. Applies the CMD+RVL house style: when an external tool wraps an open, visible
> technique, port the technique — don't take a subprocess dependency on the tool.

## Finding
None of the three proposed adapters wraps anything opaque:
- **Splink** = Fellegi–Sunter probabilistic linkage + EM-estimated m/u weights (textbook 1969).
  Its unique asset is a SQL scale backend (DuckDB/Spark), not the math.
- **Dedupe** = active learning + a logistic pair-scorer + learned predicate blocking (documented).
- **OpenRefine** = not a matcher at all here (`bd-z4by`); it's a review-GUI interchange ("treat
  OpenRefine as a review *client*"). Its clustering methods are standard and already native in `bd-21nh`.

The plan already requires native to be the complete default (`bd-wv6j`) and requires adapters to
prove marginal lift to justify maintenance (`bd-y2ti`). So adapters are a redundant second layer:
we must build native regardless, then build + maintain + differentially-evaluate each adapter to
justify wrapping math we can own in a few hundred lines of deterministic Rust. Native evidence is
also *trusted, deterministic, fully-provenanced* — strictly more aligned with the exact/reviewed/
replayable thesis than untrusted, nondeterministic subprocess suggestions.

## What is genuinely non-portable (and why it doesn't matter here)
- Splink's SQL scale engine — relevant only at billions of pairs; irrelevant at our scale.
- OpenRefine's GUI — Canon builds its own review flywheel (P04).
- An operator's pre-trained external model — an adoption convenience, kept available via the
  generic seam below.

## Decision
1. **Port the techniques natively** (new beads):
   - Fellegi–Sunter + EM probabilistic scoring as a native evidence operator (P07.9).
   - Learned predicate blocking as a native candidate operator serving the recall gate (P07.10).
   - Active-learning query strategy folded into the review flywheel — the unresolved inbox *is*
     the active learner (P04.10).
   Phonetic/ngram/fingerprint clustering is already native (`bd-21nh`).
2. **Keep the generic seam** `bd-3ofg` (matcher adapter conformance contract) as the sole retained
   adapter surface — an optional bring-your-own external matcher on-ramp, built on demand only.
3. **Retire the three concrete adapters** `bd-cuy5` (Splink), `bd-3m42` (Dedupe), `bd-z4by`
   (OpenRefine): closed, not first-set. Reopen a specific one only if a real operator arrives with
   a real model/workflow to bring. `bd-y2ti` differential eval narrows to native-operator ablation
   (native-only vs baseline), reusing the P12 ablation harness.

## Effect
Same capability, native and trusted; no per-tool subprocess/pinned-env/digest/determinism ceremony;
the thesis gets tighter. The generic `bd-3ofg` seam preserves optionality at near-zero standing cost.
