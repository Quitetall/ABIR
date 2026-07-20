use crate::{
    Atom, ChannelBasis, Clock, ContentId, CoordinateFrame, DatasetTag, Derivation, ExecutionRecord,
    FailureCode, Fidelity, FidelityKind, ObjectId, ObjectKind, Policy, Proof, Recording,
    SemanticRef, SourceCapsule, Stream, ValidationFailure, ValidationLimits, ValidationReport,
};
use alloc::collections::BTreeSet;
use alloc::format;
use alloc::vec::Vec;

#[derive(Clone, Debug)]
pub struct DatasetDraft {
    id: ObjectId<DatasetTag>,
    recordings: Vec<Recording>,
    streams: Vec<Stream>,
    atoms: Vec<Atom>,
    clocks: Vec<Clock>,
    coordinate_frames: Vec<CoordinateFrame>,
    channel_bases: Vec<ChannelBasis>,
    policies: Vec<Policy>,
    proofs: Vec<Proof>,
    derivations: Vec<Derivation>,
    fidelity: Vec<Fidelity>,
    source_capsules: Vec<SourceCapsule>,
    observed_execution: Vec<ExecutionRecord>,
}

impl DatasetDraft {
    pub fn new(id: ObjectId<DatasetTag>) -> Self {
        Self {
            id,
            recordings: Vec::new(),
            streams: Vec::new(),
            atoms: Vec::new(),
            clocks: Vec::new(),
            coordinate_frames: Vec::new(),
            channel_bases: Vec::new(),
            policies: Vec::new(),
            proofs: Vec::new(),
            derivations: Vec::new(),
            fidelity: Vec::new(),
            source_capsules: Vec::new(),
            observed_execution: Vec::new(),
        }
    }

    pub fn add_recording(&mut self, value: Recording) {
        self.recordings.push(value);
    }
    pub fn add_stream(&mut self, value: Stream) {
        self.streams.push(value);
    }
    pub fn add_atom(&mut self, value: Atom) {
        self.atoms.push(value);
    }
    pub fn add_clock(&mut self, value: Clock) {
        self.clocks.push(value);
    }
    pub fn add_coordinate_frame(&mut self, value: CoordinateFrame) {
        self.coordinate_frames.push(value);
    }
    pub fn add_channel_basis(&mut self, value: ChannelBasis) {
        self.channel_bases.push(value);
    }
    pub fn add_policy(&mut self, value: Policy) {
        self.policies.push(value);
    }
    pub fn add_proof(&mut self, value: Proof) {
        self.proofs.push(value);
    }
    pub fn add_derivation(&mut self, value: Derivation) {
        self.derivations.push(value);
    }
    pub fn add_fidelity(&mut self, value: Fidelity) {
        self.fidelity.push(value);
    }
    pub fn add_source_capsule(&mut self, value: SourceCapsule) {
        self.source_capsules.push(value);
    }
    pub fn add_observed_execution(&mut self, value: ExecutionRecord) {
        self.observed_execution.push(value);
    }
    pub fn recordings(&self) -> &[Recording] {
        &self.recordings
    }
    pub fn streams(&self) -> &[Stream] {
        &self.streams
    }
    pub fn streams_mut(&mut self) -> &mut [Stream] {
        &mut self.streams
    }
    pub fn atoms(&self) -> &[Atom] {
        &self.atoms
    }
    pub fn atoms_mut(&mut self) -> &mut [Atom] {
        &mut self.atoms
    }
    pub fn clocks_mut(&mut self) -> &mut [Clock] {
        &mut self.clocks
    }
    pub fn coordinate_frames_mut(&mut self) -> &mut [CoordinateFrame] {
        &mut self.coordinate_frames
    }

    pub fn validate(self, limits: ValidationLimits) -> Result<AbirDataset, ValidationReport> {
        let mut report = None;
        check_limit(
            &mut report,
            self.recordings.len(),
            limits.max_recordings,
            "recordings",
        );
        check_limit(
            &mut report,
            self.streams.len(),
            limits.max_streams,
            "streams",
        );
        check_limit(&mut report, self.atoms.len(), limits.max_atoms, "atoms");

        let recording_ids = unique_ids(
            &mut report,
            self.recordings.iter().map(Recording::id),
            "recordings",
        );
        let stream_ids = unique_ids(&mut report, self.streams.iter().map(Stream::id), "streams");
        let atom_ids = unique_ids(&mut report, self.atoms.iter().map(Atom::id), "atoms");
        let clock_ids = unique_ids(&mut report, self.clocks.iter().map(Clock::id), "clocks");
        let frame_ids = unique_ids(
            &mut report,
            self.coordinate_frames.iter().map(CoordinateFrame::id),
            "coordinate_frames",
        );
        let basis_ids = unique_ids(
            &mut report,
            self.channel_bases.iter().map(ChannelBasis::id),
            "channel_bases",
        );
        let policy_ids = unique_ids(
            &mut report,
            self.policies.iter().map(Policy::id),
            "policies",
        );
        drop(unique_ids(
            &mut report,
            self.proofs.iter().map(Proof::id),
            "proofs",
        ));
        drop(unique_ids(
            &mut report,
            self.derivations.iter().map(Derivation::id),
            "derivations",
        ));

        for (index, recording) in self.recordings.iter().enumerate() {
            for stream_id in recording.streams() {
                if !stream_ids.contains(stream_id) {
                    push(
                        &mut report,
                        ValidationFailure::error(
                            FailureCode::DanglingReference,
                            format!("recordings[{index}].streams"),
                        )
                        .with_related_object(stream_id.to_bytes()),
                    );
                }
            }
        }

        for (index, stream) in self.streams.iter().enumerate() {
            if !recording_ids.contains(&stream.recording_id()) {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::DanglingReference,
                        format!("streams[{index}].recording_id"),
                    )
                    .with_related_object(stream.recording_id().to_bytes()),
                );
            } else if !self.recordings.iter().any(|recording| {
                recording.id() == stream.recording_id()
                    && recording.streams().contains(&stream.id())
            }) {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::DanglingReference,
                        format!("streams[{index}].recording_membership"),
                    )
                    .with_related_object(stream.id().to_bytes()),
                );
            }
            for atom_id in stream.atoms() {
                if !atom_ids.contains(atom_id) {
                    push(
                        &mut report,
                        ValidationFailure::error(
                            FailureCode::DanglingReference,
                            format!("streams[{index}].atoms"),
                        )
                        .with_related_object(atom_id.to_bytes()),
                    );
                }
            }
            if let Some(clock_id) = stream.clock_id() {
                if !clock_ids.contains(&clock_id) {
                    push(
                        &mut report,
                        ValidationFailure::error(
                            FailureCode::UnresolvedClock,
                            format!("streams[{index}].clock_id"),
                        )
                        .with_related_object(clock_id.to_bytes()),
                    );
                }
            }
            if let Some(basis_id) = stream.channel_basis_id() {
                if !basis_ids.contains(&basis_id) {
                    push(
                        &mut report,
                        ValidationFailure::error(
                            FailureCode::DanglingReference,
                            format!("streams[{index}].channel_basis_id"),
                        )
                        .with_related_object(basis_id.to_bytes()),
                    );
                }
            }
            if let Some(policy_id) = stream.policy_id() {
                if !policy_ids.contains(&policy_id) {
                    push(
                        &mut report,
                        ValidationFailure::error(
                            FailureCode::DanglingReference,
                            format!("streams[{index}].policy_id"),
                        )
                        .with_related_object(policy_id.to_bytes()),
                    );
                }
            }
        }

        for (index, atom) in self.atoms.iter().enumerate() {
            if !atom.is_structurally_valid(limits) {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::PayloadMismatch,
                        format!("atoms[{index}]"),
                    )
                    .with_related_object(atom.id().to_bytes()),
                );
            }
        }

        for (index, clock) in self.clocks.iter().enumerate() {
            if !clock.rate().is_positive() || clock.uncertainty().parts().0 < 0 {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::InvalidExactNumber,
                        format!("clocks[{index}]"),
                    ),
                );
            }
            if let Some(parent) = clock.parent_id() {
                if !clock_ids.contains(&parent) {
                    push(
                        &mut report,
                        ValidationFailure::error(
                            FailureCode::UnresolvedClock,
                            format!("clocks[{index}].parent_id"),
                        )
                        .with_related_object(parent.to_bytes()),
                    );
                }
            }
        }
        validate_clock_ancestry(&mut report, &self.clocks, limits);

        for (index, frame) in self.coordinate_frames.iter().enumerate() {
            if frame.uncertainty().parts().0 < 0 {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::InvalidExactNumber,
                        format!("coordinate_frames[{index}].uncertainty"),
                    ),
                );
            }
            if let Some(parent) = frame.parent_id() {
                if !frame_ids.contains(&parent) {
                    push(
                        &mut report,
                        ValidationFailure::error(
                            FailureCode::UnresolvedCoordinateFrame,
                            format!("coordinate_frames[{index}].parent_id"),
                        )
                        .with_related_object(parent.to_bytes()),
                    );
                }
            }
        }
        validate_frame_ancestry(&mut report, &self.coordinate_frames, limits);

        for (basis_index, basis) in self.channel_bases.iter().enumerate() {
            if basis.channels().len() > limits.max_channels {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::StructuralLimit,
                        format!("channel_bases[{basis_index}].channels"),
                    ),
                );
            }
            for (channel_index, channel) in basis.channels().iter().enumerate() {
                if let Some(frame_id) = channel.coordinate_frame_id() {
                    if !frame_ids.contains(&frame_id) {
                        push(
                            &mut report,
                            ValidationFailure::error(
                                FailureCode::UnresolvedCoordinateFrame,
                                format!(
                                    "channel_bases[{basis_index}].channels[{channel_index}].coordinate_frame_id"
                                ),
                            )
                            .with_related_object(frame_id.to_bytes()),
                        );
                    }
                }
            }
        }

        validate_policies(&mut report, &self.policies, limits);

        let mut semantic_refs = BTreeSet::new();
        semantic_refs.insert(SemanticRef::of(self.id));
        semantic_refs.extend(
            self.recordings
                .iter()
                .map(|value| SemanticRef::of(value.id())),
        );
        semantic_refs.extend(self.streams.iter().map(|value| SemanticRef::of(value.id())));
        semantic_refs.extend(self.atoms.iter().map(|value| SemanticRef::of(value.id())));
        semantic_refs.extend(self.clocks.iter().map(|value| SemanticRef::of(value.id())));
        semantic_refs.extend(
            self.coordinate_frames
                .iter()
                .map(|value| SemanticRef::of(value.id())),
        );
        semantic_refs.extend(
            self.channel_bases
                .iter()
                .map(|value| SemanticRef::of(value.id())),
        );
        semantic_refs.extend(
            self.policies
                .iter()
                .map(|value| SemanticRef::of(value.id())),
        );
        semantic_refs.extend(self.proofs.iter().map(|value| SemanticRef::of(value.id())));
        semantic_refs.extend(
            self.derivations
                .iter()
                .map(|value| SemanticRef::of(value.id())),
        );

        for (index, proof) in self.proofs.iter().enumerate() {
            if !semantic_refs.contains(&proof.subject()) {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::DanglingReference,
                        format!("proofs[{index}].subject"),
                    ),
                );
            } else if proof_kind_misused(proof) {
                push(
                    &mut report,
                    ValidationFailure::error(FailureCode::ProofMisuse, format!("proofs[{index}]")),
                );
            }
        }

        for (index, derivation) in self.derivations.iter().enumerate() {
            for reference in derivation.inputs().iter().chain(derivation.outputs()) {
                if !semantic_refs.contains(reference) {
                    push(
                        &mut report,
                        ValidationFailure::error(
                            FailureCode::DanglingReference,
                            format!("derivations[{index}]"),
                        ),
                    );
                }
            }
        }

        for (index, statement) in self.fidelity.iter().enumerate() {
            if !semantic_refs.contains(&statement.subject()) {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::DanglingReference,
                        format!("fidelity[{index}].subject"),
                    ),
                );
            }
            let shape_valid = match statement.kind() {
                FidelityKind::Exact => statement.metric().is_none() && statement.bound().is_none(),
                FidelityKind::Bounded => {
                    statement.metric().is_some() && statement.bound().is_some()
                }
                FidelityKind::Transformed => statement.metric().is_some(),
            };
            if !shape_valid {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::InvalidShapeOrExtent,
                        format!("fidelity[{index}]"),
                    ),
                );
            }
        }

        if let Some(report) = report {
            return Err(report);
        }
        Ok(AbirDataset {
            id: self.id,
            recordings: self.recordings,
            streams: self.streams,
            atoms: self.atoms,
            clocks: self.clocks,
            coordinate_frames: self.coordinate_frames,
            channel_bases: self.channel_bases,
            policies: self.policies,
            proofs: self.proofs,
            derivations: self.derivations,
            fidelity: self.fidelity,
            source_capsules: self.source_capsules,
            observed_execution: self.observed_execution,
        })
    }
}

#[derive(Clone, Debug)]
pub struct AbirDataset {
    id: ObjectId<DatasetTag>,
    recordings: Vec<Recording>,
    streams: Vec<Stream>,
    atoms: Vec<Atom>,
    clocks: Vec<Clock>,
    coordinate_frames: Vec<CoordinateFrame>,
    channel_bases: Vec<ChannelBasis>,
    policies: Vec<Policy>,
    proofs: Vec<Proof>,
    derivations: Vec<Derivation>,
    fidelity: Vec<Fidelity>,
    source_capsules: Vec<SourceCapsule>,
    observed_execution: Vec<ExecutionRecord>,
}

impl AbirDataset {
    pub const fn id(&self) -> ObjectId<DatasetTag> {
        self.id
    }
    pub fn recordings(&self) -> &[Recording] {
        &self.recordings
    }
    pub fn streams(&self) -> &[Stream] {
        &self.streams
    }
    pub fn atoms(&self) -> &[Atom] {
        &self.atoms
    }
    pub fn clocks(&self) -> &[Clock] {
        &self.clocks
    }
    pub fn coordinate_frames(&self) -> &[CoordinateFrame] {
        &self.coordinate_frames
    }
    pub fn channel_bases(&self) -> &[ChannelBasis] {
        &self.channel_bases
    }
    pub fn policies(&self) -> &[Policy] {
        &self.policies
    }
    pub fn proofs(&self) -> &[Proof] {
        &self.proofs
    }
    pub fn derivations(&self) -> &[Derivation] {
        &self.derivations
    }
    pub fn fidelity(&self) -> &[Fidelity] {
        &self.fidelity
    }
    pub fn source_capsules(&self) -> &[SourceCapsule] {
        &self.source_capsules
    }
    pub fn observed_execution(&self) -> &[ExecutionRecord] {
        &self.observed_execution
    }
    pub fn payload_content_ids(&self) -> Vec<ContentId> {
        self.atoms
            .iter()
            .filter_map(Atom::payload)
            .map(|payload| payload.content_id())
            .collect()
    }
}

fn push(report: &mut Option<ValidationReport>, failure: ValidationFailure) {
    match report {
        Some(report) => report.push(failure),
        None => *report = Some(ValidationReport::new(failure)),
    }
}

fn check_limit(report: &mut Option<ValidationReport>, actual: usize, maximum: usize, path: &str) {
    if actual > maximum {
        push(
            report,
            ValidationFailure::error(FailureCode::StructuralLimit, path),
        );
    }
}

fn unique_ids<T>(
    report: &mut Option<ValidationReport>,
    ids: impl Iterator<Item = ObjectId<T>>,
    path: &str,
) -> BTreeSet<ObjectId<T>> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            push(
                report,
                ValidationFailure::error(FailureCode::DuplicateId, path)
                    .with_related_object(id.to_bytes()),
            );
        }
    }
    seen
}

fn validate_clock_ancestry(
    report: &mut Option<ValidationReport>,
    clocks: &[Clock],
    limits: ValidationLimits,
) {
    for (index, clock) in clocks.iter().enumerate() {
        let mut seen = BTreeSet::new();
        let mut current = Some(clock.id());
        let mut depth = 0_usize;
        while let Some(id) = current {
            if !seen.insert(id) {
                push(
                    report,
                    ValidationFailure::error(
                        FailureCode::UnresolvedClock,
                        format!("clocks[{index}].ancestry_cycle"),
                    )
                    .with_related_object(id.to_bytes()),
                );
                break;
            }
            depth += 1;
            if depth > limits.max_nesting_depth {
                push(
                    report,
                    ValidationFailure::error(
                        FailureCode::StructuralLimit,
                        format!("clocks[{index}].ancestry_depth"),
                    ),
                );
                break;
            }
            current = clocks
                .iter()
                .find(|candidate| candidate.id() == id)
                .and_then(Clock::parent_id);
        }
    }
}

fn validate_frame_ancestry(
    report: &mut Option<ValidationReport>,
    frames: &[CoordinateFrame],
    limits: ValidationLimits,
) {
    for (index, frame) in frames.iter().enumerate() {
        let mut seen = BTreeSet::new();
        let mut current = Some(frame.id());
        let mut depth = 0_usize;
        while let Some(id) = current {
            if !seen.insert(id) {
                push(
                    report,
                    ValidationFailure::error(
                        FailureCode::UnresolvedCoordinateFrame,
                        format!("coordinate_frames[{index}].ancestry_cycle"),
                    )
                    .with_related_object(id.to_bytes()),
                );
                break;
            }
            depth += 1;
            if depth > limits.max_nesting_depth {
                push(
                    report,
                    ValidationFailure::error(
                        FailureCode::StructuralLimit,
                        format!("coordinate_frames[{index}].ancestry_depth"),
                    ),
                );
                break;
            }
            current = frames
                .iter()
                .find(|candidate| candidate.id() == id)
                .and_then(CoordinateFrame::parent_id);
        }
    }
}

fn validate_policies(
    report: &mut Option<ValidationReport>,
    policies: &[Policy],
    limits: ValidationLimits,
) {
    let ids: BTreeSet<_> = policies.iter().map(Policy::id).collect();
    for (index, policy) in policies.iter().enumerate() {
        if let Some(parent_id) = policy.parent_id() {
            let Some(parent) = policies
                .iter()
                .find(|candidate| candidate.id() == parent_id)
            else {
                push(
                    report,
                    ValidationFailure::error(
                        FailureCode::DanglingReference,
                        format!("policies[{index}].parent_id"),
                    )
                    .with_related_object(parent_id.to_bytes()),
                );
                continue;
            };
            if !parent
                .restrictions()
                .iter()
                .all(|restriction| policy.restrictions().contains(restriction))
            {
                push(
                    report,
                    ValidationFailure::error(
                        FailureCode::PolicyRelaxation,
                        format!("policies[{index}].restrictions"),
                    ),
                );
            }
        }

        let mut seen = BTreeSet::new();
        let mut current = Some(policy.id());
        let mut depth = 0_usize;
        while let Some(id) = current {
            if !seen.insert(id) {
                push(
                    report,
                    ValidationFailure::error(
                        FailureCode::PolicyRelaxation,
                        format!("policies[{index}].ancestry_cycle"),
                    ),
                );
                break;
            }
            depth += 1;
            if depth > limits.max_nesting_depth {
                push(
                    report,
                    ValidationFailure::error(
                        FailureCode::StructuralLimit,
                        format!("policies[{index}].ancestry_depth"),
                    ),
                );
                break;
            }
            current = policies
                .iter()
                .find(|candidate| candidate.id() == id)
                .and_then(Policy::parent_id);
            if current.is_some_and(|parent| !ids.contains(&parent)) {
                break;
            }
        }
    }
}

fn proof_kind_misused(proof: &Proof) -> bool {
    match proof.kind().as_str() {
        "abir:proof/derivation" => proof.subject().kind() != ObjectKind::Derivation,
        "abir:proof/policy-attestation" => proof.subject().kind() != ObjectKind::Policy,
        "abir:proof/content-integrity" | "abir:proof/fidelity-bound" => false,
        _ => false,
    }
}
