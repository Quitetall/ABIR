use crate::{ContentKey, TrainingError, TrainingRow, TrainingSnapshot};
use abir::ElementType;
use abir_bcs::{Bcs2View, FrameKind, ResourceBounds, RootKind, StorageContract};
use std::collections::{BTreeMap, BTreeSet};

/// A validated zero-copy lease into the original BCS2 artifact.
#[derive(Clone, Copy, Debug)]
pub struct TrainingRowLease<'a> {
    bytes: &'a [u8],
    row: &'a TrainingRow,
}

impl<'a> TrainingRowLease<'a> {
    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub const fn element(self) -> ElementType {
        self.row.element
    }

    pub const fn metadata(self) -> &'a TrainingRow {
        self.row
    }

    pub fn shape(self) -> &'a [u64] {
        &self.row.shape
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

        let expected: BTreeSet<_> = snapshot.rows().iter().map(|row| row.payload).collect();
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

        Ok(Self {
            frame_index,
            snapshot,
            view,
        })
    }

    pub fn snapshot(&self) -> &TrainingSnapshot {
        &self.snapshot
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
}
