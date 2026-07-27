//! The LML optimum kernel family is gated by a required-capability bit, not by a
//! separate profile.
//!
//! ADR 0139 contract 5 says BCS2 is ONE wire family and that LML, LMQ, training
//! packs, streams and forensic archives are profiles rather than independent
//! container architectures. The optimum tier produces the same semantics as
//! baseline LML — same root kinds, exact fidelity, the same decoded signal — via
//! a materially different bitstream. Giving it its own `ProfileId` would have
//! implied a different KIND of artifact where there is only a different
//! compressor, and would have re-fragmented the family the contract exists to
//! unify.
//!
//! So the distinction lives in the capability mask, and the property that makes
//! that safe is that the mask is FAIL-CLOSED: a reader which does not advertise
//! the bit must be refused rather than allowed to mis-parse an optimum payload as
//! baseline LML. This test pins that refusal, because a capability bit nothing
//! enforces is a comment.

use abir_bcs::{encode_blob, Bcs2Error, Bcs2View, ResourceBounds, CAP_LML_OPTIMUM_V1};

/// Byte offset of the required-capability mask in the fixed 128-byte envelope.
const REQUIRED_CAPABILITIES_OFFSET: usize = 24;

fn artifact_requiring(capabilities: u64) -> Vec<u8> {
    let mut bytes = encode_blob(
        b"optimum-coded payload",
        "application/octet-stream",
        ResourceBounds::default(),
    )
    .expect("fixture encodes");
    bytes[REQUIRED_CAPABILITIES_OFFSET..REQUIRED_CAPABILITIES_OFFSET + 8]
        .copy_from_slice(&capabilities.to_le_bytes());
    bytes
}

#[test]
fn optimum_capability_is_a_distinct_bit_from_encryption() {
    // Sharing a bit with the crypto registry would silently make an optimum
    // payload look encrypted, and vice versa.
    assert_ne!(CAP_LML_OPTIMUM_V1, abir_bcs::CAP_XCHACHA20_POLY1305);
    assert_eq!(CAP_LML_OPTIMUM_V1.count_ones(), 1, "must be a single bit");
}

#[test]
fn reader_without_the_optimum_bit_is_refused() {
    let bytes = artifact_requiring(CAP_LML_OPTIMUM_V1);

    // A baseline-only reader advertises no capabilities.
    let refused = Bcs2View::parse(&bytes, 0, ResourceBounds::default()).unwrap_err();
    assert_eq!(
        refused,
        Bcs2Error::UnsupportedCapabilities(CAP_LML_OPTIMUM_V1),
        "a baseline reader must refuse an optimum payload, not mis-parse it"
    );

    // Advertising an unrelated capability must not help.
    let still_refused = Bcs2View::parse(
        &bytes,
        abir_bcs::CAP_XCHACHA20_POLY1305,
        ResourceBounds::default(),
    )
    .unwrap_err();
    assert_eq!(
        still_refused,
        Bcs2Error::UnsupportedCapabilities(CAP_LML_OPTIMUM_V1)
    );
}

#[test]
fn reader_advertising_the_optimum_bit_is_admitted() {
    let bytes = artifact_requiring(CAP_LML_OPTIMUM_V1);
    Bcs2View::parse(&bytes, CAP_LML_OPTIMUM_V1, ResourceBounds::default())
        .expect("an optimum-capable reader parses the artifact");

    // Superset of capabilities also admits it — the mask is AND-based, so
    // supporting more than is required is always safe.
    let superset = CAP_LML_OPTIMUM_V1 | abir_bcs::CAP_XCHACHA20_POLY1305;
    Bcs2View::parse(&bytes, superset, ResourceBounds::default())
        .expect("a superset-capable reader parses the artifact");
}

#[test]
fn baseline_artifacts_stay_readable_by_baseline_readers() {
    // The bit is opt-in: adding it to the registry must not make existing
    // baseline LML artifacts suddenly require it.
    let bytes = artifact_requiring(0);
    Bcs2View::parse(&bytes, 0, ResourceBounds::default())
        .expect("baseline artifact still parses with no capabilities");
}

// ─── The real codec-bundle path ───────────────────────────────────────────
//
// The tests above patch a blob's mask by hand, which pins the wire rule but not
// the producer. These pin that an optimum bundle built through the actual
// encoder declares the bit, that a baseline reader is refused, and — the part
// that would otherwise rot silently — that the canonical semantics frame stays
// readable regardless, because a consumer must be able to learn what an artifact
// describes even when it cannot decode the signal.

use abir::ContentId;
use abir_bcs::{
    encode_codec_bundle, CodecBundleInput, CodecBundleView, CodecFidelity, CodecFidelityKind,
    CodecImplementation, CodecParameter, CodecParameterValue, CodecProfile,
};

fn semantics() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("fixtures/valid/canonical-tensor.json"),
    )
    .expect("canonical semantics fixture")
}

/// An LML bundle whose packets are optimum-coded.
///
/// Same profile and same exact fidelity as baseline — only the kernel id and the
/// declared capability differ, which is precisely the claim being made.
fn optimum_bundle(semantics: &[u8], capabilities: u64, kernel_id: &str) -> Vec<u8> {
    encode_codec_bundle(
        CodecBundleInput {
            required_capabilities: capabilities,
            canonical_semantics: semantics,
            fidelity: CodecFidelity {
                bound: None,
                contract_id: ContentId::from_bytes([0x32; 32]),
                kind: CodecFidelityKind::Exact,
                metric: None,
            },
            implementation: CodecImplementation {
                build_id: "optimum-capability-test".into(),
                implementation_id: ContentId::from_bytes([0x31; 32]),
                kernel_id: kernel_id.into(),
            },
            model_provenance: None,
            packets: &[b"optimum packet\x00"],
            parameters: vec![CodecParameter {
                name: "predictor.order".into(),
                value: CodecParameterValue::Integer { value: "8".into() },
            }],
            profile: CodecProfile::LmlLossless,
        },
        ResourceBounds::default(),
    )
    .expect("bundle encodes")
}

#[test]
fn an_optimum_bundle_refuses_a_baseline_reader() {
    let semantics = semantics();
    let bytes = optimum_bundle(
        &semantics,
        CAP_LML_OPTIMUM_V1,
        "org.quitetall.lamquant.lml.optimum-v1",
    );

    assert!(
        CodecBundleView::open(&bytes, ResourceBounds::default()).is_err(),
        "the default open advertises nothing and must refuse an optimum bitstream"
    );
    let view = CodecBundleView::open_with_capabilities(
        &bytes,
        CAP_LML_OPTIMUM_V1,
        ResourceBounds::default(),
    )
    .expect("a reader holding the optimum kernel is admitted");
    assert_eq!(
        view.catalog().profile(),
        CodecProfile::LmlLossless,
        "optimum is the SAME profile as baseline; only the kernel and capability differ"
    );
}

#[test]
fn a_baseline_bundle_still_opens_with_no_capabilities() {
    // The bit is opt-in at the producer too: a baseline bundle must not acquire
    // a requirement merely because the field now exists.
    let semantics = semantics();
    let bytes = optimum_bundle(&semantics, 0, "org.quitetall.lamquant.lml.baseline-v1");
    assert!(CodecBundleView::open(&bytes, ResourceBounds::default()).is_ok());
}

#[test]
fn the_semantics_frame_is_never_capability_gated() {
    // A reader that cannot decode optimum packets must still be able to learn
    // what the artifact describes. Gating the semantics frame would make an
    // undecodable artifact also unidentifiable, which is a worse failure than
    // the one the capability exists to prevent.
    let semantics = semantics();
    let bytes = optimum_bundle(
        &semantics,
        CAP_LML_OPTIMUM_V1,
        "org.quitetall.lamquant.lml.optimum-v1",
    );
    let view = Bcs2View::parse(&bytes, CAP_LML_OPTIMUM_V1, ResourceBounds::default())
        .expect("parses with the bit");

    let gated: Vec<u64> = view
        .frames()
        .iter()
        .map(|frame| frame.required_capabilities())
        .collect();
    assert!(
        gated.contains(&0),
        "the canonical semantics frame must remain ungated"
    );
    assert!(
        gated.contains(&CAP_LML_OPTIMUM_V1),
        "the packet frames must carry the requirement"
    );
}
