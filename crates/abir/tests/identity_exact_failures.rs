use abir::{
    ConceptId, ContentId, DatasetTag, ExactNumber, FailureCode, Handle, ObjectId, Rational,
    SourceKey, StorageId, ValidationFailure, ValidationLimits, ValidationReport,
};

#[test]
fn typed_ids_preserve_bytes_without_interchanging_types() {
    let dataset = ObjectId::<DatasetTag>::from_bytes([0xabu8; 16]);
    assert_eq!(dataset.to_bytes(), [0xabu8; 16]);
    assert_eq!(dataset.to_string(), "abababababababababababababababab");
    assert_eq!(dataset, dataset);

    let handle = Handle::<DatasetTag>::new(17);
    assert_eq!(handle.get(), 17);
    assert_eq!(ContentId::from_bytes([3; 32]).to_bytes(), [3; 32]);
    assert_eq!(StorageId::from_bytes([4; 32]).to_bytes(), [4; 32]);
}

#[test]
fn rationals_are_reduced_and_denominators_are_positive() {
    assert_eq!(Rational::new(6, -8).unwrap(), Rational::new(-3, 4).unwrap());
    assert_eq!(Rational::new(0, 99).unwrap().parts(), (0, 1));
    assert!(Rational::new(1, 0).is_err());
    assert!(Rational::new(1, i128::MIN).is_err());
    assert_eq!(ExactNumber::from(7_i64).to_string(), "7");
}

#[test]
fn concepts_and_source_keys_preserve_namespaced_foreign_identity() {
    let concept = ConceptId::new("abir:modality/eeg").unwrap();
    assert_eq!(concept.as_str(), "abir:modality/eeg");
    assert!(ConceptId::new("EEG").is_err());
    assert!(ConceptId::new("ABIR:modality/eeg").is_err());

    let key = SourceKey::new("edf.signal", "EEG Fp1-Ref").unwrap();
    assert_eq!(key.namespace(), "edf.signal");
    assert_eq!(key.value(), "EEG Fp1-Ref");
}

#[test]
fn validation_reports_are_structured_and_nonempty() {
    let failure = ValidationFailure::error(FailureCode::DuplicateId, "recordings[1].id");
    let report = ValidationReport::new(failure);
    assert_eq!(report.len(), 1);
    assert_eq!(report.failures()[0].code(), "ABIR-E001");
    assert!(!report.is_empty());

    let limits = ValidationLimits::default();
    assert!(limits.max_recordings > 0);
    assert!(limits.max_nesting_depth > 0);
}
