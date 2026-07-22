use abir::{DatasetDraft, DatasetTag, ObjectId, ValidationLimits};
use abir_bcs::{
    append_dataset_generation, encode_blob, encode_dataset, encode_forensic_tree,
    encode_generational_dataset, encrypt_bcs2, Bcs2View, ForensicEntry, ForensicFileType,
    ForensicTree, ForensicTreeView, PrivacyMode, ProfileId, ResourceBounds,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let output = output_directory();
    fs::create_dir_all(&output).expect("create BCS2 fixture directory");
    let bounds = ResourceBounds::default();
    let dataset = DatasetDraft::new(ObjectId::<DatasetTag>::from_bytes([0x21; 16]))
        .validate(ValidationLimits::default())
        .expect("fixture dataset");
    let dataset_bytes = encode_dataset(&dataset, ProfileId::LML_LOSSLESS_V1, bounds)
        .expect("encode dataset fixture");

    let blob_bytes = encode_blob(
        b"ABIR forensic image\0\x01",
        "application/octet-stream",
        bounds,
    )
    .expect("encode blob fixture");

    let tree = fixture_tree();
    let tree_bytes = encode_forensic_tree(&tree, bounds).expect("encode tree fixture");

    let mut generation_bytes =
        encode_generational_dataset(&dataset, ProfileId::LML_LOSSLESS_V1, bounds, [])
            .expect("encode generation zero");
    let next_dataset = DatasetDraft::new(ObjectId::<DatasetTag>::from_bytes([0x22; 16]))
        .validate(ValidationLimits::default())
        .expect("second fixture dataset");
    append_dataset_generation(&mut generation_bytes, &next_dataset, [], 0, bounds)
        .expect("append fixture generation");

    let encrypted_bytes = encrypt_bcs2(
        &dataset_bytes,
        PrivacyMode::EncryptedDiscoverable,
        &[0x42; 32],
        &[0x24; 24],
        0,
        bounds,
    )
    .expect("encrypt fixture");

    let vectors = [
        ("dataset.bcs2", dataset_bytes),
        ("forensic-image.bcs2", blob_bytes),
        ("forensic-tree.bcs2", tree_bytes),
        ("generational-dataset.bcs2", generation_bytes),
        ("encrypted-discoverable.bcs2", encrypted_bytes),
    ];
    let mut entries = Vec::new();
    for (name, bytes) in vectors {
        fs::write(output.join(name), &bytes).expect("write fixture");
        let identity = if name == "forensic-tree.bcs2" {
            ForensicTreeView::parse(&bytes, 0, bounds)
                .expect("parse tree fixture")
                .content_id()
        } else if name == "encrypted-discoverable.bcs2" {
            Bcs2View::parse(
                &fs::read(output.join("dataset.bcs2")).expect("read dataset fixture"),
                0,
                bounds,
            )
            .expect("parse dataset fixture")
            .root_content_id()
        } else {
            Bcs2View::parse(&bytes, 0, bounds)
                .expect("parse fixture")
                .root_content_id()
        };
        entries.push(json!({
            "name": name,
            "bytes": bytes.len(),
            "root_content_id": identity.to_string(),
            "sha256": format!("{:x}", Sha256::digest(&bytes)),
        }));
    }
    let manifest = json!({
        "schema_version": 1,
        "wire_major": 2,
        "generated_by": "cargo run -p abir-conformance --bin generate_bcs2_vectors -- fixtures/bcs2/v1",
        "vectors": entries,
    });
    fs::write(
        output.join("manifest.json"),
        format!("{}\n", serde_json::to_string_pretty(&manifest).unwrap()),
    )
    .expect("write fixture manifest");
}

fn output_directory() -> PathBuf {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new("fixtures/bcs2/v1").to_path_buf())
}

fn fixture_tree() -> ForensicTree {
    let directory = entry(b"source", ForensicFileType::Directory, None);
    let mut raw = entry(
        b"source/raw.edf",
        ForensicFileType::Regular,
        Some(b"EDF fixture bytes".to_vec()),
    );
    raw.mode = 0o640;
    let mut hardlink = entry(b"source/raw-copy.edf", ForensicFileType::Hardlink, None);
    hardlink.mode = raw.mode;
    hardlink.hardlink_target = Some(b"source/raw.edf".to_vec());
    let mut symlink = entry(b"source/raw-current.edf", ForensicFileType::Symlink, None);
    symlink.mode = 0o777;
    symlink.symlink_target = Some(b"raw.edf".to_vec());
    let mut entries = vec![directory, raw, hardlink, symlink];
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    ForensicTree {
        platform: "linux".into(),
        entries,
    }
}

fn entry(path: &[u8], file_type: ForensicFileType, content: Option<Vec<u8>>) -> ForensicEntry {
    ForensicEntry {
        path: path.to_vec(),
        file_type,
        mode: 0o755,
        owner: None,
        timestamps: [None; 4],
        acl: None,
        xattrs: Vec::new(),
        hardlink_target: None,
        symlink_target: None,
        sparse_extents: Vec::new(),
        flags: 0,
        device: None,
        special_type: None,
        content,
    }
}
