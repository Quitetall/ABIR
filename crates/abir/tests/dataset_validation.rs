use abir::{
    canonical_debug_json, interchange_content_id, parse_canonical_dataset, Atom, AtomTag,
    ByteOrder, Calibration, Channel, ChannelBasis, ChannelBasisConstructionError, ChannelBasisTag,
    ChannelBasisTerm, ChannelBasisVector, ChannelSpec, ChannelTag, Clock, ClockTag, ConceptId,
    ContentId, CoordinateFrame, CoordinateFrameTag, DatasetDraft, DatasetTag, Derivation,
    DerivationTag, ElementType, FailureCode, Layout, ObjectId, PayloadDescriptor, Policy,
    PolicyTag, Presence, Proof, ProofTag, Rational, Recording, RecordingTag, ReferenceKind,
    SemanticRef, SignalBlock, Stream, StreamTag, Table, TableColumn, TemporalTable, TimeAxis,
    TimeSegment, ValidationLimits,
};

fn id<T>(value: u8) -> ObjectId<T> {
    ObjectId::from_bytes([value; 16])
}

fn eeg_dataset_with_basis(basis: Option<ChannelBasis>) -> DatasetDraft {
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
    draft.add_channel_basis(basis.unwrap_or_else(|| {
        ChannelBasis::new(
            basis_id,
            vec![
                ChannelSpec::new(ConceptId::new("eeg:channel/fp1").unwrap()),
                ChannelSpec::new(ConceptId::new("eeg:channel/fp2").unwrap()),
            ],
            ReferenceKind::Common,
        )
    }));
    draft.add_atom(Atom::SignalBlock(SignalBlock::new(
        atom_id,
        Presence::Present,
        Some(payload),
        axis,
        Some(calibration),
    )));
    draft
}

fn eeg_dataset() -> DatasetDraft {
    eeg_dataset_with_basis(None)
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

fn weighted_basis_dataset(
    reference_source: u8,
    reverse_terms: bool,
    coefficient: i128,
) -> abir::AbirDataset {
    let outputs = [id::<ChannelTag>(40), id::<ChannelTag>(41)];
    let vectors = outputs
        .into_iter()
        .map(|output| {
            let mut terms = vec![
                ChannelBasisTerm::new(output, Rational::new(coefficient, 1).unwrap()).unwrap(),
                ChannelBasisTerm::new(
                    id::<ChannelTag>(reference_source),
                    Rational::new(-coefficient, 1).unwrap(),
                )
                .unwrap(),
            ];
            if reverse_terms {
                terms.reverse();
            }
            ChannelBasisVector::new(terms).unwrap()
        })
        .collect();
    let basis = ChannelBasis::new(
        id::<ChannelBasisTag>(5),
        vec![
            ChannelSpec::new(ConceptId::new("eeg:channel/fp1").unwrap()),
            ChannelSpec::new(ConceptId::new("eeg:channel/fp2").unwrap()),
        ],
        ReferenceKind::Common,
    )
    .with_construction(vectors)
    .unwrap();
    let mut draft = eeg_dataset_with_basis(Some(basis));
    draft.add_channel(Channel::new(
        id::<ChannelTag>(40),
        ConceptId::new("eeg:electrode/fp1").unwrap(),
    ));
    draft.add_channel(Channel::new(
        id::<ChannelTag>(41),
        ConceptId::new("eeg:electrode/fp2").unwrap(),
    ));
    draft.add_channel(Channel::new(
        id::<ChannelTag>(reference_source),
        ConceptId::new("eeg:electrode/reference").unwrap(),
    ));
    draft.validate(ValidationLimits::default()).unwrap()
}

#[test]
fn weighted_channel_basis_is_exact_canonical_and_round_trips() {
    let a1 = weighted_basis_dataset(42, false, 1);
    let same_reordered = weighted_basis_dataset(42, true, 1);
    let changed_coefficient = weighted_basis_dataset(42, false, 2);
    let cz = weighted_basis_dataset(43, false, 1);

    assert_eq!(
        interchange_content_id(&a1).unwrap(),
        interchange_content_id(&same_reordered).unwrap()
    );
    assert_ne!(
        interchange_content_id(&a1).unwrap(),
        interchange_content_id(&cz).unwrap()
    );
    assert_ne!(
        interchange_content_id(&a1).unwrap(),
        interchange_content_id(&changed_coefficient).unwrap()
    );
    let canonical = canonical_debug_json(&a1).unwrap();
    let parsed = parse_canonical_dataset(&canonical).unwrap();
    assert_eq!(canonical_debug_json(&parsed).unwrap(), canonical);
    assert_eq!(
        parsed.channel_bases()[0].construction().unwrap()[0].terms()[0]
            .source()
            .to_bytes(),
        id::<ChannelTag>(40).to_bytes()
    );
}

#[test]
fn malformed_weighted_channel_basis_is_unconstructible() {
    let source = id::<ChannelTag>(70);
    assert_eq!(
        ChannelBasisVector::new(Vec::new()),
        Err(ChannelBasisConstructionError::EmptyVector)
    );
    assert_eq!(
        ChannelBasis::new(
            id::<ChannelBasisTag>(89),
            Vec::new(),
            ReferenceKind::Absolute,
        )
        .with_construction(Vec::new()),
        Err(ChannelBasisConstructionError::EmptyConstruction)
    );
    assert_eq!(
        ChannelBasisTerm::new(source, Rational::new(0, 1).unwrap()),
        Err(ChannelBasisConstructionError::ZeroCoefficient)
    );
    let term = ChannelBasisTerm::new(source, Rational::new(1, 1).unwrap()).unwrap();
    assert_eq!(
        ChannelBasisVector::new(vec![term, term]),
        Err(ChannelBasisConstructionError::DuplicateSource)
    );
    let one_vector = ChannelBasisVector::new(vec![ChannelBasisTerm::new(
        id::<ChannelTag>(71),
        Rational::new(1, 1).unwrap(),
    )
    .unwrap()])
    .unwrap();
    assert_eq!(
        ChannelBasis::new(
            id::<ChannelBasisTag>(88),
            vec![
                ChannelSpec::new(ConceptId::new("eeg:channel/fp1").unwrap()),
                ChannelSpec::new(ConceptId::new("eeg:channel/fp2").unwrap()),
            ],
            ReferenceKind::Absolute,
        )
        .with_construction(vec![one_vector]),
        Err(ChannelBasisConstructionError::RowCountMismatch {
            expected: 2,
            actual: 1,
        })
    );
    assert_eq!(
        ChannelBasis::new(
            id::<ChannelBasisTag>(90),
            vec![ChannelSpec::new(ConceptId::new("eeg:channel/fp1").unwrap())],
            ReferenceKind::Unknown,
        )
        .with_construction(vec![ChannelBasisVector::new(vec![ChannelBasisTerm::new(
            id::<ChannelTag>(72),
            Rational::new(1, 1).unwrap(),
        )
        .unwrap(),])
        .unwrap()]),
        Err(ChannelBasisConstructionError::UnknownReference)
    );
}

#[test]
fn weighted_basis_sources_are_distinct_channel_observations() {
    let first = ChannelBasisTerm::new(id::<ChannelTag>(91), Rational::new(1, 1).unwrap()).unwrap();
    let second = ChannelBasisTerm::new(id::<ChannelTag>(92), Rational::new(1, 1).unwrap()).unwrap();

    assert!(ChannelBasisVector::new(vec![first, second]).is_ok());
}

#[test]
fn weighted_basis_rejects_unresolved_source_channel() {
    let basis = ChannelBasis::new(
        id::<ChannelBasisTag>(5),
        vec![ChannelSpec::new(ConceptId::new("eeg:channel/fp1").unwrap())],
        ReferenceKind::Absolute,
    )
    .with_construction(vec![ChannelBasisVector::new(vec![ChannelBasisTerm::new(
        id::<ChannelTag>(99),
        Rational::new(1, 1).unwrap(),
    )
    .unwrap()])
    .unwrap()])
    .unwrap();

    let report = eeg_dataset_with_basis(Some(basis))
        .validate(ValidationLimits::default())
        .expect_err("unresolved weighted-basis source must fail closed");
    let failure = report
        .failures()
        .iter()
        .find(|failure| failure.path() == "channel_bases[0].construction[0][0].source")
        .expect("source-specific failure");
    assert_eq!(failure.failure_code(), FailureCode::DanglingReference);
    assert_eq!(
        failure.related_object(),
        Some(id::<ChannelTag>(99).to_bytes())
    );
}

#[test]
fn metadata_limit_counts_repeated_reference_storage() {
    let recording_id = id::<RecordingTag>(2);
    let stream_id = id::<StreamTag>(3);
    let atom_id = id::<AtomTag>(4);
    let mut draft = DatasetDraft::new(id::<DatasetTag>(1));
    draft.add_recording(Recording::new(recording_id, vec![stream_id]));
    draft.add_stream(Stream::new(
        stream_id,
        recording_id,
        ConceptId::new("abir:modality/eeg").unwrap(),
        vec![atom_id; 1_024],
        None,
        None,
        None,
    ));
    draft.add_atom(Atom::Table(Table::new(
        atom_id,
        Presence::AbsentAtSource,
        None,
        vec![TableColumn::new(
            ConceptId::new("abir:column/value").unwrap(),
            ElementType::I16,
            false,
        )],
    )));

    let report = draft
        .validate(ValidationLimits {
            max_metadata_bytes: 4_096,
            ..ValidationLimits::default()
        })
        .unwrap_err();
    assert!(report.failures().iter().any(|failure| {
        failure.failure_code() == FailureCode::StructuralLimit && failure.path() == "metadata_bytes"
    }));
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
