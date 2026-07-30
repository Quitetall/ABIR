use abir::{payload_content_id, ByteOrder, ContentId, ElementType};
use abir_bcs::{ResourceBounds, SemanticPayloadFrame};
use abir_training::{
    encode_snapshot, ContentKey, DecisionLog, DecisionLogReplayState, DecisionRecord, EpochShard,
    PrefetchPolicy, SamplerStrategy, SamplerStratum, SamplerStratumKey, TrainingExecutionDecision,
    TrainingProfile, TrainingProgram, TrainingRow, TrainingSampler, TrainingSemanticDescriptor,
    TrainingSemanticRole, TrainingSnapshot, TrainingSpec, TrainingWindowStore,
};
use serde_json::json;
use std::collections::BTreeMap;

fn key(seed: u8) -> ContentKey {
    ContentKey::new(ContentId::from_bytes([seed; 32]))
}

fn row(logical_seed: u8, label_seed: u8) -> (TrainingRow, [u8; 2]) {
    let bytes = [logical_seed, 0];
    (
        TrainingRow {
            byte_order: ByteOrder::Little,
            encoding: None,
            element: ElementType::I16,
            group: key(20),
            label: key(label_seed),
            logical_bytes: 2,
            logical_id: key(logical_seed),
            payload: ContentKey::new(payload_content_id(ElementType::I16, &bytes)),
            shape: vec![1],
            split: key(30),
        },
        bytes,
    )
}

fn program_and_spec(
    strategy: SamplerStrategy,
) -> (
    TrainingProgram,
    TrainingSpec,
    Vec<TrainingSemanticDescriptor>,
) {
    let descriptors = TrainingSemanticRole::ALL
        .into_iter()
        .enumerate()
        .map(|(index, role)| {
            TrainingSemanticDescriptor::new(
                role,
                json!({
                    "definition": {"version": index + 1},
                    "schema": role.domain(),
                }),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let ids = descriptors
        .iter()
        .map(|descriptor| {
            (
                descriptor.role(),
                ContentKey::from(descriptor.content_id().unwrap()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let sampler = TrainingSampler::seal(strategy).unwrap();
    let spec = TrainingSpec {
        augmentation: ids[&TrainingSemanticRole::Augmentation],
        authorized_purpose: "representation-learning".to_owned(),
        cohort: ids[&TrainingSemanticRole::Cohort],
        feature: ids[&TrainingSemanticRole::Feature],
        fitted_state: ids[&TrainingSemanticRole::FittedState],
        grouping: ids[&TrainingSemanticRole::Grouping],
        label: ids[&TrainingSemanticRole::Label],
        policy: ids[&TrainingSemanticRole::Policy],
        preprocessing: ids[&TrainingSemanticRole::Preprocessing],
        sampler: ContentKey::from(sampler.content_id().unwrap()),
        seed: 42,
        split: ids[&TrainingSemanticRole::Split],
        view: ids[&TrainingSemanticRole::View],
        window: ids[&TrainingSemanticRole::Window],
        allowed_adaptive_knobs: vec!["prefetch-depth".to_owned()],
    };
    let program =
        TrainingProgram::seal(&spec, descriptors.clone(), sampler, vec![key(200)]).unwrap();
    (program, spec, descriptors)
}

#[test]
fn semantic_roles_own_distinct_fixed_identity_domains() {
    let augmentation = TrainingSemanticDescriptor::new(
        TrainingSemanticRole::Augmentation,
        json!({
            "definition": {"version": 1},
            "schema": TrainingSemanticRole::Augmentation.domain(),
        }),
    )
    .unwrap();
    let preprocessing = TrainingSemanticDescriptor::new(
        TrainingSemanticRole::Preprocessing,
        json!({
            "definition": {"version": 1},
            "schema": TrainingSemanticRole::Preprocessing.domain(),
        }),
    )
    .unwrap();

    assert_ne!(
        augmentation.content_id().unwrap(),
        preprocessing.content_id().unwrap()
    );
    assert_eq!(
        augmentation.domain(),
        "org.quitetall.abir.training.semantic.augmentation-v1"
    );
    assert_eq!(
        preprocessing.domain(),
        "org.quitetall.abir.training.semantic.preprocessing-v1"
    );
}

#[test]
fn semantic_descriptors_reject_excessive_nesting() {
    let mut definition = json!(0);
    for _ in 0..70 {
        definition = json!([definition]);
    }

    assert!(TrainingSemanticDescriptor::new(
        TrainingSemanticRole::View,
        json!({
            "definition": definition,
            "schema": TrainingSemanticRole::View.domain(),
        }),
    )
    .is_err());
}

#[test]
fn semantic_descriptors_reject_cross_role_schema_aliases() {
    assert!(TrainingSemanticDescriptor::new(
        TrainingSemanticRole::Preprocessing,
        json!({
            "definition": {"steps": []},
            "schema": TrainingSemanticRole::View.domain(),
        }),
    )
    .is_err());
}

#[test]
fn semantic_and_sampler_canonical_json_use_lexicographic_object_keys() {
    assert!(TrainingSemanticDescriptor::from_canonical_json(
        TrainingSemanticRole::View,
        br#"{"schema":"org.quitetall.abir.training.semantic.view-v1","definition":{"b":2,"a":1}}"#,
    )
    .is_err());
    assert!(TrainingSemanticDescriptor::from_canonical_json(
        TrainingSemanticRole::View,
        br#"{"definition":{"a":1,"b":2},"schema":"org.quitetall.abir.training.semantic.view-v1"}"#,
    )
    .is_ok());

    let sampler = TrainingSampler::seal(SamplerStrategy::Stratified {
        key: SamplerStratumKey::Label,
        strata: vec![SamplerStratum {
            draws: 1,
            value: key(1),
        }],
    })
    .unwrap();
    let canonical = String::from_utf8(sampler.canonical_json().unwrap()).unwrap();
    assert!(canonical.contains(r#""strategy":{"key":"label","kind":"stratified","strata":"#));
}

#[test]
fn training_v4_positive_fixture_matches_typed_sealer() {
    let (program, spec, _) = program_and_spec(SamplerStrategy::Shuffle);
    let decision_log = DecisionLog::seal(&spec, vec![]).unwrap();
    let (row, _) = row(10, 21);
    let snapshot = TrainingSnapshot::seal_with_program(
        vec![key(1)],
        spec,
        program,
        TrainingProfile::Balanced,
        vec![row],
        decision_log,
    )
    .unwrap();
    let fixture =
        include_bytes!("../../../fixtures/training/v4/valid-snapshot.json").strip_suffix(b"\n");

    assert_eq!(snapshot.canonical_json().unwrap(), fixture.unwrap());
    assert_eq!(
        snapshot.content_id().unwrap().to_string(),
        include_str!("../../../fixtures/training/v4/valid-snapshot.content-id").trim()
    );
}

#[test]
fn snapshot_v4_embeds_replayable_program_and_compiles_world_invariant_epoch() {
    let (program, spec, _) = program_and_spec(SamplerStrategy::Shuffle);
    let decision_log = DecisionLog::seal(&spec, vec![]).unwrap();
    let rows_and_bytes = [row(10, 1), row(11, 1), row(12, 2), row(13, 2)];
    let rows = rows_and_bytes
        .iter()
        .map(|(row, _)| row.clone())
        .collect::<Vec<_>>();
    let snapshot = TrainingSnapshot::seal_with_program(
        vec![key(1)],
        spec,
        program,
        TrainingProfile::Balanced,
        rows,
        decision_log,
    )
    .unwrap();
    assert!(String::from_utf8(snapshot.canonical_json().unwrap())
        .unwrap()
        .contains("org.quitetall.abir.training.snapshot-v4"));
    let frames = rows_and_bytes
        .iter()
        .map(|(_, bytes)| SemanticPayloadFrame::new(ElementType::I16, bytes))
        .collect::<Vec<_>>();
    let artifact = encode_snapshot(&snapshot, &frames, ResourceBounds::default()).unwrap();
    let store = TrainingWindowStore::open(&artifact, ResourceBounds::default()).unwrap();

    assert_eq!(
        store.decision_log_replay_state(),
        DecisionLogReplayState::ReplayReady
    );
    let single = store.compile_epoch(3, key(200), 0, 1).unwrap();
    let rank_zero = store.compile_epoch(3, key(200), 0, 2).unwrap();
    let rank_one = store.compile_epoch(3, key(200), 1, 2).unwrap();
    assert_eq!(single.global_schedule_id(), rank_zero.global_schedule_id());
    assert_eq!(single.global_schedule_id(), rank_one.global_schedule_id());

    let mut sharded = rank_zero
        .rows()
        .iter()
        .chain(rank_one.rows())
        .copied()
        .collect::<Vec<_>>();
    sharded.sort_by_key(|row| row.global_index);
    assert_eq!(single.rows(), sharded);
    assert_eq!(single, store.compile_epoch(3, key(200), 0, 1).unwrap());
    assert_ne!(
        single.rows()[0].stochastic_seed,
        store.compile_epoch(4, key(200), 0, 1).unwrap().rows()[0].stochastic_seed
    );
}

#[test]
fn stochastic_seed_is_bound_to_declared_node_identity() {
    let (base_program, spec, descriptors) = program_and_spec(SamplerStrategy::Sequential);
    let sampler = base_program.sampler().clone();
    let program =
        TrainingProgram::seal(&spec, descriptors, sampler, vec![key(200), key(201)]).unwrap();
    let rows = [row(10, 1).0, row(11, 1).0];
    let first = program
        .compile_epoch(&spec, &rows, key(90), 3, key(200), EpochShard::new(0, 1))
        .unwrap();
    let second = program
        .compile_epoch(&spec, &rows, key(90), 3, key(201), EpochShard::new(0, 1))
        .unwrap();

    assert_eq!(first.global_schedule_id(), second.global_schedule_id());
    assert_eq!(
        first
            .rows()
            .iter()
            .map(|row| row.logical_id)
            .collect::<Vec<_>>(),
        second
            .rows()
            .iter()
            .map(|row| row.logical_id)
            .collect::<Vec<_>>()
    );
    assert_ne!(
        first
            .rows()
            .iter()
            .map(|row| row.stochastic_seed)
            .collect::<Vec<_>>(),
        second
            .rows()
            .iter()
            .map(|row| row.stochastic_seed)
            .collect::<Vec<_>>()
    );
    assert_eq!(first.stochastic_node_id(), key(200));
    assert_eq!(second.stochastic_node_id(), key(201));
    assert!(program
        .compile_epoch(&spec, &rows, key(90), 3, key(202), EpochShard::new(0, 1),)
        .is_err());
}

#[test]
fn snapshot_v4_replay_resolves_typed_execution_decisions_at_declared_barriers() {
    let (_, spec, descriptors) = program_and_spec(SamplerStrategy::Shuffle);
    let sampler = TrainingSampler::seal(SamplerStrategy::Shuffle).unwrap();
    let decision = TrainingExecutionDecision::PrefetchDepth(PrefetchPolicy::Rows { rows: 7 });
    let decision_id = ContentKey::from(decision.content_id().unwrap());
    let log = DecisionLog::seal(
        &spec,
        vec![DecisionRecord {
            activation_barrier: 4,
            decision: decision_id,
            durable_before_activation: true,
            knob: "prefetch-depth".to_owned(),
            rank: 0,
            sequence: 0,
        }],
    )
    .unwrap();
    let program = TrainingProgram::seal_with_execution_decisions(
        &spec,
        descriptors.clone(),
        sampler.clone(),
        vec![key(200)],
        vec![decision],
    )
    .unwrap();
    let before = program
        .compile_execution_plan_at_barrier(&spec, &log, TrainingProfile::Balanced, 3)
        .unwrap();
    let after = program
        .compile_execution_plan_at_barrier(&spec, &log, TrainingProfile::Balanced, 4)
        .unwrap();

    assert_ne!(before.prefetch(), PrefetchPolicy::Rows { rows: 7 });
    assert_eq!(after.prefetch(), PrefetchPolicy::Rows { rows: 7 });

    let unresolved = TrainingProgram::seal(&spec, descriptors, sampler, vec![key(200)]).unwrap();
    assert!(TrainingSnapshot::seal_with_program(
        vec![key(1)],
        spec,
        unresolved,
        TrainingProfile::Balanced,
        vec![row(10, 1).0],
        log,
    )
    .is_err());
}

#[test]
fn epoch_execution_compiles_schedule_and_replayed_plan_from_one_log() {
    let (_, spec, descriptors) = program_and_spec(SamplerStrategy::Sequential);
    let sampler = TrainingSampler::seal(SamplerStrategy::Sequential).unwrap();
    let decision = TrainingExecutionDecision::PrefetchDepth(PrefetchPolicy::Rows { rows: 7 });
    let decision_id = ContentKey::from(decision.content_id().unwrap());
    let log = DecisionLog::seal(
        &spec,
        vec![DecisionRecord {
            activation_barrier: 4,
            decision: decision_id,
            durable_before_activation: true,
            knob: "prefetch-depth".to_owned(),
            rank: 0,
            sequence: 0,
        }],
    )
    .unwrap();
    let program = TrainingProgram::seal_with_execution_decisions(
        &spec,
        descriptors,
        sampler,
        vec![key(200)],
        vec![decision],
    )
    .unwrap();
    let rows = [row(10, 1).0, row(11, 1).0];

    let before = program
        .compile_epoch_execution(
            &spec,
            &rows,
            &log,
            TrainingProfile::Balanced,
            3,
            7,
            key(200),
            0,
            1,
        )
        .unwrap();
    let after = program
        .compile_epoch_execution(
            &spec,
            &rows,
            &log,
            TrainingProfile::Balanced,
            4,
            7,
            key(200),
            0,
            1,
        )
        .unwrap();

    assert_eq!(before.epoch(), after.epoch());
    assert_eq!(before.applied_decision_count(), 0);
    assert_eq!(after.applied_decision_count(), 1);
    assert_ne!(
        before.plan().content_id().unwrap(),
        after.plan().content_id().unwrap()
    );
    assert_eq!(after.plan().prefetch(), PrefetchPolicy::Rows { rows: 7 });
    assert_eq!(
        after.epoch().decision_log_id(),
        ContentKey::from(log.content_id().unwrap())
    );
}

#[test]
fn compiled_epoch_binds_decision_log_without_changing_scientific_order() {
    let (program, spec, _) = program_and_spec(SamplerStrategy::Shuffle);
    let rows = [row(10, 1).0, row(11, 1).0];
    let first = program
        .compile_epoch(&spec, &rows, key(90), 3, key(200), EpochShard::new(0, 1))
        .unwrap();
    let second = program
        .compile_epoch(&spec, &rows, key(91), 3, key(200), EpochShard::new(0, 1))
        .unwrap();

    assert_eq!(first.rows(), second.rows());
    assert_ne!(first.decision_log_id(), second.decision_log_id());
    assert_ne!(first.global_schedule_id(), second.global_schedule_id());
    assert_ne!(first.content_id().unwrap(), second.content_id().unwrap());
}

#[test]
fn semantic_descriptor_mismatch_and_incomplete_closure_fail_closed() {
    let (program, mut spec, mut descriptors) = program_and_spec(SamplerStrategy::Sequential);
    spec.view = key(99);
    assert!(TrainingProgram::seal(
        &spec,
        descriptors.clone(),
        program.sampler().clone(),
        vec![key(200)],
    )
    .is_err());

    descriptors.pop();
    assert!(TrainingProgram::seal(
        &spec,
        descriptors,
        program.sampler().clone(),
        vec![key(200)],
    )
    .is_err());
}

#[test]
fn stratified_sampler_replays_exact_declared_draws_with_replacement() {
    let strategy = SamplerStrategy::Stratified {
        key: SamplerStratumKey::Label,
        strata: vec![
            SamplerStratum {
                draws: 4,
                value: key(1),
            },
            SamplerStratum {
                draws: 2,
                value: key(2),
            },
        ],
    };
    let (program, spec, _) = program_and_spec(strategy);
    let rows = vec![row(10, 1).0, row(11, 2).0, row(12, 2).0];
    let compiled = program
        .compile_epoch(&spec, &rows, key(90), 7, key(200), EpochShard::new(0, 1))
        .unwrap();
    let rare = compiled
        .rows()
        .iter()
        .filter(|draw| draw.logical_id == key(10))
        .count();
    assert_eq!(compiled.global_rows(), 6);
    assert_eq!(rare, 4);
    assert_eq!(
        compiled,
        program
            .compile_epoch(&spec, &rows, key(90), 7, key(200), EpochShard::new(0, 1),)
            .unwrap()
    );
}

#[test]
fn distributed_epoch_refuses_implicit_example_drops() {
    let (program, spec, _) = program_and_spec(SamplerStrategy::Sequential);
    let rows = vec![row(10, 1).0, row(11, 1).0, row(12, 1).0];
    let error = program
        .compile_epoch(&spec, &rows, key(90), 0, key(200), EpochShard::new(0, 2))
        .unwrap_err();
    assert!(matches!(
        error,
        abir_training::TrainingError::UnevenDistributedEpoch {
            rows: 3,
            world_size: 2
        }
    ));
}
