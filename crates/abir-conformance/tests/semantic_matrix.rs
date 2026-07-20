use abir::*;

fn id<T>(value: u8) -> ObjectId<T> {
    ObjectId::from_bytes([value; 16])
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
        ContentId::from_bytes([value; 32]),
        logical_bytes,
        element,
        ByteOrder::Little,
        shape,
        layout,
        Some(ConceptId::new("future:encoding/custom").unwrap()),
        media_type.map(str::to_owned),
    )
}

#[test]
fn full_semantic_matrix_validates() {
    let recording_id = id::<RecordingTag>(2);
    let stream_id = id::<StreamTag>(3);
    let clock_id = id::<ClockTag>(4);
    let frame_id = id::<CoordinateFrameTag>(5);
    let basis_id = id::<ChannelBasisTag>(6);
    let policy_id = id::<PolicyTag>(7);
    let atom_ids: Vec<_> = (10_u8..17).map(id::<AtomTag>).collect();

    let mut transform = [ExactNumber::Integer(0); 16];
    for diagonal in [0_usize, 5, 10, 15] {
        transform[diagonal] = ExactNumber::Integer(1);
    }

    let mut draft = DatasetDraft::new(id::<DatasetTag>(1));
    draft.add_recording(Recording::new(recording_id, vec![stream_id]));
    draft.add_stream(Stream::new(
        stream_id,
        recording_id,
        ConceptId::new("future:modality/quantum-biosignal").unwrap(),
        atom_ids.clone(),
        Some(clock_id),
        Some(basis_id),
        Some(policy_id),
    ));
    draft.add_clock(Clock::new(
        clock_id,
        ConceptId::new("abir:clock/device").unwrap(),
        None,
        Rational::new(0, 1).unwrap(),
        Rational::new(1, 1).unwrap(),
        Rational::new(1, 1_000_000).unwrap(),
    ));
    draft.add_coordinate_frame(CoordinateFrame::new(
        frame_id,
        ConceptId::new("abir:frame/head").unwrap(),
        None,
        Some(transform),
        Rational::new(1, 1_000).unwrap(),
    ));
    draft.add_channel_basis(ChannelBasis::new(
        basis_id,
        vec![
            ChannelSpec::new(ConceptId::new("eeg:channel/fp1").unwrap())
                .with_coordinate_frame(frame_id),
            ChannelSpec::new(ConceptId::new("eeg:channel/fp2").unwrap())
                .with_coordinate_frame(frame_id),
        ],
        ReferenceKind::Differential,
    ));
    draft.add_policy(Policy::new(
        policy_id,
        None,
        vec![ConceptId::new("future:policy/audited-use").unwrap()],
    ));

    draft.add_atom(Atom::SignalBlock(SignalBlock::new(
        atom_ids[0],
        Presence::Present,
        Some(payload(
            20,
            16,
            ElementType::I16,
            vec![2, 4],
            Layout::DenseRowMajor,
            None,
        )),
        TimeAxis::Regular(
            TimeSegment::new(
                Rational::new(0, 1).unwrap(),
                Rational::new(256, 1).unwrap(),
                4,
            )
            .unwrap(),
        ),
        Some(
            Calibration::new(
                Rational::new(1, 10).unwrap(),
                Rational::new(-2, 1).unwrap(),
                ConceptId::new("ucum:uV").unwrap(),
            )
            .unwrap(),
        ),
    )));
    draft.add_atom(Atom::TemporalTable(TemporalTable::new(
        atom_ids[1],
        Presence::Present,
        Some(payload(
            21,
            8,
            ElementType::Bytes,
            vec![2],
            Layout::Ragged { rows: 2 },
            None,
        )),
    )));
    draft.add_atom(Atom::Table(Table::new(
        atom_ids[2],
        Presence::Present,
        Some(payload(
            22,
            8,
            ElementType::I16,
            vec![2, 2],
            Layout::SparseCoo { nonzero: 1 },
            None,
        )),
    )));
    draft.add_atom(Atom::Tensor(Tensor::new(
        atom_ids[3],
        Presence::Present,
        Some(payload(
            23,
            8,
            ElementType::I16,
            vec![2, 4],
            Layout::BlockFloatingPoint {
                block_len: 4,
                mantissa_bits: 12,
            },
            None,
        )),
    )));
    draft.add_atom(Atom::EncodedBlock(EncodedBlock::new(
        atom_ids[4],
        Presence::Present,
        Some(payload(
            24,
            4,
            ElementType::Bytes,
            vec![4],
            Layout::DenseRowMajor,
            None,
        )),
    )));
    draft.add_atom(Atom::BlobRef(BlobRef::new(
        atom_ids[5],
        Presence::Present,
        Some(payload(
            25,
            3,
            ElementType::Bytes,
            vec![3],
            Layout::DenseRowMajor,
            Some("application/octet-stream"),
        )),
    )));
    draft.add_atom(Atom::Table(Table::new(
        atom_ids[6],
        Presence::Missing,
        None,
    )));

    draft.add_proof(Proof::new(
        id::<ProofTag>(30),
        ConceptId::new("abir:proof/policy-attestation").unwrap(),
        SemanticRef::of(policy_id),
        ContentId::from_bytes([30; 32]),
    ));
    draft.add_derivation(Derivation::new(
        id::<DerivationTag>(31),
        ConceptId::new("future:operation/derive").unwrap(),
        vec![SemanticRef::of(atom_ids[0])],
        vec![SemanticRef::of(atom_ids[3])],
    ));
    draft.add_fidelity(Fidelity::new(
        SemanticRef::of(atom_ids[0]),
        FidelityKind::Exact,
        None,
        None,
    ));
    draft.add_source_capsule(SourceCapsule::new(
        SourceKey::new("nwb.object", "acquisition/eeg").unwrap(),
        ContentId::from_bytes([32; 32]),
        Some("application/x-hdf5"),
    ));
    draft.add_observed_execution(
        ExecutionRecord::new(
            ConceptId::new("future:operation/validate").unwrap(),
            "abir-conformance",
        )
        .with_hardware("host"),
    );

    let dataset = draft.validate(ValidationLimits::default()).unwrap();
    assert_eq!(dataset.atoms().len(), 7);
    assert_eq!(dataset.payload_content_ids().len(), 6);
    assert_eq!(dataset.coordinate_frames().len(), 1);
    assert_eq!(dataset.policies().len(), 1);
}
