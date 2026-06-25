# Namekit Unicode Normalization Decision

This note is the ENT-P02.9 contract for deterministic Unicode and
normality/OpenRefine parity semantics. Runtime implementation belongs to
`src/namekit/normalize.rs`; this document fixes the behavior that later code
must satisfy.

## Decision

Canon namekit v0 will use pinned pure-Rust Unicode primitives and explicit
tables only. It must not call Python, host ICU, Java, locale-sensitive OS APIs,
network services, runtime model downloads, or frontier models. Any optional
ICU4X adoption must be statically linked, version-pinned, covered by the same
fixtures, and must not make output depend on host locale or installed data.

The comparison-normalization pipeline is:

1. Accept UTF-8 Rust `str` input and preserve the raw input in provenance.
2. Apply deterministic Unicode decomposition and latinization/folding where the
   pinned Rust implementation has stable behavior.
3. Lowercase with locale-independent Unicode/default case behavior.
4. Remove or fold punctuation and control characters into separator boundaries.
5. Collapse all whitespace runs to one ASCII space and trim edges.
6. Emit normalized comparison text plus ordered `canon_namekit_explain.v0`
   reasons.

OpenRefine-style fingerprint views are separate from plain normalized views.
They start from the normalized text, split on ASCII spaces, sort tokens
byte-stably, deduplicate exact tokens, and join with one ASCII space.

## Boundaries

Namekit Unicode normalization may create support evidence. It must not erase
profile-scoped anti-merge semantics. Legal suffix stripping, protected-token
handling, tenant noise removal, firm role semantics, and profile-specific
drop/preserve policy begin after this generic Unicode layer and must emit their
own reason codes.

The ASCII fast path and Unicode path must produce identical logical reason-code
ordering for equivalent punctuation/control/whitespace transformations. Unicode
folding is emitted only when non-ASCII characters are folded or latinized.

Reason-code order follows `ReasonCode::ALL` in `src/namekit/explain.rs`.
The core normalization reasons used by this contract are:

- `unicode_folded`
- `punctuation_removed`
- `control_removed`
- `whitespace_collapsed`
- `tokens_sorted`
- `tokens_deduped`
- `source_parity_reference`

## Source Mapping

`normality` contributes observable text-cleanup semantics: Unicode input,
diacritic folding, punctuation/control removal, and whitespace collapse. Its
Python and `pyicu` runtime dependencies are rejected.

OpenRefine fingerprinting contributes trim/lowercase/punctuation removal,
western-character folding, token split/sort/deduplicate, and stable joining.
The Java runtime, UI clustering workflow, and locale-sensitive phonetic
fingerprints are rejected.

Rigour name semantics begin after this layer: typed parts, symbols/spans, legal
or stop tagging, and profile-specific preserved/protected tokens must remain
visible as later evidence rather than being erased by Unicode cleanup.

## Fixtures

The authoritative v0 fixture for this decision is
`tests/fixtures/namekit/normalization/unicode_normality.jsonl`.
Every row records:

- `case_id`
- `fixture_id`
- `profile`
- `source`
- `raw`
- `normalized`
- `fingerprint`
- `lossy`
- ordered `reasons`
- conservative `profile_boundary`

`NK_U004` rows cover accented/non-ASCII names. `NK_U005` rows cover reordered
whitespace and punctuation variants that must produce the same normalized view
and reason-code ordering.
