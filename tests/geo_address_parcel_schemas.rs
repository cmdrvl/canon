#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_ADDRESS_PARCEL_BRIDGE_REQUEST_VERSION,
    CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION, CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION,
    CANON_GEO_PAD_ADDRESS_SET_VERSION, GeoAddressHouseNumber, GeoAddressJurisdiction,
    GeoAddressParcelBridge, GeoAddressParcelBridgeRequest, GeoAddressParcelEvidenceBundle,
    GeoAddressParcelEvidenceRequest, GeoAddressParseRequest, GeoAddressStreet, GeoAsOf,
    GeoEvidenceRecordRef, GeoNycBorough, GeoPadAddressMember, GeoPadAddressSet,
    GeoPadMemberSourceRecord, GeoRhoObservationKind, GeoStreetDirection, GeoStreetSuffix,
    GeoValidTimeInterval, GeoValueOrigin, bridge_pad_membership_to_parcel_observation,
    build_address_parcel_evidence, evaluate_pad_membership, geo_pad_member_blake3,
    parse_address_forest,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const ADDRESS_PARSE_REQUEST_FILE: &str = "canon.geo.address_parse_request.v0.schema.json";
const ADDRESS_PARSE_FOREST_FILE: &str = "canon.geo.address_parse_forest.v0.schema.json";
const PAD_ADDRESS_SET_FILE: &str = "canon.geo.pad_address_set.v0.schema.json";
const PAD_MEMBERSHIP_FILE: &str = "canon.geo.pad_membership.v0.schema.json";
const BRIDGE_REQUEST_FILE: &str = "canon.geo.address_parcel_bridge_request.v0.schema.json";
const BRIDGE_FILE: &str = "canon.geo.address_parcel_bridge.v0.schema.json";
const EVIDENCE_REQUEST_FILE: &str = "canon.geo.address_parcel_evidence_request.v0.schema.json";
const EVIDENCE_BUNDLE_FILE: &str = "canon.geo.address_parcel_evidence_bundle.v0.schema.json";

const ADDRESS_PARSE_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.address_parse_request.v0.schema.json");
const ADDRESS_PARSE_FOREST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.address_parse_forest.v0.schema.json");
const PAD_ADDRESS_SET_SCHEMA: &str =
    include_str!("../schemas/canon.geo.pad_address_set.v0.schema.json");
const PAD_MEMBERSHIP_SCHEMA: &str =
    include_str!("../schemas/canon.geo.pad_membership.v0.schema.json");
const BRIDGE_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.address_parcel_bridge_request.v0.schema.json");
const BRIDGE_SCHEMA: &str =
    include_str!("../schemas/canon.geo.address_parcel_bridge.v0.schema.json");
const EVIDENCE_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.address_parcel_evidence_request.v0.schema.json");
const EVIDENCE_BUNDLE_SCHEMA: &str =
    include_str!("../schemas/canon.geo.address_parcel_evidence_bundle.v0.schema.json");

fn schema_source(file: &str) -> &'static str {
    match file {
        ADDRESS_PARSE_REQUEST_FILE => ADDRESS_PARSE_REQUEST_SCHEMA,
        ADDRESS_PARSE_FOREST_FILE => ADDRESS_PARSE_FOREST_SCHEMA,
        PAD_ADDRESS_SET_FILE => PAD_ADDRESS_SET_SCHEMA,
        PAD_MEMBERSHIP_FILE => PAD_MEMBERSHIP_SCHEMA,
        BRIDGE_REQUEST_FILE => BRIDGE_REQUEST_SCHEMA,
        BRIDGE_FILE => BRIDGE_SCHEMA,
        EVIDENCE_REQUEST_FILE => EVIDENCE_REQUEST_SCHEMA,
        EVIDENCE_BUNDLE_FILE => EVIDENCE_BUNDLE_SCHEMA,
        _ => panic!("unregistered address parcel schema ref: {file}"),
    }
}

fn parsed_schema(file: &str) -> Value {
    serde_json::from_str(schema_source(file)).expect("schema file must be valid JSON")
}

fn split_ref<'a>(root_file: &'a str, reference: &'a str) -> (&'a str, &'a str) {
    if let Some(local) = reference.strip_prefix('#') {
        return (root_file, local);
    }
    reference.split_once('#').unwrap_or((reference, ""))
}

fn resolve_schema_ref(root_file: &str, reference: &str) -> (String, Value) {
    let (file, pointer) = split_ref(root_file, reference);
    let schema = parsed_schema(file);
    let target = schema
        .pointer(pointer)
        .unwrap_or_else(|| panic!("{root_file}: ref {reference} does not resolve"))
        .clone();
    (file.to_string(), target)
}

fn collect_refs(value: &Value, refs: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                refs.insert(reference.to_string());
            }
            for child in object.values() {
                collect_refs(child, refs);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_refs(item, refs);
            }
        }
        _ => {}
    }
}

fn validate_with_schema(file: &str, value: &Value) -> Vec<String> {
    let schema = parsed_schema(file);
    let mut errors = Vec::new();
    validate_node(file, &schema, value, "$", &mut errors);
    errors
}

fn assert_schema_accepts(file: &str, value: &Value) {
    let errors = validate_with_schema(file, value);
    assert!(
        errors.is_empty(),
        "{file} rejected value:\n{}\nerrors: {errors:#?}",
        serde_json::to_string_pretty(value).expect("fixture pretty-prints")
    );
}

fn assert_schema_rejects(file: &str, value: &Value, reason: &str) {
    assert!(
        !validate_with_schema(file, value).is_empty(),
        "{file} accepted invalid value: {reason}"
    );
}

fn validate_node(
    root_file: &str,
    schema: &Value,
    value: &Value,
    path: &str,
    errors: &mut Vec<String>,
) {
    let Some(schema_object) = schema.as_object() else {
        return;
    };

    if let Some(reference) = schema_object.get("$ref").and_then(Value::as_str) {
        let (target_file, target) = resolve_schema_ref(root_file, reference);
        validate_node(&target_file, &target, value, path, errors);
        return;
    }

    if let Some(all_of) = schema_object.get("allOf").and_then(Value::as_array) {
        for subschema in all_of {
            validate_node(root_file, subschema, value, path, errors);
        }
    }

    if let Some(one_of) = schema_object.get("oneOf").and_then(Value::as_array) {
        let matches = one_of
            .iter()
            .filter(|subschema| {
                let mut branch_errors = Vec::new();
                validate_node(root_file, subschema, value, path, &mut branch_errors);
                branch_errors.is_empty()
            })
            .count();
        if matches != 1 {
            errors.push(format!(
                "{path}: expected exactly one oneOf match, got {matches}"
            ));
        }
        return;
    }

    if let Some(expected) = schema_object.get("const")
        && value != expected
    {
        errors.push(format!("{path}: const mismatch"));
    }

    if let Some(enumerants) = schema_object.get("enum").and_then(Value::as_array)
        && !enumerants.iter().any(|candidate| candidate == value)
    {
        errors.push(format!("{path}: enum mismatch"));
    }

    if let Some(schema_type) = schema_object.get("type").and_then(Value::as_str) {
        validate_type(schema_type, value, path, errors);
    }

    if let Some(minimum) = schema_object.get("minimum").and_then(Value::as_i64)
        && value.as_i64().is_none_or(|actual| actual < minimum)
    {
        errors.push(format!("{path}: value below minimum {minimum}"));
    }

    if let Some(maximum) = schema_object.get("maximum").and_then(Value::as_u64)
        && value.as_u64().is_none_or(|actual| actual > maximum)
    {
        errors.push(format!("{path}: value above maximum {maximum}"));
    }

    if let Some(pattern) = schema_object.get("pattern").and_then(Value::as_str) {
        validate_pattern(pattern, value, path, errors);
    }

    if let Some(min_items) = schema_object.get("minItems").and_then(Value::as_u64)
        && value
            .as_array()
            .is_none_or(|actual| actual.len() < min_items as usize)
    {
        errors.push(format!("{path}: array shorter than minItems {min_items}"));
    }

    if let Some(max_items) = schema_object.get("maxItems").and_then(Value::as_u64)
        && value
            .as_array()
            .is_none_or(|actual| actual.len() > max_items as usize)
    {
        errors.push(format!("{path}: array longer than maxItems {max_items}"));
    }

    if let Some(object) = value.as_object() {
        if let Some(required) = schema_object.get("required").and_then(Value::as_array) {
            for field in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(field) {
                    errors.push(format!("{path}: missing required field {field}"));
                }
            }
        }

        if schema_object
            .get("additionalProperties")
            .and_then(Value::as_bool)
            == Some(false)
        {
            let known = schema_object
                .get("properties")
                .and_then(Value::as_object)
                .map(|properties| properties.keys().cloned().collect::<BTreeSet<_>>())
                .unwrap_or_default();
            for field in object.keys() {
                if !known.contains(field) {
                    errors.push(format!("{path}: unknown field {field}"));
                }
            }
        }

        if let Some(properties) = schema_object.get("properties").and_then(Value::as_object) {
            for (field, field_schema) in properties {
                if let Some(field_value) = object.get(field) {
                    validate_node(
                        root_file,
                        field_schema,
                        field_value,
                        &format!("{path}.{field}"),
                        errors,
                    );
                }
            }
        }
    }

    if let (Some(items_schema), Some(items)) = (schema_object.get("items"), value.as_array()) {
        for (index, item) in items.iter().enumerate() {
            validate_node(
                root_file,
                items_schema,
                item,
                &format!("{path}[{index}]"),
                errors,
            );
        }
    }
}

fn validate_type(schema_type: &str, value: &Value, path: &str, errors: &mut Vec<String>) {
    let valid = match schema_type {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        other => panic!("unsupported schema type in focused validator: {other}"),
    };
    if !valid {
        errors.push(format!("{path}: expected type {schema_type}"));
    }
}

fn validate_pattern(pattern: &str, value: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(text) = value.as_str() else {
        errors.push(format!("{path}: pattern applied to non-string"));
        return;
    };
    let matches = match pattern {
        "^[0-9a-f]{64}$" => {
            text.len() == 64
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }
        other => panic!("unsupported schema pattern in focused validator: {other}"),
    };
    if !matches {
        errors.push(format!("{path}: pattern mismatch {pattern}"));
    }
}

fn parse_request(input: &str) -> GeoAddressParseRequest {
    GeoAddressParseRequest {
        version: CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION.to_string(),
        input: input.to_string(),
        jurisdiction: Some(GeoAddressJurisdiction::nyc_borough(
            GeoNycBorough::Manhattan,
        )),
    }
}

fn ordinal_street(
    direction: Option<GeoStreetDirection>,
    value: u16,
    suffix: GeoStreetSuffix,
) -> GeoAddressStreet {
    GeoAddressStreet::ordinal(direction, value, Some(suffix)).expect("street fixture is valid")
}

fn pad_member(
    member_id: &str,
    lot_id: &str,
    house: GeoAddressHouseNumber,
    street: GeoAddressStreet,
) -> GeoPadAddressMember {
    GeoPadAddressMember::new(member_id, lot_id, house, street)
}

fn address_set() -> GeoPadAddressSet {
    GeoPadAddressSet {
        version: CANON_GEO_PAD_ADDRESS_SET_VERSION.to_string(),
        jurisdiction: GeoAddressJurisdiction::nyc_borough(GeoNycBorough::Manhattan),
        members: vec![
            pad_member(
                "pad:first:199",
                "mn:first:199",
                GeoAddressHouseNumber::discrete(199),
                ordinal_street(None, 1, GeoStreetSuffix::Avenue),
            ),
            pad_member(
                "pad:e12:349",
                "mn:e12:349",
                GeoAddressHouseNumber::discrete(349),
                ordinal_street(Some(GeoStreetDirection::East), 12, GeoStreetSuffix::Street),
            ),
        ],
    }
}

fn source_record(id: &str) -> GeoEvidenceRecordRef {
    GeoEvidenceRecordRef {
        source_record_id: id.to_string(),
        source_vintage: "26B".to_string(),
        record_blake3: blake3::hash(id.as_bytes()).to_hex().to_string(),
    }
}

fn source_binding(member_id: &str) -> GeoPadMemberSourceRecord {
    let set = address_set();
    let member = set
        .members
        .iter()
        .find(|member| member.member_id == member_id)
        .expect("binding fixture member exists");
    GeoPadMemberSourceRecord {
        member_id: member_id.to_string(),
        normalized_member_blake3: geo_pad_member_blake3(member).expect("member hash"),
        source_record: source_record(&format!("src:{member_id}")),
    }
}

fn bridge_request() -> GeoAddressParcelBridgeRequest {
    GeoAddressParcelBridgeRequest {
        version: CANON_GEO_ADDRESS_PARCEL_BRIDGE_REQUEST_VERSION.to_string(),
        observation_id: "obs.address.pad.membership".to_string(),
        contract_id: "rho.address.pad.membership".to_string(),
        query_as_of: Some(GeoAsOf {
            utc_day: "2026-08-31".to_string(),
            semantic_id: "fixture:query_as_of".to_string(),
            unit: "utc_day".to_string(),
            origin: GeoValueOrigin::CallerDeclared,
        }),
        valid_time: Some(GeoValidTimeInterval {
            start_day: 20_696,
            end_day: 20_696,
        }),
        member_source_records: vec![
            source_binding("pad:first:199"),
            source_binding("pad:e12:349"),
        ],
    }
}

fn evidence_request() -> GeoAddressParcelEvidenceRequest {
    GeoAddressParcelEvidenceRequest {
        version: CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION.to_string(),
        parse_request: parse_request("199 First Avenue a/k/a 349 East 12th Street"),
        address_set: address_set(),
        bridge_request: bridge_request(),
    }
}

fn staged_bridge() -> GeoAddressParcelBridge {
    let request = evidence_request();
    let forest = parse_address_forest(&request.parse_request).expect("parse stage");
    let membership = evaluate_pad_membership(&forest, &request.address_set).expect("PAD stage");
    bridge_pad_membership_to_parcel_observation(
        &forest,
        &request.address_set,
        &membership,
        &request.bridge_request,
    )
    .expect("bridge stage")
}

fn evidence_bundle() -> GeoAddressParcelEvidenceBundle {
    build_address_parcel_evidence(&evidence_request()).expect("bundle builds")
}

fn assert_root_shape(file: &str, expected_title: &str, expected_version: &str) {
    let schema = parsed_schema(file);
    assert_eq!(
        schema.get("title").and_then(Value::as_str),
        Some(expected_title)
    );
    assert_eq!(
        schema
            .pointer("/properties/version/const")
            .and_then(Value::as_str),
        Some(expected_version)
    );
    assert_eq!(
        schema.get("additionalProperties").and_then(Value::as_bool),
        Some(false)
    );
    assert!(
        schema
            .get("required")
            .and_then(Value::as_array)
            .is_some_and(|required| required.iter().any(|field| field == "version"))
    );
}

fn assert_unknown_field_rejected_by_serde<T>(mut value: Value, field: &str)
where
    T: DeserializeOwned + std::fmt::Debug,
{
    value
        .as_object_mut()
        .expect("fixture must be an object")
        .insert(field.to_string(), json!("unexpected"));
    serde_json::from_value::<T>(value).expect_err("unknown fields must be rejected by serde");
}

#[test]
fn address_parcel_schema_roots_are_strict_versioned_and_resolvable() {
    assert_root_shape(
        BRIDGE_REQUEST_FILE,
        "canon.geo.address_parcel_bridge_request.v0",
        "canon_geo_address_parcel_bridge_request.v0",
    );
    assert_root_shape(
        BRIDGE_FILE,
        "canon.geo.address_parcel_bridge.v0",
        "canon_geo_address_parcel_bridge.v0",
    );
    assert_root_shape(
        EVIDENCE_REQUEST_FILE,
        "canon.geo.address_parcel_evidence_request.v0",
        "canon_geo_address_parcel_evidence_request.v0",
    );
    assert_root_shape(
        EVIDENCE_BUNDLE_FILE,
        "canon.geo.address_parcel_evidence_bundle.v0",
        "canon_geo_address_parcel_evidence_bundle.v0",
    );

    for file in [
        BRIDGE_REQUEST_FILE,
        BRIDGE_FILE,
        EVIDENCE_REQUEST_FILE,
        EVIDENCE_BUNDLE_FILE,
    ] {
        let schema = parsed_schema(file);
        let mut refs = BTreeSet::new();
        collect_refs(&schema, &mut refs);
        for reference in refs {
            let _ = resolve_schema_ref(file, &reference);
        }
    }
}

#[test]
fn schemas_validate_real_serialized_address_parcel_artifacts() {
    let bridge_request = bridge_request();
    let bridge = staged_bridge();
    let evidence_request = evidence_request();
    let bundle = evidence_bundle();

    assert_schema_accepts(
        BRIDGE_REQUEST_FILE,
        &serde_json::to_value(&bridge_request).expect("bridge request serializes"),
    );
    assert_schema_accepts(
        BRIDGE_FILE,
        &serde_json::to_value(&bridge).expect("bridge serializes"),
    );
    assert_schema_accepts(
        EVIDENCE_REQUEST_FILE,
        &serde_json::to_value(&evidence_request).expect("evidence request serializes"),
    );
    assert_schema_accepts(
        EVIDENCE_BUNDLE_FILE,
        &serde_json::to_value(&bundle).expect("evidence bundle serializes"),
    );

    let observation = bridge
        .observation
        .as_ref()
        .expect("fixture emits an address-to-parcel observation");
    let GeoRhoObservationKind::ExistentialMembership { members } = &observation.observation else {
        panic!("address parcel bridge must emit existential parcel membership");
    };
    assert_eq!(
        members
            .iter()
            .map(|member| (member.level, member.id.as_str()))
            .collect::<Vec<_>>(),
        [
            (canon::geo::GeoEntityLevel::Parcel, "mn:e12:349"),
            (canon::geo::GeoEntityLevel::Parcel, "mn:first:199")
        ]
    );
}

#[test]
fn schemas_reject_unknown_fields_matching_address_serde_contracts() {
    let bridge_request = serde_json::to_value(bridge_request()).expect("request serializes");
    let bridge = serde_json::to_value(staged_bridge()).expect("bridge serializes");
    let evidence_request = serde_json::to_value(evidence_request()).expect("request serializes");
    let bundle = serde_json::to_value(evidence_bundle()).expect("bundle serializes");

    assert_unknown_field_rejected_by_serde::<GeoAddressParcelBridgeRequest>(
        bridge_request.clone(),
        "confidence",
    );
    assert_unknown_field_rejected_by_serde::<GeoAddressParcelBridge>(bridge.clone(), "confidence");
    assert_unknown_field_rejected_by_serde::<GeoAddressParcelEvidenceRequest>(
        evidence_request.clone(),
        "confidence",
    );
    assert_unknown_field_rejected_by_serde::<GeoAddressParcelEvidenceBundle>(
        bundle.clone(),
        "confidence",
    );

    let mut invalid_bridge_request = bridge_request.clone();
    invalid_bridge_request["member_source_records"][0]["confidence"] = json!("unexpected");
    assert_schema_rejects(
        BRIDGE_REQUEST_FILE,
        &invalid_bridge_request,
        "nested source-record binding unknown field",
    );

    let mut invalid_bridge = bridge.clone();
    invalid_bridge["readings"][0]["confidence"] = json!("unexpected");
    assert_schema_rejects(BRIDGE_FILE, &invalid_bridge, "nested reading unknown field");

    let mut invalid_evidence_request = evidence_request.clone();
    invalid_evidence_request["bridge_request"]["member_source_records"][0]["confidence"] =
        json!("unexpected");
    assert_schema_rejects(
        EVIDENCE_REQUEST_FILE,
        &invalid_evidence_request,
        "nested bridge request unknown field",
    );

    let mut invalid_bundle = bundle.clone();
    invalid_bundle["bridge"]["readings"][0]["confidence"] = json!("unexpected");
    assert_schema_rejects(
        EVIDENCE_BUNDLE_FILE,
        &invalid_bundle,
        "nested bridge unknown field",
    );

    let mut invalid_root = bridge_request;
    invalid_root["confidence"] = json!("unexpected");
    assert_schema_rejects(BRIDGE_REQUEST_FILE, &invalid_root, "root unknown field");
}

#[test]
fn schemas_reject_wrong_version_and_tag_shapes() {
    let bridge_request = serde_json::to_value(bridge_request()).expect("request serializes");
    let bridge = serde_json::to_value(staged_bridge()).expect("bridge serializes");
    let evidence_request = serde_json::to_value(evidence_request()).expect("request serializes");
    let bundle = serde_json::to_value(evidence_bundle()).expect("bundle serializes");

    for (file, mut value) in [
        (BRIDGE_REQUEST_FILE, bridge_request.clone()),
        (BRIDGE_FILE, bridge.clone()),
        (EVIDENCE_REQUEST_FILE, evidence_request.clone()),
        (EVIDENCE_BUNDLE_FILE, bundle.clone()),
    ] {
        value["version"] = json!("canon_geo_wrong.v0");
        assert_schema_rejects(file, &value, "wrong root version const");
    }

    for (file, mut value) in [
        (BRIDGE_REQUEST_FILE, bridge_request),
        (BRIDGE_FILE, bridge.clone()),
        (EVIDENCE_REQUEST_FILE, evidence_request.clone()),
        (EVIDENCE_BUNDLE_FILE, bundle.clone()),
    ] {
        value
            .as_object_mut()
            .expect("root fixture is an object")
            .remove("version");
        assert_schema_rejects(file, &value, "missing root version");
    }

    let mut wrong_nested_request = evidence_request;
    wrong_nested_request["parse_request"]["version"] = json!("canon_geo_address_parse_request.v9");
    assert_schema_rejects(
        EVIDENCE_REQUEST_FILE,
        &wrong_nested_request,
        "wrong nested parse request version",
    );

    let mut wrong_nested_bundle = bundle;
    wrong_nested_bundle["bridge"]["version"] = json!("canon_geo_address_parcel_bridge.v9");
    assert_schema_rejects(
        EVIDENCE_BUNDLE_FILE,
        &wrong_nested_bundle,
        "wrong nested bridge version",
    );

    let mut wrong_observation_kind = bridge;
    wrong_observation_kind["observation"]["observation"]["kind"] = json!("same_as");
    assert_schema_rejects(
        BRIDGE_FILE,
        &wrong_observation_kind,
        "wrong tagged rho observation kind",
    );

    let mut non_existential_observation = serde_json::to_value(staged_bridge())
        .expect("bridge serializes for semantic schema negative");
    non_existential_observation["observation"]["observation"] = json!({
        "kind": "exact_sets",
        "level": "parcel",
        "sets": [["mn:first:199"]]
    });
    assert_schema_rejects(
        BRIDGE_FILE,
        &non_existential_observation,
        "address parcel bridge must emit only existential membership",
    );

    let mut non_parcel_candidate =
        serde_json::to_value(staged_bridge()).expect("bridge serializes for level negative");
    non_parcel_candidate["parcel_candidates"][0]["level"] = json!("building");
    assert_schema_rejects(
        BRIDGE_FILE,
        &non_parcel_candidate,
        "address parcel bridge candidates must remain parcel-grain",
    );

    let mut missing_observation =
        serde_json::to_value(staged_bridge()).expect("bridge serializes for status negative");
    missing_observation
        .as_object_mut()
        .expect("bridge object")
        .remove("observation");
    assert_schema_rejects(
        BRIDGE_FILE,
        &missing_observation,
        "evidence_observation status requires an observation",
    );
}
