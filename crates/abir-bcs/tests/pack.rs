use abir::{DatasetDraft, DatasetTag, ObjectId, ValidationLimits};
use abir_bcs::{
    encode_dataset_with_references, repack_with_frames, Bcs2Error, Bcs2View, ProfileId,
    ResourceBounds,
};

fn artifact(seed: u8, references: impl IntoIterator<Item = abir::ContentId>) -> Vec<u8> {
    let dataset = DatasetDraft::new(ObjectId::<DatasetTag>::from_bytes([seed; 16]))
        .validate(ValidationLimits::default())
        .unwrap();
    encode_dataset_with_references(
        &dataset,
        ProfileId::LML_LOSSLESS_V1,
        ResourceBounds::default(),
        references,
    )
    .unwrap()
}

#[test]
fn portable_frames_are_deterministic_borrowed_and_identity_checked() {
    let grandchild = artifact(1, []);
    let grandchild_id = Bcs2View::parse(&grandchild, 0, ResourceBounds::default())
        .unwrap()
        .root_content_id();
    let child = artifact(2, [grandchild_id]);
    let child_id = Bcs2View::parse(&child, 0, ResourceBounds::default())
        .unwrap()
        .root_content_id();
    let root = artifact(3, [child_id]);
    let plain = Bcs2View::parse(&root, 0, ResourceBounds::default()).unwrap();

    let packed = repack_with_frames(
        &root,
        &[child.as_slice(), grandchild.as_slice()],
        0,
        ResourceBounds::default(),
    )
    .unwrap();
    let reversed = repack_with_frames(
        &root,
        &[grandchild.as_slice(), child.as_slice()],
        0,
        ResourceBounds::default(),
    )
    .unwrap();
    assert_eq!(packed, reversed);
    let view = Bcs2View::parse(&packed, 0, ResourceBounds::default()).unwrap();
    assert_eq!(view.root_content_id(), plain.root_content_id());
    assert_ne!(view.storage_id(), plain.storage_id());
    assert_eq!(view.frames().len(), 2);
    assert!(view.frames()[0].content_id() < view.frames()[1].content_id());
    for frame in view.frames() {
        let offset = frame.bytes().as_ptr() as usize - packed.as_ptr() as usize;
        assert_eq!(&packed[offset..offset + frame.bytes().len()], frame.bytes());
        let nested = Bcs2View::parse(frame.bytes(), 0, ResourceBounds::default()).unwrap();
        assert_eq!(nested.root_content_id(), frame.content_id());
        assert_eq!(nested.storage_id(), frame.storage_id());
    }

    let first_frame_offset = view.frames()[0].bytes().as_ptr() as usize - packed.as_ptr() as usize;
    let mut corrupt = packed.clone();
    corrupt[first_frame_offset + 16] ^= 1;
    assert_eq!(
        Bcs2View::parse(&corrupt, 0, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::FrameDigestMismatch
    );
}

#[test]
fn packs_reject_duplicates_nonportable_profiles_and_nested_packs() {
    let child = artifact(4, []);
    let child_id = Bcs2View::parse(&child, 0, ResourceBounds::default())
        .unwrap()
        .root_content_id();
    let root = artifact(5, [child_id]);
    assert_eq!(
        repack_with_frames(
            &root,
            &[child.as_slice(), child.as_slice()],
            0,
            ResourceBounds::default()
        ),
        Err(Bcs2Error::DuplicateFrame)
    );

    assert_eq!(
        repack_with_frames(&root, &[], 0, ResourceBounds::default()),
        Err(Bcs2Error::IncompletePortableClosure(child_id))
    );
    let unrelated = artifact(7, []);
    let unrelated_id = Bcs2View::parse(&unrelated, 0, ResourceBounds::default())
        .unwrap()
        .root_content_id();
    assert_eq!(
        repack_with_frames(
            &root,
            &[child.as_slice(), unrelated.as_slice()],
            0,
            ResourceBounds::default()
        ),
        Err(Bcs2Error::ExtraPortableFrame(unrelated_id))
    );

    let dataset = DatasetDraft::new(ObjectId::<DatasetTag>::from_bytes([6; 16]))
        .validate(ValidationLimits::default())
        .unwrap();
    let nonportable = encode_dataset_with_references(
        &dataset,
        ProfileId::TRAINING_BALANCED_V1,
        ResourceBounds::default(),
        [child_id],
    )
    .unwrap();
    assert_eq!(
        repack_with_frames(
            &nonportable,
            &[child.as_slice()],
            0,
            ResourceBounds::default()
        ),
        Err(Bcs2Error::ProfileNotPortable)
    );

    let packed =
        repack_with_frames(&root, &[child.as_slice()], 0, ResourceBounds::default()).unwrap();
    assert_eq!(
        repack_with_frames(&root, &[packed.as_slice()], 0, ResourceBounds::default()),
        Err(Bcs2Error::DuplicateFrame)
    );
}
