use crate::TrainingError;
use abir::{ContentId, ElementType};
use abir_bcs::{encode_semantic_bundle, ProfileId, ResourceBounds, SemanticPayloadFrame};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

const SPEC_HASH_DOMAIN: &[u8] = b"org.quitetall.abir.training.spec-v1\0";
const SNAPSHOT_HASH_DOMAIN: &[u8] = b"org.quitetall.abir.training.snapshot-v1\0";
const SNAPSHOT_SCHEMA: &str = "org.quitetall.abir.training.snapshot-v1";

/// A serde-compatible lowercase hexadecimal wrapper around an ABIR ContentId.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContentKey(ContentId);

impl ContentKey {
    pub const fn new(content_id: ContentId) -> Self {
        Self(content_id)
    }

    pub const fn content_id(self) -> ContentId {
        self.0
    }
}

impl From<ContentId> for ContentKey {
    fn from(value: ContentId) -> Self {
        Self(value)
    }
}

impl From<ContentKey> for ContentId {
    fn from(value: ContentKey) -> Self {
        value.0
    }
}

impl fmt::Display for ContentKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for ContentKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ContentKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        decode_content_key(&value).map_err(serde::de::Error::custom)
    }
}

fn decode_content_key(value: &str) -> Result<ContentKey, TrainingError> {
    if value.len() != 64
        || value
            .as_bytes()
            .iter()
            .any(|byte| !matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(TrainingError::InvalidContentKey);
    }
    let mut decoded = [0_u8; 32];
    for (output, pair) in decoded.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        *output = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    Ok(ContentKey(ContentId::from_bytes(decoded)))
}

fn hex_nibble(value: u8) -> Result<u8, TrainingError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(TrainingError::InvalidContentKey),
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrainingProfile {
    Speed,
    Balanced,
    Memory,
    Compact,
    UltraCompact,
    Stream,
}

impl TrainingProfile {
    pub const ALL: [Self; 6] = [
        Self::Speed,
        Self::Balanced,
        Self::Memory,
        Self::Compact,
        Self::UltraCompact,
        Self::Stream,
    ];

    pub const fn bcs2_profile(self) -> ProfileId {
        match self {
            Self::Speed => ProfileId::TRAINING_SPEED_V1,
            Self::Balanced => ProfileId::TRAINING_BALANCED_V1,
            Self::Memory => ProfileId::TRAINING_MEMORY_V1,
            Self::Compact => ProfileId::TRAINING_COMPACT_V1,
            Self::UltraCompact => ProfileId::TRAINING_ULTRA_COMPACT_V1,
            Self::Stream => ProfileId::TRAINING_STREAM_V1,
        }
    }

    pub fn from_bcs2(profile: ProfileId) -> Result<Self, TrainingError> {
        match profile {
            ProfileId::TRAINING_SPEED_V1 => Ok(Self::Speed),
            ProfileId::TRAINING_BALANCED_V1 => Ok(Self::Balanced),
            ProfileId::TRAINING_MEMORY_V1 => Ok(Self::Memory),
            ProfileId::TRAINING_COMPACT_V1 => Ok(Self::Compact),
            ProfileId::TRAINING_ULTRA_COMPACT_V1 => Ok(Self::UltraCompact),
            ProfileId::TRAINING_STREAM_V1 => Ok(Self::Stream),
            _ => Err(TrainingError::InvalidProfile),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TrainingInput {
    Dataset {
        dataset_id: ContentKey,
        spec: Box<TrainingSpec>,
    },
    Snapshot {
        snapshot_id: ContentKey,
    },
}

/// Every behavioral input to a training view is bound by ContentId.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrainingSpec {
    pub augmentation: ContentKey,
    pub authorized_purpose: String,
    pub cohort: ContentKey,
    pub feature: ContentKey,
    pub fitted_state: ContentKey,
    pub grouping: ContentKey,
    pub label: ContentKey,
    pub policy: ContentKey,
    pub preprocessing: ContentKey,
    pub sampler: ContentKey,
    pub seed: u64,
    pub split: ContentKey,
    pub view: ContentKey,
    pub window: ContentKey,
    pub allowed_adaptive_knobs: Vec<String>,
}

impl TrainingSpec {
    pub fn canonical_json(&self) -> Result<Vec<u8>, TrainingError> {
        let mut normalized = self.clone();
        normalized.allowed_adaptive_knobs = self.normalized_adaptive_knobs()?;
        validate_authorized_purpose(&normalized.authorized_purpose)?;
        canonical_json(&normalized)
    }

    pub fn content_id(&self) -> Result<ContentId, TrainingError> {
        hash_canonical(SPEC_HASH_DOMAIN, &self.canonical_json()?)
    }

    pub fn allows_adaptive_knob(&self, knob: &str) -> Result<bool, TrainingError> {
        Ok(self
            .normalized_adaptive_knobs()?
            .binary_search_by(|candidate| candidate.as_str().cmp(knob))
            .is_ok())
    }

    fn normalized_adaptive_knobs(&self) -> Result<Vec<String>, TrainingError> {
        let mut knobs = self.allowed_adaptive_knobs.clone();
        for knob in &knobs {
            validate_adaptive_knob(knob)?;
        }
        knobs.sort();
        knobs.dedup();
        Ok(knobs)
    }
}

/// Metadata for one independently addressable training row.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrainingRow {
    pub group: ContentKey,
    pub label: ContentKey,
    pub logical_bytes: u64,
    pub logical_id: ContentKey,
    pub payload: ContentKey,
    #[serde(with = "element_serde")]
    pub element: ElementType,
    pub shape: Vec<u64>,
    pub split: ContentKey,
}

impl TrainingRow {
    fn validate(&self) -> Result<(), TrainingError> {
        if self.shape.is_empty() || self.shape.contains(&0) || self.logical_bytes == 0 {
            return Err(TrainingError::InvalidRowExtent(self.logical_id.0));
        }
        if let Some(width) = self.element.byte_width() {
            let expected = self
                .shape
                .iter()
                .try_fold(width, |total, extent| total.checked_mul(*extent));
            if expected != Some(self.logical_bytes) {
                return Err(TrainingError::InvalidRowExtent(self.logical_id.0));
            }
        }
        Ok(())
    }
}

/// An immutable catalog describing a complete training snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrainingSnapshot {
    dataset_roots: Vec<ContentKey>,
    decision_log_id: ContentKey,
    profile: TrainingProfile,
    rows: Vec<TrainingRow>,
    schema: String,
    sealed: bool,
    spec_id: ContentKey,
}

impl TrainingSnapshot {
    pub fn seal(
        mut dataset_roots: Vec<ContentKey>,
        spec_id: ContentKey,
        profile: TrainingProfile,
        mut rows: Vec<TrainingRow>,
        decision_log_id: ContentKey,
    ) -> Result<Self, TrainingError> {
        dataset_roots.sort_unstable();
        if dataset_roots.is_empty() || rows.is_empty() {
            return Err(TrainingError::InvalidSnapshot);
        }
        if let Some(duplicate) = adjacent_duplicate(&dataset_roots) {
            return Err(TrainingError::DuplicateDatasetRoot(duplicate.0));
        }
        rows.sort_by_key(|row| row.logical_id);
        if let Some(duplicate) = rows
            .windows(2)
            .find(|pair| pair[0].logical_id == pair[1].logical_id)
        {
            return Err(TrainingError::DuplicateLogicalRow(
                duplicate[0].logical_id.0,
            ));
        }
        for row in &rows {
            row.validate()?;
        }
        validate_payload_metadata(&rows)?;
        let snapshot = Self {
            dataset_roots,
            decision_log_id,
            profile,
            rows,
            schema: SNAPSHOT_SCHEMA.to_owned(),
            sealed: true,
            spec_id,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, TrainingError> {
        self.validate()?;
        canonical_json(self)
    }

    pub fn content_id(&self) -> Result<ContentId, TrainingError> {
        hash_canonical(SNAPSHOT_HASH_DOMAIN, &self.canonical_json()?)
    }

    pub fn dataset_roots(&self) -> &[ContentKey] {
        &self.dataset_roots
    }

    pub const fn decision_log_id(&self) -> ContentKey {
        self.decision_log_id
    }

    pub const fn profile(&self) -> TrainingProfile {
        self.profile
    }

    pub fn rows(&self) -> &[TrainingRow] {
        &self.rows
    }

    pub const fn spec_id(&self) -> ContentKey {
        self.spec_id
    }

    pub(crate) fn from_catalog(catalog: &[u8]) -> Result<Self, TrainingError> {
        let snapshot: Self = serde_json::from_slice(catalog)?;
        snapshot.validate()?;
        if snapshot.canonical_json()? != catalog {
            return Err(TrainingError::CanonicalCatalog);
        }
        Ok(snapshot)
    }

    fn validate(&self) -> Result<(), TrainingError> {
        if self.schema != SNAPSHOT_SCHEMA || !self.sealed {
            return Err(TrainingError::NotSealed);
        }
        if self.dataset_roots.is_empty()
            || self.rows.is_empty()
            || self.dataset_roots.windows(2).any(|pair| pair[0] >= pair[1])
            || self
                .rows
                .windows(2)
                .any(|pair| pair[0].logical_id >= pair[1].logical_id)
        {
            return Err(TrainingError::InvalidSnapshot);
        }
        for row in &self.rows {
            row.validate()?;
        }
        validate_payload_metadata(&self.rows)
    }
}

pub fn encode_snapshot(
    snapshot: &TrainingSnapshot,
    frames: &[SemanticPayloadFrame<'_>],
    bounds: ResourceBounds,
) -> Result<Vec<u8>, TrainingError> {
    snapshot.validate()?;
    let expected: BTreeSet<_> = snapshot.rows.iter().map(|row| row.payload).collect();
    let mut actual = BTreeMap::new();
    for frame in frames {
        let key = ContentKey::from(frame.content_id());
        if let Some((prior_element, prior_bytes)) =
            actual.insert(key, (frame.element(), frame.bytes()))
        {
            if prior_element != frame.element() || prior_bytes != frame.bytes() {
                return Err(TrainingError::DuplicatePayload(key.0));
            }
        }
    }
    validate_frame_closure(snapshot, &expected, &actual)?;
    let catalog = snapshot.canonical_json()?;
    encode_semantic_bundle(
        snapshot.content_id()?,
        &catalog,
        snapshot.profile.bcs2_profile(),
        frames,
        bounds,
    )
    .map_err(TrainingError::from)
}

fn validate_frame_closure(
    snapshot: &TrainingSnapshot,
    expected: &BTreeSet<ContentKey>,
    actual: &BTreeMap<ContentKey, (ElementType, &[u8])>,
) -> Result<(), TrainingError> {
    if let Some(missing) = expected.iter().find(|key| !actual.contains_key(key)) {
        return Err(TrainingError::MissingPayload(missing.0));
    }
    if let Some(extra) = actual.keys().find(|key| !expected.contains(key)) {
        return Err(TrainingError::ExtraPayload(extra.0));
    }
    for row in &snapshot.rows {
        let (element, bytes) = actual
            .get(&row.payload)
            .ok_or(TrainingError::MissingPayload(row.payload.0))?;
        if *element != row.element
            || u64::try_from(bytes.len()).ok() != Some(row.logical_bytes)
            || abir::payload_content_id(*element, bytes) != row.payload.0
        {
            return Err(TrainingError::InvalidRowExtent(row.logical_id.0));
        }
    }
    Ok(())
}

fn validate_payload_metadata(rows: &[TrainingRow]) -> Result<(), TrainingError> {
    let mut payloads = BTreeMap::new();
    for row in rows {
        let metadata = (row.element, row.logical_bytes);
        if let Some(previous) = payloads.insert(row.payload, metadata) {
            if previous != metadata {
                return Err(TrainingError::DuplicatePayload(row.payload.0));
            }
        }
    }
    Ok(())
}

fn adjacent_duplicate(values: &[ContentKey]) -> Option<ContentKey> {
    values
        .windows(2)
        .find(|pair| pair[0] == pair[1])
        .map(|pair| pair[0])
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, TrainingError> {
    // serde_json's default Map is a BTreeMap. ABIR's restricted JSON domain
    // contains no floating point values, so this is RFC 8785 canonical.
    let value = serde_json::to_value(value)?;
    Ok(serde_json::to_vec(&value)?)
}

fn hash_canonical(domain: &[u8], bytes: &[u8]) -> Result<ContentId, TrainingError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(bytes);
    Ok(ContentId::from_bytes(*hasher.finalize().as_bytes()))
}

fn validate_authorized_purpose(purpose: &str) -> Result<(), TrainingError> {
    if purpose.is_empty()
        || purpose.len() > 256
        || purpose.trim() != purpose
        || purpose.chars().any(char::is_control)
    {
        return Err(TrainingError::InvalidAuthorizedPurpose);
    }
    Ok(())
}

fn validate_adaptive_knob(knob: &str) -> Result<(), TrainingError> {
    if knob.is_empty()
        || knob.len() > 128
        || !knob.as_bytes()[0].is_ascii_lowercase()
        || knob.as_bytes().iter().any(|byte| {
            !byte.is_ascii_lowercase()
                && !byte.is_ascii_digit()
                && !matches!(byte, b'-' | b'_' | b'.')
        })
    {
        return Err(TrainingError::InvalidAdaptiveKnob(knob.to_owned()));
    }
    Ok(())
}

mod element_serde {
    use super::TrainingError;
    use abir::ElementType;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(element: &ElementType, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(name(*element))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ElementType, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        parse(&value).map_err(serde::de::Error::custom)
    }

    fn name(element: ElementType) -> &'static str {
        match element {
            ElementType::I8 => "i8",
            ElementType::I16 => "i16",
            ElementType::I24 => "i24",
            ElementType::I32 => "i32",
            ElementType::I64 => "i64",
            ElementType::U8 => "u8",
            ElementType::U16 => "u16",
            ElementType::U32 => "u32",
            ElementType::U64 => "u64",
            ElementType::F16 => "f16",
            ElementType::F32 => "f32",
            ElementType::F64 => "f64",
            ElementType::Bool => "bool",
            ElementType::Utf8 => "utf8",
            ElementType::Bytes => "bytes",
        }
    }

    fn parse(value: &str) -> Result<ElementType, TrainingError> {
        match value {
            "i8" => Ok(ElementType::I8),
            "i16" => Ok(ElementType::I16),
            "i24" => Ok(ElementType::I24),
            "i32" => Ok(ElementType::I32),
            "i64" => Ok(ElementType::I64),
            "u8" => Ok(ElementType::U8),
            "u16" => Ok(ElementType::U16),
            "u32" => Ok(ElementType::U32),
            "u64" => Ok(ElementType::U64),
            "f16" => Ok(ElementType::F16),
            "f32" => Ok(ElementType::F32),
            "f64" => Ok(ElementType::F64),
            "bool" => Ok(ElementType::Bool),
            "utf8" => Ok(ElementType::Utf8),
            "bytes" => Ok(ElementType::Bytes),
            other => Err(TrainingError::InvalidElement(other.to_owned())),
        }
    }
}
