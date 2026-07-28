//! A password-encrypted envelope carries its key-derivation parameters, bound
//! to the ciphertext they belong to.
//!
//! Before this, salt and Argon2 cost parameters lived in a detached
//! `<file>.lmcrypt.header` sidecar. Two failures followed from that, and both
//! are silent:
//!
//! 1. Losing one file destroys the ciphertext permanently — the key can never
//!    be re-derived, and nothing about the ciphertext says why.
//! 2. Nothing bound the pair, so a *mismatched* sidecar derives the wrong key
//!    and fails authentication in a way indistinguishable from corruption. An
//!    operator sees "this file is damaged" when the file is perfectly intact.
//!
//! The descriptor now travels inside the envelope, in the clear (these are not
//! secrets — they are needed *before* any key exists) and inside the AEAD
//! associated data. So a descriptor that does not belong to this ciphertext is
//! DETECTED, which is a different and honest answer.

use abir_bcs::{
    decrypt_bcs2, encode_blob, encrypt_bcs2, encrypt_bcs2_with_kdf, EncryptedEnvelopeView,
    PrivacyMode, ResourceBounds, KDF_ALGORITHM_ARGON2ID,
};

const KEY: [u8; 32] = [0x5a; 32];
const NONCE: [u8; 24] = [0x11; 24];

/// A 24-byte Argon2id descriptor: 16-byte salt, then m/t/p cost parameters.
fn argon2_descriptor(salt_byte: u8) -> Vec<u8> {
    let mut descriptor = vec![salt_byte; 16];
    descriptor.extend_from_slice(&65536_u32.to_le_bytes()); // m_kib
    descriptor.extend_from_slice(&3_u16.to_le_bytes()); // t_cost
    descriptor.push(1); // p_cost
    descriptor.push(0); // reserved
    descriptor
}

fn plaintext() -> Vec<u8> {
    encode_blob(
        b"sealed recording",
        "application/octet-stream",
        ResourceBounds::default(),
    )
    .expect("inner artifact encodes")
}

fn sealed_with(descriptor: &[u8]) -> Vec<u8> {
    encrypt_bcs2_with_kdf(
        &plaintext(),
        PrivacyMode::EncryptedOpaque,
        &KEY,
        &NONCE,
        Some((KDF_ALGORITHM_ARGON2ID, descriptor)),
        u64::MAX,
        ResourceBounds::default(),
    )
    .expect("envelope encodes")
}

#[test]
fn an_envelope_without_a_descriptor_is_unchanged() {
    // The compatibility claim, asserted rather than assumed: every envelope
    // written before this field existed must still decrypt, which requires that
    // an absent descriptor writes no bytes AND authenticates exactly the header.
    let direct = encrypt_bcs2(
        &plaintext(),
        PrivacyMode::EncryptedOpaque,
        &KEY,
        &NONCE,
        u64::MAX,
        ResourceBounds::default(),
    )
    .expect("encodes");
    let via_kdf = encrypt_bcs2_with_kdf(
        &plaintext(),
        PrivacyMode::EncryptedOpaque,
        &KEY,
        &NONCE,
        None,
        u64::MAX,
        ResourceBounds::default(),
    )
    .expect("encodes");

    assert_eq!(
        direct, via_kdf,
        "an absent descriptor must produce byte-identical output"
    );
    assert_eq!(
        decrypt_bcs2(&direct, &KEY, u64::MAX, ResourceBounds::default()).unwrap(),
        plaintext()
    );
}

#[test]
fn the_descriptor_is_readable_without_the_key_and_round_trips() {
    // It must be readable BEFORE any key exists — that is what it is for.
    let descriptor = argon2_descriptor(0xA1);
    let sealed = sealed_with(&descriptor);

    let view = EncryptedEnvelopeView::parse(&sealed, ResourceBounds::default())
        .expect("parses without a key");
    assert_eq!(view.kdf_descriptor(), descriptor.as_slice());
    assert_eq!(view.kdf_algorithm(), KDF_ALGORITHM_ARGON2ID);

    assert_eq!(
        decrypt_bcs2(&sealed, &KEY, u64::MAX, ResourceBounds::default()).unwrap(),
        plaintext()
    );
}

#[test]
fn tampering_with_the_descriptor_is_an_authentication_failure() {
    // THE point. With a sidecar, altering the salt silently derives a different
    // key and the failure looks like corruption. Here it is detected as what it
    // is.
    let sealed = sealed_with(&argon2_descriptor(0xA1));
    let mut tampered = sealed.clone();
    // Flip one salt byte, which sits immediately after the nonce.
    let salt_start = 128 + 24;
    tampered[salt_start] ^= 0x01;

    assert!(
        decrypt_bcs2(&tampered, &KEY, u64::MAX, ResourceBounds::default()).is_err(),
        "an altered salt must fail authentication, not derive a different key"
    );
}

#[test]
fn a_descriptor_from_another_envelope_is_rejected() {
    // The mismatched-sidecar failure, reproduced deliberately. Two envelopes
    // over the same plaintext with different salts: splicing one's descriptor
    // into the other must not decrypt.
    let mine = sealed_with(&argon2_descriptor(0xA1));
    let theirs = sealed_with(&argon2_descriptor(0xB2));
    assert_ne!(
        mine, theirs,
        "different salts must produce different envelopes"
    );

    let descriptor_range = 128 + 24..128 + 24 + 24;
    let mut spliced = mine.clone();
    spliced[descriptor_range.clone()].copy_from_slice(&theirs[descriptor_range]);

    assert!(
        decrypt_bcs2(&spliced, &KEY, u64::MAX, ResourceBounds::default()).is_err(),
        "a descriptor that does not belong to this ciphertext must be detected"
    );
}

#[test]
fn half_written_declarations_are_refused() {
    // A descriptor with no algorithm, or an algorithm with no descriptor, is
    // half a statement. Admitting either would give one envelope two meanings.
    let descriptor = argon2_descriptor(0xA1);
    assert!(
        encrypt_bcs2_with_kdf(
            &plaintext(),
            PrivacyMode::EncryptedOpaque,
            &KEY,
            &NONCE,
            Some((0, &descriptor)),
            u64::MAX,
            ResourceBounds::default(),
        )
        .is_err(),
        "algorithm zero is the spelling for absent; it cannot accompany a descriptor"
    );
    assert!(
        encrypt_bcs2_with_kdf(
            &plaintext(),
            PrivacyMode::EncryptedOpaque,
            &KEY,
            &NONCE,
            Some((KDF_ALGORITHM_ARGON2ID, &[])),
            u64::MAX,
            ResourceBounds::default(),
        )
        .is_err(),
        "an algorithm with an empty descriptor says nothing"
    );
}

#[test]
fn an_oversized_descriptor_is_refused() {
    // Descriptors are a salt and a few costs. The cap stops a malformed length
    // from driving an allocation.
    let huge = vec![0_u8; 4096];
    assert!(encrypt_bcs2_with_kdf(
        &plaintext(),
        PrivacyMode::EncryptedOpaque,
        &KEY,
        &NONCE,
        Some((KDF_ALGORITHM_ARGON2ID, &huge)),
        u64::MAX,
        ResourceBounds::default(),
    )
    .is_err());
}

#[test]
fn a_declared_length_longer_than_the_envelope_is_refused() {
    // Hand-edited length field: the parser must not slice past the buffer.
    let mut sealed = sealed_with(&argon2_descriptor(0xA1));
    sealed[32..36].copy_from_slice(&200_u32.to_le_bytes());
    assert!(EncryptedEnvelopeView::parse(&sealed, ResourceBounds::default()).is_err());
}
