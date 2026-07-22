use abir::{
    payload_content_id, Atom, AtomTag, BorrowedPayload, BorrowedPayloadAccess, ByteOrder,
    ConceptId, DatasetDraft, DatasetTag, ElementType, Layout, ObjectId, PayloadDescriptor,
    Presence, Recording, RecordingTag, SemanticAxis, Stream, StreamTag, Tensor, ValidationLimits,
};
use abir_bcs::{
    encode_dataset_with_payloads, Bcs2Error, Bcs2View, FrameKind, ProfileId, ResourceBounds,
};

fn id<T>(value: u8) -> ObjectId<T> {
    ObjectId::from_bytes([value; 16])
}

fn fixture() -> (abir::AbirDataset, [u8; 8], abir::ContentId) {
    let bytes = [0_u8, 1, 2, 3, 4, 5, 6, 7];
    let content_id = payload_content_id(ElementType::I16, &bytes);
    let recording_id = id::<RecordingTag>(2);
    let stream_id = id::<StreamTag>(3);
    let atom_id = id::<AtomTag>(4);
    let descriptor = PayloadDescriptor::new(
        content_id,
        bytes.len() as u64,
        ElementType::I16,
        ByteOrder::Little,
        vec![4],
        Layout::DenseRowMajor,
        Some(ConceptId::new("abir:encoding/raw").unwrap()),
        None,
    );
    let mut draft = DatasetDraft::new(id::<DatasetTag>(1));
    draft.add_recording(Recording::new(recording_id, vec![stream_id]));
    draft.add_stream(Stream::new(
        stream_id,
        recording_id,
        ConceptId::new("abir:modality/eeg").unwrap(),
        vec![atom_id],
        None,
        None,
        None,
    ));
    draft.add_atom(Atom::Tensor(Tensor::new(
        atom_id,
        Presence::Present,
        Some(descriptor),
        vec![SemanticAxis::new(
            ConceptId::new("abir:axis/value").unwrap(),
            4,
        )],
    )));
    (
        draft.validate(ValidationLimits::default()).unwrap(),
        bytes,
        content_id,
    )
}

#[test]
fn dataset_payload_frames_are_typed_deterministic_and_borrowed() {
    let (dataset, bytes, content_id) = fixture();
    let payloads = [BorrowedPayload::new(content_id, &bytes)];
    let access = BorrowedPayloadAccess::new(&payloads);
    let first = encode_dataset_with_payloads(
        &dataset,
        &access,
        ProfileId::TRAINING_COMPACT_V1,
        ResourceBounds::default(),
    )
    .unwrap();
    let second = encode_dataset_with_payloads(
        &dataset,
        &access,
        ProfileId::TRAINING_COMPACT_V1,
        ResourceBounds::default(),
    )
    .unwrap();
    assert_eq!(first, second);

    let view = Bcs2View::parse(&first, 0, ResourceBounds::default()).unwrap();
    assert_eq!(view.frames().len(), 1);
    let frame = view.frames()[0];
    assert_eq!(frame.kind(), FrameKind::SemanticPayload);
    assert_eq!(frame.element(), Some(ElementType::I16));
    assert_eq!(frame.content_id(), content_id);
    assert_eq!(frame.bytes(), bytes);
    let artifact = first.as_ptr() as usize..first.as_ptr() as usize + first.len();
    assert!(artifact.contains(&(frame.bytes().as_ptr() as usize)));
}

#[test]
fn payload_frames_fail_closed_on_missing_and_relabelled_content() {
    let (dataset, bytes, content_id) = fixture();
    let empty: [BorrowedPayload<'_>; 0] = [];
    assert_eq!(
        encode_dataset_with_payloads(
            &dataset,
            &BorrowedPayloadAccess::new(&empty),
            ProfileId::TRAINING_COMPACT_V1,
            ResourceBounds::default(),
        ),
        Err(Bcs2Error::MissingPayload(content_id))
    );

    let payloads = [BorrowedPayload::new(content_id, &bytes)];
    let mut encoded = encode_dataset_with_payloads(
        &dataset,
        &BorrowedPayloadAccess::new(&payloads),
        ProfileId::TRAINING_COMPACT_V1,
        ResourceBounds::default(),
    )
    .unwrap();
    let index_offset = u64::from_le_bytes(encoded[72..80].try_into().unwrap()) as usize;
    encoded[index_offset + 48 + 81] = 4;
    assert_eq!(
        Bcs2View::parse(&encoded, 0, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::FrameIdentityMismatch
    );
}
