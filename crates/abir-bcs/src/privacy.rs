use crate::wire::storage_id_for;
use crate::{
    Bcs2Error, Bcs2View, PrivacyMode, ProfileId, ResourceBounds, RootKind, StorageContract,
    BCS2_HEADER_LEN, BCS2_MAGIC,
};
use abir::{ContentId, StorageId};
use alloc::{vec, vec::Vec};
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};

/// Required-capability bitmask for crypto-registry bit zero (algorithm ID 2).
pub const CAP_XCHACHA20_POLY1305: u64 = 1 << 0;
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const WIRE_MAJOR: u16 = 2;
const WIRE_MINOR: u16 = 0;
const SEMANTIC_GENERATION: u32 = 1;
/// Offset of the key-derivation descriptor length, and of its algorithm id.
///
/// These eight bytes were enforced-zero, so every reader written before this
/// change rejects an envelope that uses them — forward compatibility that fails
/// closed by construction rather than by convention.
const KDF_LEN_OFFSET: usize = 32;
const KDF_ALGORITHM_OFFSET: usize = 36;

/// Argon2id, the only registered derivation today.
pub const KDF_ALGORITHM_ARGON2ID: u32 = 1;

/// Descriptors are small by nature — a salt and a few cost parameters. The cap
/// exists so a malformed length cannot drive an allocation.
const MAX_KDF_LEN: usize = 256;

/// Where the nonce ends and the descriptor begins.
const NONCE_END: usize = BCS2_HEADER_LEN + NONCE_LEN;

/// The bytes authenticated alongside the ciphertext: header, then descriptor.
///
/// Header and descriptor are not adjacent -- the nonce sits between them -- so
/// this has to be materialised rather than borrowed. Cheap: the header is fixed
/// and the descriptor is capped at [`MAX_KDF_LEN`].
///
/// With no descriptor the result is exactly the header, which is what keeps
/// every envelope written before this field existed decryptable byte for byte.
/// Binding the descriptor here is the entire point: a descriptor that does not
/// belong to this ciphertext fails authentication instead of quietly deriving
/// the wrong key.
fn associated_data(header: &[u8], kdf_descriptor: &[u8]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(header.len() + kdf_descriptor.len());
    aad.extend_from_slice(header);
    aad.extend_from_slice(kdf_descriptor);
    aad
}

#[derive(Debug)]
pub struct EncryptedEnvelopeView<'a> {
    bytes: &'a [u8],
    privacy_mode: PrivacyMode,
    profile: Option<ProfileId>,
    root_kind: Option<RootKind>,
    root_content_id: Option<ContentId>,
    nonce: &'a [u8; NONCE_LEN],
    kdf_descriptor: &'a [u8],
    kdf_algorithm: u32,
    ciphertext: &'a [u8],
}

impl<'a> EncryptedEnvelopeView<'a> {
    pub fn parse(bytes: &'a [u8], accepted_bounds: ResourceBounds) -> Result<Self, Bcs2Error> {
        // Minimum: header + nonce + tag. A descriptor only adds to this, and its
        // declared length is validated once the header is readable.
        if bytes.len() < NONCE_END + TAG_LEN {
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
            || StorageContract::try_from(bytes[41])? != StorageContract::SealedImmutable
            || bytes[43] != 2
            || get_u32(bytes, 48)? as usize != NONCE_LEN
            || get_u32(bytes, 52)? as usize != TAG_LEN
            || to_usize(get_u64(bytes, 72)?)? != BCS2_HEADER_LEN
            || to_usize(get_u64(bytes, 80)?)? != NONCE_LEN
            || get_u64(bytes, 88)? != 0
        {
            return Err(Bcs2Error::InvalidEncryptedEnvelope);
        }
        let kdf_len = get_u32(bytes, KDF_LEN_OFFSET)? as usize;
        let kdf_algorithm = get_u32(bytes, KDF_ALGORITHM_OFFSET)?;
        if kdf_len > MAX_KDF_LEN
            // An algorithm without a descriptor, or a descriptor without an
            // algorithm, is a half-written statement. Refuse both spellings so
            // one envelope has one meaning.
            || (kdf_len == 0) != (kdf_algorithm == 0)
        {
            return Err(Bcs2Error::InvalidEncryptedEnvelope);
        }
        let ciphertext_offset = NONCE_END
            .checked_add(kdf_len)
            .ok_or(Bcs2Error::InvalidExtent)?;
        if to_usize(get_u64(bytes, 56)?)? != ciphertext_offset {
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
            || ciphertext_offset
                .checked_add(ciphertext_len)
                .ok_or(Bcs2Error::InvalidExtent)?
                != bytes.len()
        {
            return Err(Bcs2Error::BoundsExceeded);
        }
        let nonce: &[u8; NONCE_LEN] = bytes[BCS2_HEADER_LEN..NONCE_END]
            .try_into()
            .map_err(|_| Bcs2Error::InvalidEncryptedEnvelope)?;
        let kdf_descriptor = &bytes[NONCE_END..ciphertext_offset];
        let ciphertext = &bytes[ciphertext_offset..];
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
            kdf_descriptor,
            kdf_algorithm,
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
    /// The key-derivation descriptor, empty when the key was supplied directly.
    ///
    /// Authenticated as associated data, so a descriptor that does not belong to
    /// this ciphertext is DETECTED rather than silently deriving the wrong key —
    /// which is the failure a detached sidecar produces, and which is
    /// indistinguishable from corruption when it happens.
    pub const fn kdf_descriptor(&self) -> &'a [u8] {
        self.kdf_descriptor
    }

    /// Registered algorithm id for [`Self::kdf_descriptor`]; zero when absent.
    pub const fn kdf_algorithm(&self) -> u32 {
        self.kdf_algorithm
    }

    pub const fn ciphertext(&self) -> &'a [u8] {
        self.ciphertext
    }
    pub fn storage_id(&self) -> StorageId {
        storage_id_for(self.bytes)
    }
}

/// Encrypts one already-validated plaintext BCS2 artifact.
///
/// `nonce` must be generated by a CSPRNG and must never repeat for the same
/// key. Already-encrypted envelopes are deliberately rejected; decrypt before
/// changing encryption or discovery policy.
/// Encrypt with the key supplied directly.
///
/// Byte-identical to what this function produced before key-derivation
/// descriptors existed, because an absent descriptor writes no bytes and
/// authenticates exactly the header.
pub fn encrypt_bcs2(
    plaintext: &[u8],
    privacy_mode: PrivacyMode,
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    supported_capabilities: u64,
    accepted_bounds: ResourceBounds,
) -> Result<Vec<u8>, Bcs2Error> {
    encrypt_bcs2_with_kdf(
        plaintext,
        privacy_mode,
        key,
        nonce,
        None,
        supported_capabilities,
        accepted_bounds,
    )
}

/// Encrypt, recording how the key was derived.
///
/// `kdf` is `(algorithm, descriptor)` — for Argon2id, the salt and cost
/// parameters. It is stored in the clear (these are not secrets) and
/// authenticated, so it travels WITH the ciphertext instead of beside it.
///
/// That is the whole reason this exists. A detached sidecar means losing one
/// file destroys the ciphertext permanently, and nothing binds the pair, so a
/// mismatched sidecar derives the wrong key and fails in a way indistinguishable
/// from corruption. Here a mismatch is an authentication failure, which is a
/// different and honest answer.
pub fn encrypt_bcs2_with_kdf(
    plaintext: &[u8],
    privacy_mode: PrivacyMode,
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    kdf: Option<(u32, &[u8])>,
    supported_capabilities: u64,
    accepted_bounds: ResourceBounds,
) -> Result<Vec<u8>, Bcs2Error> {
    if !matches!(
        privacy_mode,
        PrivacyMode::EncryptedOpaque | PrivacyMode::EncryptedDiscoverable
    ) {
        return Err(Bcs2Error::InvalidEncryptedEnvelope);
    }
    let (kdf_algorithm, kdf_descriptor) = match kdf {
        // Zero is the spelling for "no descriptor", so an algorithm id of zero
        // with a descriptor would be unreadable on the way back in.
        Some((algorithm, descriptor)) => {
            if algorithm == 0 || descriptor.is_empty() || descriptor.len() > MAX_KDF_LEN {
                return Err(Bcs2Error::InvalidEncryptedEnvelope);
            }
            (algorithm, descriptor)
        }
        None => (0, &[][..]),
    };
    let ciphertext_offset = NONCE_END
        .checked_add(kdf_descriptor.len())
        .ok_or(Bcs2Error::BoundsExceeded)?;
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
    let total = ciphertext_offset
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
    put_u32(&mut bytes, KDF_LEN_OFFSET, kdf_descriptor.len() as u32);
    put_u32(&mut bytes, KDF_ALGORITHM_OFFSET, kdf_algorithm);
    put_u64(&mut bytes, 56, ciphertext_offset as u64);
    put_u64(&mut bytes, 64, ciphertext_len as u64);
    put_u64(&mut bytes, 72, BCS2_HEADER_LEN as u64);
    put_u64(&mut bytes, 80, NONCE_LEN as u64);
    bytes[BCS2_HEADER_LEN..NONCE_END].copy_from_slice(nonce);
    bytes[NONCE_END..ciphertext_offset].copy_from_slice(kdf_descriptor);
    let aad = associated_data(&bytes[..BCS2_HEADER_LEN], kdf_descriptor);
    let cipher = XChaCha20Poly1305::new(key.into());
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| Bcs2Error::AuthenticationFailed)?;
    bytes[ciphertext_offset..].copy_from_slice(&ciphertext);
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
                aad: &associated_data(&encrypted[..BCS2_HEADER_LEN], envelope.kdf_descriptor()),
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
