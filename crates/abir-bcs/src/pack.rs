use crate::wire::{get_u64, put_u32, put_u64, INDEX_ENTRY_LEN, INDEX_LEN, INDEX_MAGIC};
use crate::{Bcs2Error, Bcs2View, PrivacyMode, ResourceBounds, StorageContract, BCS2_HEADER_LEN};
use abir::{ContentId, StorageId};
use alloc::collections::BTreeMap;
use alloc::{vec, vec::Vec};

struct Embedded<'a> {
    storage_id: StorageId,
    bytes: &'a [u8],
}

/// Rewrites an immutable plaintext artifact as a deterministic, self-contained
/// physical variant carrying borrowed BCS2 objects in fixed frame-index order.
pub fn repack_with_frames(
    root_bytes: &[u8],
    embedded_objects: &[&[u8]],
    supported_capabilities: u64,
    accepted_bounds: ResourceBounds,
) -> Result<Vec<u8>, Bcs2Error> {
    let root = Bcs2View::parse(root_bytes, supported_capabilities, accepted_bounds)?;
    if root.storage_contract() != StorageContract::SealedImmutable {
        return Err(Bcs2Error::StorageContractNotImplemented(
            root.storage_contract(),
        ));
    }
    if root.privacy_mode() != PrivacyMode::Plaintext {
        return Err(Bcs2Error::PrivacyModeNotImplemented(root.privacy_mode()));
    }
    if !root.profile().is_portable() {
        return Err(Bcs2Error::ProfileNotPortable);
    }
    if !root.frames().is_empty() {
        return Err(Bcs2Error::DuplicateFrame);
    }
    if embedded_objects.len() > root.bounds().max_index_entries as usize {
        return Err(Bcs2Error::BoundsExceeded);
    }

    let mut embedded = BTreeMap::<ContentId, Embedded<'_>>::new();
    for bytes in embedded_objects {
        if bytes.len() > root.bounds().max_frame_bytes as usize {
            return Err(Bcs2Error::BoundsExceeded);
        }
        let view = Bcs2View::parse(bytes, supported_capabilities, accepted_bounds)?;
        if !view.frames().is_empty() || view.root_content_id() == root.root_content_id() {
            return Err(Bcs2Error::DuplicateFrame);
        }
        let item = Embedded {
            storage_id: view.storage_id(),
            bytes,
        };
        if embedded.insert(view.root_content_id(), item).is_some() {
            return Err(Bcs2Error::DuplicateFrame);
        }
    }

    let catalog_offset =
        usize::try_from(get_u64(root_bytes, 56)?).map_err(|_| Bcs2Error::InvalidExtent)?;
    let catalog_len =
        usize::try_from(get_u64(root_bytes, 64)?).map_err(|_| Bcs2Error::InvalidExtent)?;
    let catalog_end = catalog_offset
        .checked_add(catalog_len)
        .ok_or(Bcs2Error::InvalidExtent)?;
    let catalog = root_bytes
        .get(catalog_offset..catalog_end)
        .ok_or(Bcs2Error::InvalidExtent)?;
    let frame_bytes = embedded.values().try_fold(0_usize, |total, item| {
        total
            .checked_add(item.bytes.len())
            .ok_or(Bcs2Error::BoundsExceeded)
    })?;
    let index_len = INDEX_LEN
        .checked_add(
            embedded
                .len()
                .checked_mul(INDEX_ENTRY_LEN)
                .ok_or(Bcs2Error::BoundsExceeded)?,
        )
        .ok_or(Bcs2Error::BoundsExceeded)?;
    let index_offset = BCS2_HEADER_LEN
        .checked_add(catalog.len())
        .and_then(|offset| offset.checked_add(frame_bytes))
        .ok_or(Bcs2Error::BoundsExceeded)?;
    let total = index_offset
        .checked_add(index_len)
        .ok_or(Bcs2Error::BoundsExceeded)?;
    let mut packed = vec![0_u8; total];
    packed[..BCS2_HEADER_LEN].copy_from_slice(&root_bytes[..BCS2_HEADER_LEN]);
    put_u64(&mut packed, 56, BCS2_HEADER_LEN as u64);
    put_u64(&mut packed, 64, catalog.len() as u64);
    put_u64(&mut packed, 72, index_offset as u64);
    put_u64(&mut packed, 80, index_len as u64);
    put_u64(&mut packed, 88, 0);
    packed[BCS2_HEADER_LEN..BCS2_HEADER_LEN + catalog.len()].copy_from_slice(catalog);

    packed[index_offset..index_offset + 8].copy_from_slice(&INDEX_MAGIC);
    put_u32(&mut packed, index_offset + 8, embedded.len() as u32);
    packed[index_offset + 16..index_offset + 48].copy_from_slice(blake3::hash(catalog).as_bytes());
    let mut frame_offset = BCS2_HEADER_LEN + catalog.len();
    for (entry_number, (content_id, item)) in embedded.iter().enumerate() {
        let frame_end = frame_offset + item.bytes.len();
        packed[frame_offset..frame_end].copy_from_slice(item.bytes);
        let entry_offset = index_offset + INDEX_LEN + entry_number * INDEX_ENTRY_LEN;
        packed[entry_offset..entry_offset + 32].copy_from_slice(content_id.as_bytes());
        packed[entry_offset + 32..entry_offset + 64].copy_from_slice(item.storage_id.as_bytes());
        put_u64(&mut packed, entry_offset + 64, frame_offset as u64);
        put_u64(&mut packed, entry_offset + 72, item.bytes.len() as u64);
        packed[entry_offset + 80] = 1;
        packed[entry_offset + 96..entry_offset + 128]
            .copy_from_slice(blake3::hash(item.bytes).as_bytes());
        frame_offset = frame_end;
    }
    let verified = Bcs2View::parse(&packed, supported_capabilities, accepted_bounds)?;
    if verified.root_content_id() != root.root_content_id()
        || verified.frames().len() != embedded.len()
    {
        return Err(Bcs2Error::FrameIdentityMismatch);
    }
    Ok(packed)
}
