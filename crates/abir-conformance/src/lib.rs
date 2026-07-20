//! Cross-language ABIR conformance fixtures and runners.
//!
//! The crate intentionally exposes no placeholder conformance result: a
//! conformance claim must be produced by executing the normative fixtures.

mod semantic_matrix;

pub use semantic_matrix::semantic_matrix_dataset;

use abir::{
    Atom, AtomTag, ByteOrder, Clock, ClockTag, ConceptId, ContentId, DatasetDraft, DatasetTag,
    ElementType, Layout, ObjectId, PayloadDescriptor, Presence, Rational, Recording, RecordingTag,
    SemanticAxis, Stream, StreamTag, Tensor, ValidationLimits,
};

fn id<T>(value: u8) -> ObjectId<T> {
    ObjectId::from_bytes([value; 16])
}

pub fn canonical_sample_dataset() -> abir::AbirDataset {
    let recording_id = id::<RecordingTag>(2);
    let stream_id = id::<StreamTag>(3);
    let atom_id = id::<AtomTag>(4);
    let mut draft = DatasetDraft::new(id::<DatasetTag>(1));
    draft.add_recording(Recording::new(recording_id, vec![stream_id]));
    draft.add_stream(Stream::new(
        stream_id,
        recording_id,
        ConceptId::new("abir:modality/eeg").expect("fixture concept"),
        vec![atom_id],
        Some(id::<ClockTag>(6)),
        None,
        None,
    ));
    draft.add_atom(Atom::Tensor(Tensor::new(
        atom_id,
        Presence::Present,
        Some(PayloadDescriptor::new(
            ContentId::from_bytes([5; 32]),
            8,
            ElementType::I16,
            ByteOrder::Little,
            vec![4],
            Layout::DenseRowMajor,
            Some(ConceptId::new("abir:encoding/raw").expect("fixture encoding")),
            None,
        )),
        vec![SemanticAxis::new(
            ConceptId::new("abir:axis/sample").expect("fixture axis"),
            4,
        )],
    )));
    draft.add_clock(Clock::new(
        id::<ClockTag>(6),
        ConceptId::new("abir:clock/device").expect("fixture clock"),
        None,
        Rational::new(-1, 3).expect("fixture offset"),
        Rational::new(256, 1).expect("fixture rate"),
        Rational::new(1, 1_000_000).expect("fixture uncertainty"),
    ));
    draft
        .validate(ValidationLimits::default())
        .expect("canonical fixture must validate")
}
