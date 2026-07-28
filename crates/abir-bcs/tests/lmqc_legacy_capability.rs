use abir_bcs::{
    encode_blob, Bcs2Error, Bcs2View, ResourceBounds, CAP_LAMQUANT_BFP_V1,
    CAP_LMA_SYNTHETIC_REEMIT, CAP_LML_ARITHMETIC_V1, CAP_LML_LOSSLESS_V1, CAP_LML_OPTIMUM_V1,
    CAP_LMQC_LEGACY_V1, CAP_XCHACHA20_POLY1305, CAP_ZSTD,
};

const REQUIRED_CAPABILITIES_OFFSET: usize = 24;

fn legacy_lmqc_artifact() -> Vec<u8> {
    let mut bytes = encode_blob(
        b"legacy-lmqc-packets",
        "application/octet-stream",
        ResourceBounds::default(),
    )
    .expect("fixture encodes");
    bytes[REQUIRED_CAPABILITIES_OFFSET..REQUIRED_CAPABILITIES_OFFSET + 8]
        .copy_from_slice(&CAP_LMQC_LEGACY_V1.to_le_bytes());
    bytes
}

#[test]
fn legacy_lmqc_owns_one_unaliased_capability_bit() {
    assert_eq!(CAP_LMQC_LEGACY_V1.count_ones(), 1);
    let occupied = CAP_XCHACHA20_POLY1305
        | CAP_LML_OPTIMUM_V1
        | CAP_ZSTD
        | CAP_LML_LOSSLESS_V1
        | CAP_LMA_SYNTHETIC_REEMIT
        | CAP_LAMQUANT_BFP_V1
        | CAP_LML_ARITHMETIC_V1;
    assert_eq!(CAP_LMQC_LEGACY_V1 & occupied, 0);
}

#[test]
fn legacy_lmqc_packets_fail_closed_at_the_envelope() {
    let bytes = legacy_lmqc_artifact();
    assert_eq!(
        Bcs2View::parse(&bytes, 0, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::UnsupportedCapabilities(CAP_LMQC_LEGACY_V1)
    );
    Bcs2View::parse(&bytes, CAP_LMQC_LEGACY_V1, ResourceBounds::default())
        .expect("legacy-LMQC-capable reader is admitted");
}
