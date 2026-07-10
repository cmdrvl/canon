# Provider SDK Conformance

External provider packages are tested as black boxes against the frozen-source SDK contract in `src/provider_sdk.rs`. The SDK envelope is generic on purpose: Canon core does not learn a provider's domain vocabulary, but conformance still requires strict separation between typed fact families.

## Required boundary

- Identity facts, relationship facts, status facts, and exception facts must remain separate typed facts with distinct `fact_key` families.
- Every emitted fact must carry provenance through `source_digest`, `source_path`, record ordinal, and field-level locator data.
- Subset builds must declare the subset predicate in the emitted fact payload so downstream rebuilds can explain why a row was included.
- Providers must bind identity, relationship, status, exception, ontology, and identifier-namespace packages explicitly and fail under the compatibility policy with `compatibility_policy` when those bindings drift.
- Hidden network access is always a conformance failure because acquisition stays outside the deterministic offline build.

## Neutral fixture

`tests/provider_identity_conformance.rs` uses invented neutral fixture records only:

- `tests/fixtures/providers/neutral-identity/manifest.json`
- `tests/fixtures/providers/neutral-identity/source.jsonl`
- `tests/fixtures/providers/neutral-identity/expected_facts.json`

The fixture proves that an external package can emit transliterated identity assertions, status changes, parent-child and successor relationships, and exception facts without linking any provider-specific client into Canon core. The expected output is compared as a projected black-box artifact, not by reaching into internal SDK helpers.
