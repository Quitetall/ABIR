//! Deterministic, source-agnostic ABIR training snapshots.
//!
//! A training snapshot is a sealed semantic catalog whose row payloads are
//! carried as typed BCS2 frames. Opening a snapshot validates the complete
//! catalog/frame closure and then lends the original frame bytes without a
//! copy.

mod acceptance;
mod canonical;
mod compiler;
mod continual;
mod decision;
mod error;
mod identity;
mod model;
mod program;
mod store;

pub use acceptance::{
    ContinualPromotion, ContinualPromotionEntry, DecisionReplayReceipt, SourceEquivalenceReceipt,
    VerifiedTrainingSnapshot,
};
pub use compiler::{
    compile_execution_plan, CacheBudget, ClosurePolicy, CompiledExecutionPlan, PayloadAccessPolicy,
    PlanCompileError, PlanOverrides, PrefetchPolicy, RowGrouping, TrainingExecutionDecision,
};
pub use continual::{
    ClosedSubscription, DatasetSubscription, MicroSnapshot, SubscriptionCorrection,
};
pub use decision::{DecisionLog, DecisionRecord, ReopenedDecisionLog};
pub use error::TrainingError;
pub use model::{
    encode_snapshot, ContentKey, TrainingAssociatedPayload, TrainingInput,
    TrainingLabelPayloadAssociation, TrainingProfile, TrainingRow, TrainingRowEncoding,
    TrainingSnapshot, TrainingSpec,
};
pub use program::{
    CompiledTrainingEpoch, CompiledTrainingRow, SamplerStrategy, SamplerStratum, SamplerStratumKey,
    TrainingProgram, TrainingSampler, TrainingSemanticDescriptor, TrainingSemanticRole,
};
pub use store::{
    DecisionLogReplayState, TrainingFileLabelPayload, TrainingFileRow, TrainingLabelPayloadLease,
    TrainingRowLease, TrainingWindowFileIndex, TrainingWindowStore,
};
