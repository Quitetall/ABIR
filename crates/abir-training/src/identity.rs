use abir::ContentId;

/// Closed registry of ABIR training identity domains.
///
/// Callers cannot mint new semantic namespaces. Adding or changing a variant
/// requires a schema generation, registry update, fixtures, and manifest hash.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrainingContentDomain {
    AugmentationV1,
    CohortV1,
    EpochV1,
    ExecutionDecisionV1,
    FeatureV1,
    FittedStateV1,
    GroupingV1,
    LabelV1,
    OrderV1,
    PolicyV1,
    PreprocessingV1,
    ProgramV1,
    SamplerV1,
    ScheduleV1,
    SplitV1,
    StochasticSeedV1,
    ViewV1,
    WindowV1,
}

impl TrainingContentDomain {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AugmentationV1 => "org.quitetall.abir.training.semantic.augmentation-v1",
            Self::CohortV1 => "org.quitetall.abir.training.semantic.cohort-v1",
            Self::EpochV1 => "org.quitetall.abir.training.epoch-v1",
            Self::ExecutionDecisionV1 => "org.quitetall.abir.training.execution-decision-v1",
            Self::FeatureV1 => "org.quitetall.abir.training.semantic.feature-v1",
            Self::FittedStateV1 => "org.quitetall.abir.training.semantic.fitted-state-v1",
            Self::GroupingV1 => "org.quitetall.abir.training.semantic.grouping-v1",
            Self::LabelV1 => "org.quitetall.abir.training.semantic.label-v1",
            Self::OrderV1 => "org.quitetall.abir.training.order-v1",
            Self::PolicyV1 => "org.quitetall.abir.training.semantic.policy-v1",
            Self::PreprocessingV1 => "org.quitetall.abir.training.semantic.preprocessing-v1",
            Self::ProgramV1 => "org.quitetall.abir.training.program-v1",
            Self::SamplerV1 => "org.quitetall.abir.training.sampler-v1",
            Self::ScheduleV1 => "org.quitetall.abir.training.schedule-v1",
            Self::SplitV1 => "org.quitetall.abir.training.semantic.split-v1",
            Self::StochasticSeedV1 => "org.quitetall.abir.training.stochastic-seed-v1",
            Self::ViewV1 => "org.quitetall.abir.training.semantic.view-v1",
            Self::WindowV1 => "org.quitetall.abir.training.semantic.window-v1",
        }
    }
}

/// Hash exact bytes under one registered ABIR training domain.
///
/// Semantic sealers own canonicalization before calling this primitive.
pub(crate) fn training_content_id(
    domain: TrainingContentDomain,
    canonical_bytes: &[u8],
) -> ContentId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain.as_str().as_bytes());
    hasher.update(&[0]);
    hasher.update(canonical_bytes);
    ContentId::from_bytes(*hasher.finalize().as_bytes())
}
