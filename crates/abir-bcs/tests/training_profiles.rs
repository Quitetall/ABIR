use abir::{ContentId, DatasetDraft, DatasetTag, ObjectId, ValidationLimits};
use abir_bcs::{
    encode_dataset, encode_dataset_with_references, Bcs2View, ProfileId, ResourceBounds, RootKind,
};

const TRAINING_PROFILES: [ProfileId; 6] = [
    ProfileId::TRAINING_SPEED_V1,
    ProfileId::TRAINING_BALANCED_V1,
    ProfileId::TRAINING_MEMORY_V1,
    ProfileId::TRAINING_COMPACT_V1,
    ProfileId::TRAINING_ULTRA_COMPACT_V1,
    ProfileId::TRAINING_STREAM_V1,
];

fn dataset() -> abir::AbirDataset {
    DatasetDraft::new(ObjectId::<DatasetTag>::from_bytes([42; 16]))
        .validate(ValidationLimits::default())
        .expect("valid dataset")
}

#[test]
fn training_profiles_accept_only_dataset_and_bundle_roots() {
    for profile in TRAINING_PROFILES {
        assert!(profile.accepts(RootKind::Dataset));
        assert!(profile.accepts(RootKind::Bundle));
        for rejected in [
            RootKind::Recording,
            RootKind::Stream,
            RootKind::Atom,
            RootKind::Blob,
        ] {
            assert!(!profile.accepts(rejected));
        }
    }
}

#[test]
fn compact_profiles_are_portable_and_other_training_profiles_allow_references() {
    for profile in [
        ProfileId::TRAINING_COMPACT_V1,
        ProfileId::TRAINING_ULTRA_COMPACT_V1,
    ] {
        assert!(profile.is_portable());
        assert!(!profile.allows_external_references());
    }

    for profile in [
        ProfileId::TRAINING_SPEED_V1,
        ProfileId::TRAINING_BALANCED_V1,
        ProfileId::TRAINING_MEMORY_V1,
        ProfileId::TRAINING_STREAM_V1,
    ] {
        assert!(!profile.is_portable());
        assert!(profile.allows_external_references());
    }
}

#[test]
fn every_training_profile_is_registered_and_round_trips_its_wire_id() {
    for profile in TRAINING_PROFILES {
        let encoded = encode_dataset(&dataset(), profile, ResourceBounds::default()).unwrap();
        let parsed = Bcs2View::parse(&encoded, 0, ResourceBounds::default()).unwrap();
        assert_eq!(parsed.profile(), profile);
    }
}

#[test]
fn reference_permitting_profiles_preserve_external_object_ids() {
    let reference = ContentId::from_bytes([99; 32]);
    for profile in [
        ProfileId::TRAINING_SPEED_V1,
        ProfileId::TRAINING_BALANCED_V1,
        ProfileId::TRAINING_MEMORY_V1,
        ProfileId::TRAINING_STREAM_V1,
    ] {
        let encoded = encode_dataset_with_references(
            &dataset(),
            profile,
            ResourceBounds::default(),
            [reference],
        )
        .unwrap();
        let parsed = Bcs2View::parse(&encoded, 0, ResourceBounds::default()).unwrap();
        assert_eq!(parsed.references(), &[reference]);
    }
}
