use canon::geo::{
    CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION, CANON_GEO_PAD_ADDRESS_SET_VERSION,
    GeoAddressAnnotation, GeoAddressErrorCode, GeoAddressHouseNumber, GeoAddressJurisdiction,
    GeoAddressParity, GeoAddressParseForest, GeoAddressParseRequest, GeoAddressRangeOperator,
    GeoAddressStreet, GeoNycBorough, GeoPadAddressMember, GeoPadAddressSet, GeoPadCandidateStatus,
    GeoStreetDirection, GeoStreetSuffix, canonical_pad_membership_bytes, evaluate_pad_membership,
    parse_address_forest,
};
use std::collections::BTreeSet;

fn request(input: &str, borough: GeoNycBorough) -> GeoAddressParseRequest {
    GeoAddressParseRequest {
        version: CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION.to_string(),
        input: input.to_string(),
        jurisdiction: Some(GeoAddressJurisdiction::nyc_borough(borough)),
    }
}

fn forest(input: &str, borough: GeoNycBorough) -> GeoAddressParseForest {
    parse_address_forest(&request(input, borough)).expect("fixture address must parse")
}

fn displays(forest: &GeoAddressParseForest) -> Vec<String> {
    forest
        .candidates
        .iter()
        .map(|candidate| candidate.display.clone())
        .collect()
}

fn candidate_keys(forest: &GeoAddressParseForest) -> Vec<String> {
    forest
        .candidates
        .iter()
        .map(|candidate| candidate.canonical_key.clone())
        .collect()
}

fn string_set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn asserted_member_ids(membership: &canon::geo::GeoPadMembershipEvaluation) -> BTreeSet<String> {
    membership
        .results
        .iter()
        .filter(|result| result.asserted_member)
        .flat_map(|result| result.matched_member_ids.iter().cloned())
        .collect()
}

fn pad_set(borough: GeoNycBorough, members: Vec<GeoPadAddressMember>) -> GeoPadAddressSet {
    GeoPadAddressSet {
        version: CANON_GEO_PAD_ADDRESS_SET_VERSION.to_string(),
        jurisdiction: GeoAddressJurisdiction::nyc_borough(borough),
        members,
    }
}

fn member(
    member_id: &str,
    lot_id: &str,
    house: GeoAddressHouseNumber,
    street: GeoAddressStreet,
) -> GeoPadAddressMember {
    GeoPadAddressMember::new(member_id, lot_id, house, street)
}

fn ordinal_street(
    direction: Option<GeoStreetDirection>,
    value: u16,
    suffix: GeoStreetSuffix,
) -> GeoAddressStreet {
    GeoAddressStreet::ordinal(direction, value, Some(suffix)).expect("street fixture is valid")
}

fn literal_street(name: &[&str], suffix: Option<GeoStreetSuffix>) -> GeoAddressStreet {
    GeoAddressStreet::literal(None, name, suffix).expect("street fixture is valid")
}

#[test]
fn six_worked_case_shapes_parse_as_address_domains() {
    let cases = [
        (
            "1 Grace Court",
            GeoNycBorough::Brooklyn,
            vec!["1 Grace Court"],
        ),
        (
            "982 Madison St",
            GeoNycBorough::Brooklyn,
            vec!["982 Madison Street"],
        ),
        (
            "107-109-111 North 9th",
            GeoNycBorough::Brooklyn,
            vec!["107-111 North 9th", "107-111 North 9th Street"],
        ),
        (
            "199, 201, 203, 205 First Avenue and 349 & 351 East 12th Street",
            GeoNycBorough::Manhattan,
            vec![
                "199 1st Avenue",
                "201 1st Avenue",
                "203 1st Avenue",
                "205 1st Avenue",
                "349 East 12th Street",
                "351 East 12th Street",
            ],
        ),
        (
            "66 Crosby Street a/k/a 514 Broadway",
            GeoNycBorough::Manhattan,
            vec!["66 Crosby Street", "514 Broadway"],
        ),
        (
            "305 E 72nd St",
            GeoNycBorough::Manhattan,
            vec!["305 East 72nd Street"],
        ),
    ];

    for (input, borough, expected) in cases {
        assert_eq!(displays(&forest(input, borough)), expected, "{input}");
    }
}

#[test]
fn case_four_pad_membership_rejects_planted_chimera() {
    let case_four = forest(
        "199, 201, 203, 205 First Avenue and 349 & 351 East 12th Street",
        GeoNycBorough::Manhattan,
    );
    assert!(
        !displays(&case_four)
            .iter()
            .any(|display| display == "199 East 12th Street"),
        "the parser must not synthesize a number/street chimera"
    );

    let first_ave = ordinal_street(None, 1, GeoStreetSuffix::Avenue);
    let east_12th = ordinal_street(Some(GeoStreetDirection::East), 12, GeoStreetSuffix::Street);
    let pad = pad_set(
        GeoNycBorough::Manhattan,
        vec![
            member(
                "pad:first:199",
                "mn:first:199",
                GeoAddressHouseNumber::discrete(199),
                first_ave.clone(),
            ),
            member(
                "pad:first:201",
                "mn:first:201",
                GeoAddressHouseNumber::discrete(201),
                first_ave.clone(),
            ),
            member(
                "pad:first:203",
                "mn:first:203",
                GeoAddressHouseNumber::discrete(203),
                first_ave.clone(),
            ),
            member(
                "pad:first:205",
                "mn:first:205",
                GeoAddressHouseNumber::discrete(205),
                first_ave,
            ),
            member(
                "pad:e12:349",
                "mn:e12:349",
                GeoAddressHouseNumber::discrete(349),
                east_12th.clone(),
            ),
            member(
                "pad:e12:351",
                "mn:e12:351",
                GeoAddressHouseNumber::discrete(351),
                east_12th.clone(),
            ),
        ],
    );

    let membership = evaluate_pad_membership(&case_four, &pad).expect("membership evaluates");
    assert_eq!(membership.results.len(), 6);
    assert!(membership.results.iter().all(|result| {
        result.status == GeoPadCandidateStatus::ExactMember
            && result.asserted_member
            && result.compatible
    }));

    let chimera = forest("199 E 12th St", GeoNycBorough::Manhattan);
    let chimera_membership =
        evaluate_pad_membership(&chimera, &pad).expect("chimera membership evaluates");
    assert_eq!(chimera_membership.results.len(), 1);
    assert_eq!(
        chimera_membership.results[0].status,
        GeoPadCandidateStatus::NoSourceMember
    );
    assert!(!chimera_membership.results[0].asserted_member);
    assert!(!chimera_membership.results[0].compatible);
}

#[test]
fn bd_2rc0_524efa30_semicolon_queens_addresses_assert_truth_rows_not_neighbor() {
    let parsed = forest(
        "18-27A; 18-29; 18-31; 18-33 42ND STREET",
        GeoNycBorough::Queens,
    );
    assert_eq!(
        displays(&parsed),
        vec![
            "18-27A 42nd Street",
            "18-29 42nd Street",
            "18-31 42nd Street",
            "18-33 42nd Street"
        ]
    );

    let street = ordinal_street(None, 42, GeoStreetSuffix::Street);
    let pad = pad_set(
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

    let membership = evaluate_pad_membership(&parsed, &pad).expect("membership evaluates");
    assert_eq!(
        asserted_member_ids(&membership),
        string_set(&["4007910022", "4007910023", "4007910122", "4007910123"])
    );
    assert!(
        !asserted_member_ids(&membership).contains("4008020060"),
        "bridge input must not fall through to the neighboring 18-22 42 Street PAD row"
    );
}

#[test]
fn bd_2rc0_a6dd2e31_house_numbers_remain_bound_to_their_street_segment() {
    let parsed = forest(
        "345, 347, 349 EAST 60TH STREET AND 1097, 1099, 1101, 1103 1ST AVENUE",
        GeoNycBorough::Manhattan,
    );
    assert_eq!(
        displays(&parsed),
        vec![
            "345 East 60th Street",
            "347 East 60th Street",
            "349 East 60th Street",
            "1097 1st Avenue",
            "1099 1st Avenue",
            "1101 1st Avenue",
            "1103 1st Avenue"
        ]
    );
    assert!(
        !displays(&parsed)
            .iter()
            .any(|display| display == "345 1st Avenue"),
        "the parser must not carry the 345 house number onto the 1 Avenue segment"
    );

    let east_60th = ordinal_street(Some(GeoStreetDirection::East), 60, GeoStreetSuffix::Street);
    let first_ave = ordinal_street(None, 1, GeoStreetSuffix::Avenue);
    let pad = pad_set(
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

    let membership = evaluate_pad_membership(&parsed, &pad).expect("membership evaluates");
    assert_eq!(
        asserted_member_ids(&membership),
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
        !asserted_member_ids(&membership).contains("1009260034"),
        "the 345 1 Avenue PAD row is a number/street chimera for this filing"
    );
}

#[test]
fn slash_and_dash_ranges_remain_range_readings_for_pad_compatibility() {
    let west_74th = ordinal_street(Some(GeoStreetDirection::West), 74, GeoStreetSuffix::Street);
    let range_house = GeoAddressHouseNumber::range(
        241,
        249,
        GeoAddressParity::Odd,
        GeoAddressRangeOperator::Slash,
        vec![241, 249],
    )
    .expect("range fixture is valid");
    let pad = pad_set(
        GeoNycBorough::Manhattan,
        vec![member(
            "pad:w74:241-249",
            "mn:w74:lot",
            range_house,
            west_74th,
        )],
    );
    let slash = forest("241/249 West 74th Street", GeoNycBorough::Manhattan);
    assert_eq!(slash.candidates.len(), 1);
    assert!(matches!(
        slash.candidates[0].house,
        GeoAddressHouseNumber::Range {
            start: 241,
            end: 249,
            parity: GeoAddressParity::Odd,
            operator: GeoAddressRangeOperator::Slash,
            ..
        }
    ));
    assert!(
        slash.candidates[0]
            .annotations
            .contains(&GeoAddressAnnotation::SlashRange)
    );
    let slash_membership = evaluate_pad_membership(&slash, &pad).expect("membership evaluates");
    assert_eq!(
        slash_membership.results[0].status,
        GeoPadCandidateStatus::RangeContained
    );

    let north_9th = ordinal_street(Some(GeoStreetDirection::North), 9, GeoStreetSuffix::Street);
    let brooklyn_pad = pad_set(
        GeoNycBorough::Brooklyn,
        vec![
            member(
                "pad:n9:107",
                "bk:n9:107",
                GeoAddressHouseNumber::discrete(107),
                north_9th.clone(),
            ),
            member(
                "pad:n9:109",
                "bk:n9:109",
                GeoAddressHouseNumber::discrete(109),
                north_9th.clone(),
            ),
            member(
                "pad:n9:111",
                "bk:n9:111",
                GeoAddressHouseNumber::discrete(111),
                north_9th,
            ),
        ],
    );
    let dash = forest("107-109-111 North 9th", GeoNycBorough::Brooklyn);
    assert!(dash.candidates.iter().any(|candidate| {
        matches!(
            candidate.house,
            GeoAddressHouseNumber::Range {
                start: 107,
                end: 111,
                parity: GeoAddressParity::Odd,
                operator: GeoAddressRangeOperator::DashList,
                ..
            }
        ) && candidate
            .annotations
            .contains(&GeoAddressAnnotation::DashListRange)
    }));
    let dash_membership =
        evaluate_pad_membership(&dash, &brooklyn_pad).expect("membership evaluates");
    let covered = dash_membership
        .results
        .iter()
        .find(|result| result.status == GeoPadCandidateStatus::CoveredByAddressSet)
        .expect("implicit North 9th Street reading must be covered by discrete PAD members");
    assert_eq!(
        covered.matched_member_ids,
        vec!["pad:n9:107", "pad:n9:109", "pad:n9:111"]
    );
}

#[test]
fn queens_hyphenates_are_literal_only_under_declared_queens_jurisdiction() {
    let queens = forest("130-50 146 Street", GeoNycBorough::Queens);
    assert_eq!(displays(&queens), vec!["130-50 146 Street"]);
    assert!(matches!(
        queens.candidates[0].house,
        GeoAddressHouseNumber::HyphenatedLiteral { .. }
    ));
    assert!(
        queens.candidates[0]
            .annotations
            .contains(&GeoAddressAnnotation::QueensHyphenateLiteral)
    );

    let missing = parse_address_forest(&GeoAddressParseRequest {
        version: CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION.to_string(),
        input: "130-50 146 Street".to_string(),
        jurisdiction: None,
    })
    .expect_err("jurisdiction absence must be typed");
    assert_eq!(missing.code, GeoAddressErrorCode::MissingJurisdiction);

    let ambiguous = parse_address_forest(&GeoAddressParseRequest {
        version: CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION.to_string(),
        input: "130-50 146 Street".to_string(),
        jurisdiction: Some(GeoAddressJurisdiction::Nyc { borough: None }),
    })
    .expect_err("NYC without borough must be typed as ambiguous");
    assert_eq!(ambiguous.code, GeoAddressErrorCode::AmbiguousJurisdiction);

    let wrong_borough =
        parse_address_forest(&request("130-50 146 Street", GeoNycBorough::Manhattan))
            .expect_err("Queens hyphenate syntax must not be guessed in Manhattan");
    assert_eq!(wrong_borough.code, GeoAddressErrorCode::InvalidHouseNumber);
}

#[test]
fn placeholders_are_typed_without_candidate_readings() {
    let forest = forest("Various", GeoNycBorough::Manhattan);
    assert!(forest.candidates.is_empty());
    assert!(forest.placeholder.is_some());
}

#[test]
fn direction_suffix_and_ordinal_classes_share_canonical_keys() {
    let abbreviated = forest("305 E 72nd St", GeoNycBorough::Manhattan);
    let expanded = forest("305 East 72nd Street", GeoNycBorough::Manhattan);
    assert_eq!(candidate_keys(&abbreviated), candidate_keys(&expanded));

    let word_ordinal = forest("199 First Avenue", GeoNycBorough::Manhattan);
    let numeric_ordinal = forest("199 1st Ave", GeoNycBorough::Manhattan);
    assert_eq!(
        candidate_keys(&word_ordinal),
        candidate_keys(&numeric_ordinal)
    );
}

#[test]
fn candidate_order_and_pad_evaluation_are_stable_under_input_permutation() {
    let canonical = forest(
        "199, 201, 203, 205 First Avenue and 349 & 351 East 12th Street",
        GeoNycBorough::Manhattan,
    );
    let permuted = forest(
        "351 & 349 East 12th Street and 205, 203, 201, 199 First Avenue",
        GeoNycBorough::Manhattan,
    );
    assert_eq!(candidate_keys(&canonical), candidate_keys(&permuted));

    let first_ave = ordinal_street(None, 1, GeoStreetSuffix::Avenue);
    let east_12th = ordinal_street(Some(GeoStreetDirection::East), 12, GeoStreetSuffix::Street);
    let members = vec![
        member(
            "pad:first:199",
            "mn:first:199",
            GeoAddressHouseNumber::discrete(199),
            first_ave.clone(),
        ),
        member(
            "pad:first:201",
            "mn:first:201",
            GeoAddressHouseNumber::discrete(201),
            first_ave.clone(),
        ),
        member(
            "pad:first:203",
            "mn:first:203",
            GeoAddressHouseNumber::discrete(203),
            first_ave.clone(),
        ),
        member(
            "pad:first:205",
            "mn:first:205",
            GeoAddressHouseNumber::discrete(205),
            first_ave,
        ),
        member(
            "pad:e12:349",
            "mn:e12:349",
            GeoAddressHouseNumber::discrete(349),
            east_12th.clone(),
        ),
        member(
            "pad:e12:351",
            "mn:e12:351",
            GeoAddressHouseNumber::discrete(351),
            east_12th,
        ),
    ];
    let mut reversed_members = members.clone();
    reversed_members.reverse();

    let first = evaluate_pad_membership(&canonical, &pad_set(GeoNycBorough::Manhattan, members))
        .expect("membership evaluates");
    let second = evaluate_pad_membership(
        &canonical,
        &pad_set(GeoNycBorough::Manhattan, reversed_members),
    )
    .expect("membership evaluates");
    assert_eq!(
        canonical_pad_membership_bytes(&first).expect("canonical bytes"),
        canonical_pad_membership_bytes(&second).expect("canonical bytes")
    );
}

#[test]
fn aka_alternatives_are_preserved_as_separate_candidate_members() {
    let forest = forest(
        "66 Crosby Street a/k/a 514 Broadway",
        GeoNycBorough::Manhattan,
    );
    assert_eq!(forest.candidates.len(), 2);
    assert!(forest.candidates.iter().all(|candidate| {
        candidate
            .annotations
            .contains(&GeoAddressAnnotation::AkaAlternative)
    }));

    let pad = pad_set(
        GeoNycBorough::Manhattan,
        vec![
            member(
                "pad:crosby:66",
                "mn:crosby:broadway",
                GeoAddressHouseNumber::discrete(66),
                literal_street(&["crosby"], Some(GeoStreetSuffix::Street)),
            ),
            member(
                "pad:broadway:514",
                "mn:crosby:broadway",
                GeoAddressHouseNumber::discrete(514),
                literal_street(&["broadway"], None),
            ),
        ],
    );
    let membership = evaluate_pad_membership(&forest, &pad).expect("membership evaluates");
    assert_eq!(membership.results.len(), 2);
    assert!(
        membership
            .results
            .iter()
            .all(|result| result.status == GeoPadCandidateStatus::ExactMember)
    );
}
