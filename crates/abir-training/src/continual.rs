use crate::{ContentKey, TrainingError};
use abir::ContentId;
use abir_bcs::ResourceBounds;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const SUBSCRIPTION_SCHEMA: &str = "org.quitetall.abir.training.subscription-v1";
const SUBSCRIPTION_HASH_DOMAIN: &[u8] = b"org.quitetall.abir.training.subscription-v1\0";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SubscriptionCorrection {
    pub prior_generation: u64,
    pub prior_snapshot_id: ContentKey,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MicroSnapshot {
    pub correction: Option<SubscriptionCorrection>,
    pub generation: u64,
    pub logical_id: ContentKey,
    pub sequence: u64,
    pub snapshot_id: ContentKey,
    pub watermark: u64,
}

#[derive(Clone, Copy, Debug)]
struct GenerationState {
    generation: u64,
    snapshot_id: ContentKey,
}

/// A mutable intake cursor. Closing it yields a sealed immutable identity.
#[derive(Debug)]
pub struct DatasetSubscription {
    closed: bool,
    events: Vec<MicroSnapshot>,
    latest: BTreeMap<ContentKey, GenerationState>,
    subscription_id: ContentKey,
    watermark: Option<u64>,
}

impl DatasetSubscription {
    pub fn new(subscription_id: ContentKey) -> Self {
        Self {
            closed: false,
            events: Vec::new(),
            latest: BTreeMap::new(),
            subscription_id,
            watermark: None,
        }
    }

    pub fn append(&mut self, snapshot: MicroSnapshot) -> Result<(), TrainingError> {
        if self.closed {
            return Err(TrainingError::ClosedSubscription);
        }
        let expected = self.events.len() as u64;
        if snapshot.sequence != expected {
            return Err(TrainingError::InvalidSubscriptionSequence {
                expected,
                actual: snapshot.sequence,
            });
        }
        if let Some(previous) = self.watermark {
            if snapshot.watermark < previous {
                return Err(TrainingError::NonMonotonicWatermark {
                    previous,
                    next: snapshot.watermark,
                });
            }
        }

        match (self.latest.get(&snapshot.logical_id), snapshot.correction) {
            (None, None) if snapshot.generation == 0 => {}
            (None, Some(_)) => {
                return Err(TrainingError::UnknownCorrection(
                    snapshot.logical_id.content_id(),
                ))
            }
            (None, None) => return Err(TrainingError::CorrectionGeneration),
            (Some(_), None) => return Err(TrainingError::CorrectionGeneration),
            (Some(previous), Some(correction))
                if correction.prior_generation == previous.generation
                    && correction.prior_snapshot_id == previous.snapshot_id
                    && snapshot.generation == previous.generation + 1 => {}
            (Some(_), Some(_)) => return Err(TrainingError::CorrectionGeneration),
        }

        self.watermark = Some(snapshot.watermark);
        self.latest.insert(
            snapshot.logical_id,
            GenerationState {
                generation: snapshot.generation,
                snapshot_id: snapshot.snapshot_id,
            },
        );
        self.events.push(snapshot);
        Ok(())
    }

    pub fn close(mut self) -> Result<ClosedSubscription, TrainingError> {
        self.closed = true;
        ClosedSubscription::seal(self.subscription_id, self.events)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClosedSubscription {
    events: Vec<MicroSnapshot>,
    schema: String,
    sealed: bool,
    subscription_id: ContentKey,
}

impl ClosedSubscription {
    fn seal(
        subscription_id: ContentKey,
        events: Vec<MicroSnapshot>,
    ) -> Result<Self, TrainingError> {
        let closed = Self {
            events,
            schema: SUBSCRIPTION_SCHEMA.to_owned(),
            sealed: true,
            subscription_id,
        };
        // Replaying through the public state machine proves the stored event
        // sequence satisfies ordering, watermark, and correction invariants.
        let mut replay = DatasetSubscription::new(subscription_id);
        for event in &closed.events {
            replay.append(event.clone())?;
        }
        closed.canonical_json()?;
        Ok(closed)
    }

    pub fn events(&self) -> &[MicroSnapshot] {
        &self.events
    }

    /// Reopens a canonical closed sequence and replays every ordering rule.
    pub fn from_canonical_json(catalog: &[u8]) -> Result<Self, TrainingError> {
        ensure_catalog_bound(catalog)?;
        let subscription: Self = serde_json::from_slice(catalog)?;
        subscription.validate()?;
        if subscription.canonical_json()? != catalog {
            return Err(TrainingError::CanonicalSubscription);
        }
        Ok(subscription)
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, TrainingError> {
        self.validate()?;
        let value = serde_json::to_value(self)?;
        let catalog = serde_json::to_vec(&value)?;
        ensure_catalog_bound(&catalog)?;
        Ok(catalog)
    }

    pub fn content_id(&self) -> Result<ContentId, TrainingError> {
        let mut hasher = blake3::Hasher::new();
        hasher.update(SUBSCRIPTION_HASH_DOMAIN);
        hasher.update(&self.canonical_json()?);
        Ok(ContentId::from_bytes(*hasher.finalize().as_bytes()))
    }

    fn validate(&self) -> Result<(), TrainingError> {
        if self.schema != SUBSCRIPTION_SCHEMA || !self.sealed {
            return Err(TrainingError::NotSealed);
        }
        let mut replay = DatasetSubscription::new(self.subscription_id);
        for event in &self.events {
            replay.append(event.clone())?;
        }
        Ok(())
    }
}

fn ensure_catalog_bound(catalog: &[u8]) -> Result<(), TrainingError> {
    if catalog.len() > ResourceBounds::default().max_catalog_bytes as usize {
        return Err(TrainingError::AcceptanceResourceBound);
    }
    Ok(())
}
