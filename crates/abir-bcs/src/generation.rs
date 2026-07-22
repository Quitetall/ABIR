use crate::Bcs2Error;
use abir::ContentId;
use alloc::vec::Vec;

pub const GENERATION_FOOTER_LEN: usize = 160;
const MAGIC: [u8; 8] = *b"BCS2GEN\0";
const DIGEST_DOMAIN: &[u8] = b"org.quitetall.abir.bcs2.generation-v1\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerationFooter {
    pub generation: u64,
    pub previous_offset: u64,
    pub previous_digest: [u8; 32],
    pub catalog_offset: u64,
    pub catalog_len: u64,
    pub index_offset: u64,
    pub index_len: u64,
    pub root_content_id: ContentId,
    pub digest: [u8; 32],
}

pub fn encode_generation_footer(
    artifact_prefix: &[u8],
    mut footer: GenerationFooter,
) -> Result<[u8; GENERATION_FOOTER_LEN], Bcs2Error> {
    validate_links(&footer)?;
    validate_extents(&footer, artifact_prefix.len() as u64)?;
    let catalog = extent(artifact_prefix, footer.catalog_offset, footer.catalog_len)?;
    let index = extent(artifact_prefix, footer.index_offset, footer.index_len)?;
    let mut bytes = [0_u8; GENERATION_FOOTER_LEN];
    bytes[..8].copy_from_slice(&MAGIC);
    put_u16(&mut bytes, 8, 2);
    put_u16(&mut bytes, 10, 0);
    put_u32(&mut bytes, 12, GENERATION_FOOTER_LEN as u32);
    put_u64(&mut bytes, 16, footer.generation);
    put_u64(&mut bytes, 24, footer.previous_offset);
    bytes[32..64].copy_from_slice(&footer.previous_digest);
    put_u64(&mut bytes, 64, footer.catalog_offset);
    put_u64(&mut bytes, 72, footer.catalog_len);
    put_u64(&mut bytes, 80, footer.index_offset);
    put_u64(&mut bytes, 88, footer.index_len);
    bytes[96..128].copy_from_slice(footer.root_content_id.as_bytes());
    footer.digest = digest(&bytes[..128], catalog, index);
    bytes[128..160].copy_from_slice(&footer.digest);
    Ok(bytes)
}

impl GenerationFooter {
    pub fn parse(artifact: &[u8], offset: u64) -> Result<Self, Bcs2Error> {
        let offset = usize::try_from(offset).map_err(|_| Bcs2Error::InvalidExtent)?;
        let end = offset
            .checked_add(GENERATION_FOOTER_LEN)
            .ok_or(Bcs2Error::InvalidExtent)?;
        let bytes = artifact.get(offset..end).ok_or(Bcs2Error::InvalidExtent)?;
        if bytes[..8] != MAGIC || get_u16(bytes, 8)? != 2 || get_u16(bytes, 10)? != 0 {
            return Err(Bcs2Error::CatalogCorrupt);
        }
        if get_u32(bytes, 12)? != GENERATION_FOOTER_LEN as u32 {
            return Err(Bcs2Error::NonCanonicalLayout);
        }
        let mut previous_digest = [0; 32];
        previous_digest.copy_from_slice(&bytes[32..64]);
        let mut root = [0; 32];
        root.copy_from_slice(&bytes[96..128]);
        let mut stored_digest = [0; 32];
        stored_digest.copy_from_slice(&bytes[128..160]);
        let footer = Self {
            generation: get_u64(bytes, 16)?,
            previous_offset: get_u64(bytes, 24)?,
            previous_digest,
            catalog_offset: get_u64(bytes, 64)?,
            catalog_len: get_u64(bytes, 72)?,
            index_offset: get_u64(bytes, 80)?,
            index_len: get_u64(bytes, 88)?,
            root_content_id: ContentId::from_bytes(root),
            digest: stored_digest,
        };
        validate_links(&footer)?;
        let catalog = extent(artifact, footer.catalog_offset, footer.catalog_len)?;
        let index = extent(artifact, footer.index_offset, footer.index_len)?;
        if digest(&bytes[..128], catalog, index) != stored_digest {
            return Err(Bcs2Error::CatalogDigestMismatch);
        }
        validate_extents(&footer, offset as u64)?;
        Ok(footer)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationChain {
    newest_first: Vec<GenerationFooter>,
}

impl GenerationChain {
    pub fn parse(
        artifact: &[u8],
        latest_offset: u64,
        max_generations: u32,
    ) -> Result<Self, Bcs2Error> {
        if latest_offset == 0 || max_generations == 0 {
            return Err(Bcs2Error::BoundsExceeded);
        }
        let mut current_offset = latest_offset;
        let mut expected_generation = None;
        let mut newest_first: Vec<GenerationFooter> = Vec::new();
        loop {
            if newest_first.len() >= max_generations as usize {
                return Err(Bcs2Error::BoundsExceeded);
            }
            let footer = GenerationFooter::parse(artifact, current_offset)?;
            if let Some(expected) = expected_generation {
                if footer.generation != expected {
                    return Err(Bcs2Error::CatalogCorrupt);
                }
                let child = newest_first.last().ok_or(Bcs2Error::CatalogCorrupt)?;
                if child.previous_digest != footer.digest {
                    return Err(Bcs2Error::CatalogDigestMismatch);
                }
            }
            if footer.generation == 0 {
                if footer.previous_offset != 0 || footer.previous_digest != [0; 32] {
                    return Err(Bcs2Error::CatalogCorrupt);
                }
                newest_first.push(footer);
                break;
            }
            if footer.previous_offset == 0 || footer.previous_offset >= current_offset {
                return Err(Bcs2Error::CatalogCorrupt);
            }
            expected_generation = Some(footer.generation - 1);
            current_offset = footer.previous_offset;
            newest_first.push(footer);
        }
        Ok(Self { newest_first })
    }

    pub fn newest_first(&self) -> &[GenerationFooter] {
        &self.newest_first
    }
}

fn validate_links(footer: &GenerationFooter) -> Result<(), Bcs2Error> {
    let valid = if footer.generation == 0 {
        footer.previous_offset == 0 && footer.previous_digest == [0; 32]
    } else {
        footer.previous_offset != 0 && footer.previous_digest != [0; 32]
    };
    if !valid {
        return Err(Bcs2Error::CatalogCorrupt);
    }
    Ok(())
}

fn validate_extents(footer: &GenerationFooter, footer_offset: u64) -> Result<(), Bcs2Error> {
    let catalog_end = footer
        .catalog_offset
        .checked_add(footer.catalog_len)
        .ok_or(Bcs2Error::InvalidExtent)?;
    let index_end = footer
        .index_offset
        .checked_add(footer.index_len)
        .ok_or(Bcs2Error::InvalidExtent)?;
    if catalog_end != footer.index_offset || index_end != footer_offset {
        return Err(Bcs2Error::NonCanonicalLayout);
    }
    Ok(())
}

fn digest(header: &[u8], catalog: &[u8], index: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(DIGEST_DOMAIN);
    hasher.update(header);
    hasher.update(catalog);
    hasher.update(index);
    *hasher.finalize().as_bytes()
}

fn extent(bytes: &[u8], offset: u64, len: u64) -> Result<&[u8], Bcs2Error> {
    let start = usize::try_from(offset).map_err(|_| Bcs2Error::InvalidExtent)?;
    let len = usize::try_from(len).map_err(|_| Bcs2Error::InvalidExtent)?;
    let end = start.checked_add(len).ok_or(Bcs2Error::InvalidExtent)?;
    bytes.get(start..end).ok_or(Bcs2Error::InvalidExtent)
}

fn get_u16(bytes: &[u8], offset: usize) -> Result<u16, Bcs2Error> {
    let value = bytes.get(offset..offset + 2).ok_or(Bcs2Error::TooShort)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}
fn get_u32(bytes: &[u8], offset: usize) -> Result<u32, Bcs2Error> {
    let value = bytes.get(offset..offset + 4).ok_or(Bcs2Error::TooShort)?;
    Ok(u32::from_le_bytes(
        value.try_into().map_err(|_| Bcs2Error::TooShort)?,
    ))
}
fn get_u64(bytes: &[u8], offset: usize) -> Result<u64, Bcs2Error> {
    let value = bytes.get(offset..offset + 8).ok_or(Bcs2Error::TooShort)?;
    Ok(u64::from_le_bytes(
        value.try_into().map_err(|_| Bcs2Error::TooShort)?,
    ))
}
fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
