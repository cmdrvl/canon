# Namekit Legal Form Provenance

Status: ENT-P02.8 implementation contract, created 2026-06-25.

`src/namekit/legal_suffix.rs` ships a small canon-curated seed table rather
than copying a third-party suffix table. The upstream libraries in
`docs/namekit/SOURCE_PORT_MAP.md` are reference sources for behavior:
`cleanco` informs repeated suffix stripping, OpenSanctions rigour/fingerprints
inform legal-form evidence shape, GLEIF ISO 20275 is the preferred future
authority for generated legal-form tables, and OpenRefine informs token
fingerprint semantics.

The seed table is intentionally narrow:

- Every row carries `term`, `normalized_term`, `source`, `source_version`,
  `license`, jurisdiction/type metadata where known, provenance text, and
  profile policy flags.
- `LEGAL_FORM_DATA_DIGEST` is a contract digest for this seed table. A generated
  replacement must use a real content digest and a pinned source snapshot.
- `LEGAL_FORM_LICENSE_REVIEW` records that no external suffix table was copied.
  Future imports from cleanco, rigour/OCCRP, or GLEIF must add explicit license
  and data-terms review before code generation.
- CMBS tenant-label views may strip common legal suffixes such as `LLC`, `Ltd`,
  and `and Co.` while recording lossy reason codes.
- Reg AB firm-identity views preserve regulated tokens such as `bank`,
  `national association`, and `N.A.` so tenant-label semantics do not erase firm
  identity boundaries.

The fixture snapshots in `tests/fixtures/namekit/legal_suffix/` are the
hand-auditable proof for the v0 table. They include repeated stripping,
`Sears, Roebuck and Co.`, and `PNC Bank, National Association` / `PNC Bank N.A.`
profile behavior.
