use abir::{
    Atom, AtomTag, ByteOrder, Calibration, ChannelBasis, ChannelBasisTag, ChannelSpec, Clock,
    ClockTag, ConceptId, ContentId, CoordinateFrame, CoordinateFrameTag, DatasetDraft, DatasetTag,
    Derivation, DerivationTag, ElementType, FailureCode, Layout, ObjectId, PayloadDescriptor,
    Policy, PolicyTag, Presence, Proof, ProofTag, Rational, Recording, RecordingTag, ReferenceKind,
    SemanticRef, SignalBlock, Stream, StreamTag, Table, TableColumn, TemporalTable, TimeAxis,
    TimeSegment, ValidationLimits,
};

fn id<T>(value: u8) -> ObjectId<T> {
    ObjectId::from_bytes([value; 16])
}

fn eeg_dataset() -> DatasetDraft {
    let recording_id = id::<RecordingTag>(2);
    let stream_id = id::<StreamTag>(3);
    let clock_id = id::<ClockTag>(4);
    let basis_id = id::<ChannelBasisTag>(5);
    let atom_id = id::<AtomTag>(6);

    let payload = PayloadDescriptor::new(
        ContentId::from_bytes([9; 32]),
        24,
        ElementType::I16,
        ByteOrder::Little,
        vec![2, 6],
        Layout::DenseRowMajor,
        Some(ConceptId::new("abir:encoding/raw").unwrap()),
        None,
    );
    let axis = TimeAxis::Piecewise(vec![
        TimeSegment::new(
            Rational::new(0, 1).unwrap(),
            Rational::new(256, 1).unwrap(),
            4,
        )
        .unwrap(),
        TimeSegment::new(
            Rational::new(1, 1).unwrap(),
            Rational::new(128, 1).unwrap(),
            2,
        )
        .unwrap(),
    ]);
    let calibration = Calibration::new(
        Rational::new(1, 10).unwrap(),
        Rational::new(0, 1).unwrap(),
        ConceptId::new("ucum:uV").unwrap(),
    )
    .unwrap();

    let mut draft = DatasetDraft::new(id::<DatasetTag>(1));
    draft.add_recording(Recording::new(recording_id, vec![stream_id]));
    draft.add_stream(Stream::new(
        stream_id,
        recording_id,
        ConceptId::new("abir:modality/eeg").unwrap(),
        vec![atom_id],
        Some(clock_id),
        Some(basis_id),
        None,
    ));
    draft.add_clock(Clock::new(
        clock_id,
        ConceptId::new("abir:clock/device").unwrap(),
        None,
        Rational::new(0, 1).unwrap(),
        Rational::new(1, 1).unwrap(),
        Rational::new(1, 1_000_000).unwrap(),
    ));
    draft.add_channel_basis(ChannelBasis::new(
        basis_id,
        vec![
            ChannelSpec::new(ConceptId::new("eeg:channel/fp1").unwrap()),
            ChannelSpec::new(ConceptId::new("eeg:channel/fp2").unwrap()),
        ],
        ReferenceKind::Common,
    ));
    draft.add_atom(Atom::SignalBlock(SignalBlock::new(
        atom_id,
        Presence::Present,
        Some(payload),
        axis,
        Some(calibration),
    )));
    draft
}

#[test]
fn valid_mixed_rate_dataset_becomes_immutable_root() {
    let dataset = eeg_dataset().validate(ValidationLimits::default()).unwrap();
    assert_eq!(dataset.recordings().len(), 1);
    assert_eq!(dataset.streams().len(), 1);
    assert_eq!(dataset.atoms().len(), 1);
    assert_eq!(
        dataset.payload_content_ids(),
        vec![ContentId::from_bytes([9; 32])]
    );
}

#[test]
fn invalid_time_segments_are_unconstructible() {
    assert!(TimeSegment::new(
        Rational::new(0, 1).unwrap(),
        Rational::new(-1, 1).unwrap(),
        1,
    )
    .is_err());
    assert!(TimeSegment::new(
        Rational::new(0, 1).unwrap(),
        Rational::new(1, 1).unwrap(),
        0,
    )
    .is_err());
}

#[test]
fn dangling_and_duplicate_ids_fail_closed() {
    let mut draft = eeg_dataset();
    let recording = draft.recordings()[0].clone();
    draft.add_recording(recording);
    let report = draft.validate(ValidationLimits::default()).unwrap_err();
    assert!(report
        .failures()
        .iter()
        .any(|failure| failure.failure_code() == FailureCode::DuplicateId));

    let mut dangling = eeg_dataset();
    dangling.streams_mut()[0].set_clock_id(Some(id::<ClockTag>(99)));
    let report = dangling.validate(ValidationLimits::default()).unwrap_err();
    assert!(report
        .failures()
        .iter()
        .any(|failure| failure.failure_code() == FailureCode::UnresolvedClock));
}

#[test]
fn payload_shape_mismatch_and_presence_mismatch_are_rejected() {
    let mut mismatch = eeg_dataset();
    mismatch.atoms_mut()[0]
        .payload_mut()
        .unwrap()
        .set_logical_bytes(23);
    let report = mismatch.validate(ValidationLimits::default()).unwrap_err();
    assert!(report
        .failures()
        .iter()
        .any(|failure| failure.failure_code() == FailureCode::PayloadMismatch));

    let mut absent = eeg_dataset();
    absent.atoms_mut()[0].set_presence(Presence::AbsentAtSource);
    let report = absent.validate(ValidationLimits::default()).unwrap_err();
    assert!(report
        .failures()
        .iter()
        .any(|failure| failure.failure_code() == FailureCode::PayloadMismatch));
}

#[test]
fn composite_payloads_and_temporal_tables_reject_dangling_semantics() {
    let mut composite = DatasetDraft::new(id::<DatasetTag>(70));
    composite.add_atom(Atom::Table(Table::new(
        id::<AtomTag>(71),
        Presence::Present,
        Some(PayloadDescriptor::new(
            ContentId::from_bytes([71; 32]),
            2,
            ElementType::I16,
            ByteOrder::Little,
            vec![1, 1],
            Layout::SparseCoo {
                nonzero: 1,
                indices: ContentId::from_bytes([72; 32]),
            },
            None,
            None,
        )),
        vec![TableColumn::new(
            ConceptId::new("abir:column/value").unwrap(),
            ElementType::I16,
            false,
        )],
    )));
    let report = composite.validate(ValidationLimits::default()).unwrap_err();
    assert!(report.failures().iter().any(|failure| {
        failure.failure_code() == FailureCode::DanglingReference
            && failure.path() == "atoms[0].payload.companion"
    }));

    let mut temporal = DatasetDraft::new(id::<DatasetTag>(73));
    temporal.add_atom(Atom::TemporalTable(TemporalTable::new(
        id::<AtomTag>(74),
        Presence::AbsentAtSource,
        None,
        id::<ClockTag>(75),
        ConceptId::new("abir:record/event").unwrap(),
        vec![TableColumn::new(
            ConceptId::new("abir:column/time").unwrap(),
            ElementType::I64,
            false,
        )],
    )));
    let report = temporal.validate(ValidationLimits::default()).unwrap_err();
    assert!(report.failures().iter().any(|failure| {
        failure.failure_code() == FailureCode::UnresolvedClock
            && failure.path() == "atoms[0].clock_id"
    }));
}

#[test]
fn clock_and_coordinate_cycles_are_rejected() {
    let mut draft = eeg_dataset();
    let clock_a = id::<ClockTag>(30);
    let clock_b = id::<ClockTag>(31);
    draft.add_clock(Clock::new(
        clock_a,
        ConceptId::new("abir:clock/device").unwrap(),
        Some(clock_b),
        Rational::new(0, 1).unwrap(),
        Rational::new(1, 1).unwrap(),
        Rational::new(0, 1).unwrap(),
    ));
    draft.add_clock(Clock::new(
        clock_b,
        ConceptId::new("abir:clock/device").unwrap(),
        Some(clock_a),
        Rational::new(0, 1).unwrap(),
        Rational::new(1, 1).unwrap(),
        Rational::new(0, 1).unwrap(),
    ));

    let frame_a = id::<CoordinateFrameTag>(40);
    let frame_b = id::<CoordinateFrameTag>(41);
    draft.add_coordinate_frame(CoordinateFrame::new(
        frame_a,
        ConceptId::new("abir:frame/head").unwrap(),
        Some(frame_b),
        None,
        Rational::new(0, 1).unwrap(),
    ));
    draft.add_coordinate_frame(CoordinateFrame::new(
        frame_b,
        ConceptId::new("abir:frame/head").unwrap(),
        Some(frame_a),
        None,
        Rational::new(0, 1).unwrap(),
    ));

    let report = draft.validate(ValidationLimits::default()).unwrap_err();
    assert!(report
        .failures()
        .iter()
        .any(|failure| failure.failure_code() == FailureCode::UnresolvedClock));
    assert!(report
        .failures()
        .iter()
        .any(|failure| { failure.failure_code() == FailureCode::UnresolvedCoordinateFrame }));
}

#[test]
fn policy_relaxation_proof_misuse_and_dangling_derivation_fail() {
    let mut draft = eeg_dataset();
    let parent_id = id::<PolicyTag>(50);
    let child_id = id::<PolicyTag>(51);
    draft.add_policy(Policy::new(
        parent_id,
        None,
        vec![ConceptId::new("abir:policy/research-only").unwrap()],
    ));
    draft.add_policy(Policy::new(child_id, Some(parent_id), vec![]));

    draft.add_proof(Proof::new(
        id::<ProofTag>(52),
        ConceptId::new("abir:proof/policy-attestation").unwrap(),
        SemanticRef::of(id::<AtomTag>(6)),
        ContentId::from_bytes([52; 32]),
    ));
    draft.add_derivation(Derivation::new(
        id::<DerivationTag>(53),
        ConceptId::new("abir:operation/filter").unwrap(),
        vec![SemanticRef::of(id::<AtomTag>(99))],
        vec![SemanticRef::of(id::<AtomTag>(6))],
    ));

    let report = draft.validate(ValidationLimits::default()).unwrap_err();
    assert!(report
        .failures()
        .iter()
        .any(|failure| failure.failure_code() == FailureCode::PolicyRelaxation));
    assert!(report
        .failures()
        .iter()
        .any(|failure| failure.failure_code() == FailureCode::ProofMisuse));
    assert!(report
        .failures()
        .iter()
        .any(|failure| failure.failure_code() == FailureCode::DanglingReference));
}
