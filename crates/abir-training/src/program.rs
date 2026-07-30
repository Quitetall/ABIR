use crate::{
    canonical::canonical_json,
    compile_execution_plan,
    compiler::{CompiledExecutionPlan, PlanOverrides, TrainingExecutionDecision},
    identity::{training_content_id, TrainingContentDomain},
    ContentKey, DecisionLog, TrainingError, TrainingProfile, TrainingRow, TrainingSpec,
};
use abir::ContentId;
use abir_bcs::ResourceBounds;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const PROGRAM_SCHEMA: &str = "org.quitetall.abir.training.program-v1";
const SAMPLER_SCHEMA: &str = "org.quitetall.abir.training.sampler-v1";
const EPOCH_SCHEMA: &str = "org.quitetall.abir.training.epoch-v1";

/// One semantic role whose exact canonical descriptor is required by a program.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrainingSemanticRole {
    Augmentation,
    Cohort,
    Feature,
    FittedState,
    Grouping,
    Label,
    Policy,
    Preprocessing,
    Split,
    View,
    Window,
}

impl TrainingSemanticRole {
    pub const ALL: [Self; 11] = [
        Self::Augmentation,
        Self::Cohort,
        Self::Feature,
        Self::FittedState,
        Self::Grouping,
        Self::Label,
        Self::Policy,
        Self::Preprocessing,
        Self::Split,
        Self::View,
        Self::Window,
    ];

    const fn expected(self, spec: &TrainingSpec) -> ContentKey {
        match self {
            Self::Augmentation => spec.augmentation,
            Self::Cohort => spec.cohort,
            Self::Feature => spec.feature,
            Self::FittedState => spec.fitted_state,
            Self::Grouping => spec.grouping,
            Self::Label => spec.label,
            Self::Policy => spec.policy,
            Self::Preprocessing => spec.preprocessing,
            Self::Split => spec.split,
            Self::View => spec.view,
            Self::Window => spec.window,
        }
    }

    const fn content_domain(self) -> TrainingContentDomain {
        match self {
            Self::Augmentation => TrainingContentDomain::AugmentationV1,
            Self::Cohort => TrainingContentDomain::CohortV1,
            Self::Feature => TrainingContentDomain::FeatureV1,
            Self::FittedState => TrainingContentDomain::FittedStateV1,
            Self::Grouping => TrainingContentDomain::GroupingV1,
            Self::Label => TrainingContentDomain::LabelV1,
            Self::Policy => TrainingContentDomain::PolicyV1,
            Self::Preprocessing => TrainingContentDomain::PreprocessingV1,
            Self::Split => TrainingContentDomain::SplitV1,
            Self::View => TrainingContentDomain::ViewV1,
            Self::Window => TrainingContentDomain::WindowV1,
        }
    }

    /// Registered identity domain and descriptor schema for this semantic role.
    pub const fn domain(self) -> &'static str {
        self.content_domain().as_str()
    }
}

/// Canonical semantic bytes behind one ContentId referenced by TrainingSpec.
///
/// Domain and value are retained, not only the digest. Opening a program can
/// therefore prove what each identifier means instead of accepting arbitrary
/// hexadecimal strings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrainingSemanticDescriptor {
    role: TrainingSemanticRole,
    value: Value,
}

impl TrainingSemanticDescriptor {
    pub fn new(role: TrainingSemanticRole, value: Value) -> Result<Self, TrainingError> {
        let descriptor = Self { role, value };
        descriptor.validate()?;
        Ok(descriptor)
    }

    pub fn from_canonical_json(
        role: TrainingSemanticRole,
        canonical_json: &[u8],
    ) -> Result<Self, TrainingError> {
        ensure_catalog_bound(canonical_json, TrainingError::InvalidTrainingProgram)?;
        let value: Value = serde_json::from_slice(canonical_json)?;
        let descriptor = Self::new(role, value)?;
        if descriptor.canonical_json()? != canonical_json {
            return Err(TrainingError::InvalidTrainingProgram);
        }
        Ok(descriptor)
    }

    pub const fn role(&self) -> TrainingSemanticRole {
        self.role
    }

    pub const fn domain(&self) -> &'static str {
        self.role.domain()
    }

    pub const fn value(&self) -> &Value {
        &self.value
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, TrainingError> {
        let canonical = canonical_json(&self.value)?;
        ensure_catalog_bound(&canonical, TrainingError::InvalidTrainingProgram)?;
        Ok(canonical)
    }

    pub fn content_id(&self) -> Result<ContentId, TrainingError> {
        Ok(training_content_id(
            self.role.content_domain(),
            &self.canonical_json()?,
        ))
    }

    fn validate(&self) -> Result<(), TrainingError> {
        let Value::Object(value) = &self.value else {
            return Err(TrainingError::InvalidTrainingProgram);
        };
        if value.len() != 2
            || value.get("schema").and_then(Value::as_str) != Some(self.role.domain())
            || !value.contains_key("definition")
        {
            return Err(TrainingError::InvalidTrainingProgram);
        }
        self.content_id().map(|_| ())
    }
}

/// Row metadata used by a stratified sampler.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SamplerStratumKey {
    Group,
    Label,
    Split,
}

impl SamplerStratumKey {
    const fn value(self, row: &TrainingRow) -> ContentKey {
        match self {
            Self::Group => row.group,
            Self::Label => row.label,
            Self::Split => row.split,
        }
    }
}

/// Exact number of draws from one semantic stratum per epoch.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SamplerStratum {
    pub draws: u64,
    pub value: ContentKey,
}

/// Scientifically meaningful logical-row selection.
///
/// Physical workers, prefetch, storage grouping, and cache policy are not
/// represented here because they may not change selected examples.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SamplerStrategy {
    Sequential,
    Shuffle,
    Stratified {
        key: SamplerStratumKey,
        strata: Vec<SamplerStratum>,
    },
}

/// Executable sampler authority embedded in a TrainingProgram.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrainingSampler {
    schema: String,
    sealed: bool,
    strategy: SamplerStrategy,
}

impl TrainingSampler {
    pub fn seal(strategy: SamplerStrategy) -> Result<Self, TrainingError> {
        let sampler = Self {
            schema: SAMPLER_SCHEMA.to_owned(),
            sealed: true,
            strategy,
        };
        sampler.validate()?;
        Ok(sampler)
    }

    pub const fn strategy(&self) -> &SamplerStrategy {
        &self.strategy
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, TrainingError> {
        self.validate()?;
        let canonical = canonical_json(self)?;
        ensure_catalog_bound(&canonical, TrainingError::InvalidSampler)?;
        Ok(canonical)
    }

    pub fn content_id(&self) -> Result<ContentId, TrainingError> {
        Ok(training_content_id(
            TrainingContentDomain::SamplerV1,
            &self.canonical_json()?,
        ))
    }

    fn validate(&self) -> Result<(), TrainingError> {
        if self.schema != SAMPLER_SCHEMA || !self.sealed {
            return Err(TrainingError::InvalidSampler);
        }
        if let SamplerStrategy::Stratified { strata, .. } = &self.strategy {
            let total_draws = strata
                .iter()
                .try_fold(0_u64, |total, stratum| total.checked_add(stratum.draws));
            if strata.is_empty()
                || strata.iter().any(|stratum| stratum.draws == 0)
                || strata.windows(2).any(|pair| pair[0].value >= pair[1].value)
                || total_draws.is_none()
                || total_draws.is_some_and(|draws| {
                    draws > u64::from(ResourceBounds::default().max_index_entries)
                })
            {
                return Err(TrainingError::InvalidSampler);
            }
        }
        Ok(())
    }
}

/// Complete semantic and executable authority for one training snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrainingProgram {
    descriptors: Vec<TrainingSemanticDescriptor>,
    execution_decisions: Vec<TrainingExecutionDecision>,
    sampler: TrainingSampler,
    schema: String,
    sealed: bool,
}

impl TrainingProgram {
    pub fn seal(
        spec: &TrainingSpec,
        descriptors: Vec<TrainingSemanticDescriptor>,
        sampler: TrainingSampler,
    ) -> Result<Self, TrainingError> {
        Self::seal_with_execution_decisions(spec, descriptors, sampler, Vec::new())
    }

    pub fn seal_with_execution_decisions(
        spec: &TrainingSpec,
        mut descriptors: Vec<TrainingSemanticDescriptor>,
        sampler: TrainingSampler,
        mut execution_decisions: Vec<TrainingExecutionDecision>,
    ) -> Result<Self, TrainingError> {
        descriptors.sort_by_key(TrainingSemanticDescriptor::role);
        execution_decisions.sort_by_key(|decision| {
            decision
                .content_id()
                .map(|content_id| *content_id.as_bytes())
                .unwrap_or([0; 32])
        });
        let program = Self {
            descriptors,
            execution_decisions,
            sampler,
            schema: PROGRAM_SCHEMA.to_owned(),
            sealed: true,
        };
        program.validate_for_spec(spec)?;
        Ok(program)
    }

    pub fn descriptors(&self) -> &[TrainingSemanticDescriptor] {
        &self.descriptors
    }

    pub fn execution_decisions(&self) -> &[TrainingExecutionDecision] {
        &self.execution_decisions
    }

    pub const fn sampler(&self) -> &TrainingSampler {
        &self.sampler
    }

    pub fn descriptor(&self, role: TrainingSemanticRole) -> Option<&TrainingSemanticDescriptor> {
        self.descriptors
            .binary_search_by_key(&role, TrainingSemanticDescriptor::role)
            .ok()
            .map(|index| &self.descriptors[index])
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, TrainingError> {
        self.validate_intrinsic()?;
        let canonical = canonical_json(self)?;
        ensure_catalog_bound(&canonical, TrainingError::InvalidTrainingProgram)?;
        Ok(canonical)
    }

    pub fn content_id(&self) -> Result<ContentId, TrainingError> {
        Ok(training_content_id(
            TrainingContentDomain::ProgramV1,
            &self.canonical_json()?,
        ))
    }

    /// Compile physical execution state visible at one durable activation barrier.
    pub fn compile_execution_plan_at_barrier(
        &self,
        spec: &TrainingSpec,
        decision_log: &DecisionLog,
        profile: TrainingProfile,
        activation_barrier: u64,
    ) -> Result<CompiledExecutionPlan, TrainingError> {
        self.validate_replay_for_spec(spec, decision_log)?;
        let mut overrides = PlanOverrides::default();
        for record in decision_log
            .records()
            .iter()
            .take_while(|record| record.activation_barrier <= activation_barrier)
        {
            let decision = self
                .execution_decision(record.decision)
                .ok_or(TrainingError::InvalidExecutionDecision)?;
            decision
                .apply(&mut overrides)
                .map_err(|_| TrainingError::InvalidExecutionDecision)?;
        }
        compile_execution_plan(profile, overrides)
            .map_err(|_| TrainingError::InvalidExecutionDecision)
    }

    pub(crate) fn validate_replay_for_spec(
        &self,
        spec: &TrainingSpec,
        decision_log: &DecisionLog,
    ) -> Result<(), TrainingError> {
        self.validate_for_spec(spec)?;
        decision_log.validate_for_spec(spec)?;
        let mut referenced = BTreeSet::new();
        for record in decision_log.records() {
            let decision = self
                .execution_decision(record.decision)
                .ok_or(TrainingError::InvalidExecutionDecision)?;
            if decision.knob() != record.knob {
                return Err(TrainingError::InvalidExecutionDecision);
            }
            referenced.insert(record.decision);
        }
        if referenced.len() != self.execution_decisions.len() {
            return Err(TrainingError::InvalidExecutionDecision);
        }
        Ok(())
    }

    pub(crate) fn validate_replay_for_profile(
        &self,
        spec: &TrainingSpec,
        decision_log: &DecisionLog,
        profile: TrainingProfile,
    ) -> Result<(), TrainingError> {
        self.validate_replay_for_spec(spec, decision_log)?;
        self.compile_execution_plan_at_barrier(spec, decision_log, profile, 0)?;
        for record in decision_log.records() {
            self.compile_execution_plan_at_barrier(
                spec,
                decision_log,
                profile,
                record.activation_barrier,
            )?;
        }
        Ok(())
    }

    fn execution_decision(&self, content_key: ContentKey) -> Option<TrainingExecutionDecision> {
        self.execution_decisions.iter().copied().find(|decision| {
            decision
                .content_id()
                .is_ok_and(|content_id| ContentKey::from(content_id) == content_key)
        })
    }

    pub fn compile_epoch(
        &self,
        spec: &TrainingSpec,
        rows: &[TrainingRow],
        decision_log_id: ContentKey,
        epoch: u64,
        rank: u32,
        world_size: u32,
    ) -> Result<CompiledTrainingEpoch, TrainingError> {
        self.validate_for_spec(spec)?;
        if world_size == 0 || rank >= world_size || rows.is_empty() {
            return Err(TrainingError::InvalidEpoch);
        }
        let sampler_id = ContentKey::from(self.sampler.content_id()?);
        let mut selected = match self.sampler.strategy() {
            SamplerStrategy::Sequential => {
                rows.iter().map(|row| row.logical_id).collect::<Vec<_>>()
            }
            SamplerStrategy::Shuffle => {
                let mut selected = rows.iter().map(|row| row.logical_id).collect::<Vec<_>>();
                selected.sort_by_cached_key(|logical_id| {
                    order_key(spec.seed, epoch, sampler_id, *logical_id, 0)
                });
                selected
            }
            SamplerStrategy::Stratified { key, strata } => {
                compile_stratified(spec.seed, epoch, sampler_id, rows, *key, strata)?
            }
        };
        if selected.len() % world_size as usize != 0 {
            return Err(TrainingError::UnevenDistributedEpoch {
                rows: selected.len(),
                world_size,
            });
        }
        let global_rows = selected.len();
        let global_schedule_id = schedule_id(spec, self, decision_log_id, epoch, &selected)?;
        let mut occurrences = BTreeMap::<ContentKey, u64>::new();
        let mut compiled_rows = Vec::with_capacity(global_rows / world_size as usize);
        for (global_index, logical_id) in selected.drain(..).enumerate() {
            let occurrence = occurrences.entry(logical_id).or_default();
            let row = CompiledTrainingRow {
                global_index: global_index as u64,
                logical_id,
                occurrence: *occurrence,
                stochastic_seed: stochastic_seed(spec, epoch, sampler_id, logical_id, *occurrence),
            };
            *occurrence += 1;
            if global_index % world_size as usize == rank as usize {
                compiled_rows.push(row);
            }
        }
        let compiled = CompiledTrainingEpoch {
            decision_log_id,
            epoch,
            global_rows: global_rows as u64,
            global_schedule_id,
            rank,
            rows: compiled_rows,
            schema: EPOCH_SCHEMA.to_owned(),
            world_size,
        };
        compiled.validate()?;
        Ok(compiled)
    }

    pub(crate) fn validate_for_spec(&self, spec: &TrainingSpec) -> Result<(), TrainingError> {
        self.validate_intrinsic()?;
        if ContentKey::from(self.sampler.content_id()?) != spec.sampler {
            return Err(TrainingError::InvalidTrainingProgram);
        }
        for role in TrainingSemanticRole::ALL {
            let descriptor = self
                .descriptor(role)
                .ok_or(TrainingError::IncompleteSemanticClosure)?;
            if ContentKey::from(descriptor.content_id()?) != role.expected(spec) {
                return Err(TrainingError::InvalidTrainingProgram);
            }
        }
        Ok(())
    }

    fn validate_intrinsic(&self) -> Result<(), TrainingError> {
        if self.schema != PROGRAM_SCHEMA
            || !self.sealed
            || self.descriptors.len() != TrainingSemanticRole::ALL.len()
            || self
                .descriptors
                .windows(2)
                .any(|pair| pair[0].role() >= pair[1].role())
        {
            return Err(TrainingError::InvalidTrainingProgram);
        }
        for descriptor in &self.descriptors {
            descriptor.validate()?;
        }
        let mut prior = None;
        for decision in &self.execution_decisions {
            let content_id = decision
                .content_id()
                .map_err(|_| TrainingError::InvalidExecutionDecision)?;
            if prior.is_some_and(|prior| prior >= content_id) {
                return Err(TrainingError::InvalidExecutionDecision);
            }
            prior = Some(content_id);
        }
        self.sampler.validate()
    }
}

/// One exact logical draw and its worker-independent stochastic seed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompiledTrainingRow {
    pub global_index: u64,
    pub logical_id: ContentKey,
    pub occurrence: u64,
    pub stochastic_seed: u64,
}

/// Rank projection of one globally compiled scientific epoch.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompiledTrainingEpoch {
    decision_log_id: ContentKey,
    epoch: u64,
    global_rows: u64,
    global_schedule_id: ContentKey,
    rank: u32,
    rows: Vec<CompiledTrainingRow>,
    schema: String,
    world_size: u32,
}

impl CompiledTrainingEpoch {
    pub const fn decision_log_id(&self) -> ContentKey {
        self.decision_log_id
    }

    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    pub const fn global_rows(&self) -> u64 {
        self.global_rows
    }

    pub const fn global_schedule_id(&self) -> ContentKey {
        self.global_schedule_id
    }

    pub const fn rank(&self) -> u32 {
        self.rank
    }

    pub fn rows(&self) -> &[CompiledTrainingRow] {
        &self.rows
    }

    pub const fn world_size(&self) -> u32 {
        self.world_size
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, TrainingError> {
        self.validate()?;
        let canonical = canonical_json(self)?;
        ensure_catalog_bound(&canonical, TrainingError::InvalidEpoch)?;
        Ok(canonical)
    }

    pub fn content_id(&self) -> Result<ContentId, TrainingError> {
        Ok(training_content_id(
            TrainingContentDomain::EpochV1,
            &self.canonical_json()?,
        ))
    }

    fn validate(&self) -> Result<(), TrainingError> {
        if self.schema != EPOCH_SCHEMA
            || self.world_size == 0
            || self.rank >= self.world_size
            || self.global_rows == 0
            || self.global_rows % u64::from(self.world_size) != 0
            || self.rows.len() as u64 != self.global_rows / u64::from(self.world_size)
            || self.rows.iter().enumerate().any(|(local, row)| {
                row.global_index != self.rank as u64 + local as u64 * self.world_size as u64
            })
        {
            return Err(TrainingError::InvalidEpoch);
        }
        Ok(())
    }
}

fn compile_stratified(
    seed: u64,
    epoch: u64,
    sampler_id: ContentKey,
    rows: &[TrainingRow],
    key: SamplerStratumKey,
    strata: &[SamplerStratum],
) -> Result<Vec<ContentKey>, TrainingError> {
    let mut by_stratum = BTreeMap::<ContentKey, Vec<ContentKey>>::new();
    for row in rows {
        by_stratum
            .entry(key.value(row))
            .or_default()
            .push(row.logical_id);
    }
    let declared = strata
        .iter()
        .map(|stratum| stratum.value)
        .collect::<BTreeSet<_>>();
    if by_stratum.keys().any(|value| !declared.contains(value)) {
        return Err(TrainingError::InvalidSampler);
    }

    let mut selected = Vec::new();
    for stratum in strata {
        let candidates = by_stratum
            .get(&stratum.value)
            .ok_or(TrainingError::InvalidSampler)?;
        let draws = usize::try_from(stratum.draws).map_err(|_| TrainingError::InvalidSampler)?;
        let mut ordered = candidates.clone();
        ordered.sort_by_cached_key(|logical_id| {
            order_key(
                seed,
                epoch,
                sampler_id,
                *logical_id,
                stratum.value.content_id().as_bytes()[0] as u64,
            )
        });
        for draw in 0..draws {
            selected.push(ordered[draw % ordered.len()]);
        }
    }
    let mut occurrences = BTreeMap::<ContentKey, u64>::new();
    let mut ordered = selected
        .into_iter()
        .map(|logical_id| {
            let occurrence = occurrences.entry(logical_id).or_default();
            let key = order_key(seed, epoch, sampler_id, logical_id, *occurrence);
            *occurrence += 1;
            (key, logical_id)
        })
        .collect::<Vec<_>>();
    ordered.sort_by_key(|(key, _)| *key);
    Ok(ordered
        .into_iter()
        .map(|(_, logical_id)| logical_id)
        .collect())
}

fn order_key(
    seed: u64,
    epoch: u64,
    sampler_id: ContentKey,
    logical_id: ContentKey,
    occurrence: u64,
) -> [u8; 32] {
    let mut bytes = [0_u8; 88];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    bytes[8..16].copy_from_slice(&epoch.to_le_bytes());
    bytes[16..48].copy_from_slice(sampler_id.content_id().as_bytes());
    bytes[48..80].copy_from_slice(logical_id.content_id().as_bytes());
    bytes[80..].copy_from_slice(&occurrence.to_le_bytes());
    training_content_id(TrainingContentDomain::OrderV1, &bytes).to_bytes()
}

fn stochastic_seed(
    spec: &TrainingSpec,
    epoch: u64,
    sampler_id: ContentKey,
    logical_id: ContentKey,
    occurrence: u64,
) -> u64 {
    let mut bytes = [0_u8; 120];
    bytes[..8].copy_from_slice(&spec.seed.to_le_bytes());
    bytes[8..16].copy_from_slice(&epoch.to_le_bytes());
    bytes[16..48].copy_from_slice(sampler_id.content_id().as_bytes());
    bytes[48..80].copy_from_slice(spec.augmentation.content_id().as_bytes());
    bytes[80..112].copy_from_slice(logical_id.content_id().as_bytes());
    bytes[112..].copy_from_slice(&occurrence.to_le_bytes());
    let digest = training_content_id(TrainingContentDomain::StochasticSeedV1, &bytes);
    u64::from_le_bytes(
        digest.as_bytes()[..8]
            .try_into()
            .expect("ContentId has eight bytes"),
    )
}

fn schedule_id(
    spec: &TrainingSpec,
    program: &TrainingProgram,
    decision_log_id: ContentKey,
    epoch: u64,
    selected: &[ContentKey],
) -> Result<ContentKey, TrainingError> {
    let program_id = program.content_id()?;
    let spec_id = spec.content_id()?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(TrainingContentDomain::ScheduleV1.as_str().as_bytes());
    hasher.update(&[0]);
    hasher.update(&epoch.to_le_bytes());
    hasher.update(decision_log_id.content_id().as_bytes());
    hasher.update(program_id.as_bytes());
    hasher.update(spec_id.as_bytes());
    hasher.update(&(selected.len() as u64).to_le_bytes());
    for logical_id in selected {
        hasher.update(logical_id.content_id().as_bytes());
    }
    Ok(ContentKey::new(ContentId::from_bytes(
        *hasher.finalize().as_bytes(),
    )))
}

fn ensure_catalog_bound(canonical: &[u8], error: TrainingError) -> Result<(), TrainingError> {
    if canonical.len() > ResourceBounds::default().max_catalog_bytes as usize {
        return Err(error);
    }
    Ok(())
}
