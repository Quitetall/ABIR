use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn normative_manifest_hashes_match() {
    let root = root();
    let manifest: Value = serde_json::from_slice(
        &fs::read(root.join("spec/semantic-v1.manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");

    for artifact in manifest["artifacts"]
        .as_array()
        .expect("manifest.artifacts must be an array")
    {
        let path = artifact["path"].as_str().expect("artifact path");
        let expected = artifact["sha256"].as_str().expect("artifact digest");
        let actual = format!(
            "{:x}",
            Sha256::digest(fs::read(root.join(path)).expect(path))
        );
        assert_eq!(actual, expected, "normative artifact changed: {path}");
    }
}

#[test]
fn stable_registries_have_unique_entries() {
    let root = root();
    for relative in [
        "registries/concepts-v1.json",
        "registries/failures-v1.json",
        "registries/proofs-v1.json",
    ] {
        let registry: Value =
            serde_json::from_slice(&fs::read(root.join(relative)).expect(relative))
                .expect(relative);
        let mut seen = BTreeSet::new();
        for entry in registry["entries"].as_array().expect("entry array") {
            let key = entry
                .get("id")
                .or_else(|| entry.get("code"))
                .and_then(Value::as_str)
                .expect("registry key");
            assert!(
                seen.insert(key),
                "duplicate registry key {key} in {relative}"
            );
        }
    }
}
