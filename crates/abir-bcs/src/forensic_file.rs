use crate::forensic::{
    decode_metadata, encode_metadata, forensic_tree_content_id, metadata_encoded_len,
    parse_semantic_json, validate_metadata,
};
use crate::wire::{
    encode_catalog, encode_index_header, encode_raw_index_entry, encode_sealed_header,
    INDEX_ENTRY_LEN, INDEX_LEN, RAW_CONTENT_HASH_DOMAIN, RAW_STORAGE_HASH_DOMAIN,
};
use crate::{
    raw_content_id, Bcs2Error, Bcs2FileError, Bcs2FileIndex, FileFrame, ForensicEntryMetadata,
    ForensicTreeMetadata, FrameKind, PrivacyMode, ProfileId, ResourceBounds, RootKind,
    StorageContract, BCS2_HEADER_LEN,
};
use abir::{ContentId, StorageId};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom, Write};

const STREAM_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ForensicWriteReceipt {
    root_content_id: ContentId,
    artifact_len: u64,
    frame_count: u32,
}

impl ForensicWriteReceipt {
    pub const fn root_content_id(&self) -> ContentId {
        self.root_content_id
    }

    pub const fn artifact_len(&self) -> u64 {
        self.artifact_len
    }

    pub const fn frame_count(&self) -> u32 {
        self.frame_count
    }
}

#[derive(Clone, Copy)]
enum FrameSource {
    Metadata,
    External,
}

#[derive(Clone, Copy)]
struct FramePlan {
    len: u64,
    capabilities: u64,
    source: FrameSource,
}

/// Stream one canonical `FORENSIC_TREE_V1` artifact without retaining payloads.
///
/// `open_frame` is called once for each unique non-metadata stored frame, in
/// ascending [`ContentId`] order. Every source is read in at most 64 KiB chunks
/// and verified against its declared length and identity while it is copied.
/// Metadata and the final fixed-width index remain resident and are bounded by
/// `ResourceBounds`; payload bytes and the complete artifact do not.
///
/// Output may be partial on failure. Callers must write to a temporary file,
/// validate it with [`ForensicFileIndex`], then publish it separately.
pub fn write_forensic_tree_streaming<W, O, R>(
    output: &mut W,
    tree: &ForensicTreeMetadata,
    mut open_frame: O,
    bounds: ResourceBounds,
) -> Result<ForensicWriteReceipt, Bcs2FileError>
where
    W: Write + Seek,
    O: FnMut(ContentId) -> std::io::Result<R>,
    R: Read,
{
    validate_bounds(bounds)?;
    if tree.entries.len() > bounds.max_index_entries as usize {
        return Err(Bcs2Error::BoundsExceeded.into());
    }
    validate_metadata(&tree.platform, &tree.entries)?;
    if metadata_encoded_len(&tree.platform, &tree.entries)? > bounds.max_frame_bytes as usize {
        return Err(Bcs2Error::BoundsExceeded.into());
    }

    let metadata = encode_metadata(&tree.platform, &tree.entries)?;
    let metadata_id = raw_content_id(&metadata);
    let root_content_id = forensic_tree_content_id(metadata_id);
    let semantic_json = format!(
        "{{\"forensic_tree\":{{\"content_id\":\"{}\",\"entries\":{},\"version\":1}}}}",
        metadata_id,
        tree.entries.len()
    );
    if semantic_json.len() > bounds.max_catalog_bytes as usize {
        return Err(Bcs2Error::BoundsExceeded.into());
    }
    let catalog = encode_catalog(semantic_json.as_bytes(), root_content_id, &BTreeSet::new())?;
    if catalog.len() > bounds.max_catalog_bytes as usize {
        return Err(Bcs2Error::BoundsExceeded.into());
    }

    let mut frames = BTreeMap::new();
    frames.insert(
        metadata_id,
        FramePlan {
            len: metadata.len() as u64,
            capabilities: 0,
            source: FrameSource::Metadata,
        },
    );
    for entry in &tree.entries {
        let Some(content_id) = entry.frame_content_id() else {
            continue;
        };
        let len = entry.frame_len().ok_or(Bcs2Error::SemanticEncoding)?;
        if len > u64::from(bounds.max_frame_bytes) {
            return Err(Bcs2Error::BoundsExceeded.into());
        }
        let candidate = FramePlan {
            len,
            capabilities: entry.required_capabilities(),
            source: FrameSource::External,
        };
        if let Some(prior) = frames.get(&content_id) {
            if prior.len != candidate.len || prior.capabilities != candidate.capabilities {
                return Err(Bcs2Error::DuplicateFrame.into());
            }
        } else {
            frames.insert(content_id, candidate);
        }
    }
    if frames.len() > bounds.max_index_entries as usize {
        return Err(Bcs2Error::BoundsExceeded.into());
    }

    let frame_bytes = frames.values().try_fold(0_u64, |total, frame| {
        total
            .checked_add(frame.len)
            .ok_or(Bcs2Error::BoundsExceeded)
    })?;
    let catalog_offset = BCS2_HEADER_LEN as u64;
    let frame_offset = catalog_offset
        .checked_add(catalog.len() as u64)
        .ok_or(Bcs2Error::BoundsExceeded)?;
    let index_offset = frame_offset
        .checked_add(frame_bytes)
        .ok_or(Bcs2Error::BoundsExceeded)?;
    let index_len = (INDEX_LEN as u64)
        .checked_add(
            (frames.len() as u64)
                .checked_mul(INDEX_ENTRY_LEN as u64)
                .ok_or(Bcs2Error::BoundsExceeded)?,
        )
        .ok_or(Bcs2Error::BoundsExceeded)?;
    let artifact_len = index_offset
        .checked_add(index_len)
        .ok_or(Bcs2Error::BoundsExceeded)?;
    let union_capabilities = frames
        .values()
        .fold(0_u64, |union, frame| union | frame.capabilities);
    let frame_count = u32::try_from(frames.len()).map_err(|_| Bcs2Error::BoundsExceeded)?;

    if output.stream_position()? != 0 || output.seek(SeekFrom::End(0))? != 0 {
        return Err(Bcs2Error::NonCanonicalLayout.into());
    }
    output.seek(SeekFrom::Start(0))?;

    let header = encode_sealed_header(
        ProfileId::FORENSIC_TREE_V1,
        RootKind::Bundle,
        root_content_id,
        union_capabilities,
        bounds,
        catalog.len() as u64,
        index_offset,
        index_len,
    );
    output.write_all(&header)?;
    output.write_all(&catalog)?;

    let mut index_entries = Vec::new();
    index_entries
        .try_reserve_exact(frames.len())
        .map_err(|_| Bcs2Error::BoundsExceeded)?;
    let mut next_frame_offset = frame_offset;
    for (content_id, frame) in frames {
        let verification = match frame.source {
            FrameSource::Metadata => {
                copy_verified_frame(&mut metadata.as_slice(), output, content_id, frame.len)?
            }
            FrameSource::External => {
                let mut source = open_frame(content_id)?;
                copy_verified_frame(&mut source, output, content_id, frame.len)?
            }
        };
        index_entries.push(encode_raw_index_entry(
            content_id,
            verification.storage_id,
            next_frame_offset,
            frame.len,
            frame.capabilities,
            verification.digest,
        ));
        next_frame_offset = next_frame_offset
            .checked_add(frame.len)
            .ok_or(Bcs2Error::BoundsExceeded)?;
    }
    if next_frame_offset != index_offset {
        return Err(Bcs2Error::NonCanonicalLayout.into());
    }

    output.write_all(&encode_index_header(frame_count, &catalog))?;
    for entry in index_entries {
        output.write_all(&entry)?;
    }

    Ok(ForensicWriteReceipt {
        root_content_id,
        artifact_len,
        frame_count,
    })
}

struct FrameVerification {
    storage_id: StorageId,
    digest: [u8; 32],
}

fn copy_verified_frame<R: Read, W: Write>(
    source: &mut R,
    output: &mut W,
    expected_content_id: ContentId,
    expected_len: u64,
) -> Result<FrameVerification, Bcs2FileError> {
    let mut content_hasher = blake3::Hasher::new();
    content_hasher.update(RAW_CONTENT_HASH_DOMAIN);
    let mut storage_hasher = blake3::Hasher::new();
    storage_hasher.update(RAW_STORAGE_HASH_DOMAIN);
    let mut digest = blake3::Hasher::new();
    let mut buffer = [0_u8; STREAM_BUFFER_BYTES];
    let mut remaining = expected_len;
    while remaining != 0 {
        let request = usize::try_from(remaining.min(STREAM_BUFFER_BYTES as u64))
            .map_err(|_| Bcs2Error::InvalidExtent)?;
        let count = source.read(&mut buffer[..request])?;
        if count == 0 {
            return Err(Bcs2Error::FrameIdentityMismatch.into());
        }
        let bytes = &buffer[..count];
        output.write_all(bytes)?;
        content_hasher.update(bytes);
        storage_hasher.update(bytes);
        digest.update(bytes);
        remaining -= count as u64;
    }
    if source.read(&mut buffer[..1])? != 0 {
        return Err(Bcs2Error::FrameIdentityMismatch.into());
    }
    let content_id = ContentId::from_bytes(*content_hasher.finalize().as_bytes());
    if content_id != expected_content_id {
        return Err(Bcs2Error::FrameIdentityMismatch.into());
    }
    Ok(FrameVerification {
        storage_id: StorageId::from_bytes(*storage_hasher.finalize().as_bytes()),
        digest: *digest.finalize().as_bytes(),
    })
}

fn validate_bounds(bounds: ResourceBounds) -> Result<(), Bcs2Error> {
    if bounds.max_catalog_bytes == 0
        || bounds.max_index_entries == 0
        || bounds.max_frame_bytes == 0
        || bounds.max_generations == 0
    {
        return Err(Bcs2Error::BoundsExceeded);
    }
    Ok(())
}

#[derive(Debug)]
pub struct ForensicFileIndex {
    artifact: Bcs2FileIndex,
    metadata: ForensicTreeMetadata,
}

impl ForensicFileIndex {
    /// Validate a file-backed forensic capsule without mapping payload frames.
    pub fn open<R: Read + Seek>(
        reader: &mut R,
        supported_capabilities: u64,
        accepted_bounds: ResourceBounds,
    ) -> Result<Self, Bcs2FileError> {
        let artifact = Bcs2FileIndex::open(reader, supported_capabilities, accepted_bounds)?;
        if artifact.root_kind() != RootKind::Bundle
            || artifact.profile() != ProfileId::FORENSIC_TREE_V1
            || artifact.privacy_mode() != PrivacyMode::Plaintext
            || artifact.storage_contract() != StorageContract::SealedImmutable
            || !artifact.references().is_empty()
            || artifact
                .frames()
                .iter()
                .any(|frame| frame.kind() != FrameKind::RawBlob)
        {
            return Err(Bcs2Error::ProfileRootMismatch.into());
        }

        let (metadata_id, declared_entries) = parse_semantic_json(artifact.semantic_json())?;
        let metadata_frame =
            find_frame(artifact.frames(), metadata_id).ok_or(Bcs2Error::RootIdentityMismatch)?;
        if metadata_frame.required_capabilities() != 0 {
            return Err(Bcs2Error::FrameIdentityMismatch.into());
        }
        if forensic_tree_content_id(metadata_id) != artifact.root_content_id() {
            return Err(Bcs2Error::RootIdentityMismatch.into());
        }
        let metadata_bytes = read_frame_bounded(reader, metadata_frame)?;
        let (platform, entries) = decode_metadata(
            &metadata_bytes,
            artifact.bounds().max_index_entries as usize,
        )?;
        if entries.len() != declared_entries {
            return Err(Bcs2Error::CatalogCorrupt.into());
        }
        validate_metadata(&platform, &entries)?;

        let mut expected_frames = BTreeMap::new();
        expected_frames.insert(metadata_id, (metadata_frame.len(), 0_u64));
        for entry in &entries {
            let Some(frame_id) = entry.frame_content_id() else {
                continue;
            };
            let expected = (
                entry.frame_len().ok_or(Bcs2Error::FrameIdentityMismatch)?,
                entry.required_capabilities(),
            );
            if let Some(prior) = expected_frames.insert(frame_id, expected) {
                if prior != expected {
                    return Err(Bcs2Error::FrameIdentityMismatch.into());
                }
            }
            let frame = find_frame(artifact.frames(), frame_id)
                .ok_or(Bcs2Error::IncompletePortableClosure(frame_id))?;
            if (frame.len(), frame.required_capabilities()) != expected {
                return Err(Bcs2Error::FrameIdentityMismatch.into());
            }
        }
        let actual_frames: BTreeSet<_> = artifact
            .frames()
            .iter()
            .map(FileFrame::content_id)
            .collect();
        let expected_ids: BTreeSet<_> = expected_frames.keys().copied().collect();
        if actual_frames != expected_ids {
            return Err(Bcs2Error::ExtraPortableFrame(
                *actual_frames
                    .symmetric_difference(&expected_ids)
                    .next()
                    .ok_or(Bcs2Error::CatalogCorrupt)?,
            )
            .into());
        }

        Ok(Self {
            artifact,
            metadata: ForensicTreeMetadata { platform, entries },
        })
    }

    pub const fn root_content_id(&self) -> ContentId {
        self.artifact.root_content_id()
    }

    pub fn platform(&self) -> &str {
        &self.metadata.platform
    }

    pub fn entries(&self) -> &[ForensicEntryMetadata] {
        &self.metadata.entries
    }

    pub const fn artifact(&self) -> &Bcs2FileIndex {
        &self.artifact
    }
}

fn find_frame(frames: &[FileFrame], content_id: ContentId) -> Option<&FileFrame> {
    frames
        .binary_search_by_key(&content_id, FileFrame::content_id)
        .ok()
        .map(|index| &frames[index])
}

fn read_frame_bounded<R: Read + Seek>(
    reader: &mut R,
    frame: &FileFrame,
) -> Result<Vec<u8>, Bcs2FileError> {
    let len = usize::try_from(frame.len()).map_err(|_| Bcs2Error::InvalidExtent)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| Bcs2Error::BoundsExceeded)?;
    reader.seek(std::io::SeekFrom::Start(frame.offset()))?;
    let mut remaining = len;
    let mut buffer = [0_u8; STREAM_BUFFER_BYTES];
    while remaining != 0 {
        let request = remaining.min(STREAM_BUFFER_BYTES);
        reader.read_exact(&mut buffer[..request])?;
        bytes.extend_from_slice(&buffer[..request]);
        remaining -= request;
    }
    Ok(bytes)
}
