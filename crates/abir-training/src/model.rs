use crate::TrainingError;
use abir::{ByteOrder, ContentId, ElementType, Presence};
use abir_bcs::{encode_semantic_bundle, ProfileId, ResourceBounds, SemanticPayloadFrame};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

const SPEC_HASH_DOMAIN: &[u8] = b"org.quitetall.abir.training.spec-v1\0";
const SNAPSHOT_V1_HASH_DOMAIN: &[u8] = b"org.quitetall.abir.training.snapshot-v1\0";
const SNAPSHOT_V2_HASH_DOMAIN: &[u8] = b"org.quitetall.abir.training.snapshot-v2\0";
const SNAPSHOT_V1_SCHEMA: &str = "org.quitetall.abir.training.snapshot-v1";
const SNAPSHOT_V2_SCHEMA: &str = "org.quitetall.abir.training.snapshot-v2";

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
    #[serde(with = "byte_order_serde")]
    pub byte_order: ByteOrder,
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
        validate_extent(
            self.element,
            self.byte_order,
            &self.shape,
            self.logical_bytes,
            self.logical_id.0,
        )
    }
}

/// A typed payload associated with a training label for one logical row.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrainingAssociatedPayload {
    #[serde(with = "byte_order_serde")]
    pub byte_order: ByteOrder,
    #[serde(with = "element_serde")]
    pub element: ElementType,
    pub logical_bytes: u64,
    pub payload: ContentKey,
    pub shape: Vec<u64>,
}

impl TrainingAssociatedPayload {
    fn validate(&self, logical_id: ContentId) -> Result<(), TrainingError> {
        validate_extent(
            self.element,
            self.byte_order,
            &self.shape,
            self.logical_bytes,
            logical_id,
        )
    }
}

/// An explicit semantic association between a logical row and label data.
///
/// `Present` requires a payload descriptor. Every other ABIR presence state
/// forbids one: absence, uncertainty, withholding, redaction, and
/// non-applicability never silently become an empty or all-zero label.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrainingLabelPayloadAssociation {
    pub concept: String,
    pub logical_id: ContentKey,
    pub payload: Option<TrainingAssociatedPayload>,
    #[serde(with = "presence_serde")]
    pub presence: Presence,
}

impl TrainingLabelPayloadAssociation {
    fn validate(&self, rows: &[TrainingRow]) -> Result<(), TrainingError> {
        validate_label_concept(&self.concept)?;
        if rows
            .binary_search_by_key(&self.logical_id, |row| row.logical_id)
            .is_err()
        {
            return Err(TrainingError::UnknownLabelRow(self.logical_id.0));
        }
        match (self.presence, &self.payload) {
            (Presence::Present, Some(payload)) => payload.validate(self.logical_id.0),
            (Presence::Present, None) => {
                Err(TrainingError::InvalidLabelPresence(self.logical_id.0))
            }
            (_, None) => Ok(()),
            (_, Some(_)) => Err(TrainingError::InvalidLabelPresence(self.logical_id.0)),
        }
    }
}

/// An immutable catalog describing a complete training snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrainingSnapshot {
    dataset_roots: Vec<ContentKey>,
    decision_log_id: ContentKey,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    label_payloads: Vec<TrainingLabelPayloadAssociation>,
    profile: TrainingProfile,
    rows: Vec<TrainingRow>,
    schema: String,
    sealed: bool,
    spec_id: ContentKey,
}

impl TrainingSnapshot {
    pub fn seal(
        dataset_roots: Vec<ContentKey>,
        spec_id: ContentKey,
        profile: TrainingProfile,
        rows: Vec<TrainingRow>,
        decision_log_id: ContentKey,
    ) -> Result<Self, TrainingError> {
        Self::seal_with_label_payloads(
            dataset_roots,
            spec_id,
            profile,
            rows,
            Vec::new(),
            decision_log_id,
        )
    }

    pub fn seal_with_label_payloads(
        mut dataset_roots: Vec<ContentKey>,
        spec_id: ContentKey,
        profile: TrainingProfile,
        mut rows: Vec<TrainingRow>,
        mut label_payloads: Vec<TrainingLabelPayloadAssociation>,
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
        label_payloads.sort_by(|left, right| {
            (left.logical_id, left.concept.as_str())
                .cmp(&(right.logical_id, right.concept.as_str()))
        });
        if let Some(duplicate) = label_payloads.windows(2).find(|pair| {
            pair[0].logical_id == pair[1].logical_id && pair[0].concept == pair[1].concept
        }) {
            return Err(TrainingError::DuplicateLabelAssociation {
                logical_id: duplicate[0].logical_id.0,
                concept: duplicate[0].concept.clone(),
            });
        }
        for association in &label_payloads {
            association.validate(&rows)?;
        }
        validate_payload_metadata(&rows, &label_payloads)?;
        let snapshot = Self {
            dataset_roots,
            decision_log_id,
            schema: if label_payloads.is_empty() {
                SNAPSHOT_V1_SCHEMA.to_owned()
            } else {
                SNAPSHOT_V2_SCHEMA.to_owned()
            },
            label_payloads,
            profile,
            rows,
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
        let domain = match self.schema.as_str() {
            SNAPSHOT_V1_SCHEMA => SNAPSHOT_V1_HASH_DOMAIN,
            SNAPSHOT_V2_SCHEMA => SNAPSHOT_V2_HASH_DOMAIN,
            _ => return Err(TrainingError::InvalidSnapshot),
        };
        hash_canonical(domain, &self.canonical_json()?)
    }

    pub fn dataset_roots(&self) -> &[ContentKey] {
        &self.dataset_roots
    }

    pub const fn decision_log_id(&self) -> ContentKey {
        self.decision_log_id
    }

    pub fn label_payloads(&self) -> &[TrainingLabelPayloadAssociation] {
        &self.label_payloads
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
        if !self.sealed {
            return Err(TrainingError::NotSealed);
        }
        match self.schema.as_str() {
            SNAPSHOT_V1_SCHEMA if self.label_payloads.is_empty() => {}
            SNAPSHOT_V2_SCHEMA if !self.label_payloads.is_empty() => {}
            _ => return Err(TrainingError::InvalidSnapshot),
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
        if self.label_payloads.windows(2).any(|pair| {
            (pair[0].logical_id, pair[0].concept.as_str())
                >= (pair[1].logical_id, pair[1].concept.as_str())
        }) {
            return Err(TrainingError::InvalidSnapshot);
        }
        for association in &self.label_payloads {
            association.validate(&self.rows)?;
        }
        validate_payload_metadata(&self.rows, &self.label_payloads)
    }
}

pub fn encode_snapshot(
    snapshot: &TrainingSnapshot,
    frames: &[SemanticPayloadFrame<'_>],
    bounds: ResourceBounds,
) -> Result<Vec<u8>, TrainingError> {
    snapshot.validate()?;
    let expected = expected_payloads(snapshot);
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
    for association in &snapshot.label_payloads {
        let Some(payload) = &association.payload else {
            continue;
        };
        let (element, bytes) = actual
            .get(&payload.payload)
            .ok_or(TrainingError::MissingPayload(payload.payload.0))?;
        if *element != payload.element
            || u64::try_from(bytes.len()).ok() != Some(payload.logical_bytes)
            || abir::payload_content_id(*element, bytes) != payload.payload.0
        {
            return Err(TrainingError::InvalidRowExtent(association.logical_id.0));
        }
    }
    Ok(())
}

pub(crate) fn expected_payloads(snapshot: &TrainingSnapshot) -> BTreeSet<ContentKey> {
    snapshot
        .rows
        .iter()
        .map(|row| row.payload)
        .chain(
            snapshot
                .label_payloads
                .iter()
                .filter_map(|association| association.payload.as_ref().map(|value| value.payload)),
        )
        .collect()
}

fn validate_payload_metadata(
    rows: &[TrainingRow],
    label_payloads: &[TrainingLabelPayloadAssociation],
) -> Result<(), TrainingError> {
    let mut payloads = BTreeMap::new();
    for row in rows {
        let metadata = (row.element, row.byte_order, row.logical_bytes);
        if let Some(previous) = payloads.insert(row.payload, metadata) {
            if previous != metadata {
                return Err(TrainingError::DuplicatePayload(row.payload.0));
            }
        }
    }
    for association in label_payloads {
        let Some(payload) = &association.payload else {
            continue;
        };
        let metadata = (payload.element, payload.byte_order, payload.logical_bytes);
        if let Some(previous) = payloads.insert(payload.payload, metadata) {
            if previous != metadata {
                return Err(TrainingError::DuplicatePayload(payload.payload.0));
            }
        }
    }
    Ok(())
}

fn validate_extent(
    element: ElementType,
    byte_order: ByteOrder,
    shape: &[u64],
    logical_bytes: u64,
    logical_id: ContentId,
) -> Result<(), TrainingError> {
    if shape.is_empty() || shape.contains(&0) || logical_bytes == 0 {
        return Err(TrainingError::InvalidRowExtent(logical_id));
    }
    match element.byte_width() {
        Some(width) if width > 1 && byte_order == ByteOrder::NotApplicable => {
            return Err(TrainingError::InvalidByteOrder(format!(
                "{element:?} requires explicit little or big endian order"
            )));
        }
        Some(1) | None if byte_order != ByteOrder::NotApplicable => {
            return Err(TrainingError::InvalidByteOrder(format!(
                "{element:?} requires not-applicable byte order"
            )));
        }
        _ => {}
    }
    if let Some(width) = element.byte_width() {
        let expected = shape
            .iter()
            .try_fold(width, |total, extent| total.checked_mul(*extent));
        if expected != Some(logical_bytes) {
            return Err(TrainingError::InvalidRowExtent(logical_id));
        }
    }
    Ok(())
}

fn validate_label_concept(concept: &str) -> Result<(), TrainingError> {
    if concept.len() < 3
        || concept.len() > 256
        || concept.trim() != concept
        || !concept.contains('.')
        || !concept.as_bytes()[0].is_ascii_lowercase() && !concept.as_bytes()[0].is_ascii_digit()
        || concept.ends_with('.')
        || concept.as_bytes().iter().any(|byte| {
            !byte.is_ascii_lowercase()
                && !byte.is_ascii_digit()
                && !matches!(byte, b'.' | b'-' | b'_')
        })
    {
        return Err(TrainingError::InvalidLabelConcept(concept.to_owned()));
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

mod byte_order_serde {
    use super::TrainingError;
    use abir::ByteOrder;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(byte_order: &ByteOrder, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match byte_order {
            ByteOrder::Little => "little",
            ByteOrder::Big => "big",
            ByteOrder::NotApplicable => "not-applicable",
        })
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ByteOrder, D::Error>
    where
        D: Deserializer<'de>,
    {
        match String::deserialize(deserializer)?.as_str() {
            "little" => Ok(ByteOrder::Little),
            "big" => Ok(ByteOrder::Big),
            "not-applicable" => Ok(ByteOrder::NotApplicable),
            other => Err(serde::de::Error::custom(TrainingError::InvalidByteOrder(
                other.to_owned(),
            ))),
        }
    }
}

mod presence_serde {
    use super::TrainingError;
    use abir::Presence;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(presence: &Presence, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match presence {
            Presence::Present => "present",
            Presence::AbsentAtSource => "absent-at-source",
            Presence::UnknownAtSource => "unknown-at-source",
            Presence::Withheld => "withheld",
            Presence::Redacted => "redacted",
            Presence::NotApplicable => "not-applicable",
        })
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Presence, D::Error>
    where
        D: Deserializer<'de>,
    {
        match String::deserialize(deserializer)?.as_str() {
            "present" => Ok(Presence::Present),
            "absent-at-source" => Ok(Presence::AbsentAtSource),
            "unknown-at-source" => Ok(Presence::UnknownAtSource),
            "withheld" => Ok(Presence::Withheld),
            "redacted" => Ok(Presence::Redacted),
            "not-applicable" => Ok(Presence::NotApplicable),
            other => Err(serde::de::Error::custom(
                TrainingError::InvalidLabelPresenceName(other.to_owned()),
            )),
        }
    }
}
