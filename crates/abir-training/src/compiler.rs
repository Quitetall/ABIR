use crate::TrainingProfile;
use abir::ContentId;
use serde::{Deserialize, Serialize};
use std::fmt;

const PLAN_SCHEMA: &str = "org.quitetall.abir.training.execution-plan-v1";
const PLAN_HASH_DOMAIN: &[u8] = b"org.quitetall.abir.training.execution-plan-v1\0";
const MIN_TARGET_GROUP_BYTES: u64 = 4 * 1024;
const MAX_TARGET_GROUP_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ROWS_PER_GROUP: u32 = 65_536;
const MAX_PREFETCH_ROWS: u32 = 4_096;
const MAX_CACHE_BYTES: u64 = 16 * 1024 * 1024 * 1024;

/// Deterministic grouping of logical rows into physical read work.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RowGrouping {
    FixedRows { rows: u32 },
    TargetBytes { bytes: u64 },
}

/// Bounded look-ahead performed by a reader.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PrefetchPolicy {
    Disabled,
    Rows { rows: u32 },
}

/// How payload bytes are exposed to a training consumer.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PayloadAccessPolicy {
    PreferMmap,
    RequireMmap,
    Materialize,
    Stream,
}

/// Whether a physical plan is a self-contained closure.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClosurePolicy {
    Portable,
    AllowVerifiedExternalReferences,
}

impl ClosurePolicy {
    pub const fn allows_external_references(self) -> bool {
        matches!(self, Self::AllowVerifiedExternalReferences)
    }
}

/// Validated execution-only cache allocation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CacheBudget {
    bytes: u64,
}

impl CacheBudget {
    pub const fn bytes(self) -> u64 {
        self.bytes
    }

    pub fn new(bytes: u64) -> Result<Self, PlanCompileError> {
        if bytes > MAX_CACHE_BYTES {
            return Err(PlanCompileError::CacheBudgetOutOfRange(bytes));
        }
        Ok(Self { bytes })
    }
}

/// Optional deterministic overrides supplied by a declared training job.
///
/// Runtime hardware observations are deliberately absent. They may select an
/// implementation of this plan, but never alter its identity.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlanOverrides {
    pub row_grouping: Option<RowGrouping>,
    pub prefetch: Option<PrefetchPolicy>,
    pub payload_access: Option<PayloadAccessPolicy>,
    pub cache_budget: Option<CacheBudget>,
    pub closure: Option<ClosurePolicy>,
}

/// A sealed physical execution plan. It changes execution, never snapshot
/// semantics or the BCS2 wire representation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompiledExecutionPlan {
    cache_budget: CacheBudget,
    closure: ClosurePolicy,
    payload_access: PayloadAccessPolicy,
    prefetch: PrefetchPolicy,
    profile: TrainingProfile,
    row_grouping: RowGrouping,
    schema: String,
}

impl CompiledExecutionPlan {
    pub const fn profile(&self) -> TrainingProfile {
        self.profile
    }

    pub const fn row_grouping(&self) -> RowGrouping {
        self.row_grouping
    }

    pub const fn prefetch(&self) -> PrefetchPolicy {
        self.prefetch
    }

    pub const fn payload_access(&self) -> PayloadAccessPolicy {
        self.payload_access
    }

    pub const fn cache_budget(&self) -> CacheBudget {
        self.cache_budget
    }

    pub const fn closure(&self) -> ClosurePolicy {
        self.closure
    }

    pub const fn allows_external_references(&self) -> bool {
        self.closure.allows_external_references()
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, PlanCompileError> {
        self.validate()?;
        let value = serde_json::to_value(self)
            .map_err(|error| PlanCompileError::Serialization(error.to_string()))?;
        serde_json::to_vec(&value)
            .map_err(|error| PlanCompileError::Serialization(error.to_string()))
    }

    pub fn content_id(&self) -> Result<ContentId, PlanCompileError> {
        let mut hasher = blake3::Hasher::new();
        hasher.update(PLAN_HASH_DOMAIN);
        hasher.update(&self.canonical_json()?);
        Ok(ContentId::from_bytes(*hasher.finalize().as_bytes()))
    }

    fn validate(&self) -> Result<(), PlanCompileError> {
        if self.schema != PLAN_SCHEMA {
            return Err(PlanCompileError::InvalidSchema);
        }
        validate_row_grouping(self.row_grouping)?;
        validate_prefetch(self.prefetch)?;
        if self.cache_budget.bytes > MAX_CACHE_BYTES {
            return Err(PlanCompileError::CacheBudgetOutOfRange(
                self.cache_budget.bytes,
            ));
        }
        if matches!(self.payload_access, PayloadAccessPolicy::Materialize)
            && self.cache_budget.bytes == 0
        {
            return Err(PlanCompileError::MaterializationRequiresCache);
        }
        if is_portable_profile(self.profile) && self.closure.allows_external_references() {
            return Err(PlanCompileError::PortableProfileExternalReferences(
                self.profile,
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanCompileError {
    CacheBudgetOutOfRange(u64),
    InvalidSchema,
    MaterializationRequiresCache,
    PortableProfileExternalReferences(TrainingProfile),
    PrefetchRowsOutOfRange(u32),
    RowGroupBytesOutOfRange(u64),
    RowGroupRowsOutOfRange(u32),
    Serialization(String),
}

impl fmt::Display for PlanCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CacheBudgetOutOfRange(bytes) => {
                write!(
                    formatter,
                    "cache budget {bytes} exceeds the execution limit"
                )
            }
            Self::InvalidSchema => formatter.write_str("invalid execution plan schema"),
            Self::MaterializationRequiresCache => {
                formatter.write_str("materialization requires a nonzero cache budget")
            }
            Self::PortableProfileExternalReferences(profile) => write!(
                formatter,
                "portable profile {profile:?} cannot allow external references"
            ),
            Self::PrefetchRowsOutOfRange(rows) => {
                write!(formatter, "prefetch row count {rows} is out of range")
            }
            Self::RowGroupBytesOutOfRange(bytes) => {
                write!(formatter, "row group target {bytes} bytes is out of range")
            }
            Self::RowGroupRowsOutOfRange(rows) => {
                write!(formatter, "row group row count {rows} is out of range")
            }
            Self::Serialization(error) => write!(formatter, "serialization error: {error}"),
        }
    }
}

impl std::error::Error for PlanCompileError {}

/// Compile one of the six registered training profiles into a deterministic
/// physical plan. The compiler has no hardware-observation input by design.
pub fn compile_execution_plan(
    profile: TrainingProfile,
    overrides: PlanOverrides,
) -> Result<CompiledExecutionPlan, PlanCompileError> {
    let mut plan = default_plan(profile)?;
    if let Some(value) = overrides.row_grouping {
        plan.row_grouping = value;
    }
    if let Some(value) = overrides.prefetch {
        plan.prefetch = value;
    }
    if let Some(value) = overrides.payload_access {
        plan.payload_access = value;
    }
    if let Some(value) = overrides.cache_budget {
        plan.cache_budget = value;
    }
    if let Some(value) = overrides.closure {
        plan.closure = value;
    }
    plan.validate()?;
    Ok(plan)
}

fn default_plan(profile: TrainingProfile) -> Result<CompiledExecutionPlan, PlanCompileError> {
    let (row_grouping, prefetch, payload_access, cache_bytes, closure) = match profile {
        TrainingProfile::Speed => (
            RowGrouping::TargetBytes {
                bytes: 64 * 1024 * 1024,
            },
            PrefetchPolicy::Rows { rows: 64 },
            PayloadAccessPolicy::RequireMmap,
            1024 * 1024 * 1024,
            ClosurePolicy::AllowVerifiedExternalReferences,
        ),
        TrainingProfile::Balanced => (
            RowGrouping::TargetBytes {
                bytes: 16 * 1024 * 1024,
            },
            PrefetchPolicy::Rows { rows: 16 },
            PayloadAccessPolicy::PreferMmap,
            256 * 1024 * 1024,
            ClosurePolicy::AllowVerifiedExternalReferences,
        ),
        TrainingProfile::Memory => (
            RowGrouping::FixedRows { rows: 1 },
            PrefetchPolicy::Rows { rows: 1 },
            PayloadAccessPolicy::PreferMmap,
            32 * 1024 * 1024,
            ClosurePolicy::AllowVerifiedExternalReferences,
        ),
        TrainingProfile::Compact => (
            RowGrouping::TargetBytes {
                bytes: 8 * 1024 * 1024,
            },
            PrefetchPolicy::Rows { rows: 4 },
            PayloadAccessPolicy::Materialize,
            128 * 1024 * 1024,
            ClosurePolicy::Portable,
        ),
        TrainingProfile::UltraCompact => (
            RowGrouping::TargetBytes {
                bytes: 64 * 1024 * 1024,
            },
            PrefetchPolicy::Disabled,
            PayloadAccessPolicy::Stream,
            16 * 1024 * 1024,
            ClosurePolicy::Portable,
        ),
        TrainingProfile::Stream => (
            RowGrouping::FixedRows { rows: 1 },
            PrefetchPolicy::Rows { rows: 2 },
            PayloadAccessPolicy::Stream,
            8 * 1024 * 1024,
            ClosurePolicy::AllowVerifiedExternalReferences,
        ),
    };
    let plan = CompiledExecutionPlan {
        cache_budget: CacheBudget::new(cache_bytes)?,
        closure,
        payload_access,
        prefetch,
        profile,
        row_grouping,
        schema: PLAN_SCHEMA.to_owned(),
    };
    plan.validate()?;
    Ok(plan)
}

fn is_portable_profile(profile: TrainingProfile) -> bool {
    matches!(
        profile,
        TrainingProfile::Compact | TrainingProfile::UltraCompact
    )
}

fn validate_row_grouping(grouping: RowGrouping) -> Result<(), PlanCompileError> {
    match grouping {
        RowGrouping::FixedRows { rows } if (1..=MAX_ROWS_PER_GROUP).contains(&rows) => Ok(()),
        RowGrouping::FixedRows { rows } => Err(PlanCompileError::RowGroupRowsOutOfRange(rows)),
        RowGrouping::TargetBytes { bytes }
            if (MIN_TARGET_GROUP_BYTES..=MAX_TARGET_GROUP_BYTES).contains(&bytes) =>
        {
            Ok(())
        }
        RowGrouping::TargetBytes { bytes } => Err(PlanCompileError::RowGroupBytesOutOfRange(bytes)),
    }
}

fn validate_prefetch(prefetch: PrefetchPolicy) -> Result<(), PlanCompileError> {
    match prefetch {
        PrefetchPolicy::Disabled => Ok(()),
        PrefetchPolicy::Rows { rows } if (1..=MAX_PREFETCH_ROWS).contains(&rows) => Ok(()),
        PrefetchPolicy::Rows { rows } => Err(PlanCompileError::PrefetchRowsOutOfRange(rows)),
    }
}
