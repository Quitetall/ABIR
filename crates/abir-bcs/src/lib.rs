#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
mod generation;
#[cfg(feature = "alloc")]
mod pack;
#[cfg(feature = "alloc")]
mod wire;

#[cfg(feature = "alloc")]
pub use generation::{
    encode_generation_footer, GenerationChain, GenerationFooter, GENERATION_FOOTER_LEN,
};

#[cfg(feature = "alloc")]
pub use pack::repack_with_frames;

#[cfg(feature = "alloc")]
pub use wire::{
    append_dataset_generation, encode_dataset, encode_dataset_with_references,
    encode_generational_dataset, Bcs2Error, Bcs2View, FrameView, PrivacyMode, ProfileId,
    ResourceBounds, RootKind, StorageContract, BCS2_HEADER_LEN, BCS2_MAGIC,
};
