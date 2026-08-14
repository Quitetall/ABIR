//! Frames declare the programmed functionality needed to interpret them.
//!
//! This is deliberately NOT a "compressed frame" concept. A frame carries a
//! capability mask naming what a consumer must be able to do to turn its stored
//! bytes into its logical content — decompress, decrypt, run a specific codec
//! kernel, or any future transform. Compression is one instance, not the design.
//!
//! Two properties make it safe, and both are pinned here:
//!
//! 1. The envelope's required mask is the UNION of the frame masks, so a reader
//!    is refused once at the header rather than discovering mid-parse that frame
//!    400 needs something it lacks.
//! 2. A frame may not require anything the envelope failed to declare. That is a
//!    malformed artifact, not a negotiation failure, and it is what stops a
//!    hand-edited index from smuggling a transform past the header check.
//!
//! The mask lives in index-entry bytes `[82..90]`, which were previously
//! enforced-zero. Every reader written before this change therefore rejects an
//! artifact that uses it — forward compatibility that fails closed by
//! construction rather than by convention.

use abir_bcs::{
    encode_blob, Bcs2Error, Bcs2View, ResourceBounds, CAP_LAMQUANT_BFP_V1,
    CAP_LMA_SYNTHETIC_REEMIT, CAP_LML1_LEGACY_MATERIALIZE, CAP_LML_ARITHMETIC_V1,
    CAP_LML_LOSSLESS_V1, CAP_LML_OPTIMUM_V1, CAP_LMQC_LEGACY_V1, CAP_XCHACHA20_POLY1305, CAP_ZSTD,
};

const HEADER_REQUIRED_OFFSET: usize = 24;

fn baseline() -> Vec<u8> {
    encode_blob(
        b"frame payload",
        "application/octet-stream",
        ResourceBounds::default(),
    )
    .expect("fixture encodes")
}

/// Locate the single index entry and patch its declared capability mask.
fn with_frame_capability(bytes: &mut [u8], capabilities: u64) {
    let index_offset = u64::from_le_bytes(bytes[72..80].try_into().unwrap()) as usize;
    // index header is 48 bytes, then one 128-byte entry; the mask sits at [82..90]
    let entry = index_offset + 48;
    bytes[entry + 82..entry + 90].copy_from_slice(&capabilities.to_le_bytes());
}

fn set_header_required(bytes: &mut [u8], capabilities: u64) {
    bytes[HEADER_REQUIRED_OFFSET..HEADER_REQUIRED_OFFSET + 8]
        .copy_from_slice(&capabilities.to_le_bytes());
}

#[test]
fn capability_registry_bits_are_distinct() {
    // Three transforms sharing a bit would make one silently pass as another.
    let bits = [
        CAP_XCHACHA20_POLY1305,
        CAP_ZSTD,
        CAP_LML_OPTIMUM_V1,
        CAP_LML_LOSSLESS_V1,
        CAP_LMA_SYNTHETIC_REEMIT,
        CAP_LAMQUANT_BFP_V1,
        CAP_LML_ARITHMETIC_V1,
        CAP_LMQC_LEGACY_V1,
        CAP_LML1_LEGACY_MATERIALIZE,
    ];
    for (index, bit) in bits.iter().enumerate() {
        assert_eq!(bit.count_ones(), 1, "capability {index} must be one bit");
        for other in &bits[index + 1..] {
            assert_eq!(bit & other, 0, "capability bits must not overlap");
        }
    }
}

#[test]
fn untransformed_frames_stay_readable_by_any_reader() {
    // Adding the field must not make existing artifacts require anything.
    let bytes = baseline();
    let view = Bcs2View::parse(&bytes, 0, ResourceBounds::default()).expect("baseline parses");
    assert_eq!(view.frames()[0].required_capabilities(), 0);
}

#[test]
fn a_transformed_frame_refuses_a_reader_that_lacks_the_capability() {
    let mut bytes = baseline();
    with_frame_capability(&mut bytes, CAP_ZSTD);
    set_header_required(&mut bytes, CAP_ZSTD);

    assert_eq!(
        Bcs2View::parse(&bytes, 0, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::UnsupportedCapabilities(CAP_ZSTD),
        "refusal must happen at the envelope, before any frame is touched"
    );
    assert_eq!(
        Bcs2View::parse(&bytes, CAP_LML_OPTIMUM_V1, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::UnsupportedCapabilities(CAP_ZSTD),
        "an unrelated capability must not admit the artifact"
    );
}

#[test]
fn a_capable_reader_sees_the_declaration() {
    let mut bytes = baseline();
    with_frame_capability(&mut bytes, CAP_ZSTD);
    set_header_required(&mut bytes, CAP_ZSTD);

    let view = Bcs2View::parse(&bytes, CAP_ZSTD, ResourceBounds::default())
        .expect("capable reader parses");
    assert_eq!(view.frames()[0].required_capabilities(), CAP_ZSTD);
}

#[test]
fn a_frame_may_not_require_more_than_the_envelope_declared() {
    // The header says "nothing required" while the frame demands zstd. A reader
    // that trusted only the header would sail past the capability gate and then
    // hand back bytes it cannot interpret, so this must be rejected outright.
    let mut bytes = baseline();
    with_frame_capability(&mut bytes, CAP_ZSTD);
    // header required deliberately left at zero

    assert_eq!(
        Bcs2View::parse(&bytes, CAP_ZSTD, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::CatalogCorrupt,
        "an undeclared frame requirement is a malformed artifact"
    );
}

#[test]
fn trailing_reserved_bytes_are_still_enforced_zero() {
    // The field claimed [82..90]; [90..96] must remain a closed extension point,
    // or the next field added here would have no fail-closed guarantee.
    let mut bytes = baseline();
    let index_offset = u64::from_le_bytes(bytes[72..80].try_into().unwrap()) as usize;
    let entry = index_offset + 48;
    bytes[entry + 90] = 1;

    assert_eq!(
        Bcs2View::parse(&bytes, 0, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::CatalogCorrupt
    );
}
