//! Deterministic, source-agnostic ABIR training snapshots.
//!
//! A training snapshot is a sealed semantic catalog whose row payloads are
//! carried as typed BCS2 frames. Opening a snapshot validates the complete
//! catalog/frame closure and then lends the original frame bytes without a
//! copy.

mod continual;
mod decision;
mod error;
mod model;
mod store;

pub use continual::{
    ClosedSubscription, DatasetSubscription, MicroSnapshot, SubscriptionCorrection,
};
pub use decision::{DecisionLog, DecisionRecord};
pub use error::TrainingError;
pub use model::{
    encode_snapshot, ContentKey, TrainingInput, TrainingProfile, TrainingRow, TrainingSnapshot,
    TrainingSpec,
};
pub use store::{TrainingRowLease, TrainingWindowStore};
