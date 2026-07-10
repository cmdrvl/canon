# OCI Artifacts

`canon` uses OCI as transport and discovery for immutable package blobs. OCI does
not replace Canon package semantics. The semantic identity remains the package's
canonical `blake3:` digest and schema contract.

## Media Types

Primary payload media types are derived directly from Canon schema ids:

| Canon schema id | OCI artifactType / primary payload media type |
| --- | --- |
| `canon.registry.package.v1` | `application/vnd.cmdrvl.canon.registry.package.v1+json` |
| `canon.strategy.package.v1` | `application/vnd.cmdrvl.canon.strategy.package.v1+json` |
| `canon.identity.fact.package.v1` | `application/vnd.cmdrvl.canon.identity.fact.package.v1+json` |
| `canon.extension.<name>.vN` | `application/vnd.cmdrvl.canon.extension.<name>.vN+json` |
| `canon.review.attestation.v1` | `application/vnd.cmdrvl.canon.review.attestation.v1+json` |
| `canon.promotion.attestation.v1` | `application/vnd.cmdrvl.canon.promotion.attestation.v1+json` |
| `canon.export.<name>.vN` | `application/vnd.cmdrvl.canon.export.<name>.vN+json` |

Manifest wrapper:

- OCI manifest media type: `application/vnd.oci.image.manifest.v1+json`
- OCI index media type: `application/vnd.oci.image.index.v1+json`
- Canon config descriptor media type: `application/vnd.cmdrvl.canon.oci.config.v1+json`

## Digest Relationship

Every OCI manifest must repeat Canon semantic identity explicitly:

- `io.cmdrvl.canon.package.schema`
- `io.cmdrvl.canon.package.digest`
- `io.cmdrvl.canon.package.id`
- `org.opencontainers.image.version`
- `io.cmdrvl.canon.verify.extension-policy=preserve-but-ignore-for-semantic-verify`

The manifest `artifactType` and the first layer's `mediaType` are the schema-derived
payload media type. The first layer is the only semantic payload layer. Its layer
annotations repeat the same Canon schema id and `blake3:` package digest.

This makes two identities visible and separately checkable:

- Canon semantic digest: `blake3:` over canonical package bytes.
- OCI transport digests: `sha256:` over manifest/config/blob bytes.

Changing OCI transport packaging changes OCI digests. Changing canonical package
bytes changes the Canon semantic digest and therefore invalidates the manifest
binding.

## Layer Ordering

Layer ordering is deterministic:

1. Primary payload layer first.
2. Optional extension layers after the primary payload, normalized by media type
   then digest.

Extension layers may be preserved for transport and caching, but semantic verify
must ignore them. Unknown extension layers are never allowed to satisfy package
schema, package digest, or subject requirements.

## Subject / Referrer Rules

Primary package artifacts do **not** carry a subject:

- registry packages
- strategy packages
- fact packages
- domain-extension packages

Derived artifacts **must** carry an OCI `subject` descriptor that points to an
immutable primary package manifest:

- review attestations
- promotion attestations
- optional export projections

Attestations and projections are separate artifacts, not extra layers inside the
subject package. This avoids circular self-reference and keeps signatures and
review evidence pinned to immutable subjects.

## Local OCI Layout

Local layout uses OCI layout version `1.0.0` and an OCI image index that points
to the Canon manifest descriptor. The manifest descriptor should carry
`org.opencontainers.image.ref.name` for the local semantic version tag or local
inspection name.

OCI layout is a distribution envelope only. Canon verify must still parse the
primary payload bytes and validate the package's own schema and semantic digest.
