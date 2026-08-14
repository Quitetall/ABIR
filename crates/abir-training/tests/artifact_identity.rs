use abir::ContentId;
use abir_training::training_artifact_content_id;

#[test]
fn artifact_identity_is_deterministic_and_domain_separated() {
    let canonical = br#"{"kind":"checkpoint","schema":1}"#;
    let first = training_artifact_content_id(canonical);
    let second = training_artifact_content_id(canonical);
    // Recompute another registered domain independently so this test catches a
    // copied domain label inside the artifact sealer itself.
    let mut semantic_hasher = blake3::Hasher::new();
    semantic_hasher.update(b"org.quitetall.abir.training.semantic.feature-v1\0");
    semantic_hasher.update(canonical);
    let semantic = ContentId::from_bytes(*semantic_hasher.finalize().as_bytes());

    assert_eq!(first, second);
    assert_eq!(
        first.to_string(),
        "8bdbc291d9e07b6fa4184c9d0865c0bd63c5ca4db49b7182023f37d4b7cbe90c"
    );
    assert_ne!(first, semantic);
    assert_ne!(first, training_artifact_content_id(b"different"));
}
