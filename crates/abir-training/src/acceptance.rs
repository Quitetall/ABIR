use crate::{
    ClosedSubscription, ContentKey, DecisionRecord, ReopenedDecisionLog, TrainingError,
    TrainingSnapshot, TrainingSpec, TrainingWindowStore,
};
use abir::ContentId;
use abir_bcs::ResourceBounds;
use serde::Serialize;

const REPLAY_RECEIPT_SCHEMA: &str = "org.quitetall.abir.training.decision-replay-receipt-v1";
const REPLAY_RECEIPT_HASH_DOMAIN: &[u8] =
    b"org.quitetall.abir.training.decision-replay-receipt-v1\0";
const SOURCE_EQUIVALENCE_SCHEMA: &str = "org.quitetall.abir.training.source-equivalence-receipt-v1";
const SOURCE_EQUIVALENCE_HASH_DOMAIN: &[u8] =
    b"org.quitetall.abir.training.source-equivalence-receipt-v1\0";
const CONTINUAL_PROMOTION_SCHEMA: &str = "org.quitetall.abir.training.continual-promotion-v1";
const CONTINUAL_PROMOTION_HASH_DOMAIN: &[u8] =
    b"org.quitetall.abir.training.continual-promotion-v1\0";

/// Evidence that one exact, durable decision log replayed under its bound spec.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DecisionReplayReceipt {
    decision_log_id: ContentKey,
    record_count: u64,
    schema: String,
    spec_id: ContentKey,
    verified: bool,
}

impl DecisionReplayReceipt {
    pub fn verify(
        spec: &TrainingSpec,
        log: &ReopenedDecisionLog,
        replayed: &[DecisionRecord],
    ) -> Result<Self, TrainingError> {
        let decision_log_id = ContentKey::from(log.replay_identity(spec, replayed)?);
        Ok(Self {
            decision_log_id,
            record_count: replayed
                .len()
                .try_into()
                .map_err(|_| TrainingError::InvalidDecisionReplayReceipt)?,
            schema: REPLAY_RECEIPT_SCHEMA.to_owned(),
            spec_id: ContentKey::from(spec.content_id()?),
            verified: true,
        })
    }

    pub const fn decision_log_id(&self) -> ContentKey {
        self.decision_log_id
    }

    pub const fn spec_id(&self) -> ContentKey {
        self.spec_id
    }

    pub const fn record_count(&self) -> u64 {
        self.record_count
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, TrainingError> {
        self.validate()?;
        canonical_json(self)
    }

    pub fn content_id(&self) -> Result<ContentId, TrainingError> {
        hash(REPLAY_RECEIPT_HASH_DOMAIN, &self.canonical_json()?)
    }

    fn validate(&self) -> Result<(), TrainingError> {
        if self.schema != REPLAY_RECEIPT_SCHEMA || !self.verified {
            return Err(TrainingError::InvalidDecisionReplayReceipt);
        }
        Ok(())
    }
}

/// Evidence that two independently opened stores expose the same logical windows.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceEquivalenceReceipt {
    first_dataset_root_count: u64,
    first_dataset_roots_id: ContentKey,
    first_snapshot_id: ContentKey,
    logical_windows_id: ContentKey,
    row_count: u64,
    schema: String,
    second_dataset_root_count: u64,
    second_dataset_roots_id: ContentKey,
    second_snapshot_id: ContentKey,
    verified: bool,
}

impl SourceEquivalenceReceipt {
    pub fn verify(
        first: &TrainingWindowStore<'_>,
        second: &TrainingWindowStore<'_>,
    ) -> Result<Self, TrainingError> {
        if first.spec_id() != second.spec_id()
            || first.snapshot().profile() != second.snapshot().profile()
            || first.decision_log_id() != second.decision_log_id()
            || first.snapshot().rows() != second.snapshot().rows()
            || first.snapshot().label_payloads() != second.snapshot().label_payloads()
            || first.rows().len() != second.rows().len()
        {
            return Err(TrainingError::SourceSnapshotMismatch);
        }
        for (left, right) in first.rows().zip(second.rows()) {
            if left.metadata() != right.metadata() || left.bytes() != right.bytes() {
                return Err(TrainingError::SourceSnapshotMismatch);
            }
        }
        let first_snapshot_id = ContentKey::from(first.snapshot_id()?);
        let second_snapshot_id = ContentKey::from(second.snapshot_id()?);
        let logical_windows_id = logical_windows_id(first.snapshot())?;
        Ok(Self {
            first_dataset_root_count: first
                .dataset_roots()
                .len()
                .try_into()
                .map_err(|_| TrainingError::InvalidSourceEquivalenceReceipt)?,
            first_dataset_roots_id: hash_keys(
                b"org.quitetall.abir.training.dataset-root-set-v1\0",
                first.dataset_roots(),
            )?,
            first_snapshot_id,
            logical_windows_id,
            second_dataset_root_count: second
                .dataset_roots()
                .len()
                .try_into()
                .map_err(|_| TrainingError::InvalidSourceEquivalenceReceipt)?,
            row_count: first
                .rows()
                .len()
                .try_into()
                .map_err(|_| TrainingError::InvalidSourceEquivalenceReceipt)?,
            schema: SOURCE_EQUIVALENCE_SCHEMA.to_owned(),
            second_dataset_roots_id: hash_keys(
                b"org.quitetall.abir.training.dataset-root-set-v1\0",
                second.dataset_roots(),
            )?,
            second_snapshot_id,
            verified: true,
        })
    }

    pub const fn first_snapshot_id(&self) -> ContentKey {
        self.first_snapshot_id
    }

    pub const fn second_snapshot_id(&self) -> ContentKey {
        self.second_snapshot_id
    }

    pub const fn first_dataset_roots_id(&self) -> ContentKey {
        self.first_dataset_roots_id
    }

    pub const fn second_dataset_roots_id(&self) -> ContentKey {
        self.second_dataset_roots_id
    }

    pub const fn logical_windows_id(&self) -> ContentKey {
        self.logical_windows_id
    }

    pub const fn first_dataset_root_count(&self) -> u64 {
        self.first_dataset_root_count
    }

    pub const fn second_dataset_root_count(&self) -> u64 {
        self.second_dataset_root_count
    }

    pub const fn row_count(&self) -> u64 {
        self.row_count
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, TrainingError> {
        self.validate()?;
        canonical_json(self)
    }

    pub fn content_id(&self) -> Result<ContentId, TrainingError> {
        hash(SOURCE_EQUIVALENCE_HASH_DOMAIN, &self.canonical_json()?)
    }

    fn validate(&self) -> Result<(), TrainingError> {
        if self.schema != SOURCE_EQUIVALENCE_SCHEMA || !self.verified || self.row_count == 0 {
            return Err(TrainingError::InvalidSourceEquivalenceReceipt);
        }
        Ok(())
    }
}

/// One ordered, artifact-verified continual-training promotion entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ContinualPromotionEntry {
    pub decision_log_id: ContentKey,
    pub decision_replay_receipt_id: ContentKey,
    pub generation: u64,
    pub logical_id: ContentKey,
    pub sequence: u64,
    pub snapshot_id: ContentKey,
    pub watermark: u64,
}

/// Snapshot metadata cloned only after a complete BCS2 store verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedTrainingSnapshot(TrainingSnapshot);

impl VerifiedTrainingSnapshot {
    pub(crate) fn from_store(store: &TrainingWindowStore<'_>) -> Self {
        Self(store.snapshot().clone())
    }

    pub fn snapshot(&self) -> &TrainingSnapshot {
        &self.0
    }
}

/// A closed subscription plus the exact snapshots and decision logs it promotes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ContinualPromotion {
    closed_subscription_id: ContentKey,
    entries: Vec<ContinualPromotionEntry>,
    schema: String,
    sealed: bool,
    spec_id: ContentKey,
}

impl ContinualPromotion {
    pub fn seal(
        subscription: &ClosedSubscription,
        spec: &TrainingSpec,
        snapshots: &[VerifiedTrainingSnapshot],
        decision_logs: &[ReopenedDecisionLog],
        replay_receipts: &[DecisionReplayReceipt],
    ) -> Result<Self, TrainingError> {
        let expected = subscription.events().len();
        if expected > ResourceBounds::default().max_generations as usize {
            return Err(TrainingError::AcceptanceResourceBound);
        }
        if expected == 0 {
            return Err(TrainingError::EmptyContinualPromotion);
        }
        if snapshots.len() != expected
            || decision_logs.len() != expected
            || replay_receipts.len() != expected
        {
            return Err(TrainingError::IncompleteContinualPromotion {
                expected,
                snapshots: snapshots.len(),
                decision_logs: decision_logs.len(),
                replay_receipts: replay_receipts.len(),
            });
        }
        let spec_id = ContentKey::from(spec.content_id()?);
        let mut entries = Vec::with_capacity(expected);
        for (((event, verified_snapshot), decision_log), replay_receipt) in subscription
            .events()
            .iter()
            .zip(snapshots)
            .zip(decision_logs)
            .zip(replay_receipts)
        {
            let snapshot = verified_snapshot.snapshot();
            if snapshot.content_id()? != event.snapshot_id.content_id() {
                return Err(TrainingError::PromotionSnapshotMismatch(event.sequence));
            }
            if snapshot.spec_id() != spec_id {
                return Err(TrainingError::DecisionSpecMismatch);
            }
            decision_log.validate_for_spec(spec)?;
            let decision_log_id = ContentKey::from(decision_log.content_id()?);
            if snapshot.decision_log_id() != decision_log_id {
                return Err(TrainingError::PromotionDecisionLogMismatch(event.sequence));
            }
            if replay_receipt.decision_log_id() != decision_log_id
                || replay_receipt.spec_id() != spec_id
            {
                return Err(TrainingError::PromotionDecisionReplayMismatch(
                    event.sequence,
                ));
            }
            entries.push(ContinualPromotionEntry {
                decision_log_id,
                decision_replay_receipt_id: ContentKey::from(replay_receipt.content_id()?),
                generation: event.generation,
                logical_id: event.logical_id,
                sequence: event.sequence,
                snapshot_id: event.snapshot_id,
                watermark: event.watermark,
            });
        }
        let promotion = Self {
            closed_subscription_id: ContentKey::from(subscription.content_id()?),
            entries,
            schema: CONTINUAL_PROMOTION_SCHEMA.to_owned(),
            sealed: true,
            spec_id,
        };
        promotion.validate()?;
        Ok(promotion)
    }

    pub const fn closed_subscription_id(&self) -> ContentKey {
        self.closed_subscription_id
    }

    pub const fn spec_id(&self) -> ContentKey {
        self.spec_id
    }

    pub fn entries(&self) -> &[ContinualPromotionEntry] {
        &self.entries
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, TrainingError> {
        self.validate()?;
        canonical_json(self)
    }

    pub fn content_id(&self) -> Result<ContentId, TrainingError> {
        hash(CONTINUAL_PROMOTION_HASH_DOMAIN, &self.canonical_json()?)
    }

    fn validate(&self) -> Result<(), TrainingError> {
        if self.schema != CONTINUAL_PROMOTION_SCHEMA
            || !self.sealed
            || self.entries.is_empty()
            || self
                .entries
                .iter()
                .enumerate()
                .any(|(expected, entry)| entry.sequence != expected as u64)
        {
            return Err(TrainingError::InvalidContinualPromotion);
        }
        Ok(())
    }
}

fn hash(domain: &[u8], canonical: &[u8]) -> Result<ContentId, TrainingError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(canonical);
    Ok(ContentId::from_bytes(*hasher.finalize().as_bytes()))
}

fn hash_keys(domain: &[u8], keys: &[ContentKey]) -> Result<ContentKey, TrainingError> {
    Ok(ContentKey::from(hash(domain, &canonical_json(keys)?)?))
}

fn logical_windows_id(snapshot: &TrainingSnapshot) -> Result<ContentKey, TrainingError> {
    let canonical = canonical_json(&(
        snapshot.spec_id(),
        snapshot.profile(),
        snapshot.decision_log_id(),
        snapshot.rows(),
        snapshot.label_payloads(),
    ))?;
    Ok(ContentKey::from(hash(
        b"org.quitetall.abir.training.logical-window-set-v1\0",
        &canonical,
    )?))
}

fn canonical_json<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, TrainingError> {
    Ok(serde_json::to_vec(&serde_json::to_value(value)?)?)
}
