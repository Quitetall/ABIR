#![no_main]

use abir::{
    Atom, AtomTag, ByteOrder, ConceptId, ContentId, DatasetDraft, DatasetTag, ElementType, Layout,
    ObjectId, PayloadDescriptor, Presence, Recording, RecordingTag, SemanticAxis, Stream,
    StreamTag, Tensor, ValidationLimits,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let byte = |index: usize| data.get(index).copied().unwrap_or(0);
    let dataset_id = ObjectId::<DatasetTag>::from_bytes([byte(0); 16]);
    let recording_id = ObjectId::<RecordingTag>::from_bytes([byte(1); 16]);
    let stream_id = ObjectId::<StreamTag>::from_bytes([byte(2); 16]);
    let atom_id = ObjectId::<AtomTag>::from_bytes([byte(3); 16]);
    let shape = vec![u64::from(byte(4)), u64::from(byte(5))];
    let axes = vec![
        SemanticAxis::new(
            ConceptId::new("abir:axis/fuzz-0").expect("static concept"),
            shape[0],
        ),
        SemanticAxis::new(
            ConceptId::new("abir:axis/fuzz-1").expect("static concept"),
            shape[1],
        ),
    ];
    let logical_bytes = u64::from(byte(6));
    let mut draft = DatasetDraft::new(dataset_id);
    draft.add_recording(Recording::new(recording_id, vec![stream_id]));
    draft.add_stream(Stream::new(
        stream_id,
        recording_id,
        ConceptId::new("future:modality/fuzz").expect("static fuzz modality is canonical"),
        vec![atom_id],
        None,
        None,
        None,
    ));
    draft.add_atom(Atom::Tensor(Tensor::new(
        atom_id,
        Presence::Present,
        Some(PayloadDescriptor::new(
            ContentId::from_bytes([byte(7); 32]),
            logical_bytes,
            ElementType::I16,
            ByteOrder::Little,
            shape,
            Layout::DenseRowMajor,
            None,
            None,
        )),
        axes,
    )));
    let _ = draft.validate(ValidationLimits {
        max_rank: usize::from(byte(8)),
        ..ValidationLimits::default()
    });
});
