use abir::{
    Acquisition, AcquisitionTag, Channel, ChannelTag, Clock, ClockRelation, ClockRelationTag,
    ClockTag, ConceptDictionary, ConceptDictionaryTag, ConceptId, ContentId, CoordinateFrame,
    CoordinateFrameTag, DatasetDraft, DatasetTag, Derivation, DerivationTag, DerivedArtifact,
    DerivedArtifactTag, Device, DeviceTag, Event, EventTag, ExactNumber, FailureCode,
    FrameTransform, FrameTransformTag, ObjectId, ObjectKind, Patient, PatientTag, Rational,
    SemanticRef, Sensor, SensorTag, Session, SessionTag, SourceKey, Subject, SubjectTag,
    ValidationLimits,
};

fn id<T>(byte: u8) -> ObjectId<T> {
    ObjectId::from_bytes([byte; 16])
}

fn concept(local: &str) -> ConceptId {
    ConceptId::new(format!("test:{local}")).unwrap()
}

fn rational(numerator: i128, denominator: i128) -> Rational {
    Rational::new(numerator, denominator).unwrap()
}

fn clock(id: ObjectId<ClockTag>) -> Clock {
    Clock::new(
        id,
        concept("clock"),
        None,
        rational(0, 1),
        rational(1, 1),
        rational(0, 1),
    )
}

fn frame(id: ObjectId<CoordinateFrameTag>) -> CoordinateFrame {
    CoordinateFrame::new(id, concept("frame"), None, None, rational(0, 1))
}

fn identity_transform() -> [ExactNumber; 16] {
    core::array::from_fn(|index| ExactNumber::Integer(if index % 5 == 0 { 1 } else { 0 }))
}

#[test]
fn typed_catalog_records_validate_and_are_retrievable() {
    let mut draft = DatasetDraft::new(id::<DatasetTag>(1));
    let source = SourceKey::new("test", "foreign-id").unwrap();

    draft
        .add_subject(Subject::new(id::<SubjectTag>(2), concept("subject")).with_source_key(source));
    draft.add_patient(Patient::new(id::<PatientTag>(3), concept("patient")));
    draft.add_session(Session::new(id::<SessionTag>(4), concept("session")));
    draft.add_acquisition(Acquisition::new(
        id::<AcquisitionTag>(5),
        concept("acquisition"),
    ));
    draft.add_device(Device::new(id::<DeviceTag>(6), concept("device")));
    draft.add_sensor(Sensor::new(id::<SensorTag>(7), concept("sensor")));
    draft.add_channel(Channel::new(id::<ChannelTag>(8), concept("channel")));
    draft.add_concept_dictionary(ConceptDictionary::new(
        id::<ConceptDictionaryTag>(9),
        concept("dictionary"),
    ));

    let clock_a = id::<ClockTag>(10);
    let clock_b = id::<ClockTag>(11);
    draft.add_clock(clock(clock_a));
    draft.add_clock(clock(clock_b));
    let relation_id = id::<ClockRelationTag>(12);
    draft.add_clock_relation(ClockRelation::new(
        relation_id,
        clock_a,
        clock_b,
        rational(5, 1000),
        rational(1, 1),
        rational(1, 1_000_000),
        concept("clock-method"),
    ));

    let frame_a = id::<CoordinateFrameTag>(13);
    let frame_b = id::<CoordinateFrameTag>(14);
    draft.add_coordinate_frame(frame(frame_a));
    draft.add_coordinate_frame(frame(frame_b));
    let transform_id = id::<FrameTransformTag>(15);
    draft.add_frame_transform(FrameTransform::new(
        transform_id,
        frame_a,
        frame_b,
        identity_transform(),
        rational(1, 10_000),
        concept("frame-method"),
    ));

    let event_id = id::<EventTag>(16);
    draft.add_event(Event::new(
        event_id,
        concept("event"),
        clock_a,
        rational(1, 2),
        rational(3, 4),
        rational(1, 1000),
    ));

    let derivation_id = id::<DerivationTag>(17);
    let artifact_id = id::<DerivedArtifactTag>(18);
    draft.add_derivation(Derivation::new(
        derivation_id,
        concept("derive"),
        Vec::new(),
        vec![SemanticRef::of(artifact_id)],
    ));
    draft.add_derived_artifact(DerivedArtifact::new(
        artifact_id,
        ContentId::from_bytes([19; 32]),
        derivation_id,
    ));

    assert!(draft.subject(id::<SubjectTag>(2)).is_some());
    let dataset = draft.validate(ValidationLimits::default()).unwrap();

    assert_eq!(dataset.subjects().len(), 1);
    assert_eq!(
        dataset
            .subject(id::<SubjectTag>(2))
            .unwrap()
            .source_keys()
            .len(),
        1
    );
    assert!(dataset.patient(id::<PatientTag>(3)).is_some());
    assert!(dataset.session(id::<SessionTag>(4)).is_some());
    assert!(dataset.acquisition(id::<AcquisitionTag>(5)).is_some());
    assert!(dataset.device(id::<DeviceTag>(6)).is_some());
    assert!(dataset.sensor(id::<SensorTag>(7)).is_some());
    assert!(dataset.channel(id::<ChannelTag>(8)).is_some());
    assert!(dataset
        .concept_dictionary(id::<ConceptDictionaryTag>(9))
        .is_some());
    assert_eq!(
        dataset.clock_relation(relation_id).unwrap().from_clock_id(),
        clock_a
    );
    assert_eq!(
        dataset.frame_transform(transform_id).unwrap().transform(),
        &identity_transform()
    );
    assert_eq!(dataset.event(event_id).unwrap().clock_id(), clock_a);
    assert_eq!(
        dataset
            .derived_artifact(artifact_id)
            .unwrap()
            .derivation_id(),
        derivation_id
    );

    assert_eq!(
        SemanticRef::of(id::<SubjectTag>(2)).kind(),
        ObjectKind::Subject
    );
    assert_eq!(
        SemanticRef::of(relation_id).kind(),
        ObjectKind::ClockRelation
    );
    assert_eq!(
        SemanticRef::of(transform_id).kind(),
        ObjectKind::FrameTransform
    );
    assert_eq!(SemanticRef::of(event_id).kind(), ObjectKind::Event);
    assert_eq!(
        SemanticRef::of(artifact_id).kind(),
        ObjectKind::DerivedArtifact
    );
}

#[test]
fn duplicate_catalog_identity_fails_closed() {
    let mut draft = DatasetDraft::new(id::<DatasetTag>(1));
    let duplicate = id::<SubjectTag>(2);
    draft.add_subject(Subject::new(duplicate, concept("subject-a")));
    draft.add_subject(Subject::new(duplicate, concept("subject-b")));

    let report = draft.validate(ValidationLimits::default()).unwrap_err();
    assert!(report.failures().iter().any(|failure| {
        failure.failure_code() == FailureCode::DuplicateId && failure.path() == "subjects"
    }));
}

#[test]
fn every_specialized_catalog_reference_fails_closed_when_dangling() {
    let mut clock_draft = DatasetDraft::new(id::<DatasetTag>(1));
    let present_clock = id::<ClockTag>(2);
    clock_draft.add_clock(clock(present_clock));
    clock_draft.add_clock_relation(ClockRelation::new(
        id::<ClockRelationTag>(3),
        present_clock,
        id::<ClockTag>(4),
        rational(0, 1),
        rational(1, 1),
        rational(0, 1),
        concept("method"),
    ));
    assert_failure(clock_draft, FailureCode::UnresolvedClock, "to_clock_id");

    let mut frame_draft = DatasetDraft::new(id::<DatasetTag>(5));
    let present_frame = id::<CoordinateFrameTag>(6);
    frame_draft.add_coordinate_frame(frame(present_frame));
    frame_draft.add_frame_transform(FrameTransform::new(
        id::<FrameTransformTag>(7),
        present_frame,
        id::<CoordinateFrameTag>(8),
        identity_transform(),
        rational(0, 1),
        concept("method"),
    ));
    assert_failure(
        frame_draft,
        FailureCode::UnresolvedCoordinateFrame,
        "to_frame_id",
    );

    let mut event_draft = DatasetDraft::new(id::<DatasetTag>(9));
    event_draft.add_event(Event::new(
        id::<EventTag>(10),
        concept("event"),
        id::<ClockTag>(11),
        rational(0, 1),
        rational(1, 1),
        rational(0, 1),
    ));
    assert_failure(event_draft, FailureCode::UnresolvedClock, "clock_id");

    let mut artifact_draft = DatasetDraft::new(id::<DatasetTag>(12));
    artifact_draft.add_derived_artifact(DerivedArtifact::new(
        id::<DerivedArtifactTag>(13),
        ContentId::from_bytes([14; 32]),
        id::<DerivationTag>(15),
    ));
    assert_failure(
        artifact_draft,
        FailureCode::DanglingReference,
        "derivation_id",
    );
}

fn assert_failure(draft: DatasetDraft, code: FailureCode, path_suffix: &str) {
    let report = draft.validate(ValidationLimits::default()).unwrap_err();
    assert!(report.failures().iter().any(|failure| {
        failure.failure_code() == code && failure.path().ends_with(path_suffix)
    }));
}

#[test]
fn invalid_intervals_uncertainty_and_rates_are_rejected() {
    let clock_id = id::<ClockTag>(2);
    let mut draft = DatasetDraft::new(id::<DatasetTag>(1));
    draft.add_clock(clock(clock_id));
    draft.add_clock_relation(ClockRelation::new(
        id::<ClockRelationTag>(3),
        clock_id,
        clock_id,
        rational(0, 1),
        rational(0, 1),
        rational(-1, 1),
        concept("method"),
    ));
    draft.add_event(Event::new(
        id::<EventTag>(4),
        concept("event"),
        clock_id,
        rational(2, 1),
        rational(1, 1),
        rational(-1, 1),
    ));

    let report = draft.validate(ValidationLimits::default()).unwrap_err();
    assert!(
        report
            .failures()
            .iter()
            .filter(|failure| failure.failure_code() == FailureCode::InvalidExactNumber)
            .count()
            >= 2
    );
}
