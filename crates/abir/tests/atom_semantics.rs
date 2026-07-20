use abir::{
    Atom, AtomTag, BlobIntegrity, BlobRef, ByteOrder, Clock, ClockTag, ConceptId, ContentId,
    DatasetDraft, DatasetTag, DecodedSemantics, ElementType, EncodedBlock, Layout, ObjectId,
    PayloadDescriptor, Presence, Rational, SemanticAxis, Table, TableColumn, TemporalTable, Tensor,
    ValidationLimits,
};

fn id<T>(value: u8) -> ObjectId<T> {
    ObjectId::from_bytes([value; 16])
}

fn content(value: u8) -> ContentId {
    ContentId::from_bytes([value; 32])
}

fn payload(content_id: ContentId, shape: Vec<u64>, layout: Layout) -> PayloadDescriptor {
    let logical_bytes = shape.iter().product::<u64>() * 2;
    PayloadDescriptor::new(
        content_id,
        logical_bytes,
        ElementType::I16,
        ByteOrder::Little,
        shape,
        layout,
        Some(ConceptId::new("abir:encoding/raw").unwrap()),
        None,
    )
}

fn validates(atom: Atom) -> bool {
    let mut draft = DatasetDraft::new(id::<DatasetTag>(1));
    if let Atom::TemporalTable(table) = &atom {
        draft.add_clock(Clock::new(
            table.clock_id(),
            ConceptId::new("abir:clock/device").unwrap(),
            None,
            Rational::new(0, 1).unwrap(),
            Rational::new(1, 1).unwrap(),
            Rational::new(0, 1).unwrap(),
        ));
    }
    let companions = atom
        .payload()
        .map(|payload| match payload.layout() {
            Layout::DenseRowMajor | Layout::DenseColumnMajor => Vec::new(),
            Layout::Ragged { offsets, .. } => vec![*offsets],
            Layout::SparseCoo { indices, .. } => vec![*indices],
            Layout::SparseCsr {
                indptr, indices, ..
            } => vec![*indptr, *indices],
            Layout::BlockFloatingPoint { scales, .. } => vec![*scales],
        })
        .unwrap_or_default();
    draft.add_atom(atom);
    for (index, content_id) in companions.into_iter().enumerate() {
        draft.add_atom(Atom::Tensor(Tensor::new(
            id::<AtomTag>(100 + index as u8),
            Presence::Present,
            Some(payload(content_id, vec![1], Layout::DenseRowMajor)),
            vec![SemanticAxis::new(
                ConceptId::new("abir:axis/companion").unwrap(),
                1,
            )],
        )));
    }
    draft.validate(ValidationLimits::default()).is_ok()
}

#[test]
fn presence_states_distinguish_absence_uncertainty_and_policy() {
    let states = [
        Presence::Present,
        Presence::AbsentAtSource,
        Presence::UnknownAtSource,
        Presence::Withheld,
        Presence::Redacted,
        Presence::NotApplicable,
    ];
    assert_eq!(states.len(), 6);

    for (index, state) in states.into_iter().enumerate().skip(1) {
        let atom = Atom::Table(Table::new(
            id::<AtomTag>(index as u8),
            state,
            None,
            vec![TableColumn::new(
                ConceptId::new("abir:column/value").unwrap(),
                ElementType::I16,
                false,
            )],
        ));
        assert!(validates(atom));
    }
}

#[test]
fn composite_layouts_name_distinct_companion_payloads() {
    let cases = [
        Layout::Ragged {
            rows: 2,
            offsets: content(20),
        },
        Layout::SparseCoo {
            nonzero: 2,
            indices: content(21),
        },
        Layout::SparseCsr {
            nonzero: 2,
            indptr: content(22),
            indices: content(23),
        },
        Layout::BlockFloatingPoint {
            block_len: 4,
            mantissa_bits: 8,
            scales: content(24),
        },
    ];

    for (index, layout) in cases.into_iter().enumerate() {
        let atom = Atom::Tensor(Tensor::new(
            id::<AtomTag>(30 + index as u8),
            Presence::Present,
            Some(payload(content(10 + index as u8), vec![2, 2], layout)),
            vec![
                SemanticAxis::new(ConceptId::new("abir:axis/row").unwrap(), 2),
                SemanticAxis::new(ConceptId::new("abir:axis/column").unwrap(), 2),
            ],
        ));
        assert!(validates(atom));
    }

    let aliased = Atom::Tensor(Tensor::new(
        id::<AtomTag>(40),
        Presence::Present,
        Some(payload(
            content(9),
            vec![2, 2],
            Layout::SparseCoo {
                nonzero: 2,
                indices: content(9),
            },
        )),
        vec![
            SemanticAxis::new(ConceptId::new("abir:axis/row").unwrap(), 2),
            SemanticAxis::new(ConceptId::new("abir:axis/column").unwrap(), 2),
        ],
    ));
    assert!(!validates(aliased));
}

#[test]
fn non_signal_atoms_require_their_semantic_contracts() {
    let columns = vec![
        TableColumn::new(
            ConceptId::new("abir:column/time").unwrap(),
            ElementType::I16,
            false,
        ),
        TableColumn::new(
            ConceptId::new("abir:column/value").unwrap(),
            ElementType::I16,
            true,
        ),
    ];
    let axes = vec![
        SemanticAxis::new(ConceptId::new("abir:axis/channel").unwrap(), 2),
        SemanticAxis::new(ConceptId::new("abir:axis/sample").unwrap(), 3),
    ];

    assert!(validates(Atom::TemporalTable(TemporalTable::new(
        id::<AtomTag>(50),
        Presence::Present,
        Some(payload(content(50), vec![3, 2], Layout::DenseRowMajor,)),
        id::<ClockTag>(51),
        ConceptId::new("abir:record/event").unwrap(),
        columns.clone(),
    ))));
    assert!(validates(Atom::Table(Table::new(
        id::<AtomTag>(52),
        Presence::Present,
        Some(payload(content(52), vec![3, 2], Layout::DenseRowMajor,)),
        columns,
    ))));
    assert!(validates(Atom::Tensor(Tensor::new(
        id::<AtomTag>(53),
        Presence::Present,
        Some(payload(content(53), vec![2, 3], Layout::DenseRowMajor,)),
        axes,
    ))));
    assert!(validates(Atom::EncodedBlock(EncodedBlock::new(
        id::<AtomTag>(54),
        Presence::Present,
        Some(payload(content(54), vec![6], Layout::DenseRowMajor)),
        DecodedSemantics::new(
            ConceptId::new("abir:atom/signal-block").unwrap(),
            ElementType::F32,
            vec![2, 3],
        ),
    ))));
    assert!(validates(Atom::BlobRef(BlobRef::new(
        id::<AtomTag>(55),
        Presence::Present,
        Some(payload(content(55), vec![6], Layout::DenseRowMajor)),
        "application/octet-stream".into(),
        BlobIntegrity::new(
            ConceptId::new("abir:integrity/blake3-256").unwrap(),
            content(55)
        ),
    ))));
}

#[test]
fn malformed_non_signal_contracts_fail_closed() {
    let duplicate_columns = vec![
        TableColumn::new(
            ConceptId::new("abir:column/value").unwrap(),
            ElementType::I16,
            false,
        ),
        TableColumn::new(
            ConceptId::new("abir:column/value").unwrap(),
            ElementType::I16,
            true,
        ),
    ];
    assert!(!validates(Atom::Table(Table::new(
        id::<AtomTag>(60),
        Presence::Present,
        Some(payload(content(60), vec![3, 2], Layout::DenseRowMajor,)),
        duplicate_columns,
    ))));

    assert!(!validates(Atom::Tensor(Tensor::new(
        id::<AtomTag>(61),
        Presence::Present,
        Some(payload(content(61), vec![2, 3], Layout::DenseRowMajor,)),
        vec![SemanticAxis::new(
            ConceptId::new("abir:axis/sample").unwrap(),
            6,
        )],
    ))));

    assert!(!validates(Atom::EncodedBlock(EncodedBlock::new(
        id::<AtomTag>(62),
        Presence::Present,
        Some(payload(content(62), vec![6], Layout::DenseRowMajor)),
        DecodedSemantics::new(
            ConceptId::new("abir:atom/signal-block").unwrap(),
            ElementType::F32,
            vec![],
        ),
    ))));

    assert!(!validates(Atom::BlobRef(BlobRef::new(
        id::<AtomTag>(63),
        Presence::Present,
        Some(payload(content(63), vec![6], Layout::DenseRowMajor)),
        "not a media type".into(),
        BlobIntegrity::new(
            ConceptId::new("abir:integrity/blake3-256").unwrap(),
            content(63)
        ),
    ))));
}
