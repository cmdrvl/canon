#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_ADDRESS_PARCEL_BRIDGE_REQUEST_VERSION,
    CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION, CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION,
    CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_PAD_ADDRESS_SET_VERSION,
    DEFAULT_MAX_MATERIALIZED_MODELS, GeoAddressHouseNumber, GeoAddressJurisdiction,
    GeoAddressParcelBridgeRequest, GeoAddressParcelBridgeStatus, GeoAddressParcelDiagnosticCode,
    GeoAddressParcelEvidenceRequest, GeoAddressParseForest, GeoAddressParseRequest,
    GeoAddressPropertyInput, GeoAddressStreet, GeoAsOf, GeoCompositionStatus,
    GeoCompositionUniverse, GeoEntityLevel, GeoEvidenceClaimRole, GeoEvidenceCompilationRequest,
    GeoEvidenceDisposition, GeoEvidenceRecordRef, GeoNycBorough, GeoPadAddressMember,
    GeoPadAddressSet, GeoPadMemberSourceRecord, GeoRhoBasis, GeoRhoContract, GeoRhoObservationKind,
    GeoStreetDirection, GeoStreetSuffix, GeoValidTimeInterval, GeoValueOrigin,
    bridge_pad_membership_to_parcel_observation, build_address_parcel_compilation_request,
    build_address_parcel_evidence, canonical_address_parcel_bridge_bytes,
    canonical_address_parcel_evidence_bundle_bytes, compile_evidence, evaluate_pad_membership,
    geo_pad_member_blake3, parse_address_forest, select_address_property_inputs_for_jurisdiction,
    solve_composition,
};
use std::collections::BTreeSet;

fn request(input: &str, borough: GeoNycBorough) -> GeoAddressParseRequest {
    GeoAddressParseRequest {
        version: CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION.to_string(),
        input: input.to_string(),
        jurisdiction: Some(GeoAddressJurisdiction::nyc_borough(borough)),
    }
}

fn forest(input: &str) -> GeoAddressParseForest {
    parse_address_forest(&request(input, GeoNycBorough::Manhattan))
        .expect("fixture address must parse")
}

fn pad_set(members: Vec<GeoPadAddressMember>) -> GeoPadAddressSet {
    pad_set_for_borough(GeoNycBorough::Manhattan, members)
}

fn pad_set_for_borough(
    borough: GeoNycBorough,
    members: Vec<GeoPadAddressMember>,
) -> GeoPadAddressSet {
    GeoPadAddressSet {
        version: CANON_GEO_PAD_ADDRESS_SET_VERSION.to_string(),
        jurisdiction: GeoAddressJurisdiction::nyc_borough(borough),
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

fn literal_street(name: &[&str], suffix: GeoStreetSuffix) -> GeoAddressStreet {
    GeoAddressStreet::literal(None, name, Some(suffix)).expect("street fixture is valid")
}

fn member(
    member_id: &str,
    lot_id: &str,
    house: GeoAddressHouseNumber,
    street: GeoAddressStreet,
) -> GeoPadAddressMember {
    GeoPadAddressMember::new(member_id, lot_id, house, street)
}

fn source_record(id: &str) -> GeoEvidenceRecordRef {
    GeoEvidenceRecordRef {
        source_record_id: id.to_string(),
        source_vintage: "26B".to_string(),
        record_blake3: blake3::hash(id.as_bytes()).to_hex().to_string(),
    }
}

fn source_binding_for_members(
    member_id: &str,
    members: &[GeoPadAddressMember],
) -> GeoPadMemberSourceRecord {
    let member = members
        .iter()
        .find(|member| member.member_id == member_id)
        .expect("binding fixture member exists");
    GeoPadMemberSourceRecord {
        member_id: member_id.to_string(),
        normalized_member_blake3: geo_pad_member_blake3(member).expect("member hash"),
        source_record: source_record(&format!("src:{member_id}")),
    }
}

fn bridge_request(member_ids: &[&str]) -> GeoAddressParcelBridgeRequest {
    let members = first_and_twelfth_members();
    bridge_request_for_members(member_ids, &members)
}

fn bridge_request_for_members(
    member_ids: &[&str],
    members: &[GeoPadAddressMember],
) -> GeoAddressParcelBridgeRequest {
    GeoAddressParcelBridgeRequest {
        version: CANON_GEO_ADDRESS_PARCEL_BRIDGE_REQUEST_VERSION.to_string(),
        observation_id: "obs.address.pad.membership".to_string(),
        contract_id: "rho.address.pad.membership".to_string(),
        query_as_of: None,
        valid_time: None,
        member_source_records: member_ids
            .iter()
            .map(|member_id| source_binding_for_members(member_id, members))
            .collect(),
    }
}

fn contract() -> GeoRhoContract {
    GeoRhoContract {
        id: "rho.address.pad.membership".to_string(),
        version: "1.0.0".to_string(),
        source_dataset: "SOURCE.NYC_DCP_PAD_ADDRESS_HOT".to_string(),
        source_release: "26B".to_string(),
        source_lineage_ids: vec!["SOURCE.NYC_DCP_PAD_ADDRESS_HOT:26B".to_string()],
        method_id: "address-parse-pad-membership-bridge".to_string(),
        method_version: "1.0.0".to_string(),
        claim_role: GeoEvidenceClaimRole::StableIdentityAnchor,
        basis: GeoRhoBasis::LogicalRelaxation {
            invariant_id: "pad-member-implies-parcel-address-membership".to_string(),
        },
    }
}

fn first_and_twelfth_members() -> Vec<GeoPadAddressMember> {
    let first_ave = ordinal_street(None, 1, GeoStreetSuffix::Avenue);
    let east_12th = ordinal_street(Some(GeoStreetDirection::East), 12, GeoStreetSuffix::Street);
    vec![
        member(
            "pad:first:199",
            "mn:first:199",
            GeoAddressHouseNumber::discrete(199),
            first_ave,
        ),
        member(
            "pad:e12:349",
            "mn:e12:349",
            GeoAddressHouseNumber::discrete(349),
            east_12th,
        ),
    ]
}

fn string_set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn bridged_observation_parcel_ids(
    input: &str,
    borough: GeoNycBorough,
    members: Vec<GeoPadAddressMember>,
) -> BTreeSet<String> {
    let member_ids = members
        .iter()
        .map(|member| member.member_id.as_str())
        .collect::<Vec<_>>();
    let request = GeoAddressParcelEvidenceRequest {
        version: CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION.to_string(),
        parse_request: request(input, borough),
        address_set: pad_set_for_borough(borough, members.clone()),
        bridge_request: bridge_request_for_members(&member_ids, &members),
        evidence_request: None,
    };
    let bundle = build_address_parcel_evidence(&request).expect("address bundle should build");
    assert_eq!(
        bundle.bridge.status,
        GeoAddressParcelBridgeStatus::EvidenceObservation
    );
    let observation = bundle
        .bridge
        .observation
        .expect("supported address bridge emits an observation");
    let GeoRhoObservationKind::ExistentialMembership { members } = observation.observation else {
        panic!("address bridge must emit existential membership");
    };
    members.into_iter().map(|member| member.id).collect()
}

fn evidence_template() -> GeoEvidenceCompilationRequest {
    GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
        universe: GeoCompositionUniverse {
            parcels: vec![
                "mn:other".to_string(),
                "mn:first:199".to_string(),
                "mn:e12:349".to_string(),
            ],
            buildings: Vec::new(),
        },
        contracts: vec![contract()],
        observations: Vec::new(),
        max_assignments: 16,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

fn evidence_request(observation: GeoRhoObservationKind) -> GeoEvidenceCompilationRequest {
    let mut request = evidence_template();
    request.observations = vec![canon::geo::GeoRhoObservation {
        id: "obs.address.pad.membership".to_string(),
        contract_id: "rho.address.pad.membership".to_string(),
        source_records: vec![
            source_record("src:pad:first:199"),
            source_record("src:pad:e12:349"),
        ],
        valid_time: None,
        observation,
    }];
    request
}

#[test]
fn multiple_surviving_readings_union_into_one_existential_observation() {
    let forest = forest("199 First Avenue a/k/a 349 East 12th Street");
    let pad = pad_set(first_and_twelfth_members());
    let membership = evaluate_pad_membership(&forest, &pad).expect("membership evaluates");
    let bridge = bridge_pad_membership_to_parcel_observation(
        &forest,
        &pad,
        &membership,
        &bridge_request(&["pad:first:199", "pad:e12:349"]),
    )
    .expect("bridge should emit evidence");

    assert_eq!(
        bridge.status,
        GeoAddressParcelBridgeStatus::EvidenceObservation
    );
    assert_eq!(
        bridge
            .parcel_candidates
            .iter()
            .map(|member| (member.level, member.id.as_str()))
            .collect::<Vec<_>>(),
        [
            (GeoEntityLevel::Parcel, "mn:e12:349"),
            (GeoEntityLevel::Parcel, "mn:first:199")
        ]
    );

    let observation = bridge
        .observation
        .clone()
        .expect("supported readings must emit an observation");
    let GeoRhoObservationKind::ExistentialMembership { members } = &observation.observation else {
        panic!("address bridge must emit existential membership");
    };
    let bridge_members = members.clone();
    assert_eq!(
        members
            .iter()
            .map(|member| member.id.as_str())
            .collect::<Vec<_>>(),
        ["mn:e12:349", "mn:first:199"]
    );

    let compiled = compile_evidence(&GeoEvidenceCompilationRequest {
        observations: vec![observation],
        ..evidence_request(GeoRhoObservationKind::ExistentialMembership {
            members: bridge_members,
        })
    })
    .expect("bridge observation must compile through the production evidence compiler");
    assert_eq!(
        compiled.admissions[0].disposition,
        GeoEvidenceDisposition::HardConstraint
    );
    let solved = solve_composition(&compiled.composition_request)
        .expect("compiled address evidence should solve");
    assert_eq!(solved.status, GeoCompositionStatus::Ambiguous);
    assert_eq!(solved.summary.residual_model_count, 6);
    assert!(
        solved.hard_forced.parcels.is_empty(),
        "the union is an AnyOf, not a guessed singleton"
    );
}

#[test]
fn bd_2rc0_524efa30_bridge_emits_truth_42_street_members_not_neighbor() {
    let street = ordinal_street(None, 42, GeoStreetSuffix::Street);
    let parcel_ids = bridged_observation_parcel_ids(
        "18-27A; 18-29; 18-31; 18-33 42ND STREET",
        GeoNycBorough::Queens,
        vec![
            member(
                "4007910022",
                "4007910022",
                GeoAddressHouseNumber::queens_hyphenated_literal("18-33"),
                street.clone(),
            ),
            member(
                "4007910023",
                "4007910023",
                GeoAddressHouseNumber::queens_hyphenated_literal("18-29"),
                street.clone(),
            ),
            member(
                "4007910122",
                "4007910122",
                GeoAddressHouseNumber::queens_hyphenated_literal("18-31"),
                street.clone(),
            ),
            member(
                "4007910123",
                "4007910123",
                GeoAddressHouseNumber::queens_hyphenated_literal("18-27A"),
                street.clone(),
            ),
            member(
                "4008020060",
                "4008020060",
                GeoAddressHouseNumber::queens_hyphenated_literal("18-22"),
                street,
            ),
        ],
    );

    assert_eq!(
        parcel_ids,
        string_set(&["4007910022", "4007910023", "4007910122", "4007910123"])
    );
    assert!(
        !parcel_ids.contains("4008020060"),
        "the hard PAD membership observation must not emit the neighboring 18-22 42 Street lot"
    );
}

#[test]
fn bd_2rc0_a6dd2e31_bridge_keeps_house_numbers_on_declared_street_segments() {
    let east_60th = ordinal_street(Some(GeoStreetDirection::East), 60, GeoStreetSuffix::Street);
    let first_ave = ordinal_street(None, 1, GeoStreetSuffix::Avenue);
    let parcel_ids = bridged_observation_parcel_ids(
        "345, 347, 349 EAST 60TH STREET AND 1097, 1099, 1101, 1103 1ST AVENUE",
        GeoNycBorough::Manhattan,
        vec![
            member(
                "1014350020",
                "1014350020",
                GeoAddressHouseNumber::discrete(345),
                east_60th.clone(),
            ),
            member(
                "1014350021",
                "1014350021",
                GeoAddressHouseNumber::discrete(347),
                east_60th.clone(),
            ),
            member(
                "1014350022",
                "1014350022",
                GeoAddressHouseNumber::discrete(349),
                east_60th,
            ),
            member(
                "1014350023",
                "1014350023",
                GeoAddressHouseNumber::discrete(1097),
                first_ave.clone(),
            ),
            member(
                "1014350024",
                "1014350024",
                GeoAddressHouseNumber::discrete(1099),
                first_ave.clone(),
            ),
            member(
                "1014350025",
                "1014350025",
                GeoAddressHouseNumber::discrete(1101),
                first_ave.clone(),
            ),
            member(
                "1014350026",
                "1014350026",
                GeoAddressHouseNumber::discrete(1103),
                first_ave.clone(),
            ),
            member(
                "1009260034",
                "1009260034",
                GeoAddressHouseNumber::discrete(345),
                first_ave,
            ),
        ],
    );

    assert_eq!(
        parcel_ids,
        string_set(&[
            "1014350020",
            "1014350021",
            "1014350022",
            "1014350023",
            "1014350024",
            "1014350025",
            "1014350026"
        ])
    );
    assert!(
        !parcel_ids.contains("1009260034"),
        "the hard PAD membership observation must not emit the 345 1 Avenue chimera"
    );
}

#[test]
fn bd_2rc0_69dbf5da_multi_property_selection_uses_subject_jurisdiction_before_bridge() {
    let property_rows = vec![
        GeoAddressPropertyInput {
            property_id: "property:manhattan:forsyth".to_string(),
            input: "100-102 FORSYTH STREET".to_string(),
            jurisdiction: GeoAddressJurisdiction::nyc_borough(GeoNycBorough::Manhattan),
        },
        GeoAddressPropertyInput {
            property_id: "property:kings:the-vue".to_string(),
            input: "1809 EMMONS AVENUE".to_string(),
            jurisdiction: GeoAddressJurisdiction::nyc_borough(GeoNycBorough::Brooklyn),
        },
    ];
    let target_jurisdiction = GeoAddressJurisdiction::nyc_borough(GeoNycBorough::Brooklyn);
    let selected =
        select_address_property_inputs_for_jurisdiction(&property_rows, &target_jurisdiction)
            .expect("property rows should select by declared subject jurisdiction");

    assert_eq!(
        selected
            .iter()
            .map(|row| row.property_id.as_str())
            .collect::<Vec<_>>(),
        ["property:kings:the-vue"]
    );
    assert!(
        selected.iter().all(|row| !row.input.contains("FORSYTH")),
        "a Manhattan collateral property must not feed the Kings subject bridge"
    );

    let parcel_ids = bridged_observation_parcel_ids(
        &selected[0].input,
        GeoNycBorough::Brooklyn,
        vec![member(
            "3087731001",
            "3087731001",
            GeoAddressHouseNumber::discrete(1809),
            literal_street(&["emmons"], GeoStreetSuffix::Avenue),
        )],
    );
    assert_eq!(parcel_ids, string_set(&["3087731001"]));
}

#[test]
fn address_bundle_emits_compiler_ready_request_when_envelope_is_supplied() {
    let request = GeoAddressParcelEvidenceRequest {
        version: CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION.to_string(),
        parse_request: request(
            "199 First Avenue a/k/a 349 East 12th Street",
            GeoNycBorough::Manhattan,
        ),
        address_set: pad_set(first_and_twelfth_members()),
        bridge_request: bridge_request(&["pad:first:199", "pad:e12:349"]),
        evidence_request: Some(evidence_template()),
    };

    let bundle = build_address_parcel_evidence(&request)
        .expect("address bundle should materialize and wrap evidence");
    let evidence_request = bundle
        .evidence_request
        .clone()
        .expect("positive bridge support should emit a compile-ready evidence request");
    assert_eq!(evidence_request.version, CANON_GEO_EVIDENCE_REQUEST_VERSION);
    assert_eq!(
        evidence_request
            .universe
            .parcels
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["mn:e12:349", "mn:first:199", "mn:other"]
    );
    assert_eq!(evidence_request.contracts.len(), 1);
    assert_eq!(
        evidence_request.contracts[0].id,
        "rho.address.pad.membership"
    );
    assert_eq!(
        evidence_request.observations,
        vec![
            bundle
                .bridge
                .observation
                .clone()
                .expect("bridge observation")
        ]
    );

    let compiled =
        compile_evidence(&evidence_request).expect("emitted evidence request must compile");
    assert_eq!(
        compiled.admissions[0].disposition,
        GeoEvidenceDisposition::HardConstraint
    );
    assert_eq!(
        compiled.composition_request.hard_constraints.len(),
        1,
        "address membership enters as one AnyOf constraint"
    );
    let solved = solve_composition(&compiled.composition_request)
        .expect("compiled address evidence should solve");
    assert_eq!(solved.status, GeoCompositionStatus::Ambiguous);
    assert_eq!(solved.summary.residual_model_count, 6);
}

#[test]
fn address_compile_envelope_must_name_the_candidate_universe() {
    let forest = forest("199 First Avenue a/k/a 349 East 12th Street");
    let pad = pad_set(first_and_twelfth_members());
    let membership = evaluate_pad_membership(&forest, &pad).expect("membership evaluates");
    let bridge = bridge_pad_membership_to_parcel_observation(
        &forest,
        &pad,
        &membership,
        &bridge_request(&["pad:first:199", "pad:e12:349"]),
    )
    .expect("bridge should emit evidence");
    let mut template = evidence_template();
    template.universe.parcels = vec!["mn:first:199".to_string()];

    let error = build_address_parcel_compilation_request(&bridge, &template)
        .expect_err("missing parcel candidates must refuse before compile-ready output");

    assert_eq!(error.code, canon::geo::GeoAddressErrorCode::InvalidInput);
    assert!(error.message.contains("valid Geo evidence request"));
}

#[test]
fn diagnostic_address_bundles_do_not_emit_empty_compile_requests() {
    let chimera_request = GeoAddressParcelEvidenceRequest {
        version: CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION.to_string(),
        parse_request: request("199 E 12th St", GeoNycBorough::Manhattan),
        address_set: pad_set(first_and_twelfth_members()),
        bridge_request: bridge_request(&["pad:first:199", "pad:e12:349"]),
        evidence_request: Some(evidence_template()),
    };
    let chimera = build_address_parcel_evidence(&chimera_request)
        .expect("chimera should materialize as diagnostic abstention");
    assert_eq!(
        chimera.bridge.status,
        GeoAddressParcelBridgeStatus::DiagnosticAbstention
    );
    assert_eq!(
        chimera
            .bridge
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code),
        Some(GeoAddressParcelDiagnosticCode::NoSourceMemberSupport)
    );
    assert!(
        chimera.evidence_request.is_none(),
        "diagnostic abstention must not become a zero-observation compile request"
    );

    let unbound_request = GeoAddressParcelEvidenceRequest {
        version: CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION.to_string(),
        parse_request: request("199 First Avenue", GeoNycBorough::Manhattan),
        address_set: pad_set(first_and_twelfth_members()),
        bridge_request: bridge_request(&[]),
        evidence_request: Some(evidence_template()),
    };
    let unbound = build_address_parcel_evidence(&unbound_request)
        .expect("unbound source member should materialize as diagnostic abstention");
    assert_eq!(
        unbound.bridge.status,
        GeoAddressParcelBridgeStatus::DiagnosticAbstention
    );
    assert_eq!(
        unbound
            .bridge
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code),
        Some(GeoAddressParcelDiagnosticCode::NoBoundSourceRecords)
    );
    assert!(
        unbound.evidence_request.is_none(),
        "unbound source members must abstain instead of fabricating hard evidence"
    );
}

#[test]
fn time_scoped_address_bundle_request_stays_diagnostic_after_compilation() {
    let mut bridge_request = bridge_request(&["pad:first:199"]);
    let interval = GeoValidTimeInterval {
        start_day: 20_696,
        end_day: 20_696,
    };
    bridge_request.query_as_of = Some(GeoAsOf {
        utc_day: "2026-08-31".to_string(),
        semantic_id: "fixture:query_as_of".to_string(),
        unit: "utc_day".to_string(),
        origin: GeoValueOrigin::CallerDeclared,
    });
    bridge_request.valid_time = Some(interval);

    let request = GeoAddressParcelEvidenceRequest {
        version: CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION.to_string(),
        parse_request: request("199 First Avenue", GeoNycBorough::Manhattan),
        address_set: pad_set(first_and_twelfth_members()),
        bridge_request,
        evidence_request: Some(evidence_template()),
    };
    let bundle = build_address_parcel_evidence(&request)
        .expect("time-scoped bundle should materialize with diagnostic evidence");
    let evidence_request = bundle
        .evidence_request
        .expect("supported time-scoped bridge still emits diagnostic evidence request");
    let compiled =
        compile_evidence(&evidence_request).expect("time-scoped evidence request compiles");

    assert_eq!(
        compiled.admissions[0].disposition,
        GeoEvidenceDisposition::DiagnosticOnly
    );
    assert!(compiled.composition_request.hard_constraints.is_empty());
}

#[test]
fn case_four_style_chimera_abstains_instead_of_emitting_an_empty_constraint() {
    let chimera = forest("199 E 12th St");
    let pad = pad_set(first_and_twelfth_members());
    let membership = evaluate_pad_membership(&chimera, &pad).expect("membership evaluates");
    let bridge = bridge_pad_membership_to_parcel_observation(
        &chimera,
        &pad,
        &membership,
        &bridge_request(&["pad:first:199", "pad:e12:349"]),
    )
    .expect("bridge should abstain without failing");

    assert_eq!(
        bridge.status,
        GeoAddressParcelBridgeStatus::DiagnosticAbstention
    );
    assert!(bridge.observation.is_none());
    assert!(bridge.parcel_candidates.is_empty());
    assert_eq!(
        bridge.diagnostic.as_ref().map(|diagnostic| diagnostic.code),
        Some(GeoAddressParcelDiagnosticCode::NoSourceMemberSupport)
    );

    let compiled = compile_evidence(&GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
        universe: GeoCompositionUniverse {
            parcels: vec!["mn:first:199".to_string(), "mn:e12:349".to_string()],
            buildings: Vec::new(),
        },
        contracts: vec![contract()],
        observations: bridge.observation.into_iter().collect(),
        max_assignments: 8,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    })
    .expect("diagnostic bridge state must not fabricate an invalid empty AnyOf");
    assert!(compiled.composition_request.hard_constraints.is_empty());
    let solved = solve_composition(&compiled.composition_request)
        .expect("unconstrained candidate universe remains solvable");
    assert_eq!(solved.status, GeoCompositionStatus::Ambiguous);
    assert_eq!(solved.summary.residual_model_count, 3);
}

#[test]
fn source_member_without_source_record_binding_abstains() {
    let forest = forest("199 First Avenue");
    let pad = pad_set(first_and_twelfth_members());
    let membership = evaluate_pad_membership(&forest, &pad).expect("membership evaluates");
    let bridge = bridge_pad_membership_to_parcel_observation(
        &forest,
        &pad,
        &membership,
        &bridge_request(&[]),
    )
    .expect("bridge should produce a typed abstention");

    assert_eq!(
        bridge.status,
        GeoAddressParcelBridgeStatus::DiagnosticAbstention
    );
    assert!(bridge.observation.is_none());
    assert_eq!(
        bridge.diagnostic.as_ref().map(|diagnostic| diagnostic.code),
        Some(GeoAddressParcelDiagnosticCode::NoBoundSourceRecords)
    );
    assert_eq!(
        bridge.diagnostic.as_ref().map(|diagnostic| diagnostic
            .matched_member_ids_without_source_records
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()),
        Some(vec!["pad:first:199"])
    );
}

#[test]
fn source_binding_must_hash_the_normalized_pad_member_payload() {
    let forest = forest("199 First Avenue");
    let pad = pad_set(first_and_twelfth_members());
    let membership = evaluate_pad_membership(&forest, &pad).expect("membership evaluates");
    let mut request = bridge_request(&["pad:first:199"]);
    request.member_source_records[0].normalized_member_blake3 =
        blake3::hash(b"different normalized member")
            .to_hex()
            .to_string();

    let error = bridge_pad_membership_to_parcel_observation(&forest, &pad, &membership, &request)
        .expect_err("mismatched normalized member binding must refuse");

    assert_eq!(error.code, canon::geo::GeoAddressErrorCode::InvalidInput);
    assert!(error.message.contains("normalized PAD member payload"));
    assert_eq!(error.detail["member_id"], "pad:first:199");
}

#[test]
fn bridge_bytes_are_stable_under_source_record_and_pad_order_permutations() {
    let forest = forest("199 First Avenue a/k/a 349 East 12th Street");
    let members = first_and_twelfth_members();
    let mut reversed_members = members.clone();
    reversed_members.reverse();
    let pad = pad_set(members);
    let reversed_pad = pad_set(reversed_members);
    let membership = evaluate_pad_membership(&forest, &pad).expect("membership evaluates");
    let reversed_membership =
        evaluate_pad_membership(&forest, &reversed_pad).expect("membership evaluates");
    let request = bridge_request(&["pad:first:199", "pad:e12:349"]);
    let mut reversed_request = bridge_request(&["pad:first:199", "pad:e12:349"]);
    let extra = GeoPadMemberSourceRecord {
        member_id: "pad:first:199".to_string(),
        normalized_member_blake3: geo_pad_member_blake3(
            first_and_twelfth_members()
                .iter()
                .find(|member| member.member_id == "pad:first:199")
                .expect("fixture member"),
        )
        .expect("member hash"),
        source_record: source_record("src:pad:first:199:secondary"),
    };
    let mut request = request;
    request.member_source_records.push(extra.clone());
    reversed_request.member_source_records.push(extra);
    reversed_request.member_source_records.reverse();

    let first = bridge_pad_membership_to_parcel_observation(&forest, &pad, &membership, &request)
        .expect("first bridge should build");
    let second = bridge_pad_membership_to_parcel_observation(
        &forest,
        &reversed_pad,
        &reversed_membership,
        &reversed_request,
    )
    .expect("second bridge should build");

    assert_eq!(
        serde_json::to_vec(&first).expect("first bridge serializes"),
        serde_json::to_vec(&second).expect("second bridge serializes")
    );
    assert_eq!(
        canonical_address_parcel_bridge_bytes(&first).expect("canonical bytes"),
        canonical_address_parcel_bridge_bytes(&second).expect("canonical bytes")
    );
}

#[test]
fn valid_time_on_address_membership_remains_diagnostic_after_compilation() {
    let forest = forest("199 First Avenue");
    let pad = pad_set(first_and_twelfth_members());
    let membership = evaluate_pad_membership(&forest, &pad).expect("membership evaluates");
    let interval = GeoValidTimeInterval {
        start_day: 20_696,
        end_day: 20_696,
    };
    let mut request = bridge_request(&["pad:first:199"]);
    request.query_as_of = Some(GeoAsOf {
        utc_day: "2026-08-31".to_string(),
        semantic_id: "fixture:query_as_of".to_string(),
        unit: "utc_day".to_string(),
        origin: GeoValueOrigin::CallerDeclared,
    });
    request.valid_time = Some(interval);
    let bridge = bridge_pad_membership_to_parcel_observation(&forest, &pad, &membership, &request)
        .expect("bridge should preserve valid time");
    assert_eq!(
        bridge
            .query_as_of
            .as_ref()
            .map(|as_of| as_of.utc_day.as_str()),
        Some("2026-08-31")
    );
    assert_eq!(bridge.valid_time, Some(interval));

    let observation = bridge
        .observation
        .expect("supported reading emits observation");
    assert_eq!(observation.valid_time, Some(interval));
    let compiled = compile_evidence(&GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
        universe: GeoCompositionUniverse {
            parcels: vec!["mn:first:199".to_string(), "mn:e12:349".to_string()],
            buildings: Vec::new(),
        },
        contracts: vec![contract()],
        observations: vec![observation],
        max_assignments: 8,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    })
    .expect("time-scoped address evidence should compile as diagnostic");
    assert_eq!(
        compiled.admissions[0].disposition,
        GeoEvidenceDisposition::DiagnosticOnly
    );
    assert!(compiled.composition_request.hard_constraints.is_empty());
}

#[test]
fn address_query_as_of_must_fall_inside_declared_valid_time() {
    let forest = forest("199 First Avenue");
    let pad = pad_set(first_and_twelfth_members());
    let membership = evaluate_pad_membership(&forest, &pad).expect("membership evaluates");
    let mut request = bridge_request(&["pad:first:199"]);
    request.query_as_of = Some(GeoAsOf {
        utc_day: "2026-08-31".to_string(),
        semantic_id: "fixture:query_as_of".to_string(),
        unit: "utc_day".to_string(),
        origin: GeoValueOrigin::CallerDeclared,
    });
    request.valid_time = Some(GeoValidTimeInterval {
        start_day: 20_695,
        end_day: 20_695,
    });

    let error = bridge_pad_membership_to_parcel_observation(&forest, &pad, &membership, &request)
        .expect_err("out-of-interval query-as-of must refuse");

    assert_eq!(error.code, canon::geo::GeoAddressErrorCode::InvalidInput);
    assert!(error.message.contains("inside valid_time"));
}

#[test]
fn convenience_bundle_matches_the_explicit_staged_flow() {
    let parse_request = request(
        "199 First Avenue a/k/a 349 East 12th Street",
        GeoNycBorough::Manhattan,
    );
    let address_set = pad_set(first_and_twelfth_members());
    let bridge_request = bridge_request(&["pad:first:199", "pad:e12:349"]);

    let parse_forest = parse_address_forest(&parse_request).expect("parse stage");
    let pad_membership = evaluate_pad_membership(&parse_forest, &address_set).expect("PAD stage");
    let bridge = bridge_pad_membership_to_parcel_observation(
        &parse_forest,
        &address_set,
        &pad_membership,
        &bridge_request,
    )
    .expect("bridge stage");

    let request = GeoAddressParcelEvidenceRequest {
        version: CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION.to_string(),
        parse_request,
        address_set,
        bridge_request,
        evidence_request: None,
    };
    let bundle = build_address_parcel_evidence(&request).expect("convenience bundle should build");

    assert_eq!(bundle.parse_forest, parse_forest);
    assert_eq!(bundle.pad_membership, pad_membership);
    assert_eq!(
        canonical_address_parcel_bridge_bytes(&bundle.bridge).expect("bundle bridge bytes"),
        canonical_address_parcel_bridge_bytes(&bridge).expect("staged bridge bytes")
    );
    let mut permuted_request = request;
    permuted_request.address_set.members.reverse();
    permuted_request
        .bridge_request
        .member_source_records
        .reverse();
    let permuted_bundle = build_address_parcel_evidence(&permuted_request)
        .expect("permuted convenience bundle should build");
    assert_eq!(
        canonical_address_parcel_evidence_bundle_bytes(&bundle).expect("bundle canonical bytes"),
        canonical_address_parcel_evidence_bundle_bytes(&permuted_bundle)
            .expect("permuted bundle canonical bytes")
    );
}
