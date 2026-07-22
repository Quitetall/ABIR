use abir::{DatasetDraft, DatasetTag, ObjectId, ValidationLimits};
use abir_bcs::{
    decrypt_bcs2, encode_dataset, encrypt_bcs2, Bcs2Error, Bcs2View, EncryptedEnvelopeView,
    PrivacyMode, ProfileId, ResourceBounds, RootKind,
};

fn plaintext() -> Vec<u8> {
    let dataset = DatasetDraft::new(ObjectId::<DatasetTag>::from_bytes([31; 16]))
        .validate(ValidationLimits::default())
        .unwrap();
    encode_dataset(
        &dataset,
        ProfileId::LML_LOSSLESS_V1,
        ResourceBounds::default(),
    )
    .unwrap()
}

#[test]
fn opaque_envelope_discloses_no_semantic_identity_and_authenticates() {
    let plaintext = plaintext();
    let key = [7; 32];
    let nonce = [9; 24];
    let encrypted = encrypt_bcs2(
        &plaintext,
        PrivacyMode::EncryptedOpaque,
        &key,
        &nonce,
        0,
        ResourceBounds::default(),
    )
    .unwrap();
    let envelope = EncryptedEnvelopeView::parse(&encrypted, ResourceBounds::default()).unwrap();
    assert_eq!(envelope.privacy_mode(), PrivacyMode::EncryptedOpaque);
    assert_eq!(envelope.disclosed_profile(), None);
    assert_eq!(envelope.disclosed_root_kind(), None);
    assert_eq!(envelope.disclosed_root_content_id(), None);
    assert!(encrypted[16..24].iter().all(|byte| *byte == 0));
    assert_eq!(encrypted[40], 0);
    assert!(encrypted[96..128].iter().all(|byte| *byte == 0));
    assert_eq!(
        Bcs2View::parse(&encrypted, 0, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::PrivacyModeNotImplemented(PrivacyMode::EncryptedOpaque)
    );
    assert_eq!(
        decrypt_bcs2(&encrypted, &key, 0, ResourceBounds::default()).unwrap(),
        plaintext
    );
    assert_eq!(
        decrypt_bcs2(&encrypted, &[8; 32], 0, ResourceBounds::default()),
        Err(Bcs2Error::AuthenticationFailed)
    );

    let mut tampered = encrypted.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    assert_eq!(
        decrypt_bcs2(&tampered, &key, 0, ResourceBounds::default()),
        Err(Bcs2Error::AuthenticationFailed)
    );
}

#[test]
fn discoverable_mode_binds_disclosed_identity_and_nonce_changes_storage() {
    let plaintext = plaintext();
    let inner = Bcs2View::parse(&plaintext, 0, ResourceBounds::default()).unwrap();
    let key = [11; 32];
    let first = encrypt_bcs2(
        &plaintext,
        PrivacyMode::EncryptedDiscoverable,
        &key,
        &[1; 24],
        0,
        ResourceBounds::default(),
    )
    .unwrap();
    let second = encrypt_bcs2(
        &plaintext,
        PrivacyMode::EncryptedDiscoverable,
        &key,
        &[2; 24],
        0,
        ResourceBounds::default(),
    )
    .unwrap();
    let envelope = EncryptedEnvelopeView::parse(&first, ResourceBounds::default()).unwrap();
    assert_eq!(
        envelope.disclosed_profile(),
        Some(ProfileId::LML_LOSSLESS_V1)
    );
    assert_eq!(envelope.disclosed_root_kind(), Some(RootKind::Dataset));
    assert_eq!(
        envelope.disclosed_root_content_id(),
        Some(inner.root_content_id())
    );
    assert_ne!(
        envelope.storage_id(),
        EncryptedEnvelopeView::parse(&second, ResourceBounds::default())
            .unwrap()
            .storage_id()
    );

    let mut false_disclosure = first.clone();
    false_disclosure[96] ^= 1;
    assert_eq!(
        decrypt_bcs2(&false_disclosure, &key, 0, ResourceBounds::default()),
        Err(Bcs2Error::AuthenticationFailed)
    );
}

#[test]
fn encrypted_envelopes_enforce_declared_resource_bounds() {
    let plaintext = plaintext();
    let encrypted = encrypt_bcs2(
        &plaintext,
        PrivacyMode::EncryptedOpaque,
        &[1; 32],
        &[2; 24],
        0,
        ResourceBounds::default(),
    )
    .unwrap();
    assert!(EncryptedEnvelopeView::parse(&encrypted[..167], ResourceBounds::default()).is_err());
    assert_eq!(
        EncryptedEnvelopeView::parse(
            &encrypted,
            ResourceBounds {
                max_frame_bytes: 1,
                ..ResourceBounds::default()
            }
        )
        .unwrap_err(),
        Bcs2Error::BoundsExceeded
    );
}
