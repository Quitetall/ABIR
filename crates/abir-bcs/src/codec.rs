use crate::{
    raw_content_id, Bcs2Error, Bcs2View, FrameKind, ProfileId, ResourceBounds, RootKind,
    StorageContract,
};
use abir::{
    canonical_debug_json, interchange_content_id, logical_content_id, parse_canonical_dataset,
    ContentId,
};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const CODEC_BUNDLE_SCHEMA: &str = "org.quitetall.abir.bcs2.codec-bundle-v1";
const CODEC_BUNDLE_HASH_DOMAIN: &[u8] = b"org.quitetall.abir.bcs2.codec-bundle-v1\0";

/// A registered ABIR codec-bundle profile.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CodecProfile {
    LmlLossless,
    LmqProgressive,
}

impl CodecProfile {
    pub const fn bcs2_profile(self) -> ProfileId {
        match self {
            Self::LmlLossless => ProfileId::LML_LOSSLESS_V1,
            Self::LmqProgressive => ProfileId::LMQ_PROGRESSIVE_V1,
        }
    }

    fn from_bcs2(profile: ProfileId) -> Result<Self, CodecBundleError> {
        match profile {
            ProfileId::LML_LOSSLESS_V1 => Ok(Self::LmlLossless),
            ProfileId::LMQ_PROGRESSIVE_V1 => Ok(Self::LmqProgressive),
            _ => Err(CodecBundleError::ProfileMismatch),
        }
    }
}

/// Exact, non-floating-point codec parameter value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum CodecParameterValue {
    Boolean {
        value: bool,
    },
    Bytes {
        hex: String,
    },
    Integer {
        value: String,
    },
    Rational {
        denominator: String,
        numerator: String,
    },
    Text {
        value: String,
    },
}

/// One named codec parameter. Parameters are sealed in lexical name order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CodecParameter {
    pub name: String,
    pub value: CodecParameterValue,
}

/// Reproducible identity of the codec implementation which emitted packets.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CodecImplementation {
    pub build_id: String,
    #[serde(with = "content_id_serde")]
    pub implementation_id: ContentId,
    pub kernel_id: String,
}

/// The semantic fidelity contract bound to this bundle.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CodecFidelityKind {
    Exact,
    Bounded,
    Transformed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CodecFidelity {
    pub bound: Option<CodecParameterValue>,
    #[serde(with = "content_id_serde")]
    pub contract_id: ContentId,
    pub kind: CodecFidelityKind,
    pub metric: Option<String>,
}

/// PCCP evidence state captured at bundle construction time.
///
/// Production promotion is deliberately absent: current authorization is an
/// external ledger decision, not immutable codec content.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PccpStatus {
    Candidate,
    GatePass,
    Rejected,
}

/// Mandatory neural model provenance for `bcs.lmq.progressive.v1`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModelProvenance {
    #[serde(with = "content_id_serde")]
    pub checkpoint_content_id: ContentId,
    #[serde(with = "digest_serde")]
    pub checkpoint_sha256: [u8; 32],
    pub pccp_change_id: String,
    #[serde(with = "content_id_serde")]
    pub pccp_evidence_id: ContentId,
    pub pccp_status: PccpStatus,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct FrameBinding {
    #[serde(with = "content_id_serde")]
    content_id: ContentId,
    logical_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct PacketBinding {
    #[serde(with = "content_id_serde")]
    content_id: ContentId,
    logical_bytes: u64,
    ordinal: u32,
}

/// Immutable, canonical logical catalog for one codec packet bundle.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CodecBundleCatalog {
    fidelity: CodecFidelity,
    implementation: CodecImplementation,
    model_provenance: Option<ModelProvenance>,
    packets: Vec<PacketBinding>,
    parameters: Vec<CodecParameter>,
    profile: CodecProfile,
    schema: String,
    semantics_frame: FrameBinding,
    #[serde(with = "content_id_serde")]
    source_interchange_id: ContentId,
    #[serde(with = "content_id_serde")]
    source_semantic_id: ContentId,
}

impl CodecBundleCatalog {
    pub fn profile(&self) -> CodecProfile {
        self.profile
    }

    pub fn fidelity(&self) -> &CodecFidelity {
        &self.fidelity
    }

    pub fn implementation(&self) -> &CodecImplementation {
        &self.implementation
    }

    pub fn model_provenance(&self) -> Option<&ModelProvenance> {
        self.model_provenance.as_ref()
    }

    pub fn parameters(&self) -> &[CodecParameter] {
        &self.parameters
    }

    pub fn packet_count(&self) -> usize {
        self.packets.len()
    }

    pub fn packet_content_id(&self, ordinal: usize) -> Option<ContentId> {
        self.packets.get(ordinal).map(|packet| packet.content_id)
    }

    pub const fn source_interchange_id(&self) -> ContentId {
        self.source_interchange_id
    }

    pub const fn source_semantic_id(&self) -> ContentId {
        self.source_semantic_id
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, CodecBundleError> {
        self.validate()?;
        canonical_json(self)
    }

    pub fn content_id(&self) -> Result<ContentId, CodecBundleError> {
        let catalog = self.canonical_json()?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(CODEC_BUNDLE_HASH_DOMAIN);
        hasher.update(&catalog);
        Ok(ContentId::from_bytes(*hasher.finalize().as_bytes()))
    }

    fn from_catalog(bytes: &[u8]) -> Result<Self, CodecBundleError> {
        let catalog: Self =
            serde_json::from_slice(bytes).map_err(|_| CodecBundleError::CanonicalCatalog)?;
        catalog.validate()?;
        if canonical_json(&catalog)? != bytes {
            return Err(CodecBundleError::CanonicalCatalog);
        }
        Ok(catalog)
    }

    fn validate(&self) -> Result<(), CodecBundleError> {
        if self.schema != CODEC_BUNDLE_SCHEMA || self.packets.is_empty() {
            return Err(CodecBundleError::InvalidCatalog);
        }
        validate_identifier(&self.implementation.build_id, 256)?;
        validate_identifier(&self.implementation.kernel_id, 256)?;
        if is_zero(self.implementation.implementation_id.as_bytes())
            || is_zero(self.fidelity.contract_id.as_bytes())
            || is_zero(self.source_semantic_id.as_bytes())
            || is_zero(self.source_interchange_id.as_bytes())
        {
            return Err(CodecBundleError::IncompleteIdentity);
        }
        validate_fidelity(&self.fidelity)?;
        match (self.profile, &self.model_provenance) {
            (CodecProfile::LmlLossless, None) if self.fidelity.kind == CodecFidelityKind::Exact => {
            }
            (CodecProfile::LmlLossless, Some(_)) => {
                return Err(CodecBundleError::ModelProvenanceForbidden)
            }
            (CodecProfile::LmlLossless, None) => return Err(CodecBundleError::FidelityMismatch),
            (CodecProfile::LmqProgressive, Some(model))
                if self.fidelity.kind != CodecFidelityKind::Exact =>
            {
                validate_model_provenance(model)?;
            }
            (CodecProfile::LmqProgressive, None) => {
                return Err(CodecBundleError::ModelProvenanceRequired)
            }
            (CodecProfile::LmqProgressive, Some(_)) => {
                return Err(CodecBundleError::FidelityMismatch)
            }
        }

        if self.semantics_frame.logical_bytes == 0 {
            return Err(CodecBundleError::InvalidFrameExtent);
        }
        let mut frame_ids = BTreeSet::new();
        frame_ids.insert(self.semantics_frame.content_id);
        for (ordinal, packet) in self.packets.iter().enumerate() {
            if packet.ordinal as usize != ordinal || packet.logical_bytes == 0 {
                return Err(CodecBundleError::InvalidPacketOrder);
            }
            if !frame_ids.insert(packet.content_id) {
                return Err(CodecBundleError::DuplicateFrame(packet.content_id));
            }
        }
        if frame_ids.len() != self.packets.len() + 1 {
            return Err(CodecBundleError::InvalidCatalog);
        }

        let mut previous = None;
        for parameter in &self.parameters {
            validate_parameter(parameter)?;
            if previous.is_some_and(|name: &str| name >= parameter.name.as_str()) {
                return Err(CodecBundleError::InvalidParameter);
            }
            previous = Some(parameter.name.as_str());
        }
        Ok(())
    }
}

/// Validated borrowed view over a sealed BCS2 codec bundle.
#[derive(Debug)]
pub struct CodecBundleView<'a> {
    catalog: CodecBundleCatalog,
    frame_index: BTreeMap<ContentId, usize>,
    view: Bcs2View<'a>,
}

impl<'a> CodecBundleView<'a> {
    pub fn open(bytes: &'a [u8], bounds: ResourceBounds) -> Result<Self, CodecBundleError> {
        let view = Bcs2View::parse(bytes, 0, bounds)?;
        if view.root_kind() != RootKind::Bundle {
            return Err(CodecBundleError::NotBundle);
        }
        if view.storage_contract() != StorageContract::SealedImmutable {
            return Err(CodecBundleError::NotSealed);
        }
        if !view.references().is_empty() {
            return Err(CodecBundleError::ExternalReferencesForbidden);
        }
        let wire_profile = CodecProfile::from_bcs2(view.profile())?;
        let catalog = CodecBundleCatalog::from_catalog(view.semantic_json())?;
        if catalog.profile != wire_profile {
            return Err(CodecBundleError::ProfileMismatch);
        }
        if catalog.content_id()? != view.root_content_id() {
            return Err(CodecBundleError::RootIdentityMismatch);
        }

        let expected: BTreeSet<_> = core::iter::once(catalog.semantics_frame.content_id)
            .chain(catalog.packets.iter().map(|packet| packet.content_id))
            .collect();
        let mut frame_index = BTreeMap::new();
        for (index, frame) in view.frames().iter().enumerate() {
            if frame.kind() != FrameKind::RawBlob {
                return Err(CodecBundleError::InvalidFrameKind(frame.content_id()));
            }
            if !expected.contains(&frame.content_id()) {
                return Err(CodecBundleError::ExtraFrame(frame.content_id()));
            }
            if frame_index.insert(frame.content_id(), index).is_some() {
                return Err(CodecBundleError::DuplicateFrame(frame.content_id()));
            }
        }
        if frame_index.len() != expected.len() {
            let missing = expected
                .iter()
                .find(|content_id| !frame_index.contains_key(content_id))
                .copied()
                .ok_or(CodecBundleError::InvalidCatalog)?;
            return Err(CodecBundleError::MissingFrame(missing));
        }
        let semantics = &view.frames()[frame_index[&catalog.semantics_frame.content_id]];
        if u64::try_from(semantics.bytes().len()).ok()
            != Some(catalog.semantics_frame.logical_bytes)
        {
            return Err(CodecBundleError::InvalidFrameExtent);
        }
        verify_semantics(&catalog, semantics.bytes())?;
        for packet in &catalog.packets {
            let frame = &view.frames()[frame_index[&packet.content_id]];
            if u64::try_from(frame.bytes().len()).ok() != Some(packet.logical_bytes) {
                return Err(CodecBundleError::InvalidFrameExtent);
            }
        }
        Ok(Self {
            catalog,
            frame_index,
            view,
        })
    }

    pub fn catalog(&self) -> &CodecBundleCatalog {
        &self.catalog
    }

    pub fn canonical_semantics(&self) -> &'a [u8] {
        self.view.frames()[self.frame_index[&self.catalog.semantics_frame.content_id]].bytes()
    }

    pub fn packet(&self, ordinal: usize) -> Option<&'a [u8]> {
        let binding = self.catalog.packets.get(ordinal)?;
        Some(self.view.frames()[self.frame_index[&binding.content_id]].bytes())
    }

    pub fn packets(&self) -> impl ExactSizeIterator<Item = &'a [u8]> + '_ {
        self.catalog
            .packets
            .iter()
            .map(|binding| self.view.frames()[self.frame_index[&binding.content_id]].bytes())
    }

    pub const fn root_content_id(&self) -> ContentId {
        self.view.root_content_id()
    }
}

/// Seals canonical ABIR semantics and an ordered codec packet sequence into a
/// Bundle-root BCS2 artifact. The root identity is always recomputed here.
pub struct CodecBundleInput<'a> {
    pub canonical_semantics: &'a [u8],
    pub fidelity: CodecFidelity,
    pub implementation: CodecImplementation,
    pub model_provenance: Option<ModelProvenance>,
    pub packets: &'a [&'a [u8]],
    pub parameters: Vec<CodecParameter>,
    pub profile: CodecProfile,
}

pub fn encode_codec_bundle(
    input: CodecBundleInput<'_>,
    bounds: ResourceBounds,
) -> Result<Vec<u8>, CodecBundleError> {
    let CodecBundleInput {
        canonical_semantics,
        fidelity,
        implementation,
        model_provenance,
        packets,
        mut parameters,
        profile,
    } = input;
    if packets.is_empty() {
        return Err(CodecBundleError::InvalidCatalog);
    }
    let dataset = parse_canonical_dataset(canonical_semantics)
        .map_err(|_| CodecBundleError::InvalidCanonicalSemantics)?;
    if canonical_debug_json(&dataset).map_err(|_| CodecBundleError::SemanticEncoding)?
        != canonical_semantics
    {
        return Err(CodecBundleError::InvalidCanonicalSemantics);
    }
    parameters.sort_by(|left, right| left.name.cmp(&right.name));
    let semantics_frame = FrameBinding {
        content_id: raw_content_id(canonical_semantics),
        logical_bytes: to_u64(canonical_semantics.len())?,
    };
    let packet_bindings = packets
        .iter()
        .enumerate()
        .map(|(ordinal, bytes)| {
            Ok(PacketBinding {
                content_id: raw_content_id(bytes),
                logical_bytes: to_u64(bytes.len())?,
                ordinal: u32::try_from(ordinal).map_err(|_| CodecBundleError::BoundsExceeded)?,
            })
        })
        .collect::<Result<Vec<_>, CodecBundleError>>()?;
    let catalog = CodecBundleCatalog {
        fidelity,
        implementation,
        model_provenance,
        packets: packet_bindings,
        parameters,
        profile,
        schema: CODEC_BUNDLE_SCHEMA.to_string(),
        semantics_frame,
        source_interchange_id: interchange_content_id(&dataset)
            .map_err(|_| CodecBundleError::SemanticEncoding)?,
        source_semantic_id: logical_content_id(&dataset)
            .map_err(|_| CodecBundleError::SemanticEncoding)?,
    };
    catalog.validate()?;
    let canonical_catalog = catalog.canonical_json()?;
    let root = catalog.content_id()?;
    let frames = core::iter::once(canonical_semantics).chain(packets.iter().copied());
    let bytes = super::wire::encode_raw_root(
        RootKind::Bundle,
        profile.bcs2_profile(),
        root,
        &canonical_catalog,
        frames,
        bounds,
    )?;
    CodecBundleView::open(&bytes, bounds)?;
    Ok(bytes)
}

fn verify_semantics(catalog: &CodecBundleCatalog, bytes: &[u8]) -> Result<(), CodecBundleError> {
    let dataset =
        parse_canonical_dataset(bytes).map_err(|_| CodecBundleError::InvalidCanonicalSemantics)?;
    if canonical_debug_json(&dataset).map_err(|_| CodecBundleError::SemanticEncoding)? != bytes {
        return Err(CodecBundleError::InvalidCanonicalSemantics);
    }
    let logical = logical_content_id(&dataset).map_err(|_| CodecBundleError::SemanticEncoding)?;
    let interchange =
        interchange_content_id(&dataset).map_err(|_| CodecBundleError::SemanticEncoding)?;
    if logical != catalog.source_semantic_id || interchange != catalog.source_interchange_id {
        return Err(CodecBundleError::SourceIdentityMismatch);
    }
    Ok(())
}

fn validate_fidelity(fidelity: &CodecFidelity) -> Result<(), CodecBundleError> {
    match fidelity.kind {
        CodecFidelityKind::Exact if fidelity.metric.is_none() && fidelity.bound.is_none() => Ok(()),
        CodecFidelityKind::Bounded if fidelity.metric.is_some() && fidelity.bound.is_some() => {
            validate_identifier(fidelity.metric.as_deref().unwrap_or_default(), 256)?;
            fidelity
                .bound
                .as_ref()
                .ok_or(CodecBundleError::FidelityMismatch)
                .and_then(validate_value)
        }
        CodecFidelityKind::Transformed if fidelity.bound.is_none() => {
            if let Some(metric) = &fidelity.metric {
                validate_identifier(metric, 256)?;
            }
            Ok(())
        }
        _ => Err(CodecBundleError::FidelityMismatch),
    }
}

fn validate_model_provenance(model: &ModelProvenance) -> Result<(), CodecBundleError> {
    if is_zero(model.checkpoint_content_id.as_bytes())
        || is_zero(&model.checkpoint_sha256)
        || is_zero(model.pccp_evidence_id.as_bytes())
    {
        return Err(CodecBundleError::IncompleteModelProvenance);
    }
    validate_identifier(&model.pccp_change_id, 128)
        .map_err(|_| CodecBundleError::IncompleteModelProvenance)
}

fn validate_parameter(parameter: &CodecParameter) -> Result<(), CodecBundleError> {
    validate_name(&parameter.name)?;
    validate_value(&parameter.value)
}

fn validate_value(value: &CodecParameterValue) -> Result<(), CodecBundleError> {
    match value {
        CodecParameterValue::Boolean { .. } => Ok(()),
        CodecParameterValue::Bytes { hex } => {
            if hex.is_empty()
                || hex.len() > 8_192
                || hex.len() % 2 != 0
                || hex
                    .bytes()
                    .any(|byte| !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase())
            {
                Err(CodecBundleError::InvalidParameter)
            } else {
                Ok(())
            }
        }
        CodecParameterValue::Integer { value } => validate_integer(value),
        CodecParameterValue::Rational {
            denominator,
            numerator,
        } => {
            validate_integer(numerator)?;
            validate_integer(denominator)?;
            if denominator == "0" || denominator.starts_with('-') {
                return Err(CodecBundleError::InvalidParameter);
            }
            Ok(())
        }
        CodecParameterValue::Text { value } => validate_identifier(value, 4_096),
    }
}

fn validate_integer(value: &str) -> Result<(), CodecBundleError> {
    let digits = value.strip_prefix('-').unwrap_or(value);
    if digits.is_empty()
        || digits.len() > 1_024
        || digits.bytes().any(|byte| !byte.is_ascii_digit())
        || (digits.len() > 1 && digits.starts_with('0'))
        || value == "-0"
    {
        Err(CodecBundleError::InvalidParameter)
    } else {
        Ok(())
    }
}

fn validate_name(value: &str) -> Result<(), CodecBundleError> {
    if value.is_empty()
        || value.len() > 128
        || !value.as_bytes()[0].is_ascii_lowercase()
        || value.bytes().any(|byte| {
            !byte.is_ascii_lowercase()
                && !byte.is_ascii_digit()
                && !matches!(byte, b'-' | b'_' | b'.')
        })
    {
        Err(CodecBundleError::InvalidParameter)
    } else {
        Ok(())
    }
}

fn validate_identifier(value: &str, max_len: usize) -> Result<(), CodecBundleError> {
    if value.is_empty()
        || value.len() > max_len
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err(CodecBundleError::InvalidCatalog)
    } else {
        Ok(())
    }
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, CodecBundleError> {
    let value = serde_json::to_value(value).map_err(|_| CodecBundleError::CanonicalCatalog)?;
    serde_json::to_vec(&value).map_err(|_| CodecBundleError::CanonicalCatalog)
}

fn is_zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}

fn to_u64(value: usize) -> Result<u64, CodecBundleError> {
    u64::try_from(value).map_err(|_| CodecBundleError::BoundsExceeded)
}

#[derive(Debug, Eq, PartialEq)]
pub enum CodecBundleError {
    Bcs2(Bcs2Error),
    BoundsExceeded,
    CanonicalCatalog,
    DuplicateFrame(ContentId),
    ExternalReferencesForbidden,
    ExtraFrame(ContentId),
    FidelityMismatch,
    IncompleteIdentity,
    IncompleteModelProvenance,
    InvalidCanonicalSemantics,
    InvalidCatalog,
    InvalidFrameExtent,
    InvalidFrameKind(ContentId),
    InvalidPacketOrder,
    InvalidParameter,
    MissingFrame(ContentId),
    ModelProvenanceForbidden,
    ModelProvenanceRequired,
    NotBundle,
    NotSealed,
    ProfileMismatch,
    RootIdentityMismatch,
    SemanticEncoding,
    SourceIdentityMismatch,
}

impl From<Bcs2Error> for CodecBundleError {
    fn from(value: Bcs2Error) -> Self {
        Self::Bcs2(value)
    }
}

impl fmt::Display for CodecBundleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "BCS2 codec bundle error: {self:?}")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CodecBundleError {}

mod content_id_serde {
    use super::*;

    pub fn serialize<S>(value: &ContentId, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ContentId, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        parse_hex_32(&value)
            .map(ContentId::from_bytes)
            .map_err(serde::de::Error::custom)
    }
}

mod digest_serde {
    use super::*;

    pub fn serialize<S>(value: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex_32(value))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        parse_hex_32(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

fn parse_hex_32(value: &str) -> Result<[u8; 32], &'static str> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase())
    {
        return Err("expected 64 lowercase hexadecimal digits");
    }
    let mut output = [0_u8; 32];
    for (slot, pair) in output.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        *slot = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Ok(output)
}

fn nibble(value: u8) -> Result<u8, &'static str> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err("invalid lowercase hexadecimal digit"),
    }
}

fn hex_32(value: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in value {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use abir::{canonical_debug_json, DatasetDraft, DatasetTag, ObjectId, ValidationLimits};

    fn semantics() -> Vec<u8> {
        let dataset = DatasetDraft::new(ObjectId::<DatasetTag>::from_bytes([9; 16]))
            .validate(ValidationLimits::default())
            .expect("fixture dataset");
        canonical_debug_json(&dataset).expect("canonical semantics")
    }

    fn implementation() -> CodecImplementation {
        CodecImplementation {
            build_id: "lml-build-20260722".into(),
            implementation_id: ContentId::from_bytes([3; 32]),
            kernel_id: "lml-rust-portable-v1".into(),
        }
    }

    fn exact_fidelity() -> CodecFidelity {
        CodecFidelity {
            bound: None,
            contract_id: ContentId::from_bytes([4; 32]),
            kind: CodecFidelityKind::Exact,
            metric: None,
        }
    }

    fn encode_lml(semantics: &[u8], packets: &[&[u8]]) -> Vec<u8> {
        encode_codec_bundle(
            CodecBundleInput {
                canonical_semantics: semantics,
                fidelity: exact_fidelity(),
                implementation: implementation(),
                model_provenance: None,
                packets,
                parameters: Vec::new(),
                profile: CodecProfile::LmlLossless,
            },
            ResourceBounds::default(),
        )
        .expect("valid LML bundle")
    }

    fn lml_catalog(semantics: &[u8], packets: &[&[u8]]) -> CodecBundleCatalog {
        let dataset = parse_canonical_dataset(semantics).expect("fixture semantics");
        CodecBundleCatalog {
            fidelity: exact_fidelity(),
            implementation: implementation(),
            model_provenance: None,
            packets: packets
                .iter()
                .enumerate()
                .map(|(ordinal, packet)| PacketBinding {
                    content_id: raw_content_id(packet),
                    logical_bytes: packet.len() as u64,
                    ordinal: ordinal as u32,
                })
                .collect(),
            parameters: Vec::new(),
            profile: CodecProfile::LmlLossless,
            schema: CODEC_BUNDLE_SCHEMA.into(),
            semantics_frame: FrameBinding {
                content_id: raw_content_id(semantics),
                logical_bytes: semantics.len() as u64,
            },
            source_interchange_id: interchange_content_id(&dataset).expect("interchange id"),
            source_semantic_id: logical_content_id(&dataset).expect("semantic id"),
        }
    }

    #[test]
    fn exact_closure_rejects_extra_raw_frame() {
        let semantics = semantics();
        let packet = b"packet-0";
        let extra = b"unreachable-extra";
        let catalog = lml_catalog(&semantics, &[packet]);
        let bytes = super::super::wire::encode_raw_root(
            RootKind::Bundle,
            ProfileId::LML_LOSSLESS_V1,
            catalog.content_id().expect("root"),
            &catalog.canonical_json().expect("catalog"),
            [semantics.as_slice(), packet.as_slice(), extra.as_slice()],
            ResourceBounds::default(),
        )
        .expect("structurally valid BCS2");
        assert!(matches!(
            CodecBundleView::open(&bytes, ResourceBounds::default()),
            Err(CodecBundleError::ExtraFrame(_))
        ));
    }

    #[test]
    fn exact_closure_rejects_missing_raw_frame() {
        let semantics = semantics();
        let packet = b"packet-0";
        let catalog = lml_catalog(&semantics, &[packet]);
        let bytes = super::super::wire::encode_raw_root(
            RootKind::Bundle,
            ProfileId::LML_LOSSLESS_V1,
            catalog.content_id().expect("root"),
            &catalog.canonical_json().expect("catalog"),
            [semantics.as_slice()],
            ResourceBounds::default(),
        )
        .expect("structurally valid BCS2");
        assert!(matches!(
            CodecBundleView::open(&bytes, ResourceBounds::default()),
            Err(CodecBundleError::MissingFrame(content_id))
                if content_id == raw_content_id(packet)
        ));
    }

    #[test]
    fn profile_and_model_provenance_are_fail_closed() {
        let semantics = semantics();
        let packet = b"packet-0";
        assert_eq!(
            encode_codec_bundle(
                CodecBundleInput {
                    canonical_semantics: &semantics,
                    fidelity: CodecFidelity {
                        bound: Some(CodecParameterValue::Rational {
                            denominator: "1000".into(),
                            numerator: "1".into(),
                        }),
                        contract_id: ContentId::from_bytes([5; 32]),
                        kind: CodecFidelityKind::Bounded,
                        metric: Some("prd".into()),
                    },
                    implementation: implementation(),
                    model_provenance: None,
                    packets: &[packet],
                    parameters: Vec::new(),
                    profile: CodecProfile::LmqProgressive,
                },
                ResourceBounds::default(),
            ),
            Err(CodecBundleError::ModelProvenanceRequired)
        );
    }

    #[test]
    fn packet_order_changes_identity_and_is_recovered_from_catalog() {
        let semantics = semantics();
        let first = b"packet-a".as_slice();
        let second = b"packet-b".as_slice();
        let forward = encode_lml(&semantics, &[first, second]);
        let reverse = encode_lml(&semantics, &[second, first]);
        let forward = CodecBundleView::open(&forward, ResourceBounds::default()).unwrap();
        let reverse = CodecBundleView::open(&reverse, ResourceBounds::default()).unwrap();
        assert_ne!(forward.root_content_id(), reverse.root_content_id());
        assert_eq!(forward.packet(0), Some(first));
        assert_eq!(forward.packet(1), Some(second));
        assert_eq!(reverse.packet(0), Some(second));
        assert_eq!(reverse.packet(1), Some(first));
    }

    #[test]
    fn incompatible_wire_profile_and_incomplete_checkpoint_fail_closed() {
        let semantics = semantics();
        let packet = b"packet-0".as_slice();
        let mut lml = encode_lml(&semantics, &[packet]);
        lml[16..20].copy_from_slice(&ProfileId::LMQ_PROGRESSIVE_V1.get().to_le_bytes());
        assert!(matches!(
            CodecBundleView::open(&lml, ResourceBounds::default()),
            Err(CodecBundleError::ProfileMismatch)
        ));

        assert!(matches!(
            encode_codec_bundle(
                CodecBundleInput {
                    canonical_semantics: &semantics,
                    fidelity: CodecFidelity {
                        bound: None,
                        contract_id: ContentId::from_bytes([6; 32]),
                        kind: CodecFidelityKind::Transformed,
                        metric: Some("latent-preservation".into()),
                    },
                    implementation: implementation(),
                    model_provenance: Some(ModelProvenance {
                        checkpoint_content_id: ContentId::from_bytes([7; 32]),
                        checkpoint_sha256: [0; 32],
                        pccp_change_id: "candidate-7".into(),
                        pccp_evidence_id: ContentId::from_bytes([8; 32]),
                        pccp_status: PccpStatus::Candidate,
                    }),
                    packets: &[packet],
                    parameters: Vec::new(),
                    profile: CodecProfile::LmqProgressive,
                },
                ResourceBounds::default(),
            ),
            Err(CodecBundleError::IncompleteModelProvenance)
        ));
    }

    #[test]
    fn generic_bundle_encoder_cannot_accept_caller_selected_codec_root() {
        assert_eq!(
            crate::encode_semantic_bundle(
                ContentId::from_bytes([0x7f; 32]),
                br#"{"schema":"caller-selected"}"#,
                ProfileId::LML_LOSSLESS_V1,
                &[],
                ResourceBounds::default(),
            ),
            Err(Bcs2Error::ProfileRootMismatch)
        );
    }
}
