use abir::{ContentId, ElementType};
use abir_bcs::{
    encode_semantic_bundle, Bcs2FileError, Bcs2FileIndex, Bcs2View, ProfileId, ResourceBounds,
    SemanticPayloadFrame,
};
use std::io::{Cursor, Read, Seek, SeekFrom};

fn artifact() -> Vec<u8> {
    let elements = [
        ElementType::I8,
        ElementType::I16,
        ElementType::I24,
        ElementType::I32,
        ElementType::I64,
        ElementType::U8,
        ElementType::U16,
        ElementType::U32,
        ElementType::U64,
        ElementType::F16,
        ElementType::F32,
        ElementType::F64,
        ElementType::Bool,
        ElementType::Utf8,
        ElementType::Bytes,
    ];
    let payloads: Vec<Vec<u8>> = elements
        .iter()
        .enumerate()
        .map(|(index, element)| {
            vec![
                u8::try_from(index + 1).unwrap();
                usize::try_from(element.byte_width().unwrap_or(1)).unwrap()
            ]
        })
        .collect();
    let frames: Vec<_> = elements
        .iter()
        .zip(&payloads)
        .map(|(element, payload)| SemanticPayloadFrame::new(*element, payload))
        .collect();
    encode_semantic_bundle(
        ContentId::from_bytes([7; 32]),
        br#"{"schema":"test"}"#,
        ProfileId::TRAINING_BALANCED_V1,
        &frames,
        ResourceBounds::default(),
    )
    .unwrap()
}

#[test]
fn file_index_matches_borrowed_view_and_preserves_exact_frame_locations() {
    let bytes = artifact();
    let borrowed = Bcs2View::parse(&bytes, 0, ResourceBounds::default()).unwrap();
    let mut cursor = Cursor::new(&bytes);
    let indexed = Bcs2FileIndex::open(&mut cursor, 0, ResourceBounds::default()).unwrap();

    assert_eq!(indexed.profile(), borrowed.profile());
    assert_eq!(indexed.root_kind(), borrowed.root_kind());
    assert_eq!(indexed.root_content_id(), borrowed.root_content_id());
    assert_eq!(indexed.semantic_json(), borrowed.semantic_json());
    assert_eq!(indexed.references(), borrowed.references());
    assert_eq!(indexed.artifact_len(), bytes.len() as u64);
    assert_eq!(indexed.frames().len(), borrowed.frames().len());
    for (location, frame) in indexed.frames().iter().zip(borrowed.frames()) {
        let mut actual = vec![0_u8; location.len() as usize];
        cursor.seek(SeekFrom::Start(location.offset())).unwrap();
        cursor.read_exact(&mut actual).unwrap();
        assert_eq!(actual, frame.bytes());
        assert_eq!(location.content_id(), frame.content_id());
        assert_eq!(location.storage_id(), frame.storage_id());
        assert_eq!(location.element(), frame.element());
    }
}

#[test]
fn file_index_rejects_payload_mutation() {
    let mut bytes = artifact();
    let borrowed = Bcs2View::parse(&bytes, 0, ResourceBounds::default()).unwrap();
    let offset = borrowed.frames()[0].bytes().as_ptr() as usize - bytes.as_ptr() as usize;
    bytes[offset] ^= 0x80;

    let error =
        Bcs2FileIndex::open(&mut Cursor::new(bytes), 0, ResourceBounds::default()).unwrap_err();

    assert!(matches!(
        error,
        Bcs2FileError::Bcs2(abir_bcs::Bcs2Error::FrameDigestMismatch)
    ));
}
