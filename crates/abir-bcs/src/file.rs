use crate::{
    wire::{
        content_id_at, element_from_wire_code, get_u16, get_u32, get_u64, parse_catalog,
        storage_id_at, INDEX_ENTRY_LEN, INDEX_LEN, INDEX_MAGIC, RAW_CONTENT_HASH_DOMAIN,
        RAW_STORAGE_HASH_DOMAIN, SEMANTIC_GENERATION, WIRE_MAJOR, WIRE_MINOR,
    },
    Bcs2Error, FrameKind, PrivacyMode, ProfileId, ResourceBounds, RootKind, StorageContract,
};
use abir::{ContentId, ElementType, PayloadContentHasher, StorageId};
use std::fmt;
use std::io::{Read, Seek, SeekFrom};

const READ_BUFFER_BYTES: usize = 64 * 1024;

/// I/O or structural failure while indexing a sealed BCS2 file.
#[derive(Debug)]
pub enum Bcs2FileError {
    Bcs2(Bcs2Error),
    Io(std::io::Error),
}

impl fmt::Display for Bcs2FileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bcs2(error) => error.fmt(formatter),
            Self::Io(error) => write!(formatter, "BCS2 file I/O error: {error}"),
        }
    }
}

impl std::error::Error for Bcs2FileError {}

impl From<Bcs2Error> for Bcs2FileError {
    fn from(value: Bcs2Error) -> Self {
        Self::Bcs2(value)
    }
}

impl From<std::io::Error> for Bcs2FileError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// Validated location and identity of one frame in a sealed BCS2 file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileFrame {
    kind: FrameKind,
    element: Option<ElementType>,
    content_id: ContentId,
    storage_id: StorageId,
    required_capabilities: u64,
    offset: u64,
    len: u64,
}

impl FileFrame {
    pub const fn kind(&self) -> FrameKind {
        self.kind
    }

    pub const fn element(&self) -> Option<ElementType> {
        self.element
    }

    pub const fn content_id(&self) -> ContentId {
        self.content_id
    }

    pub const fn storage_id(&self) -> StorageId {
        self.storage_id
    }

    pub const fn required_capabilities(&self) -> u64 {
        self.required_capabilities
    }

    pub const fn offset(&self) -> u64 {
        self.offset
    }

    pub const fn len(&self) -> u64 {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Owned metadata for a fully validated sealed BCS2 file.
///
/// Unlike [`crate::Bcs2View`], this index never creates a slice over the whole
/// file. Catalog and index metadata are bounded and owned; frame bytes are
/// streamed through a fixed-size buffer and represented only by validated
/// offsets. This permits safe access to very large files without an mmap or a
/// private artifact-sized copy.
#[derive(Debug)]
pub struct Bcs2FileIndex {
    profile: ProfileId,
    root_kind: RootKind,
    storage_contract: StorageContract,
    privacy_mode: PrivacyMode,
    bounds: ResourceBounds,
    root_content_id: ContentId,
    semantic_json: Vec<u8>,
    references: Vec<ContentId>,
    frames: Vec<FileFrame>,
    artifact_len: u64,
}

impl Bcs2FileIndex {
    /// Validate and index a sealed immutable BCS2 artifact through `Read + Seek`.
    ///
    /// Embedded BCS2 frames are refused because validating their recursive
    /// structure would require materializing the complete embedded artifact.
    /// Raw and semantic payload frames are validated in bounded memory.
    pub fn open<R: Read + Seek>(
        reader: &mut R,
        supported_capabilities: u64,
        accepted_bounds: ResourceBounds,
    ) -> Result<Self, Bcs2FileError> {
        let artifact_len = reader.seek(SeekFrom::End(0))?;
        if artifact_len < u64::try_from(crate::BCS2_HEADER_LEN + INDEX_LEN).unwrap() {
            return Err(Bcs2Error::TooShort.into());
        }

        let mut header = [0_u8; crate::BCS2_HEADER_LEN];
        read_exact_at(reader, 0, &mut header)?;
        if header[..8] != crate::BCS2_MAGIC {
            return Err(Bcs2Error::BadMagic.into());
        }
        let major = get_u16(&header, 8)?;
        let minor = get_u16(&header, 10)?;
        if major != WIRE_MAJOR || minor != WIRE_MINOR {
            return Err(Bcs2Error::UnsupportedVersion { major, minor }.into());
        }
        if get_u32(&header, 12)? != crate::BCS2_HEADER_LEN as u32 {
            return Err(Bcs2Error::NonCanonicalLayout.into());
        }

        let profile = ProfileId::from_registered(get_u32(&header, 16)?)?;
        let semantic_generation = get_u32(&header, 20)?;
        if semantic_generation != SEMANTIC_GENERATION {
            return Err(Bcs2Error::UnsupportedSemanticGeneration(semantic_generation).into());
        }
        let required = get_u64(&header, 24)?;
        let unsupported = required & !supported_capabilities;
        if unsupported != 0 {
            return Err(Bcs2Error::UnsupportedCapabilities(unsupported).into());
        }
        let root_kind = RootKind::try_from(header[40])?;
        if !profile.accepts(root_kind) {
            return Err(Bcs2Error::ProfileRootMismatch.into());
        }
        let storage_contract = StorageContract::try_from(header[41])?;
        if storage_contract != StorageContract::SealedImmutable {
            return Err(Bcs2Error::StorageContractNotImplemented(storage_contract).into());
        }
        let privacy_mode = PrivacyMode::try_from(header[42])?;
        if privacy_mode != PrivacyMode::Plaintext {
            return Err(Bcs2Error::PrivacyModeNotImplemented(privacy_mode).into());
        }
        if header[43] != 1 {
            return Err(Bcs2Error::UnsupportedIntegrity(header[43]).into());
        }
        let bounds = ResourceBounds {
            max_catalog_bytes: get_u32(&header, 44)?,
            max_index_entries: get_u32(&header, 48)?,
            max_frame_bytes: get_u32(&header, 52)?,
            max_generations: accepted_bounds.max_generations,
        };
        if bounds.max_catalog_bytes == 0
            || bounds.max_index_entries == 0
            || bounds.max_frame_bytes == 0
            || bounds.max_generations == 0
            || bounds.max_catalog_bytes > accepted_bounds.max_catalog_bytes
            || bounds.max_index_entries > accepted_bounds.max_index_entries
            || bounds.max_frame_bytes > accepted_bounds.max_frame_bytes
        {
            return Err(Bcs2Error::BoundsExceeded.into());
        }

        let catalog_offset = get_u64(&header, 56)?;
        let catalog_len = get_u64(&header, 64)?;
        let index_offset = get_u64(&header, 72)?;
        let index_len = get_u64(&header, 80)?;
        if get_u64(&header, 88)? != 0
            || catalog_offset != crate::BCS2_HEADER_LEN as u64
            || catalog_len > u64::from(bounds.max_catalog_bytes)
        {
            return Err(Bcs2Error::NonCanonicalLayout.into());
        }
        let catalog_end = catalog_offset
            .checked_add(catalog_len)
            .ok_or(Bcs2Error::InvalidExtent)?;
        let index_end = index_offset
            .checked_add(index_len)
            .ok_or(Bcs2Error::InvalidExtent)?;
        if catalog_end > index_offset || index_len < INDEX_LEN as u64 || index_end != artifact_len {
            return Err(Bcs2Error::NonCanonicalLayout.into());
        }

        let catalog_len_usize =
            usize::try_from(catalog_len).map_err(|_| Bcs2Error::InvalidExtent)?;
        let mut catalog = vec![0_u8; catalog_len_usize];
        read_exact_at(reader, catalog_offset, &mut catalog)?;

        let mut index_header = [0_u8; INDEX_LEN];
        read_exact_at(reader, index_offset, &mut index_header)?;
        if index_header[..8] != INDEX_MAGIC || index_header[12..16].iter().any(|byte| *byte != 0) {
            return Err(Bcs2Error::CatalogCorrupt.into());
        }
        if blake3::hash(&catalog).as_bytes() != &index_header[16..48] {
            return Err(Bcs2Error::CatalogDigestMismatch.into());
        }
        let frame_count =
            usize::try_from(get_u32(&index_header, 8)?).map_err(|_| Bcs2Error::BoundsExceeded)?;
        if frame_count > bounds.max_index_entries as usize {
            return Err(Bcs2Error::BoundsExceeded.into());
        }
        let expected_index_len = INDEX_LEN
            .checked_add(
                frame_count
                    .checked_mul(INDEX_ENTRY_LEN)
                    .ok_or(Bcs2Error::InvalidExtent)?,
            )
            .ok_or(Bcs2Error::InvalidExtent)?;
        if index_len != expected_index_len as u64 {
            return Err(Bcs2Error::NonCanonicalLayout.into());
        }

        let mut frames = Vec::with_capacity(frame_count);
        let mut expected_frame_offset = catalog_end;
        for entry_number in 0..frame_count {
            let entry_offset = index_offset
                .checked_add(INDEX_LEN as u64)
                .and_then(|value| {
                    value.checked_add(
                        u64::try_from(entry_number)
                            .ok()?
                            .checked_mul(INDEX_ENTRY_LEN as u64)?,
                    )
                })
                .ok_or(Bcs2Error::InvalidExtent)?;
            let mut entry = [0_u8; INDEX_ENTRY_LEN];
            read_exact_at(reader, entry_offset, &mut entry)?;
            let kind = FrameKind::try_from(entry[80])?;
            if kind == FrameKind::EmbeddedBcs2 {
                return Err(Bcs2Error::FileFrameKindNotSupported(kind).into());
            }
            let element = if kind == FrameKind::SemanticPayload {
                Some(element_from_wire_code(entry[81])?)
            } else {
                if entry[81] != 0 {
                    return Err(Bcs2Error::CatalogCorrupt.into());
                }
                None
            };
            let frame_capabilities = get_u64(&entry, 82)?;
            if frame_capabilities & !required != 0 || entry[90..96].iter().any(|byte| *byte != 0) {
                return Err(Bcs2Error::CatalogCorrupt.into());
            }
            let content_id = content_id_at(&entry, 0)?;
            if frames
                .last()
                .is_some_and(|prior: &FileFrame| prior.content_id >= content_id)
            {
                return Err(Bcs2Error::CatalogCorrupt.into());
            }
            let storage_id = storage_id_at(&entry, 32)?;
            let frame_offset = get_u64(&entry, 64)?;
            let frame_len = get_u64(&entry, 72)?;
            if frame_offset != expected_frame_offset
                || frame_len > u64::from(bounds.max_frame_bytes)
            {
                return Err(Bcs2Error::NonCanonicalLayout.into());
            }
            let frame_end = frame_offset
                .checked_add(frame_len)
                .ok_or(Bcs2Error::InvalidExtent)?;
            if frame_end > index_offset {
                return Err(Bcs2Error::InvalidExtent.into());
            }
            verify_frame(
                reader,
                kind,
                element,
                frame_offset,
                frame_len,
                content_id,
                storage_id,
                &entry[96..128],
            )?;
            frames.push(FileFrame {
                kind,
                element,
                content_id,
                storage_id,
                required_capabilities: frame_capabilities,
                offset: frame_offset,
                len: frame_len,
            });
            expected_frame_offset = frame_end;
        }
        if expected_frame_offset != index_offset {
            return Err(Bcs2Error::NonCanonicalLayout.into());
        }

        let root_content_id = content_id_at(&header, 96)?;
        let parsed_catalog = parse_catalog(&catalog, root_content_id, bounds.max_index_entries)?;
        Ok(Self {
            profile,
            root_kind,
            storage_contract,
            privacy_mode,
            bounds,
            root_content_id,
            semantic_json: parsed_catalog.semantic_json.to_vec(),
            references: parsed_catalog.references,
            frames,
            artifact_len,
        })
    }

    pub const fn profile(&self) -> ProfileId {
        self.profile
    }

    pub const fn root_kind(&self) -> RootKind {
        self.root_kind
    }

    pub const fn storage_contract(&self) -> StorageContract {
        self.storage_contract
    }

    pub const fn privacy_mode(&self) -> PrivacyMode {
        self.privacy_mode
    }

    pub const fn bounds(&self) -> ResourceBounds {
        self.bounds
    }

    pub const fn root_content_id(&self) -> ContentId {
        self.root_content_id
    }

    pub fn semantic_json(&self) -> &[u8] {
        &self.semantic_json
    }

    pub fn references(&self) -> &[ContentId] {
        &self.references
    }

    pub fn frames(&self) -> &[FileFrame] {
        &self.frames
    }

    pub const fn artifact_len(&self) -> u64 {
        self.artifact_len
    }
}

fn read_exact_at<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    destination: &mut [u8],
) -> Result<(), Bcs2FileError> {
    reader.seek(SeekFrom::Start(offset))?;
    reader.read_exact(destination)?;
    Ok(())
}

enum FrameContentHasher {
    Raw(blake3::Hasher),
    Semantic(PayloadContentHasher),
}

impl FrameContentHasher {
    fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Raw(hasher) => {
                hasher.update(bytes);
            }
            Self::Semantic(hasher) => hasher.update(bytes),
        }
    }

    fn finalize(self) -> ContentId {
        match self {
            Self::Raw(hasher) => ContentId::from_bytes(*hasher.finalize().as_bytes()),
            Self::Semantic(hasher) => hasher.finalize(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn verify_frame<R: Read + Seek>(
    reader: &mut R,
    kind: FrameKind,
    element: Option<ElementType>,
    offset: u64,
    len: u64,
    expected_content_id: ContentId,
    expected_storage_id: StorageId,
    expected_digest: &[u8],
) -> Result<(), Bcs2FileError> {
    reader.seek(SeekFrom::Start(offset))?;
    let mut frame_digest = blake3::Hasher::new();
    let mut storage_digest = blake3::Hasher::new();
    storage_digest.update(RAW_STORAGE_HASH_DOMAIN);
    let mut content_digest = match kind {
        FrameKind::RawBlob => {
            let mut hasher = blake3::Hasher::new();
            hasher.update(RAW_CONTENT_HASH_DOMAIN);
            FrameContentHasher::Raw(hasher)
        }
        FrameKind::SemanticPayload => FrameContentHasher::Semantic(PayloadContentHasher::new(
            element.ok_or(Bcs2Error::CatalogCorrupt)?,
        )),
        FrameKind::EmbeddedBcs2 => {
            return Err(Bcs2Error::FileFrameKindNotSupported(kind).into());
        }
    };

    let mut remaining = len;
    let mut buffer = [0_u8; READ_BUFFER_BYTES];
    while remaining != 0 {
        let count = usize::try_from(remaining.min(READ_BUFFER_BYTES as u64))
            .map_err(|_| Bcs2Error::InvalidExtent)?;
        reader.read_exact(&mut buffer[..count])?;
        frame_digest.update(&buffer[..count]);
        storage_digest.update(&buffer[..count]);
        content_digest.update(&buffer[..count]);
        remaining -= count as u64;
    }
    if frame_digest.finalize().as_bytes() != expected_digest {
        return Err(Bcs2Error::FrameDigestMismatch.into());
    }
    if content_digest.finalize() != expected_content_id
        || StorageId::from_bytes(*storage_digest.finalize().as_bytes()) != expected_storage_id
    {
        return Err(Bcs2Error::FrameIdentityMismatch.into());
    }
    Ok(())
}
