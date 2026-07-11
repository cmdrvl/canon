-- table alias_kind_weights on alias_kind_weights
CREATE TABLE alias_kind_weights (
    alias_kind TEXT PRIMARY KEY,
    weight REAL NOT NULL
);

-- table aliases on aliases
CREATE TABLE aliases (
    id INTEGER PRIMARY KEY,
    alias TEXT NOT NULL,
    normalized_key TEXT NOT NULL,
    alias_kind TEXT NOT NULL,
    canonical_id TEXT NOT NULL,
    canonical_iri TEXT NOT NULL,
    canonical_type TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    match_source TEXT NOT NULL,
    source_file TEXT NOT NULL,
    entry_order INTEGER NOT NULL,
    registry_id TEXT NOT NULL,
    registry_version TEXT NOT NULL
);

-- table aliases_fts on aliases_fts
CREATE VIRTUAL TABLE aliases_fts
USING fts5(alias, normalized_key, canonical_id, canonical_iri, content='aliases', content_rowid='id');

-- table capabilities on capabilities
CREATE TABLE capabilities (
    capability TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    description TEXT NOT NULL
);

-- table entities on entities
CREATE TABLE entities (
    canonical_id TEXT NOT NULL,
    canonical_iri TEXT NOT NULL,
    canonical_type TEXT NOT NULL,
    display_name TEXT NOT NULL,
    normalized_display_key TEXT NOT NULL,
    alias_count INTEGER NOT NULL,
    PRIMARY KEY (canonical_type, canonical_id)
);

-- table external_keys on external_keys
CREATE TABLE external_keys (
    canonical_type TEXT NOT NULL,
    canonical_id TEXT NOT NULL,
    key_namespace TEXT NOT NULL,
    key_value TEXT NOT NULL,
    canonical_iri TEXT NOT NULL,
    source TEXT NOT NULL,
    PRIMARY KEY (canonical_type, canonical_id, key_namespace, key_value)
);

-- table field_weights on field_weights
CREATE TABLE field_weights (
    field TEXT PRIMARY KEY,
    weight REAL NOT NULL
);

-- table metadata on metadata
CREATE TABLE metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- table scoring_tiers on scoring_tiers
CREATE TABLE scoring_tiers (
    tier TEXT PRIMARY KEY,
    score INTEGER NOT NULL,
    description TEXT NOT NULL
);

-- index idx_aliases_canonical_iri on aliases
CREATE INDEX idx_aliases_canonical_iri ON aliases(canonical_iri);

-- index idx_aliases_kind_key on aliases
CREATE INDEX idx_aliases_kind_key ON aliases(alias_kind, normalized_key);

-- index idx_aliases_normalized_key on aliases
CREATE INDEX idx_aliases_normalized_key ON aliases(normalized_key);

-- index idx_external_keys_lookup on external_keys
CREATE INDEX idx_external_keys_lookup ON external_keys(key_namespace, key_value);
