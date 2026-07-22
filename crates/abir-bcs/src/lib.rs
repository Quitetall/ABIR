#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
mod wire;

#[cfg(feature = "alloc")]
pub use wire::{
    encode_dataset, encode_dataset_with_references, Bcs2Error, Bcs2View, PrivacyMode, ProfileId,
    ResourceBounds, RootKind, StorageContract, BCS2_HEADER_LEN, BCS2_MAGIC,
};
