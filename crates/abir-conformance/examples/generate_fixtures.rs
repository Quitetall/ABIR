use abir::{canonical_debug_json, logical_content_id};
use abir_conformance::{canonical_sample_dataset, semantic_matrix_dataset};
use std::fs;
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let valid = root.join("fixtures/valid");
    fs::create_dir_all(&valid).expect("create fixture directory");
    write_fixture(&valid, "canonical-tensor", &canonical_sample_dataset());
    write_fixture(&valid, "semantic-matrix", &semantic_matrix_dataset());
}

fn write_fixture(valid: &Path, name: &str, dataset: &abir::AbirDataset) {
    fs::write(
        valid.join(format!("{name}.json")),
        canonical_debug_json(dataset).expect("canonical JSON"),
    )
    .expect("write JSON fixture");
    fs::write(
        valid.join(format!("{name}.content-id")),
        format!(
            "{}\n",
            logical_content_id(dataset).expect("logical content ID")
        ),
    )
    .expect("write content ID fixture");
}
