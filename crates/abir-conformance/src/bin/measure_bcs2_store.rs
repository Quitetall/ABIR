use abir::{DatasetDraft, DatasetTag, ObjectId, ValidationLimits};
use abir_bcs::{encode_blob, encode_dataset, Bcs2View, BlobView, ProfileId, ResourceBounds};
use abir_store::AbirStore;
use serde_json::json;
use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let iterations = std::env::args()
        .nth(1)
        .map(|value| value.parse::<u64>().expect("integer iterations"))
        .unwrap_or(100_000);
    assert!(iterations > 0);
    let bounds = ResourceBounds::default();
    let dataset = DatasetDraft::new(ObjectId::<DatasetTag>::from_bytes([0x51; 16]))
        .validate(ValidationLimits::default())
        .expect("measurement dataset");
    let artifact = encode_dataset(&dataset, ProfileId::LML_LOSSLESS_V1, bounds)
        .expect("encode measurement artifact");

    let start = Instant::now();
    for _ in 0..iterations {
        let view = Bcs2View::parse(black_box(&artifact), 0, bounds).expect("parse artifact");
        black_box(view.root_content_id());
    }
    let parse_elapsed = start.elapsed();

    let start = Instant::now();
    for _ in 0..iterations {
        black_box(
            encode_dataset(black_box(&dataset), ProfileId::LML_LOSSLESS_V1, bounds)
                .expect("encode artifact"),
        );
    }
    let encode_elapsed = start.elapsed();

    let payload = vec![0xA5; 1024 * 1024];
    let blob = encode_blob(&payload, "application/octet-stream", bounds).expect("encode blob");
    let blob_view = BlobView::parse(&blob, 0, bounds).expect("parse blob");
    let blob_start = blob.as_ptr() as usize;
    let payload_pointer = blob_view.bytes().as_ptr() as usize;
    let zero_copy = (blob_start..blob_start + blob.len()).contains(&payload_pointer);

    let mut store = AbirStore::default();
    let stored_bytes: Arc<[u8]> = Arc::from(artifact.clone());
    let original_arc_pointer = stored_bytes.as_ptr();
    let start = Instant::now();
    let (content_id, storage_id) = store
        .insert_bcs2(Arc::clone(&stored_bytes), 0, bounds)
        .expect("insert store object");
    let insert_elapsed = start.elapsed();
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(store.lease(content_id).expect("lease object"));
    }
    let lease_elapsed = start.elapsed();
    let lease = store.lease_storage(storage_id).expect("lease storage");

    let per_second = |elapsed: std::time::Duration| {
        iterations as f64 / elapsed.as_secs_f64().max(f64::MIN_POSITIVE)
    };
    let evidence = json!({
        "iterations": iterations,
        "artifact_bytes": artifact.len(),
        "parse_ops_per_second": per_second(parse_elapsed),
        "encode_ops_per_second": per_second(encode_elapsed),
        "store_insert_nanoseconds": insert_elapsed.as_nanos(),
        "store_lease_ops_per_second": per_second(lease_elapsed),
        "store_lease_pointer_identity": lease.bytes().as_ptr() == original_arc_pointer,
        "blob_payload_bytes": payload.len(),
        "blob_zero_copy_pointer_identity": zero_copy,
        "bcs2_view_stack_bytes": std::mem::size_of::<Bcs2View<'_>>(),
        "blob_view_stack_bytes": std::mem::size_of::<BlobView<'_>>(),
    });
    println!("{}", serde_json::to_string(&evidence).unwrap());
}
