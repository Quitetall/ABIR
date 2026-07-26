//! A training row may declare that its frame holds an encoding of its logical
//! array rather than the array itself.
//!
//! A window pack stores each window block-floating-point encoded: a per-channel
//! `f32` scale plus integer mantissas, a third the size of raw `f32` and exactly
//! what a GPU wants to load before dequantising itself. Expressing that without
//! this mechanism required one of two lies — declare the row to be integer
//! mantissas (losing the fact that a window is real-valued, and hiding the
//! scales), or declare it `F32` while storing something else (breaking frame
//! closure).
//!
//! Four properties are pinned here:
//!
//! 1. A snapshot that encodes nothing serialises with no trace of the field, so
//!    every already-sealed snapshot keeps its content id and the evidence signed
//!    over it stays valid.
//! 2. An encoded row closes against its STORED extent, not its logical one.
//! 3. `logical_bytes()` refuses an encoded row rather than handing back
//!    mantissas that a caller would read as amplitudes.
//! 4. The degenerate spellings are refused, so one snapshot has one encoding.

use abir::{payload_content_id, ByteOrder, ContentId, ElementType};
use abir_bcs::{ResourceBounds, CAP_LAMQUANT_BFP_V1};
use abir_training::{
    encode_snapshot, ContentKey, TrainingProfile, TrainingRow, TrainingRowEncoding,
    TrainingSnapshot, TrainingWindowStore,
};

fn key(seed: u8) -> ContentKey {
    ContentKey::new(ContentId::from_bytes([seed; 32]))
}

/// Logical window: 2 channels x 3 samples of `f32`.
const LOGICAL_ELEMENT: ElementType = ElementType::F32;
const CHANNELS: u64 = 2;
const SAMPLES: u64 = 3;
const LOGICAL_BYTES: u64 = CHANNELS * SAMPLES * 4;

/// Stored form: 2 `f32` scales followed by 6 `i16` mantissas, as one opaque
/// byte frame — the pack's on-disk record for one window.
fn stored_record() -> Vec<u8> {
    let mut bytes = Vec::new();
    for scale in [0.5_f32, 0.25_f32] {
        bytes.extend_from_slice(&scale.to_le_bytes());
    }
    for mantissa in [1_i16, -2, 3, -4, 5, -6] {
        bytes.extend_from_slice(&mantissa.to_le_bytes());
    }
    bytes
}

/// The logical array those scales and mantissas dequantise to.
fn logical_window() -> Vec<u8> {
    let mut bytes = Vec::new();
    for (scale, mantissas) in [(0.5_f32, [1_i16, -2, 3]), (0.25_f32, [-4, 5, -6])] {
        for mantissa in mantissas {
            bytes.extend_from_slice(&(f32::from(mantissa) * scale).to_le_bytes());
        }
    }
    bytes
}

fn plain_row(bytes: &[u8]) -> TrainingRow {
    TrainingRow {
        byte_order: ByteOrder::Little,
        encoding: None,
        group: key(3),
        label: key(9),
        logical_bytes: bytes.len() as u64,
        logical_id: key(1),
        payload: ContentKey::new(payload_content_id(ElementType::I16, bytes)),
        element: ElementType::I16,
        shape: vec![(bytes.len() / 2) as u64],
        split: key(8),
    }
}

fn encoded_row(stored: &[u8]) -> TrainingRow {
    TrainingRow {
        byte_order: ByteOrder::Little,
        encoding: Some(TrainingRowEncoding {
            capabilities: CAP_LAMQUANT_BFP_V1,
            stored_element: ElementType::U8,
            stored_bytes: stored.len() as u64,
            stored_payload: ContentKey::new(payload_content_id(ElementType::U8, stored)),
        }),
        group: key(3),
        label: key(9),
        // The row keeps its HONEST logical description.
        logical_bytes: LOGICAL_BYTES,
        logical_id: key(1),
        payload: ContentKey::new(payload_content_id(LOGICAL_ELEMENT, &logical_window())),
        element: LOGICAL_ELEMENT,
        shape: vec![CHANNELS, SAMPLES],
        split: key(8),
    }
}

fn seal(row: TrainingRow) -> TrainingSnapshot {
    TrainingSnapshot::seal(
        vec![key(2)],
        key(4),
        TrainingProfile::Balanced,
        vec![row],
        key(5),
    )
    .expect("snapshot seals")
}

#[test]
fn a_snapshot_that_encodes_nothing_carries_no_trace_of_the_field() {
    // If the field serialised even as null, every sealed snapshot's content id
    // would shift and the signed evidence over them would stop verifying. This
    // is the compatibility claim, asserted rather than assumed.
    let bytes = vec![7_u8; 8];
    let snapshot = seal(plain_row(&bytes));
    let catalog = snapshot.canonical_json().expect("catalog");
    let text = String::from_utf8(catalog).expect("catalog is utf-8");
    assert!(
        !text.contains("encoding"),
        "an unencoded snapshot must serialise exactly as it did before the field existed"
    );
}

#[test]
fn an_encoded_row_closes_against_its_stored_extent() {
    let stored = stored_record();
    let snapshot = seal(encoded_row(&stored));
    let frame =
        abir_bcs::SemanticPayloadFrame::encoded(ElementType::U8, &stored, CAP_LAMQUANT_BFP_V1);
    let artifact = encode_snapshot(&snapshot, &[frame], ResourceBounds::default())
        .expect("encoded snapshot encodes");

    let store = TrainingWindowStore::open_with_capabilities(
        &artifact,
        CAP_LAMQUANT_BFP_V1,
        ResourceBounds::default(),
    )
    .expect("a capable reader opens it");
    let row = store.rows().next().expect("one row");

    // Logical description survives untouched.
    assert_eq!(row.element(), LOGICAL_ELEMENT);
    assert_eq!(row.shape(), &[CHANNELS, SAMPLES]);
    // The frame is the stored record.
    assert_eq!(row.bytes(), stored.as_slice());
    let encoding = row.encoding().expect("encoding recorded");
    assert_eq!(encoding.capabilities, CAP_LAMQUANT_BFP_V1);
    assert_eq!(encoding.stored_bytes, stored.len() as u64);
}

#[test]
fn logical_bytes_refuses_an_encoded_row() {
    // Mantissas are plausible-looking integers. Returning them where a caller
    // expects amplitudes would train a model on numerically wrong data with no
    // error raised anywhere, so the accessor refuses instead.
    let stored = stored_record();
    let snapshot = seal(encoded_row(&stored));
    let frame =
        abir_bcs::SemanticPayloadFrame::encoded(ElementType::U8, &stored, CAP_LAMQUANT_BFP_V1);
    let artifact =
        encode_snapshot(&snapshot, &[frame], ResourceBounds::default()).expect("encodes");
    let store = TrainingWindowStore::open_with_capabilities(
        &artifact,
        CAP_LAMQUANT_BFP_V1,
        ResourceBounds::default(),
    )
    .expect("opens");
    let row = store.rows().next().expect("one row");

    assert_eq!(row.logical_bytes(), None);
    assert!(row.bytes().len() == stored.len());
}

#[test]
fn an_unencoded_row_still_lends_its_logical_array() {
    let bytes = vec![7_u8; 8];
    let snapshot = seal(plain_row(&bytes));
    let frame = abir_bcs::SemanticPayloadFrame::new(ElementType::I16, &bytes);
    let artifact =
        encode_snapshot(&snapshot, &[frame], ResourceBounds::default()).expect("encodes");
    let store = TrainingWindowStore::open(&artifact, ResourceBounds::default()).expect("opens");
    let row = store.rows().next().expect("one row");

    assert_eq!(row.logical_bytes(), Some(bytes.as_slice()));
    assert!(row.encoding().is_none());
}

#[test]
fn a_reader_without_the_capability_is_refused() {
    let stored = stored_record();
    let snapshot = seal(encoded_row(&stored));
    let frame =
        abir_bcs::SemanticPayloadFrame::encoded(ElementType::U8, &stored, CAP_LAMQUANT_BFP_V1);
    let artifact =
        encode_snapshot(&snapshot, &[frame], ResourceBounds::default()).expect("encodes");

    // `open` advertises nothing, so it must refuse outright -- the default a
    // caller gets without asking for the encoding is the safe one.
    assert!(
        TrainingWindowStore::open(&artifact, ResourceBounds::default()).is_err(),
        "the default open must refuse a BFP-encoded snapshot"
    );
}

#[test]
fn degenerate_encodings_are_refused() {
    let stored = stored_record();

    // Requires nothing: that is the spelling for "not encoded".
    let mut row = encoded_row(&stored);
    row.encoding.as_mut().unwrap().capabilities = 0;
    assert!(
        TrainingSnapshot::seal(
            vec![key(2)],
            key(4),
            TrainingProfile::Balanced,
            vec![row],
            key(5)
        )
        .is_err(),
        "a zero-capability encoding must be refused"
    );

    // Stores exactly the logical payload: a statement with no content.
    let mut row = encoded_row(&stored);
    let logical = row.payload;
    row.encoding.as_mut().unwrap().stored_payload = logical;
    assert!(
        TrainingSnapshot::seal(
            vec![key(2)],
            key(4),
            TrainingProfile::Balanced,
            vec![row],
            key(5)
        )
        .is_err(),
        "an encoding whose stored payload IS the logical payload must be refused"
    );
}
