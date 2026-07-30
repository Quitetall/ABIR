use abir::{payload_content_id, ByteOrder, ContentId, ElementType};
use abir_training::{
    ContentKey, DecisionLog, SamplerStrategy, TrainingProfile, TrainingProgram, TrainingRow,
    TrainingSampler, TrainingSemanticDescriptor, TrainingSemanticRole, TrainingSnapshot,
    TrainingSpec,
};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn key(seed: u8) -> ContentKey {
    ContentKey::new(ContentId::from_bytes([seed; 32]))
}

fn main() {
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
            .expect("fixture descriptor must seal")
        })
        .collect::<Vec<_>>();
    let ids = descriptors
        .iter()
        .map(|descriptor| {
            (
                descriptor.role(),
                ContentKey::from(
                    descriptor
                        .content_id()
                        .expect("fixture descriptor identity must seal"),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let sampler =
        TrainingSampler::seal(SamplerStrategy::Shuffle).expect("fixture sampler must seal");
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
        sampler: ContentKey::from(
            sampler
                .content_id()
                .expect("fixture sampler identity must seal"),
        ),
        seed: 42,
        split: ids[&TrainingSemanticRole::Split],
        view: ids[&TrainingSemanticRole::View],
        window: ids[&TrainingSemanticRole::Window],
        allowed_adaptive_knobs: vec!["prefetch-depth".to_owned()],
    };
    let program = TrainingProgram::seal(&spec, descriptors, sampler, vec![key(200)])
        .expect("fixture program must seal");
    let decision_log = DecisionLog::seal(&spec, vec![]).expect("fixture decision log must seal");
    let payload = [10_u8, 0];
    let row = TrainingRow {
        byte_order: ByteOrder::Little,
        encoding: None,
        element: ElementType::I16,
        group: key(20),
        label: key(21),
        logical_bytes: payload.len() as u64,
        logical_id: key(10),
        payload: ContentKey::new(payload_content_id(ElementType::I16, &payload)),
        shape: vec![1],
        split: key(30),
    };
    let snapshot = TrainingSnapshot::seal_with_program(
        vec![key(1)],
        spec,
        program,
        TrainingProfile::Balanced,
        vec![row],
        decision_log,
    )
    .expect("fixture snapshot must seal");

    let mut catalog = snapshot
        .canonical_json()
        .expect("fixture catalog must canonicalize");
    let content_id = snapshot
        .content_id()
        .expect("fixture snapshot identity must seal")
        .to_string();
    if let Some(output_dir) = std::env::args_os().nth(1).map(PathBuf::from) {
        std::fs::create_dir_all(&output_dir).expect("fixture output directory must exist");
        catalog.push(b'\n');
        std::fs::write(output_dir.join("valid-snapshot.json"), catalog)
            .expect("fixture catalog must write");
        std::fs::write(
            output_dir.join("valid-snapshot.content-id"),
            format!("{content_id}\n"),
        )
        .expect("fixture identity must write");
    } else {
        println!(
            "{}",
            String::from_utf8(catalog).expect("canonical JSON must be UTF-8")
        );
        println!("{content_id}");
    }
}
