use crate::{
    canonical_debug_json, AbirDataset, AcquisitionTag, Atom, AtomTag, BlobIntegrity, BlobRef,
    ByteOrder, Calibration, CatalogRecord, ChannelBasis, ChannelBasisTag, ChannelSpec, ChannelTag,
    Clock, ClockRelation, ClockRelationTag, ClockTag, ConceptDictionaryTag, ConceptId, ContentId,
    CoordinateFrame, CoordinateFrameTag, DatasetDraft, DatasetTag, DecodedSemantics, Derivation,
    DerivationTag, DerivedArtifact, DerivedArtifactTag, DeviceTag, ElementType, EncodedBlock,
    Event, EventTag, ExactNumber, ExecutionRecord, Fidelity, FidelityKind, FrameTransform,
    FrameTransformTag, Layout, ObjectId, PatientTag, PayloadDescriptor, Policy, PolicyTag,
    Presence, Proof, ProofTag, Rational, Recording, RecordingTag, ReferenceKind, SemanticAxis,
    SemanticRef, SemanticTag, SensorTag, SessionTag, SignalBlock, SourceCapsule, SourceKey,
    SourceRelationship, Stream, StreamTag, SubjectTag, Table, TableColumn, TemporalTable, Tensor,
    TimeAxis, TimeSegment, ValidationLimits,
};
use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use serde_json::{Map, Value};

/// Failure to parse or validate a canonical semantic-v1 dataset document.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CanonicalParseError {
    /// JSON syntax, shape, exact-value, or canonical-form failure.
    Document {
        path: String,
        message: String,
        display_path: bool,
    },
    /// Typed dataset construction reached the semantic verifier and failed.
    Validation(crate::ValidationReport),
}

impl CanonicalParseError {
    fn new(path: &str, message: impl Into<String>) -> Self {
        Self::Document {
            path: path.to_owned(),
            message: message.into(),
            display_path: true,
        }
    }

    fn message_only(path: &str, message: impl Into<String>) -> Self {
        Self::Document {
            path: path.to_owned(),
            message: message.into(),
            display_path: false,
        }
    }

    /// JSON path at which the failure was detected.
    pub fn path(&self) -> &str {
        match self {
            Self::Document { path, .. } => path,
            Self::Validation(report) => report
                .failures()
                .first()
                .map_or("$", crate::ValidationFailure::path),
        }
    }

    /// Stable human-readable document failure description. Semantic
    /// validation retains structured failures instead.
    pub fn document_message(&self) -> Option<&str> {
        match self {
            Self::Document { message, .. } => Some(message),
            Self::Validation(_) => None,
        }
    }

    /// Complete structured semantic validation report, when construction
    /// reached the `DatasetDraft::validate` boundary.
    pub fn validation_report(&self) -> Option<&crate::ValidationReport> {
        match self {
            Self::Document { .. } => None,
            Self::Validation(report) => Some(report),
        }
    }
}

impl fmt::Display for CanonicalParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document {
                path,
                message,
                display_path,
            } if *display_path => write!(formatter, "{message} at {path}"),
            Self::Document { message, .. } => formatter.write_str(message),
            Self::Validation(report) => {
                for (index, failure) in report.failures().iter().enumerate() {
                    if index != 0 {
                        formatter.write_str("; ")?;
                    }
                    write!(formatter, "{} at {}", failure.code(), failure.path())?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CanonicalParseError {}

type ParseResult<T> = Result<T, CanonicalParseError>;

/// Parse a complete canonical semantic-v1 document through ABIR's typed
/// construction and validation boundary.
pub fn parse_canonical_dataset(document: &[u8]) -> ParseResult<AbirDataset> {
    parse_canonical_dataset_with_limits(document, ValidationLimits::default())
}

/// Parse canonical semantic-v1 using a caller-selected structural limit
/// profile.
pub fn parse_canonical_dataset_with_limits(
    document: &[u8],
    limits: ValidationLimits,
) -> ParseResult<AbirDataset> {
    let document_value: Value = serde_json::from_slice(document).map_err(|error| {
        CanonicalParseError::message_only("$", format!("invalid JSON: {error}"))
    })?;
    let root = object(&document_value, "$")?;
    if string(field(root, "semantic_version", "$")?, "$.semantic_version")? != "1" {
        return Err(parse_error(
            "$.semantic_version",
            "expected semantic version 1",
        ));
    }
    let mut draft = DatasetDraft::new(parse_id(field(root, "dataset_id", "$")?, "$.dataset_id")?);

    for (index, value) in values(root, "subjects")?.iter().enumerate() {
        draft.add_subject(parse_catalog::<SubjectTag>(
            value,
            &format!("$.subjects[{index}]"),
        )?);
    }
    for (index, value) in values(root, "patients")?.iter().enumerate() {
        draft.add_patient(parse_catalog::<PatientTag>(
            value,
            &format!("$.patients[{index}]"),
        )?);
    }
    for (index, value) in values(root, "sessions")?.iter().enumerate() {
        draft.add_session(parse_catalog::<SessionTag>(
            value,
            &format!("$.sessions[{index}]"),
        )?);
    }
    for (index, value) in values(root, "acquisitions")?.iter().enumerate() {
        draft.add_acquisition(parse_catalog::<AcquisitionTag>(
            value,
            &format!("$.acquisitions[{index}]"),
        )?);
    }
    for (index, value) in values(root, "devices")?.iter().enumerate() {
        draft.add_device(parse_catalog::<DeviceTag>(
            value,
            &format!("$.devices[{index}]"),
        )?);
    }
    for (index, value) in values(root, "sensors")?.iter().enumerate() {
        draft.add_sensor(parse_catalog::<SensorTag>(
            value,
            &format!("$.sensors[{index}]"),
        )?);
    }
    for (index, value) in values(root, "channels")?.iter().enumerate() {
        draft.add_channel(parse_catalog::<ChannelTag>(
            value,
            &format!("$.channels[{index}]"),
        )?);
    }
    for (index, value) in values(root, "concept_dictionaries")?.iter().enumerate() {
        draft.add_concept_dictionary(parse_catalog::<ConceptDictionaryTag>(
            value,
            &format!("$.concept_dictionaries[{index}]"),
        )?);
    }
    parse_source_relationships(root, &mut draft)?;

    for (index, value) in values(root, "recordings")?.iter().enumerate() {
        let path = format!("$.recordings[{index}]");
        let record = object(value, &path)?;
        let stream_ids = values_at(record, "streams", &path)?
            .iter()
            .enumerate()
            .map(|(i, value)| parse_id(value, &format!("{path}.streams[{i}]")))
            .collect::<ParseResult<Vec<ObjectId<StreamTag>>>>()?;
        let mut recording = Recording::new(
            parse_id(field(record, "id", &path)?, &format!("{path}.id"))?,
            stream_ids,
        );
        for key in parse_source_keys(
            field(record, "source_keys", &path)?,
            &format!("{path}.source_keys"),
        )? {
            recording.add_source_key(key);
        }
        draft.add_recording(recording);
    }

    for (index, value) in values(root, "streams")?.iter().enumerate() {
        let path = format!("$.streams[{index}]");
        let stream = object(value, &path)?;
        let atom_ids = values_at(stream, "atoms", &path)?
            .iter()
            .enumerate()
            .map(|(i, value)| parse_id(value, &format!("{path}.atoms[{i}]")))
            .collect::<ParseResult<Vec<ObjectId<AtomTag>>>>()?;
        draft.add_stream(Stream::new(
            parse_id(field(stream, "id", &path)?, &format!("{path}.id"))?,
            parse_id(
                field(stream, "recording_id", &path)?,
                &format!("{path}.recording_id"),
            )?,
            concept(
                field(stream, "modality", &path)?,
                &format!("{path}.modality"),
            )?,
            atom_ids,
            optional_id(stream.get("clock_id"), &format!("{path}.clock_id"))?,
            optional_id(
                stream.get("channel_basis_id"),
                &format!("{path}.channel_basis_id"),
            )?,
            optional_id(stream.get("policy_id"), &format!("{path}.policy_id"))?,
        ));
    }

    for (index, value) in values(root, "atoms")?.iter().enumerate() {
        draft.add_atom(parse_atom(value, &format!("$.atoms[{index}]"))?);
    }

    for (index, value) in values(root, "clocks")?.iter().enumerate() {
        let path = format!("$.clocks[{index}]");
        let item = object(value, &path)?;
        draft.add_clock(Clock::new(
            parse_id(field(item, "id", &path)?, &format!("{path}.id"))?,
            concept(field(item, "kind", &path)?, &format!("{path}.kind"))?,
            optional_id(item.get("parent_id"), &format!("{path}.parent_id"))?,
            rational(field(item, "offset", &path)?, &format!("{path}.offset"))?,
            rational(field(item, "rate", &path)?, &format!("{path}.rate"))?,
            rational(
                field(item, "uncertainty", &path)?,
                &format!("{path}.uncertainty"),
            )?,
        ));
    }

    for (index, value) in values(root, "coordinate_frames")?.iter().enumerate() {
        let path = format!("$.coordinate_frames[{index}]");
        let item = object(value, &path)?;
        let transform = match item.get("transform") {
            None | Some(Value::Null) => None,
            Some(value) => Some(exact_matrix(value, &format!("{path}.transform"))?),
        };
        draft.add_coordinate_frame(CoordinateFrame::new(
            parse_id(field(item, "id", &path)?, &format!("{path}.id"))?,
            concept(field(item, "kind", &path)?, &format!("{path}.kind"))?,
            optional_id(item.get("parent_id"), &format!("{path}.parent_id"))?,
            transform,
            rational(
                field(item, "uncertainty", &path)?,
                &format!("{path}.uncertainty"),
            )?,
        ));
    }

    for (index, value) in values(root, "channel_bases")?.iter().enumerate() {
        let path = format!("$.channel_bases[{index}]");
        let item = object(value, &path)?;
        let mut channels = Vec::new();
        for (channel_index, value) in values_at(item, "channels", &path)?.iter().enumerate() {
            let channel_path = format!("{path}.channels[{channel_index}]");
            let channel_value = object(value, &channel_path)?;
            let mut channel = ChannelSpec::new(concept(
                field(channel_value, "concept", &channel_path)?,
                &format!("{channel_path}.concept"),
            )?);
            if let Some(frame_id) = optional_id(
                channel_value.get("coordinate_frame_id"),
                &format!("{channel_path}.coordinate_frame_id"),
            )? {
                channel = channel.with_coordinate_frame(frame_id);
            }
            for key in parse_source_keys(
                field(channel_value, "source_keys", &channel_path)?,
                &format!("{channel_path}.source_keys"),
            )? {
                channel = channel.with_source_key(key);
            }
            channels.push(channel);
        }
        draft.add_channel_basis(ChannelBasis::new(
            parse_id(field(item, "id", &path)?, &format!("{path}.id"))?,
            channels,
            parse_reference(
                field(item, "reference", &path)?,
                &format!("{path}.reference"),
            )?,
        ));
    }

    parse_relations(root, &mut draft)?;
    parse_governance(root, &mut draft)?;

    let dataset = draft
        .validate(limits)
        .map_err(CanonicalParseError::Validation)?;
    let normalized_input = serde_json::to_vec(&document_value)
        .map_err(|error| CanonicalParseError::message_only("$", error.to_string()))?;
    let normalized_semantics = canonical_debug_json(&dataset)
        .map_err(|error| CanonicalParseError::message_only("$", error.to_string()))?;
    if normalized_input != normalized_semantics {
        return Err(parse_error(
            "$",
            "document is not the exact semantic-v1 canonical debug form",
        ));
    }
    Ok(dataset)
}

fn parse_relations(root: &Map<String, Value>, draft: &mut DatasetDraft) -> ParseResult<()> {
    for (index, value) in values(root, "clock_relations")?.iter().enumerate() {
        let path = format!("$.clock_relations[{index}]");
        let item = object(value, &path)?;
        let validity_start = rational(
            field(item, "validity_start", &path)?,
            &format!("{path}.validity_start"),
        )?;
        let validity_end = match item.get("validity_end") {
            None | Some(Value::Null) => None,
            Some(value) => Some(rational(value, &format!("{path}.validity_end"))?),
        };
        let provenance = parse_content(
            field(item, "provenance", &path)?,
            &format!("{path}.provenance"),
        )?;
        draft.add_clock_relation(ClockRelation::new(
            parse_id(field(item, "id", &path)?, &format!("{path}.id"))?,
            parse_id(
                field(item, "from_clock_id", &path)?,
                &format!("{path}.from_clock_id"),
            )?,
            parse_id(
                field(item, "to_clock_id", &path)?,
                &format!("{path}.to_clock_id"),
            )?,
            rational(field(item, "offset", &path)?, &format!("{path}.offset"))?,
            rational(field(item, "rate", &path)?, &format!("{path}.rate"))?,
            rational(
                field(item, "uncertainty", &path)?,
                &format!("{path}.uncertainty"),
            )?,
            concept(field(item, "method", &path)?, &format!("{path}.method"))?,
            validity_start,
            validity_end,
            provenance,
        ));
    }
    for (index, value) in values(root, "frame_transforms")?.iter().enumerate() {
        let path = format!("$.frame_transforms[{index}]");
        let item = object(value, &path)?;
        draft.add_frame_transform(FrameTransform::new(
            parse_id(field(item, "id", &path)?, &format!("{path}.id"))?,
            parse_id(
                field(item, "from_frame_id", &path)?,
                &format!("{path}.from_frame_id"),
            )?,
            parse_id(
                field(item, "to_frame_id", &path)?,
                &format!("{path}.to_frame_id"),
            )?,
            exact_matrix(
                field(item, "transform", &path)?,
                &format!("{path}.transform"),
            )?,
            rational(
                field(item, "uncertainty", &path)?,
                &format!("{path}.uncertainty"),
            )?,
            concept(field(item, "method", &path)?, &format!("{path}.method"))?,
        ));
    }
    for (index, value) in values(root, "events")?.iter().enumerate() {
        let path = format!("$.events[{index}]");
        let item = object(value, &path)?;
        draft.add_event(Event::new(
            parse_id(field(item, "id", &path)?, &format!("{path}.id"))?,
            concept(field(item, "kind", &path)?, &format!("{path}.kind"))?,
            parse_id(field(item, "clock_id", &path)?, &format!("{path}.clock_id"))?,
            rational(field(item, "start", &path)?, &format!("{path}.start"))?,
            rational(field(item, "end", &path)?, &format!("{path}.end"))?,
            rational(
                field(item, "uncertainty", &path)?,
                &format!("{path}.uncertainty"),
            )?,
        ));
    }
    for (index, value) in values(root, "derived_artifacts")?.iter().enumerate() {
        let path = format!("$.derived_artifacts[{index}]");
        let item = object(value, &path)?;
        draft.add_derived_artifact(DerivedArtifact::new(
            parse_id(field(item, "id", &path)?, &format!("{path}.id"))?,
            parse_content(
                field(item, "content_id", &path)?,
                &format!("{path}.content_id"),
            )?,
            parse_id(
                field(item, "derivation_id", &path)?,
                &format!("{path}.derivation_id"),
            )?,
        ));
    }
    Ok(())
}

fn parse_source_relationships(
    root: &Map<String, Value>,
    draft: &mut DatasetDraft,
) -> ParseResult<()> {
    let Some(relationships) = root.get("source_relationships") else {
        return Ok(());
    };
    for (index, value) in array(relationships, "$.source_relationships")?
        .iter()
        .enumerate()
    {
        let path = format!("$.source_relationships[{index}]");
        let item = object(value, &path)?;
        let relationship = match string(field(item, "kind", &path)?, &format!("{path}.kind"))? {
            "patient-subject" => SourceRelationship::PatientSubject {
                patient_id: parse_id(
                    field(item, "patient_id", &path)?,
                    &format!("{path}.patient_id"),
                )?,
                subject_id: parse_id(
                    field(item, "subject_id", &path)?,
                    &format!("{path}.subject_id"),
                )?,
            },
            "session-subject" => SourceRelationship::SessionSubject {
                session_id: parse_id(
                    field(item, "session_id", &path)?,
                    &format!("{path}.session_id"),
                )?,
                subject_id: parse_id(
                    field(item, "subject_id", &path)?,
                    &format!("{path}.subject_id"),
                )?,
            },
            "session-patient" => SourceRelationship::SessionPatient {
                session_id: parse_id(
                    field(item, "session_id", &path)?,
                    &format!("{path}.session_id"),
                )?,
                patient_id: parse_id(
                    field(item, "patient_id", &path)?,
                    &format!("{path}.patient_id"),
                )?,
            },
            "acquisition-session" => SourceRelationship::AcquisitionSession {
                acquisition_id: parse_id(
                    field(item, "acquisition_id", &path)?,
                    &format!("{path}.acquisition_id"),
                )?,
                session_id: parse_id(
                    field(item, "session_id", &path)?,
                    &format!("{path}.session_id"),
                )?,
            },
            "acquisition-device" => SourceRelationship::AcquisitionDevice {
                acquisition_id: parse_id(
                    field(item, "acquisition_id", &path)?,
                    &format!("{path}.acquisition_id"),
                )?,
                device_id: parse_id(
                    field(item, "device_id", &path)?,
                    &format!("{path}.device_id"),
                )?,
            },
            "device-sensor" => SourceRelationship::DeviceSensor {
                device_id: parse_id(
                    field(item, "device_id", &path)?,
                    &format!("{path}.device_id"),
                )?,
                sensor_id: parse_id(
                    field(item, "sensor_id", &path)?,
                    &format!("{path}.sensor_id"),
                )?,
            },
            "sensor-channel" => SourceRelationship::SensorChannel {
                sensor_id: parse_id(
                    field(item, "sensor_id", &path)?,
                    &format!("{path}.sensor_id"),
                )?,
                channel_id: parse_id(
                    field(item, "channel_id", &path)?,
                    &format!("{path}.channel_id"),
                )?,
            },
            "acquisition-recording" => SourceRelationship::AcquisitionRecording {
                acquisition_id: parse_id(
                    field(item, "acquisition_id", &path)?,
                    &format!("{path}.acquisition_id"),
                )?,
                recording_id: parse_id(
                    field(item, "recording_id", &path)?,
                    &format!("{path}.recording_id"),
                )?,
            },
            "channel-basis-member" => SourceRelationship::ChannelBasisMember {
                channel_id: parse_id(
                    field(item, "channel_id", &path)?,
                    &format!("{path}.channel_id"),
                )?,
                basis_id: parse_id(field(item, "basis_id", &path)?, &format!("{path}.basis_id"))?,
                position: u32::try_from(unsigned(
                    field(item, "position", &path)?,
                    &format!("{path}.position"),
                )?)
                .map_err(|_| parse_error(&format!("{path}.position"), "position exceeds u32"))?,
            },
            _ => {
                return Err(parse_error(
                    &format!("{path}.kind"),
                    "unknown source relationship kind",
                ))
            }
        };
        draft.add_source_relationship(relationship);
    }
    Ok(())
}

fn parse_governance(root: &Map<String, Value>, draft: &mut DatasetDraft) -> ParseResult<()> {
    for (index, value) in values(root, "policies")?.iter().enumerate() {
        let path = format!("$.policies[{index}]");
        let item = object(value, &path)?;
        let restrictions = values_at(item, "restrictions", &path)?
            .iter()
            .enumerate()
            .map(|(i, value)| concept(value, &format!("{path}.restrictions[{i}]")))
            .collect::<ParseResult<Vec<_>>>()?;
        draft.add_policy(Policy::new(
            parse_id(field(item, "id", &path)?, &format!("{path}.id"))?,
            optional_id(item.get("parent_id"), &format!("{path}.parent_id"))?,
            restrictions,
        ));
    }
    for (index, value) in values(root, "proofs")?.iter().enumerate() {
        let path = format!("$.proofs[{index}]");
        let item = object(value, &path)?;
        draft.add_proof(Proof::new(
            parse_id(field(item, "id", &path)?, &format!("{path}.id"))?,
            concept(field(item, "kind", &path)?, &format!("{path}.kind"))?,
            parse_semantic_ref(field(item, "subject", &path)?, &format!("{path}.subject"))?,
            parse_content(field(item, "payload", &path)?, &format!("{path}.payload"))?,
        ));
    }
    for (index, value) in values(root, "derivations")?.iter().enumerate() {
        let path = format!("$.derivations[{index}]");
        let item = object(value, &path)?;
        let inputs = values_at(item, "inputs", &path)?
            .iter()
            .enumerate()
            .map(|(i, value)| parse_semantic_ref(value, &format!("{path}.inputs[{i}]")))
            .collect::<ParseResult<Vec<_>>>()?;
        let outputs = values_at(item, "outputs", &path)?
            .iter()
            .enumerate()
            .map(|(i, value)| parse_semantic_ref(value, &format!("{path}.outputs[{i}]")))
            .collect::<ParseResult<Vec<_>>>()?;
        draft.add_derivation(Derivation::new(
            parse_id(field(item, "id", &path)?, &format!("{path}.id"))?,
            concept(
                field(item, "operation", &path)?,
                &format!("{path}.operation"),
            )?,
            inputs,
            outputs,
        ));
    }
    for (index, value) in values(root, "fidelity")?.iter().enumerate() {
        let path = format!("$.fidelity[{index}]");
        let item = object(value, &path)?;
        let kind = match string(field(item, "kind", &path)?, &format!("{path}.kind"))? {
            "exact" => FidelityKind::Exact,
            "bounded" => FidelityKind::Bounded,
            "transformed" => FidelityKind::Transformed,
            _ => {
                return Err(parse_error(
                    &format!("{path}.kind"),
                    "unknown fidelity kind",
                ))
            }
        };
        let metric = optional_concept(item.get("metric"), &format!("{path}.metric"))?;
        let bound = match item.get("bound") {
            None | Some(Value::Null) => None,
            Some(value) => Some(exact(value, &format!("{path}.bound"))?),
        };
        draft.add_fidelity(Fidelity::new(
            parse_semantic_ref(field(item, "subject", &path)?, &format!("{path}.subject"))?,
            kind,
            metric,
            bound,
        ));
    }
    for (index, value) in values(root, "source_capsules")?.iter().enumerate() {
        let path = format!("$.source_capsules[{index}]");
        let item = object(value, &path)?;
        let source = parse_source_key(field(item, "source", &path)?, &format!("{path}.source"))?;
        let content_id = parse_content(
            field(item, "content_id", &path)?,
            &format!("{path}.content_id"),
        )?;
        let media = optional_string(item.get("media_type"), &format!("{path}.media_type"))?;
        draft.add_source_capsule(SourceCapsule::new(source, content_id, media.as_deref()));
    }
    for (index, value) in values(root, "observed_execution")?.iter().enumerate() {
        let path = format!("$.observed_execution[{index}]");
        let item = object(value, &path)?;
        let mut execution = ExecutionRecord::new(
            concept(
                field(item, "operation", &path)?,
                &format!("{path}.operation"),
            )?,
            string(
                field(item, "implementation", &path)?,
                &format!("{path}.implementation"),
            )?,
        );
        if let Some(hardware) = optional_string(item.get("hardware"), &format!("{path}.hardware"))?
        {
            execution = execution.with_hardware(hardware);
        }
        draft.add_observed_execution(execution);
    }
    Ok(())
}

fn parse_atom(value: &Value, path: &str) -> ParseResult<Atom> {
    let item = object(value, path)?;
    let id = parse_id(field(item, "id", path)?, &format!("{path}.id"))?;
    let presence = parse_presence(field(item, "presence", path)?, &format!("{path}.presence"))?;
    let payload = match item.get("payload") {
        None | Some(Value::Null) => None,
        Some(value) => Some(parse_payload(value, &format!("{path}.payload"))?),
    };
    match string(field(item, "kind", path)?, &format!("{path}.kind"))? {
        "signal-block" => {
            let calibration = match item.get("calibration") {
                None | Some(Value::Null) => None,
                Some(value) => {
                    let value = object(value, &format!("{path}.calibration"))?;
                    Some(
                        Calibration::new(
                            rational(
                                field(value, "scale", path)?,
                                &format!("{path}.calibration.scale"),
                            )?,
                            rational(
                                field(value, "offset", path)?,
                                &format!("{path}.calibration.offset"),
                            )?,
                            concept(
                                field(value, "unit", path)?,
                                &format!("{path}.calibration.unit"),
                            )?,
                        )
                        .map_err(|error| {
                            parse_error(&format!("{path}.calibration"), &error.to_string())
                        })?,
                    )
                }
            };
            Ok(Atom::SignalBlock(SignalBlock::new(
                id,
                presence,
                payload,
                parse_time_axis(
                    field(item, "time_axis", path)?,
                    &format!("{path}.time_axis"),
                )?,
                calibration,
            )))
        }
        "temporal-table" => Ok(Atom::TemporalTable(TemporalTable::new(
            id,
            presence,
            payload,
            parse_id(field(item, "clock_id", path)?, &format!("{path}.clock_id"))?,
            concept(
                field(item, "record_kind", path)?,
                &format!("{path}.record_kind"),
            )?,
            parse_columns(field(item, "columns", path)?, &format!("{path}.columns"))?,
        ))),
        "table" => Ok(Atom::Table(Table::new(
            id,
            presence,
            payload,
            parse_columns(field(item, "columns", path)?, &format!("{path}.columns"))?,
        ))),
        "tensor" => {
            let axes = array(field(item, "axes", path)?, &format!("{path}.axes"))?
                .iter()
                .enumerate()
                .map(|(i, value)| {
                    let axis_path = format!("{path}.axes[{i}]");
                    let axis = object(value, &axis_path)?;
                    Ok(SemanticAxis::new(
                        concept(
                            field(axis, "semantic", &axis_path)?,
                            &format!("{axis_path}.semantic"),
                        )?,
                        unsigned(
                            field(axis, "extent", &axis_path)?,
                            &format!("{axis_path}.extent"),
                        )?,
                    ))
                })
                .collect::<ParseResult<Vec<_>>>()?;
            Ok(Atom::Tensor(Tensor::new(id, presence, payload, axes)))
        }
        "encoded-block" => {
            let decoded_path = format!("{path}.decoded_semantics");
            let decoded = object(field(item, "decoded_semantics", path)?, &decoded_path)?;
            Ok(Atom::EncodedBlock(EncodedBlock::new(
                id,
                presence,
                payload,
                DecodedSemantics::new(
                    concept(
                        field(decoded, "atom_kind", &decoded_path)?,
                        &format!("{decoded_path}.atom_kind"),
                    )?,
                    parse_element(
                        string(
                            field(decoded, "element", &decoded_path)?,
                            &format!("{decoded_path}.element"),
                        )?,
                        &format!("{decoded_path}.element"),
                    )?,
                    parse_shape(
                        field(decoded, "shape", &decoded_path)?,
                        &format!("{decoded_path}.shape"),
                    )?,
                ),
            )))
        }
        "blob-ref" => {
            let integrity_path = format!("{path}.integrity");
            let integrity = object(field(item, "integrity", path)?, &integrity_path)?;
            Ok(Atom::BlobRef(BlobRef::new(
                id,
                presence,
                payload,
                string(
                    field(item, "media_type", path)?,
                    &format!("{path}.media_type"),
                )?
                .to_owned(),
                BlobIntegrity::new(
                    concept(
                        field(integrity, "algorithm", &integrity_path)?,
                        &format!("{integrity_path}.algorithm"),
                    )?,
                    parse_content(
                        field(integrity, "digest", &integrity_path)?,
                        &format!("{integrity_path}.digest"),
                    )?,
                ),
            )))
        }
        _ => Err(parse_error(&format!("{path}.kind"), "unknown atom kind")),
    }
}

fn parse_payload(value: &Value, path: &str) -> ParseResult<PayloadDescriptor> {
    let item = object(value, path)?;
    Ok(PayloadDescriptor::new(
        parse_content(
            field(item, "content_id", path)?,
            &format!("{path}.content_id"),
        )?,
        unsigned(
            field(item, "logical_bytes", path)?,
            &format!("{path}.logical_bytes"),
        )?,
        parse_element(
            string(field(item, "element", path)?, &format!("{path}.element"))?,
            &format!("{path}.element"),
        )?,
        parse_byte_order(
            string(
                field(item, "byte_order", path)?,
                &format!("{path}.byte_order"),
            )?,
            &format!("{path}.byte_order"),
        )?,
        parse_shape(field(item, "shape", path)?, &format!("{path}.shape"))?,
        parse_layout_value(field(item, "layout", path)?, &format!("{path}.layout"))?,
        optional_concept(item.get("encoding"), &format!("{path}.encoding"))?,
        optional_string(item.get("media_type"), &format!("{path}.media_type"))?,
    ))
}

fn parse_layout_value(value: &Value, path: &str) -> ParseResult<Layout> {
    if let Some(value) = value.as_str() {
        return parse_layout(value, path);
    }
    let item = object(value, path)?;
    if let Some(value) = item.get("ragged") {
        let variant_path = format!("{path}.ragged");
        let value = object(value, &variant_path)?;
        let rows_path = format!("{variant_path}.rows");
        let offsets_path = format!("{variant_path}.offsets");
        return Ok(Layout::Ragged {
            rows: unsigned(field(value, "rows", &variant_path)?, &rows_path)?,
            offsets: parse_content(field(value, "offsets", &variant_path)?, &offsets_path)?,
        });
    }
    if let Some(value) = item.get("sparse-coo") {
        let variant_path = format!("{path}.sparse-coo");
        let value = object(value, &variant_path)?;
        let nonzero_path = format!("{variant_path}.nonzero");
        let indices_path = format!("{variant_path}.indices");
        return Ok(Layout::SparseCoo {
            nonzero: unsigned(field(value, "nonzero", &variant_path)?, &nonzero_path)?,
            indices: parse_content(field(value, "indices", &variant_path)?, &indices_path)?,
        });
    }
    if let Some(value) = item.get("sparse-csr") {
        let variant_path = format!("{path}.sparse-csr");
        let value = object(value, &variant_path)?;
        let nonzero_path = format!("{variant_path}.nonzero");
        let indptr_path = format!("{variant_path}.indptr");
        let indices_path = format!("{variant_path}.indices");
        return Ok(Layout::SparseCsr {
            nonzero: unsigned(field(value, "nonzero", &variant_path)?, &nonzero_path)?,
            indptr: parse_content(field(value, "indptr", &variant_path)?, &indptr_path)?,
            indices: parse_content(field(value, "indices", &variant_path)?, &indices_path)?,
        });
    }
    if let Some(value) = item.get("bfp") {
        let variant_path = format!("{path}.bfp");
        let value = object(value, &variant_path)?;
        let block_len_path = format!("{variant_path}.block_len");
        let mantissa_bits_path = format!("{variant_path}.mantissa_bits");
        let scales_path = format!("{variant_path}.scales");
        let block_len = u32::try_from(unsigned(
            field(value, "block_len", &variant_path)?,
            &block_len_path,
        )?)
        .map_err(|_| parse_error(&block_len_path, "block_len exceeds u32"))?;
        let mantissa_bits = u8::try_from(unsigned(
            field(value, "mantissa_bits", &variant_path)?,
            &mantissa_bits_path,
        )?)
        .map_err(|_| parse_error(&mantissa_bits_path, "mantissa_bits exceeds u8"))?;
        return Ok(Layout::BlockFloatingPoint {
            block_len,
            mantissa_bits,
            scales: parse_content(field(value, "scales", &variant_path)?, &scales_path)?,
        });
    }
    Err(parse_error(path, "unknown layout"))
}

fn parse_time_axis(value: &Value, path: &str) -> ParseResult<TimeAxis> {
    let item = object(value, path)?;
    if let Some(value) = item.get("regular") {
        return Ok(TimeAxis::Regular(parse_segment(
            value,
            &format!("{path}.regular"),
        )?));
    }
    if let Some(value) = item.get("piecewise") {
        let segments = array(value, &format!("{path}.piecewise"))?
            .iter()
            .enumerate()
            .map(|(i, value)| parse_segment(value, &format!("{path}.piecewise[{i}]")))
            .collect::<ParseResult<Vec<_>>>()?;
        return Ok(TimeAxis::Piecewise(segments));
    }
    if let Some(value) = item.get("explicit") {
        let explicit_path = format!("{path}.explicit");
        let explicit = object(value, &explicit_path)?;
        let timestamps_path = format!("{explicit_path}.timestamps");
        let count_path = format!("{explicit_path}.count");
        return Ok(TimeAxis::Explicit {
            timestamps: parse_content(
                field(explicit, "timestamps", &explicit_path)?,
                &timestamps_path,
            )?,
            count: unsigned(field(explicit, "count", &explicit_path)?, &count_path)?,
        });
    }
    Err(parse_error(path, "unknown time axis"))
}

fn parse_segment(value: &Value, path: &str) -> ParseResult<TimeSegment> {
    let item = object(value, path)?;
    TimeSegment::new(
        rational(field(item, "start", path)?, &format!("{path}.start"))?,
        rational(field(item, "rate", path)?, &format!("{path}.rate"))?,
        unsigned(field(item, "samples", path)?, &format!("{path}.samples"))?,
    )
    .map_err(|error| parse_error(path, &error.to_string()))
}

fn parse_columns(value: &Value, path: &str) -> ParseResult<Vec<TableColumn>> {
    array(value, path)?
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let column_path = format!("{path}[{index}]");
            let item = object(value, &column_path)?;
            Ok(TableColumn::new(
                concept(
                    field(item, "semantic", &column_path)?,
                    &format!("{column_path}.semantic"),
                )?,
                parse_element(
                    string(
                        field(item, "element", &column_path)?,
                        &format!("{column_path}.element"),
                    )?,
                    &format!("{column_path}.element"),
                )?,
                boolean(
                    field(item, "nullable", &column_path)?,
                    &format!("{column_path}.nullable"),
                )?,
            ))
        })
        .collect()
}

fn parse_catalog<T: SemanticTag>(value: &Value, path: &str) -> ParseResult<CatalogRecord<T>> {
    let item = object(value, path)?;
    let mut record = CatalogRecord::new(
        parse_id(field(item, "id", path)?, &format!("{path}.id"))?,
        concept(field(item, "kind", path)?, &format!("{path}.kind"))?,
    );
    for key in parse_source_keys(
        field(item, "source_keys", path)?,
        &format!("{path}.source_keys"),
    )? {
        record = record.with_source_key(key);
    }
    Ok(record)
}

fn parse_source_keys(value: &Value, path: &str) -> ParseResult<Vec<SourceKey>> {
    array(value, path)?
        .iter()
        .enumerate()
        .map(|(index, value)| parse_source_key(value, &format!("{path}[{index}]")))
        .collect()
}

fn parse_source_key(value: &Value, path: &str) -> ParseResult<SourceKey> {
    let item = object(value, path)?;
    SourceKey::new(
        string(
            field(item, "namespace", path)?,
            &format!("{path}.namespace"),
        )?,
        string(field(item, "value", path)?, &format!("{path}.value"))?,
    )
    .map_err(|error| parse_error(path, &error.to_string()))
}

fn parse_semantic_ref(value: &Value, path: &str) -> ParseResult<SemanticRef> {
    let item = object(value, path)?;
    let id_path = format!("{path}.id");
    let id = field(item, "id", path)?;
    Ok(
        match string(field(item, "kind", path)?, &format!("{path}.kind"))? {
            "dataset" => SemanticRef::of(parse_id::<DatasetTag>(id, &id_path)?),
            "recording" => SemanticRef::of(parse_id::<RecordingTag>(id, &id_path)?),
            "stream" => SemanticRef::of(parse_id::<StreamTag>(id, &id_path)?),
            "atom" => SemanticRef::of(parse_id::<AtomTag>(id, &id_path)?),
            "clock" => SemanticRef::of(parse_id::<ClockTag>(id, &id_path)?),
            "coordinate-frame" => SemanticRef::of(parse_id::<CoordinateFrameTag>(id, &id_path)?),
            "channel-basis" => SemanticRef::of(parse_id::<ChannelBasisTag>(id, &id_path)?),
            "policy" => SemanticRef::of(parse_id::<PolicyTag>(id, &id_path)?),
            "proof" => SemanticRef::of(parse_id::<ProofTag>(id, &id_path)?),
            "derivation" => SemanticRef::of(parse_id::<DerivationTag>(id, &id_path)?),
            "subject" => SemanticRef::of(parse_id::<SubjectTag>(id, &id_path)?),
            "patient" => SemanticRef::of(parse_id::<PatientTag>(id, &id_path)?),
            "session" => SemanticRef::of(parse_id::<SessionTag>(id, &id_path)?),
            "acquisition" => SemanticRef::of(parse_id::<AcquisitionTag>(id, &id_path)?),
            "device" => SemanticRef::of(parse_id::<DeviceTag>(id, &id_path)?),
            "sensor" => SemanticRef::of(parse_id::<SensorTag>(id, &id_path)?),
            "channel" => SemanticRef::of(parse_id::<ChannelTag>(id, &id_path)?),
            "clock-relation" => SemanticRef::of(parse_id::<ClockRelationTag>(id, &id_path)?),
            "frame-transform" => SemanticRef::of(parse_id::<FrameTransformTag>(id, &id_path)?),
            "event" => SemanticRef::of(parse_id::<EventTag>(id, &id_path)?),
            "concept-dictionary" => {
                SemanticRef::of(parse_id::<ConceptDictionaryTag>(id, &id_path)?)
            }
            "derived-artifact" => SemanticRef::of(parse_id::<DerivedArtifactTag>(id, &id_path)?),
            _ => {
                return Err(parse_error(
                    &format!("{path}.kind"),
                    "unknown semantic reference kind",
                ))
            }
        },
    )
}

fn exact_matrix(value: &Value, path: &str) -> ParseResult<[ExactNumber; 16]> {
    let values = array(value, path)?
        .iter()
        .enumerate()
        .map(|(index, value)| exact(value, &format!("{path}[{index}]")))
        .collect::<ParseResult<Vec<_>>>()?;
    values
        .try_into()
        .map_err(|_| parse_error(path, "expected exactly 16 transform values"))
}

fn exact(value: &Value, path: &str) -> ParseResult<ExactNumber> {
    let item = object(value, path)?;
    if let Some(value) = item.get("$integer") {
        return string(value, path)?
            .parse::<i128>()
            .map(ExactNumber::Integer)
            .map_err(|_| parse_error(path, "integer is outside i128"));
    }
    Ok(ExactNumber::Rational(rational(value, path)?))
}

fn rational(value: &Value, path: &str) -> ParseResult<Rational> {
    let item = object(value, path)?;
    let parts = array(field(item, "$rational", path)?, path)?;
    if parts.len() != 2 {
        return Err(parse_error(
            path,
            "rational must contain numerator and denominator",
        ));
    }
    let numerator = string(&parts[0], path)?
        .parse::<i128>()
        .map_err(|_| parse_error(path, "invalid i128 numerator"))?;
    let denominator = string(&parts[1], path)?
        .parse::<i128>()
        .map_err(|_| parse_error(path, "invalid i128 denominator"))?;
    Rational::new(numerator, denominator).map_err(|error| parse_error(path, &error.to_string()))
}

fn parse_presence(value: &Value, path: &str) -> ParseResult<Presence> {
    match string(value, path)? {
        "present" => Ok(Presence::Present),
        "absent-at-source" => Ok(Presence::AbsentAtSource),
        "unknown-at-source" => Ok(Presence::UnknownAtSource),
        "withheld" => Ok(Presence::Withheld),
        "redacted" => Ok(Presence::Redacted),
        "not-applicable" => Ok(Presence::NotApplicable),
        _ => Err(parse_error(path, "unknown presence state")),
    }
}

fn parse_reference(value: &Value, path: &str) -> ParseResult<ReferenceKind> {
    match string(value, path)? {
        "absolute" => Ok(ReferenceKind::Absolute),
        "common" => Ok(ReferenceKind::Common),
        "differential" => Ok(ReferenceKind::Differential),
        "unknown" => Ok(ReferenceKind::Unknown),
        _ => Err(parse_error(path, "unknown reference kind")),
    }
}

fn parse_shape(value: &Value, path: &str) -> ParseResult<Vec<u64>> {
    array(value, path)?
        .iter()
        .enumerate()
        .map(|(index, value)| unsigned(value, &format!("{path}[{index}]")))
        .collect()
}

fn parse_id<T>(value: &Value, path: &str) -> ParseResult<ObjectId<T>> {
    Ok(ObjectId::from_bytes(parse_hex_at::<16>(
        string(value, path)?,
        path,
    )?))
}
fn parse_content(value: &Value, path: &str) -> ParseResult<ContentId> {
    Ok(ContentId::from_bytes(parse_hex_at::<32>(
        string(value, path)?,
        path,
    )?))
}
fn optional_id<T>(value: Option<&Value>, path: &str) -> ParseResult<Option<ObjectId<T>>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => parse_id(value, path).map(Some),
    }
}
fn optional_concept(value: Option<&Value>, path: &str) -> ParseResult<Option<ConceptId>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => concept(value, path).map(Some),
    }
}
fn optional_string(value: Option<&Value>, path: &str) -> ParseResult<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => string(value, path).map(str::to_owned).map(Some),
    }
}
fn concept(value: &Value, path: &str) -> ParseResult<ConceptId> {
    ConceptId::new(string(value, path)?).map_err(|error| parse_error(path, &error.to_string()))
}
fn values<'a>(object: &'a Map<String, Value>, name: &str) -> ParseResult<&'a Vec<Value>> {
    values_at(object, name, "$")
}
fn values_at<'a>(
    object: &'a Map<String, Value>,
    name: &str,
    owner_path: &str,
) -> ParseResult<&'a Vec<Value>> {
    let member_path = format!("{owner_path}.{name}");
    array(field(object, name, owner_path)?, &member_path)
}
fn field<'a>(object: &'a Map<String, Value>, name: &str, path: &str) -> ParseResult<&'a Value> {
    object
        .get(name)
        .ok_or_else(|| parse_error(path, &format!("missing member {name}")))
}
fn object<'a>(value: &'a Value, path: &str) -> ParseResult<&'a Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| parse_error(path, "expected object"))
}
fn array<'a>(value: &'a Value, path: &str) -> ParseResult<&'a Vec<Value>> {
    value
        .as_array()
        .ok_or_else(|| parse_error(path, "expected array"))
}
fn string<'a>(value: &'a Value, path: &str) -> ParseResult<&'a str> {
    value
        .as_str()
        .ok_or_else(|| parse_error(path, "expected string"))
}
fn unsigned(value: &Value, path: &str) -> ParseResult<u64> {
    value
        .as_u64()
        .ok_or_else(|| parse_error(path, "expected unsigned integer"))
}
fn boolean(value: &Value, path: &str) -> ParseResult<bool> {
    value
        .as_bool()
        .ok_or_else(|| parse_error(path, "expected boolean"))
}
fn parse_error(path: &str, message: &str) -> CanonicalParseError {
    CanonicalParseError::new(path, message)
}

fn parse_hex_at<const N: usize>(value: &str, path: &str) -> ParseResult<[u8; N]> {
    if value.len() != N * 2 {
        return Err(parse_error(
            path,
            &format!("expected {} lower-case hexadecimal characters", N * 2),
        ));
    }
    let mut bytes = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_digit(pair[0])
            .ok_or_else(|| parse_error(path, "identifiers must use lower-case hexadecimal"))?;
        let low = hex_digit(pair[1])
            .ok_or_else(|| parse_error(path, "identifiers must use lower-case hexadecimal"))?;
        bytes[index] = (high << 4) | low;
    }
    Ok(bytes)
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn parse_element(value: &str, path: &str) -> ParseResult<ElementType> {
    match value {
        "i8" => Ok(ElementType::I8),
        "i16" => Ok(ElementType::I16),
        "i24" => Ok(ElementType::I24),
        "i32" => Ok(ElementType::I32),
        "i64" => Ok(ElementType::I64),
        "u8" => Ok(ElementType::U8),
        "u16" => Ok(ElementType::U16),
        "u32" => Ok(ElementType::U32),
        "u64" => Ok(ElementType::U64),
        "f16" => Ok(ElementType::F16),
        "f32" => Ok(ElementType::F32),
        "f64" => Ok(ElementType::F64),
        "bool" => Ok(ElementType::Bool),
        "utf8" => Ok(ElementType::Utf8),
        "bytes" => Ok(ElementType::Bytes),
        _ => Err(parse_error(path, "unsupported fixed-width element type")),
    }
}

fn parse_byte_order(value: &str, path: &str) -> ParseResult<ByteOrder> {
    match value {
        "little" => Ok(ByteOrder::Little),
        "big" => Ok(ByteOrder::Big),
        "not-applicable" => Ok(ByteOrder::NotApplicable),
        _ => Err(parse_error(path, "unknown byte order")),
    }
}

fn parse_layout(value: &str, path: &str) -> ParseResult<Layout> {
    match value {
        "dense-row-major" => Ok(Layout::DenseRowMajor),
        "dense-column-major" => Ok(Layout::DenseColumnMajor),
        _ => Err(parse_error(path, "unknown dense layout")),
    }
}
