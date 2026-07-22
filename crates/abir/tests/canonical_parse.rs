use abir::{
    canonical_debug_json, parse_canonical_dataset, parse_canonical_dataset_with_limits,
    FailureCode, ValidationLimits,
};

const CANONICAL_TENSOR: &[u8] = include_bytes!("../../../fixtures/valid/canonical-tensor.json");
const SEMANTIC_MATRIX: &[u8] = include_bytes!("../../../fixtures/valid/semantic-matrix.json");

#[test]
fn canonical_fixtures_round_trip_through_typed_semantics() {
    for fixture in [CANONICAL_TENSOR, SEMANTIC_MATRIX] {
        let dataset = parse_canonical_dataset(fixture).expect("canonical fixture must parse");
        assert_eq!(
            canonical_debug_json(&dataset).expect("dataset must serialize"),
            fixture
        );
    }
}

#[test]
fn malformed_json_reports_the_root_path() {
    let error = parse_canonical_dataset(br#"{"semantic_version":"1""#)
        .expect_err("malformed JSON must fail");

    assert_eq!(error.path(), "$");
    assert!(error
        .document_message()
        .expect("document failure")
        .starts_with("invalid JSON:"));
}

#[test]
fn unsupported_semantic_version_reports_its_field() {
    let document = CANONICAL_TENSOR
        .windows(b"\"semantic_version\":\"1\"".len())
        .position(|window| window == b"\"semantic_version\":\"1\"")
        .expect("fixture version marker");
    let mut changed = CANONICAL_TENSOR.to_vec();
    let version = document + b"\"semantic_version\":\"".len();
    changed[version] = b'2';

    let error = parse_canonical_dataset(&changed).expect_err("version 2 must fail");
    assert_eq!(error.path(), "$.semantic_version");
    assert_eq!(
        error.document_message(),
        Some("expected semantic version 1")
    );
}

#[test]
fn unmodeled_members_are_rejected_as_noncanonical() {
    let mut value: serde_json::Value =
        serde_json::from_slice(CANONICAL_TENSOR).expect("fixture JSON");
    value
        .as_object_mut()
        .expect("fixture root")
        .insert("unmodeled".into(), serde_json::Value::Bool(true));
    let document = serde_json::to_vec(&value).expect("mutated JSON");

    let error = parse_canonical_dataset(&document).expect_err("unknown members must fail closed");
    assert_eq!(error.path(), "$");
    assert_eq!(
        error.document_message(),
        Some("document is not the exact semantic-v1 canonical debug form")
    );
}

#[test]
fn caller_limits_return_the_complete_structured_validation_report() {
    let limits = ValidationLimits {
        max_recordings: 0,
        ..ValidationLimits::default()
    };
    let error = parse_canonical_dataset_with_limits(CANONICAL_TENSOR, limits)
        .expect_err("caller limit must fail closed");
    let report = error
        .validation_report()
        .expect("semantic verifier must remain structured");

    assert!(!report.is_empty());
    assert_eq!(
        report.failures()[0].failure_code(),
        FailureCode::StructuralLimit
    );
    assert_eq!(error.path(), report.failures()[0].path());
    assert_eq!(error.to_string(), "ABIR-E012 at recordings");
}

#[test]
fn malformed_nested_values_report_the_exact_field_path() {
    let cases = [
        (
            "/atoms/0/payload/element",
            serde_json::Value::String("unknown".into()),
            "$.atoms[0].payload.element",
        ),
        (
            "/atoms/0/payload/byte_order",
            serde_json::Value::String("middle".into()),
            "$.atoms[0].payload.byte_order",
        ),
        (
            "/atoms/0/payload/layout",
            serde_json::Value::String("diagonal".into()),
            "$.atoms[0].payload.layout",
        ),
    ];
    for (pointer, replacement, expected_path) in cases {
        let mut value: serde_json::Value =
            serde_json::from_slice(CANONICAL_TENSOR).expect("fixture JSON");
        *value.pointer_mut(pointer).expect("fixture field") = replacement;
        let error = parse_canonical_dataset(&serde_json::to_vec(&value).expect("mutated JSON"))
            .expect_err("malformed nested value must fail");
        assert_eq!(error.path(), expected_path);
    }

    let mut value: serde_json::Value =
        serde_json::from_slice(SEMANTIC_MATRIX).expect("fixture JSON");
    *value
        .pointer_mut("/proofs/0/subject/id")
        .expect("semantic reference ID") = serde_json::Value::String("not-an-object-id".into());
    let error = parse_canonical_dataset(&serde_json::to_vec(&value).expect("mutated JSON"))
        .expect_err("malformed semantic reference ID must fail");
    assert_eq!(error.path(), "$.proofs[0].subject.id");
}

#[test]
fn every_canonical_element_name_including_utf8_is_parseable() {
    let mut value: serde_json::Value =
        serde_json::from_slice(SEMANTIC_MATRIX).expect("fixture JSON");
    *value
        .pointer_mut("/atoms/2/columns/0/element")
        .expect("table column element") = serde_json::Value::String("utf8".into());
    let document = serde_json::to_vec(&value).expect("mutated JSON");

    let dataset = parse_canonical_dataset(&document).expect("UTF-8 element must parse");
    assert_eq!(canonical_debug_json(&dataset).unwrap(), document);
}

#[test]
fn composite_layout_and_explicit_time_members_report_exact_paths() {
    let cases = [
        (
            "/atoms/1/payload/layout/ragged/offsets",
            serde_json::Value::String("bad".into()),
            "$.atoms[1].payload.layout.ragged.offsets",
        ),
        (
            "/atoms/6/payload/layout/sparse-csr/indices",
            serde_json::Value::String("bad".into()),
            "$.atoms[6].payload.layout.sparse-csr.indices",
        ),
        (
            "/atoms/3/payload/layout/bfp/mantissa_bits",
            serde_json::Value::Number(999_u64.into()),
            "$.atoms[3].payload.layout.bfp.mantissa_bits",
        ),
    ];
    for (pointer, replacement, expected_path) in cases {
        let mut value: serde_json::Value =
            serde_json::from_slice(SEMANTIC_MATRIX).expect("fixture JSON");
        *value.pointer_mut(pointer).expect("fixture field") = replacement;
        let error = parse_canonical_dataset(&serde_json::to_vec(&value).expect("mutated JSON"))
            .expect_err("malformed composite member must fail");
        assert_eq!(error.path(), expected_path);
    }

    let mut value: serde_json::Value =
        serde_json::from_slice(SEMANTIC_MATRIX).expect("fixture JSON");
    *value.pointer_mut("/atoms/0/time_axis").expect("time axis") = serde_json::json!({
        "explicit": {"timestamps": "bad", "count": 4}
    });
    let error = parse_canonical_dataset(&serde_json::to_vec(&value).expect("mutated JSON"))
        .expect_err("malformed explicit timestamp identity must fail");
    assert_eq!(error.path(), "$.atoms[0].time_axis.explicit.timestamps");
}

#[test]
fn nested_collection_shape_errors_retain_their_owner_path() {
    let mut value: serde_json::Value =
        serde_json::from_slice(CANONICAL_TENSOR).expect("fixture JSON");
    *value
        .pointer_mut("/recordings/0/streams")
        .expect("recording streams") = serde_json::Value::String("not-an-array".into());

    let error = parse_canonical_dataset(&serde_json::to_vec(&value).expect("mutated JSON"))
        .expect_err("nested collection with wrong shape must fail");
    assert_eq!(error.path(), "$.recordings[0].streams");
}
