use abir::ContentId;
use abir_bcs::{
    encode_codec_bundle, CodecBundleInput, CodecBundleView, CodecFidelity, CodecFidelityKind,
    CodecImplementation, CodecParameter, CodecParameterValue, CodecProfile, ModelProvenance,
    PccpStatus, ResourceBounds,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let output = output_directory();
    fs::create_dir_all(&output).expect("create codec fixture directory");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let semantics = fs::read(root.join("fixtures/valid/canonical-tensor.json"))
        .expect("read canonical semantic fixture");
    let bounds = ResourceBounds::default();

    let lml_packets: &[&[u8]] = &[b"LML packet header\x00", b"LML packet residual\x01"];
    let lml = encode_codec_bundle(
        CodecBundleInput {
            canonical_semantics: &semantics,
            fidelity: CodecFidelity {
                bound: None,
                contract_id: ContentId::from_bytes([0x32; 32]),
                kind: CodecFidelityKind::Exact,
                metric: None,
            },
            implementation: CodecImplementation {
                build_id: "fixture-lml-build-v1".into(),
                implementation_id: ContentId::from_bytes([0x31; 32]),
                kernel_id: "fixture-lml-portable-kernel-v1".into(),
            },
            model_provenance: None,
            packets: lml_packets,
            parameters: vec![CodecParameter {
                name: "predictor.order".into(),
                value: CodecParameterValue::Integer { value: "8".into() },
            }],
            profile: CodecProfile::LmlLossless,
        },
        bounds,
    )
    .expect("encode LML codec fixture");

    let lmq_packets: &[&[u8]] = &[b"LMQ base layer\x00", b"LMQ enhancement layer\x01"];
    let lmq = encode_codec_bundle(
        CodecBundleInput {
            canonical_semantics: &semantics,
            fidelity: CodecFidelity {
                bound: Some(CodecParameterValue::Rational {
                    denominator: "1000".into(),
                    numerator: "75".into(),
                }),
                contract_id: ContentId::from_bytes([0x42; 32]),
                kind: CodecFidelityKind::Bounded,
                metric: Some("prd".into()),
            },
            implementation: CodecImplementation {
                build_id: "fixture-lmq-build-v1".into(),
                implementation_id: ContentId::from_bytes([0x41; 32]),
                kernel_id: "fixture-lmq-reference-kernel-v1".into(),
            },
            model_provenance: Some(ModelProvenance {
                checkpoint_content_id: ContentId::from_bytes([0x43; 32]),
                checkpoint_sha256: [0x44; 32],
                pccp_change_id: "fixture-candidate-not-promoted".into(),
                pccp_evidence_id: ContentId::from_bytes([0x45; 32]),
                pccp_status: PccpStatus::Candidate,
            }),
            packets: lmq_packets,
            parameters: vec![
                CodecParameter {
                    name: "fsq.level".into(),
                    value: CodecParameterValue::Integer { value: "8".into() },
                },
                CodecParameter {
                    name: "temporal.tokens".into(),
                    value: CodecParameterValue::Integer { value: "79".into() },
                },
            ],
            profile: CodecProfile::LmqProgressive,
        },
        bounds,
    )
    .expect("encode LMQ codec fixture");

    let vectors = [("lml-lossless.bcs2", lml), ("lmq-progressive.bcs2", lmq)];
    let mut entries = Vec::new();
    for (name, bytes) in vectors {
        let view = CodecBundleView::open(&bytes, bounds).expect("verify generated codec fixture");
        fs::write(output.join(name), &bytes).expect("write codec fixture");
        let catalog_name = name.replace(".bcs2", ".catalog.json");
        fs::write(
            output.join(&catalog_name),
            view.catalog().canonical_json().expect("canonical catalog"),
        )
        .expect("write codec catalog fixture");
        entries.push(json!({
            "bytes": bytes.len(),
            "catalog": catalog_name,
            "name": name,
            "root_content_id": view.root_content_id().to_string(),
            "sha256": format!("{:x}", Sha256::digest(&bytes)),
        }));
    }
    let manifest = json!({
        "generated_by": "cargo run -p abir-conformance --bin generate_codec_bundle_vectors -- fixtures/bcs2/codec-v1",
        "schema_version": 1,
        "vectors": entries,
        "wire_major": 2,
    });
    fs::write(
        output.join("manifest.json"),
        format!("{}\n", serde_json::to_string_pretty(&manifest).unwrap()),
    )
    .expect("write codec fixture manifest");
}

fn output_directory() -> PathBuf {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new("fixtures/bcs2/codec-v1").to_path_buf())
}
