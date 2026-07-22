use abir_bcs::{CodecBundleView, CodecProfile, PccpStatus, ResourceBounds};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn committed_codec_vectors_match_generator_and_typed_verifier() {
    let root = root();
    let committed = root.join("fixtures/bcs2/codec-v1");
    let temporary = std::env::temp_dir().join(format!("abir-codec-vectors-{}", std::process::id()));
    if temporary.exists() {
        fs::remove_dir_all(&temporary).unwrap();
    }
    let status = Command::new(env!("CARGO_BIN_EXE_generate_codec_bundle_vectors"))
        .arg(&temporary)
        .status()
        .expect("run codec vector generator");
    assert!(status.success());

    let manifest: Value =
        serde_json::from_slice(&fs::read(committed.join("manifest.json")).unwrap()).unwrap();
    for vector in manifest["vectors"].as_array().expect("vectors array") {
        let name = vector["name"].as_str().expect("vector name");
        let expected = fs::read(committed.join(name)).expect("read committed codec vector");
        assert_eq!(
            fs::read(temporary.join(name)).unwrap(),
            expected,
            "generator drift for {name}"
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(&expected)),
            vector["sha256"].as_str().unwrap()
        );
        let view = CodecBundleView::open(&expected, ResourceBounds::default()).unwrap();
        assert_eq!(
            view.root_content_id().to_string(),
            vector["root_content_id"].as_str().unwrap()
        );
        assert_eq!(view.catalog().packet_count(), 2);
        match view.catalog().profile() {
            CodecProfile::LmlLossless => {
                assert!(view.catalog().model_provenance().is_none());
            }
            CodecProfile::LmqProgressive => {
                assert_eq!(
                    view.catalog().model_provenance().unwrap().pccp_status,
                    PccpStatus::Candidate
                );
            }
        }
        let catalog_name = vector["catalog"].as_str().unwrap();
        assert_eq!(
            view.catalog().canonical_json().unwrap(),
            fs::read(committed.join(catalog_name)).unwrap()
        );
    }
    assert_eq!(
        fs::read(temporary.join("manifest.json")).unwrap(),
        fs::read(committed.join("manifest.json")).unwrap()
    );
    fs::remove_dir_all(temporary).unwrap();
}

#[test]
fn corruption_and_physical_reordering_fail_closed() {
    let committed = root().join("fixtures/bcs2/codec-v1/lml-lossless.bcs2");
    let bytes = fs::read(committed).unwrap();

    let mut corrupted = bytes.clone();
    let catalog_len = u64::from_le_bytes(corrupted[64..72].try_into().unwrap()) as usize;
    corrupted[128 + catalog_len] ^= 0x80;
    assert!(CodecBundleView::open(&corrupted, ResourceBounds::default()).is_err());

    let mut reordered = bytes;
    let index_offset = u64::from_le_bytes(reordered[72..80].try_into().unwrap()) as usize;
    let first = reordered[index_offset + 48..index_offset + 176].to_vec();
    let second = reordered[index_offset + 176..index_offset + 304].to_vec();
    reordered[index_offset + 48..index_offset + 176].copy_from_slice(&second);
    reordered[index_offset + 176..index_offset + 304].copy_from_slice(&first);
    assert!(CodecBundleView::open(&reordered, ResourceBounds::default()).is_err());
}
