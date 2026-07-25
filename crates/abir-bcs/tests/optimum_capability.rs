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
