# Extensions

Canon keeps the engine reusable by limiting built-in extension code to typed,
portable package contracts under `src/extensions/`.

## Boundary

- Runtime code in `src/extensions/*.rs` may validate package mechanics, opaque
  IDs, digests, ordering, declared capabilities, and compatibility rules.
- Runtime code in this boundary may not embed industry vocabularies,
  deployment-specific identifier lists, provider-specific clients, or private
  ontology thresholds.
- Domain-owned behavior must enter through typed package data and may live in
  optional out-of-tree deployment assets without changing the open-source build.

## Neutral Examples

Two unrelated synthetic packages should work through the same contract surface
without code changes:

- `pkg.alpha.identity_profile`
- `pkg.beta.identity_profile`

Those packages can declare cluster and link modes, pin opaque dependency
digests, and project override keys without the engine learning anything about a
specific deployment domain.

## Checker Scope

The `domain_neutrality` contract test scans executable extension source after
removing Rust comments, then fails on precise path and rule output when a
forbidden runtime term appears in identifiers, string literals, or branches.
The companion docs check stays narrow: it requires synthetic examples and
rejects only known shipped-domain references, so neutral prose stays allowed.
