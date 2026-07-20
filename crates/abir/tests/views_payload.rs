use abir::{
    Atom, AtomTag, BorrowedPayload, BorrowedPayloadAccess, ByteOrder, ConceptId, ContentId,
    DatasetDraft, DatasetTag, ElementType, InMemoryPayloadAccess, Layout, ObjectId, OpenedDataset,
    PayloadDescriptor, Presence, Recording, RecordingTag, SemanticAxis, Stream, StreamTag, Tensor,
    ValidationLimits,
};

fn id<T>(value: u8) -> ObjectId<T> {
    ObjectId::from_bytes([value; 16])
}

fn tensor_dataset() -> (abir::AbirDataset, ContentId) {
    let recording_id = id::<RecordingTag>(2);
    let stream_id = id::<StreamTag>(3);
    let atom_id = id::<AtomTag>(4);
    let content_id = ContentId::from_bytes([5; 32]);
    let descriptor = PayloadDescriptor::new(
        content_id,
        8,
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
        ConceptId::new("future:modality/custom").unwrap(),
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
        content_id,
    )
}

#[test]
fn borrowed_tensor_view_preserves_pointer_identity() {
    let (dataset, content_id) = tensor_dataset();
    let bytes = [0_u8, 1, 2, 3, 4, 5, 6, 7];
    let payloads = [BorrowedPayload::new(content_id, &bytes)];
    let opened = OpenedDataset::new(dataset, BorrowedPayloadAccess::new(&payloads));

    let tensor = opened.tensor_view(id::<AtomTag>(4)).unwrap();
    assert_eq!(tensor.bytes().as_ptr(), bytes.as_ptr());
    assert_eq!(tensor.bytes(), bytes.as_slice());
    assert_eq!(tensor.dataset().id(), id::<DatasetTag>(1));
    assert_eq!(
        opened
            .recording_view(id::<RecordingTag>(2))
            .unwrap()
            .dataset()
            .id(),
        id::<DatasetTag>(1)
    );
    assert_eq!(
        opened
            .stream_view(id::<StreamTag>(3))
            .unwrap()
            .stream()
            .modality()
            .as_str(),
        "future:modality/custom"
    );
}

#[test]
fn in_memory_adapter_moves_buffers_without_view_copy() {
    let (dataset, content_id) = tensor_dataset();
    let bytes = vec![8_u8; 8];
    let pointer_before_move = bytes.as_ptr();
    let mut access = InMemoryPayloadAccess::new();
    assert!(access.insert(content_id, bytes).is_none());
    let opened = OpenedDataset::new(dataset, access);

    let view = opened.block_view(id::<AtomTag>(4)).unwrap();
    assert_eq!(view.bytes().as_ptr(), pointer_before_move);
    assert_eq!(view.descriptor().content_id(), content_id);
}

#[test]
fn payload_length_mismatch_is_reported_without_copying() {
    let (dataset, content_id) = tensor_dataset();
    let bytes = [0_u8; 7];
    let payloads = [BorrowedPayload::new(content_id, &bytes)];
    let opened = OpenedDataset::new(dataset, BorrowedPayloadAccess::new(&payloads));
    let error = opened.block_view(id::<AtomTag>(4)).unwrap_err();
    assert!(matches!(
        error,
        abir::PayloadAccessError::LengthMismatch {
            expected: 8,
            actual: 7
        }
    ));
}
