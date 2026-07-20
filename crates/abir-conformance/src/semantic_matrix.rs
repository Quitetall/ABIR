use abir::*;

fn id<T>(value: u8) -> ObjectId<T> {
    ObjectId::from_bytes([value; 16])
}

fn content(value: u8) -> ContentId {
    ContentId::from_bytes([value; 32])
}

fn concept(value: &str) -> ConceptId {
    ConceptId::new(value).expect("semantic matrix concepts are valid")
}

fn rational(numerator: i128, denominator: i128) -> Rational {
    Rational::new(numerator, denominator).expect("semantic matrix rationals are valid")
}

fn payload(
    value: u8,
    logical_bytes: u64,
    element: ElementType,
    shape: Vec<u64>,
    layout: Layout,
    media_type: Option<&str>,
) -> PayloadDescriptor {
    PayloadDescriptor::new(
        content(value),
        logical_bytes,
        element,
        ByteOrder::Little,
        shape,
        layout,
        Some(concept("future:encoding/custom")),
        media_type.map(str::to_owned),
    )
}

fn identity_transform() -> [ExactNumber; 16] {
    core::array::from_fn(|index| ExactNumber::Integer(if index % 5 == 0 { 1 } else { 0 }))
}

/// A deterministic dataset spanning every semantic-v1 catalog, atom, layout,
/// presence, timing, governance, provenance, and fidelity family.
pub fn semantic_matrix_dataset() -> AbirDataset {
    let recording_id = id::<RecordingTag>(2);
    let stream_id = id::<StreamTag>(3);
    let clock_a = id::<ClockTag>(4);
    let clock_b = id::<ClockTag>(5);
    let frame_a = id::<CoordinateFrameTag>(6);
    let frame_b = id::<CoordinateFrameTag>(7);
    let basis_id = id::<ChannelBasisTag>(8);
    let policy_id = id::<PolicyTag>(9);
    let atom_ids: Vec<_> = (10_u8..27).map(id::<AtomTag>).collect();

    let mut draft = DatasetDraft::new(id::<DatasetTag>(1));
    draft.add_subject(
        Subject::new(id::<SubjectTag>(30), concept("abir:subject/human"))
            .with_source_key(SourceKey::new("bids.subject", "sub-01").unwrap()),
    );
    draft.add_patient(Patient::new(
        id::<PatientTag>(31),
        concept("abir:patient/clinical"),
    ));
    draft.add_session(Session::new(
        id::<SessionTag>(32),
        concept("abir:session/recording"),
    ));
    draft.add_acquisition(Acquisition::new(
        id::<AcquisitionTag>(33),
        concept("abir:acquisition/eeg"),
    ));
    draft.add_device(Device::new(
        id::<DeviceTag>(34),
        concept("abir:device/amplifier"),
    ));
    draft.add_sensor(Sensor::new(
        id::<SensorTag>(35),
        concept("abir:sensor/electrode"),
    ));
    draft.add_channel(Channel::new(
        id::<ChannelTag>(36),
        concept("eeg:channel/fp1"),
    ));
    draft.add_concept_dictionary(ConceptDictionary::new(
        id::<ConceptDictionaryTag>(37),
        concept("abir:dictionary/semantic-v1"),
    ));

    draft.add_clock(Clock::new(
        clock_a,
        concept("abir:clock/device"),
        None,
        rational(0, 1),
        rational(1, 1),
        rational(1, 1_000_000),
    ));
    draft.add_clock(Clock::new(
        clock_b,
        concept("abir:clock/reference"),
        None,
        rational(1, 1000),
        rational(1, 1),
        rational(1, 10_000_000),
    ));
    draft.add_clock_relation(ClockRelation::new(
        id::<ClockRelationTag>(38),
        clock_a,
        clock_b,
        rational(1, 1000),
        rational(1, 1),
        rational(1, 1_000_000),
        concept("abir:clock-relation/measured"),
        rational(0, 1),
        Some(rational(10, 1)),
        content(83),
    ));

    draft.add_coordinate_frame(CoordinateFrame::new(
        frame_a,
        concept("abir:frame/head"),
        None,
        Some(identity_transform()),
        rational(1, 1000),
    ));
    draft.add_coordinate_frame(CoordinateFrame::new(
        frame_b,
        concept("abir:frame/sensor"),
        None,
        Some(identity_transform()),
        rational(1, 10_000),
    ));
    draft.add_frame_transform(FrameTransform::new(
        id::<FrameTransformTag>(39),
        frame_b,
        frame_a,
        identity_transform(),
        rational(1, 1000),
        concept("abir:frame-transform/measured"),
    ));
    draft.add_channel_basis(ChannelBasis::new(
        basis_id,
        vec![
            ChannelSpec::new(concept("eeg:channel/fp1"))
                .with_coordinate_frame(frame_a)
                .with_source_key(SourceKey::new("edf.signal", "Fp1-Ref").unwrap()),
            ChannelSpec::new(concept("eeg:channel/fp2")).with_coordinate_frame(frame_a),
        ],
        ReferenceKind::Differential,
    ));
    draft.add_source_relationship(SourceRelationship::PatientSubject {
        patient_id: id::<PatientTag>(31),
        subject_id: id::<SubjectTag>(30),
    });
    draft.add_source_relationship(SourceRelationship::SessionSubject {
        session_id: id::<SessionTag>(32),
        subject_id: id::<SubjectTag>(30),
    });
    draft.add_source_relationship(SourceRelationship::SessionPatient {
        session_id: id::<SessionTag>(32),
        patient_id: id::<PatientTag>(31),
    });
    draft.add_source_relationship(SourceRelationship::AcquisitionSession {
        acquisition_id: id::<AcquisitionTag>(33),
        session_id: id::<SessionTag>(32),
    });
    draft.add_source_relationship(SourceRelationship::AcquisitionDevice {
        acquisition_id: id::<AcquisitionTag>(33),
        device_id: id::<DeviceTag>(34),
    });
    draft.add_source_relationship(SourceRelationship::DeviceSensor {
        device_id: id::<DeviceTag>(34),
        sensor_id: id::<SensorTag>(35),
    });
    draft.add_source_relationship(SourceRelationship::SensorChannel {
        sensor_id: id::<SensorTag>(35),
        channel_id: id::<ChannelTag>(36),
    });
    draft.add_source_relationship(SourceRelationship::AcquisitionRecording {
        acquisition_id: id::<AcquisitionTag>(33),
        recording_id,
    });
    draft.add_source_relationship(SourceRelationship::ChannelBasisMember {
        channel_id: id::<ChannelTag>(36),
        basis_id,
        position: 0,
    });
    draft.add_policy(Policy::new(
        policy_id,
        None,
        vec![concept("abir:policy/research-only")],
    ));

    draft.add_recording(Recording::new(recording_id, vec![stream_id]));
    draft.add_stream(Stream::new(
        stream_id,
        recording_id,
        concept("future:modality/unknown-biosignal"),
        atom_ids.clone(),
        Some(clock_a),
        Some(basis_id),
        Some(policy_id),
    ));

    draft.add_atom(Atom::SignalBlock(SignalBlock::new(
        atom_ids[0],
        Presence::Present,
        Some(payload(
            50,
            16,
            ElementType::I16,
            vec![2, 4],
            Layout::DenseRowMajor,
            None,
        )),
        TimeAxis::Piecewise(vec![
            TimeSegment::new(rational(0, 1), rational(256, 1), 2).unwrap(),
            TimeSegment::new(rational(1, 1), rational(512, 1), 2).unwrap(),
        ]),
        Some(Calibration::new(rational(1, 10), rational(-2, 1), concept("ucum:uV")).unwrap()),
    )));
    draft.add_atom(Atom::TemporalTable(TemporalTable::new(
        atom_ids[1],
        Presence::Present,
        Some(payload(
            51,
            4,
            ElementType::Bytes,
            vec![2, 1],
            Layout::Ragged {
                rows: 2,
                offsets: content(70),
            },
            None,
        )),
        clock_a,
        concept("abir:record/event"),
        vec![TableColumn::new(
            concept("abir:column/value"),
            ElementType::Bytes,
            false,
        )],
    )));
    draft.add_atom(Atom::Table(Table::new(
        atom_ids[2],
        Presence::Present,
        Some(payload(
            52,
            8,
            ElementType::I16,
            vec![2, 2],
            Layout::SparseCoo {
                nonzero: 1,
                indices: content(71),
            },
            None,
        )),
        vec![
            TableColumn::new(concept("abir:column/time"), ElementType::I16, false),
            TableColumn::new(concept("abir:column/value"), ElementType::I16, true),
        ],
    )));
    draft.add_atom(Atom::Tensor(Tensor::new(
        atom_ids[3],
        Presence::Present,
        Some(payload(
            53,
            8,
            ElementType::I16,
            vec![2, 4],
            Layout::BlockFloatingPoint {
                block_len: 4,
                mantissa_bits: 12,
                scales: content(74),
            },
            None,
        )),
        vec![
            SemanticAxis::new(concept("abir:axis/channel"), 2),
            SemanticAxis::new(concept("abir:axis/sample"), 4),
        ],
    )));
    draft.add_atom(Atom::EncodedBlock(EncodedBlock::new(
        atom_ids[4],
        Presence::Present,
        Some(payload(
            54,
            4,
            ElementType::Bytes,
            vec![4],
            Layout::DenseRowMajor,
            None,
        )),
        DecodedSemantics::new(
            concept("abir:atom/signal-block"),
            ElementType::I16,
            vec![2, 4],
        ),
    )));
    draft.add_atom(Atom::BlobRef(BlobRef::new(
        atom_ids[5],
        Presence::Present,
        Some(payload(
            55,
            3,
            ElementType::Bytes,
            vec![3],
            Layout::DenseRowMajor,
            Some("application/octet-stream"),
        )),
        "application/octet-stream".into(),
        BlobIntegrity::new(concept("abir:integrity/blake3-256"), content(55)),
    )));
    draft.add_atom(Atom::Tensor(Tensor::new(
        atom_ids[6],
        Presence::Present,
        Some(payload(
            56,
            8,
            ElementType::I16,
            vec![2, 2],
            Layout::SparseCsr {
                nonzero: 1,
                indptr: content(72),
                indices: content(73),
            },
            None,
        )),
        vec![
            SemanticAxis::new(concept("abir:axis/row"), 2),
            SemanticAxis::new(concept("abir:axis/column"), 2),
        ],
    )));

    for (index, presence) in [
        Presence::AbsentAtSource,
        Presence::UnknownAtSource,
        Presence::Withheld,
        Presence::Redacted,
        Presence::NotApplicable,
    ]
    .into_iter()
    .enumerate()
    {
        draft.add_atom(Atom::Table(Table::new(
            atom_ids[7 + index],
            presence,
            None,
            vec![TableColumn::new(
                concept("abir:column/presence"),
                ElementType::Bool,
                false,
            )],
        )));
    }

    for (index, content_id, element, logical_bytes, shape, semantic) in [
        (
            12_usize,
            70_u8,
            ElementType::I32,
            12_u64,
            vec![3_u64],
            "abir:axis/ragged-offset",
        ),
        (
            13,
            71,
            ElementType::U32,
            8,
            vec![1, 2],
            "abir:axis/coo-index",
        ),
        (
            14,
            72,
            ElementType::U32,
            12,
            vec![3],
            "abir:axis/csr-indptr",
        ),
        (15, 73, ElementType::U32, 4, vec![1], "abir:axis/csr-index"),
        (16, 74, ElementType::F32, 8, vec![2], "abir:axis/bfp-scale"),
    ] {
        draft.add_atom(Atom::Tensor(Tensor::new(
            atom_ids[index],
            Presence::Present,
            Some(payload(
                content_id,
                logical_bytes,
                element,
                shape.clone(),
                Layout::DenseRowMajor,
                None,
            )),
            shape
                .into_iter()
                .map(|extent| SemanticAxis::new(concept(semantic), extent))
                .collect(),
        )));
    }

    draft.add_event(Event::new(
        id::<EventTag>(40),
        concept("abir:event/stimulus"),
        clock_a,
        rational(1, 2),
        rational(3, 4),
        rational(1, 1000),
    ));
    draft.add_proof(Proof::new(
        id::<ProofTag>(41),
        concept("abir:proof/policy-attestation"),
        SemanticRef::of(policy_id),
        content(80),
    ));
    let derivation_id = id::<DerivationTag>(42);
    let artifact_id = id::<DerivedArtifactTag>(43);
    draft.add_derivation(Derivation::new(
        derivation_id,
        concept("future:operation/derive"),
        vec![SemanticRef::of(atom_ids[0])],
        vec![SemanticRef::of(artifact_id)],
    ));
    draft.add_derived_artifact(DerivedArtifact::new(
        artifact_id,
        content(81),
        derivation_id,
    ));
    draft.add_fidelity(Fidelity::new(
        SemanticRef::of(atom_ids[0]),
        FidelityKind::Exact,
        None,
        None,
    ));
    draft.add_source_capsule(SourceCapsule::new(
        SourceKey::new("nwb.object", "acquisition/eeg").unwrap(),
        content(82),
        Some("application/x-hdf5"),
    ));
    draft.add_observed_execution(
        ExecutionRecord::new(concept("future:operation/validate"), "abir-conformance")
            .with_hardware("host"),
    );

    draft
        .validate(ValidationLimits::default())
        .expect("full semantic matrix must validate")
}
