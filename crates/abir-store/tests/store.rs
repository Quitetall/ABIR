use abir::{DatasetDraft, DatasetTag, ObjectId, ValidationLimits};
use abir_bcs::{
    encode_dataset, encode_dataset_with_references, Bcs2View, ProfileId, ResourceBounds,
};
use abir_store::{AbirStore, StoreError};
use std::sync::Arc;

fn artifact(seed: u8, profile: ProfileId) -> Arc<[u8]> {
    let dataset = DatasetDraft::new(ObjectId::<DatasetTag>::from_bytes([seed; 16]))
        .validate(ValidationLimits::default())
        .expect("valid dataset");
    encode_dataset(&dataset, profile, ResourceBounds::default())
        .expect("encode")
        .into()
}

fn artifact_with_references(
    seed: u8,
    profile: ProfileId,
    references: impl IntoIterator<Item = abir::ContentId>,
) -> Arc<[u8]> {
    let dataset = DatasetDraft::new(ObjectId::<DatasetTag>::from_bytes([seed; 16]))
        .validate(ValidationLimits::default())
        .expect("valid dataset");
    encode_dataset_with_references(&dataset, profile, ResourceBounds::default(), references)
        .expect("encode")
        .into()
}

#[test]
fn logical_identity_can_have_multiple_physical_variants() {
    let mut store = AbirStore::default();
    let first = artifact(1, ProfileId::LML_LOSSLESS_V1);
    let second = artifact(1, ProfileId::TRAINING_COMPACT_V1);
    let (content, first_storage) = store
        .insert_bcs2(Arc::clone(&first), 0, ResourceBounds::default())
        .expect("insert first");
    let (same_content, second_storage) = store
        .insert_bcs2(Arc::clone(&second), 0, ResourceBounds::default())
        .expect("insert second");
    assert_eq!(content, same_content);
    assert_ne!(first_storage, second_storage);
    assert_eq!(store.physical_variants(content), 2);
    assert_eq!(store.object_count(), 2);
}

#[test]
fn closure_must_exist_before_pin_and_gc_preserves_pins() {
    let mut store = AbirStore::default();
    let leaf_bytes = artifact(2, ProfileId::LML_LOSSLESS_V1);
    let (leaf, _) = store
        .insert_bcs2(leaf_bytes, 0, ResourceBounds::default())
        .expect("leaf");
    let root_bytes = artifact_with_references(3, ProfileId::LML_LOSSLESS_V1, [leaf]);
    let (root, _) = store
        .insert_bcs2(root_bytes, 0, ResourceBounds::default())
        .expect("root");
    let orphan_bytes = artifact(4, ProfileId::LML_LOSSLESS_V1);
    store
        .insert_bcs2(orphan_bytes, 0, ResourceBounds::default())
        .expect("orphan");
    store.pin(root).expect("complete closure pins");
    assert_eq!(store.reachable_closure(root).unwrap().len(), 2);
    assert_eq!(store.collect_unreachable(), 1);
    assert_eq!(store.object_count(), 2);

    let missing = abir::ContentId::from_bytes([99; 32]);
    let missing_bytes = artifact_with_references(5, ProfileId::LML_LOSSLESS_V1, [missing]);
    let (bad_root, _) = store
        .insert_bcs2(missing_bytes, 0, ResourceBounds::default())
        .expect("store may be assembled before closure arrives");
    assert_eq!(
        store.pin(bad_root),
        Err(StoreError::IncompleteClosure(missing))
    );
}

#[test]
fn leases_are_zero_copy_and_hold_unpinned_storage_alive() {
    let mut store = AbirStore::default();
    let bytes = artifact(6, ProfileId::LML_LOSSLESS_V1);
    let source_ptr = bytes.as_ptr();
    let (content, storage) = store
        .insert_bcs2(Arc::clone(&bytes), 0, ResourceBounds::default())
        .expect("insert");
    drop(bytes);
    let lease = store.lease_storage(storage).expect("lease");
    assert_eq!(lease.content_id(), content);
    assert_eq!(lease.bytes().as_ptr(), source_ptr);
    assert_eq!(
        store.collect_unreachable(),
        0,
        "active lease protects bytes"
    );
    drop(lease);
    assert_eq!(store.collect_unreachable(), 1);
    assert!(matches!(
        store.lease(content),
        Err(StoreError::MissingObject(id)) if id == content
    ));
}

#[test]
fn physical_variants_cannot_redefine_logical_closure() {
    let mut store = AbirStore::default();
    let plain = artifact(7, ProfileId::LML_LOSSLESS_V1);
    let foreign = abir::ContentId::from_bytes([77; 32]);
    let conflicting = artifact_with_references(7, ProfileId::TRAINING_COMPACT_V1, [foreign]);
    let (content, _) = store
        .insert_bcs2(plain, 0, ResourceBounds::default())
        .unwrap();
    assert_eq!(
        store.insert_bcs2(conflicting, 0, ResourceBounds::default()),
        Err(StoreError::ConflictingClosure(content))
    );
    assert_eq!(store.physical_variants(content), 1);
}

#[test]
fn portable_export_import_carries_exact_reachable_closure() {
    let mut source = AbirStore::default();
    let grandchild_bytes = artifact(20, ProfileId::LML_LOSSLESS_V1);
    let (grandchild, _) = source
        .insert_bcs2(grandchild_bytes, 0, ResourceBounds::default())
        .unwrap();
    let child_bytes = artifact_with_references(21, ProfileId::LML_LOSSLESS_V1, [grandchild]);
    let (child, _) = source
        .insert_bcs2(child_bytes, 0, ResourceBounds::default())
        .unwrap();
    let root_bytes = artifact_with_references(22, ProfileId::LML_LOSSLESS_V1, [child]);
    let (root, root_storage) = source
        .insert_bcs2(root_bytes, 0, ResourceBounds::default())
        .unwrap();

    let portable = source
        .export_portable(root, 0, ResourceBounds::default())
        .unwrap();
    let packed = Bcs2View::parse(&portable, 0, ResourceBounds::default()).unwrap();
    assert_eq!(packed.root_content_id(), root);
    assert_ne!(packed.storage_id(), root_storage);
    assert_eq!(packed.frames().len(), 2);

    let mut destination = AbirStore::default();
    let imported = destination
        .import_portable(Arc::from(portable), 0, ResourceBounds::default())
        .unwrap();
    assert_eq!(imported.0, root);
    assert_eq!(destination.reachable_closure(root).unwrap().len(), 3);
    assert_eq!(destination.physical_variants(root), 1);
}

#[test]
fn portable_import_rejects_missing_frames_before_publication() {
    let child = artifact(23, ProfileId::LML_LOSSLESS_V1);
    let child_id = Bcs2View::parse(&child, 0, ResourceBounds::default())
        .unwrap()
        .root_content_id();
    let root = artifact_with_references(24, ProfileId::LML_LOSSLESS_V1, [child_id]);
    let mut store = AbirStore::default();
    assert_eq!(
        store.import_portable(Arc::clone(&root), 0, ResourceBounds::default()),
        Err(StoreError::IncompletePortableBundle(child_id))
    );
    assert_eq!(store.object_count(), 0);
}
