use abir::{ContentId, StorageId};
use abir_bcs::{repack_with_frames, Bcs2Error, Bcs2View, ResourceBounds};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

mod fs;
pub use fs::{FsAbirStore, MmapLease};

#[derive(Debug, Eq, PartialEq)]
pub enum StoreError {
    Wire(Bcs2Error),
    MissingObject(ContentId),
    MissingStorage(StorageId),
    ConflictingStorageIdentity(StorageId),
    IncompleteClosure(ContentId),
    IncompletePortableBundle(ContentId),
    ExtraPortableFrame(ContentId),
    ConflictingClosure(ContentId),
    Io {
        operation: &'static str,
        kind: std::io::ErrorKind,
    },
    InvalidObjectName,
    StoreBusy,
}

impl StoreError {
    fn io(operation: &'static str, error: std::io::Error) -> Self {
        Self::Io {
            operation,
            kind: error.kind(),
        }
    }
}

impl From<Bcs2Error> for StoreError {
    fn from(value: Bcs2Error) -> Self {
        Self::Wire(value)
    }
}

#[derive(Clone, Debug)]
struct StoredObject {
    content_id: ContentId,
    storage_id: StorageId,
    references: BTreeSet<ContentId>,
    bytes: Arc<[u8]>,
}

#[derive(Clone, Debug)]
pub struct StoreLease {
    content_id: ContentId,
    storage_id: StorageId,
    bytes: Arc<[u8]>,
}

impl StoreLease {
    pub const fn content_id(&self) -> ContentId {
        self.content_id
    }

    pub const fn storage_id(&self) -> StorageId {
        self.storage_id
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Default)]
pub struct AbirStore {
    by_storage: BTreeMap<StorageId, StoredObject>,
    by_content: BTreeMap<ContentId, BTreeSet<StorageId>>,
    pinned_roots: BTreeSet<ContentId>,
}

impl AbirStore {
    pub fn insert_bcs2(
        &mut self,
        bytes: Arc<[u8]>,
        supported_capabilities: u64,
        limits: ResourceBounds,
    ) -> Result<(ContentId, StorageId), StoreError> {
        let view = Bcs2View::parse(&bytes, supported_capabilities, limits)?;
        let content_id = view.root_content_id();
        let storage_id = view.storage_id();
        let references: BTreeSet<_> = view.references().iter().copied().collect();
        if let Some(existing) = self.by_storage.get(&storage_id) {
            if existing.bytes.as_ref() != bytes.as_ref() || existing.content_id != content_id {
                return Err(StoreError::ConflictingStorageIdentity(storage_id));
            }
            return Ok((content_id, storage_id));
        }
        if let Some(existing_storage) = self.by_content.get(&content_id).and_then(|ids| ids.first())
        {
            let existing = self
                .by_storage
                .get(existing_storage)
                .ok_or(StoreError::MissingStorage(*existing_storage))?;
            if existing.references != references {
                return Err(StoreError::ConflictingClosure(content_id));
            }
        }
        let object = StoredObject {
            content_id,
            storage_id,
            references,
            bytes,
        };
        self.by_storage.insert(storage_id, object);
        self.by_content
            .entry(content_id)
            .or_default()
            .insert(storage_id);
        Ok((content_id, storage_id))
    }

    pub fn lease(&self, content_id: ContentId) -> Result<StoreLease, StoreError> {
        let storage_id = self
            .by_content
            .get(&content_id)
            .and_then(|ids| ids.first().copied())
            .ok_or(StoreError::MissingObject(content_id))?;
        self.lease_storage(storage_id)
    }

    pub fn lease_storage(&self, storage_id: StorageId) -> Result<StoreLease, StoreError> {
        let object = self
            .by_storage
            .get(&storage_id)
            .ok_or(StoreError::MissingStorage(storage_id))?;
        Ok(StoreLease {
            content_id: object.content_id,
            storage_id: object.storage_id,
            bytes: Arc::clone(&object.bytes),
        })
    }

    pub fn pin(&mut self, root: ContentId) -> Result<(), StoreError> {
        self.reachable_closure(root)?;
        self.pinned_roots.insert(root);
        Ok(())
    }

    pub fn export_portable(
        &self,
        root: ContentId,
        supported_capabilities: u64,
        limits: ResourceBounds,
    ) -> Result<Vec<u8>, StoreError> {
        let closure = self.reachable_closure(root)?;
        let root_storage = self
            .by_content
            .get(&root)
            .and_then(|variants| variants.first())
            .ok_or(StoreError::MissingObject(root))?;
        let root_object = self
            .by_storage
            .get(root_storage)
            .ok_or(StoreError::MissingStorage(*root_storage))?;
        let mut embedded = Vec::with_capacity(closure.len().saturating_sub(1));
        for content_id in closure.into_iter().filter(|content_id| *content_id != root) {
            let storage_id = self
                .by_content
                .get(&content_id)
                .and_then(|variants| variants.first())
                .ok_or(StoreError::IncompleteClosure(content_id))?;
            let object = self
                .by_storage
                .get(storage_id)
                .ok_or(StoreError::MissingStorage(*storage_id))?;
            embedded.push(object.bytes.as_ref());
        }
        Ok(repack_with_frames(
            root_object.bytes.as_ref(),
            &embedded,
            supported_capabilities,
            limits,
        )?)
    }

    pub fn import_portable(
        &mut self,
        bytes: Arc<[u8]>,
        supported_capabilities: u64,
        limits: ResourceBounds,
    ) -> Result<(ContentId, StorageId), StoreError> {
        let validation = validate_portable_bundle(&bytes, supported_capabilities, limits)?;
        for frame in validation.frames {
            self.insert_bcs2(frame, supported_capabilities, limits)?;
        }
        self.insert_bcs2(bytes, supported_capabilities, limits)?;
        Ok(validation.root_ids)
    }

    pub fn reachable_closure(&self, root: ContentId) -> Result<BTreeSet<ContentId>, StoreError> {
        let mut reached = BTreeSet::new();
        let mut pending = vec![root];
        while let Some(content_id) = pending.pop() {
            if !reached.insert(content_id) {
                continue;
            }
            let storage_id = self
                .by_content
                .get(&content_id)
                .and_then(|ids| ids.first())
                .ok_or(StoreError::IncompleteClosure(content_id))?;
            let object = self
                .by_storage
                .get(storage_id)
                .ok_or(StoreError::IncompleteClosure(content_id))?;
            pending.extend(object.references.iter().copied());
        }
        Ok(reached)
    }

    pub fn collect_unreachable(&mut self) -> usize {
        let reachable: BTreeSet<_> = self
            .pinned_roots
            .iter()
            .filter_map(|root| self.reachable_closure(*root).ok())
            .flatten()
            .collect();
        let before = self.by_storage.len();
        self.by_storage.retain(|_, object| {
            reachable.contains(&object.content_id) || Arc::strong_count(&object.bytes) > 1
        });
        self.by_content.clear();
        for object in self.by_storage.values() {
            self.by_content
                .entry(object.content_id)
                .or_default()
                .insert(object.storage_id);
        }
        before - self.by_storage.len()
    }

    pub fn object_count(&self) -> usize {
        self.by_storage.len()
    }

    pub fn physical_variants(&self, content_id: ContentId) -> usize {
        self.by_content.get(&content_id).map_or(0, BTreeSet::len)
    }
}

pub(crate) struct PortableBundleValidation {
    pub root_ids: (ContentId, StorageId),
    pub frames: Vec<Arc<[u8]>>,
}

pub(crate) fn validate_portable_bundle(
    bytes: &[u8],
    supported_capabilities: u64,
    limits: ResourceBounds,
) -> Result<PortableBundleValidation, StoreError> {
    let view = Bcs2View::parse(bytes, supported_capabilities, limits)?;
    if !view.profile().is_portable() {
        return Err(StoreError::Wire(Bcs2Error::ProfileNotPortable));
    }
    let frame_ids: BTreeSet<_> = view
        .frames()
        .iter()
        .map(|frame| frame.content_id())
        .collect();
    let mut reached = BTreeSet::new();
    let mut pending: Vec<_> = view.references().to_vec();
    while let Some(content_id) = pending.pop() {
        if !reached.insert(content_id) {
            continue;
        }
        let frame = view
            .frames()
            .iter()
            .find(|frame| frame.content_id() == content_id)
            .ok_or(StoreError::IncompletePortableBundle(content_id))?;
        let nested = Bcs2View::parse(frame.bytes(), supported_capabilities, limits)?;
        pending.extend(nested.references().iter().copied());
    }
    if let Some(extra) = frame_ids.difference(&reached).next() {
        return Err(StoreError::ExtraPortableFrame(*extra));
    }
    let frame_bytes = view
        .frames()
        .iter()
        .map(|frame| Arc::<[u8]>::from(frame.bytes()))
        .collect();
    Ok(PortableBundleValidation {
        root_ids: (view.root_content_id(), view.storage_id()),
        frames: frame_bytes,
    })
}
