use crate::wire::{
    put_u16, put_u32, put_u64, raw_content_id, raw_storage_id, INDEX_ENTRY_LEN, INDEX_LEN,
    INDEX_MAGIC,
};
use crate::{
    Bcs2Error, Bcs2View, FrameKind, PrivacyMode, ProfileId, ResourceBounds, RootKind,
    StorageContract, BCS2_HEADER_LEN, BCS2_MAGIC,
};
use abir::ContentId;
use alloc::format;
use alloc::{vec, vec::Vec};
use minicbor::Encoder;

const BLOB_ROOT_DOMAIN: &[u8] = b"org.quitetall.abir.bcs2.blob-root-v1\0";

#[derive(Debug)]
pub struct BlobView<'a> {
    artifact: Bcs2View<'a>,
    media_type: &'a str,
    bytes: &'a [u8],
}

impl<'a> BlobView<'a> {
    pub fn parse(
        bytes: &'a [u8],
        supported_capabilities: u64,
        accepted_bounds: ResourceBounds,
    ) -> Result<Self, Bcs2Error> {
        let artifact = Bcs2View::parse(bytes, supported_capabilities, accepted_bounds)?;
        if artifact.root_kind() != RootKind::Blob
            || artifact.profile() != ProfileId::FORENSIC_IMAGE_V1
            || artifact.privacy_mode() != PrivacyMode::Plaintext
            || artifact.storage_contract() != StorageContract::SealedImmutable
            || !artifact.references().is_empty()
            || artifact.frames().len() != 1
            || artifact.frames()[0].kind() != FrameKind::RawBlob
        {
            return Err(Bcs2Error::ProfileRootMismatch);
        }
        let frame = artifact.frames()[0];
        let (media_type, declared_content, declared_len) =
            parse_semantic_json(artifact.semantic_json())?;
        if declared_content != frame.content_id()
            || declared_len != frame.bytes().len()
            || blob_root_content_id(media_type, declared_content, declared_len)
                != artifact.root_content_id()
        {
            return Err(Bcs2Error::RootIdentityMismatch);
        }
        Ok(Self {
            artifact,
            media_type,
            bytes: frame.bytes(),
        })
    }

    pub const fn content_id(&self) -> ContentId {
        self.artifact.root_content_id()
    }
    pub const fn media_type(&self) -> &'a str {
        self.media_type
    }
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
    pub const fn artifact(&self) -> &Bcs2View<'a> {
        &self.artifact
    }
}

pub fn encode_blob(
    payload: &[u8],
    media_type: &str,
    bounds: ResourceBounds,
) -> Result<Vec<u8>, Bcs2Error> {
    validate_media_type(media_type)?;
    if payload.len() > bounds.max_frame_bytes as usize
        || bounds.max_catalog_bytes == 0
        || bounds.max_index_entries == 0
        || bounds.max_generations == 0
    {
        return Err(Bcs2Error::BoundsExceeded);
    }
    let payload_content_id = raw_content_id(payload);
    let root_content_id = blob_root_content_id(media_type, payload_content_id, payload.len());
    let semantic_json = format!(
        "{{\"blob\":{{\"content_id\":\"{}\",\"length\":{},\"media_type\":\"{}\"}}}}",
        payload_content_id,
        payload.len(),
        media_type
    );
    let mut encoder = Encoder::new(Vec::new());
    encoder
        .map(3)
        .and_then(|encoder| encoder.u8(1))
        .and_then(|encoder| encoder.bytes(semantic_json.as_bytes()))
        .and_then(|encoder| encoder.u8(2))
        .and_then(|encoder| encoder.bytes(root_content_id.as_bytes()))
        .and_then(|encoder| encoder.u8(3))
        .and_then(|encoder| encoder.array(0))
        .map_err(|_| Bcs2Error::SemanticEncoding)?;
    let catalog = encoder.into_writer();
    if catalog.len() > bounds.max_catalog_bytes as usize {
        return Err(Bcs2Error::BoundsExceeded);
    }
    let frame_offset = BCS2_HEADER_LEN
        .checked_add(catalog.len())
        .ok_or(Bcs2Error::BoundsExceeded)?;
    let index_offset = frame_offset
        .checked_add(payload.len())
        .ok_or(Bcs2Error::BoundsExceeded)?;
    let index_len = INDEX_LEN + INDEX_ENTRY_LEN;
    let total = index_offset
        .checked_add(index_len)
        .ok_or(Bcs2Error::BoundsExceeded)?;
    let mut bytes = vec![0_u8; total];
    bytes[..8].copy_from_slice(&BCS2_MAGIC);
    put_u16(&mut bytes, 8, 2);
    put_u16(&mut bytes, 10, 0);
    put_u32(&mut bytes, 12, BCS2_HEADER_LEN as u32);
    put_u32(&mut bytes, 16, ProfileId::FORENSIC_IMAGE_V1.get());
    put_u32(&mut bytes, 20, 1);
    bytes[40] = RootKind::Blob as u8;
    bytes[41] = StorageContract::SealedImmutable as u8;
    bytes[42] = PrivacyMode::Plaintext as u8;
    bytes[43] = 1;
    put_u32(&mut bytes, 44, bounds.max_catalog_bytes);
    put_u32(&mut bytes, 48, bounds.max_index_entries);
    put_u32(&mut bytes, 52, bounds.max_frame_bytes);
    put_u64(&mut bytes, 56, BCS2_HEADER_LEN as u64);
    put_u64(&mut bytes, 64, catalog.len() as u64);
    put_u64(&mut bytes, 72, index_offset as u64);
    put_u64(&mut bytes, 80, index_len as u64);
    bytes[96..128].copy_from_slice(root_content_id.as_bytes());
    bytes[BCS2_HEADER_LEN..frame_offset].copy_from_slice(&catalog);
    bytes[frame_offset..index_offset].copy_from_slice(payload);
    bytes[index_offset..index_offset + 8].copy_from_slice(&INDEX_MAGIC);
    put_u32(&mut bytes, index_offset + 8, 1);
    bytes[index_offset + 16..index_offset + 48].copy_from_slice(blake3::hash(&catalog).as_bytes());
    let entry = index_offset + INDEX_LEN;
    bytes[entry..entry + 32].copy_from_slice(payload_content_id.as_bytes());
    bytes[entry + 32..entry + 64].copy_from_slice(raw_storage_id(payload).as_bytes());
    put_u64(&mut bytes, entry + 64, frame_offset as u64);
    put_u64(&mut bytes, entry + 72, payload.len() as u64);
    bytes[entry + 80] = FrameKind::RawBlob as u8;
    bytes[entry + 96..entry + 128].copy_from_slice(blake3::hash(payload).as_bytes());
    BlobView::parse(&bytes, 0, bounds)?;
    Ok(bytes)
}

fn blob_root_content_id(media_type: &str, payload: ContentId, len: usize) -> ContentId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(BLOB_ROOT_DOMAIN);
    hasher.update(&(media_type.len() as u64).to_le_bytes());
    hasher.update(media_type.as_bytes());
    hasher.update(&(len as u64).to_le_bytes());
    hasher.update(payload.as_bytes());
    ContentId::from_bytes(*hasher.finalize().as_bytes())
}

fn validate_media_type(value: &str) -> Result<(), Bcs2Error> {
    if value.is_empty()
        || value.len() > 255
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-' | b'/'
                )
        })
    {
        return Err(Bcs2Error::SemanticEncoding);
    }
    Ok(())
}

fn parse_semantic_json(bytes: &[u8]) -> Result<(&str, ContentId, usize), Bcs2Error> {
    const PREFIX: &str = "{\"blob\":{\"content_id\":\"";
    const LENGTH: &str = "\",\"length\":";
    const MEDIA: &str = ",\"media_type\":\"";
    const SUFFIX: &str = "\"}}";
    let text = core::str::from_utf8(bytes).map_err(|_| Bcs2Error::CatalogCorrupt)?;
    let after_prefix = text.strip_prefix(PREFIX).ok_or(Bcs2Error::CatalogCorrupt)?;
    let (content_hex, after_content) = after_prefix
        .split_once(LENGTH)
        .ok_or(Bcs2Error::CatalogCorrupt)?;
    let (length, media_tail) = after_content
        .split_once(MEDIA)
        .ok_or(Bcs2Error::CatalogCorrupt)?;
    let media_type = media_tail
        .strip_suffix(SUFFIX)
        .ok_or(Bcs2Error::CatalogCorrupt)?;
    validate_media_type(media_type)?;
    let declared_len = length
        .parse::<usize>()
        .map_err(|_| Bcs2Error::CatalogCorrupt)?;
    let content_id = parse_content_id(content_hex)?;
    Ok((media_type, content_id, declared_len))
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
