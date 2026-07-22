use crate::wire::{encode_raw_root, raw_content_id};
use crate::{
    Bcs2Error, Bcs2View, FrameKind, PrivacyMode, ProfileId, ResourceBounds, RootKind,
    StorageContract,
};
use abir::ContentId;
use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minicbor::data::Type;
use minicbor::{Decoder, Encoder};

const FORENSIC_TREE_VERSION: u8 = 1;
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
    pub content: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForensicTree {
    pub platform: String,
    pub entries: Vec<ForensicEntry>,
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
    pub content_id: Option<ContentId>,
    pub content_len: Option<u64>,
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
            if let Some(content_id) = entry.content_id {
                expected_frames.insert(content_id);
                let frame = artifact
                    .frames()
                    .iter()
                    .find(|frame| frame.content_id() == content_id)
                    .ok_or(Bcs2Error::IncompletePortableClosure(content_id))?;
                if entry.content_len != Some(frame.bytes().len() as u64) {
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

    pub fn content_bytes(&self, entry: &ForensicEntryMetadata) -> Option<&'a [u8]> {
        let content_id = entry.content_id?;
        self.artifact
            .frames()
            .iter()
            .find(|frame| frame.content_id() == content_id)
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
    let entries = metadata_from_tree(tree)?;
    validate_metadata(&tree.platform, &entries)?;
    let metadata = encode_metadata(&tree.platform, &entries)?;
    let metadata_id = raw_content_id(&metadata);
    let root_content_id = forensic_tree_content_id(metadata_id);
    let semantic_json = format!(
        "{{\"forensic_tree\":{{\"content_id\":\"{}\",\"entries\":{},\"version\":1}}}}",
        metadata_id,
        entries.len()
    );
    let raw_frames = core::iter::once(metadata.as_slice()).chain(
        tree.entries
            .iter()
            .filter_map(|entry| entry.content.as_deref()),
    );
    let bytes = encode_raw_root(
        RootKind::Bundle,
        ProfileId::FORENSIC_TREE_V1,
        root_content_id,
        semantic_json.as_bytes(),
        raw_frames,
        bounds,
    )?;
    ForensicTreeView::parse(&bytes, 0, bounds)?;
    Ok(bytes)
}

fn metadata_from_tree(tree: &ForensicTree) -> Result<Vec<ForensicEntryMetadata>, Bcs2Error> {
    tree.entries
        .iter()
        .map(|entry| {
            let content_len = entry
                .content
                .as_ref()
                .map(|bytes| u64::try_from(bytes.len()).map_err(|_| Bcs2Error::BoundsExceeded))
                .transpose()?;
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
                content_id: entry.content.as_deref().map(raw_content_id),
                content_len,
            })
        })
        .collect()
}

fn forensic_tree_content_id(metadata_id: ContentId) -> ContentId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(FORENSIC_TREE_DOMAIN);
    hasher.update(metadata_id.as_bytes());
    ContentId::from_bytes(*hasher.finalize().as_bytes())
}

fn encode_metadata(
    platform: &str,
    entries: &[ForensicEntryMetadata],
) -> Result<Vec<u8>, Bcs2Error> {
    let mut encoder = Encoder::new(Vec::new());
    encoder
        .array(3)
        .and_then(|encoder| encoder.u8(FORENSIC_TREE_VERSION))
        .and_then(|encoder| encoder.str(platform))
        .and_then(|encoder| encoder.array(entries.len() as u64))
        .map_err(|_| Bcs2Error::SemanticEncoding)?;
    for entry in entries {
        encoder
            .array(15)
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
    }
    Ok(encoder.into_writer())
}

fn decode_metadata(
    bytes: &[u8],
    max_items: usize,
) -> Result<(String, Vec<ForensicEntryMetadata>), Bcs2Error> {
    let mut decoder = Decoder::new(bytes);
    require_array(&mut decoder, 3)?;
    if decoder.u8().map_err(|_| Bcs2Error::CatalogCorrupt)? != FORENSIC_TREE_VERSION {
        return Err(Bcs2Error::CatalogCorrupt);
    }
    let platform = String::from(decoder.str().map_err(|_| Bcs2Error::CatalogCorrupt)?);
    let entry_count = definite_array(&mut decoder)?;
    let entry_count = usize::try_from(entry_count).map_err(|_| Bcs2Error::BoundsExceeded)?;
    if entry_count > max_items {
        return Err(Bcs2Error::BoundsExceeded);
    }
    let mut entries = Vec::with_capacity(entry_count);
    for _ in 0..entry_count {
        require_array(&mut decoder, 15)?;
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
        });
    }
    if decoder.position() != bytes.len() || encode_metadata(&platform, &entries)? != bytes {
        return Err(Bcs2Error::CatalogCorrupt);
    }
    Ok((platform, entries))
}

fn validate_metadata(platform: &str, entries: &[ForensicEntryMetadata]) -> Result<(), Bcs2Error> {
    if platform.is_empty()
        || platform.len() > 64
        || !platform
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(Bcs2Error::SemanticEncoding);
    }
    let mut prior_path: Option<&[u8]> = None;
    let regular_paths: BTreeSet<&[u8]> = entries
        .iter()
        .filter(|entry| entry.file_type == ForensicFileType::Regular)
        .map(|entry| entry.path.as_slice())
        .collect();
    let directory_paths: BTreeSet<&[u8]> = entries
        .iter()
        .filter(|entry| entry.file_type == ForensicFileType::Directory)
        .map(|entry| entry.path.as_slice())
        .collect();
    for entry in entries {
        validate_path(&entry.path)?;
        if prior_path.is_some_and(|prior| prior >= entry.path.as_slice()) {
            return Err(Bcs2Error::SemanticEncoding);
        }
        if let Some(parent) = parent_path(&entry.path) {
            if !directory_paths.contains(parent) {
                return Err(Bcs2Error::SemanticEncoding);
            }
        }
        prior_path = Some(&entry.path);
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
                let target_entry = entries
                    .iter()
                    .find(|candidate| candidate.path.as_slice() == target)
                    .ok_or(Bcs2Error::SemanticEncoding)?;
                if !regular_paths.contains(target)
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
    if path.is_empty() || path[0] == b'/' || path.contains(&0) {
        return Err(Bcs2Error::SemanticEncoding);
    }
    for component in path.split(|byte| *byte == b'/') {
        if component.is_empty() || component == b"." || component == b".." {
            return Err(Bcs2Error::SemanticEncoding);
        }
    }
    Ok(())
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

fn parse_semantic_json(bytes: &[u8]) -> Result<(ContentId, usize), Bcs2Error> {
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

fn encode_pair_u32(
    encoder: &mut Encoder<Vec<u8>>,
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

fn encode_timestamp(
    encoder: &mut Encoder<Vec<u8>>,
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

fn encode_optional_bytes(
    encoder: &mut Encoder<Vec<u8>>,
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

fn encode_optional_content_id(
    encoder: &mut Encoder<Vec<u8>>,
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
