#![forbid(unsafe_code)]

#[path = "../src/registry/export_projection.rs"]
mod export_projection;

use export_projection::{
    EXPORT_PROJECTION_VERSION, ProjectionColumn, ProjectionColumnType, ProjectionErrorCode,
    ProjectionExpression, ProjectionPackage, ProjectionRowKind, ProjectionSearchField,
    ProjectionSearchMode, ProjectionSourceRecord, ProjectionTable,
    export_projection_schema_version, plan_projection_exports, write_dbt_projection_seeds,
    write_sqlite_projection_index,
};
use rusqlite::Connection;
use serde_json::Value;
use std::{collections::BTreeMap, fs};
use tempfile::TempDir;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.export.projection.v1.schema.json");

#[test]
fn projection_schema_declares_generic_safe_backend_contract() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], EXPORT_PROJECTION_VERSION);
    assert_eq!(
        schema["properties"]["version"]["const"],
        export_projection_schema_version()
    );
    assert_eq!(
        schema["x-canon-contract"]["relations_and_assignments_do_not_imply_identity"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["raw_sql_expressions"],
        "rejected by compiler; expression language is closed and deterministic"
    );
}

#[test]
fn two_unknown_extension_packages_export_to_dbt_and_sqlite_with_matching_rows() {
    let plan = plan_projection_exports(vec![identifier_package(), relation_assignment_package()])
        .expect("projection plan");
    assert_eq!(plan.tables.len(), 3);
    assert_eq!(
        plan.tables
            .iter()
            .map(|table| table.table_name.as_str())
            .collect::<Vec<_>>(),
        vec!["ext_assignments", "ext_identifiers", "ext_related_context"]
    );

    let temp = TempDir::new().unwrap();
    let seed_dir = temp.path().join("seeds");
    let db_path = temp.path().join("projection.sqlite");
    let first_seed_paths = write_dbt_projection_seeds(&plan, &seed_dir).expect("dbt seeds");
    write_sqlite_projection_index(&plan, &db_path).expect("sqlite projection");

    let identifier_seed = fs::read_to_string(seed_dir.join("ext_identifiers.csv")).unwrap();
    assert!(identifier_seed.contains(
        "canonical_id,namespace_id,identifier_value,source_ref,registry_snapshot_digest"
    ));
    assert!(identifier_seed.contains("CANON-001,neutral:id,NEU-0001,src:a,"));
    assert!(identifier_seed.contains("[REDACTED]9999"));

    let conn = Connection::open(db_path).unwrap();
    let canonical_ids = query_column(
        &conn,
        "select canonical_id from ext_identifiers order by canonical_id",
    );
    assert_eq!(canonical_ids, vec!["CANON-001", "CANON-002"]);
    let relations = query_column(
        &conn,
        "select relation_id || ':' || left_canonical_id || ':' || right_canonical_id from ext_related_context order by relation_id",
    );
    assert_eq!(relations, vec!["REL-001:CANON-001:CANON-002"]);
    let assignments = query_column(
        &conn,
        "select assignment_id || ':' || subject_canonical_id || ':' || role_id from ext_assignments order by assignment_id",
    );
    assert_eq!(assignments, vec!["ASN-001:CANON-001:neutral:role"]);
    let search_fields = query_column(
        &conn,
        "select table_name || ':' || field_name || ':' || column_name from projection_search_fields order by table_name, field_name",
    );
    assert_eq!(
        search_fields,
        vec![
            "ext_assignments:role:role_id",
            "ext_identifiers:identifier:identifier_value",
            "ext_related_context:relation:relation_type_id",
        ]
    );

    let second_plan =
        plan_projection_exports(vec![relation_assignment_package(), identifier_package()])
            .expect("projection plan is order independent");
    assert_eq!(plan, second_plan);
    let second_seed_dir = temp.path().join("seeds_again");
    let second_seed_paths =
        write_dbt_projection_seeds(&second_plan, &second_seed_dir).expect("dbt seeds again");
    assert_eq!(
        first_seed_paths
            .iter()
            .map(|path| path.rsplit('/').next().unwrap().to_string())
            .collect::<Vec<_>>(),
        second_seed_paths
            .iter()
            .map(|path| path.rsplit('/').next().unwrap().to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        fs::read(seed_dir.join("ext_identifiers.csv")).unwrap(),
        fs::read(second_seed_dir.join("ext_identifiers.csv")).unwrap()
    );
}

#[test]
fn empty_matching_dataset_still_exports_declared_tables() {
    let mut package = identifier_package();
    package.records.clear();
    let plan = plan_projection_exports(vec![package]).expect("empty projection plan");
    assert_eq!(plan.tables[0].rows.len(), 0);

    let temp = TempDir::new().unwrap();
    write_dbt_projection_seeds(&plan, temp.path()).unwrap();
    let seed = fs::read_to_string(temp.path().join("ext_identifiers.csv")).unwrap();
    assert_eq!(
        seed.lines().next().unwrap(),
        "canonical_id,namespace_id,identifier_value,source_ref,registry_snapshot_digest,masked_secret"
    );
}

#[test]
fn unsafe_sql_collisions_nondeterminism_and_bad_boundaries_refuse_at_plan_time() {
    let mut reserved = identifier_package();
    reserved.tables[0].table_name = "entities".to_string();
    let error = plan_projection_exports(vec![reserved]).expect_err("reserved table refuses");
    assert_eq!(error.code, ProjectionErrorCode::ReservedTableCollision);

    let mut injection = identifier_package();
    injection.tables[0].table_name = "drop table aliases".to_string();
    let error = plan_projection_exports(vec![injection]).expect_err("unsafe table refuses");
    assert_eq!(error.code, ProjectionErrorCode::UnsafeIdentifier);

    let mut raw_sql = identifier_package();
    raw_sql.tables[0].columns[0].expression = ProjectionExpression::UnsafeSql {
        sql: "select canonical_id from aliases".to_string(),
    };
    let error = plan_projection_exports(vec![raw_sql]).expect_err("raw SQL refuses");
    assert_eq!(error.code, ProjectionErrorCode::UnsafeExpression);

    let mut nondeterministic = identifier_package();
    nondeterministic.tables[0].columns[0].expression = ProjectionExpression::Now;
    let error = plan_projection_exports(vec![nondeterministic]).expect_err("now refuses");
    assert_eq!(error.code, ProjectionErrorCode::NondeterministicExpression);

    let mut bad_boundary = identifier_package();
    bad_boundary.records[0].relation_id = Some("REL-BOGUS".to_string());
    let error = plan_projection_exports(vec![bad_boundary]).expect_err("boundary refuses");
    assert_eq!(error.code, ProjectionErrorCode::BoundaryViolation);

    let mut wrong_snapshot = relation_assignment_package();
    wrong_snapshot.registry_snapshot_digest = digest("different-snapshot");
    let error =
        plan_projection_exports(vec![identifier_package(), wrong_snapshot]).expect_err("snapshot");
    assert_eq!(error.code, ProjectionErrorCode::IncompatibleSnapshot);
}

fn identifier_package() -> ProjectionPackage {
    ProjectionPackage {
        version: EXPORT_PROJECTION_VERSION.to_string(),
        package_id: "neutral.identifier.package".to_string(),
        package_version: "1.0.0".to_string(),
        package_digest: digest("identifier-package"),
        registry_snapshot_digest: digest("registry-snapshot"),
        records: vec![
            identifier_record("id-a", "CANON-001", "NEU-0001", "src:a", "1234569999"),
            identifier_record("id-b", "CANON-002", "NEU-0002", "src:b", "5555511111"),
        ],
        tables: vec![ProjectionTable {
            table_name: "ext_identifiers".to_string(),
            row_kind: ProjectionRowKind::Identifier,
            primary_key: vec![
                "canonical_id".to_string(),
                "namespace_id".to_string(),
                "identifier_value".to_string(),
            ],
            search_fields: vec![ProjectionSearchField {
                field_name: "identifier".to_string(),
                column: "identifier_value".to_string(),
                mode: ProjectionSearchMode::Exact,
            }],
            columns: vec![
                column(
                    "canonical_id",
                    ProjectionColumnType::CanonicalId,
                    ProjectionExpression::CanonicalId,
                ),
                field_column("namespace_id", "namespace_id"),
                field_column("identifier_value", "identifier_value"),
                field_column("source_ref", "source_ref"),
                column(
                    "registry_snapshot_digest",
                    ProjectionColumnType::Digest,
                    ProjectionExpression::RegistrySnapshotDigest,
                ),
                column(
                    "masked_secret",
                    ProjectionColumnType::Text,
                    ProjectionExpression::RedactedField {
                        name: "restricted_value".to_string(),
                        keep_last: 4,
                    },
                ),
            ],
        }],
    }
}

fn relation_assignment_package() -> ProjectionPackage {
    ProjectionPackage {
        version: EXPORT_PROJECTION_VERSION.to_string(),
        package_id: "neutral.relationship.package".to_string(),
        package_version: "1.0.0".to_string(),
        package_digest: digest("relationship-package"),
        registry_snapshot_digest: digest("registry-snapshot"),
        records: vec![relation_record(), assignment_record()],
        tables: vec![
            ProjectionTable {
                table_name: "ext_related_context".to_string(),
                row_kind: ProjectionRowKind::Relation,
                primary_key: vec!["relation_id".to_string()],
                search_fields: vec![ProjectionSearchField {
                    field_name: "relation".to_string(),
                    column: "relation_type_id".to_string(),
                    mode: ProjectionSearchMode::Prefix,
                }],
                columns: vec![
                    column(
                        "relation_id",
                        ProjectionColumnType::Text,
                        ProjectionExpression::RelationId,
                    ),
                    field_column("left_canonical_id", "left_canonical_id"),
                    field_column("right_canonical_id", "right_canonical_id"),
                    field_column("relation_type_id", "relation_type_id"),
                    field_column("source_ref", "source_ref"),
                ],
            },
            ProjectionTable {
                table_name: "ext_assignments".to_string(),
                row_kind: ProjectionRowKind::Assignment,
                primary_key: vec!["assignment_id".to_string()],
                search_fields: vec![ProjectionSearchField {
                    field_name: "role".to_string(),
                    column: "role_id".to_string(),
                    mode: ProjectionSearchMode::Exact,
                }],
                columns: vec![
                    column(
                        "assignment_id",
                        ProjectionColumnType::Text,
                        ProjectionExpression::AssignmentId,
                    ),
                    field_column("subject_canonical_id", "subject_canonical_id"),
                    field_column("role_id", "role_id"),
                    field_column("assignee_disclosed_value", "assignee_disclosed_value"),
                ],
            },
        ],
    }
}

fn identifier_record(
    record_id: &str,
    canonical_id: &str,
    identifier_value: &str,
    source_ref: &str,
    restricted_value: &str,
) -> ProjectionSourceRecord {
    record(
        record_id,
        ProjectionRowKind::Identifier,
        Some(canonical_id),
        None,
        None,
        [
            ("namespace_id", "neutral:id"),
            ("identifier_value", identifier_value),
            ("source_ref", source_ref),
            ("restricted_value", restricted_value),
        ],
    )
}

fn relation_record() -> ProjectionSourceRecord {
    record(
        "rel-a",
        ProjectionRowKind::Relation,
        None,
        Some("REL-001"),
        None,
        [
            ("left_canonical_id", "CANON-001"),
            ("right_canonical_id", "CANON-002"),
            ("relation_type_id", "neutral:related"),
            ("source_ref", "src:relation"),
        ],
    )
}

fn assignment_record() -> ProjectionSourceRecord {
    record(
        "asn-a",
        ProjectionRowKind::Assignment,
        None,
        None,
        Some("ASN-001"),
        [
            ("subject_canonical_id", "CANON-001"),
            ("role_id", "neutral:role"),
            ("assignee_disclosed_value", "example role holder"),
        ],
    )
}

fn record<const N: usize>(
    record_id: &str,
    row_kind: ProjectionRowKind,
    canonical_id: Option<&str>,
    relation_id: Option<&str>,
    assignment_id: Option<&str>,
    fields: [(&str, &str); N],
) -> ProjectionSourceRecord {
    ProjectionSourceRecord {
        record_id: record_id.to_string(),
        row_kind,
        canonical_id: canonical_id.map(str::to_string),
        relation_id: relation_id.map(str::to_string),
        assignment_id: assignment_id.map(str::to_string),
        provenance_ref: None,
        fields: fields
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect::<BTreeMap<_, _>>(),
    }
}

fn field_column(name: &str, field: &str) -> ProjectionColumn {
    column(
        name,
        ProjectionColumnType::Text,
        ProjectionExpression::Field {
            name: field.to_string(),
        },
    )
}

fn column(
    name: &str,
    column_type: ProjectionColumnType,
    expression: ProjectionExpression,
) -> ProjectionColumn {
    ProjectionColumn {
        name: name.to_string(),
        column_type,
        expression,
    }
}

fn query_column(conn: &Connection, sql: &str) -> Vec<String> {
    let mut statement = conn.prepare(sql).unwrap();
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap();
    rows.map(|row| row.unwrap()).collect::<Vec<_>>()
}

fn digest(seed: &str) -> String {
    format!("blake3:{}", blake3::hash(seed.as_bytes()).to_hex())
}
