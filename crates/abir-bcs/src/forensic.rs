use crate::wire::{encode_raw_root_with_capabilities, raw_content_id};
use crate::{
    Bcs2Error, Bcs2View, FrameKind, PrivacyMode, ProfileId, ResourceBounds, RootKind,
    StorageContract, CAP_LMA_SYNTHETIC_REEMIT,
};
use abir::ContentId;
use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minicbor::data::Type;
use minicbor::{Decoder, Encoder};

/// Metadata document shape holding one field group per entry.
const FORENSIC_TREE_VERSION: u8 = 1;
/// Same shape plus a per-entry stored-form declaration.
///
/// The version is a function of the content, not a global switch: a tree in
/// which every entry is stored verbatim still encodes as v1, byte for byte.
/// That keeps every capsule written before stored forms existed re-encoding
/// identically under `decode_metadata`'s canonicalisation check, so this
/// extension costs no existing artifact its readability.
const FORENSIC_TREE_VERSION_STORED_FORM: u8 = 2;
/// Stored-form shape with a fixed capability-defined parameter descriptor.
///
/// Version 2 remains readable and maps to an all-zero descriptor. Version 3 is
/// selected only when at least one transformed entry carries non-zero
/// parameters, preserving every version-2 artifact byte for byte.
const FORENSIC_TREE_VERSION_TRANSFORM_PARAMETERS: u8 = 3;
const FORENSIC_TREE_DOMAIN: &[u8] = b"org.quitetall.abir.bcs2.forensic-tree-v1\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ForensicFileType {
    Regular = 1,
    Directory = 2,
    Symlink = 3,
    Hardlink = 4,
    Fifo = 5,
    Socket = 6,
    CharacterDevice = 7,
    BlockDevice = 8,
    Unknown = 255,
}

impl TryFrom<u8> for ForensicFileType {
    type Error = Bcs2Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Regular),
            2 => Ok(Self::Directory),
            3 => Ok(Self::Symlink),
            4 => Ok(Self::Hardlink),
            5 => Ok(Self::Fifo),
            6 => Ok(Self::Socket),
            7 => Ok(Self::CharacterDevice),
            8 => Ok(Self::BlockDevice),
            255 => Ok(Self::Unknown),
            _ => Err(Bcs2Error::CatalogCorrupt),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ForensicTimestamp {
    pub seconds: i64,
    pub nanoseconds: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SparseExtent {
    pub offset: u64,
    pub length: u64,
    pub is_hole: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForensicXattr {
    pub name: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForensicEntry {
    pub path: Vec<u8>,
    pub file_type: ForensicFileType,
    pub mode: u32,
    pub owner: Option<(u32, u32)>,
    /// Access, modification, status-change, and birth timestamps.
    pub timestamps: [Option<ForensicTimestamp>; 4],
    pub acl: Option<Vec<u8>>,
    pub xattrs: Vec<ForensicXattr>,
    pub hardlink_target: Option<Vec<u8>>,
    pub symlink_target: Option<Vec<u8>>,
    pub sparse_extents: Vec<SparseExtent>,
    pub flags: u64,
    pub device: Option<(u32, u32)>,
    pub special_type: Option<Vec<u8>>,
    /// The bytes to store for this entry.
    ///
    /// Verbatim file content unless `content_transform` says otherwise.
    pub content: Option<Vec<u8>>,
    /// Set when `content` holds a transformed encoding instead of the file bytes.
    pub content_transform: Option<ForensicContentTransform>,
}

/// A declaration that a stored frame is an encoding of the file rather than the
/// file itself.
///
/// A forensic capsule's per-entry `content_id` is the chain-of-custody anchor:
/// it answers "is this the file that was seized?". Letting a transform quietly
/// redefine it as the hash of a compressed blob would keep every check passing
/// while destroying the only claim the capsule exists to make. So the logical
/// identity stays primary and travels here, and the stored frame's own identity
/// is recorded beside it as a second, subordinate fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ForensicContentTransform {
    /// What a consumer must be able to do to recover the file bytes. Must be
    /// non-zero — a transform that requires nothing is a verbatim entry, and
    /// admitting both spellings would give one tree two encodings.
    pub capabilities: u64,
    /// blake3 of the original file bytes.
    pub logical_content_id: ContentId,
    /// Length of the original file in bytes.
    pub logical_len: u64,
    /// Fixed-width capability-defined transform parameters. All zero means the
    /// transform needs no out-of-band parameters.
    pub parameters: [u8; 32],
}

impl ForensicContentTransform {
    /// Declare a transform with no capability-specific parameters.
    pub const fn new(capabilities: u64, logical_content_id: ContentId, logical_len: u64) -> Self {
        Self {
            capabilities,
            logical_content_id,
            logical_len,
            parameters: [0; 32],
        }
    }

    /// Attach a fixed capability-specific parameter descriptor.
    pub const fn with_parameters(mut self, parameters: [u8; 32]) -> Self {
        self.parameters = parameters;
        self
    }
}

/// How an entry's stored frame differs from its logical content, as recovered
/// from a parsed capsule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ForensicStoredForm {
    pub capabilities: u64,
    /// blake3 of the frame as stored — what locates it in the artifact index.
    pub stored_content_id: ContentId,
    pub stored_len: u64,
    /// Fixed-width capability-defined transform parameters. All zero means the
    /// transform needs no out-of-band parameters.
    pub parameters: [u8; 32],
}

impl ForensicStoredForm {
    pub const fn new(capabilities: u64, stored_content_id: ContentId, stored_len: u64) -> Self {
        Self {
            capabilities,
            stored_content_id,
            stored_len,
            parameters: [0; 32],
        }
    }
}

/// Line-ending tag used by the registered `lma-synthetic-reemit` descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum LmaSyntheticLineEnding {
    Lf = 0,
    CrLf = 1,
}

/// Parameters for `lma-synthetic-reemit` descriptor version 1.
///
/// This fixed schema belongs to the parameter-bearing capability bit, not to
/// the full capability mask. Decoder capabilities such as zstd or LML may be
/// combined with it without creating competing interpretations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LmaSyntheticReemitParametersV1 {
    pub line_ending: LmaSyntheticLineEnding,
    pub leading_whitespace: u8,
    pub field_width: u8,
    pub trailing_newline: bool,
}

impl LmaSyntheticReemitParametersV1 {
    pub const fn new(
        line_ending: LmaSyntheticLineEnding,
        leading_whitespace: u8,
        field_width: u8,
        trailing_newline: bool,
    ) -> Self {
        Self {
            line_ending,
            leading_whitespace,
            field_width,
            trailing_newline,
        }
    }

    pub const fn encode(self) -> [u8; 32] {
        let mut bytes = [0; 32];
        bytes[0] = 1;
        bytes[1] = 1;
        bytes[2] = self.line_ending as u8;
        bytes[3] = self.leading_whitespace;
        bytes[4] = self.field_width;
        bytes[5] = self.trailing_newline as u8;
        bytes
    }

    pub fn decode(bytes: [u8; 32]) -> Result<Self, Bcs2Error> {
        if bytes[0] != 1
            || bytes[1] != 1
            || bytes[2] > 1
            || bytes[5] > 1
            || bytes[6..].iter().any(|byte| *byte != 0)
        {
            return Err(Bcs2Error::CatalogCorrupt);
        }
        Ok(Self {
            line_ending: if bytes[2] == 0 {
                LmaSyntheticLineEnding::Lf
            } else {
                LmaSyntheticLineEnding::CrLf
            },
            leading_whitespace: bytes[3],
            field_width: bytes[4],
            trailing_newline: bytes[5] != 0,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForensicTree {
    pub platform: String,
    pub entries: Vec<ForensicEntry>,
}

/// Content-addressed forensic tree metadata without resident frame bytes.
///
/// This is the semantic input to the streaming file writer. Each regular entry
/// declares the identity and length of its stored frame; a caller supplies those
/// frames lazily by [`ContentId`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForensicTreeMetadata {
    pub platform: String,
    pub entries: Vec<ForensicEntryMetadata>,
}

impl ForensicTreeMetadata {
    pub fn content_id(&self) -> Result<ContentId, Bcs2Error> {
        validate_metadata(&self.platform, &self.entries)?;
        let metadata = encode_metadata(&self.platform, &self.entries)?;
        Ok(forensic_tree_content_id(raw_content_id(&metadata)))
    }
}

impl TryFrom<&ForensicTree> for ForensicTreeMetadata {
    type Error = Bcs2Error;

    fn try_from(tree: &ForensicTree) -> Result<Self, Self::Error> {
        let metadata = Self {
            platform: tree.platform.clone(),
            entries: metadata_from_tree(tree)?,
        };
        validate_metadata(&metadata.platform, &metadata.entries)?;
        Ok(metadata)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForensicEntryMetadata {
    pub path: Vec<u8>,
    pub file_type: ForensicFileType,
    pub mode: u32,
    pub owner: Option<(u32, u32)>,
    pub timestamps: [Option<ForensicTimestamp>; 4],
    pub acl: Option<Vec<u8>>,
    pub xattrs: Vec<ForensicXattr>,
    pub hardlink_target: Option<Vec<u8>>,
    pub symlink_target: Option<Vec<u8>>,
    pub sparse_extents: Vec<SparseExtent>,
    pub flags: u64,
    pub device: Option<(u32, u32)>,
    pub special_type: Option<Vec<u8>>,
    /// blake3 of the *logical* file content, whatever form it is stored in.
    pub content_id: Option<ContentId>,
    /// Length of the logical file content.
    pub content_len: Option<u64>,
    /// `None` when the frame holds the file bytes verbatim.
    pub stored_form: Option<ForensicStoredForm>,
}

impl ForensicEntryMetadata {
    /// The content id under which this entry's frame is filed in the artifact.
    ///
    /// Equal to `content_id` for verbatim entries; use this, never `content_id`,
    /// to look a frame up.
    pub fn frame_content_id(&self) -> Option<ContentId> {
        match self.stored_form {
            Some(stored) => Some(stored.stored_content_id),
            None => self.content_id,
        }
    }

    /// Length of the frame as stored.
    pub fn frame_len(&self) -> Option<u64> {
        match self.stored_form {
            Some(stored) => Some(stored.stored_len),
            None => self.content_len,
        }
    }

    /// Capabilities a consumer needs to recover this entry's file bytes.
    pub fn required_capabilities(&self) -> u64 {
        self.stored_form.map_or(0, |stored| stored.capabilities)
    }
}

#[derive(Debug)]
pub struct ForensicTreeView<'a> {
    artifact: Bcs2View<'a>,
    platform: String,
    entries: Vec<ForensicEntryMetadata>,
}

impl<'a> ForensicTreeView<'a> {
    pub fn parse(
        bytes: &'a [u8],
        supported_capabilities: u64,
        accepted_bounds: ResourceBounds,
    ) -> Result<Self, Bcs2Error> {
        let artifact = Bcs2View::parse(bytes, supported_capabilities, accepted_bounds)?;
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
            return Err(Bcs2Error::ProfileRootMismatch);
        }
        let (metadata_id, declared_entries) = parse_semantic_json(artifact.semantic_json())?;
        let metadata_frame = artifact
            .frames()
            .iter()
            .find(|frame| frame.content_id() == metadata_id)
            .ok_or(Bcs2Error::RootIdentityMismatch)?;
        if metadata_frame.required_capabilities() != 0 {
            return Err(Bcs2Error::FrameIdentityMismatch);
        }
        if forensic_tree_content_id(metadata_id) != artifact.root_content_id() {
            return Err(Bcs2Error::RootIdentityMismatch);
        }
        let (platform, entries) = decode_metadata(
            metadata_frame.bytes(),
            artifact.bounds().max_index_entries as usize,
        )?;
        if entries.len() != declared_entries {
            return Err(Bcs2Error::CatalogCorrupt);
        }
        validate_metadata(&platform, &entries)?;

        let mut expected_frames = BTreeSet::from([metadata_id]);
        for entry in &entries {
            if let Some(frame_id) = entry.frame_content_id() {
                expected_frames.insert(frame_id);
                let frame = artifact
                    .frames()
                    .iter()
                    .find(|frame| frame.content_id() == frame_id)
                    .ok_or(Bcs2Error::IncompletePortableClosure(frame_id))?;
                if entry.frame_len() != Some(frame.bytes().len() as u64) {
                    return Err(Bcs2Error::FrameIdentityMismatch);
                }
                // The metadata is bound into the root content id; the index entry
                // is not. Requiring the two to agree is what makes an index-only
                // edit of a capability mask detectable rather than merely wrong.
                if frame.required_capabilities() != entry.required_capabilities() {
                    return Err(Bcs2Error::FrameIdentityMismatch);
                }
            }
        }
        let actual_frames: BTreeSet<_> = artifact
            .frames()
            .iter()
            .map(|frame| frame.content_id())
            .collect();
        if actual_frames != expected_frames {
            return Err(Bcs2Error::ExtraPortableFrame(
                *actual_frames
                    .symmetric_difference(&expected_frames)
                    .next()
                    .ok_or(Bcs2Error::CatalogCorrupt)?,
            ));
        }
        Ok(Self {
            artifact,
            platform,
            entries,
        })
    }

    pub const fn content_id(&self) -> ContentId {
        self.artifact.root_content_id()
    }

    pub fn platform(&self) -> &str {
        &self.platform
    }

    pub fn entries(&self) -> &[ForensicEntryMetadata] {
        &self.entries
    }

    /// The entry's file bytes, or `None` if it has none *or* is stored in a
    /// transformed form.
    ///
    /// Deliberately refuses transformed entries rather than returning their
    /// stored bytes: every caller written before stored forms existed treats
    /// this return value as the file, and handing back a compressed blob would
    /// turn a wire extension into silent data corruption at each of them. Use
    /// [`Self::stored_bytes`] plus
    /// [`ForensicEntryMetadata::required_capabilities`] to handle both.
    pub fn content_bytes(&self, entry: &ForensicEntryMetadata) -> Option<&'a [u8]> {
        if entry.stored_form.is_some() {
            return None;
        }
        self.stored_bytes(entry)
    }

    /// The entry's frame exactly as stored, transformed or not.
    pub fn stored_bytes(&self, entry: &ForensicEntryMetadata) -> Option<&'a [u8]> {
        let frame_id = entry.frame_content_id()?;
        self.artifact
            .frames()
            .iter()
            .find(|frame| frame.content_id() == frame_id)
            .map(|frame| frame.bytes())
    }

    pub const fn artifact(&self) -> &Bcs2View<'a> {
        &self.artifact
    }
}

pub fn encode_forensic_tree(
    tree: &ForensicTree,
    bounds: ResourceBounds,
) -> Result<Vec<u8>, Bcs2Error> {
    if tree.entries.len() > bounds.max_index_entries as usize {
        return Err(Bcs2Error::BoundsExceeded);
    }
    let entries = metadata_from_tree(tree)?;
    validate_metadata(&tree.platform, &entries)?;
    let metadata_len = metadata_encoded_len(&tree.platform, &entries)?;
    if metadata_len > bounds.max_frame_bytes as usize
        || metadata_len > bounds.max_catalog_bytes as usize
    {
        return Err(Bcs2Error::BoundsExceeded);
    }
    let metadata = encode_metadata(&tree.platform, &entries)?;
    let metadata_id = raw_content_id(&metadata);
    let root_content_id = forensic_tree_content_id(metadata_id);
    let semantic_json = format!(
        "{{\"forensic_tree\":{{\"content_id\":\"{}\",\"entries\":{},\"version\":1}}}}",
        metadata_id,
        entries.len()
    );
    let raw_frames = core::iter::once((metadata.as_slice(), 0_u64)).chain(
        tree.entries.iter().filter_map(|entry| {
            let content = entry.content.as_deref()?;
            let capabilities = entry
                .content_transform
                .map_or(0, |transform| transform.capabilities);
            Some((content, capabilities))
        }),
    );
    let bytes = encode_raw_root_with_capabilities(
        RootKind::Bundle,
        ProfileId::FORENSIC_TREE_V1,
        root_content_id,
        semantic_json.as_bytes(),
        raw_frames,
        bounds,
    )?;
    let union_capabilities = entries
        .iter()
        .fold(0_u64, |union, entry| union | entry.required_capabilities());
    ForensicTreeView::parse(&bytes, union_capabilities, bounds)?;
    Ok(bytes)
}

fn metadata_from_tree(tree: &ForensicTree) -> Result<Vec<ForensicEntryMetadata>, Bcs2Error> {
    tree.entries
        .iter()
        .map(|entry| {
            let stored_len = entry
                .content
                .as_ref()
                .map(|bytes| u64::try_from(bytes.len()).map_err(|_| Bcs2Error::BoundsExceeded))
                .transpose()?;
            let stored_id = entry.content.as_deref().map(raw_content_id);
            let (content_id, content_len, stored_form) = match entry.content_transform {
                None => (stored_id, stored_len, None),
                Some(transform) => {
                    // A transform declared on an entry that stores nothing has
                    // nothing to describe.
                    let stored_id = stored_id.ok_or(Bcs2Error::SemanticEncoding)?;
                    let stored_len = stored_len.ok_or(Bcs2Error::SemanticEncoding)?;
                    // Zero capabilities is the verbatim spelling; allowing it
                    // here would give one tree two encodings.
                    if transform.capabilities == 0 || transform.logical_content_id == stored_id {
                        return Err(Bcs2Error::SemanticEncoding);
                    }
                    (
                        Some(transform.logical_content_id),
                        Some(transform.logical_len),
                        Some(ForensicStoredForm {
                            capabilities: transform.capabilities,
                            stored_content_id: stored_id,
                            stored_len,
                            parameters: transform.parameters,
                        }),
                    )
                }
            };
            Ok(ForensicEntryMetadata {
                path: entry.path.clone(),
                file_type: entry.file_type,
                mode: entry.mode,
                owner: entry.owner,
                timestamps: entry.timestamps,
                acl: entry.acl.clone(),
                xattrs: entry.xattrs.clone(),
                hardlink_target: entry.hardlink_target.clone(),
                symlink_target: entry.symlink_target.clone(),
                sparse_extents: entry.sparse_extents.clone(),
                flags: entry.flags,
                device: entry.device,
                special_type: entry.special_type.clone(),
                content_id,
                content_len,
                stored_form,
            })
        })
        .collect()
}

pub(crate) fn forensic_tree_content_id(metadata_id: ContentId) -> ContentId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(FORENSIC_TREE_DOMAIN);
    hasher.update(metadata_id.as_bytes());
    ContentId::from_bytes(*hasher.finalize().as_bytes())
}

/// The lowest document version that can express these entries.
///
/// Encoding is canonical, so this must be a pure function of the content:
/// `decode_metadata` re-encodes what it read and rejects any difference.
fn metadata_version(entries: &[ForensicEntryMetadata]) -> u8 {
    if entries.iter().any(|entry| {
        entry
            .stored_form
            .is_some_and(|stored| stored.parameters != [0; 32])
    }) {
        FORENSIC_TREE_VERSION_TRANSFORM_PARAMETERS
    } else if entries.iter().any(|entry| entry.stored_form.is_some()) {
        FORENSIC_TREE_VERSION_STORED_FORM
    } else {
        FORENSIC_TREE_VERSION
    }
}

const fn entry_field_count(version: u8) -> u64 {
    if version >= FORENSIC_TREE_VERSION_STORED_FORM {
        16
    } else {
        15
    }
}

pub(crate) fn encode_metadata(
    platform: &str,
    entries: &[ForensicEntryMetadata],
) -> Result<Vec<u8>, Bcs2Error> {
    let encoded_len = metadata_encoded_len(platform, entries)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(encoded_len)
        .map_err(|_| Bcs2Error::BoundsExceeded)?;
    let bytes = encode_metadata_to(bytes, platform, entries)?;
    debug_assert_eq!(bytes.len(), encoded_len);
    Ok(bytes)
}

pub(crate) fn metadata_encoded_len(
    platform: &str,
    entries: &[ForensicEntryMetadata],
) -> Result<usize, Bcs2Error> {
    Ok(encode_metadata_to(MetadataLength::default(), platform, entries)?.0)
}

#[derive(Default)]
struct MetadataLength(usize);

impl minicbor::encode::Write for MetadataLength {
    type Error = ();

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.0 = self.0.checked_add(bytes.len()).ok_or(())?;
        Ok(())
    }
}

fn encode_metadata_to<W: minicbor::encode::Write>(
    writer: W,
    platform: &str,
    entries: &[ForensicEntryMetadata],
) -> Result<W, Bcs2Error> {
    let version = metadata_version(entries);
    let entry_fields = entry_field_count(version);
    let mut encoder = Encoder::new(writer);
    encoder
        .array(3)
        .and_then(|encoder| encoder.u8(version))
        .and_then(|encoder| encoder.str(platform))
        .and_then(|encoder| encoder.array(entries.len() as u64))
        .map_err(|_| Bcs2Error::SemanticEncoding)?;
    for entry in entries {
        encoder
            .array(entry_fields)
            .and_then(|encoder| encoder.bytes(&entry.path))
            .and_then(|encoder| encoder.u8(entry.file_type as u8))
            .and_then(|encoder| encoder.u32(entry.mode))
            .map_err(|_| Bcs2Error::SemanticEncoding)?;
        encode_pair_u32(&mut encoder, entry.owner)?;
        encoder.array(4).map_err(|_| Bcs2Error::SemanticEncoding)?;
        for timestamp in entry.timestamps {
            encode_timestamp(&mut encoder, timestamp)?;
        }
        encode_optional_bytes(&mut encoder, entry.acl.as_deref())?;
        encoder
            .array(entry.xattrs.len() as u64)
            .map_err(|_| Bcs2Error::SemanticEncoding)?;
        for xattr in &entry.xattrs {
            encoder
                .array(2)
                .and_then(|encoder| encoder.bytes(&xattr.name))
                .and_then(|encoder| encoder.bytes(&xattr.value))
                .map_err(|_| Bcs2Error::SemanticEncoding)?;
        }
        encode_optional_bytes(&mut encoder, entry.hardlink_target.as_deref())?;
        encode_optional_bytes(&mut encoder, entry.symlink_target.as_deref())?;
        encoder
            .array(entry.sparse_extents.len() as u64)
            .map_err(|_| Bcs2Error::SemanticEncoding)?;
        for extent in &entry.sparse_extents {
            encoder
                .array(3)
                .and_then(|encoder| encoder.u64(extent.offset))
                .and_then(|encoder| encoder.u64(extent.length))
                .and_then(|encoder| encoder.bool(extent.is_hole))
                .map_err(|_| Bcs2Error::SemanticEncoding)?;
        }
        encoder
            .u64(entry.flags)
            .map_err(|_| Bcs2Error::SemanticEncoding)?;
        encode_pair_u32(&mut encoder, entry.device)?;
        encode_optional_bytes(&mut encoder, entry.special_type.as_deref())?;
        encode_optional_content_id(&mut encoder, entry.content_id)?;
        match entry.content_len {
            Some(length) => encoder
                .u64(length)
                .map_err(|_| Bcs2Error::SemanticEncoding)?,
            None => encoder.null().map_err(|_| Bcs2Error::SemanticEncoding)?,
        };
        if version >= FORENSIC_TREE_VERSION_STORED_FORM {
            match entry.stored_form {
                Some(stored) => {
                    encoder
                        .array(if version >= FORENSIC_TREE_VERSION_TRANSFORM_PARAMETERS {
                            4
                        } else {
                            3
                        })
                        .and_then(|encoder| encoder.u64(stored.capabilities))
                        .and_then(|encoder| encoder.bytes(stored.stored_content_id.as_bytes()))
                        .and_then(|encoder| encoder.u64(stored.stored_len))
                        .map_err(|_| Bcs2Error::SemanticEncoding)?;
                    if version >= FORENSIC_TREE_VERSION_TRANSFORM_PARAMETERS {
                        encoder
                            .bytes(&stored.parameters)
                            .map_err(|_| Bcs2Error::SemanticEncoding)?;
                    }
                }
                None => {
                    encoder.null().map_err(|_| Bcs2Error::SemanticEncoding)?;
                }
            }
        }
    }
    Ok(encoder.into_writer())
}

pub(crate) fn decode_metadata(
    bytes: &[u8],
    max_items: usize,
) -> Result<(String, Vec<ForensicEntryMetadata>), Bcs2Error> {
    let mut decoder = Decoder::new(bytes);
    require_array(&mut decoder, 3)?;
    let version = decoder.u8().map_err(|_| Bcs2Error::CatalogCorrupt)?;
    if !matches!(
        version,
        FORENSIC_TREE_VERSION
            | FORENSIC_TREE_VERSION_STORED_FORM
            | FORENSIC_TREE_VERSION_TRANSFORM_PARAMETERS
    ) {
        return Err(Bcs2Error::CatalogCorrupt);
    }
    let entry_fields = entry_field_count(version);
    let platform = String::from(decoder.str().map_err(|_| Bcs2Error::CatalogCorrupt)?);
    let entry_count = definite_array(&mut decoder)?;
    let entry_count = usize::try_from(entry_count).map_err(|_| Bcs2Error::BoundsExceeded)?;
    if entry_count > max_items {
        return Err(Bcs2Error::BoundsExceeded);
    }
    let mut entries = Vec::with_capacity(entry_count);
    for _ in 0..entry_count {
        require_array(&mut decoder, entry_fields)?;
        let path = decoder
            .bytes()
            .map_err(|_| Bcs2Error::CatalogCorrupt)?
            .to_vec();
        let file_type =
            ForensicFileType::try_from(decoder.u8().map_err(|_| Bcs2Error::CatalogCorrupt)?)?;
        let mode = decoder.u32().map_err(|_| Bcs2Error::CatalogCorrupt)?;
        let owner = decode_pair_u32(&mut decoder)?;
        require_array(&mut decoder, 4)?;
        let mut timestamps = [None; 4];
        for timestamp in &mut timestamps {
            *timestamp = decode_timestamp(&mut decoder)?;
        }
        let acl = decode_optional_bytes(&mut decoder)?;
        let xattr_count = definite_array(&mut decoder)?;
        let xattr_count = usize::try_from(xattr_count).map_err(|_| Bcs2Error::BoundsExceeded)?;
        if xattr_count > max_items {
            return Err(Bcs2Error::BoundsExceeded);
        }
        let mut xattrs = Vec::with_capacity(xattr_count);
        for _ in 0..xattr_count {
            require_array(&mut decoder, 2)?;
            xattrs.push(ForensicXattr {
                name: decoder
                    .bytes()
                    .map_err(|_| Bcs2Error::CatalogCorrupt)?
                    .to_vec(),
                value: decoder
                    .bytes()
                    .map_err(|_| Bcs2Error::CatalogCorrupt)?
                    .to_vec(),
            });
        }
        let hardlink_target = decode_optional_bytes(&mut decoder)?;
        let symlink_target = decode_optional_bytes(&mut decoder)?;
        let extent_count = definite_array(&mut decoder)?;
        let extent_count = usize::try_from(extent_count).map_err(|_| Bcs2Error::BoundsExceeded)?;
        if extent_count > max_items {
            return Err(Bcs2Error::BoundsExceeded);
        }
        let mut sparse_extents = Vec::with_capacity(extent_count);
        for _ in 0..extent_count {
            require_array(&mut decoder, 3)?;
            sparse_extents.push(SparseExtent {
                offset: decoder.u64().map_err(|_| Bcs2Error::CatalogCorrupt)?,
                length: decoder.u64().map_err(|_| Bcs2Error::CatalogCorrupt)?,
                is_hole: decoder.bool().map_err(|_| Bcs2Error::CatalogCorrupt)?,
            });
        }
        let flags = decoder.u64().map_err(|_| Bcs2Error::CatalogCorrupt)?;
        let device = decode_pair_u32(&mut decoder)?;
        let special_type = decode_optional_bytes(&mut decoder)?;
        let content_id = decode_optional_content_id(&mut decoder)?;
        let content_len =
            if decoder.datatype().map_err(|_| Bcs2Error::CatalogCorrupt)? == Type::Null {
                decoder.null().map_err(|_| Bcs2Error::CatalogCorrupt)?;
                None
            } else {
                Some(decoder.u64().map_err(|_| Bcs2Error::CatalogCorrupt)?)
            };
        let stored_form = if version >= FORENSIC_TREE_VERSION_STORED_FORM {
            decode_stored_form(&mut decoder, version)?
        } else {
            None
        };
        entries.push(ForensicEntryMetadata {
            path,
            file_type,
            mode,
            owner,
            timestamps,
            acl,
            xattrs,
            hardlink_target,
            symlink_target,
            sparse_extents,
            flags,
            device,
            special_type,
            content_id,
            content_len,
            stored_form,
        });
    }
    // A v2 document in which nothing declares a stored form would re-encode as
    // v1, so this catches it as the non-canonical encoding it is.
    if decoder.position() != bytes.len() || encode_metadata(&platform, &entries)? != bytes {
        return Err(Bcs2Error::CatalogCorrupt);
    }
    Ok((platform, entries))
}

pub(crate) fn validate_metadata(
    platform: &str,
    entries: &[ForensicEntryMetadata],
) -> Result<(), Bcs2Error> {
    if platform.is_empty()
        || platform.len() > 64
        || !platform
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(Bcs2Error::SemanticEncoding);
    }
    let mut prior_path: Option<&[u8]> = None;
    for entry in entries {
        validate_path(&entry.path)?;
        if prior_path.is_some_and(|prior| prior >= entry.path.as_slice()) {
            return Err(Bcs2Error::SemanticEncoding);
        }
        prior_path = Some(&entry.path);
    }
    for entry in entries {
        if let Some(parent) = parent_path(&entry.path) {
            let Some(parent_entry) = find_entry_by_path(entries, parent) else {
                return Err(Bcs2Error::SemanticEncoding);
            };
            if parent_entry.file_type != ForensicFileType::Directory {
                return Err(Bcs2Error::SemanticEncoding);
            }
        }
        for timestamp in entry.timestamps.into_iter().flatten() {
            if timestamp.nanoseconds >= 1_000_000_000 {
                return Err(Bcs2Error::SemanticEncoding);
            }
        }
        let mut prior_xattr: Option<&[u8]> = None;
        for xattr in &entry.xattrs {
            if xattr.name.is_empty()
                || xattr.name.contains(&0)
                || prior_xattr.is_some_and(|prior| prior >= xattr.name.as_slice())
            {
                return Err(Bcs2Error::SemanticEncoding);
            }
            prior_xattr = Some(&xattr.name);
        }
        if entry
            .symlink_target
            .as_deref()
            .is_some_and(|target| target.is_empty() || target.contains(&0))
            || entry
                .special_type
                .as_deref()
                .is_some_and(|name| name.is_empty() || name.contains(&0))
        {
            return Err(Bcs2Error::SemanticEncoding);
        }
        // Only a regular file has content, so only a regular file can have a
        // transformed encoding of it. Checked once here rather than in each arm
        // so a file type added later cannot acquire one by omission.
        if entry.stored_form.is_some() && entry.file_type != ForensicFileType::Regular {
            return Err(Bcs2Error::SemanticEncoding);
        }
        if let Some(stored) = entry.stored_form {
            if stored.capabilities == 0 || Some(stored.stored_content_id) == entry.content_id {
                return Err(Bcs2Error::SemanticEncoding);
            }
            if stored.parameters != [0; 32]
                && (stored.capabilities & CAP_LMA_SYNTHETIC_REEMIT == 0
                    || LmaSyntheticReemitParametersV1::decode(stored.parameters).is_err())
            {
                return Err(Bcs2Error::SemanticEncoding);
            }
        }
        match entry.file_type {
            ForensicFileType::Regular => {
                if entry.content_id.is_none()
                    || entry.content_len.is_none()
                    || entry.hardlink_target.is_some()
                    || entry.symlink_target.is_some()
                    || entry.device.is_some()
                    || entry.special_type.is_some()
                {
                    return Err(Bcs2Error::SemanticEncoding);
                }
                validate_sparse_extents(&entry.sparse_extents, entry.content_len.unwrap_or(0))?;
            }
            ForensicFileType::Directory => {
                require_no_payload_fields(entry)?;
            }
            ForensicFileType::Symlink => {
                if entry.symlink_target.is_none()
                    || entry.hardlink_target.is_some()
                    || entry.content_id.is_some()
                    || entry.content_len.is_some()
                    || entry.device.is_some()
                    || entry.special_type.is_some()
                    || !entry.sparse_extents.is_empty()
                {
                    return Err(Bcs2Error::SemanticEncoding);
                }
            }
            ForensicFileType::Hardlink => {
                let target = entry
                    .hardlink_target
                    .as_deref()
                    .ok_or(Bcs2Error::SemanticEncoding)?;
                validate_path(target)?;
                let target_entry =
                    find_entry_by_path(entries, target).ok_or(Bcs2Error::SemanticEncoding)?;
                if target_entry.file_type != ForensicFileType::Regular
                    || entry.mode != target_entry.mode
                    || entry.owner != target_entry.owner
                    || entry.timestamps != target_entry.timestamps
                    || entry.acl != target_entry.acl
                    || entry.xattrs != target_entry.xattrs
                    || entry.flags != target_entry.flags
                    || entry.symlink_target.is_some()
                    || entry.content_id.is_some()
                    || entry.content_len.is_some()
                    || entry.device.is_some()
                    || entry.special_type.is_some()
                    || !entry.sparse_extents.is_empty()
                {
                    return Err(Bcs2Error::SemanticEncoding);
                }
            }
            ForensicFileType::CharacterDevice | ForensicFileType::BlockDevice => {
                if entry.device.is_none()
                    || entry.content_id.is_some()
                    || entry.content_len.is_some()
                    || entry.hardlink_target.is_some()
                    || entry.symlink_target.is_some()
                    || entry.special_type.is_some()
                    || !entry.sparse_extents.is_empty()
                {
                    return Err(Bcs2Error::SemanticEncoding);
                }
            }
            ForensicFileType::Fifo | ForensicFileType::Socket => require_no_payload_fields(entry)?,
            ForensicFileType::Unknown => {
                if entry.special_type.is_none()
                    || entry.content_id.is_some()
                    || entry.content_len.is_some()
                    || entry.hardlink_target.is_some()
                    || entry.symlink_target.is_some()
                    || entry.device.is_some()
                    || !entry.sparse_extents.is_empty()
                {
                    return Err(Bcs2Error::SemanticEncoding);
                }
            }
        }
    }
    Ok(())
}

fn find_entry_by_path<'a>(
    entries: &'a [ForensicEntryMetadata],
    path: &[u8],
) -> Option<&'a ForensicEntryMetadata> {
    entries
        .binary_search_by(|entry| entry.path.as_slice().cmp(path))
        .ok()
        .map(|index| &entries[index])
}

fn require_no_payload_fields(entry: &ForensicEntryMetadata) -> Result<(), Bcs2Error> {
    if entry.content_id.is_some()
        || entry.content_len.is_some()
        || entry.hardlink_target.is_some()
        || entry.symlink_target.is_some()
        || entry.device.is_some()
        || entry.special_type.is_some()
        || !entry.sparse_extents.is_empty()
    {
        return Err(Bcs2Error::SemanticEncoding);
    }
    Ok(())
}

fn validate_sparse_extents(extents: &[SparseExtent], content_len: u64) -> Result<(), Bcs2Error> {
    if extents.is_empty() {
        return Ok(());
    }
    let mut next = 0_u64;
    for extent in extents {
        if extent.offset != next || extent.length == 0 {
            return Err(Bcs2Error::SemanticEncoding);
        }
        next = next
            .checked_add(extent.length)
            .ok_or(Bcs2Error::SemanticEncoding)?;
    }
    if next != content_len {
        return Err(Bcs2Error::SemanticEncoding);
    }
    Ok(())
}

fn validate_path(path: &[u8]) -> Result<(), Bcs2Error> {
    // Portable forensic paths always use `/`. Backslashes and colons are
    // rejected even on Unix so a capsule cannot become traversal, drive-prefix,
    // UNC, or alternate-data-stream syntax when restored on Windows.
    if path.is_empty()
        || path[0] == b'/'
        || path.contains(&0)
        || path.contains(&b'\\')
        || path.contains(&b':')
        || path
            .iter()
            .any(|byte| *byte < b' ' || matches!(*byte, b'<' | b'>' | b'"' | b'|' | b'?' | b'*'))
    {
        return Err(Bcs2Error::SemanticEncoding);
    }
    for component in path.split(|byte| *byte == b'/') {
        if component.is_empty()
            || component == b"."
            || component == b".."
            || component.ends_with(b".")
            || component.ends_with(b" ")
            || is_windows_device_name(component)
        {
            return Err(Bcs2Error::SemanticEncoding);
        }
    }
    Ok(())
}

fn is_windows_device_name(component: &[u8]) -> bool {
    // Win32 device aliases remain reserved with an extension. Normalize spaces
    // before that extension too, so `AUX .txt` cannot evade portable checks.
    let stem = component
        .split(|byte| *byte == b'.')
        .next()
        .unwrap_or(component);
    let stem = &stem[..stem
        .iter()
        .rposition(|byte| *byte != b' ')
        .map_or(0, |index| index + 1)];
    let equals = |name: &[u8]| stem.eq_ignore_ascii_case(name);
    equals(b"CON")
        || equals(b"PRN")
        || equals(b"AUX")
        || equals(b"NUL")
        || equals(b"CLOCK$")
        || (stem.len() == 4
            && (stem[..3].eq_ignore_ascii_case(b"COM") || stem[..3].eq_ignore_ascii_case(b"LPT"))
            && matches!(stem[3], b'1'..=b'9'))
}

#[cfg(feature = "std")]
pub(crate) fn validate_restore_path(path: &[u8]) -> Result<(), Bcs2Error> {
    validate_path(path)
}

fn parent_path(path: &[u8]) -> Option<&[u8]> {
    path.iter()
        .rposition(|byte| *byte == b'/')
        .map(|separator| &path[..separator])
}

pub(crate) fn parse_semantic_json(bytes: &[u8]) -> Result<(ContentId, usize), Bcs2Error> {
    const PREFIX: &str = "{\"forensic_tree\":{\"content_id\":\"";
    const ENTRIES: &str = "\",\"entries\":";
    const SUFFIX: &str = ",\"version\":1}}";
    let text = core::str::from_utf8(bytes).map_err(|_| Bcs2Error::CatalogCorrupt)?;
    let tail = text.strip_prefix(PREFIX).ok_or(Bcs2Error::CatalogCorrupt)?;
    let (content_id, tail) = tail.split_once(ENTRIES).ok_or(Bcs2Error::CatalogCorrupt)?;
    let entries = tail.strip_suffix(SUFFIX).ok_or(Bcs2Error::CatalogCorrupt)?;
    let entries = entries
        .parse::<usize>()
        .map_err(|_| Bcs2Error::CatalogCorrupt)?;
    Ok((parse_content_id(content_id)?, entries))
}

fn parse_content_id(value: &str) -> Result<ContentId, Bcs2Error> {
    if value.len() != 64 {
        return Err(Bcs2Error::CatalogCorrupt);
    }
    let mut bytes = [0; 32];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = core::str::from_utf8(chunk).map_err(|_| Bcs2Error::CatalogCorrupt)?;
        bytes[index] = u8::from_str_radix(text, 16).map_err(|_| Bcs2Error::CatalogCorrupt)?;
    }
    Ok(ContentId::from_bytes(bytes))
}

fn definite_array(decoder: &mut Decoder<'_>) -> Result<u64, Bcs2Error> {
    decoder
        .array()
        .map_err(|_| Bcs2Error::CatalogCorrupt)?
        .ok_or(Bcs2Error::CatalogCorrupt)
}

fn require_array(decoder: &mut Decoder<'_>, expected: u64) -> Result<(), Bcs2Error> {
    if definite_array(decoder)? != expected {
        return Err(Bcs2Error::CatalogCorrupt);
    }
    Ok(())
}

fn encode_pair_u32<W: minicbor::encode::Write>(
    encoder: &mut Encoder<W>,
    value: Option<(u32, u32)>,
) -> Result<(), Bcs2Error> {
    match value {
        Some((first, second)) => {
            encoder
                .array(2)
                .and_then(|encoder| encoder.u32(first))
                .and_then(|encoder| encoder.u32(second))
                .map_err(|_| Bcs2Error::SemanticEncoding)?;
        }
        None => {
            encoder.null().map_err(|_| Bcs2Error::SemanticEncoding)?;
        }
    }
    Ok(())
}

fn decode_pair_u32(decoder: &mut Decoder<'_>) -> Result<Option<(u32, u32)>, Bcs2Error> {
    if decoder.datatype().map_err(|_| Bcs2Error::CatalogCorrupt)? == Type::Null {
        decoder.null().map_err(|_| Bcs2Error::CatalogCorrupt)?;
        return Ok(None);
    }
    require_array(decoder, 2)?;
    Ok(Some((
        decoder.u32().map_err(|_| Bcs2Error::CatalogCorrupt)?,
        decoder.u32().map_err(|_| Bcs2Error::CatalogCorrupt)?,
    )))
}

fn encode_timestamp<W: minicbor::encode::Write>(
    encoder: &mut Encoder<W>,
    value: Option<ForensicTimestamp>,
) -> Result<(), Bcs2Error> {
    match value {
        Some(value) => {
            encoder
                .array(2)
                .and_then(|encoder| encoder.i64(value.seconds))
                .and_then(|encoder| encoder.u32(value.nanoseconds))
                .map_err(|_| Bcs2Error::SemanticEncoding)?;
        }
        None => {
            encoder.null().map_err(|_| Bcs2Error::SemanticEncoding)?;
        }
    }
    Ok(())
}

fn decode_timestamp(decoder: &mut Decoder<'_>) -> Result<Option<ForensicTimestamp>, Bcs2Error> {
    if decoder.datatype().map_err(|_| Bcs2Error::CatalogCorrupt)? == Type::Null {
        decoder.null().map_err(|_| Bcs2Error::CatalogCorrupt)?;
        return Ok(None);
    }
    require_array(decoder, 2)?;
    Ok(Some(ForensicTimestamp {
        seconds: decoder.i64().map_err(|_| Bcs2Error::CatalogCorrupt)?,
        nanoseconds: decoder.u32().map_err(|_| Bcs2Error::CatalogCorrupt)?,
    }))
}

fn encode_optional_bytes<W: minicbor::encode::Write>(
    encoder: &mut Encoder<W>,
    value: Option<&[u8]>,
) -> Result<(), Bcs2Error> {
    match value {
        Some(value) => {
            encoder
                .bytes(value)
                .map_err(|_| Bcs2Error::SemanticEncoding)?;
        }
        None => {
            encoder.null().map_err(|_| Bcs2Error::SemanticEncoding)?;
        }
    }
    Ok(())
}

fn decode_optional_bytes(decoder: &mut Decoder<'_>) -> Result<Option<Vec<u8>>, Bcs2Error> {
    if decoder.datatype().map_err(|_| Bcs2Error::CatalogCorrupt)? == Type::Null {
        decoder.null().map_err(|_| Bcs2Error::CatalogCorrupt)?;
        return Ok(None);
    }
    Ok(Some(
        decoder
            .bytes()
            .map_err(|_| Bcs2Error::CatalogCorrupt)?
            .to_vec(),
    ))
}

fn encode_optional_content_id<W: minicbor::encode::Write>(
    encoder: &mut Encoder<W>,
    value: Option<ContentId>,
) -> Result<(), Bcs2Error> {
    match value {
        Some(value) => {
            encoder
                .bytes(value.as_bytes())
                .map_err(|_| Bcs2Error::SemanticEncoding)?;
        }
        None => {
            encoder.null().map_err(|_| Bcs2Error::SemanticEncoding)?;
        }
    }
    Ok(())
}

fn decode_stored_form(
    decoder: &mut Decoder<'_>,
    version: u8,
) -> Result<Option<ForensicStoredForm>, Bcs2Error> {
    if decoder.datatype().map_err(|_| Bcs2Error::CatalogCorrupt)? == Type::Null {
        decoder.null().map_err(|_| Bcs2Error::CatalogCorrupt)?;
        return Ok(None);
    }
    require_array(
        decoder,
        if version >= FORENSIC_TREE_VERSION_TRANSFORM_PARAMETERS {
            4
        } else {
            3
        },
    )?;
    let capabilities = decoder.u64().map_err(|_| Bcs2Error::CatalogCorrupt)?;
    let stored_content_id =
        decode_optional_content_id(decoder)?.ok_or(Bcs2Error::CatalogCorrupt)?;
    let stored_len = decoder.u64().map_err(|_| Bcs2Error::CatalogCorrupt)?;
    let parameters = if version >= FORENSIC_TREE_VERSION_TRANSFORM_PARAMETERS {
        decoder
            .bytes()
            .map_err(|_| Bcs2Error::CatalogCorrupt)?
            .try_into()
            .map_err(|_| Bcs2Error::CatalogCorrupt)?
    } else {
        [0; 32]
    };
    Ok(Some(ForensicStoredForm {
        capabilities,
        stored_content_id,
        stored_len,
        parameters,
    }))
}

fn decode_optional_content_id(decoder: &mut Decoder<'_>) -> Result<Option<ContentId>, Bcs2Error> {
    if decoder.datatype().map_err(|_| Bcs2Error::CatalogCorrupt)? == Type::Null {
        decoder.null().map_err(|_| Bcs2Error::CatalogCorrupt)?;
        return Ok(None);
    }
    let bytes: [u8; 32] = decoder
        .bytes()
        .map_err(|_| Bcs2Error::CatalogCorrupt)?
        .try_into()
        .map_err(|_| Bcs2Error::CatalogCorrupt)?;
    Ok(Some(ContentId::from_bytes(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded_stored_form(parameters: Option<&[u8]>) -> Vec<u8> {
        let mut encoder = Encoder::new(Vec::new());
        encoder
            .array(if parameters.is_some() { 4 } else { 3 })
            .unwrap()
            .u64(1)
            .unwrap()
            .bytes(&[7; 32])
            .unwrap()
            .u64(9)
            .unwrap();
        if let Some(parameters) = parameters {
            encoder.bytes(parameters).unwrap();
        }
        encoder.into_writer()
    }

    #[test]
    fn version_two_stored_form_decodes_with_zero_parameters() {
        let bytes = encoded_stored_form(None);
        let stored =
            decode_stored_form(&mut Decoder::new(&bytes), FORENSIC_TREE_VERSION_STORED_FORM)
                .unwrap()
                .unwrap();
        assert_eq!(stored.parameters, [0; 32]);
    }

    #[test]
    fn version_three_rejects_wrong_parameter_length() {
        for parameters in [&[3; 31][..], &[3; 33][..]] {
            let bytes = encoded_stored_form(Some(parameters));
            assert_eq!(
                decode_stored_form(
                    &mut Decoder::new(&bytes),
                    FORENSIC_TREE_VERSION_TRANSFORM_PARAMETERS,
                ),
                Err(Bcs2Error::CatalogCorrupt)
            );
        }
    }
}
