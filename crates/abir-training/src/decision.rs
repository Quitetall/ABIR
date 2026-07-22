use crate::{ContentKey, TrainingError, TrainingSpec};
use abir::ContentId;
use abir_bcs::ResourceBounds;
use serde::{Deserialize, Serialize};

const DECISION_SCHEMA: &str = "org.quitetall.abir.training.decision-log-v1";
const DECISION_HASH_DOMAIN: &[u8] = b"org.quitetall.abir.training.decision-log-v1\0";

/// A deterministic global decision captured before worker activation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecisionRecord {
    pub activation_barrier: u64,
    pub decision: ContentKey,
    pub durable_before_activation: bool,
    pub knob: String,
    pub rank: u32,
    pub sequence: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecisionLog {
    records: Vec<DecisionRecord>,
    schema: String,
    sealed: bool,
    spec_id: ContentKey,
}

/// A decision log reopened from its exact canonical byte representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReopenedDecisionLog(DecisionLog);

impl DecisionLog {
    pub fn seal(spec: &TrainingSpec, records: Vec<DecisionRecord>) -> Result<Self, TrainingError> {
        validate_records(spec, &records)?;
        let log = Self {
            records,
            schema: DECISION_SCHEMA.to_owned(),
            sealed: true,
            spec_id: ContentKey::from(spec.content_id()?),
        };
        log.canonical_json()?;
        Ok(log)
    }

    pub fn records(&self) -> &[DecisionRecord] {
        &self.records
    }

    pub const fn spec_id(&self) -> ContentKey {
        self.spec_id
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, TrainingError> {
        self.validate()?;
        let value = serde_json::to_value(self)?;
        let catalog = serde_json::to_vec(&value)?;
        ensure_catalog_bound(&catalog)?;
        Ok(catalog)
    }

    /// Reopens a durable canonical log without treating it as replay evidence.
    pub fn from_canonical_json(catalog: &[u8]) -> Result<ReopenedDecisionLog, TrainingError> {
        ensure_catalog_bound(catalog)?;
        let log: Self = serde_json::from_slice(catalog)?;
        log.validate()?;
        if log.canonical_json()? != catalog {
            return Err(TrainingError::CanonicalDecisionLog);
        }
        Ok(ReopenedDecisionLog(log))
    }

    pub fn content_id(&self) -> Result<ContentId, TrainingError> {
        let mut hasher = blake3::Hasher::new();
        hasher.update(DECISION_HASH_DOMAIN);
        hasher.update(&self.canonical_json()?);
        Ok(ContentId::from_bytes(*hasher.finalize().as_bytes()))
    }

    /// Verifies deterministic replay and returns the stable log identity.
    pub fn replay_identity(
        &self,
        spec: &TrainingSpec,
        replayed: &[DecisionRecord],
    ) -> Result<ContentId, TrainingError> {
        if ContentKey::from(spec.content_id()?) != self.spec_id {
            return Err(TrainingError::DecisionSpecMismatch);
        }
        validate_records(spec, replayed)?;
        if replayed != self.records {
            return Err(TrainingError::DecisionReplayMismatch);
        }
        self.content_id()
    }

    pub(crate) fn validate_for_spec(&self, spec: &TrainingSpec) -> Result<(), TrainingError> {
        if ContentKey::from(spec.content_id()?) != self.spec_id {
            return Err(TrainingError::DecisionSpecMismatch);
        }
        self.validate()?;
        validate_records(spec, &self.records)
    }

    fn validate(&self) -> Result<(), TrainingError> {
        if self.schema != DECISION_SCHEMA || !self.sealed {
            return Err(TrainingError::NotSealed);
        }
        validate_intrinsic_records(&self.records)
    }
}

impl ReopenedDecisionLog {
    pub fn records(&self) -> &[DecisionRecord] {
        self.0.records()
    }

    pub const fn spec_id(&self) -> ContentKey {
        self.0.spec_id()
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, TrainingError> {
        self.0.canonical_json()
    }

    pub fn content_id(&self) -> Result<ContentId, TrainingError> {
        self.0.content_id()
    }

    pub(crate) fn replay_identity(
        &self,
        spec: &TrainingSpec,
        replayed: &[DecisionRecord],
    ) -> Result<ContentId, TrainingError> {
        self.0.replay_identity(spec, replayed)
    }

    pub(crate) fn validate_for_spec(&self, spec: &TrainingSpec) -> Result<(), TrainingError> {
        self.0.validate_for_spec(spec)
    }
}

fn ensure_catalog_bound(catalog: &[u8]) -> Result<(), TrainingError> {
    if catalog.len() > ResourceBounds::default().max_catalog_bytes as usize {
        return Err(TrainingError::AcceptanceResourceBound);
    }
    Ok(())
}

fn validate_records(spec: &TrainingSpec, records: &[DecisionRecord]) -> Result<(), TrainingError> {
    validate_intrinsic_records(records)?;
    for record in records {
        if !spec.allows_adaptive_knob(&record.knob)? {
            return Err(TrainingError::InvalidAdaptiveKnob(record.knob.clone()));
        }
    }
    Ok(())
}

fn validate_intrinsic_records(records: &[DecisionRecord]) -> Result<(), TrainingError> {
    let mut previous_barrier = None;
    for (expected, record) in records.iter().enumerate() {
        let expected = expected as u64;
        if record.sequence != expected {
            return Err(TrainingError::InvalidDecisionSequence {
                expected,
                actual: record.sequence,
            });
        }
        if record.rank != 0 {
            return Err(TrainingError::RankNotZero(record.rank));
        }
        if !record.durable_before_activation {
            return Err(TrainingError::DecisionWasNotDurable);
        }
        if record.knob.is_empty() {
            return Err(TrainingError::InvalidAdaptiveKnob(record.knob.clone()));
        }
        if let Some(previous) = previous_barrier {
            if record.activation_barrier < previous {
                return Err(TrainingError::ActivationBarrierRegression {
                    previous,
                    next: record.activation_barrier,
                });
            }
        }
        previous_barrier = Some(record.activation_barrier);
    }
    Ok(())
}
