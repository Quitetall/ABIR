use abir_bcs::{Bcs2View, EncryptedEnvelopeView, ForensicTreeView, ResourceBounds, BCS2_MAGIC};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn committed_bcs2_vectors_match_generator_and_rust_reader() {
    let root = root();
    let committed = root.join("fixtures/bcs2/v1");
    let temporary = std::env::temp_dir().join(format!("abir-bcs2-vectors-{}", std::process::id()));
    if temporary.exists() {
        fs::remove_dir_all(&temporary).unwrap();
    }
    let status = Command::new(env!("CARGO_BIN_EXE_generate_bcs2_vectors"))
        .arg(&temporary)
        .status()
        .expect("run BCS2 vector generator");
    assert!(status.success());

    let manifest: Value =
        serde_json::from_slice(&fs::read(committed.join("manifest.json")).unwrap())
            .expect("parse vector manifest");
    for vector in manifest["vectors"].as_array().expect("vectors array") {
        let name = vector["name"].as_str().expect("vector name");
        let expected = fs::read(committed.join(name)).expect("read committed vector");
        let generated = fs::read(temporary.join(name)).expect("read generated vector");
        assert_eq!(generated, expected, "generator drift for {name}");
        assert_eq!(
            format!("{:x}", Sha256::digest(&expected)),
            vector["sha256"].as_str().expect("vector sha256")
        );
        assert_eq!(expected.len() as u64, vector["bytes"].as_u64().unwrap());
        assert_eq!(&expected[..8], &BCS2_MAGIC);
        if name == "encrypted-discoverable.bcs2" {
            EncryptedEnvelopeView::parse(&expected, ResourceBounds::default()).unwrap();
        } else {
            let view = Bcs2View::parse(&expected, 0, ResourceBounds::default()).unwrap();
            assert_eq!(
                view.root_content_id().to_string(),
                vector["root_content_id"].as_str().unwrap()
            );
            if name == "forensic-tree.bcs2" {
                ForensicTreeView::parse(&expected, 0, ResourceBounds::default()).unwrap();
            }
        }
    }
    assert_eq!(
        fs::read(temporary.join("manifest.json")).unwrap(),
        fs::read(committed.join("manifest.json")).unwrap(),
        "manifest generator drift"
    );
    fs::remove_dir_all(temporary).unwrap();
}
