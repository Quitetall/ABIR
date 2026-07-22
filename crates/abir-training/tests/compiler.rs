use abir_training::{
    compile_execution_plan, CacheBudget, ClosurePolicy, PayloadAccessPolicy, PlanCompileError,
    PlanOverrides, PrefetchPolicy, RowGrouping, TrainingProfile,
};
use std::collections::BTreeSet;

#[test]
fn all_registered_profiles_compile_to_distinct_plans() {
    let mut identities = BTreeSet::new();
    for profile in TrainingProfile::ALL {
        let plan = compile_execution_plan(profile, PlanOverrides::default()).unwrap();
        assert_eq!(plan.profile(), profile);
        assert!(identities.insert(plan.content_id().unwrap()));
    }
    assert_eq!(identities.len(), TrainingProfile::ALL.len());
}

#[test]
fn portable_profiles_are_closed_and_forbid_external_references() {
    for profile in [TrainingProfile::Compact, TrainingProfile::UltraCompact] {
        let plan = compile_execution_plan(profile, PlanOverrides::default()).unwrap();
        assert_eq!(plan.closure(), ClosurePolicy::Portable);
        assert!(!plan.allows_external_references());

        let error = compile_execution_plan(
            profile,
            PlanOverrides {
                closure: Some(ClosurePolicy::AllowVerifiedExternalReferences),
                ..PlanOverrides::default()
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            PlanCompileError::PortableProfileExternalReferences(profile)
        );
    }
}

#[test]
fn execution_overrides_change_only_the_plan_identity() {
    let baseline =
        compile_execution_plan(TrainingProfile::Balanced, PlanOverrides::default()).unwrap();
    let overridden = compile_execution_plan(
        TrainingProfile::Balanced,
        PlanOverrides {
            row_grouping: Some(RowGrouping::FixedRows { rows: 8 }),
            prefetch: Some(PrefetchPolicy::Rows { rows: 2 }),
            payload_access: Some(PayloadAccessPolicy::Materialize),
            cache_budget: Some(CacheBudget::new(64 * 1024 * 1024).unwrap()),
            closure: Some(ClosurePolicy::Portable),
        },
    )
    .unwrap();

    assert_ne!(
        baseline.content_id().unwrap(),
        overridden.content_id().unwrap()
    );
    assert_eq!(overridden.profile(), TrainingProfile::Balanced);
    assert!(!overridden.allows_external_references());
}

#[test]
fn hardware_observation_is_not_an_identity_input() {
    #[derive(Clone, Copy)]
    struct HardwareObservation {
        _available_memory: u64,
        _worker_count: u32,
    }

    fn compile_on(_hardware: HardwareObservation) -> abir_training::CompiledExecutionPlan {
        compile_execution_plan(TrainingProfile::Speed, PlanOverrides::default()).unwrap()
    }

    let small = compile_on(HardwareObservation {
        _available_memory: 64 * 1024 * 1024,
        _worker_count: 1,
    });
    let large = compile_on(HardwareObservation {
        _available_memory: 1024 * 1024 * 1024 * 1024,
        _worker_count: 128,
    });
    assert_eq!(small.content_id().unwrap(), large.content_id().unwrap());
}

#[test]
fn invalid_execution_overrides_fail_closed() {
    let cases = [
        PlanOverrides {
            row_grouping: Some(RowGrouping::FixedRows { rows: 0 }),
            ..PlanOverrides::default()
        },
        PlanOverrides {
            row_grouping: Some(RowGrouping::TargetBytes { bytes: 1024 }),
            ..PlanOverrides::default()
        },
        PlanOverrides {
            prefetch: Some(PrefetchPolicy::Rows { rows: 0 }),
            ..PlanOverrides::default()
        },
        PlanOverrides {
            payload_access: Some(PayloadAccessPolicy::Materialize),
            cache_budget: Some(CacheBudget::new(0).unwrap()),
            ..PlanOverrides::default()
        },
    ];

    for overrides in cases {
        assert!(compile_execution_plan(TrainingProfile::Balanced, overrides).is_err());
    }
    assert_eq!(
        CacheBudget::new(16 * 1024 * 1024 * 1024 + 1).unwrap_err(),
        PlanCompileError::CacheBudgetOutOfRange(16 * 1024 * 1024 * 1024 + 1)
    );
}
