#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
mod atom;
#[cfg(feature = "alloc")]
mod canonical;
#[cfg(feature = "alloc")]
mod catalog;
#[cfg(feature = "alloc")]
mod concept;
#[cfg(feature = "alloc")]
mod dataset;
mod exact;
#[cfg(feature = "alloc")]
mod failure;
#[cfg(feature = "alloc")]
mod governance;
mod id;
mod limits;
#[cfg(feature = "alloc")]
mod time;
#[cfg(feature = "alloc")]
mod view;

#[cfg(feature = "alloc")]
pub use atom::{
    Atom, BlobRef, ByteOrder, ElementType, EncodedBlock, Layout, PayloadDescriptor, Presence,
    SignalBlock, Table, TemporalTable, Tensor,
};
#[cfg(feature = "alloc")]
pub use canonical::{canonical_debug_json, logical_content_id};
#[cfg(feature = "alloc")]
pub use catalog::{
    Calibration, CalibrationError, ChannelBasis, ChannelSpec, Clock, CoordinateFrame, Recording,
    ReferenceKind, Stream,
};
#[cfg(feature = "alloc")]
pub use concept::{ConceptError, ConceptId, SourceKey, SourceKeyError};
#[cfg(feature = "alloc")]
pub use dataset::{AbirDataset, DatasetDraft};
pub use exact::{ExactNumber, Rational, RationalError};
#[cfg(feature = "alloc")]
pub use failure::{FailureCode, Severity, ValidationFailure, ValidationReport};
#[cfg(feature = "alloc")]
pub use governance::{
    Derivation, ExecutionRecord, Fidelity, FidelityKind, Policy, Proof, SourceCapsule,
};
pub use id::{
    AtomTag, ChannelBasisTag, ClockTag, ContentId, CoordinateFrameTag, DatasetTag, DerivationTag,
    Handle, ObjectId, ObjectKind, PolicyTag, ProofTag, RecordingTag, SemanticRef, SemanticTag,
    StorageId, StreamTag,
};
pub use limits::ValidationLimits;
#[cfg(feature = "alloc")]
pub use time::{TimeAxis, TimeError, TimeSegment};
#[cfg(feature = "alloc")]
pub use view::{
    BlockView, BorrowedPayload, BorrowedPayloadAccess, InMemoryPayloadAccess, OpenedDataset,
    PayloadAccess, PayloadAccessError, PayloadLease, RecordingView, StreamView, TensorView,
};

/// Package version. This is not the semantic schema version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
