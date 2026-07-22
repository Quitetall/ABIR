//! Deterministic, source-agnostic ABIR training snapshots.
//!
//! A training snapshot is a sealed semantic catalog whose row payloads are
//! carried as typed BCS2 frames. Opening a snapshot validates the complete
//! catalog/frame closure and then lends the original frame bytes without a
//! copy.

mod acceptance;
mod compiler;
mod continual;
mod decision;
mod error;
mod model;
mod store;

pub use acceptance::{
    ContinualPromotion, ContinualPromotionEntry, DecisionReplayReceipt, SourceEquivalenceReceipt,
    VerifiedTrainingSnapshot,
};
pub use compiler::{
    compile_execution_plan, CacheBudget, ClosurePolicy, CompiledExecutionPlan, PayloadAccessPolicy,
    PlanCompileError, PlanOverrides, PrefetchPolicy, RowGrouping,
};
pub use continual::{
    ClosedSubscription, DatasetSubscription, MicroSnapshot, SubscriptionCorrection,
};
pub use decision::{DecisionLog, DecisionRecord, ReopenedDecisionLog};
pub use error::TrainingError;
pub use model::{
    encode_snapshot, ContentKey, TrainingAssociatedPayload, TrainingInput,
    TrainingLabelPayloadAssociation, TrainingProfile, TrainingRow, TrainingSnapshot, TrainingSpec,
};
pub use store::{
    DecisionLogReplayState, TrainingLabelPayloadLease, TrainingRowLease, TrainingWindowStore,
};
