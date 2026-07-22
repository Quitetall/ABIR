use crate::wire::{
    element_wire_code, get_u64, put_u32, put_u64, INDEX_ENTRY_LEN, INDEX_LEN, INDEX_MAGIC,
};
use crate::{
    encode_dataset, raw_storage_id, Bcs2Error, Bcs2View, FrameKind, ProfileId, ResourceBounds,
    StorageContract, BCS2_HEADER_LEN,
};
use abir::{
    verify_payload_content, AbirDataset, ContentId, ElementType, PayloadAccess, PayloadLease,
};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::{vec, vec::Vec};

struct SemanticPayload {
    element: ElementType,
    bytes: Vec<u8>,
}

/// Encodes one immutable dataset and closes every semantic payload descriptor
/// into a typed BCS2 frame addressable by its semantic `ContentId`.
pub fn encode_dataset_with_payloads<A: PayloadAccess>(
    dataset: &AbirDataset,
    access: &A,
    profile: ProfileId,
    bounds: ResourceBounds,
) -> Result<Vec<u8>, Bcs2Error> {
    let mut payloads = BTreeMap::new();
    for descriptor in dataset.atoms().iter().filter_map(abir::Atom::payload) {
        let lease = access
            .lease(descriptor)
            .map_err(|_| Bcs2Error::MissingPayload(descriptor.content_id()))?;
        verify_payload_content(descriptor, lease.bytes())
            .map_err(|_| Bcs2Error::PayloadDescriptorMismatch(descriptor.content_id()))?;
        let payload = SemanticPayload {
            element: descriptor.element(),
            bytes: lease.bytes().to_vec(),
        };
        if let Some(previous) = payloads.insert(descriptor.content_id(), payload) {
            let current = payloads
                .get(&descriptor.content_id())
                .ok_or(Bcs2Error::FrameIdentityMismatch)?;
            if previous.element != current.element || previous.bytes != current.bytes {
                return Err(Bcs2Error::DuplicateFrame);
            }
        }
    }
    let expected: BTreeSet<_> = dataset.payload_content_ids().into_iter().collect();
    let actual: BTreeSet<_> = payloads.keys().copied().collect();
    if let Some(missing) = expected.difference(&actual).next() {
        return Err(Bcs2Error::MissingPayload(*missing));
    }
    if let Some(extra) = actual.difference(&expected).next() {
        return Err(Bcs2Error::ExtraPortableFrame(*extra));
    }

    let base = encode_dataset(dataset, profile, bounds)?;
    repack_with_payloads(&base, &payloads, bounds)
}

fn repack_with_payloads(
    root_bytes: &[u8],
    payloads: &BTreeMap<ContentId, SemanticPayload>,
    accepted_bounds: ResourceBounds,
) -> Result<Vec<u8>, Bcs2Error> {
    let root = Bcs2View::parse(root_bytes, 0, accepted_bounds)?;
    if root.storage_contract() != StorageContract::SealedImmutable {
        return Err(Bcs2Error::StorageContractNotImplemented(
            root.storage_contract(),
        ));
    }
    if !root.frames().is_empty() {
        return Err(Bcs2Error::DuplicateFrame);
    }
    if payloads.len() > root.bounds().max_index_entries as usize {
        return Err(Bcs2Error::BoundsExceeded);
    }
    for payload in payloads.values() {
        if payload.bytes.len() > root.bounds().max_frame_bytes as usize {
            return Err(Bcs2Error::BoundsExceeded);
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
    let frame_bytes = payloads.values().try_fold(0_usize, |total, payload| {
        total
            .checked_add(payload.bytes.len())
            .ok_or(Bcs2Error::BoundsExceeded)
    })?;
    let index_len = INDEX_LEN
        .checked_add(
            payloads
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
    put_u32(
        &mut packed,
        index_offset + 8,
        u32::try_from(payloads.len()).map_err(|_| Bcs2Error::BoundsExceeded)?,
    );
    packed[index_offset + 16..index_offset + 48].copy_from_slice(blake3::hash(catalog).as_bytes());
    let mut frame_offset = BCS2_HEADER_LEN + catalog.len();
    for (entry_number, (content_id, payload)) in payloads.iter().enumerate() {
        let frame_end = frame_offset
            .checked_add(payload.bytes.len())
            .ok_or(Bcs2Error::BoundsExceeded)?;
        packed[frame_offset..frame_end].copy_from_slice(&payload.bytes);
        let entry = index_offset + INDEX_LEN + entry_number * INDEX_ENTRY_LEN;
        packed[entry..entry + 32].copy_from_slice(content_id.as_bytes());
        packed[entry + 32..entry + 64].copy_from_slice(raw_storage_id(&payload.bytes).as_bytes());
        put_u64(&mut packed, entry + 64, frame_offset as u64);
        put_u64(&mut packed, entry + 72, payload.bytes.len() as u64);
        packed[entry + 80] = FrameKind::SemanticPayload as u8;
        packed[entry + 81] = element_wire_code(payload.element);
        packed[entry + 96..entry + 128].copy_from_slice(blake3::hash(&payload.bytes).as_bytes());
        frame_offset = frame_end;
    }

    let verified = Bcs2View::parse(&packed, 0, accepted_bounds)?;
    if verified.root_content_id() != root.root_content_id()
        || verified.frames().len() != payloads.len()
        || verified
            .frames()
            .iter()
            .any(|frame| frame.kind() != FrameKind::SemanticPayload)
    {
        return Err(Bcs2Error::FrameIdentityMismatch);
    }
    Ok(packed)
}
