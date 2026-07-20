#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

/// Package version. This is not the semantic schema version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
