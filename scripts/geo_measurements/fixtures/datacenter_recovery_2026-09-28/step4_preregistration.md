# Step 4 pre-registration: entity-level replay with evidence-justified opposition (2026-09-29)

Committed with `step4_replay.py` BEFORE it is run on any case and before any label-based scoring of its output. Nothing here may change after scoring; a change is a new experiment revision.
Code under test: branch `geo-prefer-absent-diagnostic` (`cost_if_present` on `GeoSoftPreference`, `prefer_absent_member` observation). Baseline: installed canon 0.14.0.

## Fixed rules (see the script's docstring for exact definitions)
- Cases: Pryor, DEV-1, H1-P, on the frozen snapshots and the entity files from `step4_entities.py`. Targets: "google", "microsoft", "microsoft".
- Universe: every physical entity within 1,300 m of any anchor with a coordinate; no size filter.
- Support weights: partition 5, module 3, cross-layer 3. Cross-layer counts distinct declared upstreams (>= 2), not rows.
- Opposition: weight 5, only from a non-closed Foursquare place whose name lacks the target token and whose point lies inside a footprint of the entity. Proximity, missing support, size and distance never create opposition.
- Readout: separable soft optimum (`core` strictly included; `upper` = core + ties), cross-checked against the kernel's rank-1 on subsets of 8, 10, 12 and 12 candidates. If the cross-check disagrees on any subset, the readout is not reported and the disagreement is investigated first.
- Comparison: baseline (no opposition, installed 0.14.0) versus patched (with opposition), same universe and same support.

## Measures (numbers only; per case, not pooled)
1. Opposition count and its examples (name, footprint) so each can be inspected.
2. Change in `core` and `upper` sizes between baseline and patched.
3. Evaluator-side, against the sealed labels (DEV-1, H1-P only): site entities in `core`, in `upper`, and non-site share of each, baseline versus patched; and whether any site entity was moved from core/upper to excluded (loss of truth).
4. Pryor: whether the readout stays ambiguous; no singleton is required.

## Decision rule (from the response's section 8, applied unchanged)
- Substantial useful discrimination that preserves site entities and transfers across DEV-1 and H1-P: continue the member formulation cautiously.
- Marginal effect (including zero opposition on a case): do not add weights, penalties, filters or providers; report that the first-level variable is likely wrong and prototype site hypotheses.
- Any site entity removed from `upper` by opposition: stop, and revise the translation.

## Known limits, stated in advance
Two labeled cases, one rule set, one weight; the evaluator's site boundaries are inferred from label cues; the H1-P label discrepancy is unreconciled; I hold one label detail for H1-P from an earlier evaluator reply. No accuracy claim.
