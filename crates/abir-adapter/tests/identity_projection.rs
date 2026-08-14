use abir::{ContentId, SourceKey};
use abir_adapter::{IdentityProjection, IntegrityObservation};

#[test]
fn projection_separates_semantic_integrity_and_foreign_identity() {
    let semantic = ContentId::from_bytes([0x11; 32]);
    let projection = IdentityProjection::new(
        semantic,
        vec![
            IntegrityObservation::new("storage.artifact", "sha256", [0x22; 32]).unwrap(),
            IntegrityObservation::new("source.bytes", "sha256", [0x33; 32]).unwrap(),
        ],
        vec![SourceKey::new("edf.recording", "subject-01/run-02").unwrap()],
    )
    .unwrap();

    assert_eq!(projection.content_id(), semantic);
    assert_eq!(
        projection.integrity_observations()[0].domain(),
        "source.bytes"
    );
    assert_eq!(
        projection.integrity_observations()[1].domain(),
        "storage.artifact"
    );
    assert_eq!(projection.source_keys()[0].namespace(), "edf.recording");
}

#[test]
fn projection_rejects_ambiguous_or_duplicate_integrity_domains() {
    assert!(IntegrityObservation::new("source bytes", "sha256", [0x44; 32]).is_err());
    assert!(IntegrityObservation::new("source.bytes", "SHA-256", [0x44; 32]).is_err());

    let observation = IntegrityObservation::new("source.bytes", "sha256", [0x44; 32]).unwrap();
    assert!(IdentityProjection::new(
        ContentId::from_bytes([0x11; 32]),
        vec![observation.clone(), observation],
        Vec::new(),
    )
    .is_err());
}

#[test]
fn projection_rejects_duplicate_foreign_keys() {
    let key = SourceKey::new("bids.subject", "sub-01").unwrap();
    assert!(IdentityProjection::new(
        ContentId::from_bytes([0x11; 32]),
        Vec::new(),
        vec![key.clone(), key],
    )
    .is_err());
}
