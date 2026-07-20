use abir::{
    canonical_debug_json, logical_content_id, BorrowedPayload, BorrowedPayloadAccess, ContentId,
    ObjectId, OpenedDataset,
};
use abir_conformance::canonical_sample_dataset;
use serde_json::json;
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let iterations = std::env::var("ABIR_BENCH_ITERS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(100_000);
    assert!(
        (1..=10_000_000).contains(&iterations),
        "ABIR_BENCH_ITERS must be between 1 and 10,000,000"
    );

    let validation_start = Instant::now();
    for _ in 0..iterations {
        black_box(canonical_sample_dataset());
    }
    let validation_seconds = validation_start.elapsed().as_secs_f64();

    let dataset = canonical_sample_dataset();
    let canonical_bytes = canonical_debug_json(&dataset).expect("canonical debug JSON");
    let content_id = ContentId::from_bytes([5; 32]);
    let payload_bytes = [0_u8; 8];
    let payloads = [BorrowedPayload::new(content_id, &payload_bytes)];
    let opened = OpenedDataset::new(dataset, BorrowedPayloadAccess::new(&payloads));

    let view_start = Instant::now();
    for _ in 0..iterations {
        let view = opened
            .block_view(ObjectId::from_bytes([4; 16]))
            .expect("fixture view");
        black_box(view.bytes().as_ptr());
    }
    let view_seconds = view_start.elapsed().as_secs_f64();

    let hash_start = Instant::now();
    for _ in 0..iterations {
        black_box(logical_content_id(opened.dataset()).expect("logical content ID"));
    }
    let hash_seconds = hash_start.elapsed().as_secs_f64();

    let evidence = json!({
        "schema_version": 1,
        "iterations": iterations,
        "validation": {
            "seconds": validation_seconds,
            "datasets_per_second": iterations as f64 / validation_seconds
        },
        "view": {
            "seconds": view_seconds,
            "nanoseconds_per_lease": view_seconds * 1_000_000_000.0 / iterations as f64,
            "pointer_identity": opened.block_view(ObjectId::from_bytes([4; 16])).unwrap().bytes().as_ptr() == payload_bytes.as_ptr()
        },
        "logical_hash": {
            "seconds": hash_seconds,
            "hashes_per_second": iterations as f64 / hash_seconds
        },
        "metadata": {
            "root_inline_bytes": std::mem::size_of_val(opened.dataset()),
            "estimated_retained_bytes": opened.dataset().estimated_metadata_bytes(),
            "estimator": "UTF-8 semantic text plus 64 bytes per record; payload bytes excluded",
            "canonical_debug_bytes": canonical_bytes.len(),
            "note": "Trusted baseline only; regression ceilings are deferred until multiple hardware samples exist."
        }
    });
    println!(
        "{}",
        serde_json::to_string(&evidence).expect("serialize evidence")
    );
}
