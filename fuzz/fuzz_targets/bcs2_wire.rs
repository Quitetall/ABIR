#![no_main]

use abir_bcs::{
    Bcs2View, BlobView, EncryptedEnvelopeView, ForensicTreeView, ResourceBounds,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let limits = ResourceBounds {
        max_catalog_bytes: 64 * 1024,
        max_index_entries: 1_024,
        max_frame_bytes: 1024 * 1024,
        max_generations: 32,
    };
    let _ = Bcs2View::parse(data, 0, limits);
    let _ = BlobView::parse(data, 0, limits);
    let _ = ForensicTreeView::parse(data, 0, limits);
    let _ = EncryptedEnvelopeView::parse(data, limits);
});
