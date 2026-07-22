use abir::{payload_content_id, ByteOrder, ContentId, ElementType};
use abir_bcs::{Bcs2View, ResourceBounds, SemanticPayloadFrame};
use abir_training::{
    encode_snapshot, ContentKey, DatasetSubscription, DecisionLog, DecisionRecord, MicroSnapshot,
    SubscriptionCorrection, TrainingError, TrainingProfile, TrainingRow, TrainingSnapshot,
    TrainingSpec, TrainingWindowStore,
};

fn key(seed: u8) -> ContentKey {
    ContentKey::new(ContentId::from_bytes([seed; 32]))
}

fn row(logical_seed: u8, group_seed: u8, bytes: &[u8]) -> TrainingRow {
    TrainingRow {
        byte_order: ByteOrder::Little,
        group: key(group_seed),
        label: key(9),
        logical_bytes: bytes.len() as u64,
        logical_id: key(logical_seed),
        payload: ContentKey::new(payload_content_id(ElementType::I16, bytes)),
        element: ElementType::I16,
        shape: vec![(bytes.len() / 2) as u64],
        split: key(8),
    }
}

fn snapshot(profile: TrainingProfile, rows: Vec<TrainingRow>) -> TrainingSnapshot {
    TrainingSnapshot::seal(vec![key(2), key(1)], key(3), profile, rows, key(4)).unwrap()
}

fn training_spec(knobs: Vec<&str>) -> TrainingSpec {
    TrainingSpec {
        augmentation: key(1),
        authorized_purpose: "representation-learning".to_owned(),
        cohort: key(2),
        feature: key(3),
        fitted_state: key(4),
        grouping: key(5),
        label: key(6),
        policy: key(7),
        preprocessing: key(8),
        sampler: key(9),
        seed: 42,
        split: key(10),
        view: key(11),
        window: key(12),
        allowed_adaptive_knobs: knobs.into_iter().map(str::to_owned).collect(),
    }
}

#[test]
fn source_equivalent_rows_and_roots_have_the_same_snapshot_identity() {
    let row_a_bytes = [1_u8, 0, 2, 0];
    let row_b_bytes = [3_u8, 0, 4, 0];
    let row_a = row(10, 20, &row_a_bytes);
    let row_b = row(11, 20, &row_b_bytes);

    let first = TrainingSnapshot::seal(
        vec![key(1), key(2)],
        key(3),
        TrainingProfile::Balanced,
        vec![row_a.clone(), row_b.clone()],
        key(4),
    )
    .unwrap();
    let second = TrainingSnapshot::seal(
        vec![key(2), key(1)],
        key(3),
        TrainingProfile::Balanced,
        vec![row_b, row_a],
        key(4),
    )
    .unwrap();

    assert_eq!(first.content_id().unwrap(), second.content_id().unwrap());
    assert_eq!(
        first.canonical_json().unwrap(),
        second.canonical_json().unwrap()
    );
}

#[test]
fn training_spec_identity_treats_adaptive_knobs_as_a_set() {
    let first = training_spec(vec!["workers", "batch", "workers"]);
    let second = training_spec(vec!["batch", "workers"]);

    assert_eq!(first.content_id().unwrap(), second.content_id().unwrap());
    assert_eq!(
        first.canonical_json().unwrap(),
        second.canonical_json().unwrap()
    );
}

#[test]
fn encoded_snapshot_opens_and_rows_lease_original_frame_bytes() {
    let row_bytes = [1_u8, 0, 2, 0];
    let metadata = row(10, 20, &row_bytes);
    let snapshot = snapshot(TrainingProfile::Balanced, vec![metadata.clone()]);
    let encoded = encode_snapshot(
        &snapshot,
        &[SemanticPayloadFrame::new(ElementType::I16, &row_bytes)],
        ResourceBounds::default(),
    )
    .unwrap();

    let wire = Bcs2View::parse(&encoded, 0, ResourceBounds::default()).unwrap();
    let frame_ptr = wire.frames()[0].bytes().as_ptr();
    let store = TrainingWindowStore::open(&encoded, ResourceBounds::default()).unwrap();
    let lease = store.row(metadata.logical_id).unwrap();

    assert_eq!(lease.bytes(), row_bytes);
    assert_eq!(lease.bytes().as_ptr(), frame_ptr);
    assert_eq!(lease.shape(), &[2]);
    assert_eq!(lease.byte_order(), ByteOrder::Little);
    assert_eq!(store.rows().len(), 1);
}

#[test]
fn row_byte_order_is_bound_and_invalid_numeric_order_fails_closed() {
    let row_bytes = [0_u8, 1, 0, 2];
    let mut big_endian = row(10, 20, &row_bytes);
    big_endian.byte_order = ByteOrder::Big;
    let snapshot = snapshot(TrainingProfile::Balanced, vec![big_endian.clone()]);
    let encoded = encode_snapshot(
        &snapshot,
        &[SemanticPayloadFrame::new(ElementType::I16, &row_bytes)],
        ResourceBounds::default(),
    )
    .unwrap();
    let store = TrainingWindowStore::open(&encoded, ResourceBounds::default()).unwrap();
    assert_eq!(
        store.row(big_endian.logical_id).unwrap().byte_order(),
        ByteOrder::Big
    );

    let mut invalid = row(11, 20, &row_bytes);
    invalid.byte_order = ByteOrder::NotApplicable;
    assert!(matches!(
        TrainingSnapshot::seal(
            vec![key(1)],
            key(2),
            TrainingProfile::Balanced,
            vec![invalid],
            key(3),
        ),
        Err(TrainingError::InvalidByteOrder(_))
    ));
}

#[test]
fn missing_extra_malformed_and_mismatched_payloads_fail_closed() {
    let row_bytes = [1_u8, 0, 2, 0];
    let extra_bytes = [9_u8, 0];
    let metadata = row(10, 20, &row_bytes);
    let snapshot = snapshot(TrainingProfile::Compact, vec![metadata]);
    let bounds = ResourceBounds::default();

    assert!(matches!(
        encode_snapshot(&snapshot, &[], bounds),
        Err(TrainingError::MissingPayload(_))
    ));
    assert!(matches!(
        encode_snapshot(
            &snapshot,
            &[
                SemanticPayloadFrame::new(ElementType::I16, &row_bytes),
                SemanticPayloadFrame::new(ElementType::I16, &extra_bytes),
            ],
            bounds,
        ),
        Err(TrainingError::ExtraPayload(_))
    ));
    assert!(encode_snapshot(
        &snapshot,
        &[SemanticPayloadFrame::new(ElementType::U16, &row_bytes)],
        bounds,
    )
    .is_err());

    let mut encoded = encode_snapshot(
        &snapshot,
        &[SemanticPayloadFrame::new(ElementType::I16, &row_bytes)],
        bounds,
    )
    .unwrap();
    let frame_offset = {
        let parsed = Bcs2View::parse(&encoded, 0, bounds).unwrap();
        parsed.frames()[0].bytes().as_ptr() as usize - encoded.as_ptr() as usize
    };
    encoded[frame_offset] ^= 1;
    assert!(TrainingWindowStore::open(&encoded, bounds).is_err());
}

#[test]
fn decision_replay_requires_consecutive_rank_zero_pre_activation_records() {
    let spec = training_spec(vec!["batch-shape", "worker-count"]);
    let records = vec![
        DecisionRecord {
            activation_barrier: 10,
            decision: key(30),
            durable_before_activation: true,
            knob: "batch-shape".to_owned(),
            rank: 0,
            sequence: 0,
        },
        DecisionRecord {
            activation_barrier: 20,
            decision: key(31),
            durable_before_activation: true,
            knob: "worker-count".to_owned(),
            rank: 0,
            sequence: 1,
        },
    ];
    let log = DecisionLog::seal(&spec, records.clone()).unwrap();
    assert_eq!(
        log.replay_identity(&spec, &records).unwrap(),
        log.content_id().unwrap()
    );

    let mut changed = records.clone();
    changed[1].decision = key(32);
    assert_eq!(
        log.replay_identity(&spec, &changed),
        Err(TrainingError::DecisionReplayMismatch)
    );
    let mut bad_rank = records;
    bad_rank[0].rank = 1;
    assert_eq!(
        DecisionLog::seal(&spec, bad_rank),
        Err(TrainingError::RankNotZero(1))
    );

    let mut not_durable = log.records().to_vec();
    not_durable[0].durable_before_activation = false;
    assert_eq!(
        DecisionLog::seal(&spec, not_durable),
        Err(TrainingError::DecisionWasNotDurable)
    );

    let mut disallowed = log.records().to_vec();
    disallowed[0].knob = "precision".to_owned();
    assert_eq!(
        DecisionLog::seal(&spec, disallowed),
        Err(TrainingError::InvalidAdaptiveKnob("precision".to_owned()))
    );

    let mut regressing = log.records().to_vec();
    regressing[1].activation_barrier = 9;
    assert_eq!(
        DecisionLog::seal(&spec, regressing),
        Err(TrainingError::ActivationBarrierRegression {
            previous: 10,
            next: 9,
        })
    );

    let other_spec = training_spec(vec!["batch-shape"]);
    assert_eq!(
        log.replay_identity(&other_spec, log.records()),
        Err(TrainingError::DecisionSpecMismatch)
    );
}

#[test]
fn continual_subscription_seals_ordered_corrections_as_new_generations() {
    let logical_id = key(40);
    let first_snapshot = key(41);
    let corrected_snapshot = key(42);
    let mut subscription = DatasetSubscription::new(key(39));
    subscription
        .append(MicroSnapshot {
            correction: None,
            generation: 0,
            logical_id,
            sequence: 0,
            snapshot_id: first_snapshot,
            watermark: 100,
        })
        .unwrap();
    subscription
        .append(MicroSnapshot {
            correction: Some(SubscriptionCorrection {
                prior_generation: 0,
                prior_snapshot_id: first_snapshot,
            }),
            generation: 1,
            logical_id,
            sequence: 1,
            snapshot_id: corrected_snapshot,
            watermark: 100,
        })
        .unwrap();
    let closed = subscription.close().unwrap();

    assert_eq!(closed.events().len(), 2);
    assert_eq!(closed.content_id().unwrap(), closed.content_id().unwrap());

    let mut regressing = DatasetSubscription::new(key(50));
    regressing
        .append(MicroSnapshot {
            correction: None,
            generation: 0,
            logical_id: key(51),
            sequence: 0,
            snapshot_id: key(52),
            watermark: 10,
        })
        .unwrap();
    assert!(matches!(
        regressing.append(MicroSnapshot {
            correction: None,
            generation: 0,
            logical_id: key(53),
            sequence: 1,
            snapshot_id: key(54),
            watermark: 9,
        }),
        Err(TrainingError::NonMonotonicWatermark { .. })
    ));
}

#[test]
fn all_six_training_profiles_map_to_registered_bcs2_profiles() {
    let row_bytes = [1_u8, 0];
    for profile in TrainingProfile::ALL {
        assert_eq!(
            TrainingProfile::from_bcs2(profile.bcs2_profile()).unwrap(),
            profile
        );
        let snapshot = snapshot(profile, vec![row(10, 20, &row_bytes)]);
        let encoded = encode_snapshot(
            &snapshot,
            &[SemanticPayloadFrame::new(ElementType::I16, &row_bytes)],
            ResourceBounds::default(),
        )
        .unwrap();
        let store = TrainingWindowStore::open(&encoded, ResourceBounds::default()).unwrap();
        assert_eq!(store.snapshot().profile(), profile);
    }
}
