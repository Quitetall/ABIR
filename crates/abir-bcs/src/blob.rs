use crate::wire::{encode_raw_root, raw_content_id};
use crate::{
    Bcs2Error, Bcs2View, FrameKind, PrivacyMode, ProfileId, ResourceBounds, RootKind,
    StorageContract,
};
use abir::ContentId;
use alloc::format;
use alloc::vec::Vec;

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
    let bytes = encode_raw_root(
        RootKind::Blob,
        ProfileId::FORENSIC_IMAGE_V1,
        root_content_id,
        semantic_json.as_bytes(),
        [payload],
        bounds,
    )?;
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
    let Some((type_name, subtype)) = value.split_once('/') else {
        return Err(Bcs2Error::SemanticEncoding);
    };
    if subtype.contains('/') || !valid_restricted_name(type_name) || !valid_restricted_name(subtype)
    {
        return Err(Bcs2Error::SemanticEncoding);
    }
    Ok(())
}

fn valid_restricted_name(value: &str) -> bool {
    value.len() <= 127
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-'
                )
        })
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
