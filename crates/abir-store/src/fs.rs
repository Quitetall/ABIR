use crate::{validate_portable_bundle, StoreError};
use abir::{
    verify_payload_content, ContentId, PayloadAccess, PayloadAccessError, PayloadDescriptor,
    PayloadLease, StorageId,
};
use abir_bcs::{repack_with_frames, Bcs2View, FrameKind, ResourceBounds};
use memmap2::{Mmap, MmapOptions};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
struct FileMeta {
    content_id: ContentId,
    references: BTreeSet<ContentId>,
    payloads: BTreeMap<ContentId, Range<usize>>,
    path: PathBuf,
}

pub struct MmapLease {
    content_id: ContentId,
    storage_id: StorageId,
    mmap: Mmap,
    active: Arc<Mutex<BTreeMap<StorageId, usize>>>,
    lock: File,
}

pub struct MmapPayloadLease {
    mmap: Mmap,
    range: Range<usize>,
    storage_id: StorageId,
    active: Arc<Mutex<BTreeMap<StorageId, usize>>>,
    lock: File,
}

impl PayloadLease for MmapPayloadLease {
    fn bytes(&self) -> &[u8] {
        &self.mmap[self.range.clone()]
    }
}

impl Drop for MmapPayloadLease {
    fn drop(&mut self) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(count) = active.get_mut(&self.storage_id) {
            *count -= 1;
            if *count == 0 {
                active.remove(&self.storage_id);
            }
        }
        let _ = fs2::FileExt::unlock(&self.lock);
    }
}

impl MmapLease {
    pub const fn content_id(&self) -> ContentId {
        self.content_id
    }
    pub const fn storage_id(&self) -> StorageId {
        self.storage_id
    }
    pub fn bytes(&self) -> &[u8] {
        &self.mmap
    }
}

impl Drop for MmapLease {
    fn drop(&mut self) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(count) = active.get_mut(&self.storage_id) {
            *count -= 1;
            if *count == 0 {
                active.remove(&self.storage_id);
            }
        }
        let _ = fs2::FileExt::unlock(&self.lock);
    }
}

pub struct FsAbirStore {
    objects: PathBuf,
    pins: PathBuf,
    lock_path: PathBuf,
    supported_capabilities: u64,
    limits: ResourceBounds,
    by_storage: BTreeMap<StorageId, FileMeta>,
    by_content: BTreeMap<ContentId, BTreeSet<StorageId>>,
    payloads: BTreeMap<ContentId, BTreeSet<StorageId>>,
    pinned_roots: BTreeSet<ContentId>,
    active: Arc<Mutex<BTreeMap<StorageId, usize>>>,
}

impl FsAbirStore {
    pub fn open(
        root: impl AsRef<Path>,
        supported_capabilities: u64,
        limits: ResourceBounds,
    ) -> Result<Self, StoreError> {
        let root = root.as_ref().to_path_buf();
        let objects = root.join("objects");
        let pins = root.join("pins");
        let lock_path = root.join("store.lock");
        fs::create_dir_all(&objects).map_err(|error| StoreError::io("create objects", error))?;
        fs::create_dir_all(&pins).map_err(|error| StoreError::io("create pins", error))?;
        let mut store = Self {
            objects,
            pins,
            lock_path,
            supported_capabilities,
            limits,
            by_storage: BTreeMap::new(),
            by_content: BTreeMap::new(),
            payloads: BTreeMap::new(),
            pinned_roots: BTreeSet::new(),
            active: Arc::new(Mutex::new(BTreeMap::new())),
        };
        let lock = store.exclusive_lock()?;
        store.rebuild_index()?;
        store.load_pins()?;
        for root in store.pinned_roots.iter().copied() {
            store.reachable_closure(root)?;
        }
        fs2::FileExt::unlock(&lock).map_err(|error| StoreError::io("unlock store", error))?;
        Ok(store)
    }

    /// Refreshes the in-memory catalog after another process publishes objects or pins.
    pub fn refresh(&mut self) -> Result<(), StoreError> {
        let lock = self.exclusive_lock()?;
        self.rebuild_index()?;
        self.load_pins()?;
        for root in self.pinned_roots.iter().copied() {
            self.reachable_closure(root)?;
        }
        fs2::FileExt::unlock(&lock).map_err(|error| StoreError::io("unlock store", error))
    }

    pub fn insert(&mut self, bytes: &[u8]) -> Result<(ContentId, StorageId), StoreError> {
        let view = Bcs2View::parse(bytes, self.supported_capabilities, self.limits)?;
        let content_id = view.root_content_id();
        let storage_id = view.storage_id();
        let references: BTreeSet<_> = view.references().iter().copied().collect();
        let lock = self.exclusive_lock()?;
        self.rebuild_index()?;
        if self.by_storage.contains_key(&storage_id) {
            fs2::FileExt::unlock(&lock).map_err(|error| StoreError::io("unlock store", error))?;
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
        let final_path = self.object_path(storage_id);
        let (temp_path, mut file) = self.create_temporary_object()?;
        let write_result = (|| {
            file.write_all(bytes)
                .map_err(|error| StoreError::io("write object", error))?;
            file.sync_all()
                .map_err(|error| StoreError::io("sync object", error))?;
            match fs::hard_link(&temp_path, &final_path) {
                Ok(()) => {
                    fs::remove_file(&temp_path)
                        .map_err(|error| StoreError::io("remove published temporary", error))?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    fs::remove_file(&temp_path)
                        .map_err(|remove| StoreError::io("remove duplicate temporary", remove))?;
                }
                Err(error) => return Err(StoreError::io("publish object", error)),
            }
            sync_directory(&self.objects)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        write_result?;
        self.index_file(final_path, Some(storage_id))?;
        fs2::FileExt::unlock(&lock).map_err(|error| StoreError::io("unlock store", error))?;
        Ok((content_id, storage_id))
    }

    pub fn lease(&self, content_id: ContentId) -> Result<MmapLease, StoreError> {
        let storage_id = self
            .by_content
            .get(&content_id)
            .and_then(|ids| ids.first().copied())
            .ok_or(StoreError::MissingObject(content_id))?;
        self.lease_storage(storage_id)
    }

    pub fn lease_storage(&self, storage_id: StorageId) -> Result<MmapLease, StoreError> {
        let lock = self.shared_lock()?;
        let meta = self
            .by_storage
            .get(&storage_id)
            .ok_or(StoreError::MissingStorage(storage_id))?;
        let file = File::open(&meta.path).map_err(|error| StoreError::io("open object", error))?;
        // Store objects are immutable after atomic publication. Mapping stays
        // valid while lease owns Mmap; GC also refuses active storage IDs.
        let mmap = unsafe { MmapOptions::new().map(&file) }
            .map_err(|error| StoreError::io("map object", error))?;
        let view = Bcs2View::parse(&mmap, self.supported_capabilities, self.limits)?;
        if view.storage_id() != storage_id || view.root_content_id() != meta.content_id {
            return Err(StoreError::ConflictingStorageIdentity(storage_id));
        }
        *self
            .active
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .entry(storage_id)
            .or_default() += 1;
        Ok(MmapLease {
            content_id: meta.content_id,
            storage_id,
            mmap,
            active: Arc::clone(&self.active),
            lock,
        })
    }

    pub fn pin(&mut self, root: ContentId) -> Result<(), StoreError> {
        let lock = self.exclusive_lock()?;
        self.rebuild_index()?;
        self.load_pins()?;
        self.reachable_closure(root)?;
        let path = self.pins.join(root.to_string());
        let file = match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = fs::symlink_metadata(&path).map_err(|metadata_error| {
                    StoreError::io("inspect existing pin", metadata_error)
                })?;
                if !metadata.file_type().is_file() || metadata.len() != 0 {
                    return Err(StoreError::InvalidObjectName);
                }
                File::open(&path)
                    .map_err(|open_error| StoreError::io("open existing pin", open_error))?
            }
            Err(error) => return Err(StoreError::io("create pin", error)),
        };
        file.sync_all()
            .map_err(|error| StoreError::io("sync pin", error))?;
        sync_directory(&self.pins)?;
        self.pinned_roots.insert(root);
        fs2::FileExt::unlock(&lock).map_err(|error| StoreError::io("unlock store", error))?;
        Ok(())
    }

    pub fn export_portable(&self, root: ContentId) -> Result<Vec<u8>, StoreError> {
        let closure = self.reachable_closure(root)?;
        let root_storage = self
            .by_content
            .get(&root)
            .and_then(|variants| variants.first())
            .ok_or(StoreError::MissingObject(root))?;
        let root_meta = self
            .by_storage
            .get(root_storage)
            .ok_or(StoreError::MissingStorage(*root_storage))?;
        let root_bytes = fs::read(&root_meta.path)
            .map_err(|error| StoreError::io("read portable root", error))?;
        let mut embedded_bytes = Vec::with_capacity(closure.len().saturating_sub(1));
        for content_id in closure.into_iter().filter(|content_id| *content_id != root) {
            let storage_id = self
                .by_content
                .get(&content_id)
                .and_then(|variants| variants.first())
                .ok_or(StoreError::IncompleteClosure(content_id))?;
            let meta = self
                .by_storage
                .get(storage_id)
                .ok_or(StoreError::MissingStorage(*storage_id))?;
            embedded_bytes.push(
                fs::read(&meta.path)
                    .map_err(|error| StoreError::io("read portable frame", error))?,
            );
        }
        let embedded: Vec<&[u8]> = embedded_bytes.iter().map(Vec::as_slice).collect();
        Ok(repack_with_frames(
            &root_bytes,
            &embedded,
            self.supported_capabilities,
            self.limits,
        )?)
    }

    pub fn import_portable(&mut self, bytes: &[u8]) -> Result<(ContentId, StorageId), StoreError> {
        let validation = validate_portable_bundle(bytes, self.supported_capabilities, self.limits)?;
        for frame in validation.frames {
            self.insert(&frame)?;
        }
        self.insert(bytes)?;
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
            let meta = self
                .by_storage
                .get(storage_id)
                .ok_or(StoreError::IncompleteClosure(content_id))?;
            pending.extend(meta.references.iter().copied());
        }
        Ok(reached)
    }

    pub fn collect_unreachable(&mut self) -> Result<usize, StoreError> {
        let lock = self.try_exclusive_lock()?;
        self.rebuild_index()?;
        self.load_pins()?;
        let mut reachable = BTreeSet::new();
        for root in self.pinned_roots.iter().copied() {
            reachable.extend(self.reachable_closure(root)?);
        }
        let active = self
            .active
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone();
        let removable: Vec<_> = self
            .by_storage
            .iter()
            .filter(|(storage_id, meta)| {
                !reachable.contains(&meta.content_id) && !active.contains_key(storage_id)
            })
            .map(|(storage_id, meta)| (*storage_id, meta.path.clone()))
            .collect();
        for (storage_id, path) in &removable {
            fs::remove_file(path).map_err(|error| StoreError::io("remove object", error))?;
            self.by_storage.remove(storage_id);
        }
        if !removable.is_empty() {
            sync_directory(&self.objects)?;
        }
        self.rebuild_content_index();
        fs2::FileExt::unlock(&lock).map_err(|error| StoreError::io("unlock store", error))?;
        Ok(removable.len())
    }

    pub fn object_count(&self) -> usize {
        self.by_storage.len()
    }

    fn create_temporary_object(&self) -> Result<(PathBuf, File), StoreError> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| StoreError::InvalidObjectName)?
            .as_nanos();
        for attempt in 0..32_u8 {
            let path = self
                .objects
                .join(format!(".tmp-{}-{nonce}-{attempt}", std::process::id()));
            match OpenOptions::new().create_new(true).write(true).open(&path) {
                Ok(file) => return Ok((path, file)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(StoreError::io("create temporary object", error)),
            }
        }
        Err(StoreError::Io {
            operation: "create unique temporary object",
            kind: std::io::ErrorKind::AlreadyExists,
        })
    }

    fn rebuild_index(&mut self) -> Result<(), StoreError> {
        self.by_storage.clear();
        self.by_content.clear();
        self.payloads.clear();
        for entry in
            fs::read_dir(&self.objects).map_err(|error| StoreError::io("read objects", error))?
        {
            let entry = entry.map_err(|error| StoreError::io("read object entry", error))?;
            let file_type = entry
                .file_type()
                .map_err(|error| StoreError::io("inspect object entry", error))?;
            if !file_type.is_file() {
                return Err(StoreError::InvalidObjectName);
            }
            let name = entry.file_name();
            let name = name.to_str().ok_or(StoreError::InvalidObjectName)?;
            if name.starts_with(".tmp-") {
                fs::remove_file(entry.path())
                    .map_err(|error| StoreError::io("remove stale temporary", error))?;
                continue;
            }
            let expected = parse_storage_name(name)?;
            self.index_file(entry.path(), Some(expected))?;
        }
        Ok(())
    }

    fn index_file(&mut self, path: PathBuf, expected: Option<StorageId>) -> Result<(), StoreError> {
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| StoreError::io("inspect object", error))?;
        if !metadata.file_type().is_file() {
            return Err(StoreError::InvalidObjectName);
        }
        let bytes = fs::read(&path).map_err(|error| StoreError::io("read object", error))?;
        let view = Bcs2View::parse(&bytes, self.supported_capabilities, self.limits)?;
        let storage_id = view.storage_id();
        if expected.is_some_and(|value| value != storage_id) {
            return Err(StoreError::ConflictingStorageIdentity(storage_id));
        }
        let base = bytes.as_ptr() as usize;
        let payloads: BTreeMap<_, _> = view
            .frames()
            .iter()
            .filter(|frame| frame.kind() == FrameKind::SemanticPayload)
            .map(|frame| {
                let start = frame.bytes().as_ptr() as usize - base;
                (frame.content_id(), start..start + frame.bytes().len())
            })
            .collect();
        let meta = FileMeta {
            content_id: view.root_content_id(),
            references: view.references().iter().copied().collect(),
            payloads: payloads.clone(),
            path,
        };
        if let Some(existing_storage) = self
            .by_content
            .get(&meta.content_id)
            .and_then(|ids| ids.first())
        {
            let existing = self
                .by_storage
                .get(existing_storage)
                .ok_or(StoreError::MissingStorage(*existing_storage))?;
            if existing.references != meta.references {
                return Err(StoreError::ConflictingClosure(meta.content_id));
            }
        }
        let content_id = meta.content_id;
        if self.by_storage.insert(storage_id, meta).is_some() {
            return Err(StoreError::ConflictingStorageIdentity(storage_id));
        }
        self.by_content
            .entry(content_id)
            .or_default()
            .insert(storage_id);
        for payload_id in payloads.keys() {
            self.payloads
                .entry(*payload_id)
                .or_default()
                .insert(storage_id);
        }
        Ok(())
    }

    fn rebuild_content_index(&mut self) {
        self.by_content.clear();
        self.payloads.clear();
        for (storage_id, meta) in &self.by_storage {
            self.by_content
                .entry(meta.content_id)
                .or_default()
                .insert(*storage_id);
            for payload_id in meta.payloads.keys() {
                self.payloads
                    .entry(*payload_id)
                    .or_default()
                    .insert(*storage_id);
            }
        }
    }

    fn load_pins(&mut self) -> Result<(), StoreError> {
        self.pinned_roots.clear();
        for entry in fs::read_dir(&self.pins).map_err(|error| StoreError::io("read pins", error))? {
            let entry = entry.map_err(|error| StoreError::io("read pin entry", error))?;
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|error| StoreError::io("inspect pin entry", error))?;
            if !metadata.file_type().is_file() || metadata.len() != 0 {
                return Err(StoreError::InvalidObjectName);
            }
            let name = entry.file_name();
            let name = name.to_str().ok_or(StoreError::InvalidObjectName)?;
            self.pinned_roots.insert(parse_content_name(name)?);
        }
        Ok(())
    }

    fn object_path(&self, storage_id: StorageId) -> PathBuf {
        self.objects.join(format!("{storage_id}.bcs2"))
    }

    fn exclusive_lock(&self) -> Result<File, StoreError> {
        let file = self.open_lock()?;
        fs2::FileExt::lock_exclusive(&file)
            .map_err(|error| StoreError::io("lock store exclusive", error))?;
        Ok(file)
    }

    fn shared_lock(&self) -> Result<File, StoreError> {
        let file = self.open_lock()?;
        fs2::FileExt::lock_shared(&file)
            .map_err(|error| StoreError::io("lock store shared", error))?;
        Ok(file)
    }

    fn try_exclusive_lock(&self) -> Result<File, StoreError> {
        let file = self.open_lock()?;
        match fs2::FileExt::try_lock_exclusive(&file) {
            Ok(()) => Ok(file),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                Err(StoreError::StoreBusy)
            }
            Err(error) => Err(StoreError::io("try lock store exclusive", error)),
        }
    }

    fn open_lock(&self) -> Result<File, StoreError> {
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&self.lock_path)
            .map_err(|error| StoreError::io("open store lock", error))
    }
}

impl PayloadAccess for FsAbirStore {
    type Lease<'a>
        = MmapPayloadLease
    where
        Self: 'a;

    fn lease<'a>(
        &'a self,
        descriptor: &PayloadDescriptor,
    ) -> Result<Self::Lease<'a>, PayloadAccessError> {
        let storage_id = self
            .payloads
            .get(&descriptor.content_id())
            .and_then(|ids| ids.first().copied())
            .ok_or(PayloadAccessError::NotFound(descriptor.content_id()))?;
        let meta = self
            .by_storage
            .get(&storage_id)
            .ok_or(PayloadAccessError::NotFound(descriptor.content_id()))?;
        let range = meta
            .payloads
            .get(&descriptor.content_id())
            .cloned()
            .ok_or(PayloadAccessError::NotFound(descriptor.content_id()))?;
        let lock = self
            .shared_lock()
            .map_err(|_| PayloadAccessError::NotFound(descriptor.content_id()))?;
        let file = File::open(&meta.path)
            .map_err(|_| PayloadAccessError::NotFound(descriptor.content_id()))?;
        let mmap = unsafe { MmapOptions::new().map(&file) }
            .map_err(|_| PayloadAccessError::NotFound(descriptor.content_id()))?;
        let payload = mmap
            .get(range.clone())
            .ok_or(PayloadAccessError::NotFound(descriptor.content_id()))?;
        if let Err(error) = verify_payload_content(descriptor, payload) {
            return match error {
                abir::PayloadVerificationError::LengthMismatch { expected, actual } => {
                    Err(PayloadAccessError::LengthMismatch { expected, actual })
                }
                abir::PayloadVerificationError::ContentIdMismatch { .. } => {
                    Err(PayloadAccessError::NotFound(descriptor.content_id()))
                }
            };
        }
        *self
            .active
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .entry(storage_id)
            .or_default() += 1;
        Ok(MmapPayloadLease {
            mmap,
            range,
            storage_id,
            active: Arc::clone(&self.active),
            lock,
        })
    }
}

fn sync_directory(path: &Path) -> Result<(), StoreError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| StoreError::io("sync directory", error))
}

fn parse_storage_name(name: &str) -> Result<StorageId, StoreError> {
    let hex = name
        .strip_suffix(".bcs2")
        .ok_or(StoreError::InvalidObjectName)?;
    Ok(StorageId::from_bytes(parse_hex_32(hex)?))
}

fn parse_content_name(name: &str) -> Result<ContentId, StoreError> {
    Ok(ContentId::from_bytes(parse_hex_32(name)?))
}

fn parse_hex_32(value: &str) -> Result<[u8; 32], StoreError> {
    if value.len() != 64 {
        return Err(StoreError::InvalidObjectName);
    }
    let mut bytes = [0; 32];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk).map_err(|_| StoreError::InvalidObjectName)?;
        bytes[index] = u8::from_str_radix(text, 16).map_err(|_| StoreError::InvalidObjectName)?;
    }
    Ok(bytes)
}
