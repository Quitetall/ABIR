use abir_bcs::{
    CAP_LAMQUANT_BFP_V1, CAP_LMA_SYNTHETIC_REEMIT, CAP_LML1_LEGACY_MATERIALIZE,
    CAP_LML_ARITHMETIC_V1, CAP_LML_LOSSLESS_V1, CAP_LML_OPTIMUM_V1, CAP_LMQC_LEGACY_V1,
    CAP_XCHACHA20_POLY1305, CAP_ZSTD,
};

#[test]
fn normative_capability_registry_matches_rust_constants() {
    let registry: serde_json::Value = serde_json::from_str(include_str!(
        "../../../registries/bcs2-capabilities-v1.json"
    ))
    .unwrap();
    let expected = [
        (0, CAP_XCHACHA20_POLY1305, "xchacha20-poly1305"),
        (1, CAP_LML_OPTIMUM_V1, "lml-optimum-v1"),
        (2, CAP_ZSTD, "zstd"),
        (3, CAP_LML_LOSSLESS_V1, "lml-lossless-v1"),
        (4, CAP_LMA_SYNTHETIC_REEMIT, "lma-synthetic-reemit"),
        (5, CAP_LAMQUANT_BFP_V1, "lamquant-bfp-v1"),
        (6, CAP_LML_ARITHMETIC_V1, "lml-arithmetic-v1"),
        (7, CAP_LMQC_LEGACY_V1, "lmqc-legacy-v1"),
        (8, CAP_LML1_LEGACY_MATERIALIZE, "lml1-legacy-materialize"),
    ];
    let capabilities = registry["capabilities"].as_array().unwrap();
    assert_eq!(capabilities.len(), expected.len());
    for (record, (bit, mask, name)) in capabilities.iter().zip(expected) {
        assert_eq!(record["bit"].as_u64(), Some(bit));
        assert_eq!(record["mask"].as_u64(), Some(mask));
        assert_eq!(record["name"].as_str(), Some(name));
        assert_eq!(mask, 1_u64 << bit);
    }
}
