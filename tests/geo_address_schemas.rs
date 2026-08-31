use canon::geo::{
    CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION, CANON_GEO_PAD_ADDRESS_SET_VERSION,
    GeoAddressHouseNumber, GeoAddressJurisdiction, GeoAddressParity, GeoAddressParseRequest,
    GeoAddressRangeOperator, GeoAddressStreet, GeoNycBorough, GeoPadAddressMember,
    GeoPadAddressSet, GeoStreetDirection, GeoStreetSuffix, evaluate_pad_membership,
    parse_address_forest,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const ADDRESS_PARSE_REQUEST_FILE: &str = "canon.geo.address_parse_request.v0.schema.json";
const ADDRESS_PARSE_FOREST_FILE: &str = "canon.geo.address_parse_forest.v0.schema.json";
const PAD_ADDRESS_SET_FILE: &str = "canon.geo.pad_address_set.v0.schema.json";
const PAD_MEMBERSHIP_FILE: &str = "canon.geo.pad_membership.v0.schema.json";

const ADDRESS_PARSE_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.address_parse_request.v0.schema.json");
const ADDRESS_PARSE_FOREST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.address_parse_forest.v0.schema.json");
const PAD_ADDRESS_SET_SCHEMA: &str =
    include_str!("../schemas/canon.geo.pad_address_set.v0.schema.json");
const PAD_MEMBERSHIP_SCHEMA: &str =
    include_str!("../schemas/canon.geo.pad_membership.v0.schema.json");

fn schema_source(file: &str) -> &'static str {
    match file {
        ADDRESS_PARSE_REQUEST_FILE => ADDRESS_PARSE_REQUEST_SCHEMA,
        ADDRESS_PARSE_FOREST_FILE => ADDRESS_PARSE_FOREST_SCHEMA,
        PAD_ADDRESS_SET_FILE => PAD_ADDRESS_SET_SCHEMA,
        PAD_MEMBERSHIP_FILE => PAD_MEMBERSHIP_SCHEMA,
        _ => panic!("unregistered address schema ref: {file}"),
    }
}

fn parsed_schema(file: &str) -> Value {
    serde_json::from_str(schema_source(file)).expect("schema file must be valid JSON")
}

fn assert_root_shape(file: &str, expected_title: &str, expected_version: &str) {
    let schema = parsed_schema(file);
    assert_eq!(
        schema.get("title").and_then(Value::as_str),
        Some(expected_title),
        "{file}: title mismatch"
    );
    assert_eq!(
        schema
            .pointer("/properties/version/const")
            .and_then(Value::as_str),
        Some(expected_version),
        "{file}: version const mismatch"
    );
    assert_eq!(
        schema.get("additionalProperties").and_then(Value::as_bool),
        Some(false),
        "{file}: public root must be strict"
    );
    assert!(
        schema
            .get("required")
            .and_then(Value::as_array)
            .is_some_and(|required| required.iter().any(|field| field == "version")),
        "{file}: public root must require version"
    );
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

fn resolve_ref(root_file: &str, reference: &str) {
    let (file, pointer) = if let Some(local) = reference.strip_prefix('#') {
        (root_file, local)
    } else {
        reference
            .split_once('#')
            .unwrap_or_else(|| panic!("external ref lacks fragment: {reference}"))
    };
    let schema = parsed_schema(file);
    schema
        .pointer(pointer)
        .unwrap_or_else(|| panic!("{root_file}: ref {reference} does not resolve"));
}

fn request(input: &str, borough: GeoNycBorough) -> GeoAddressParseRequest {
    GeoAddressParseRequest {
        version: CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION.to_string(),
        input: input.to_string(),
        jurisdiction: Some(GeoAddressJurisdiction::nyc_borough(borough)),
    }
}

fn pad_set(members: Vec<GeoPadAddressMember>) -> GeoPadAddressSet {
    GeoPadAddressSet {
        version: CANON_GEO_PAD_ADDRESS_SET_VERSION.to_string(),
        jurisdiction: GeoAddressJurisdiction::nyc_borough(GeoNycBorough::Manhattan),
        members,
    }
}

fn ordinal_street(
    direction: Option<GeoStreetDirection>,
    value: u16,
    suffix: GeoStreetSuffix,
) -> GeoAddressStreet {
    GeoAddressStreet::ordinal(direction, value, Some(suffix)).expect("street fixture is valid")
}

fn member(
    member_id: &str,
    lot_id: &str,
    house: GeoAddressHouseNumber,
    street: GeoAddressStreet,
) -> GeoPadAddressMember {
    GeoPadAddressMember::new(member_id, lot_id, house, street)
}

fn sample_pad_set() -> GeoPadAddressSet {
    let west_74th = ordinal_street(Some(GeoStreetDirection::West), 74, GeoStreetSuffix::Street);
    let first_ave = ordinal_street(None, 1, GeoStreetSuffix::Avenue);
    pad_set(vec![
        member(
            "pad:w74:241-249",
            "mn:w74:lot",
            GeoAddressHouseNumber::range(
                241,
                249,
                GeoAddressParity::Odd,
                GeoAddressRangeOperator::Slash,
                vec![241, 249],
            )
            .expect("range fixture is valid"),
            west_74th,
        ),
        member(
            "pad:first:199",
            "mn:first:199",
            GeoAddressHouseNumber::discrete(199),
            first_ave,
        ),
    ])
}

fn assert_round_trip<T>(value: &T)
where
    T: Clone + Eq + std::fmt::Debug + serde::Serialize + DeserializeOwned,
{
    let encoded = serde_json::to_value(value).expect("artifact serializes");
    let decoded: T = serde_json::from_value(encoded).expect("artifact deserializes");
    assert_eq!(&decoded, value);
}

fn assert_unknown_field_rejected<T>(mut value: Value, field: &str)
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
fn public_address_schema_roots_are_strict_and_versioned() {
    assert_root_shape(
        ADDRESS_PARSE_REQUEST_FILE,
        "canon.geo.address_parse_request.v0",
        "canon_geo_address_parse_request.v0",
    );
    assert_root_shape(
        ADDRESS_PARSE_FOREST_FILE,
        "canon.geo.address_parse_forest.v0",
        "canon_geo_address_parse_forest.v0",
    );
    assert_root_shape(
        PAD_ADDRESS_SET_FILE,
        "canon.geo.pad_address_set.v0",
        "canon_geo_pad_address_set.v0",
    );
    assert_root_shape(
        PAD_MEMBERSHIP_FILE,
        "canon.geo.pad_membership.v0",
        "canon_geo_pad_membership.v0",
    );
}

#[test]
fn every_address_schema_ref_resolves_through_the_focused_registry() {
    let files = [
        ADDRESS_PARSE_REQUEST_FILE,
        ADDRESS_PARSE_FOREST_FILE,
        PAD_ADDRESS_SET_FILE,
        PAD_MEMBERSHIP_FILE,
    ];
    let mut external_refs = BTreeSet::new();
    for file in files {
        let schema = parsed_schema(file);
        let mut refs = BTreeSet::new();
        collect_refs(&schema, &mut refs);
        for reference in refs {
            resolve_ref(file, &reference);
            if !reference.starts_with('#') {
                external_refs.insert(reference);
            }
        }
    }
    assert_eq!(
        external_refs,
        BTreeSet::from([
            "canon.geo.address_parse_forest.v0.schema.json#/$defs/address_candidate".to_string(),
            "canon.geo.address_parse_forest.v0.schema.json#/$defs/grammar_ref".to_string(),
            "canon.geo.address_parse_forest.v0.schema.json#/$defs/house_number".to_string(),
            "canon.geo.address_parse_forest.v0.schema.json#/$defs/street".to_string(),
            "canon.geo.address_parse_request.v0.schema.json#/$defs/address_jurisdiction"
                .to_string(),
        ])
    );
}

#[test]
fn address_artifacts_round_trip_through_strict_serde_contracts() {
    let request = request("241/249 West 74th Street", GeoNycBorough::Manhattan);
    let forest = parse_address_forest(&request).expect("request must parse");
    let pad = sample_pad_set();
    let membership = evaluate_pad_membership(&forest, &pad).expect("membership evaluates");

    assert_round_trip(&request);
    assert_round_trip(&forest);
    assert_round_trip(&pad);
    assert_round_trip(&membership);

    assert_unknown_field_rejected::<GeoAddressParseRequest>(
        serde_json::to_value(&request).expect("request serializes"),
        "confidence",
    );
    assert_unknown_field_rejected::<GeoPadAddressSet>(
        serde_json::to_value(&pad).expect("PAD set serializes"),
        "confidence",
    );
}

#[test]
fn schema_declares_range_and_parity_rejection_shapes_without_semantic_validation() {
    let forest_schema = parsed_schema(ADDRESS_PARSE_FOREST_FILE);
    assert_eq!(
        forest_schema.pointer("/$defs/parity/enum"),
        Some(&json!(["any", "even", "odd"]))
    );
    assert_eq!(
        forest_schema.pointer("/$defs/range_slash/allOf/1/properties/operator/const"),
        Some(&json!("slash"))
    );
    assert_eq!(
        forest_schema.pointer("/$defs/range_slash/allOf/1/properties/asserted_numbers/minItems"),
        Some(&json!(2))
    );
    assert_eq!(
        forest_schema.pointer("/$defs/range_slash/allOf/1/properties/asserted_numbers/maxItems"),
        Some(&json!(2))
    );
    assert_eq!(
        forest_schema
            .pointer("/$defs/range_dash_list/allOf/1/properties/asserted_numbers/minItems"),
        Some(&json!(3))
    );
}

#[test]
fn membership_schema_reuses_forest_and_pad_contract_shapes() {
    let pad_schema = parsed_schema(PAD_ADDRESS_SET_FILE);
    assert_eq!(
        pad_schema.pointer("/properties/jurisdiction/$ref"),
        Some(&json!(
            "canon.geo.address_parse_request.v0.schema.json#/$defs/address_jurisdiction"
        ))
    );
    assert_eq!(
        pad_schema.pointer("/$defs/pad_address_member/properties/house/$ref"),
        Some(&json!(
            "canon.geo.address_parse_forest.v0.schema.json#/$defs/house_number"
        ))
    );

    let membership_schema = parsed_schema(PAD_MEMBERSHIP_FILE);
    assert_eq!(
        membership_schema.pointer("/properties/grammar/$ref"),
        Some(&json!(
            "canon.geo.address_parse_forest.v0.schema.json#/$defs/grammar_ref"
        ))
    );
    assert_eq!(
        membership_schema.pointer("/$defs/candidate_evaluation/properties/candidate/$ref"),
        Some(&json!(
            "canon.geo.address_parse_forest.v0.schema.json#/$defs/address_candidate"
        ))
    );
}
