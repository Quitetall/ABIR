use abir_core::{
    canonical_debug_json, logical_content_id, parse_canonical_dataset, Atom, AtomTag, ByteOrder,
    Clock, ConceptId, ContentId, DatasetDraft, DatasetTag, ElementType, Layout, ObjectId,
    PayloadDescriptor, Presence, Rational, Recording, RecordingTag, SemanticAxis, Stream,
    StreamTag, Tensor, ValidationLimits,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyModule, PyTuple};

mod training;

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
        let inner = parse_canonical_dataset(document)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
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
    module.add_class::<training::PyTrainingWindowStore>()?;
    module.add_function(wrap_pyfunction!(training::seal_training_snapshot, module)?)?;
    #[cfg(feature = "test-fixtures")]
    module.add_function(wrap_pyfunction!(training::training_fixture_bytes, module)?)?;
    module.add_function(wrap_pyfunction!(version, module)?)?;
    Ok(())
}
