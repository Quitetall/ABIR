use abir::{
    canonical_debug_json, logical_content_id, Atom, AtomTag, ByteOrder, Clock, ClockTag, ConceptId,
    ContentId, DatasetDraft, DatasetTag, ElementType, ExecutionRecord, Layout, ObjectId,
    PayloadDescriptor, Presence, Rational, Recording, RecordingTag, Stream, StreamTag, Tensor,
    ValidationLimits,
};

fn id<T>(value: u8) -> ObjectId<T> {
    ObjectId::from_bytes([value; 16])
}

fn dataset(reverse: bool, layout: Layout, observed_execution: bool) -> abir::AbirDataset {
    let mut draft = DatasetDraft::new(id::<DatasetTag>(1));
    let values = if reverse { [20_u8, 10] } else { [10_u8, 20] };
    for value in values {
        let recording_id = id::<RecordingTag>(value);
        let stream_id = id::<StreamTag>(value + 1);
        let atom_id = id::<AtomTag>(value + 2);
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
            Some(PayloadDescriptor::new(
                ContentId::from_bytes([value; 32]),
                8,
                ElementType::I16,
                if reverse {
                    ByteOrder::Big
                } else {
                    ByteOrder::Little
                },
                vec![4],
                layout.clone(),
                Some(ConceptId::new("abir:encoding/raw").unwrap()),
                None,
            )),
        )));
    }
    draft.add_clock(Clock::new(
        id::<ClockTag>(7),
        ConceptId::new("abir:clock/device").unwrap(),
        None,
        Rational::new(-1, 3).unwrap(),
        Rational::new(256, 1).unwrap(),
        Rational::new(1, 1_000_000).unwrap(),
    ));
    if observed_execution {
        draft.add_observed_execution(
            ExecutionRecord::new(
                ConceptId::new("abir:operation/validate").unwrap(),
                "test-runtime",
            )
            .with_hardware("test-cpu"),
        );
    }
    draft.validate(ValidationLimits::default()).unwrap()
}

#[test]
fn logical_identity_ignores_insertion_storage_layout_and_observed_execution() {
    let first = dataset(false, Layout::DenseRowMajor, false);
    let second = dataset(true, Layout::DenseColumnMajor, true);
    assert_eq!(
        logical_content_id(&first).unwrap(),
        logical_content_id(&second).unwrap()
    );
    assert_ne!(
        canonical_debug_json(&first).unwrap(),
        canonical_debug_json(&second).unwrap()
    );
}

#[test]
fn canonical_debug_json_uses_tagged_exact_numbers_and_sorted_catalogs() {
    let dataset = dataset(true, Layout::DenseRowMajor, false);
    let bytes = canonical_debug_json(&dataset).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["semantic_version"], "1");
    assert_eq!(value["clocks"][0]["offset"]["$rational"][0], "-1");
    assert_eq!(
        value["recordings"][0]["id"],
        id::<RecordingTag>(10).to_string()
    );
}
