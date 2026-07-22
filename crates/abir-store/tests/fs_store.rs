use abir::{ContentId, DatasetDraft, DatasetTag, ObjectId, ValidationLimits};
use abir_bcs::{
    encode_blob, encode_dataset, encode_dataset_with_references, BlobView, ProfileId,
    ResourceBounds,
};
use abir_store::{FsAbirStore, StoreError};

fn artifact(seed: u8, references: impl IntoIterator<Item = ContentId>) -> Vec<u8> {
    let dataset = DatasetDraft::new(ObjectId::<DatasetTag>::from_bytes([seed; 16]))
        .validate(ValidationLimits::default())
        .expect("valid dataset");
    encode_dataset_with_references(
        &dataset,
        ProfileId::LML_LOSSLESS_V1,
        ResourceBounds::default(),
        references,
    )
    .expect("encode")
}

#[test]
fn filesystem_store_reopens_pins_and_honors_active_mmaps() {
    let directory = tempfile::tempdir().expect("temporary store");
    let mut store =
        FsAbirStore::open(directory.path(), 0, ResourceBounds::default()).expect("open store");
    let (leaf, _) = store.insert(&artifact(1, [])).expect("insert leaf");
    let (root, _) = store.insert(&artifact(2, [leaf])).expect("insert root");
    let (orphan, orphan_storage) = store.insert(&artifact(3, [])).expect("insert orphan");
    store.pin(root).expect("pin complete closure");
    let lease = store.lease_storage(orphan_storage).expect("mmap orphan");
    assert_eq!(lease.content_id(), orphan);
    assert!(!lease.bytes().is_empty());
    assert_eq!(store.collect_unreachable(), Err(StoreError::StoreBusy));
    drop(lease);
    assert_eq!(store.collect_unreachable().unwrap(), 1);
    assert_eq!(store.object_count(), 2);
    drop(store);

    let reopened =
        FsAbirStore::open(directory.path(), 0, ResourceBounds::default()).expect("reopen store");
    assert_eq!(reopened.object_count(), 2);
    assert_eq!(reopened.reachable_closure(root).unwrap().len(), 2);
    assert_eq!(reopened.lease(leaf).unwrap().content_id(), leaf);
}

#[test]
fn publication_is_idempotent_and_reopen_detects_corruption() {
    let directory = tempfile::tempdir().expect("temporary store");
    let mut store = FsAbirStore::open(directory.path(), 0, ResourceBounds::default()).unwrap();
    let bytes = artifact(4, []);
    let first = store.insert(&bytes).expect("first insert");
    let second = store.insert(&bytes).expect("duplicate insert");
    assert_eq!(first, second);
    assert_eq!(store.object_count(), 1);
    drop(store);

    let path = directory
        .path()
        .join("objects")
        .join(format!("{}.bcs2", first.1));
    let mut corrupt = std::fs::read(&path).unwrap();
    corrupt[130] ^= 1;
    std::fs::write(&path, corrupt).unwrap();
    assert!(FsAbirStore::open(directory.path(), 0, ResourceBounds::default()).is_err());
}

#[test]
fn stale_temporary_files_are_removed_and_unknown_names_fail_closed() {
    let directory = tempfile::tempdir().expect("temporary store");
    let objects = directory.path().join("objects");
    std::fs::create_dir_all(&objects).unwrap();
    std::fs::write(objects.join(".tmp-dead"), b"partial").unwrap();
    FsAbirStore::open(directory.path(), 0, ResourceBounds::default()).unwrap();
    assert!(!objects.join(".tmp-dead").exists());
    std::fs::write(objects.join("not-an-object"), b"junk").unwrap();
    assert_eq!(
        FsAbirStore::open(directory.path(), 0, ResourceBounds::default()).err(),
        Some(StoreError::InvalidObjectName)
    );
}

#[test]
fn plain_dataset_encoder_remains_store_compatible() {
    let directory = tempfile::tempdir().expect("temporary store");
    let dataset = DatasetDraft::new(ObjectId::<DatasetTag>::from_bytes([8; 16]))
        .validate(ValidationLimits::default())
        .unwrap();
    let bytes = encode_dataset(
        &dataset,
        ProfileId::TRAINING_COMPACT_V1,
        ResourceBounds::default(),
    )
    .unwrap();
    let mut store = FsAbirStore::open(directory.path(), 0, ResourceBounds::default()).unwrap();
    let (content, _) = store.insert(&bytes).unwrap();
    assert_eq!(store.lease(content).unwrap().bytes(), bytes);
}

#[test]
fn filesystem_store_rejects_conflicting_logical_closure_before_publish() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = FsAbirStore::open(directory.path(), 0, ResourceBounds::default()).unwrap();
    let plain = artifact(9, []);
    let foreign = ContentId::from_bytes([90; 32]);
    let conflicting = artifact(9, [foreign]);
    let (content, _) = store.insert(&plain).unwrap();
    assert_eq!(
        store.insert(&conflicting),
        Err(StoreError::ConflictingClosure(content))
    );
    assert_eq!(store.object_count(), 1);
    assert_eq!(
        std::fs::read_dir(directory.path().join("objects"))
            .unwrap()
            .count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn filesystem_store_never_follows_object_symlinks() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let objects = directory.path().join("objects");
    std::fs::create_dir_all(&objects).unwrap();
    let outside = directory.path().join("outside");
    std::fs::write(&outside, artifact(10, [])).unwrap();
    symlink(
        &outside,
        objects.join(format!("{}.bcs2", abir::StorageId::from_bytes([1; 32]))),
    )
    .unwrap();
    assert_eq!(
        FsAbirStore::open(directory.path(), 0, ResourceBounds::default()).err(),
        Some(StoreError::InvalidObjectName)
    );
}

#[test]
fn refresh_observes_objects_published_by_another_handle() {
    let directory = tempfile::tempdir().unwrap();
    let mut publisher = FsAbirStore::open(directory.path(), 0, ResourceBounds::default()).unwrap();
    let mut observer = FsAbirStore::open(directory.path(), 0, ResourceBounds::default()).unwrap();
    let (content, _) = publisher.insert(&artifact(11, [])).unwrap();

    assert_eq!(
        observer.lease(content).err(),
        Some(StoreError::MissingObject(content))
    );
    observer.refresh().unwrap();
    assert_eq!(observer.lease(content).unwrap().content_id(), content);
}

#[test]
fn filesystem_store_exports_and_imports_portable_closure() {
    let source_directory = tempfile::tempdir().unwrap();
    let mut source =
        FsAbirStore::open(source_directory.path(), 0, ResourceBounds::default()).unwrap();
    let (child, _) = source.insert(&artifact(12, [])).unwrap();
    let (root, _) = source.insert(&artifact(13, [child])).unwrap();
    let portable = source.export_portable(root).unwrap();

    let destination_directory = tempfile::tempdir().unwrap();
    let mut destination =
        FsAbirStore::open(destination_directory.path(), 0, ResourceBounds::default()).unwrap();
    assert_eq!(destination.import_portable(&portable).unwrap().0, root);
    assert_eq!(destination.object_count(), 2);
    assert_eq!(destination.reachable_closure(root).unwrap().len(), 2);
    drop(destination);

    let reopened =
        FsAbirStore::open(destination_directory.path(), 0, ResourceBounds::default()).unwrap();
    assert_eq!(reopened.object_count(), 2);
}

#[test]
fn filesystem_store_preserves_zero_copy_blob_payload_after_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = FsAbirStore::open(directory.path(), 0, ResourceBounds::default()).unwrap();
    let blob = encode_blob(
        b"disk image",
        "application/octet-stream",
        ResourceBounds::default(),
    )
    .unwrap();
    let (content, storage) = store.insert(&blob).unwrap();
    drop(store);

    let reopened = FsAbirStore::open(directory.path(), 0, ResourceBounds::default()).unwrap();
    let lease = reopened.lease_storage(storage).unwrap();
    assert_eq!(lease.content_id(), content);
    assert_eq!(
        BlobView::parse(lease.bytes(), 0, ResourceBounds::default())
            .unwrap()
            .bytes(),
        b"disk image"
    );
}
