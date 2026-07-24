// SPDX-License-Identifier: AGPL-3.0-or-later
//! ADR 0139 P3 compute slice: LML/LMQ expressed as BCS2 codec bundles.
//!
//! Encodes one codec bundle under the requested profile, re-opens it from its
//! encoded bytes, and prints a deterministic JSON line describing what survived
//! the roundtrip:
//!
//! * `input_content_id` / `output_content_id` -- the canonical semantics as fed
//!   in versus as read back, so a semantics-losing bundle is observable;
//! * `profile_id` -- the registered BCS2 profile the bundle declares;
//! * `model_provenance_bound` / `pccp_status` -- for `bcs.lmq.progressive.v1`,
//!   whether the mandatory typed neural-model provenance decoded intact.
//!
//! Everything printed is content-addressed, so repeated runs are byte-identical.

use abir::ContentId;
use abir_bcs::{
    encode_codec_bundle, raw_content_id, CodecBundleInput, CodecBundleView, CodecFidelity,
    CodecFidelityKind, CodecImplementation, CodecParameter, CodecParameterValue, CodecProfile,
    ModelProvenance, PccpStatus, ResourceBounds,
};
use std::path::{Path, PathBuf};

fn semantics_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/valid/canonical-tensor.json")
}

fn lml_input(semantics: &[u8]) -> CodecBundleInput<'_> {
    CodecBundleInput {
        canonical_semantics: semantics,
        fidelity: CodecFidelity {
            bound: None,
            contract_id: ContentId::from_bytes([0x32; 32]),
            kind: CodecFidelityKind::Exact,
            metric: None,
        },
        implementation: CodecImplementation {
            build_id: "adr0139-compute-lml-v1".into(),
            implementation_id: ContentId::from_bytes([0x31; 32]),
            kernel_id: "adr0139-lml-portable-kernel-v1".into(),
        },
        model_provenance: None,
        packets: &[b"LML packet header\x00", b"LML packet residual\x01"],
        parameters: vec![CodecParameter {
            name: "predictor.order".into(),
            value: CodecParameterValue::Integer { value: "8".into() },
        }],
        profile: CodecProfile::LmlLossless,
    }
}

fn lmq_input(semantics: &[u8]) -> CodecBundleInput<'_> {
    CodecBundleInput {
        canonical_semantics: semantics,
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
            build_id: "adr0139-compute-lmq-v1".into(),
            implementation_id: ContentId::from_bytes([0x41; 32]),
            kernel_id: "adr0139-lmq-reference-kernel-v1".into(),
        },
        // A candidate checkpoint: the bundle must carry typed, decodable PCCP
        // provenance. Its status is reported verbatim and never upgraded here.
        model_provenance: Some(ModelProvenance {
            checkpoint_content_id: ContentId::from_bytes([0x43; 32]),
            checkpoint_sha256: [0x44; 32],
            pccp_change_id: "adr0139-compute-candidate".into(),
            pccp_evidence_id: ContentId::from_bytes([0x45; 32]),
            pccp_status: PccpStatus::Candidate,
        }),
        packets: &[b"LMQ base layer\x00", b"LMQ enhancement layer\x01"],
        parameters: vec![
            CodecParameter {
                name: "fsq.level".into(),
                value: CodecParameterValue::Integer { value: "8".into() },
            },
            CodecParameter {
                name: "temporal.tokens".into(),
                value: CodecParameterValue::Integer {
                    value: "79".into(),
                },
            },
        ],
        profile: CodecProfile::LmqProgressive,
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut output, "{byte:02x}").expect("string write cannot fail");
    }
    output
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let profile = arguments.next().ok_or("missing profile (lml|lmq)")?;
    if arguments.next().is_some() {
        return Err("unexpected extra argument".into());
    }
    let semantics = std::fs::read(semantics_path())?;
    let bounds = ResourceBounds::default();
    let input = match profile.as_str() {
        "lml" => lml_input(&semantics),
        "lmq" => lmq_input(&semantics),
        other => return Err(format!("unknown codec profile: {other}").into()),
    };

    let encoded = encode_codec_bundle(input, bounds)?;
    let view = CodecBundleView::open(&encoded, bounds)?;
    let restored = view.canonical_semantics();
    let input_id = raw_content_id(&semantics);
    let output_id = raw_content_id(restored);
    let catalog = view.catalog();
    let provenance = catalog.model_provenance();
    let status = provenance.map_or("absent", |item| match item.pccp_status {
        PccpStatus::Candidate => "candidate",
        PccpStatus::GatePass => "gate-pass",
        PccpStatus::Rejected => "rejected",
    });
    let checkpoint = provenance.map_or_else(String::new, |item| hex(&item.checkpoint_sha256));
    // Spec-normative profile names (spec/BCS2_V1.md, spec/CODEC_BUNDLE_V1.md);
    // the numeric wire code is reported alongside so both bind.
    let profile_name = match catalog.profile() {
        CodecProfile::LmlLossless => "bcs.lml.lossless.v1",
        CodecProfile::LmqProgressive => "bcs.lmq.progressive.v1",
    };
    println!(
        concat!(
            "{{\"profile_id\":\"{}\",\"input_content_id\":\"{}\",",
            "\"output_content_id\":\"{}\",\"packet_count\":{},",
            "\"model_provenance_bound\":{},\"pccp_status\":\"{}\",",
            "\"checkpoint_sha256\":\"{}\",\"bundle_bytes\":{},",
            "\"profile_code\":{}}}"
        ),
        profile_name,
        input_id,
        output_id,
        catalog.packet_count(),
        provenance.is_some(),
        status,
        checkpoint,
        encoded.len(),
        catalog.profile().bcs2_profile().get(),
    );
    Ok(())
}
