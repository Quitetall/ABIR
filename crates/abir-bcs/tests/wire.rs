use abir::{DatasetDraft, DatasetTag, ObjectId, ValidationLimits};
use abir_bcs::{
    encode_dataset, encode_dataset_with_references, Bcs2Error, Bcs2View, PrivacyMode, ProfileId,
    ResourceBounds, RootKind, StorageContract,
};

fn dataset() -> abir::AbirDataset {
    DatasetDraft::new(ObjectId::<DatasetTag>::from_bytes([7; 16]))
        .validate(ValidationLimits::default())
        .expect("valid dataset")
}

#[test]
fn closure_references_are_canonical_storage_bytes() {
    let dataset = dataset();
    let lower = abir::ContentId::from_bytes([1; 32]);
    let higher = abir::ContentId::from_bytes([2; 32]);
    let bytes = encode_dataset_with_references(
        &dataset,
        ProfileId::LML_LOSSLESS_V1,
        ResourceBounds::default(),
        [higher, lower, higher],
    )
    .expect("encode references");
    let view = Bcs2View::parse(&bytes, 0, ResourceBounds::default()).expect("parse");
    assert_eq!(view.references(), &[lower, higher]);
    let without_references = encode_dataset(
        &dataset,
        ProfileId::LML_LOSSLESS_V1,
        ResourceBounds::default(),
    )
    .expect("encode empty closure");
    assert_ne!(
        view.storage_id(),
        Bcs2View::parse(&without_references, 0, ResourceBounds::default())
            .unwrap()
            .storage_id()
    );
    assert_eq!(
        view.root_content_id(),
        Bcs2View::parse(&without_references, 0, ResourceBounds::default())
            .unwrap()
            .root_content_id()
    );
}

#[test]
fn dataset_wire_is_deterministic_and_borrowed() {
    let dataset = dataset();
    let first = encode_dataset(
        &dataset,
        ProfileId::LML_LOSSLESS_V1,
        ResourceBounds::default(),
    )
    .expect("encode");
    let second = encode_dataset(
        &dataset,
        ProfileId::LML_LOSSLESS_V1,
        ResourceBounds::default(),
    )
    .expect("encode");
    assert_eq!(first, second);
    let view = Bcs2View::parse(&first, 0, ResourceBounds::default()).expect("parse");
    assert_eq!(view.root_kind(), RootKind::Dataset);
    assert!(core::ptr::eq(
        view.semantic_json().as_ptr(),
        first[BCS2_CATALOG_JSON_OFFSET(&first)..].as_ptr()
    ));
    assert_eq!(
        view.storage_id(),
        Bcs2View::parse(&second, 0, ResourceBounds::default())
            .unwrap()
            .storage_id()
    );
}

#[allow(non_snake_case)]
fn BCS2_CATALOG_JSON_OFFSET(bytes: &[u8]) -> usize {
    let view = Bcs2View::parse(bytes, 0, ResourceBounds::default()).expect("parse");
    view.semantic_json().as_ptr() as usize - bytes.as_ptr() as usize
}

#[test]
fn malformed_wire_fails_closed() {
    let dataset = dataset();
    let valid = encode_dataset(
        &dataset,
        ProfileId::LML_LOSSLESS_V1,
        ResourceBounds::default(),
    )
    .expect("encode");
    for length in [0, 7, 127, valid.len() - 1] {
        assert!(Bcs2View::parse(&valid[..length], 0, ResourceBounds::default()).is_err());
    }
    let mut corrupt = valid.clone();
    corrupt[130] ^= 0x55;
    assert_eq!(
        Bcs2View::parse(&corrupt, 0, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::CatalogDigestMismatch
    );
    let mut unsupported = valid.clone();
    unsupported[24] = 1;
    assert_eq!(
        Bcs2View::parse(&unsupported, 0, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::UnsupportedCapabilities(1)
    );
    let mut optional = valid.clone();
    optional[32] = 1;
    Bcs2View::parse(&optional, 0, ResourceBounds::default())
        .expect("unknown optional capability is ignorable");
    let mut false_encryption = valid.clone();
    false_encryption[42] = PrivacyMode::EncryptedOpaque as u8;
    assert_eq!(
        Bcs2View::parse(&false_encryption, 0, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::PrivacyModeNotImplemented(PrivacyMode::EncryptedOpaque)
    );
    let mut false_generation = valid.clone();
    false_generation[41] = StorageContract::SealedGenerational as u8;
    assert_eq!(
        Bcs2View::parse(&false_generation, 0, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::StorageContractNotImplemented(StorageContract::SealedGenerational)
    );
    assert_eq!(
        Bcs2View::parse(
            &valid,
            0,
            ResourceBounds {
                max_catalog_bytes: 1,
                ..ResourceBounds::default()
            }
        )
        .unwrap_err(),
        Bcs2Error::BoundsExceeded
    );
}

#[test]
fn profile_and_resource_bounds_are_enforced() {
    let dataset = dataset();
    assert_eq!(
        encode_dataset(
            &dataset,
            ProfileId::STREAM_BOUNDED_V1,
            ResourceBounds::default()
        ),
        Err(Bcs2Error::ProfileRootMismatch)
    );
    assert_eq!(
        encode_dataset(
            &dataset,
            ProfileId::LML_LOSSLESS_V1,
            ResourceBounds {
                max_catalog_bytes: 1,
                ..ResourceBounds::default()
            }
        ),
        Err(Bcs2Error::BoundsExceeded)
    );
    assert_eq!(
        encode_dataset(
            &dataset,
            ProfileId::LML_LOSSLESS_V1,
            ResourceBounds {
                max_index_entries: 0,
                ..ResourceBounds::default()
            }
        ),
        Err(Bcs2Error::BoundsExceeded)
    );
}
