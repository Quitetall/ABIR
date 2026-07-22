use crate::{
    model::expected_payloads, ContentKey, DecisionRecord, DecisionReplayReceipt,
    ReopenedDecisionLog, TrainingError, TrainingLabelPayloadAssociation, TrainingRow,
    TrainingSnapshot, TrainingSpec, VerifiedTrainingSnapshot,
};
use abir::{ByteOrder, ElementType, Presence};
use abir_bcs::{Bcs2View, FrameKind, ResourceBounds, RootKind, StorageContract};
use std::collections::{BTreeMap, BTreeSet};

/// The replay assurance available from an opened training snapshot.
///
/// A snapshot binds the exact decision-log identity, but does not embed the
/// decision records needed to replay and verify that log. Callers must not
/// treat this state as replay verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecisionLogReplayState {
    /// The snapshot binds a decision-log ContentId, but carries no replayable records.
    IdentityBound,
}

impl DecisionLogReplayState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IdentityBound => "identity-bound",
        }
    }
}

/// A validated zero-copy lease into the original BCS2 artifact.
#[derive(Clone, Copy, Debug)]
pub struct TrainingRowLease<'a> {
    bytes: &'a [u8],
    row: &'a TrainingRow,
}

/// A validated lease for typed label data associated with one logical row.
#[derive(Clone, Copy, Debug)]
pub struct TrainingLabelPayloadLease<'a> {
    association: &'a TrainingLabelPayloadAssociation,
    bytes: Option<&'a [u8]>,
}

impl<'a> TrainingLabelPayloadLease<'a> {
    pub fn concept(self) -> &'a str {
        &self.association.concept
    }

    pub const fn presence(self) -> Presence {
        self.association.presence
    }

    pub const fn bytes(self) -> Option<&'a [u8]> {
        self.bytes
    }

    pub fn element(self) -> Option<ElementType> {
        self.association
            .payload
            .as_ref()
            .map(|payload| payload.element)
    }

    pub fn byte_order(self) -> Option<ByteOrder> {
        self.association
            .payload
            .as_ref()
            .map(|payload| payload.byte_order)
    }

    pub fn shape(self) -> Option<&'a [u64]> {
        self.association
            .payload
            .as_ref()
            .map(|payload| payload.shape.as_slice())
    }

    pub fn payload_id(self) -> Option<ContentKey> {
        self.association
            .payload
            .as_ref()
            .map(|payload| payload.payload)
    }
}

impl<'a> TrainingRowLease<'a> {
    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub const fn element(self) -> ElementType {
        self.row.element
    }

    pub const fn byte_order(self) -> ByteOrder {
        self.row.byte_order
    }

    pub const fn metadata(self) -> &'a TrainingRow {
        self.row
    }

    pub fn shape(self) -> &'a [u64] {
        &self.row.shape
    }

    pub const fn logical_id(self) -> ContentKey {
        self.row.logical_id
    }

    pub const fn group(self) -> ContentKey {
        self.row.group
    }

    pub const fn label(self) -> ContentKey {
        self.row.label
    }

    pub const fn split(self) -> ContentKey {
        self.row.split
    }

    pub const fn payload_id(self) -> ContentKey {
        self.row.payload
    }
}

/// A validated host-side view of an immutable BCS2 training bundle.
#[derive(Debug)]
pub struct TrainingWindowStore<'a> {
    frame_index: BTreeMap<ContentKey, usize>,
    snapshot: TrainingSnapshot,
    view: Bcs2View<'a>,
}

impl<'a> TrainingWindowStore<'a> {
    pub fn open(bytes: &'a [u8], bounds: ResourceBounds) -> Result<Self, TrainingError> {
        let view = Bcs2View::parse(bytes, 0, bounds)?;
        if view.root_kind() != RootKind::Bundle {
            return Err(TrainingError::NotBundle);
        }
        if view.storage_contract() != StorageContract::SealedImmutable {
            return Err(TrainingError::NotSealed);
        }
        let wire_profile = crate::TrainingProfile::from_bcs2(view.profile())?;
        let snapshot = TrainingSnapshot::from_catalog(view.semantic_json())?;
        if snapshot.profile() != wire_profile {
            return Err(TrainingError::ProfileMismatch);
        }
        if snapshot.content_id()? != view.root_content_id() {
            return Err(TrainingError::ContentIdMismatch);
        }
        if let Some(reference) = view.references().first() {
            return Err(TrainingError::ExternalReference(*reference));
        }

        let expected: BTreeSet<_> = expected_payloads(&snapshot);
        let mut frame_index = BTreeMap::new();
        for (index, frame) in view.frames().iter().enumerate() {
            if frame.kind() != FrameKind::SemanticPayload {
                return Err(TrainingError::ExtraPayload(frame.content_id()));
            }
            let key = ContentKey::from(frame.content_id());
            if frame_index.insert(key, index).is_some() {
                return Err(TrainingError::DuplicatePayload(frame.content_id()));
            }
            if !expected.contains(&key) {
                return Err(TrainingError::ExtraPayload(frame.content_id()));
            }
        }
        if let Some(missing) = expected.iter().find(|key| !frame_index.contains_key(key)) {
            return Err(TrainingError::MissingPayload(missing.content_id()));
        }
        for row in snapshot.rows() {
            let frame = &view.frames()[frame_index[&row.payload]];
            if frame.element() != Some(row.element)
                || u64::try_from(frame.bytes().len()).ok() != Some(row.logical_bytes)
            {
                return Err(TrainingError::InvalidRowExtent(row.logical_id.content_id()));
            }
        }
        for association in snapshot.label_payloads() {
            let Some(payload) = &association.payload else {
                continue;
            };
            let frame = &view.frames()[frame_index[&payload.payload]];
            if frame.element() != Some(payload.element)
                || u64::try_from(frame.bytes().len()).ok() != Some(payload.logical_bytes)
            {
                return Err(TrainingError::InvalidRowExtent(
                    association.logical_id.content_id(),
                ));
            }
        }

        Ok(Self {
            frame_index,
            snapshot,
            view,
        })
    }

    pub fn snapshot(&self) -> &TrainingSnapshot {
        &self.snapshot
    }

    pub fn snapshot_id(&self) -> Result<abir::ContentId, TrainingError> {
        self.snapshot.content_id()
    }

    pub const fn spec_id(&self) -> ContentKey {
        self.snapshot.spec_id()
    }

    pub fn dataset_roots(&self) -> &[ContentKey] {
        self.snapshot.dataset_roots()
    }

    pub const fn decision_log_id(&self) -> ContentKey {
        self.snapshot.decision_log_id()
    }

    pub const fn decision_log_replay_state(&self) -> DecisionLogReplayState {
        DecisionLogReplayState::IdentityBound
    }

    /// Verifies replay against the exact decision-log identity bound by this snapshot.
    pub fn verify_decision_replay(
        &self,
        spec: &TrainingSpec,
        log: &ReopenedDecisionLog,
        replayed: &[DecisionRecord],
    ) -> Result<DecisionReplayReceipt, TrainingError> {
        let receipt = DecisionReplayReceipt::verify(spec, log, replayed)?;
        if receipt.decision_log_id() != self.decision_log_id()
            || receipt.spec_id() != self.spec_id()
        {
            return Err(TrainingError::DecisionReplayMismatch);
        }
        Ok(receipt)
    }

    /// Produces an opaque promotion input only after full store verification.
    pub fn verified_snapshot(&self) -> VerifiedTrainingSnapshot {
        VerifiedTrainingSnapshot::from_store(self)
    }

    pub fn row(&self, logical_id: ContentKey) -> Option<TrainingRowLease<'_>> {
        let row = self
            .snapshot
            .rows()
            .binary_search_by_key(&logical_id, |row| row.logical_id)
            .ok()
            .map(|index| &self.snapshot.rows()[index])?;
        let frame = &self.view.frames()[self.frame_index[&row.payload]];
        Some(TrainingRowLease {
            bytes: frame.bytes(),
            row,
        })
    }

    pub fn rows(&self) -> impl ExactSizeIterator<Item = TrainingRowLease<'_>> {
        self.snapshot.rows().iter().map(|row| {
            let frame = &self.view.frames()[self.frame_index[&row.payload]];
            TrainingRowLease {
                bytes: frame.bytes(),
                row,
            }
        })
    }

    pub fn label_payload(
        &self,
        logical_id: ContentKey,
        concept: &str,
    ) -> Option<TrainingLabelPayloadLease<'_>> {
        let association = self
            .snapshot
            .label_payloads()
            .binary_search_by(|association| {
                (association.logical_id, association.concept.as_str()).cmp(&(logical_id, concept))
            })
            .ok()
            .map(|index| &self.snapshot.label_payloads()[index])?;
        let bytes = association
            .payload
            .as_ref()
            .map(|payload| self.view.frames()[self.frame_index[&payload.payload]].bytes());
        Some(TrainingLabelPayloadLease { association, bytes })
    }
}
