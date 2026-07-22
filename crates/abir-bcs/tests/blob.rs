use abir::{DatasetDraft, DatasetTag, ObjectId, ValidationLimits};
use abir_bcs::{
    encode_blob, encode_dataset_with_references, repack_with_frames, Bcs2Error, Bcs2View, BlobView,
    FrameKind, ProfileId, ResourceBounds,
};

#[test]
fn forensic_blob_is_deterministic_zero_copy_and_supports_empty_files() {
    let payload = b"exact source bytes\0\xff";
    let first = encode_blob(
        payload,
        "application/octet-stream",
        ResourceBounds::default(),
    )
    .unwrap();
    let second = encode_blob(
        payload,
        "application/octet-stream",
        ResourceBounds::default(),
    )
    .unwrap();
    assert_eq!(first, second);
    let view = BlobView::parse(&first, 0, ResourceBounds::default()).unwrap();
    assert_eq!(view.media_type(), "application/octet-stream");
    assert_eq!(view.bytes(), payload);
    assert_eq!(view.artifact().frames()[0].kind(), FrameKind::RawBlob);
    let offset = view.bytes().as_ptr() as usize - first.as_ptr() as usize;
    assert_eq!(&first[offset..offset + payload.len()], payload);

    let empty = encode_blob(&[], "application/octet-stream", ResourceBounds::default()).unwrap();
    assert!(BlobView::parse(&empty, 0, ResourceBounds::default())
        .unwrap()
        .bytes()
        .is_empty());
    assert_eq!(
        encode_blob(payload, "bad media type", ResourceBounds::default()),
        Err(Bcs2Error::SemanticEncoding)
    );
    for invalid in ["image", "/png", "image/", "image//png", "!image/png"] {
        assert_eq!(
            encode_blob(payload, invalid, ResourceBounds::default()),
            Err(Bcs2Error::SemanticEncoding)
        );
    }
    assert!(encode_blob(payload, "image/png#x", ResourceBounds::default()).is_ok());
}

#[test]
fn raw_payload_corruption_and_identity_relabeling_fail_closed() {
    let encoded = encode_blob(
        b"payload",
        "application/octet-stream",
        ResourceBounds::default(),
    )
    .unwrap();
    let view = BlobView::parse(&encoded, 0, ResourceBounds::default()).unwrap();
    let offset = view.bytes().as_ptr() as usize - encoded.as_ptr() as usize;
    let mut corrupt = encoded.clone();
    corrupt[offset] ^= 1;
    assert_eq!(
        Bcs2View::parse(&corrupt, 0, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::FrameDigestMismatch
    );

    let index_offset = u64::from_le_bytes(encoded[72..80].try_into().unwrap()) as usize;
    let mut relabeled = encoded.clone();
    relabeled[index_offset + 48] ^= 1;
    assert_eq!(
        Bcs2View::parse(&relabeled, 0, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::FrameIdentityMismatch
    );
}

#[test]
fn portable_tree_can_embed_self_contained_blob_artifacts() {
    let blob = encode_blob(b"edf bytes", "application/edf", ResourceBounds::default()).unwrap();
    let blob_id = BlobView::parse(&blob, 0, ResourceBounds::default())
        .unwrap()
        .content_id();
    let dataset = DatasetDraft::new(ObjectId::<DatasetTag>::from_bytes([42; 16]))
        .validate(ValidationLimits::default())
        .unwrap();
    let tree = encode_dataset_with_references(
        &dataset,
        ProfileId::FORENSIC_TREE_V1,
        ResourceBounds::default(),
        [blob_id],
    )
    .unwrap();
    let packed =
        repack_with_frames(&tree, &[blob.as_slice()], 0, ResourceBounds::default()).unwrap();
    let view = Bcs2View::parse(&packed, 0, ResourceBounds::default()).unwrap();
    assert_eq!(view.frames().len(), 1);
    assert_eq!(
        BlobView::parse(view.frames()[0].bytes(), 0, ResourceBounds::default())
            .unwrap()
            .bytes(),
        b"edf bytes"
    );
}
