use abir::{canonical_debug_json, logical_content_id};
use abir_conformance::{
    canonical_sample_dataset, semantic_matrix_construction_free_dataset, semantic_matrix_dataset,
};
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn rust_matches_full_semantic_matrix_goldens() {
    let dataset = semantic_matrix_dataset();
    assert_eq!(
        canonical_debug_json(&dataset).unwrap(),
        fs::read(root().join("fixtures/valid/semantic-matrix.json")).unwrap()
    );
    assert_eq!(
        format!("{}\n", logical_content_id(&dataset).unwrap()),
        fs::read_to_string(root().join("fixtures/valid/semantic-matrix.content-id")).unwrap()
    );
}

#[test]
fn rust_matches_canonical_json_and_content_id_goldens() {
    let dataset = canonical_sample_dataset();
    assert_eq!(
        canonical_debug_json(&dataset).unwrap(),
        fs::read(root().join("fixtures/valid/canonical-tensor.json")).unwrap()
    );
    assert_eq!(
        format!("{}\n", logical_content_id(&dataset).unwrap()),
        fs::read_to_string(root().join("fixtures/valid/canonical-tensor.content-id")).unwrap()
    );
}

#[test]
fn construction_free_semantic_matrix_retains_frozen_identity() {
    let dataset = semantic_matrix_construction_free_dataset();
    assert_eq!(
        canonical_debug_json(&dataset).unwrap(),
        fs::read(root().join("fixtures/valid/semantic-matrix-construction-free-v1.json")).unwrap()
    );
    let content_id = logical_content_id(&dataset).unwrap().to_string();
    assert_eq!(
        content_id,
        "5dc2c9afcdfa5c96fc15c3a6119cd5084183a2653326c88824ce6fd7a03f1456"
    );
    assert_eq!(
        format!("{content_id}\n"),
        fs::read_to_string(
            root().join("fixtures/valid/semantic-matrix-construction-free-v1.content-id")
        )
        .unwrap()
    );
}
