use abir_core::{
    canonical_debug_json, logical_content_id, AcquisitionTag, Atom, AtomTag, BlobIntegrity,
    BlobRef, ByteOrder, Calibration, ChannelBasis, ChannelBasisTag, ChannelSpec, ChannelTag, Clock,
    ClockRelation, ClockRelationTag, ClockTag, ConceptDictionaryTag, ConceptId, ContentId,
    CoordinateFrame, CoordinateFrameTag, DatasetDraft, DatasetTag, DecodedSemantics, Derivation,
    DerivationTag, DerivedArtifact, DerivedArtifactTag, DeviceTag, ElementType, EncodedBlock,
    Event, EventTag, ExactNumber, ExecutionRecord, Fidelity, FidelityKind, FrameTransform,
    FrameTransformTag, Layout, ObjectId, PatientTag, PayloadDescriptor, Policy, PolicyTag,
    Presence, Proof, ProofTag, Rational, Recording, RecordingTag, ReferenceKind, SemanticAxis,
    SemanticRef, SemanticTag, SensorTag, SessionTag, SignalBlock, SourceCapsule, SourceKey,
    SourceRelationship, Stream, StreamTag, SubjectTag, Table, TableColumn, TemporalTable, Tensor,
    TimeAxis, TimeSegment, ValidationLimits,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyModule, PyTuple};
use serde_json::{Map, Value};

#[pyclass(name = "Dataset", frozen)]
struct PyDataset {
    inner: abir_core::AbirDataset,
    atom_id: ObjectId<AtomTag>,
    payload: Py<PyBytes>,
}

#[pymethods]
impl PyDataset {
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    fn from_tensor(
        py: Python<'_>,
        dataset_id: &str,
        recording_id: &str,
        stream_id: &str,
        atom_id: &str,
        content_id: &str,
        modality: &str,
        element: &str,
        byte_order: &str,
        layout: &str,
        shape: Vec<u64>,
        payload: Py<PyBytes>,
    ) -> PyResult<Self> {
        let dataset_id = parse_object_id(dataset_id)?;
        let recording_id = parse_object_id(recording_id)?;
        let stream_id = parse_object_id(stream_id)?;
        let atom_id = parse_object_id(atom_id)?;
        let content_id = parse_content_id(content_id)?;
        let element = parse_element(element)?;
        let byte_order = parse_byte_order(byte_order)?;
        let layout = parse_layout(layout)?;
        let logical_bytes = payload.bind(py).as_bytes().len();
        let logical_bytes = u64::try_from(logical_bytes)
            .map_err(|_| PyValueError::new_err("payload is too large"))?;
        let inner = build_tensor_dataset(
            dataset_id,
            recording_id,
            stream_id,
            atom_id,
            content_id,
            ConceptId::new(modality).map_err(|error| PyValueError::new_err(error.to_string()))?,
            element,
            byte_order,
            layout,
            shape,
            logical_bytes,
            None,
        )?;
        Ok(Self {
            inner,
            atom_id,
            payload,
        })
    }

    #[staticmethod]
    #[pyo3(signature = (payload=None))]
    fn canonical_fixture(py: Python<'_>, payload: Option<Py<PyBytes>>) -> PyResult<Self> {
        let payload = payload.unwrap_or_else(|| PyBytes::new_bound(py, &[0_u8; 8]).unbind());
        let atom_id = ObjectId::from_bytes([4; 16]);
        let clock = Clock::new(
            ObjectId::from_bytes([6; 16]),
            ConceptId::new("abir:clock/device").expect("static concept"),
            None,
            Rational::new(-1, 3).expect("static rational"),
            Rational::new(256, 1).expect("static rational"),
            Rational::new(1, 1_000_000).expect("static rational"),
        );
        let inner = build_tensor_dataset(
            ObjectId::from_bytes([1; 16]),
            ObjectId::from_bytes([2; 16]),
            ObjectId::from_bytes([3; 16]),
            atom_id,
            ContentId::from_bytes([5; 32]),
            ConceptId::new("abir:modality/eeg").expect("static concept"),
            ElementType::I16,
            ByteOrder::Little,
            Layout::DenseRowMajor,
            vec![4],
            8,
            Some(clock),
        )?;
        Ok(Self {
            inner,
            atom_id,
            payload,
        })
    }

    /// Complete cross-language semantic-v1 conformance fixture.
    #[staticmethod]
    fn semantic_matrix_fixture(py: Python<'_>) -> Self {
        Self {
            inner: abir_conformance::semantic_matrix_dataset(),
            atom_id: ObjectId::from_bytes([13; 16]),
            payload: PyBytes::new_bound(py, &[]).unbind(),
        }
    }

    /// Parse the complete canonical semantic-v1 document through Rust's typed
    /// construction and validation boundary.
    #[staticmethod]
    fn from_canonical_json(py: Python<'_>, document: &[u8]) -> PyResult<Self> {
        let inner = parse_canonical_dataset(document)?;
        let atom_id = inner
            .atoms()
            .iter()
            .find(|atom| matches!(atom, Atom::Tensor(_)))
            .or_else(|| inner.atoms().first())
            .map(Atom::id)
            .unwrap_or_else(|| ObjectId::from_bytes([0; 16]));
        Ok(Self {
            inner,
            atom_id,
            payload: PyBytes::new_bound(py, &[]).unbind(),
        })
    }

    fn canonical_json<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let bytes = canonical_debug_json(&self.inner)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(PyBytes::new_bound(py, &bytes))
    }

    fn content_id(&self) -> PyResult<String> {
        logical_content_id(&self.inner)
            .map(|id| id.to_string())
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    fn payload_pointer(&self, py: Python<'_>) -> usize {
        self.payload.bind(py).as_bytes().as_ptr() as usize
    }

    fn numpy_view(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let atom = self
            .inner
            .atoms()
            .iter()
            .find(|atom| atom.id() == self.atom_id)
            .ok_or_else(|| PyValueError::new_err("tensor atom is missing"))?;
        if !matches!(atom, Atom::Tensor(_)) {
            return Err(PyValueError::new_err("atom is not a tensor"));
        }
        let descriptor = atom
            .payload()
            .ok_or_else(|| PyValueError::new_err("tensor payload is absent"))?;
        if !matches!(descriptor.layout(), Layout::DenseRowMajor) {
            return Err(PyValueError::new_err(
                "zero-copy NumPy view currently requires dense row-major layout",
            ));
        }
        let dtype = numpy_dtype(descriptor.element(), descriptor.byte_order())?;
        let numpy = PyModule::import_bound(py, "numpy")?;
        let array = numpy.call_method1("frombuffer", (self.payload.bind(py), dtype))?;
        let shape = PyTuple::new_bound(py, descriptor.shape().iter().copied());
        Ok(array.call_method("reshape", shape, None)?.unbind())
    }

    #[getter]
    fn recording_count(&self) -> usize {
        self.inner.recordings().len()
    }

    #[getter]
    fn stream_count(&self) -> usize {
        self.inner.streams().len()
    }

    #[getter]
    fn atom_count(&self) -> usize {
        self.inner.atoms().len()
    }

    #[getter]
    fn semantic_family_counts(&self) -> (usize, usize, usize, usize, usize, usize) {
        (
            self.inner.subjects().len()
                + self.inner.patients().len()
                + self.inner.sessions().len()
                + self.inner.acquisitions().len()
                + self.inner.devices().len()
                + self.inner.sensors().len()
                + self.inner.channels().len(),
            self.inner.clock_relations().len(),
            self.inner.frame_transforms().len(),
            self.inner.events().len(),
            self.inner.derived_artifacts().len(),
            self.inner.concept_dictionaries().len(),
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn build_tensor_dataset(
    dataset_id: ObjectId<DatasetTag>,
    recording_id: ObjectId<RecordingTag>,
    stream_id: ObjectId<StreamTag>,
    atom_id: ObjectId<AtomTag>,
    content_id: ContentId,
    modality: ConceptId,
    element: ElementType,
    byte_order: ByteOrder,
    layout: Layout,
    shape: Vec<u64>,
    logical_bytes: u64,
    clock: Option<Clock>,
) -> PyResult<abir_core::AbirDataset> {
    let clock_id = clock.as_ref().map(Clock::id);
    let axes = shape
        .iter()
        .copied()
        .map(|extent| {
            SemanticAxis::new(
                ConceptId::new("abir:axis/sample").expect("static concept"),
                extent,
            )
        })
        .collect();
    let mut draft = DatasetDraft::new(dataset_id);
    draft.add_recording(Recording::new(recording_id, vec![stream_id]));
    draft.add_stream(Stream::new(
        stream_id,
        recording_id,
        modality,
        vec![atom_id],
        clock_id,
        None,
        None,
    ));
    draft.add_atom(Atom::Tensor(Tensor::new(
        atom_id,
        Presence::Present,
        Some(PayloadDescriptor::new(
            content_id,
            logical_bytes,
            element,
            byte_order,
            shape,
            layout,
            Some(ConceptId::new("abir:encoding/raw").expect("static concept")),
            None,
        )),
        axes,
    )));
    if let Some(clock) = clock {
        draft.add_clock(clock);
    }
    draft
        .validate(ValidationLimits::default())
        .map_err(|report| {
            let failures = report
                .failures()
                .iter()
                .map(|failure| format!("{} at {}", failure.code(), failure.path()))
                .collect::<Vec<_>>()
                .join("; ");
            PyValueError::new_err(failures)
        })
}

fn parse_canonical_dataset(document: &[u8]) -> PyResult<abir_core::AbirDataset> {
    let root: Value = serde_json::from_slice(document)
        .map_err(|error| PyValueError::new_err(format!("invalid JSON: {error}")))?;
    let root = object(&root, "$")?;
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
        let stream_ids = values(record, "streams")?
            .iter()
            .enumerate()
            .map(|(i, value)| parse_id(value, &format!("{path}.streams[{i}]")))
            .collect::<PyResult<Vec<ObjectId<StreamTag>>>>()?;
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
        let atom_ids = values(stream, "atoms")?
            .iter()
            .enumerate()
            .map(|(i, value)| parse_id(value, &format!("{path}.atoms[{i}]")))
            .collect::<PyResult<Vec<ObjectId<AtomTag>>>>()?;
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
        for (channel_index, value) in values(item, "channels")?.iter().enumerate() {
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

    draft
        .validate(ValidationLimits::default())
        .map_err(|report| {
            let failures = report
                .failures()
                .iter()
                .map(|failure| format!("{} at {}", failure.code(), failure.path()))
                .collect::<Vec<_>>()
                .join("; ");
            PyValueError::new_err(failures)
        })
}

fn parse_relations(root: &Map<String, Value>, draft: &mut DatasetDraft) -> PyResult<()> {
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

fn parse_source_relationships(root: &Map<String, Value>, draft: &mut DatasetDraft) -> PyResult<()> {
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

fn parse_governance(root: &Map<String, Value>, draft: &mut DatasetDraft) -> PyResult<()> {
    for (index, value) in values(root, "policies")?.iter().enumerate() {
        let path = format!("$.policies[{index}]");
        let item = object(value, &path)?;
        let restrictions = values(item, "restrictions")?
            .iter()
            .enumerate()
            .map(|(i, value)| concept(value, &format!("{path}.restrictions[{i}]")))
            .collect::<PyResult<Vec<_>>>()?;
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
        let inputs = values(item, "inputs")?
            .iter()
            .enumerate()
            .map(|(i, value)| parse_semantic_ref(value, &format!("{path}.inputs[{i}]")))
            .collect::<PyResult<Vec<_>>>()?;
        let outputs = values(item, "outputs")?
            .iter()
            .enumerate()
            .map(|(i, value)| parse_semantic_ref(value, &format!("{path}.outputs[{i}]")))
            .collect::<PyResult<Vec<_>>>()?;
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

fn parse_atom(value: &Value, path: &str) -> PyResult<Atom> {
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
                .collect::<PyResult<Vec<_>>>()?;
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
                    parse_element(string(
                        field(decoded, "element", &decoded_path)?,
                        &format!("{decoded_path}.element"),
                    )?)?,
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

fn parse_payload(value: &Value, path: &str) -> PyResult<PayloadDescriptor> {
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
        parse_element(string(
            field(item, "element", path)?,
            &format!("{path}.element"),
        )?)?,
        parse_byte_order(string(
            field(item, "byte_order", path)?,
            &format!("{path}.byte_order"),
        )?)?,
        parse_shape(field(item, "shape", path)?, &format!("{path}.shape"))?,
        parse_layout_value(field(item, "layout", path)?, &format!("{path}.layout"))?,
        optional_concept(item.get("encoding"), &format!("{path}.encoding"))?,
        optional_string(item.get("media_type"), &format!("{path}.media_type"))?,
    ))
}

fn parse_layout_value(value: &Value, path: &str) -> PyResult<Layout> {
    if let Some(value) = value.as_str() {
        return parse_layout(value);
    }
    let item = object(value, path)?;
    if let Some(value) = item.get("ragged") {
        let value = object(value, path)?;
        return Ok(Layout::Ragged {
            rows: unsigned(field(value, "rows", path)?, path)?,
            offsets: parse_content(field(value, "offsets", path)?, path)?,
        });
    }
    if let Some(value) = item.get("sparse-coo") {
        let value = object(value, path)?;
        return Ok(Layout::SparseCoo {
            nonzero: unsigned(field(value, "nonzero", path)?, path)?,
            indices: parse_content(field(value, "indices", path)?, path)?,
        });
    }
    if let Some(value) = item.get("sparse-csr") {
        let value = object(value, path)?;
        return Ok(Layout::SparseCsr {
            nonzero: unsigned(field(value, "nonzero", path)?, path)?,
            indptr: parse_content(field(value, "indptr", path)?, path)?,
            indices: parse_content(field(value, "indices", path)?, path)?,
        });
    }
    if let Some(value) = item.get("bfp") {
        let value = object(value, path)?;
        let block_len = u32::try_from(unsigned(field(value, "block_len", path)?, path)?)
            .map_err(|_| parse_error(path, "block_len exceeds u32"))?;
        let mantissa_bits = u8::try_from(unsigned(field(value, "mantissa_bits", path)?, path)?)
            .map_err(|_| parse_error(path, "mantissa_bits exceeds u8"))?;
        return Ok(Layout::BlockFloatingPoint {
            block_len,
            mantissa_bits,
            scales: parse_content(field(value, "scales", path)?, path)?,
        });
    }
    Err(parse_error(path, "unknown layout"))
}

fn parse_time_axis(value: &Value, path: &str) -> PyResult<TimeAxis> {
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
            .collect::<PyResult<Vec<_>>>()?;
        return Ok(TimeAxis::Piecewise(segments));
    }
    if let Some(value) = item.get("explicit") {
        let explicit = object(value, &format!("{path}.explicit"))?;
        return Ok(TimeAxis::Explicit {
            timestamps: parse_content(field(explicit, "timestamps", path)?, path)?,
            count: unsigned(field(explicit, "count", path)?, path)?,
        });
    }
    Err(parse_error(path, "unknown time axis"))
}

fn parse_segment(value: &Value, path: &str) -> PyResult<TimeSegment> {
    let item = object(value, path)?;
    TimeSegment::new(
        rational(field(item, "start", path)?, &format!("{path}.start"))?,
        rational(field(item, "rate", path)?, &format!("{path}.rate"))?,
        unsigned(field(item, "samples", path)?, &format!("{path}.samples"))?,
    )
    .map_err(|error| parse_error(path, &error.to_string()))
}

fn parse_columns(value: &Value, path: &str) -> PyResult<Vec<TableColumn>> {
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
                parse_element(string(
                    field(item, "element", &column_path)?,
                    &format!("{column_path}.element"),
                )?)?,
                boolean(
                    field(item, "nullable", &column_path)?,
                    &format!("{column_path}.nullable"),
                )?,
            ))
        })
        .collect()
}

fn parse_catalog<T: SemanticTag>(
    value: &Value,
    path: &str,
) -> PyResult<abir_core::CatalogRecord<T>> {
    let item = object(value, path)?;
    let mut record = abir_core::CatalogRecord::new(
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

fn parse_source_keys(value: &Value, path: &str) -> PyResult<Vec<SourceKey>> {
    array(value, path)?
        .iter()
        .enumerate()
        .map(|(index, value)| parse_source_key(value, &format!("{path}[{index}]")))
        .collect()
}

fn parse_source_key(value: &Value, path: &str) -> PyResult<SourceKey> {
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

fn parse_semantic_ref(value: &Value, path: &str) -> PyResult<SemanticRef> {
    let item = object(value, path)?;
    let id = field(item, "id", path)?;
    Ok(
        match string(field(item, "kind", path)?, &format!("{path}.kind"))? {
            "dataset" => SemanticRef::of(parse_id::<DatasetTag>(id, path)?),
            "recording" => SemanticRef::of(parse_id::<RecordingTag>(id, path)?),
            "stream" => SemanticRef::of(parse_id::<StreamTag>(id, path)?),
            "atom" => SemanticRef::of(parse_id::<AtomTag>(id, path)?),
            "clock" => SemanticRef::of(parse_id::<ClockTag>(id, path)?),
            "coordinate-frame" => SemanticRef::of(parse_id::<CoordinateFrameTag>(id, path)?),
            "channel-basis" => SemanticRef::of(parse_id::<ChannelBasisTag>(id, path)?),
            "policy" => SemanticRef::of(parse_id::<PolicyTag>(id, path)?),
            "proof" => SemanticRef::of(parse_id::<ProofTag>(id, path)?),
            "derivation" => SemanticRef::of(parse_id::<DerivationTag>(id, path)?),
            "subject" => SemanticRef::of(parse_id::<SubjectTag>(id, path)?),
            "patient" => SemanticRef::of(parse_id::<PatientTag>(id, path)?),
            "session" => SemanticRef::of(parse_id::<SessionTag>(id, path)?),
            "acquisition" => SemanticRef::of(parse_id::<AcquisitionTag>(id, path)?),
            "device" => SemanticRef::of(parse_id::<DeviceTag>(id, path)?),
            "sensor" => SemanticRef::of(parse_id::<SensorTag>(id, path)?),
            "channel" => SemanticRef::of(parse_id::<ChannelTag>(id, path)?),
            "clock-relation" => SemanticRef::of(parse_id::<ClockRelationTag>(id, path)?),
            "frame-transform" => SemanticRef::of(parse_id::<FrameTransformTag>(id, path)?),
            "event" => SemanticRef::of(parse_id::<EventTag>(id, path)?),
            "concept-dictionary" => SemanticRef::of(parse_id::<ConceptDictionaryTag>(id, path)?),
            "derived-artifact" => SemanticRef::of(parse_id::<DerivedArtifactTag>(id, path)?),
            _ => {
                return Err(parse_error(
                    &format!("{path}.kind"),
                    "unknown semantic reference kind",
                ))
            }
        },
    )
}

fn exact_matrix(value: &Value, path: &str) -> PyResult<[ExactNumber; 16]> {
    let values = array(value, path)?
        .iter()
        .enumerate()
        .map(|(index, value)| exact(value, &format!("{path}[{index}]")))
        .collect::<PyResult<Vec<_>>>()?;
    values
        .try_into()
        .map_err(|_| parse_error(path, "expected exactly 16 transform values"))
}

fn exact(value: &Value, path: &str) -> PyResult<ExactNumber> {
    let item = object(value, path)?;
    if let Some(value) = item.get("$integer") {
        return string(value, path)?
            .parse::<i128>()
            .map(ExactNumber::Integer)
            .map_err(|_| parse_error(path, "integer is outside i64"));
    }
    Ok(ExactNumber::Rational(rational(value, path)?))
}

fn rational(value: &Value, path: &str) -> PyResult<Rational> {
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

fn parse_presence(value: &Value, path: &str) -> PyResult<Presence> {
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

fn parse_reference(value: &Value, path: &str) -> PyResult<ReferenceKind> {
    match string(value, path)? {
        "absolute" => Ok(ReferenceKind::Absolute),
        "common" => Ok(ReferenceKind::Common),
        "differential" => Ok(ReferenceKind::Differential),
        "unknown" => Ok(ReferenceKind::Unknown),
        _ => Err(parse_error(path, "unknown reference kind")),
    }
}

fn parse_shape(value: &Value, path: &str) -> PyResult<Vec<u64>> {
    array(value, path)?
        .iter()
        .enumerate()
        .map(|(index, value)| unsigned(value, &format!("{path}[{index}]")))
        .collect()
}

fn parse_id<T>(value: &Value, path: &str) -> PyResult<ObjectId<T>> {
    Ok(ObjectId::from_bytes(parse_hex_at::<16>(
        string(value, path)?,
        path,
    )?))
}
fn parse_content(value: &Value, path: &str) -> PyResult<ContentId> {
    Ok(ContentId::from_bytes(parse_hex_at::<32>(
        string(value, path)?,
        path,
    )?))
}
fn optional_id<T>(value: Option<&Value>, path: &str) -> PyResult<Option<ObjectId<T>>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => parse_id(value, path).map(Some),
    }
}
fn optional_concept(value: Option<&Value>, path: &str) -> PyResult<Option<ConceptId>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => concept(value, path).map(Some),
    }
}
fn optional_string(value: Option<&Value>, path: &str) -> PyResult<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => string(value, path).map(str::to_owned).map(Some),
    }
}
fn concept(value: &Value, path: &str) -> PyResult<ConceptId> {
    ConceptId::new(string(value, path)?).map_err(|error| parse_error(path, &error.to_string()))
}
fn values<'a>(object: &'a Map<String, Value>, name: &str) -> PyResult<&'a Vec<Value>> {
    array(field(object, name, "$")?, &format!("$.{name}"))
}
fn field<'a>(object: &'a Map<String, Value>, name: &str, path: &str) -> PyResult<&'a Value> {
    object
        .get(name)
        .ok_or_else(|| parse_error(path, &format!("missing member {name}")))
}
fn object<'a>(value: &'a Value, path: &str) -> PyResult<&'a Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| parse_error(path, "expected object"))
}
fn array<'a>(value: &'a Value, path: &str) -> PyResult<&'a Vec<Value>> {
    value
        .as_array()
        .ok_or_else(|| parse_error(path, "expected array"))
}
fn string<'a>(value: &'a Value, path: &str) -> PyResult<&'a str> {
    value
        .as_str()
        .ok_or_else(|| parse_error(path, "expected string"))
}
fn unsigned(value: &Value, path: &str) -> PyResult<u64> {
    value
        .as_u64()
        .ok_or_else(|| parse_error(path, "expected unsigned integer"))
}
fn boolean(value: &Value, path: &str) -> PyResult<bool> {
    value
        .as_bool()
        .ok_or_else(|| parse_error(path, "expected boolean"))
}
fn parse_error(path: &str, message: &str) -> PyErr {
    PyValueError::new_err(format!("{message} at {path}"))
}

fn parse_hex_at<const N: usize>(value: &str, path: &str) -> PyResult<[u8; N]> {
    parse_hex::<N>(value).map_err(|error| parse_error(path, &error.to_string()))
}

fn parse_object_id<T>(value: &str) -> PyResult<ObjectId<T>> {
    Ok(ObjectId::from_bytes(parse_hex::<16>(value)?))
}

fn parse_content_id(value: &str) -> PyResult<ContentId> {
    Ok(ContentId::from_bytes(parse_hex::<32>(value)?))
}

fn parse_hex<const N: usize>(value: &str) -> PyResult<[u8; N]> {
    if value.len() != N * 2 {
        return Err(PyValueError::new_err(format!(
            "expected {} lower-case hexadecimal characters",
            N * 2
        )));
    }
    let mut bytes = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_digit(pair[0])?;
        let low = hex_digit(pair[1])?;
        bytes[index] = (high << 4) | low;
    }
    Ok(bytes)
}

fn hex_digit(value: u8) -> PyResult<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(PyValueError::new_err(
            "identifiers must use lower-case hexadecimal",
        )),
    }
}

fn parse_element(value: &str) -> PyResult<ElementType> {
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
        "bytes" => Ok(ElementType::Bytes),
        _ => Err(PyValueError::new_err(
            "unsupported fixed-width element type",
        )),
    }
}

fn parse_byte_order(value: &str) -> PyResult<ByteOrder> {
    match value {
        "little" => Ok(ByteOrder::Little),
        "big" => Ok(ByteOrder::Big),
        "not-applicable" => Ok(ByteOrder::NotApplicable),
        _ => Err(PyValueError::new_err("unknown byte order")),
    }
}

fn parse_layout(value: &str) -> PyResult<Layout> {
    match value {
        "dense-row-major" => Ok(Layout::DenseRowMajor),
        "dense-column-major" => Ok(Layout::DenseColumnMajor),
        _ => Err(PyValueError::new_err(
            "Python tensor construction currently supports dense layouts",
        )),
    }
}

fn numpy_dtype(element: ElementType, byte_order: ByteOrder) -> PyResult<&'static str> {
    let little = matches!(byte_order, ByteOrder::Little);
    match (element, little) {
        (ElementType::I8, _) => Ok("i1"),
        (ElementType::U8 | ElementType::Bytes, _) => Ok("u1"),
        (ElementType::Bool, _) => Ok("?"),
        (ElementType::I16, true) => Ok("<i2"),
        (ElementType::I16, false) => Ok(">i2"),
        (ElementType::U16, true) => Ok("<u2"),
        (ElementType::U16, false) => Ok(">u2"),
        (ElementType::I32, true) => Ok("<i4"),
        (ElementType::I32, false) => Ok(">i4"),
        (ElementType::U32, true) => Ok("<u4"),
        (ElementType::U32, false) => Ok(">u4"),
        (ElementType::I64, true) => Ok("<i8"),
        (ElementType::I64, false) => Ok(">i8"),
        (ElementType::U64, true) => Ok("<u8"),
        (ElementType::U64, false) => Ok(">u8"),
        (ElementType::F16, true) => Ok("<f2"),
        (ElementType::F16, false) => Ok(">f2"),
        (ElementType::F32, true) => Ok("<f4"),
        (ElementType::F32, false) => Ok(">f4"),
        (ElementType::F64, true) => Ok("<f8"),
        (ElementType::F64, false) => Ok(">f8"),
        (ElementType::I24 | ElementType::Utf8, _) => Err(PyValueError::new_err(
            "element type has no direct NumPy dtype",
        )),
    }
}

#[pyfunction]
fn version() -> &'static str {
    abir_core::VERSION
}

#[pymodule]
fn abir(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyDataset>()?;
    module.add_function(wrap_pyfunction!(version, module)?)?;
    Ok(())
}
