use abir::{canonical_debug_json, logical_content_id, AbirDataset, ContentId, StorageId};
use alloc::{vec, vec::Vec};
use core::fmt;
use minicbor::{Decoder, Encoder};

use crate::{encode_generation_footer, GenerationChain, GenerationFooter, GENERATION_FOOTER_LEN};

pub const BCS2_MAGIC: [u8; 8] = *b"ABIRBCS2";
pub const BCS2_HEADER_LEN: usize = 128;
pub(crate) const INDEX_MAGIC: [u8; 8] = *b"BCS2IDX\0";
pub(crate) const INDEX_LEN: usize = 48;
pub(crate) const INDEX_ENTRY_LEN: usize = 128;
const WIRE_MAJOR: u16 = 2;
const WIRE_MINOR: u16 = 0;
const SEMANTIC_GENERATION: u32 = 1;
const STORAGE_HASH_DOMAIN: &[u8] = b"org.quitetall.abir.bcs2.storage\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RootKind {
    Dataset = 1,
    Recording = 2,
    Stream = 3,
    Atom = 4,
    Blob = 5,
    Bundle = 6,
}

impl TryFrom<u8> for RootKind {
    type Error = Bcs2Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Dataset),
            2 => Ok(Self::Recording),
            3 => Ok(Self::Stream),
            4 => Ok(Self::Atom),
            5 => Ok(Self::Blob),
            6 => Ok(Self::Bundle),
            other => Err(Bcs2Error::UnknownRootKind(other)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum StorageContract {
    SealedImmutable = 1,
    SealedGenerational = 2,
    UnsealedWorkspace = 3,
    RewriteCompact = 4,
}

impl TryFrom<u8> for StorageContract {
    type Error = Bcs2Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::SealedImmutable),
            2 => Ok(Self::SealedGenerational),
            3 => Ok(Self::UnsealedWorkspace),
            4 => Ok(Self::RewriteCompact),
            other => Err(Bcs2Error::UnknownStorageContract(other)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum PrivacyMode {
    Plaintext = 1,
    EncryptedOpaque = 2,
    EncryptedDiscoverable = 3,
}

impl TryFrom<u8> for PrivacyMode {
    type Error = Bcs2Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Plaintext),
            2 => Ok(Self::EncryptedOpaque),
            3 => Ok(Self::EncryptedDiscoverable),
            other => Err(Bcs2Error::UnknownPrivacyMode(other)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileId(u32);

impl ProfileId {
    pub const LML_LOSSLESS_V1: Self = Self(0x0001_0001);
    pub const LMQ_PROGRESSIVE_V1: Self = Self(0x0002_0001);
    pub const TRAINING_BALANCED_V1: Self = Self(0x0003_0001);
    pub const TRAINING_COMPACT_V1: Self = Self(0x0003_0002);
    pub const STREAM_BOUNDED_V1: Self = Self(0x0004_0001);
    pub const FORENSIC_TREE_V1: Self = Self(0x0005_0001);
    pub const FORENSIC_IMAGE_V1: Self = Self(0x0005_0002);

    pub const fn get(self) -> u32 {
        self.0
    }

    fn from_registered(value: u32) -> Result<Self, Bcs2Error> {
        let profile = Self(value);
        match profile {
            Self::LML_LOSSLESS_V1
            | Self::LMQ_PROGRESSIVE_V1
            | Self::TRAINING_BALANCED_V1
            | Self::TRAINING_COMPACT_V1
            | Self::STREAM_BOUNDED_V1
            | Self::FORENSIC_TREE_V1
            | Self::FORENSIC_IMAGE_V1 => Ok(profile),
            _ => Err(Bcs2Error::UnknownProfile(value)),
        }
    }

    pub const fn accepts(self, root: RootKind) -> bool {
        match self {
            Self::LML_LOSSLESS_V1 => matches!(root, RootKind::Dataset | RootKind::Recording),
            Self::LMQ_PROGRESSIVE_V1 => {
                matches!(
                    root,
                    RootKind::Dataset | RootKind::Recording | RootKind::Stream
                )
            }
            Self::TRAINING_BALANCED_V1 | Self::TRAINING_COMPACT_V1 => {
                matches!(root, RootKind::Dataset | RootKind::Bundle)
            }
            Self::STREAM_BOUNDED_V1 => matches!(root, RootKind::Stream | RootKind::Bundle),
            Self::FORENSIC_TREE_V1 => matches!(root, RootKind::Dataset | RootKind::Bundle),
            Self::FORENSIC_IMAGE_V1 => matches!(root, RootKind::Blob | RootKind::Bundle),
            _ => false,
        }
    }

    pub const fn is_portable(self) -> bool {
        matches!(
            self,
            Self::LML_LOSSLESS_V1
                | Self::LMQ_PROGRESSIVE_V1
                | Self::TRAINING_COMPACT_V1
                | Self::FORENSIC_TREE_V1
                | Self::FORENSIC_IMAGE_V1
        )
    }

    pub const fn allows_external_references(self) -> bool {
        matches!(self, Self::TRAINING_BALANCED_V1 | Self::STREAM_BOUNDED_V1)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceBounds {
    pub max_catalog_bytes: u32,
    pub max_index_entries: u32,
    pub max_frame_bytes: u32,
    /// Reader-side chain traversal bound; it is not serialized in generation 2.
    pub max_generations: u32,
}

impl Default for ResourceBounds {
    fn default() -> Self {
        Self {
            max_catalog_bytes: 16 * 1024 * 1024,
            max_index_entries: 1_000_000,
            max_frame_bytes: 64 * 1024 * 1024,
            max_generations: 4_096,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum Bcs2Error {
    TooShort,
    BadMagic,
    UnsupportedVersion { major: u16, minor: u16 },
    UnsupportedSemanticGeneration(u32),
    UnsupportedCapabilities(u64),
    UnknownProfile(u32),
    UnknownRootKind(u8),
    UnknownStorageContract(u8),
    UnknownPrivacyMode(u8),
    StorageContractNotImplemented(StorageContract),
    PrivacyModeNotImplemented(PrivacyMode),
    UnsupportedIntegrity(u8),
    ProfileRootMismatch,
    ProfileNotPortable,
    DuplicateFrame,
    IncompletePortableClosure(ContentId),
    ExtraPortableFrame(ContentId),
    BoundsExceeded,
    InvalidExtent,
    NonCanonicalLayout,
    CatalogCorrupt,
    CatalogDigestMismatch,
    FrameDigestMismatch,
    FrameIdentityMismatch,
    RootIdentityMismatch,
    GenerationRootMismatch,
    SemanticEncoding,
}

impl fmt::Display for Bcs2Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BCS2 error: {self:?}")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Bcs2Error {}

pub fn encode_dataset(
    dataset: &AbirDataset,
    profile: ProfileId,
    bounds: ResourceBounds,
) -> Result<Vec<u8>, Bcs2Error> {
    encode_dataset_with_references(dataset, profile, bounds, [])
}

pub fn encode_dataset_with_references(
    dataset: &AbirDataset,
    profile: ProfileId,
    bounds: ResourceBounds,
    references: impl IntoIterator<Item = ContentId>,
) -> Result<Vec<u8>, Bcs2Error> {
    if bounds.max_catalog_bytes == 0
        || bounds.max_index_entries == 0
        || bounds.max_frame_bytes == 0
        || bounds.max_generations == 0
    {
        return Err(Bcs2Error::BoundsExceeded);
    }
    let root = RootKind::Dataset;
    if !profile.accepts(root) {
        return Err(Bcs2Error::ProfileRootMismatch);
    }
    let semantic_json = canonical_debug_json(dataset).map_err(|_| Bcs2Error::SemanticEncoding)?;
    if semantic_json.len() > bounds.max_catalog_bytes as usize {
        return Err(Bcs2Error::BoundsExceeded);
    }
    let root_id = logical_content_id(dataset).map_err(|_| Bcs2Error::SemanticEncoding)?;
    let references: alloc::collections::BTreeSet<_> = references.into_iter().collect();
    if references.len() > bounds.max_index_entries as usize {
        return Err(Bcs2Error::BoundsExceeded);
    }
    let mut encoder = Encoder::new(Vec::new());
    encoder
        .map(3)
        .and_then(|encoder| encoder.u8(1))
        .and_then(|encoder| encoder.bytes(&semantic_json))
        .and_then(|encoder| encoder.u8(2))
        .and_then(|encoder| encoder.bytes(root_id.as_bytes()))
        .and_then(|encoder| encoder.u8(3))
        .and_then(|encoder| encoder.array(references.len() as u64))
        .map_err(|_| Bcs2Error::SemanticEncoding)?;
    for reference in references {
        encoder
            .bytes(reference.as_bytes())
            .map_err(|_| Bcs2Error::SemanticEncoding)?;
    }
    let catalog = encoder.into_writer();
    if catalog.len() > bounds.max_catalog_bytes as usize {
        return Err(Bcs2Error::BoundsExceeded);
    }

    let catalog_offset = BCS2_HEADER_LEN;
    let index_offset = catalog_offset
        .checked_add(catalog.len())
        .ok_or(Bcs2Error::BoundsExceeded)?;
    let total = index_offset
        .checked_add(INDEX_LEN)
        .ok_or(Bcs2Error::BoundsExceeded)?;
    let mut bytes = vec![0_u8; total];
    bytes[..8].copy_from_slice(&BCS2_MAGIC);
    put_u16(&mut bytes, 8, WIRE_MAJOR);
    put_u16(&mut bytes, 10, WIRE_MINOR);
    put_u32(&mut bytes, 12, BCS2_HEADER_LEN as u32);
    put_u32(&mut bytes, 16, profile.get());
    put_u32(&mut bytes, 20, SEMANTIC_GENERATION);
    bytes[40] = root as u8;
    bytes[41] = StorageContract::SealedImmutable as u8;
    bytes[42] = PrivacyMode::Plaintext as u8;
    bytes[43] = 1;
    put_u32(&mut bytes, 44, bounds.max_catalog_bytes);
    put_u32(&mut bytes, 48, bounds.max_index_entries);
    put_u32(&mut bytes, 52, bounds.max_frame_bytes);
    put_u64(&mut bytes, 56, catalog_offset as u64);
    put_u64(&mut bytes, 64, catalog.len() as u64);
    put_u64(&mut bytes, 72, index_offset as u64);
    put_u64(&mut bytes, 80, INDEX_LEN as u64);
    bytes[96..128].copy_from_slice(root_id.as_bytes());
    bytes[catalog_offset..index_offset].copy_from_slice(&catalog);
    bytes[index_offset..index_offset + 8].copy_from_slice(&INDEX_MAGIC);
    let catalog_digest = blake3::hash(&catalog);
    bytes[index_offset + 16..index_offset + 48].copy_from_slice(catalog_digest.as_bytes());
    Ok(bytes)
}

/// Encodes generation zero of an append-only sealed artifact.
pub fn encode_generational_dataset(
    dataset: &AbirDataset,
    profile: ProfileId,
    bounds: ResourceBounds,
    references: impl IntoIterator<Item = ContentId>,
) -> Result<Vec<u8>, Bcs2Error> {
    let mut bytes = encode_dataset_with_references(dataset, profile, bounds, references)?;
    let catalog_offset = get_u64(&bytes, 56)?;
    let catalog_len = get_u64(&bytes, 64)?;
    let index_offset = get_u64(&bytes, 72)?;
    let index_len = get_u64(&bytes, 80)?;
    let root_content_id = content_id_at(&bytes, 96)?;
    let footer_offset = bytes.len() as u64;
    let footer = encode_generation_footer(
        &bytes,
        GenerationFooter {
            generation: 0,
            previous_offset: 0,
            previous_digest: [0; 32],
            catalog_offset,
            catalog_len,
            index_offset,
            index_len,
            root_content_id,
            digest: [0; 32],
        },
    )?;
    bytes.extend_from_slice(&footer);
    bytes[41] = StorageContract::SealedGenerational as u8;
    put_u64(&mut bytes, 88, footer_offset);
    Ok(bytes)
}

/// Appends a new immutable generation and updates the envelope's latest pointer.
pub fn append_dataset_generation(
    artifact: &mut Vec<u8>,
    dataset: &AbirDataset,
    references: impl IntoIterator<Item = ContentId>,
    supported_capabilities: u64,
    accepted_bounds: ResourceBounds,
) -> Result<(), Bcs2Error> {
    let current = Bcs2View::parse(artifact, supported_capabilities, accepted_bounds)?;
    if current.storage_contract != StorageContract::SealedGenerational {
        return Err(Bcs2Error::StorageContractNotImplemented(
            current.storage_contract,
        ));
    }
    let latest_offset = get_u64(artifact, 88)?;
    let previous = GenerationFooter::parse(artifact, latest_offset)?;
    let next_generation = previous
        .generation
        .checked_add(1)
        .ok_or(Bcs2Error::BoundsExceeded)?;
    if next_generation >= accepted_bounds.max_generations as u64 {
        return Err(Bcs2Error::BoundsExceeded);
    }
    let encoded =
        encode_dataset_with_references(dataset, current.profile, current.bounds, references)?;
    let source_catalog_offset = to_usize(get_u64(&encoded, 56)?)?;
    let source_catalog_len = to_usize(get_u64(&encoded, 64)?)?;
    let source_index_offset = to_usize(get_u64(&encoded, 72)?)?;
    let source_index_len = to_usize(get_u64(&encoded, 80)?)?;
    let source_catalog_end = source_catalog_offset
        .checked_add(source_catalog_len)
        .ok_or(Bcs2Error::InvalidExtent)?;
    let source_index_end = source_index_offset
        .checked_add(source_index_len)
        .ok_or(Bcs2Error::InvalidExtent)?;
    let catalog = encoded
        .get(source_catalog_offset..source_catalog_end)
        .ok_or(Bcs2Error::InvalidExtent)?;
    let index = encoded
        .get(source_index_offset..source_index_end)
        .ok_or(Bcs2Error::InvalidExtent)?;
    let catalog_offset = artifact.len() as u64;
    artifact.extend_from_slice(catalog);
    let index_offset = artifact.len() as u64;
    artifact.extend_from_slice(index);
    let footer_offset = artifact.len() as u64;
    let root_content_id = content_id_at(&encoded, 96)?;
    let footer = encode_generation_footer(
        artifact,
        GenerationFooter {
            generation: next_generation,
            previous_offset: latest_offset,
            previous_digest: previous.digest,
            catalog_offset,
            catalog_len: catalog.len() as u64,
            index_offset,
            index_len: index.len() as u64,
            root_content_id,
            digest: [0; 32],
        },
    )?;
    artifact.extend_from_slice(&footer);
    put_u64(artifact, 88, footer_offset);
    artifact[96..128].copy_from_slice(root_content_id.as_bytes());
    Ok(())
}

#[derive(Debug)]
pub struct Bcs2View<'a> {
    bytes: &'a [u8],
    profile: ProfileId,
    root_kind: RootKind,
    storage_contract: StorageContract,
    privacy_mode: PrivacyMode,
    bounds: ResourceBounds,
    root_content_id: ContentId,
    semantic_json: &'a [u8],
    references: Vec<ContentId>,
    frames: Vec<FrameView<'a>>,
    generation_chain: Option<GenerationChain>,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameView<'a> {
    content_id: ContentId,
    storage_id: StorageId,
    bytes: &'a [u8],
}

impl<'a> FrameView<'a> {
    pub const fn content_id(&self) -> ContentId {
        self.content_id
    }
    pub const fn storage_id(&self) -> StorageId {
        self.storage_id
    }
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

impl<'a> Bcs2View<'a> {
    pub fn parse(
        bytes: &'a [u8],
        supported_capabilities: u64,
        accepted_bounds: ResourceBounds,
    ) -> Result<Self, Bcs2Error> {
        Self::parse_inner(bytes, supported_capabilities, accepted_bounds, true)
    }

    fn parse_inner(
        bytes: &'a [u8],
        supported_capabilities: u64,
        accepted_bounds: ResourceBounds,
        allow_frames: bool,
    ) -> Result<Self, Bcs2Error> {
        if bytes.len() < BCS2_HEADER_LEN + INDEX_LEN {
            return Err(Bcs2Error::TooShort);
        }
        if bytes[..8] != BCS2_MAGIC {
            return Err(Bcs2Error::BadMagic);
        }
        let major = get_u16(bytes, 8)?;
        let minor = get_u16(bytes, 10)?;
        if major != WIRE_MAJOR || minor != WIRE_MINOR {
            return Err(Bcs2Error::UnsupportedVersion { major, minor });
        }
        if get_u32(bytes, 12)? != BCS2_HEADER_LEN as u32 {
            return Err(Bcs2Error::NonCanonicalLayout);
        }
        let profile = ProfileId::from_registered(get_u32(bytes, 16)?)?;
        let semantic_generation = get_u32(bytes, 20)?;
        if semantic_generation != SEMANTIC_GENERATION {
            return Err(Bcs2Error::UnsupportedSemanticGeneration(
                semantic_generation,
            ));
        }
        let required = get_u64(bytes, 24)?;
        let unsupported = required & !supported_capabilities;
        if unsupported != 0 {
            return Err(Bcs2Error::UnsupportedCapabilities(unsupported));
        }
        let _optional_capabilities = get_u64(bytes, 32)?;
        let root_kind = RootKind::try_from(bytes[40])?;
        if !profile.accepts(root_kind) {
            return Err(Bcs2Error::ProfileRootMismatch);
        }
        let storage_contract = StorageContract::try_from(bytes[41])?;
        let privacy_mode = PrivacyMode::try_from(bytes[42])?;
        if !matches!(
            storage_contract,
            StorageContract::SealedImmutable | StorageContract::SealedGenerational
        ) {
            return Err(Bcs2Error::StorageContractNotImplemented(storage_contract));
        }
        if privacy_mode != PrivacyMode::Plaintext {
            return Err(Bcs2Error::PrivacyModeNotImplemented(privacy_mode));
        }
        if bytes[43] != 1 {
            return Err(Bcs2Error::UnsupportedIntegrity(bytes[43]));
        }
        let bounds = ResourceBounds {
            max_catalog_bytes: get_u32(bytes, 44)?,
            max_index_entries: get_u32(bytes, 48)?,
            max_frame_bytes: get_u32(bytes, 52)?,
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
            return Err(Bcs2Error::BoundsExceeded);
        }
        let envelope_catalog_offset = get_u64(bytes, 56)?;
        let envelope_catalog_len = get_u64(bytes, 64)?;
        let envelope_index_offset = get_u64(bytes, 72)?;
        let envelope_index_len = get_u64(bytes, 80)?;
        let latest_footer_offset = get_u64(bytes, 88)?;
        let mut generation_chain = None;
        let (catalog_offset, catalog_len, index_offset, index_len, expected_end) =
            if storage_contract == StorageContract::SealedGenerational {
                let chain = GenerationChain::parse(
                    bytes,
                    latest_footer_offset,
                    accepted_bounds.max_generations,
                )?;
                let latest = chain
                    .newest_first()
                    .first()
                    .ok_or(Bcs2Error::CatalogCorrupt)?;
                let envelope_root = content_id_at(bytes, 96)?;
                if latest.root_content_id != envelope_root {
                    return Err(Bcs2Error::GenerationRootMismatch);
                }
                let oldest = chain
                    .newest_first()
                    .last()
                    .ok_or(Bcs2Error::CatalogCorrupt)?;
                if oldest.catalog_offset != envelope_catalog_offset
                    || oldest.catalog_len != envelope_catalog_len
                    || oldest.index_offset != envelope_index_offset
                    || oldest.index_len != envelope_index_len
                {
                    return Err(Bcs2Error::NonCanonicalLayout);
                }
                let expected_end = to_usize(latest_footer_offset)?
                    .checked_add(GENERATION_FOOTER_LEN)
                    .ok_or(Bcs2Error::InvalidExtent)?;
                let extents = (
                    to_usize(latest.catalog_offset)?,
                    to_usize(latest.catalog_len)?,
                    to_usize(latest.index_offset)?,
                    to_usize(latest.index_len)?,
                    expected_end,
                );
                generation_chain = Some(chain);
                extents
            } else {
                if latest_footer_offset != 0 {
                    return Err(Bcs2Error::NonCanonicalLayout);
                }
                (
                    to_usize(envelope_catalog_offset)?,
                    to_usize(envelope_catalog_len)?,
                    to_usize(envelope_index_offset)?,
                    to_usize(envelope_index_len)?,
                    bytes.len(),
                )
            };
        if catalog_len > bounds.max_catalog_bytes as usize {
            return Err(Bcs2Error::BoundsExceeded);
        }
        let catalog_end = catalog_offset
            .checked_add(catalog_len)
            .ok_or(Bcs2Error::InvalidExtent)?;
        let index_end = index_offset
            .checked_add(index_len)
            .ok_or(Bcs2Error::InvalidExtent)?;
        if (storage_contract == StorageContract::SealedImmutable
            && catalog_offset != BCS2_HEADER_LEN)
            || catalog_end > index_offset
            || index_len < INDEX_LEN
            || index_end
                .checked_add(if storage_contract == StorageContract::SealedGenerational {
                    GENERATION_FOOTER_LEN
                } else {
                    0
                })
                .ok_or(Bcs2Error::InvalidExtent)?
                != expected_end
            || expected_end != bytes.len()
        {
            return Err(Bcs2Error::NonCanonicalLayout);
        }
        let catalog = bytes
            .get(catalog_offset..catalog_end)
            .ok_or(Bcs2Error::InvalidExtent)?;
        let index = bytes
            .get(index_offset..index_end)
            .ok_or(Bcs2Error::InvalidExtent)?;
        if index[..8] != INDEX_MAGIC || index[12..16].iter().any(|byte| *byte != 0) {
            return Err(Bcs2Error::CatalogCorrupt);
        }
        if blake3::hash(catalog).as_bytes() != &index[16..48] {
            return Err(Bcs2Error::CatalogDigestMismatch);
        }
        let frame_count = get_u32(index, 8)? as usize;
        if frame_count > bounds.max_index_entries as usize {
            return Err(Bcs2Error::BoundsExceeded);
        }
        if frame_count != 0 && !allow_frames {
            return Err(Bcs2Error::DuplicateFrame);
        }
        let expected_index_len = INDEX_LEN
            .checked_add(
                frame_count
                    .checked_mul(INDEX_ENTRY_LEN)
                    .ok_or(Bcs2Error::InvalidExtent)?,
            )
            .ok_or(Bcs2Error::InvalidExtent)?;
        if index_len != expected_index_len {
            return Err(Bcs2Error::NonCanonicalLayout);
        }
        let mut frames = Vec::with_capacity(frame_count);
        let mut expected_frame_offset = catalog_end;
        for entry_number in 0..frame_count {
            let entry_offset = INDEX_LEN + entry_number * INDEX_ENTRY_LEN;
            let entry = &index[entry_offset..entry_offset + INDEX_ENTRY_LEN];
            if entry[80] != 1 || entry[81] != 0 || entry[82..96].iter().any(|byte| *byte != 0) {
                return Err(Bcs2Error::CatalogCorrupt);
            }
            let content_id = content_id_at(entry, 0)?;
            if frames
                .last()
                .is_some_and(|prior: &FrameView<'_>| prior.content_id >= content_id)
            {
                return Err(Bcs2Error::CatalogCorrupt);
            }
            let storage_id = storage_id_at(entry, 32)?;
            let frame_offset = to_usize(get_u64(entry, 64)?)?;
            let frame_len = to_usize(get_u64(entry, 72)?)?;
            if frame_offset != expected_frame_offset
                || frame_len == 0
                || frame_len > bounds.max_frame_bytes as usize
            {
                return Err(Bcs2Error::NonCanonicalLayout);
            }
            let frame_end = frame_offset
                .checked_add(frame_len)
                .ok_or(Bcs2Error::InvalidExtent)?;
            let frame = bytes
                .get(frame_offset..frame_end)
                .ok_or(Bcs2Error::InvalidExtent)?;
            if blake3::hash(frame).as_bytes() != &entry[96..128] {
                return Err(Bcs2Error::FrameDigestMismatch);
            }
            if storage_id_for(frame) != storage_id {
                return Err(Bcs2Error::FrameIdentityMismatch);
            }
            let embedded =
                Self::parse_inner(frame, supported_capabilities, accepted_bounds, false)?;
            if embedded.root_content_id != content_id || embedded.storage_id() != storage_id {
                return Err(Bcs2Error::FrameIdentityMismatch);
            }
            frames.push(FrameView {
                content_id,
                storage_id,
                bytes: frame,
            });
            expected_frame_offset = frame_end;
        }
        if expected_frame_offset != index_offset {
            return Err(Bcs2Error::NonCanonicalLayout);
        }
        let mut root_bytes = [0_u8; 32];
        root_bytes.copy_from_slice(&bytes[96..128]);
        let root_content_id = ContentId::from_bytes(root_bytes);
        let mut decoder = Decoder::new(catalog);
        if decoder.map().map_err(|_| Bcs2Error::CatalogCorrupt)? != Some(3)
            || decoder.u8().map_err(|_| Bcs2Error::CatalogCorrupt)? != 1
        {
            return Err(Bcs2Error::CatalogCorrupt);
        }
        let semantic_json = decoder.bytes().map_err(|_| Bcs2Error::CatalogCorrupt)?;
        if decoder.u8().map_err(|_| Bcs2Error::CatalogCorrupt)? != 2 {
            return Err(Bcs2Error::CatalogCorrupt);
        }
        let embedded_root = decoder.bytes().map_err(|_| Bcs2Error::CatalogCorrupt)?;
        if embedded_root != root_content_id.as_bytes() {
            return Err(Bcs2Error::RootIdentityMismatch);
        }
        if decoder.u8().map_err(|_| Bcs2Error::CatalogCorrupt)? != 3 {
            return Err(Bcs2Error::CatalogCorrupt);
        }
        let reference_count = decoder
            .array()
            .map_err(|_| Bcs2Error::CatalogCorrupt)?
            .ok_or(Bcs2Error::CatalogCorrupt)?;
        if reference_count > bounds.max_index_entries as u64 {
            return Err(Bcs2Error::BoundsExceeded);
        }
        let mut references = Vec::with_capacity(reference_count as usize);
        for _ in 0..reference_count {
            let encoded = decoder.bytes().map_err(|_| Bcs2Error::CatalogCorrupt)?;
            let bytes: [u8; 32] = encoded.try_into().map_err(|_| Bcs2Error::CatalogCorrupt)?;
            let reference = ContentId::from_bytes(bytes);
            if references.last().is_some_and(|prior| prior >= &reference) {
                return Err(Bcs2Error::CatalogCorrupt);
            }
            references.push(reference);
        }
        if decoder.position() != catalog.len() {
            return Err(Bcs2Error::CatalogCorrupt);
        }
        Ok(Self {
            bytes,
            profile,
            root_kind,
            storage_contract,
            privacy_mode,
            bounds,
            root_content_id,
            semantic_json,
            references,
            frames,
            generation_chain,
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
    pub const fn semantic_json(&self) -> &'a [u8] {
        self.semantic_json
    }
    pub fn references(&self) -> &[ContentId] {
        &self.references
    }
    pub fn frames(&self) -> &[FrameView<'a>] {
        &self.frames
    }
    pub const fn artifact_bytes(&self) -> &'a [u8] {
        self.bytes
    }
    pub fn generation_chain(&self) -> Option<&GenerationChain> {
        self.generation_chain.as_ref()
    }
    pub fn storage_id(&self) -> StorageId {
        storage_id_for(self.bytes)
    }
}

fn content_id_at(bytes: &[u8], offset: usize) -> Result<ContentId, Bcs2Error> {
    let encoded = bytes.get(offset..offset + 32).ok_or(Bcs2Error::TooShort)?;
    let value: [u8; 32] = encoded.try_into().map_err(|_| Bcs2Error::TooShort)?;
    Ok(ContentId::from_bytes(value))
}

fn storage_id_at(bytes: &[u8], offset: usize) -> Result<StorageId, Bcs2Error> {
    let encoded = bytes.get(offset..offset + 32).ok_or(Bcs2Error::TooShort)?;
    let value: [u8; 32] = encoded.try_into().map_err(|_| Bcs2Error::TooShort)?;
    Ok(StorageId::from_bytes(value))
}

pub(crate) fn storage_id_for(bytes: &[u8]) -> StorageId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(STORAGE_HASH_DOMAIN);
    hasher.update(bytes);
    StorageId::from_bytes(*hasher.finalize().as_bytes())
}

fn get_u16(bytes: &[u8], offset: usize) -> Result<u16, Bcs2Error> {
    let value = bytes.get(offset..offset + 2).ok_or(Bcs2Error::TooShort)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn get_u32(bytes: &[u8], offset: usize) -> Result<u32, Bcs2Error> {
    let value = bytes.get(offset..offset + 4).ok_or(Bcs2Error::TooShort)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

pub(crate) fn get_u64(bytes: &[u8], offset: usize) -> Result<u64, Bcs2Error> {
    let value = bytes.get(offset..offset + 8).ok_or(Bcs2Error::TooShort)?;
    Ok(u64::from_le_bytes([
        value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
    ]))
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn to_usize(value: u64) -> Result<usize, Bcs2Error> {
    usize::try_from(value).map_err(|_| Bcs2Error::InvalidExtent)
}
