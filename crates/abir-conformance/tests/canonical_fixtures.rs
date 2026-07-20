use abir::{canonical_debug_json, logical_content_id};
use abir_conformance::{canonical_sample_dataset, semantic_matrix_dataset};
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
