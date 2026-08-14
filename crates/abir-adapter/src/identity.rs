use std::fmt;

use abir::{ContentId, SourceKey};

const MAX_LABEL_BYTES: usize = 96;
const MAX_DIGEST_BYTES: usize = 128;

/// One byte-integrity observation, explicitly outside ABIR semantic identity.
///
/// `domain` names what bytes were measured (`source.bytes`,
/// `storage.artifact`, and similar); `algorithm` names how. Callers never pass a
/// bare 64-hex string and ask consumers to infer either fact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrityObservation {
    domain: String,
    algorithm: String,
    digest: Vec<u8>,
}

impl IntegrityObservation {
    pub fn new(
        domain: impl AsRef<str>,
        algorithm: impl AsRef<str>,
        digest: impl AsRef<[u8]>,
    ) -> Result<Self, IdentityProjectionError> {
        let domain = domain.as_ref();
        let algorithm = algorithm.as_ref();
        let digest = digest.as_ref();
        if !valid_label(domain)
            || !valid_label(algorithm)
            || digest.is_empty()
            || digest.len() > MAX_DIGEST_BYTES
        {
            return Err(IdentityProjectionError::InvalidIntegrityObservation);
        }
        Ok(Self {
            domain: domain.to_owned(),
            algorithm: algorithm.to_owned(),
            digest: digest.to_vec(),
        })
    }

    pub fn domain(&self) -> &str {
        &self.domain
    }

    pub fn algorithm(&self) -> &str {
        &self.algorithm
    }

    pub fn digest(&self) -> &[u8] {
        &self.digest
    }
}

/// Canonical semantic identity plus separately labelled observations and
/// foreign identifiers.
///
/// Construction sorts both auxiliary collections and rejects ambiguous
/// duplicates. Relocation, repacking, or checksum changes can therefore update
/// observations without redefining `content_id`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityProjection {
    content_id: ContentId,
    integrity_observations: Vec<IntegrityObservation>,
    source_keys: Vec<SourceKey>,
}

impl IdentityProjection {
    pub fn new(
        content_id: ContentId,
        mut integrity_observations: Vec<IntegrityObservation>,
        mut source_keys: Vec<SourceKey>,
    ) -> Result<Self, IdentityProjectionError> {
        integrity_observations.sort_by(|left, right| {
            (&left.domain, &left.algorithm).cmp(&(&right.domain, &right.algorithm))
        });
        if integrity_observations
            .windows(2)
            .any(|pair| pair[0].domain == pair[1].domain && pair[0].algorithm == pair[1].algorithm)
        {
            return Err(IdentityProjectionError::DuplicateIntegrityObservation);
        }

        source_keys.sort_by(|left, right| {
            (left.namespace(), left.value()).cmp(&(right.namespace(), right.value()))
        });
        if source_keys.windows(2).any(|pair| {
            pair[0].namespace() == pair[1].namespace() && pair[0].value() == pair[1].value()
        }) {
            return Err(IdentityProjectionError::DuplicateSourceKey);
        }

        Ok(Self {
            content_id,
            integrity_observations,
            source_keys,
        })
    }

    pub const fn content_id(&self) -> ContentId {
        self.content_id
    }

    pub fn integrity_observations(&self) -> &[IntegrityObservation] {
        &self.integrity_observations
    }

    pub fn source_keys(&self) -> &[SourceKey] {
        &self.source_keys
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityProjectionError {
    InvalidIntegrityObservation,
    DuplicateIntegrityObservation,
    DuplicateSourceKey,
}

impl fmt::Display for IdentityProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIntegrityObservation => {
                formatter.write_str("integrity observation domain, algorithm, or digest is invalid")
            }
            Self::DuplicateIntegrityObservation => {
                formatter.write_str("integrity observation domain and algorithm must be unique")
            }
            Self::DuplicateSourceKey => formatter.write_str("source keys must be unique"),
        }
    }
}

impl std::error::Error for IdentityProjectionError {}

fn valid_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_LABEL_BYTES
        && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
}
