use abir::{
    payload_content_id, verify_payload_content, Atom, AtomTag, ByteOrder, ConceptId, ContentId,
    DatasetDraft, DatasetTag, ElementType, Layout, ObjectId, PayloadDescriptor,
    PayloadVerificationError, Presence, SemanticAxis, SignalBlock, Tensor, TimeAxis,
    ValidationLimits,
};

fn id<T>(value: u8) -> ObjectId<T> {
    ObjectId::from_bytes([value; 16])
}

fn dense_payload(
    content_id: ContentId,
    element: ElementType,
    shape: Vec<u64>,
) -> PayloadDescriptor {
    let logical_bytes = shape.iter().product::<u64>() * element.byte_width().unwrap();
    PayloadDescriptor::new(
        content_id,
        logical_bytes,
        element,
        ByteOrder::Little,
        shape,
        Layout::DenseRowMajor,
        Some(ConceptId::new("abir:encoding/raw").unwrap()),
        None,
    )
}

fn companion_tensor(atom_id: u8, descriptor: PayloadDescriptor) -> Atom {
    let axes = descriptor
        .shape()
        .iter()
        .copied()
        .map(|extent| SemanticAxis::new(ConceptId::new("abir:axis/companion").unwrap(), extent))
        .collect();
    Atom::Tensor(Tensor::new(
        id::<AtomTag>(atom_id),
        Presence::Present,
        Some(descriptor),
        axes,
    ))
}

#[test]
fn payload_identity_is_stable_and_element_separated() {
    let bytes = [0x34, 0x12, 0x78, 0x56];
    let first = payload_content_id(ElementType::I16, &bytes);
    let second = payload_content_id(ElementType::I16, &bytes);

    assert_eq!(first, second);
    assert_eq!(
        first.to_string(),
        "71543c5b6e2a2dc8e0db2c305eba9d68432bec841d2ff2ca0d91fc139233d203"
    );
    assert_ne!(first, payload_content_id(ElementType::U16, &bytes));
    assert_ne!(first, payload_content_id(ElementType::I16, &[0x34, 0x12]));
}

#[test]
fn payload_verifier_rejects_wrong_length_and_identity() {
    let bytes = [1_u8, 2, 3, 4];
    let descriptor = dense_payload(
        payload_content_id(ElementType::I16, &bytes),
        ElementType::I16,
        vec![2],
    );
    assert_eq!(verify_payload_content(&descriptor, &bytes), Ok(()));

    assert_eq!(
        verify_payload_content(&descriptor, &bytes[..3]),
        Err(PayloadVerificationError::LengthMismatch {
            expected: 4,
            actual: 3,
        })
    );

    let wrong_id = dense_payload(ContentId::from_bytes([9; 32]), ElementType::I16, vec![2]);
    assert!(matches!(
        verify_payload_content(&wrong_id, &bytes),
        Err(PayloadVerificationError::ContentIdMismatch { expected, actual })
            if expected == ContentId::from_bytes([9; 32])
                && actual == descriptor.content_id()
    ));
}

#[test]
fn dataset_payload_ids_include_layout_and_timestamp_companions() {
    let primary = ContentId::from_bytes([40; 32]);
    let indptr = ContentId::from_bytes([10; 32]);
    let indices = ContentId::from_bytes([30; 32]);
    let timestamps = ContentId::from_bytes([20; 32]);

    let mut draft = DatasetDraft::new(id::<DatasetTag>(1));
    draft.add_atom(Atom::SignalBlock(SignalBlock::new(
        id::<AtomTag>(2),
        Presence::Present,
        Some(PayloadDescriptor::new(
            primary,
            4,
            ElementType::I16,
            ByteOrder::Little,
            vec![2, 2],
            Layout::SparseCsr {
                nonzero: 2,
                indptr,
                indices,
            },
            Some(ConceptId::new("abir:encoding/raw").unwrap()),
            None,
        )),
        TimeAxis::Explicit {
            timestamps,
            count: 2,
        },
        None,
    )));
    draft.add_atom(companion_tensor(
        3,
        dense_payload(indptr, ElementType::U32, vec![3]),
    ));
    draft.add_atom(companion_tensor(
        4,
        dense_payload(indices, ElementType::U32, vec![2]),
    ));
    draft.add_atom(companion_tensor(
        5,
        dense_payload(timestamps, ElementType::I64, vec![2]),
    ));

    let dataset = draft.validate(ValidationLimits::default()).unwrap();
    assert_eq!(
        dataset.payload_content_ids(),
        vec![indptr, timestamps, indices, primary]
    );
}
