# External Candidate Import

This is the disposable bridge for using an external seeded scorer as a
bootstrap candidate oracle on genuinely multi-column corpora, then importing
operator-authored decisions through Canon's existing native review import.

It does not add a Canon runtime adapter, public command, registry mutation path,
or promotion shortcut. The external scorer proposes; `canon entity review import
--source-review` validates the source-review hash, decision binding, registry
snapshot hash, exact mode context, and contradiction rules before emitting a
patch receipt.

## Intended Surface

Use this only where a record has several independently useful identity columns.
The BDC loan/investment surface is the reference shape:

- issuer name
- coupon or rate
- maturity date
- security or instrument type

Do not use this recipe for single-column vocabularies. Splink's own guidance
describes it as "not designed for a single bag-of-words column"; servicers,
sponsors, instrument types, and other one-column vocabularies stay in pure
Canon review, calibration, and promotion.

## Offline Recipe

1. Run the normal Canon entity workflow to produce run/link artifacts and a
   native review artifact:

   ```bash
   canon entity run <PROJECT_OR_INPUTS> --emit json > run.json
   canon entity review export link.json --artifact native-review --emit json > native-review.json
   ```

2. Run the external scorer outside Canon. For Splink, pin every source of
   randomness and persist the scoring model next to the proposed decisions:

   - call `estimate_u(seed=...)` with a recorded seed
   - train EM only from deterministic rules
   - score review-queue pairs directly, for example with `compare_records`
   - persist the final model with `save_model_to_json`

3. Convert scored candidates into native review decisions in an operator-owned
   script outside this repository. The script must read the saved model JSON,
   the native review artifact, and the scored candidate file; it must write only
   Canon's existing `canon_entity_native_review_decisions.v0` envelope.

4. Author decisions under these rules:

   - `alias`: only when the score is at or above the operator-set probability
     threshold and the source review item has no cannot-link conflict.
   - `cannot_link`: only when the source review item already carries a
     corroborating native negative cue, such as amount, date, category, or
     instrument-type contradiction. The external scorer must not invent
     cannot-link authority.
   - `defer`: every candidate below threshold, every candidate with incomplete
     corroboration, and every candidate whose intended action is not allowed by
     the native review item.

5. Import through the existing Canon validator:

   ```bash
   canon entity review import external-decisions.json \
     --registry <REGISTRY_DIR> \
     --next-version <NEXT_VERSION> \
     --source-review native-review.json \
     --emit json > native-import-receipt.json
   ```

   The native import path emits a patch receipt only. It does not read or mutate the registry,
   and it does not consume `--next-version` beyond the existing CLI compatibility requirement.
   Audit, promote, and exact replay proceed through the normal Canon gates.

## Provenance Bundle

Commit or archive these files together for every bridge batch:

- Canon input manifest and input content hashes.
- Canon run/link/review artifact hashes.
- External scorer corpus extract hash.
- External scorer seed and deterministic blocking/training rules.
- Saved model JSON hash.
- Scored candidate file hash.
- Converted native decision file hash.
- Native import receipt hash.

The receipt is useful only because it is bound back to the native review artifact
and its source hashes. Do not present an external score file by itself as Canon
evidence.

## Sampled Spot Check

Before promotion, predeclare the denominator and sampling seed in the run note.
The operator samples all mechanically authored `cannot_link` decisions and the
larger of 25 decisions or 5% of mechanically authored `alias` decisions, ranked
deterministically by descending external probability and then by review ID. Any
sampled contradiction cancels mechanical promotion for that batch; the affected
items remain in review until corrected and re-imported.

Timeouts, censored attempts, and rows that could not be scored stay inside the
denominator. Do not edit the denominator after seeing the result.

## Fixture

The offline fixture in `tests/fixtures/entity/external_import/` is intentionally
tiny:

- `bdc_investments.csv`: four BDC investment rows.
- `splink_saved_model.json`: saved model provenance with a fixed seed.
- `pre_scored_decisions.json`: three scored review decisions from that model:
  one alias, one cannot-link backed by native negative cues, and one defer.

The integration test converts those pre-scored decisions into native review
decisions at runtime, binds them to a freshly hashed native review artifact, and
runs `canon entity review import --source-review` offline. Its negative feeds a
same-pair alias plus cannot-link batch and asserts the existing validator refuses
with no registry mutation.

## Retirement

This recipe is scaffolding. Retire it when all three native Canon calibration
beads have closed and a central verification pass shows comparable candidate
recall on the BDC surface without an external scorer:

- `bd-3qfq` / S1: gold-driven threshold selection report
- `bd-3el3` / S3: value-level frequency weighting with rare-value floor
- `bd-2qgz` / S6: unsupervised EM weight suggestion

At retirement, keep historical receipts readable, but stop producing new
external-candidate decision files for Canon-owned BDC workflows.
