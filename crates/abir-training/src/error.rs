use abir::ContentId;
use core::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrainingError {
    ActivationBarrierRegression { previous: u64, next: u64 },
    Bcs2(String),
    CanonicalCatalog,
    ClosedSubscription,
    ContentIdMismatch,
    CorrectionGeneration,
    DecisionReplayMismatch,
    DecisionSpecMismatch,
    DecisionWasNotDurable,
    DuplicateDatasetRoot(ContentId),
    DuplicateLogicalRow(ContentId),
    DuplicatePayload(ContentId),
    ExternalReference(ContentId),
    ExtraPayload(ContentId),
    InvalidAdaptiveKnob(String),
    InvalidAuthorizedPurpose,
    InvalidContentKey,
    InvalidDecisionSequence { expected: u64, actual: u64 },
    InvalidElement(String),
    InvalidProfile,
    InvalidRowExtent(ContentId),
    InvalidSnapshot,
    InvalidSubscriptionSequence { expected: u64, actual: u64 },
    MissingPayload(ContentId),
    NonMonotonicWatermark { previous: u64, next: u64 },
    NotBundle,
    NotSealed,
    ProfileMismatch,
    RankNotZero(u32),
    Serialization(String),
    UnknownCorrection(ContentId),
}

impl fmt::Display for TrainingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ActivationBarrierRegression { previous, next } => {
                write!(f, "activation barrier regressed from {previous} to {next}")
            }
            Self::Bcs2(error) => write!(f, "BCS2 error: {error}"),
            Self::CanonicalCatalog => f.write_str("training catalog is not canonical JSON"),
            Self::ClosedSubscription => f.write_str("dataset subscription is already closed"),
            Self::ContentIdMismatch => f.write_str("training content identity mismatch"),
            Self::CorrectionGeneration => f.write_str("correction must create the next generation"),
            Self::DecisionReplayMismatch => f.write_str("decision replay identity mismatch"),
            Self::DecisionSpecMismatch => {
                f.write_str("decision log does not match the declared training spec")
            }
            Self::DecisionWasNotDurable => {
                f.write_str("decision was not durable before its activation barrier")
            }
            Self::DuplicateDatasetRoot(id) => write!(f, "duplicate dataset root {id}"),
            Self::DuplicateLogicalRow(id) => write!(f, "duplicate logical row {id}"),
            Self::DuplicatePayload(id) => write!(f, "conflicting duplicate payload {id}"),
            Self::ExternalReference(id) => write!(f, "undeclared external reference {id}"),
            Self::ExtraPayload(id) => write!(f, "extra payload frame {id}"),
            Self::InvalidAdaptiveKnob(knob) => write!(f, "invalid adaptive knob {knob:?}"),
            Self::InvalidAuthorizedPurpose => f.write_str("invalid authorized purpose"),
            Self::InvalidContentKey => f.write_str("invalid 64-digit lowercase content key"),
            Self::InvalidDecisionSequence { expected, actual } => write!(
                f,
                "decision sequence is not consecutive: expected {expected}, got {actual}"
            ),
            Self::InvalidElement(element) => write!(f, "unknown element type {element}"),
            Self::InvalidProfile => f.write_str("not a registered training profile"),
            Self::InvalidRowExtent(id) => write!(f, "invalid logical extent for row {id}"),
            Self::InvalidSnapshot => f.write_str("invalid sealed training snapshot"),
            Self::InvalidSubscriptionSequence { expected, actual } => write!(
                f,
                "micro-snapshot sequence is not consecutive: expected {expected}, got {actual}"
            ),
            Self::MissingPayload(id) => write!(f, "missing payload frame {id}"),
            Self::NonMonotonicWatermark { previous, next } => {
                write!(f, "watermark regressed from {previous} to {next}")
            }
            Self::NotBundle => f.write_str("BCS2 root is not a bundle"),
            Self::NotSealed => f.write_str("training snapshot is not sealed"),
            Self::ProfileMismatch => f.write_str("catalog and BCS2 profiles differ"),
            Self::RankNotZero(rank) => {
                write!(f, "pre-activation decision was recorded on rank {rank}")
            }
            Self::Serialization(error) => write!(f, "serialization error: {error}"),
            Self::UnknownCorrection(id) => write!(f, "correction has no prior generation for {id}"),
        }
    }
}

impl std::error::Error for TrainingError {}

impl From<serde_json::Error> for TrainingError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization(value.to_string())
    }
}

impl From<abir_bcs::Bcs2Error> for TrainingError {
    fn from(value: abir_bcs::Bcs2Error) -> Self {
        Self::Bcs2(format!("{value:?}"))
    }
}
