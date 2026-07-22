use crate::wire::storage_id_for;
use crate::{
    Bcs2Error, Bcs2View, PrivacyMode, ProfileId, ResourceBounds, RootKind, StorageContract,
    BCS2_HEADER_LEN, BCS2_MAGIC,
};
use abir::{ContentId, StorageId};
use alloc::{vec, vec::Vec};
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};

pub const CAP_XCHACHA20_POLY1305: u64 = 1;
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const WIRE_MAJOR: u16 = 2;
const WIRE_MINOR: u16 = 0;
const SEMANTIC_GENERATION: u32 = 1;
const CIPHERTEXT_OFFSET: usize = BCS2_HEADER_LEN + NONCE_LEN;

#[derive(Debug)]
pub struct EncryptedEnvelopeView<'a> {
    bytes: &'a [u8],
    privacy_mode: PrivacyMode,
    profile: Option<ProfileId>,
    root_kind: Option<RootKind>,
    root_content_id: Option<ContentId>,
    nonce: &'a [u8; NONCE_LEN],
    ciphertext: &'a [u8],
}

impl<'a> EncryptedEnvelopeView<'a> {
    pub fn parse(bytes: &'a [u8], accepted_bounds: ResourceBounds) -> Result<Self, Bcs2Error> {
        if bytes.len() < CIPHERTEXT_OFFSET + TAG_LEN {
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
        if get_u32(bytes, 12)? != BCS2_HEADER_LEN as u32
            || get_u64(bytes, 24)? != CAP_XCHACHA20_POLY1305
            || get_u64(bytes, 32)? != 0
            || StorageContract::try_from(bytes[41])? != StorageContract::SealedImmutable
            || bytes[43] != 2
            || get_u32(bytes, 48)? as usize != NONCE_LEN
            || get_u32(bytes, 52)? as usize != TAG_LEN
            || to_usize(get_u64(bytes, 56)?)? != CIPHERTEXT_OFFSET
            || to_usize(get_u64(bytes, 72)?)? != BCS2_HEADER_LEN
            || to_usize(get_u64(bytes, 80)?)? != NONCE_LEN
            || get_u64(bytes, 88)? != 0
        {
            return Err(Bcs2Error::InvalidEncryptedEnvelope);
        }
        let privacy_mode = PrivacyMode::try_from(bytes[42])?;
        if !matches!(
            privacy_mode,
            PrivacyMode::EncryptedOpaque | PrivacyMode::EncryptedDiscoverable
        ) {
            return Err(Bcs2Error::InvalidEncryptedEnvelope);
        }
        let ciphertext_len = to_usize(get_u64(bytes, 64)?)?;
        if ciphertext_len < TAG_LEN
            || ciphertext_len > accepted_bounds.max_frame_bytes as usize
            || get_u32(bytes, 44)? as usize != ciphertext_len
            || CIPHERTEXT_OFFSET
                .checked_add(ciphertext_len)
                .ok_or(Bcs2Error::InvalidExtent)?
                != bytes.len()
        {
            return Err(Bcs2Error::BoundsExceeded);
        }
        let nonce: &[u8; NONCE_LEN] = bytes[BCS2_HEADER_LEN..CIPHERTEXT_OFFSET]
            .try_into()
            .map_err(|_| Bcs2Error::InvalidEncryptedEnvelope)?;
        let ciphertext = &bytes[CIPHERTEXT_OFFSET..];
        let (profile, root_kind, root_content_id) = if privacy_mode == PrivacyMode::EncryptedOpaque
        {
            if bytes[16..24].iter().any(|byte| *byte != 0)
                || bytes[40] != 0
                || bytes[96..128].iter().any(|byte| *byte != 0)
            {
                return Err(Bcs2Error::InvalidEncryptedEnvelope);
            }
            (None, None, None)
        } else {
            let profile = ProfileId::from_registered(get_u32(bytes, 16)?)?;
            if get_u32(bytes, 20)? != SEMANTIC_GENERATION {
                return Err(Bcs2Error::InvalidEncryptedEnvelope);
            }
            let root_kind = RootKind::try_from(bytes[40])?;
            if !profile.accepts(root_kind) {
                return Err(Bcs2Error::ProfileRootMismatch);
            }
            let root_content_id = content_id_at(bytes, 96)?;
            (Some(profile), Some(root_kind), Some(root_content_id))
        };
        Ok(Self {
            bytes,
            privacy_mode,
            profile,
            root_kind,
            root_content_id,
            nonce,
            ciphertext,
        })
    }

    pub const fn privacy_mode(&self) -> PrivacyMode {
        self.privacy_mode
    }
    pub const fn disclosed_profile(&self) -> Option<ProfileId> {
        self.profile
    }
    pub const fn disclosed_root_kind(&self) -> Option<RootKind> {
        self.root_kind
    }
    pub const fn disclosed_root_content_id(&self) -> Option<ContentId> {
        self.root_content_id
    }
    pub const fn nonce(&self) -> &'a [u8; NONCE_LEN] {
        self.nonce
    }
    pub const fn ciphertext(&self) -> &'a [u8] {
        self.ciphertext
    }
    pub fn storage_id(&self) -> StorageId {
        storage_id_for(self.bytes)
    }
}

pub fn encrypt_bcs2(
    plaintext: &[u8],
    privacy_mode: PrivacyMode,
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    supported_capabilities: u64,
    accepted_bounds: ResourceBounds,
) -> Result<Vec<u8>, Bcs2Error> {
    if !matches!(
        privacy_mode,
        PrivacyMode::EncryptedOpaque | PrivacyMode::EncryptedDiscoverable
    ) {
        return Err(Bcs2Error::InvalidEncryptedEnvelope);
    }
    let inner = Bcs2View::parse(plaintext, supported_capabilities, accepted_bounds)?;
    let ciphertext_len = plaintext
        .len()
        .checked_add(TAG_LEN)
        .ok_or(Bcs2Error::BoundsExceeded)?;
    if ciphertext_len > accepted_bounds.max_frame_bytes as usize {
        return Err(Bcs2Error::BoundsExceeded);
    }
    let ciphertext_len_u32 =
        u32::try_from(ciphertext_len).map_err(|_| Bcs2Error::BoundsExceeded)?;
    let total = CIPHERTEXT_OFFSET
        .checked_add(ciphertext_len)
        .ok_or(Bcs2Error::BoundsExceeded)?;
    let mut bytes = vec![0_u8; total];
    bytes[..8].copy_from_slice(&BCS2_MAGIC);
    put_u16(&mut bytes, 8, WIRE_MAJOR);
    put_u16(&mut bytes, 10, WIRE_MINOR);
    put_u32(&mut bytes, 12, BCS2_HEADER_LEN as u32);
    if privacy_mode == PrivacyMode::EncryptedDiscoverable {
        put_u32(&mut bytes, 16, inner.profile().get());
        put_u32(&mut bytes, 20, SEMANTIC_GENERATION);
        bytes[40] = inner.root_kind() as u8;
        bytes[96..128].copy_from_slice(inner.root_content_id().as_bytes());
    }
    put_u64(&mut bytes, 24, CAP_XCHACHA20_POLY1305);
    bytes[41] = StorageContract::SealedImmutable as u8;
    bytes[42] = privacy_mode as u8;
    bytes[43] = 2;
    put_u32(&mut bytes, 44, ciphertext_len_u32);
    put_u32(&mut bytes, 48, NONCE_LEN as u32);
    put_u32(&mut bytes, 52, TAG_LEN as u32);
    put_u64(&mut bytes, 56, CIPHERTEXT_OFFSET as u64);
    put_u64(&mut bytes, 64, ciphertext_len as u64);
    put_u64(&mut bytes, 72, BCS2_HEADER_LEN as u64);
    put_u64(&mut bytes, 80, NONCE_LEN as u64);
    bytes[BCS2_HEADER_LEN..CIPHERTEXT_OFFSET].copy_from_slice(nonce);
    let cipher = XChaCha20Poly1305::new(key.into());
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad: &bytes[..BCS2_HEADER_LEN],
            },
        )
        .map_err(|_| Bcs2Error::AuthenticationFailed)?;
    bytes[CIPHERTEXT_OFFSET..].copy_from_slice(&ciphertext);
    Ok(bytes)
}

pub fn decrypt_bcs2(
    encrypted: &[u8],
    key: &[u8; 32],
    supported_capabilities: u64,
    accepted_bounds: ResourceBounds,
) -> Result<Vec<u8>, Bcs2Error> {
    let envelope = EncryptedEnvelopeView::parse(encrypted, accepted_bounds)?;
    let cipher = XChaCha20Poly1305::new(key.into());
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(envelope.nonce),
            Payload {
                msg: envelope.ciphertext,
                aad: &encrypted[..BCS2_HEADER_LEN],
            },
        )
        .map_err(|_| Bcs2Error::AuthenticationFailed)?;
    let inner = Bcs2View::parse(&plaintext, supported_capabilities, accepted_bounds)?;
    if let Some(profile) = envelope.profile {
        if profile != inner.profile()
            || envelope.root_kind != Some(inner.root_kind())
            || envelope.root_content_id != Some(inner.root_content_id())
        {
            return Err(Bcs2Error::InvalidEncryptedEnvelope);
        }
    }
    Ok(plaintext)
}

fn content_id_at(bytes: &[u8], offset: usize) -> Result<ContentId, Bcs2Error> {
    let value: [u8; 32] = bytes
        .get(offset..offset + 32)
        .ok_or(Bcs2Error::TooShort)?
        .try_into()
        .map_err(|_| Bcs2Error::TooShort)?;
    Ok(ContentId::from_bytes(value))
}

fn get_u16(bytes: &[u8], offset: usize) -> Result<u16, Bcs2Error> {
    let value: [u8; 2] = bytes
        .get(offset..offset + 2)
        .ok_or(Bcs2Error::TooShort)?
        .try_into()
        .map_err(|_| Bcs2Error::TooShort)?;
    Ok(u16::from_le_bytes(value))
}

fn get_u32(bytes: &[u8], offset: usize) -> Result<u32, Bcs2Error> {
    let value: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or(Bcs2Error::TooShort)?
        .try_into()
        .map_err(|_| Bcs2Error::TooShort)?;
    Ok(u32::from_le_bytes(value))
}

fn get_u64(bytes: &[u8], offset: usize) -> Result<u64, Bcs2Error> {
    let value: [u8; 8] = bytes
        .get(offset..offset + 8)
        .ok_or(Bcs2Error::TooShort)?
        .try_into()
        .map_err(|_| Bcs2Error::TooShort)?;
    Ok(u64::from_le_bytes(value))
}

fn to_usize(value: u64) -> Result<usize, Bcs2Error> {
    usize::try_from(value).map_err(|_| Bcs2Error::InvalidExtent)
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
