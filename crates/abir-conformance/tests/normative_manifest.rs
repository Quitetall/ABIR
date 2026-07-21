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
fn bcs2_manifest_hashes_match() {
    let root = root();
    let manifest: Value = serde_json::from_slice(
        &fs::read(root.join("spec/bcs2-v1.manifest.json")).expect("read BCS2 manifest"),
    )
    .expect("parse BCS2 manifest");

    for artifact in manifest["artifacts"]
        .as_array()
        .expect("BCS2 manifest.artifacts must be an array")
    {
        let path = artifact["path"].as_str().expect("artifact path");
        let expected = artifact["sha256"].as_str().expect("artifact digest");
        let actual = format!(
            "{:x}",
            Sha256::digest(fs::read(root.join(path)).expect(path))
        );
        assert_eq!(actual, expected, "normative BCS2 artifact changed: {path}");
    }
}

#[test]
fn bcs2_profile_registry_is_stable_and_unambiguous() {
    let root = root();
    let registry: Value = serde_json::from_slice(
        &fs::read(root.join("registries/bcs2-profiles-v1.json")).expect("read registry"),
    )
    .expect("parse registry");
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for profile in registry["profiles"].as_array().expect("profiles array") {
        let id = profile["id"].as_u64().expect("profile id");
        let name = profile["name"].as_str().expect("profile name");
        assert!(ids.insert(id), "duplicate BCS2 profile id {id}");
        assert!(names.insert(name), "duplicate BCS2 profile name {name}");
        assert!(
            !(profile["portable"].as_bool().expect("portable")
                && profile["external_references"]
                    .as_bool()
                    .expect("external_references")),
            "portable profile {name} cannot permit external references"
        );
        let family = registry["id_family_prefixes"][(id >> 16).to_string()]
            .as_str()
            .expect("registered profile family");
        assert!(
            name.starts_with(&format!("bcs.{family}.")),
            "profile {name} does not match its numeric family"
        );
    }
    for retired in registry["retired_ids"]
        .as_array()
        .expect("retired_ids array")
    {
        assert!(
            !ids.contains(&retired.as_u64().expect("retired profile id")),
            "active BCS2 profile id is retired"
        );
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
