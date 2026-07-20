use crate::{
    Acquisition, AcquisitionTag, Atom, Channel, ChannelBasis, ChannelTag, Clock, ClockRelation,
    ClockRelationTag, ConceptDictionary, ConceptDictionaryTag, ContentId, CoordinateFrame,
    DatasetTag, Derivation, DerivedArtifact, DerivedArtifactTag, Device, DeviceTag, Event,
    EventTag, ExecutionRecord, FailureCode, Fidelity, FidelityKind, FrameTransform,
    FrameTransformTag, Layout, ObjectId, ObjectKind, Patient, PatientTag, Policy, Proof, Recording,
    SemanticRef, Sensor, SensorTag, Session, SessionTag, SourceCapsule, SourceRelationship, Stream,
    Subject, SubjectTag, TimeAxis, ValidationFailure, ValidationLimits, ValidationReport,
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
    subjects: Vec<Subject>,
    patients: Vec<Patient>,
    sessions: Vec<Session>,
    acquisitions: Vec<Acquisition>,
    devices: Vec<Device>,
    sensors: Vec<Sensor>,
    channels: Vec<Channel>,
    clock_relations: Vec<ClockRelation>,
    frame_transforms: Vec<FrameTransform>,
    events: Vec<Event>,
    concept_dictionaries: Vec<ConceptDictionary>,
    derived_artifacts: Vec<DerivedArtifact>,
    source_relationships: Vec<SourceRelationship>,
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
            subjects: Vec::new(),
            patients: Vec::new(),
            sessions: Vec::new(),
            acquisitions: Vec::new(),
            devices: Vec::new(),
            sensors: Vec::new(),
            channels: Vec::new(),
            clock_relations: Vec::new(),
            frame_transforms: Vec::new(),
            events: Vec::new(),
            concept_dictionaries: Vec::new(),
            derived_artifacts: Vec::new(),
            source_relationships: Vec::new(),
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
    pub fn add_subject(&mut self, value: Subject) {
        self.subjects.push(value);
    }
    pub fn add_patient(&mut self, value: Patient) {
        self.patients.push(value);
    }
    pub fn add_session(&mut self, value: Session) {
        self.sessions.push(value);
    }
    pub fn add_acquisition(&mut self, value: Acquisition) {
        self.acquisitions.push(value);
    }
    pub fn add_device(&mut self, value: Device) {
        self.devices.push(value);
    }
    pub fn add_sensor(&mut self, value: Sensor) {
        self.sensors.push(value);
    }
    pub fn add_channel(&mut self, value: Channel) {
        self.channels.push(value);
    }
    pub fn add_clock_relation(&mut self, value: ClockRelation) {
        self.clock_relations.push(value);
    }
    pub fn add_frame_transform(&mut self, value: FrameTransform) {
        self.frame_transforms.push(value);
    }
    pub fn add_event(&mut self, value: Event) {
        self.events.push(value);
    }
    pub fn add_concept_dictionary(&mut self, value: ConceptDictionary) {
        self.concept_dictionaries.push(value);
    }
    pub fn add_derived_artifact(&mut self, value: DerivedArtifact) {
        self.derived_artifacts.push(value);
    }
    pub fn add_source_relationship(&mut self, value: SourceRelationship) {
        self.source_relationships.push(value);
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
    pub fn subjects(&self) -> &[Subject] {
        &self.subjects
    }
    pub fn subject(&self, id: ObjectId<SubjectTag>) -> Option<&Subject> {
        self.subjects.iter().find(|value| value.id() == id)
    }
    pub fn patients(&self) -> &[Patient] {
        &self.patients
    }
    pub fn patient(&self, id: ObjectId<PatientTag>) -> Option<&Patient> {
        self.patients.iter().find(|value| value.id() == id)
    }
    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }
    pub fn session(&self, id: ObjectId<SessionTag>) -> Option<&Session> {
        self.sessions.iter().find(|value| value.id() == id)
    }
    pub fn acquisitions(&self) -> &[Acquisition] {
        &self.acquisitions
    }
    pub fn acquisition(&self, id: ObjectId<AcquisitionTag>) -> Option<&Acquisition> {
        self.acquisitions.iter().find(|value| value.id() == id)
    }
    pub fn devices(&self) -> &[Device] {
        &self.devices
    }
    pub fn device(&self, id: ObjectId<DeviceTag>) -> Option<&Device> {
        self.devices.iter().find(|value| value.id() == id)
    }
    pub fn sensors(&self) -> &[Sensor] {
        &self.sensors
    }
    pub fn sensor(&self, id: ObjectId<SensorTag>) -> Option<&Sensor> {
        self.sensors.iter().find(|value| value.id() == id)
    }
    pub fn channels(&self) -> &[Channel] {
        &self.channels
    }
    pub fn channel(&self, id: ObjectId<ChannelTag>) -> Option<&Channel> {
        self.channels.iter().find(|value| value.id() == id)
    }
    pub fn clock_relations(&self) -> &[ClockRelation] {
        &self.clock_relations
    }
    pub fn clock_relation(&self, id: ObjectId<ClockRelationTag>) -> Option<&ClockRelation> {
        self.clock_relations.iter().find(|value| value.id() == id)
    }
    pub fn frame_transforms(&self) -> &[FrameTransform] {
        &self.frame_transforms
    }
    pub fn frame_transform(&self, id: ObjectId<FrameTransformTag>) -> Option<&FrameTransform> {
        self.frame_transforms.iter().find(|value| value.id() == id)
    }
    pub fn events(&self) -> &[Event] {
        &self.events
    }
    pub fn event(&self, id: ObjectId<EventTag>) -> Option<&Event> {
        self.events.iter().find(|value| value.id() == id)
    }
    pub fn concept_dictionaries(&self) -> &[ConceptDictionary] {
        &self.concept_dictionaries
    }
    pub fn concept_dictionary(
        &self,
        id: ObjectId<ConceptDictionaryTag>,
    ) -> Option<&ConceptDictionary> {
        self.concept_dictionaries
            .iter()
            .find(|value| value.id() == id)
    }
    pub fn derived_artifacts(&self) -> &[DerivedArtifact] {
        &self.derived_artifacts
    }
    pub fn derived_artifact(&self, id: ObjectId<DerivedArtifactTag>) -> Option<&DerivedArtifact> {
        self.derived_artifacts.iter().find(|value| value.id() == id)
    }
    pub fn source_relationships(&self) -> &[SourceRelationship] {
        &self.source_relationships
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
        let catalog_records = [
            self.subjects.len(),
            self.patients.len(),
            self.sessions.len(),
            self.acquisitions.len(),
            self.devices.len(),
            self.sensors.len(),
            self.channels.len(),
            self.concept_dictionaries.len(),
        ]
        .into_iter()
        .try_fold(0_usize, usize::checked_add)
        .unwrap_or(usize::MAX);
        check_limit(
            &mut report,
            catalog_records,
            limits.max_catalog_records,
            "catalog_records",
        );
        check_limit(
            &mut report,
            self.source_relationships.len(),
            limits.max_relationships,
            "source_relationships",
        );
        let governance_records = [
            self.policies.len(),
            self.proofs.len(),
            self.derivations.len(),
            self.fidelity.len(),
            self.source_capsules.len(),
        ]
        .into_iter()
        .try_fold(0_usize, usize::checked_add)
        .unwrap_or(usize::MAX);
        check_limit(
            &mut report,
            governance_records,
            limits.max_governance_records,
            "governance_records",
        );

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
        let derivation_ids = unique_ids(
            &mut report,
            self.derivations.iter().map(Derivation::id),
            "derivations",
        );
        let subject_ids = unique_ids(
            &mut report,
            self.subjects.iter().map(Subject::id),
            "subjects",
        );
        let patient_ids = unique_ids(
            &mut report,
            self.patients.iter().map(Patient::id),
            "patients",
        );
        let session_ids = unique_ids(
            &mut report,
            self.sessions.iter().map(Session::id),
            "sessions",
        );
        let acquisition_ids = unique_ids(
            &mut report,
            self.acquisitions.iter().map(Acquisition::id),
            "acquisitions",
        );
        let device_ids = unique_ids(&mut report, self.devices.iter().map(Device::id), "devices");
        let sensor_ids = unique_ids(&mut report, self.sensors.iter().map(Sensor::id), "sensors");
        let channel_ids = unique_ids(
            &mut report,
            self.channels.iter().map(Channel::id),
            "channels",
        );
        drop(unique_ids(
            &mut report,
            self.clock_relations.iter().map(ClockRelation::id),
            "clock_relations",
        ));
        drop(unique_ids(
            &mut report,
            self.frame_transforms.iter().map(FrameTransform::id),
            "frame_transforms",
        ));
        drop(unique_ids(
            &mut report,
            self.events.iter().map(Event::id),
            "events",
        ));
        drop(unique_ids(
            &mut report,
            self.concept_dictionaries.iter().map(ConceptDictionary::id),
            "concept_dictionaries",
        ));
        drop(unique_ids(
            &mut report,
            self.derived_artifacts.iter().map(DerivedArtifact::id),
            "derived_artifacts",
        ));

        for (index, relationship) in self.source_relationships.iter().enumerate() {
            let resolved = match relationship {
                SourceRelationship::PatientSubject {
                    patient_id,
                    subject_id,
                } => patient_ids.contains(patient_id) && subject_ids.contains(subject_id),
                SourceRelationship::SessionSubject {
                    session_id,
                    subject_id,
                } => session_ids.contains(session_id) && subject_ids.contains(subject_id),
                SourceRelationship::SessionPatient {
                    session_id,
                    patient_id,
                } => session_ids.contains(session_id) && patient_ids.contains(patient_id),
                SourceRelationship::AcquisitionSession {
                    acquisition_id,
                    session_id,
                } => acquisition_ids.contains(acquisition_id) && session_ids.contains(session_id),
                SourceRelationship::AcquisitionDevice {
                    acquisition_id,
                    device_id,
                } => acquisition_ids.contains(acquisition_id) && device_ids.contains(device_id),
                SourceRelationship::DeviceSensor {
                    device_id,
                    sensor_id,
                } => device_ids.contains(device_id) && sensor_ids.contains(sensor_id),
                SourceRelationship::SensorChannel {
                    sensor_id,
                    channel_id,
                } => sensor_ids.contains(sensor_id) && channel_ids.contains(channel_id),
                SourceRelationship::AcquisitionRecording {
                    acquisition_id,
                    recording_id,
                } => {
                    acquisition_ids.contains(acquisition_id) && recording_ids.contains(recording_id)
                }
                SourceRelationship::ChannelBasisMember {
                    channel_id,
                    basis_id,
                    position,
                } => {
                    channel_ids.contains(channel_id)
                        && basis_ids.contains(basis_id)
                        && self.channel_bases.iter().any(|basis| {
                            basis.id() == *basis_id
                                && usize::try_from(*position)
                                    .is_ok_and(|position| position < basis.channels().len())
                        })
                }
            };
            if !resolved {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::DanglingReference,
                        format!("source_relationships[{index}]"),
                    ),
                );
            }
        }

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

        let payload_content_ids: BTreeSet<_> = self
            .atoms
            .iter()
            .filter_map(Atom::payload)
            .map(|payload| payload.content_id())
            .collect();
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
            if let Atom::TemporalTable(table) = atom {
                if !clock_ids.contains(&table.clock_id()) {
                    push(
                        &mut report,
                        ValidationFailure::error(
                            FailureCode::UnresolvedClock,
                            format!("atoms[{index}].clock_id"),
                        )
                        .with_related_object(table.clock_id().to_bytes()),
                    );
                }
            }
            let companion_ids = atom_companion_content_ids(atom);
            let mut companions_resolve = true;
            for content_id in companion_ids {
                if !payload_content_ids.contains(&content_id) {
                    companions_resolve = false;
                    push(
                        &mut report,
                        ValidationFailure::error(
                            FailureCode::DanglingReference,
                            format!("atoms[{index}].payload.companion"),
                        )
                        .with_evidence(alloc::vec![content_id]),
                    );
                }
            }
            if companions_resolve && !atom_companion_semantics_are_valid(atom, &self.atoms) {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::PayloadMismatch,
                        format!("atoms[{index}].payload.companion"),
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

        for (index, relation) in self.clock_relations.iter().enumerate() {
            for (field, clock_id) in [
                ("from_clock_id", relation.from_clock_id()),
                ("to_clock_id", relation.to_clock_id()),
            ] {
                if !clock_ids.contains(&clock_id) {
                    push(
                        &mut report,
                        ValidationFailure::error(
                            FailureCode::UnresolvedClock,
                            format!("clock_relations[{index}].{field}"),
                        )
                        .with_related_object(clock_id.to_bytes()),
                    );
                }
            }
            if !relation.rate().is_positive() || relation.uncertainty().parts().0 < 0 {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::InvalidExactNumber,
                        format!("clock_relations[{index}]"),
                    ),
                );
            }
            if relation
                .validity_end()
                .is_some_and(|end| rational_order(end, relation.validity_start()).is_lt())
            {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::InvalidExactNumber,
                        format!("clock_relations[{index}].validity"),
                    ),
                );
            }
        }

        for (index, transform) in self.frame_transforms.iter().enumerate() {
            for (field, frame_id) in [
                ("from_frame_id", transform.from_frame_id()),
                ("to_frame_id", transform.to_frame_id()),
            ] {
                if !frame_ids.contains(&frame_id) {
                    push(
                        &mut report,
                        ValidationFailure::error(
                            FailureCode::UnresolvedCoordinateFrame,
                            format!("frame_transforms[{index}].{field}"),
                        )
                        .with_related_object(frame_id.to_bytes()),
                    );
                }
            }
            if transform.uncertainty().parts().0 < 0 {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::InvalidExactNumber,
                        format!("frame_transforms[{index}].uncertainty"),
                    ),
                );
            }
        }

        for (index, event) in self.events.iter().enumerate() {
            if !clock_ids.contains(&event.clock_id()) {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::UnresolvedClock,
                        format!("events[{index}].clock_id"),
                    )
                    .with_related_object(event.clock_id().to_bytes()),
                );
            }
            if !matches!(
                rational_order(event.end(), event.start()),
                core::cmp::Ordering::Equal | core::cmp::Ordering::Greater
            ) || event.uncertainty().parts().0 < 0
            {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::InvalidExactNumber,
                        format!("events[{index}].interval"),
                    ),
                );
            }
        }

        for (index, artifact) in self.derived_artifacts.iter().enumerate() {
            let derivation = self
                .derivations
                .iter()
                .find(|value| value.id() == artifact.derivation_id());
            if !derivation_ids.contains(&artifact.derivation_id()) {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::DanglingReference,
                        format!("derived_artifacts[{index}].derivation_id"),
                    )
                    .with_related_object(artifact.derivation_id().to_bytes()),
                );
            } else if !derivation
                .is_some_and(|value| value.outputs().contains(&SemanticRef::of(artifact.id())))
            {
                push(
                    &mut report,
                    ValidationFailure::error(
                        FailureCode::DanglingReference,
                        format!("derived_artifacts[{index}].derivation_output"),
                    )
                    .with_related_object(artifact.id().to_bytes()),
                );
            }
        }

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
        semantic_refs.extend(
            self.subjects
                .iter()
                .map(|value| SemanticRef::of(value.id())),
        );
        semantic_refs.extend(
            self.patients
                .iter()
                .map(|value| SemanticRef::of(value.id())),
        );
        semantic_refs.extend(
            self.sessions
                .iter()
                .map(|value| SemanticRef::of(value.id())),
        );
        semantic_refs.extend(
            self.acquisitions
                .iter()
                .map(|value| SemanticRef::of(value.id())),
        );
        semantic_refs.extend(self.devices.iter().map(|value| SemanticRef::of(value.id())));
        semantic_refs.extend(self.sensors.iter().map(|value| SemanticRef::of(value.id())));
        semantic_refs.extend(
            self.channels
                .iter()
                .map(|value| SemanticRef::of(value.id())),
        );
        semantic_refs.extend(
            self.clock_relations
                .iter()
                .map(|value| SemanticRef::of(value.id())),
        );
        semantic_refs.extend(
            self.frame_transforms
                .iter()
                .map(|value| SemanticRef::of(value.id())),
        );
        semantic_refs.extend(self.events.iter().map(|value| SemanticRef::of(value.id())));
        semantic_refs.extend(
            self.concept_dictionaries
                .iter()
                .map(|value| SemanticRef::of(value.id())),
        );
        semantic_refs.extend(
            self.derived_artifacts
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
            subjects: self.subjects,
            patients: self.patients,
            sessions: self.sessions,
            acquisitions: self.acquisitions,
            devices: self.devices,
            sensors: self.sensors,
            channels: self.channels,
            clock_relations: self.clock_relations,
            frame_transforms: self.frame_transforms,
            events: self.events,
            concept_dictionaries: self.concept_dictionaries,
            derived_artifacts: self.derived_artifacts,
            source_relationships: self.source_relationships,
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
    subjects: Vec<Subject>,
    patients: Vec<Patient>,
    sessions: Vec<Session>,
    acquisitions: Vec<Acquisition>,
    devices: Vec<Device>,
    sensors: Vec<Sensor>,
    channels: Vec<Channel>,
    clock_relations: Vec<ClockRelation>,
    frame_transforms: Vec<FrameTransform>,
    events: Vec<Event>,
    concept_dictionaries: Vec<ConceptDictionary>,
    derived_artifacts: Vec<DerivedArtifact>,
    source_relationships: Vec<SourceRelationship>,
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
    pub fn subjects(&self) -> &[Subject] {
        &self.subjects
    }
    pub fn subject(&self, id: ObjectId<SubjectTag>) -> Option<&Subject> {
        self.subjects.iter().find(|value| value.id() == id)
    }
    pub fn patients(&self) -> &[Patient] {
        &self.patients
    }
    pub fn patient(&self, id: ObjectId<PatientTag>) -> Option<&Patient> {
        self.patients.iter().find(|value| value.id() == id)
    }
    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }
    pub fn session(&self, id: ObjectId<SessionTag>) -> Option<&Session> {
        self.sessions.iter().find(|value| value.id() == id)
    }
    pub fn acquisitions(&self) -> &[Acquisition] {
        &self.acquisitions
    }
    pub fn acquisition(&self, id: ObjectId<AcquisitionTag>) -> Option<&Acquisition> {
        self.acquisitions.iter().find(|value| value.id() == id)
    }
    pub fn devices(&self) -> &[Device] {
        &self.devices
    }
    pub fn device(&self, id: ObjectId<DeviceTag>) -> Option<&Device> {
        self.devices.iter().find(|value| value.id() == id)
    }
    pub fn sensors(&self) -> &[Sensor] {
        &self.sensors
    }
    pub fn sensor(&self, id: ObjectId<SensorTag>) -> Option<&Sensor> {
        self.sensors.iter().find(|value| value.id() == id)
    }
    pub fn channels(&self) -> &[Channel] {
        &self.channels
    }
    pub fn channel(&self, id: ObjectId<ChannelTag>) -> Option<&Channel> {
        self.channels.iter().find(|value| value.id() == id)
    }
    pub fn clock_relations(&self) -> &[ClockRelation] {
        &self.clock_relations
    }
    pub fn clock_relation(&self, id: ObjectId<ClockRelationTag>) -> Option<&ClockRelation> {
        self.clock_relations.iter().find(|value| value.id() == id)
    }
    pub fn frame_transforms(&self) -> &[FrameTransform] {
        &self.frame_transforms
    }
    pub fn frame_transform(&self, id: ObjectId<FrameTransformTag>) -> Option<&FrameTransform> {
        self.frame_transforms.iter().find(|value| value.id() == id)
    }
    pub fn events(&self) -> &[Event] {
        &self.events
    }
    pub fn event(&self, id: ObjectId<EventTag>) -> Option<&Event> {
        self.events.iter().find(|value| value.id() == id)
    }
    pub fn concept_dictionaries(&self) -> &[ConceptDictionary] {
        &self.concept_dictionaries
    }
    pub fn concept_dictionary(
        &self,
        id: ObjectId<ConceptDictionaryTag>,
    ) -> Option<&ConceptDictionary> {
        self.concept_dictionaries
            .iter()
            .find(|value| value.id() == id)
    }
    pub fn derived_artifacts(&self) -> &[DerivedArtifact] {
        &self.derived_artifacts
    }
    pub fn derived_artifact(&self, id: ObjectId<DerivedArtifactTag>) -> Option<&DerivedArtifact> {
        self.derived_artifacts.iter().find(|value| value.id() == id)
    }
    pub fn source_relationships(&self) -> &[SourceRelationship] {
        &self.source_relationships
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

fn rational_order(left: crate::Rational, right: crate::Rational) -> core::cmp::Ordering {
    let (left_numerator, left_denominator) = left.parts();
    let (right_numerator, right_denominator) = right.parts();
    match (left_numerator.is_negative(), right_numerator.is_negative()) {
        (true, false) => core::cmp::Ordering::Less,
        (false, true) => core::cmp::Ordering::Greater,
        (false, false) => compare_unsigned_fractions(
            left_numerator as u128,
            left_denominator as u128,
            right_numerator as u128,
            right_denominator as u128,
        ),
        (true, true) => compare_unsigned_fractions(
            left_numerator.unsigned_abs(),
            left_denominator as u128,
            right_numerator.unsigned_abs(),
            right_denominator as u128,
        )
        .reverse(),
    }
}

fn compare_unsigned_fractions(
    mut left_numerator: u128,
    mut left_denominator: u128,
    mut right_numerator: u128,
    mut right_denominator: u128,
) -> core::cmp::Ordering {
    let mut reverse = false;
    loop {
        let quotient_order =
            (left_numerator / left_denominator).cmp(&(right_numerator / right_denominator));
        if quotient_order != core::cmp::Ordering::Equal {
            return if reverse {
                quotient_order.reverse()
            } else {
                quotient_order
            };
        }
        let left_remainder = left_numerator % left_denominator;
        let right_remainder = right_numerator % right_denominator;
        if left_remainder == 0 || right_remainder == 0 {
            let remainder_order = left_remainder.cmp(&right_remainder);
            return if reverse {
                remainder_order.reverse()
            } else {
                remainder_order
            };
        }
        left_numerator = left_denominator;
        left_denominator = left_remainder;
        right_numerator = right_denominator;
        right_denominator = right_remainder;
        reverse = !reverse;
    }
}

fn atom_companion_content_ids(atom: &Atom) -> Vec<ContentId> {
    let mut ids = Vec::new();
    if let Some(payload) = atom.payload() {
        match payload.layout() {
            Layout::DenseRowMajor | Layout::DenseColumnMajor => {}
            Layout::Ragged { offsets, .. } => ids.push(*offsets),
            Layout::SparseCoo { indices, .. } => ids.push(*indices),
            Layout::SparseCsr {
                indptr, indices, ..
            } => {
                ids.push(*indptr);
                ids.push(*indices);
            }
            Layout::BlockFloatingPoint { scales, .. } => ids.push(*scales),
        }
    }
    if let Atom::SignalBlock(block) = atom {
        if let TimeAxis::Explicit { timestamps, .. } = block.time_axis() {
            ids.push(*timestamps);
        }
    }
    ids
}

fn atom_companion_semantics_are_valid(atom: &Atom, atoms: &[Atom]) -> bool {
    let descriptor_matches =
        |content_id: ContentId, predicate: &dyn Fn(&crate::PayloadDescriptor) -> bool| {
            let mut found = false;
            let all_match = atoms
                .iter()
                .filter_map(Atom::payload)
                .filter(|payload| payload.content_id() == content_id)
                .all(|payload| {
                    found = true;
                    predicate(payload)
                });
            found && all_match
        };

    if let Atom::SignalBlock(block) = atom {
        if let TimeAxis::Explicit { timestamps, count } = block.time_axis() {
            if !descriptor_matches(*timestamps, &|payload| {
                is_dense(payload.layout())
                    && payload.element() == crate::ElementType::I64
                    && payload.shape() == [*count]
            }) {
                return false;
            }
        }
    }

    let Some(payload) = atom.payload() else {
        return true;
    };
    match payload.layout() {
        Layout::DenseRowMajor | Layout::DenseColumnMajor => true,
        Layout::Ragged { rows, offsets } => rows.checked_add(1).is_some_and(|extent| {
            descriptor_matches(*offsets, &|companion| {
                is_dense(companion.layout())
                    && is_integer(companion.element())
                    && companion.shape() == [extent]
            })
        }),
        Layout::SparseCoo { nonzero, indices } => {
            let rank = u64::try_from(payload.shape().len()).ok();
            rank.is_some_and(|rank| {
                descriptor_matches(*indices, &|companion| {
                    is_dense(companion.layout())
                        && is_integer(companion.element())
                        && companion.shape() == [*nonzero, rank]
                })
            })
        }
        Layout::SparseCsr {
            nonzero,
            indptr,
            indices,
        } => payload.shape().first().copied().is_some_and(|rows| {
            rows.checked_add(1).is_some_and(|indptr_extent| {
                descriptor_matches(*indptr, &|companion| {
                    is_dense(companion.layout())
                        && is_integer(companion.element())
                        && companion.shape() == [indptr_extent]
                }) && descriptor_matches(*indices, &|companion| {
                    is_dense(companion.layout())
                        && is_integer(companion.element())
                        && companion.shape() == [*nonzero]
                })
            })
        }),
        Layout::BlockFloatingPoint {
            block_len, scales, ..
        } => payload
            .shape()
            .iter()
            .try_fold(1_u64, |count, extent| count.checked_mul(*extent))
            .and_then(|elements| elements.checked_add(u64::from(*block_len) - 1))
            .map(|rounded| rounded / u64::from(*block_len))
            .is_some_and(|blocks| {
                descriptor_matches(*scales, &|companion| {
                    is_dense(companion.layout())
                        && matches!(
                            companion.element(),
                            crate::ElementType::F16
                                | crate::ElementType::F32
                                | crate::ElementType::F64
                        )
                        && companion.shape() == [blocks]
                })
            }),
    }
}

fn is_dense(layout: &Layout) -> bool {
    matches!(layout, Layout::DenseRowMajor | Layout::DenseColumnMajor)
}

fn is_integer(element: crate::ElementType) -> bool {
    matches!(
        element,
        crate::ElementType::I8
            | crate::ElementType::I16
            | crate::ElementType::I24
            | crate::ElementType::I32
            | crate::ElementType::I64
            | crate::ElementType::U8
            | crate::ElementType::U16
            | crate::ElementType::U32
            | crate::ElementType::U64
    )
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
