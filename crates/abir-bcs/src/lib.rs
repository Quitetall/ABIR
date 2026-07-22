#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
mod blob;
#[cfg(feature = "alloc")]
mod codec;
#[cfg(feature = "alloc")]
mod forensic;
#[cfg(feature = "alloc")]
mod generation;
#[cfg(feature = "alloc")]
mod pack;
#[cfg(feature = "alloc")]
mod payload;
#[cfg(feature = "alloc")]
mod privacy;
#[cfg(feature = "std")]
mod restore;
#[cfg(feature = "alloc")]
mod wire;

#[cfg(feature = "alloc")]
pub use generation::{
    encode_generation_footer, GenerationChain, GenerationFooter, GENERATION_FOOTER_LEN,
};

#[cfg(feature = "alloc")]
pub use forensic::{
    encode_forensic_tree, ForensicEntry, ForensicEntryMetadata, ForensicFileType,
    ForensicTimestamp, ForensicTree, ForensicTreeView, ForensicXattr, SparseExtent,
};

#[cfg(feature = "alloc")]
pub use blob::{encode_blob, BlobView};

#[cfg(feature = "alloc")]
pub use codec::{
    encode_codec_bundle, CodecBundleCatalog, CodecBundleError, CodecBundleInput, CodecBundleView,
    CodecFidelity, CodecFidelityKind, CodecImplementation, CodecParameter, CodecParameterValue,
    CodecProfile, ModelProvenance, PccpStatus,
};

#[cfg(feature = "alloc")]
pub use pack::repack_with_frames;

#[cfg(feature = "alloc")]
pub use payload::{encode_dataset_with_payloads, encode_semantic_bundle, SemanticPayloadFrame};

#[cfg(feature = "alloc")]
pub use privacy::{decrypt_bcs2, encrypt_bcs2, EncryptedEnvelopeView, CAP_XCHACHA20_POLY1305};

#[cfg(feature = "std")]
pub use restore::{
    restore_forensic_tree_sandboxed, RestoreError, RestoreMode, RestoreOmission, RestoreReport,
};

#[cfg(feature = "alloc")]
pub use wire::{
    append_dataset_generation, encode_dataset, encode_dataset_with_references,
    encode_generational_dataset, raw_content_id, raw_storage_id, Bcs2Error, Bcs2View, FrameKind,
    FrameView, PrivacyMode, ProfileId, ResourceBounds, RootKind, StorageContract, BCS2_HEADER_LEN,
    BCS2_MAGIC,
};
