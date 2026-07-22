use abir::{payload_content_id, ByteOrder, ContentId, ElementType};
use abir_bcs::{Bcs2View, ResourceBounds, SemanticPayloadFrame};
use abir_training::{
    encode_snapshot, ContentKey, ContinualPromotion, DatasetSubscription, DecisionLog,
    DecisionLogReplayState, DecisionRecord, DecisionReplayReceipt, MicroSnapshot,
    SourceEquivalenceReceipt, SubscriptionCorrection, TrainingAssociatedPayload, TrainingError,
    TrainingLabelPayloadAssociation, TrainingProfile, TrainingRow, TrainingSnapshot, TrainingSpec,
    TrainingWindowStore,
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

const SEIZURE_MASK: &str = "org.quitetall.lamquant.label.seizure-mask-v1";

fn seizure_mask_association(
    logical_id: ContentKey,
    bytes: &[u8],
) -> TrainingLabelPayloadAssociation {
    TrainingLabelPayloadAssociation {
        concept: SEIZURE_MASK.to_owned(),
        logical_id,
        payload: Some(TrainingAssociatedPayload {
            byte_order: ByteOrder::NotApplicable,
            element: ElementType::U8,
            logical_bytes: bytes.len() as u64,
            payload: ContentKey::new(payload_content_id(ElementType::U8, bytes)),
            shape: vec![bytes.len() as u64],
        }),
        presence: abir::Presence::Present,
    }
}

#[test]
fn typed_label_payload_is_leased_with_exact_presence_and_bytes() {
    let signal = [1_u8, 0, 2, 0];
    let mask = [0_u8, 1];
    let row = row(10, 20, &signal);
    let snapshot = TrainingSnapshot::seal_with_label_payloads(
        vec![key(1)],
        key(3),
        TrainingProfile::Balanced,
        vec![row.clone()],
        vec![seizure_mask_association(row.logical_id, &mask)],
        key(4),
    )
    .unwrap();
    let catalog = String::from_utf8(snapshot.canonical_json().unwrap()).unwrap();
    assert!(catalog.contains("org.quitetall.abir.training.snapshot-v2"));
    assert!(catalog.contains("label_payloads"));
    let encoded = encode_snapshot(
        &snapshot,
        &[
            SemanticPayloadFrame::new(ElementType::I16, &signal),
            SemanticPayloadFrame::new(ElementType::U8, &mask),
        ],
        ResourceBounds::default(),
    )
    .unwrap();

    let store = TrainingWindowStore::open(&encoded, ResourceBounds::default()).unwrap();
    let lease = store
        .label_payload(row.logical_id, SEIZURE_MASK)
        .expect("sealed association");

    assert_eq!(lease.presence(), abir::Presence::Present);
    assert_eq!(lease.bytes(), Some(mask.as_slice()));
    assert_eq!(lease.element(), Some(ElementType::U8));
    assert_eq!(lease.shape(), Some([2].as_slice()));
}

#[test]
fn unavailable_label_payloads_are_explicit_and_carry_no_frame() {
    let signal = [1_u8, 0, 2, 0];
    let row = row(10, 20, &signal);
    let association = TrainingLabelPayloadAssociation {
        concept: SEIZURE_MASK.to_owned(),
        logical_id: row.logical_id,
        payload: None,
        presence: abir::Presence::UnknownAtSource,
    };
    let snapshot = TrainingSnapshot::seal_with_label_payloads(
        vec![key(1)],
        key(3),
        TrainingProfile::Balanced,
        vec![row.clone()],
        vec![association],
        key(4),
    )
    .unwrap();
    let encoded = encode_snapshot(
        &snapshot,
        &[SemanticPayloadFrame::new(ElementType::I16, &signal)],
        ResourceBounds::default(),
    )
    .unwrap();

    let store = TrainingWindowStore::open(&encoded, ResourceBounds::default()).unwrap();
    let lease = store.label_payload(row.logical_id, SEIZURE_MASK).unwrap();
    assert_eq!(lease.presence(), abir::Presence::UnknownAtSource);
    assert_eq!(lease.bytes(), None);
}

#[test]
fn label_payload_presence_and_association_identity_fail_closed() {
    let signal = [1_u8, 0, 2, 0];
    let mask = [0_u8, 1];
    let row = row(10, 20, &signal);
    let mut absent_with_payload = seizure_mask_association(row.logical_id, &mask);
    absent_with_payload.presence = abir::Presence::AbsentAtSource;
    assert!(TrainingSnapshot::seal_with_label_payloads(
        vec![key(1)],
        key(3),
        TrainingProfile::Balanced,
        vec![row.clone()],
        vec![absent_with_payload],
        key(4),
    )
    .is_err());

    let unknown_row = TrainingLabelPayloadAssociation {
        concept: SEIZURE_MASK.to_owned(),
        logical_id: key(99),
        payload: None,
        presence: abir::Presence::UnknownAtSource,
    };
    assert!(TrainingSnapshot::seal_with_label_payloads(
        vec![key(1)],
        key(3),
        TrainingProfile::Balanced,
        vec![row],
        vec![unknown_row],
        key(4),
    )
    .is_err());
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
    let catalog = String::from_utf8(first.canonical_json().unwrap()).unwrap();
    assert!(catalog.contains("org.quitetall.abir.training.snapshot-v1"));
    assert!(!catalog.contains("label_payloads"));
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
fn opened_store_exposes_only_snapshot_bound_training_metadata() {
    let row_bytes = [1_u8, 0, 2, 0];
    let metadata = row(10, 20, &row_bytes);
    let snapshot = snapshot(TrainingProfile::Balanced, vec![metadata.clone()]);
    let encoded = encode_snapshot(
        &snapshot,
        &[SemanticPayloadFrame::new(ElementType::I16, &row_bytes)],
        ResourceBounds::default(),
    )
    .unwrap();

    let store = TrainingWindowStore::open(&encoded, ResourceBounds::default()).unwrap();
    let lease = store.row(metadata.logical_id).unwrap();

    assert_eq!(store.snapshot_id().unwrap(), snapshot.content_id().unwrap());
    assert_eq!(store.spec_id(), key(3));
    assert_eq!(store.dataset_roots(), &[key(1), key(2)]);
    assert_eq!(store.decision_log_id(), key(4));
    assert_eq!(
        store.decision_log_replay_state(),
        DecisionLogReplayState::IdentityBound
    );
    assert_eq!(store.decision_log_replay_state().as_str(), "identity-bound");
    assert_eq!(lease.logical_id(), key(10));
    assert_eq!(lease.group(), key(20));
    assert_eq!(lease.label(), key(9));
    assert_eq!(lease.split(), key(8));
    assert_eq!(lease.payload_id(), metadata.payload);
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
fn durable_decision_log_reopens_and_yields_a_snapshot_bound_replay_receipt() {
    let spec = training_spec(vec!["worker-count"]);
    let records = vec![DecisionRecord {
        activation_barrier: 10,
        decision: key(30),
        durable_before_activation: true,
        knob: "worker-count".to_owned(),
        rank: 0,
        sequence: 0,
    }];
    let log = DecisionLog::seal(&spec, records.clone()).unwrap();
    let reopened = DecisionLog::from_canonical_json(&log.canonical_json().unwrap()).unwrap();
    let receipt = DecisionReplayReceipt::verify(&spec, &reopened, &records).unwrap();
    let snapshot = TrainingSnapshot::seal(
        vec![key(1)],
        ContentKey::from(spec.content_id().unwrap()),
        TrainingProfile::Balanced,
        vec![row(10, 20, &[1, 0])],
        ContentKey::from(log.content_id().unwrap()),
    )
    .unwrap();
    let encoded = encode_snapshot(
        &snapshot,
        &[SemanticPayloadFrame::new(ElementType::I16, &[1, 0])],
        ResourceBounds::default(),
    )
    .unwrap();
    let store = TrainingWindowStore::open(&encoded, ResourceBounds::default()).unwrap();

    assert_eq!(receipt.record_count(), 1);
    assert_eq!(receipt.decision_log_id(), store.decision_log_id());
    assert_eq!(
        store.verify_decision_replay(&spec, &reopened, &records),
        Ok(receipt)
    );

    let mut changed = records;
    changed[0].decision = key(31);
    assert_eq!(
        store.verify_decision_replay(&spec, &reopened, &changed),
        Err(TrainingError::DecisionReplayMismatch)
    );
}

#[test]
fn validated_stores_produce_source_equivalence_receipts_only_for_exact_windows() {
    let bytes = [1_u8, 0, 2, 0];
    let left_snapshot = TrainingSnapshot::seal(
        vec![key(1)],
        key(3),
        TrainingProfile::Balanced,
        vec![row(10, 20, &bytes)],
        key(4),
    )
    .unwrap();
    let right_snapshot = TrainingSnapshot::seal(
        vec![key(2)],
        key(3),
        TrainingProfile::Balanced,
        vec![row(10, 20, &bytes)],
        key(4),
    )
    .unwrap();
    let left_artifact = encode_snapshot(
        &left_snapshot,
        &[SemanticPayloadFrame::new(ElementType::I16, &bytes)],
        ResourceBounds::default(),
    )
    .unwrap();
    let right_artifact = encode_snapshot(
        &right_snapshot,
        &[SemanticPayloadFrame::new(ElementType::I16, &bytes)],
        ResourceBounds::default(),
    )
    .unwrap();
    let left = TrainingWindowStore::open(&left_artifact, ResourceBounds::default()).unwrap();
    let right = TrainingWindowStore::open(&right_artifact, ResourceBounds::default()).unwrap();
    let receipt = SourceEquivalenceReceipt::verify(&left, &right).unwrap();

    assert_eq!(
        receipt.first_snapshot_id(),
        left_snapshot.content_id().unwrap().into()
    );
    assert_eq!(
        receipt.second_snapshot_id(),
        right_snapshot.content_id().unwrap().into()
    );
    assert_ne!(receipt.first_snapshot_id(), receipt.second_snapshot_id());
    assert_ne!(
        receipt.first_dataset_roots_id(),
        receipt.second_dataset_roots_id()
    );
    assert_eq!(receipt.row_count(), 1);

    let other_bytes = [9_u8, 0, 2, 0];
    let other_snapshot = snapshot(TrainingProfile::Balanced, vec![row(10, 20, &other_bytes)]);
    let other_artifact = encode_snapshot(
        &other_snapshot,
        &[SemanticPayloadFrame::new(ElementType::I16, &other_bytes)],
        ResourceBounds::default(),
    )
    .unwrap();
    let other = TrainingWindowStore::open(&other_artifact, ResourceBounds::default()).unwrap();
    assert_eq!(
        SourceEquivalenceReceipt::verify(&left, &other),
        Err(TrainingError::SourceSnapshotMismatch)
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
fn continual_promotion_requires_every_ordered_snapshot_and_decision_log() {
    let spec = training_spec(vec![]);
    let log = DecisionLog::seal(&spec, Vec::new()).unwrap();
    let row_bytes = [1_u8, 0];
    let first = TrainingSnapshot::seal(
        vec![key(61)],
        ContentKey::from(spec.content_id().unwrap()),
        TrainingProfile::Stream,
        vec![row(62, 63, &row_bytes)],
        ContentKey::from(log.content_id().unwrap()),
    )
    .unwrap();
    let first_artifact = encode_snapshot(
        &first,
        &[SemanticPayloadFrame::new(ElementType::I16, &row_bytes)],
        ResourceBounds::default(),
    )
    .unwrap();
    let first_store =
        TrainingWindowStore::open(&first_artifact, ResourceBounds::default()).unwrap();
    let verified_first = first_store.verified_snapshot();
    let mut subscription = DatasetSubscription::new(key(60));
    subscription
        .append(MicroSnapshot {
            correction: None,
            generation: 0,
            logical_id: key(64),
            sequence: 0,
            snapshot_id: ContentKey::from(first.content_id().unwrap()),
            watermark: 100,
        })
        .unwrap();
    let closed = subscription.close().unwrap();
    let reopened_subscription =
        abir_training::ClosedSubscription::from_canonical_json(&closed.canonical_json().unwrap())
            .unwrap();
    let reopened_log = DecisionLog::from_canonical_json(&log.canonical_json().unwrap()).unwrap();
    let replay = DecisionReplayReceipt::verify(&spec, &reopened_log, log.records()).unwrap();
    let promotion = ContinualPromotion::seal(
        &reopened_subscription,
        &spec,
        &[verified_first.clone()],
        &[reopened_log.clone()],
        &[replay.clone()],
    )
    .unwrap();

    assert_eq!(promotion.entry_count(), 1);
    assert_eq!(
        promotion.closed_subscription_id(),
        closed.content_id().unwrap().into()
    );
    assert_eq!(
        promotion.entries()[0].snapshot_id,
        first.content_id().unwrap().into()
    );
    assert_eq!(
        promotion.entries()[0].decision_log_id,
        log.content_id().unwrap().into()
    );
    assert_eq!(
        ContinualPromotion::seal(&reopened_subscription, &spec, &[], &[], &[]),
        Err(TrainingError::IncompleteContinualPromotion {
            expected: 1,
            snapshots: 0,
            decision_logs: 0,
            replay_receipts: 0,
        })
    );

    let wrong_log = DecisionLog::seal(&training_spec(vec!["workers"]), Vec::new()).unwrap();
    let wrong_log = DecisionLog::from_canonical_json(&wrong_log.canonical_json().unwrap()).unwrap();
    assert_eq!(
        ContinualPromotion::seal(
            &reopened_subscription,
            &spec,
            &[verified_first],
            &[wrong_log],
            &[replay]
        ),
        Err(TrainingError::DecisionSpecMismatch)
    );
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
