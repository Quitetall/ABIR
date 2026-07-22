use abir::{
    payload_content_id, Atom, AtomTag, BorrowedPayload, BorrowedPayloadAccess, ByteOrder,
    ConceptId, DatasetDraft, DatasetTag, ElementType, Layout, ObjectId, OpenedDataset,
    PayloadDescriptor, Presence, Recording, RecordingTag, SemanticAxis, Stream, StreamTag, Tensor,
    ValidationLimits,
};
use abir_bcs::{encode_dataset_with_payloads, Bcs2View, ProfileId, ResourceBounds};
use abir_store::{AbirStore, FsAbirStore};
use std::sync::Arc;

fn id<T>(value: u8) -> ObjectId<T> {
    ObjectId::from_bytes([value; 16])
}

fn fixture() -> (abir::AbirDataset, [u8; 8], abir::ContentId) {
    let bytes = [8_u8, 7, 6, 5, 4, 3, 2, 1];
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
fn in_memory_store_is_zero_copy_payload_access() {
    let (dataset, bytes, content_id) = fixture();
    let inputs = [BorrowedPayload::new(content_id, &bytes)];
    let encoded = encode_dataset_with_payloads(
        &dataset,
        &BorrowedPayloadAccess::new(&inputs),
        ProfileId::TRAINING_COMPACT_V1,
        ResourceBounds::default(),
    )
    .unwrap();
    let artifact: Arc<[u8]> = Arc::from(encoded);
    let expected_pointer = Bcs2View::parse(&artifact, 0, ResourceBounds::default())
        .unwrap()
        .frames()[0]
        .bytes()
        .as_ptr();
    let mut store = AbirStore::default();
    store
        .insert_bcs2(Arc::clone(&artifact), 0, ResourceBounds::default())
        .unwrap();

    let opened = OpenedDataset::new(dataset, store);
    let tensor = opened.tensor_view(id::<AtomTag>(4)).unwrap();
    assert_eq!(tensor.bytes(), bytes);
    assert_eq!(tensor.bytes().as_ptr(), expected_pointer);
}

#[test]
fn filesystem_store_reopens_as_mmap_payload_access() {
    let (dataset, bytes, content_id) = fixture();
    let inputs = [BorrowedPayload::new(content_id, &bytes)];
    let encoded = encode_dataset_with_payloads(
        &dataset,
        &BorrowedPayloadAccess::new(&inputs),
        ProfileId::TRAINING_COMPACT_V1,
        ResourceBounds::default(),
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut writer = FsAbirStore::open(directory.path(), 0, ResourceBounds::default()).unwrap();
    writer.insert(&encoded).unwrap();
    drop(writer);

    let reader = FsAbirStore::open(directory.path(), 0, ResourceBounds::default()).unwrap();
    let opened = OpenedDataset::new(dataset, reader);
    let tensor = opened.tensor_view(id::<AtomTag>(4)).unwrap();
    assert_eq!(tensor.bytes(), bytes);
}
