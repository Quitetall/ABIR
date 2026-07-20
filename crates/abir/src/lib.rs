#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
mod concept;
mod exact;
#[cfg(feature = "alloc")]
mod failure;
mod id;
mod limits;

#[cfg(feature = "alloc")]
pub use concept::{ConceptError, ConceptId, SourceKey, SourceKeyError};
pub use exact::{ExactNumber, Rational, RationalError};
#[cfg(feature = "alloc")]
pub use failure::{FailureCode, Severity, ValidationFailure, ValidationReport};
pub use id::{
    AtomTag, ChannelBasisTag, ClockTag, ContentId, DatasetTag, DerivationTag, Handle, ObjectId,
    PolicyTag, ProofTag, RecordingTag, StorageId, StreamTag,
};
pub use limits::ValidationLimits;

/// Package version. This is not the semantic schema version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
