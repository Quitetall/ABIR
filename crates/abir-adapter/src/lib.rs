//! Capability-declared foreign-standard Adapter interface.
//!
//! Adapters translate foreign semantics into validated ABIR; they never own
//! ABIR invariants. Exact source preservation and semantic mapping are separate
//! claims. A forensic-only import is useful and honest, but is never reported as
//! first-class semantic support.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use abir::{
    AbirDataset, ContentId, DatasetDraft, DatasetTag, ObjectId, SourceCapsule, SourceKey,
    ValidationLimits,
};
use serde::{Deserialize, Serialize};

pub const ADAPTER_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct ProfileId(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterProfile {
    pub id: ProfileId,
    pub standard: String,
    pub edition: String,
    pub media_types: Vec<String>,
    pub status: ProfileStatus,
    #[serde(rename = "validator")]
    pub required_validator: String,
    pub capabilities: BTreeSet<AdapterCapability>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileStatus {
    Forensic,
    Semantic,
    Stream,
    Hardware,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterProfileRegistry {
    pub schema_version: u32,
    pub profiles: Vec<AdapterProfile>,
}

impl AdapterProfileRegistry {
    pub fn parse_json(bytes: &[u8]) -> Result<Self, AdapterError> {
        let registry: Self = serde_json::from_slice(bytes)
            .map_err(|error| AdapterError::InvalidSource(error.to_string()))?;
        if registry.schema_version != 1 || registry.profiles.is_empty() {
            return Err(AdapterError::InvalidSource(
                "unsupported or empty Adapter profile registry".to_owned(),
            ));
        }
        let mut ids = BTreeSet::new();
        for profile in &registry.profiles {
            if profile.id.0.is_empty()
                || profile.standard.is_empty()
                || profile.edition.is_empty()
                || profile.media_types.is_empty()
                || profile
                    .media_types
                    .iter()
                    .any(|media_type| !valid_media_type(media_type))
                || profile.required_validator.is_empty()
                || profile.capabilities.is_empty()
                || !ids.insert(profile.id.clone())
            {
                return Err(AdapterError::InvalidSource(
                    "invalid or duplicate Adapter profile".to_owned(),
                ));
            }
            let has_core = [
                AdapterCapability::Inspect,
                AdapterCapability::Import,
                AdapterCapability::PlanExport,
                AdapterCapability::Export,
                AdapterCapability::Validate,
            ]
            .into_iter()
            .all(|capability| profile.capabilities.contains(&capability));
            if !has_core
                || (profile.status == ProfileStatus::Stream
                    && !profile.capabilities.contains(&AdapterCapability::Stream))
                || (profile.status == ProfileStatus::Hardware
                    && (!profile.capabilities.contains(&AdapterCapability::Stream)
                        || !profile.capabilities.contains(&AdapterCapability::Hardware)))
            {
                return Err(AdapterError::InvalidSource(format!(
                    "Adapter profile {} has capabilities inconsistent with status",
                    profile.id.0
                )));
            }
        }
        Ok(registry)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdapterCapability {
    Inspect,
    Import,
    PlanExport,
    Export,
    Validate,
    Stream,
    Hardware,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForeignEntry {
    pub path: String,
    pub media_type: Option<String>,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForeignObject {
    pub profile: ProfileId,
    pub entries: Vec<ForeignEntry>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SemanticCoverage {
    ExactSemantic,
    ProjectedSemantic,
    ForensicOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MappingDisposition {
    Exact,
    Projected,
    Lossy,
    Quarantined,
    Unsupported,
    UserDecision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MappingEntry {
    pub source_path: String,
    pub target: String,
    pub disposition: MappingDisposition,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MappingReport {
    pub source_profile: ProfileId,
    pub target_profile: ProfileId,
    pub semantic_coverage: SemanticCoverage,
    pub entries: Vec<MappingEntry>,
    pub preserved_unknowns: u64,
    pub sample_values_changed: bool,
    pub timing_changed: bool,
}

impl MappingReport {
    pub fn first_class_semantic(&self) -> bool {
        matches!(self.semantic_coverage, SemanticCoverage::ExactSemantic)
            && !self.sample_values_changed
            && !self.timing_changed
            && self.entries.iter().all(|entry| {
                matches!(
                    entry.disposition,
                    MappingDisposition::Exact | MappingDisposition::Quarantined
                )
            })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InspectReport {
    pub profile: ProfileId,
    pub entry_count: usize,
    pub logical_bytes: u64,
    pub risks: Vec<String>,
    pub required_resources: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayloadObject {
    pub content_id: ContentId,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct ImportOutcome {
    pub dataset: AbirDataset,
    pub report: MappingReport,
    pub payloads: Vec<PayloadObject>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportPlan {
    pub source_dataset: String,
    pub target_profile: ProfileId,
    pub mappings: Vec<MappingEntry>,
    pub requires_user_acceptance: bool,
    pub unsupported: bool,
    pub plan_id: String,
}

impl ExportPlan {
    pub fn accepts_without_loss(&self) -> bool {
        !self.requires_user_acceptance
            && !self.unsupported
            && self.mappings.iter().all(|mapping| {
                matches!(
                    mapping.disposition,
                    MappingDisposition::Exact | MappingDisposition::Quarantined
                )
            })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FidelityReceipt {
    pub plan_id: String,
    pub exact_source_restoration: bool,
    pub semantic_equivalence: bool,
    pub output_content_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ValidationArtifact {
    pub profile: ProfileId,
    pub internal_valid: bool,
    pub independent_validator: String,
    pub independent_valid: Option<bool>,
    pub diagnostics: Vec<String>,
}

pub trait PayloadResolver {
    fn resolve(&self, content_id: ContentId) -> Result<Vec<u8>, AdapterError>;
}

pub trait Adapter {
    fn profile(&self) -> &AdapterProfile;
    fn inspect(&self, source: &ForeignObject) -> Result<InspectReport, AdapterError>;
    fn import(
        &self,
        source: &ForeignObject,
        limits: ValidationLimits,
    ) -> Result<ImportOutcome, AdapterError>;
    fn plan_export(&self, dataset: &AbirDataset) -> Result<ExportPlan, AdapterError>;
    fn export(
        &self,
        dataset: &AbirDataset,
        plan: &ExportPlan,
        payloads: &dyn PayloadResolver,
    ) -> Result<(ForeignObject, FidelityReceipt), AdapterError>;
    fn validate(&self, source: &ForeignObject) -> ValidationArtifact;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdapterError {
    ProfileMismatch {
        expected: ProfileId,
        actual: ProfileId,
    },
    EmptySource,
    DuplicatePath(String),
    InvalidPath(String),
    SourceTooLarge,
    InvalidSource(String),
    AbirValidation,
    MissingPayload(ContentId),
    ExportPlanMismatch,
    ExportRequiresAcceptance,
    UnsupportedMeaning(String),
    AdapterUnavailable {
        package: String,
        capability: String,
    },
}

impl fmt::Display for AdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProfileMismatch { expected, actual } => write!(
                formatter,
                "Adapter profile mismatch: expected {}, got {}",
                expected.0, actual.0
            ),
            Self::EmptySource => formatter.write_str("foreign source contains no entries"),
            Self::DuplicatePath(path) => write!(formatter, "duplicate source path: {path}"),
            Self::InvalidPath(path) => write!(formatter, "invalid source path: {path}"),
            Self::SourceTooLarge => formatter.write_str("foreign source exceeds declared limits"),
            Self::InvalidSource(reason) => write!(formatter, "invalid foreign source: {reason}"),
            Self::AbirValidation => formatter.write_str("imported ABIR root did not validate"),
            Self::MissingPayload(id) => write!(formatter, "missing or corrupt payload: {id}"),
            Self::ExportPlanMismatch => formatter.write_str("export plan does not match dataset"),
            Self::ExportRequiresAcceptance => {
                formatter.write_str("export requires explicit loss acceptance")
            }
            Self::UnsupportedMeaning(reason) => {
                write!(
                    formatter,
                    "target cannot represent required meaning: {reason}"
                )
            }
            Self::AdapterUnavailable {
                package,
                capability,
            } => write!(
                formatter,
                "Adapter capability {capability} is unavailable; install package {package}"
            ),
        }
    }
}

impl std::error::Error for AdapterError {}

#[derive(Default)]
pub struct AdapterRegistry {
    adapters: BTreeMap<ProfileId, Box<dyn Adapter + Send + Sync>>,
    providers: BTreeMap<ProfileId, String>,
}

impl AdapterRegistry {
    pub fn register(
        &mut self,
        adapter: impl Adapter + Send + Sync + 'static,
    ) -> Result<(), AdapterError> {
        let id = adapter.profile().id.clone();
        if self.adapters.contains_key(&id) {
            return Err(AdapterError::InvalidSource(format!(
                "duplicate adapter profile {}",
                id.0
            )));
        }
        self.adapters.insert(id, Box::new(adapter));
        Ok(())
    }

    pub fn declare_provider(
        &mut self,
        id: ProfileId,
        package: impl Into<String>,
    ) -> Result<(), AdapterError> {
        if self.providers.contains_key(&id) {
            return Err(AdapterError::InvalidSource(format!(
                "duplicate adapter provider {}",
                id.0
            )));
        }
        self.providers.insert(id, package.into());
        Ok(())
    }

    pub fn get(&self, id: &ProfileId) -> Result<&(dyn Adapter + Send + Sync), AdapterError> {
        self.adapters
            .get(id)
            .map(Box::as_ref)
            .ok_or_else(|| AdapterError::AdapterUnavailable {
                package: self
                    .providers
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| "unregistered-adapter".to_owned()),
                capability: id.0.clone(),
            })
    }
}

/// Exact source-capsule preservation. This deliberately reports
/// `ForensicOnly`; a standard becomes first-class only in its semantic Adapter.
pub struct ForensicAdapter {
    profile: AdapterProfile,
    max_source_bytes: u64,
}

impl ForensicAdapter {
    pub fn new(profile: AdapterProfile, max_source_bytes: u64) -> Self {
        Self {
            profile,
            max_source_bytes,
        }
    }

    fn check(&self, source: &ForeignObject) -> Result<(), AdapterError> {
        if source.profile != self.profile.id {
            return Err(AdapterError::ProfileMismatch {
                expected: self.profile.id.clone(),
                actual: source.profile.clone(),
            });
        }
        if source.entries.is_empty() {
            return Err(AdapterError::EmptySource);
        }
        let mut paths = BTreeSet::new();
        let mut total = 0u64;
        for entry in &source.entries {
            if entry.path.is_empty()
                || entry.path.starts_with('/')
                || entry.path.contains('\\')
                || entry.path.chars().any(char::is_control)
                || entry
                    .path
                    .split('/')
                    .any(|part| part.is_empty() || part == "..")
            {
                return Err(AdapterError::InvalidPath(entry.path.clone()));
            }
            if !paths.insert(entry.path.clone()) {
                return Err(AdapterError::DuplicatePath(entry.path.clone()));
            }
            total = total
                .checked_add(entry.bytes.len() as u64)
                .ok_or(AdapterError::SourceTooLarge)?;
        }
        if total > self.max_source_bytes {
            return Err(AdapterError::SourceTooLarge);
        }
        Ok(())
    }
}

impl Adapter for ForensicAdapter {
    fn profile(&self) -> &AdapterProfile {
        &self.profile
    }

    fn inspect(&self, source: &ForeignObject) -> Result<InspectReport, AdapterError> {
        self.check(source)?;
        let logical_bytes = source
            .entries
            .iter()
            .try_fold(0u64, |sum, entry| sum.checked_add(entry.bytes.len() as u64));
        Ok(InspectReport {
            profile: self.profile.id.clone(),
            entry_count: source.entries.len(),
            logical_bytes: logical_bytes.ok_or(AdapterError::SourceTooLarge)?,
            risks: vec!["semantic mapping not installed; source preserved exactly".to_owned()],
            required_resources: BTreeMap::from([(
                "max-source-bytes".to_owned(),
                self.max_source_bytes,
            )]),
        })
    }

    fn import(
        &self,
        source: &ForeignObject,
        limits: ValidationLimits,
    ) -> Result<ImportOutcome, AdapterError> {
        self.check(source)?;
        let root_hash = hash_foreign_object(source);
        let mut dataset_bytes = [0u8; 16];
        dataset_bytes.copy_from_slice(&root_hash[..16]);
        let mut draft = DatasetDraft::new(ObjectId::<DatasetTag>::from_bytes(dataset_bytes));
        let mut payloads = Vec::with_capacity(source.entries.len());
        let mut entries = Vec::with_capacity(source.entries.len());
        for entry in &source.entries {
            let content_id = content_id(&entry.bytes);
            let key = SourceKey::new(format!("adapter.{}", self.profile.id.0), &entry.path)
                .map_err(|_| AdapterError::InvalidPath(entry.path.clone()))?;
            draft.add_source_capsule(SourceCapsule::new(
                key,
                content_id,
                entry.media_type.as_deref(),
            ));
            payloads.push(PayloadObject {
                content_id,
                bytes: entry.bytes.clone(),
            });
            entries.push(MappingEntry {
                source_path: entry.path.clone(),
                target: format!("source-capsule:{content_id}"),
                disposition: MappingDisposition::Quarantined,
                reason: Some("exact bytes preserved; semantic mapper absent".to_owned()),
            });
        }
        let dataset = draft
            .validate(limits)
            .map_err(|_| AdapterError::AbirValidation)?;
        Ok(ImportOutcome {
            dataset,
            report: MappingReport {
                source_profile: self.profile.id.clone(),
                target_profile: ProfileId("abir.semantic.v1".to_owned()),
                semantic_coverage: SemanticCoverage::ForensicOnly,
                entries,
                preserved_unknowns: source.entries.len() as u64,
                sample_values_changed: false,
                timing_changed: false,
            },
            payloads,
        })
    }

    fn plan_export(&self, dataset: &AbirDataset) -> Result<ExportPlan, AdapterError> {
        let prefix = format!("adapter.{}", self.profile.id.0);
        let matching: Vec<_> = dataset
            .source_capsules()
            .iter()
            .filter(|capsule| capsule.source().namespace() == prefix)
            .collect();
        let unsupported = matching.is_empty();
        let mappings = matching
            .into_iter()
            .map(|capsule| MappingEntry {
                source_path: capsule.source().value().to_owned(),
                target: capsule.source().value().to_owned(),
                disposition: MappingDisposition::Exact,
                reason: None,
            })
            .collect::<Vec<_>>();
        let mut plan = ExportPlan {
            source_dataset: dataset.id().to_string(),
            target_profile: self.profile.id.clone(),
            mappings,
            requires_user_acceptance: false,
            unsupported,
            plan_id: String::new(),
        };
        plan.plan_id = export_plan_id(&plan);
        Ok(plan)
    }

    fn export(
        &self,
        dataset: &AbirDataset,
        plan: &ExportPlan,
        payloads: &dyn PayloadResolver,
    ) -> Result<(ForeignObject, FidelityReceipt), AdapterError> {
        let expected = self.plan_export(dataset)?;
        if expected != *plan || export_plan_id(plan) != plan.plan_id {
            return Err(AdapterError::ExportPlanMismatch);
        }
        if !plan.accepts_without_loss() {
            return Err(if plan.unsupported {
                AdapterError::UnsupportedMeaning("no matching source capsules".to_owned())
            } else {
                AdapterError::ExportRequiresAcceptance
            });
        }
        let prefix = format!("adapter.{}", self.profile.id.0);
        let mut entries = Vec::new();
        let mut output_ids = Vec::new();
        for capsule in dataset
            .source_capsules()
            .iter()
            .filter(|capsule| capsule.source().namespace() == prefix)
        {
            let bytes = payloads.resolve(capsule.content_id())?;
            if content_id(&bytes) != capsule.content_id() {
                return Err(AdapterError::MissingPayload(capsule.content_id()));
            }
            output_ids.push(capsule.content_id().to_string());
            entries.push(ForeignEntry {
                path: capsule.source().value().to_owned(),
                media_type: capsule.media_type().map(str::to_owned),
                bytes,
            });
        }
        Ok((
            ForeignObject {
                profile: self.profile.id.clone(),
                entries,
            },
            FidelityReceipt {
                plan_id: plan.plan_id.clone(),
                exact_source_restoration: true,
                semantic_equivalence: false,
                output_content_ids: output_ids,
            },
        ))
    }

    fn validate(&self, source: &ForeignObject) -> ValidationArtifact {
        let result = self.check(source);
        ValidationArtifact {
            profile: self.profile.id.clone(),
            internal_valid: result.is_ok(),
            independent_validator: self.profile.required_validator.clone(),
            independent_valid: None,
            diagnostics: result
                .err()
                .map(|error| error.to_string())
                .into_iter()
                .collect(),
        }
    }
}

fn content_id(bytes: &[u8]) -> ContentId {
    let mut hasher = blake3::Hasher::new_derive_key("abir.adapter.payload.v1");
    hasher.update(bytes);
    ContentId::from_bytes(*hasher.finalize().as_bytes())
}

fn hash_foreign_object(source: &ForeignObject) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key("abir.adapter.foreign-object.v1");
    hasher.update(&(source.profile.0.len() as u64).to_le_bytes());
    hasher.update(source.profile.0.as_bytes());
    for entry in &source.entries {
        hasher.update(&(entry.path.len() as u64).to_le_bytes());
        hasher.update(entry.path.as_bytes());
        match &entry.media_type {
            Some(media_type) => {
                hasher.update(&[1]);
                hasher.update(&(media_type.len() as u64).to_le_bytes());
                hasher.update(media_type.as_bytes());
            }
            None => {
                hasher.update(&[0]);
            }
        }
        hasher.update(content_id(&entry.bytes).as_bytes());
    }
    *hasher.finalize().as_bytes()
}

fn valid_media_type(media_type: &str) -> bool {
    let Some((kind, subtype)) = media_type.split_once('/') else {
        return false;
    };
    !kind.is_empty()
        && !subtype.is_empty()
        && !media_type.chars().any(char::is_whitespace)
        && media_type.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

fn export_plan_id(plan: &ExportPlan) -> String {
    let mut normalized = plan.clone();
    normalized.plan_id.clear();
    let bytes = serde_json::to_vec(&normalized).expect("serializable export plan");
    let mut hasher = blake3::Hasher::new_derive_key("abir.adapter.export-plan.v1");
    hasher.update(&bytes);
    hasher.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> AdapterProfile {
        AdapterProfile {
            id: ProfileId("edfplus.1".to_owned()),
            standard: "EDF+".to_owned(),
            edition: "1".to_owned(),
            media_types: vec!["application/edf".to_owned()],
            status: ProfileStatus::Forensic,
            required_validator: "edfbrowser".to_owned(),
            capabilities: BTreeSet::from([
                AdapterCapability::Inspect,
                AdapterCapability::Import,
                AdapterCapability::PlanExport,
                AdapterCapability::Export,
                AdapterCapability::Validate,
            ]),
        }
    }

    fn source() -> ForeignObject {
        ForeignObject {
            profile: ProfileId("edfplus.1".to_owned()),
            entries: vec![ForeignEntry {
                path: "recording.edf".to_owned(),
                media_type: Some("application/edf".to_owned()),
                bytes: b"exact source bytes".to_vec(),
            }],
        }
    }

    struct Resolver(BTreeMap<ContentId, Vec<u8>>);

    impl PayloadResolver for Resolver {
        fn resolve(&self, id: ContentId) -> Result<Vec<u8>, AdapterError> {
            self.0
                .get(&id)
                .cloned()
                .ok_or(AdapterError::MissingPayload(id))
        }
    }

    #[test]
    fn forensic_round_trip_is_exact_but_never_semantic() {
        let adapter = ForensicAdapter::new(profile(), 1024);
        let imported = adapter
            .import(&source(), ValidationLimits::default())
            .unwrap();
        // Parser coverage and exact source restoration are not semantic
        // promotion. All profiles remain forensic/stream/hardware until their
        // complete mapping and independent conformance evidence are accepted.
        assert_eq!(
            imported.report.semantic_coverage,
            SemanticCoverage::ForensicOnly
        );
        assert!(!imported.report.first_class_semantic());
        let resolver = Resolver(
            imported
                .payloads
                .iter()
                .map(|payload| (payload.content_id, payload.bytes.clone()))
                .collect(),
        );
        let plan = adapter.plan_export(&imported.dataset).unwrap();
        let (restored, receipt) = adapter.export(&imported.dataset, &plan, &resolver).unwrap();
        assert_eq!(restored, source());
        assert!(receipt.exact_source_restoration);
        assert!(!receipt.semantic_equivalence);
    }

    #[test]
    fn export_fails_before_resolving_unrepresentable_dataset() {
        let adapter = ForensicAdapter::new(profile(), 1024);
        let dataset = DatasetDraft::new(ObjectId::from_bytes([9; 16]))
            .validate(ValidationLimits::default())
            .unwrap();
        let plan = adapter.plan_export(&dataset).unwrap();
        assert!(plan.unsupported);
        assert!(matches!(
            adapter.export(&dataset, &plan, &Resolver(BTreeMap::new())),
            Err(AdapterError::UnsupportedMeaning(_))
        ));
    }

    #[test]
    fn duplicate_paths_and_traversal_fail_closed() {
        let adapter = ForensicAdapter::new(profile(), 1024);
        let mut duplicate = source();
        duplicate.entries.push(duplicate.entries[0].clone());
        assert!(matches!(
            adapter.inspect(&duplicate),
            Err(AdapterError::DuplicatePath(_))
        ));
        let mut traversal = source();
        traversal.entries[0].path = "../secret".to_owned();
        assert!(matches!(
            adapter.inspect(&traversal),
            Err(AdapterError::InvalidPath(_))
        ));
    }

    #[test]
    fn missing_adapter_is_installable_structured_failure() {
        let mut registry = AdapterRegistry::default();
        registry
            .declare_provider(ProfileId("bcs1".to_owned()), "lamquant-legacy")
            .unwrap();
        match registry.get(&ProfileId("bcs1".to_owned())) {
            Err(AdapterError::AdapterUnavailable {
                package,
                capability,
            }) => {
                assert_eq!(package, "lamquant-legacy");
                assert_eq!(capability, "bcs1");
            }
            _ => panic!("missing adapter must return AdapterUnavailable"),
        }
    }

    #[test]
    fn duplicate_provider_declaration_fails_closed() {
        let mut registry = AdapterRegistry::default();
        let id = ProfileId("bcs1".to_owned());
        registry
            .declare_provider(id.clone(), "lamquant-legacy")
            .unwrap();
        assert!(registry.declare_provider(id, "other-package").is_err());
    }

    #[test]
    fn duplicate_registration_preserves_original_adapter() {
        let mut registry = AdapterRegistry::default();
        registry
            .register(ForensicAdapter::new(profile(), 1024))
            .unwrap();
        assert!(registry
            .register(ForensicAdapter::new(profile(), 2048))
            .is_err());
        assert_eq!(
            registry
                .get(&ProfileId("edfplus.1".to_owned()))
                .unwrap()
                .inspect(&source())
                .unwrap()
                .required_resources["max-source-bytes"],
            1024
        );
    }

    #[test]
    fn committed_profile_registry_matches_rust_contract() {
        let registry = AdapterProfileRegistry::parse_json(include_bytes!(
            "../../../registries/adapter-profiles-v1.json"
        ))
        .unwrap();
        assert_eq!(registry.profiles.len(), 12);
        assert!(registry.profiles.iter().all(|profile| {
            profile.capabilities.contains(&AdapterCapability::Inspect)
                && profile.capabilities.contains(&AdapterCapability::Validate)
        }));
        // Semantic promotion is earned at the conformance/standards gate (ADR 0143;
        // see verify_adapter_contract.py), not asserted structurally here — a
        // permanent "no semantic profiles" assertion would invalidate every
        // correctly promoted Adapter. Structural contract: any semantic profile
        // declares the full core Adapter capability set.
        let core = [
            AdapterCapability::Inspect,
            AdapterCapability::Import,
            AdapterCapability::PlanExport,
            AdapterCapability::Export,
            AdapterCapability::Validate,
        ];
        assert!(registry
            .profiles
            .iter()
            .filter(|profile| profile.status == ProfileStatus::Semantic)
            .all(|profile| core
                .iter()
                .all(|capability| profile.capabilities.contains(capability))));
    }
}
