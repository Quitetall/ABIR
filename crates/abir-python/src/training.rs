use abir_bcs::{ResourceBounds, SemanticPayloadFrame};
use abir_core::{payload_content_id, ByteOrder, ContentId, ElementType};
use abir_training::{
    encode_snapshot, ContentKey, TrainingProfile, TrainingRow, TrainingSnapshot,
    TrainingWindowStore,
};
use pyo3::exceptions::{PyKeyError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyModule, PyTuple};

#[derive(Debug)]
struct RowLocation {
    byte_order: ByteOrder,
    element: ElementType,
    logical_id: String,
    logical_bytes: usize,
    offset: usize,
    shape: Vec<u64>,
}

/// A validated, immutable view of a sealed ABIR BCS2 training bundle.
///
/// The Python object owns the original `bytes` object. NumPy rows use that
/// object as their buffer owner, so a row remains valid after this store is
/// released without copying the frame payload.
#[pyclass(name = "TrainingWindowStore", frozen)]
pub(crate) struct PyTrainingWindowStore {
    artifact: Py<PyBytes>,
    profile: &'static str,
    rows: Vec<RowLocation>,
    snapshot_id: String,
}

#[pymethods]
impl PyTrainingWindowStore {
    #[staticmethod]
    fn open_bytes(py: Python<'_>, artifact: Py<PyBytes>) -> PyResult<Self> {
        let artifact_bytes = artifact.bind(py).as_bytes();
        let store = TrainingWindowStore::open(artifact_bytes, ResourceBounds::default())
            .map_err(training_error)?;
        let base = artifact_bytes.as_ptr() as usize;
        let artifact_len = artifact_bytes.len();
        let rows = store
            .rows()
            .map(|lease| {
                let bytes = lease.bytes();
                let offset = (bytes.as_ptr() as usize)
                    .checked_sub(base)
                    .filter(|offset| {
                        offset
                            .checked_add(bytes.len())
                            .is_some_and(|end| end <= artifact_len)
                    })
                    .ok_or_else(|| {
                        PyValueError::new_err("validated training row is outside its artifact")
                    })?;
                Ok(RowLocation {
                    byte_order: lease.byte_order(),
                    element: lease.element(),
                    logical_id: lease.metadata().logical_id.to_string(),
                    logical_bytes: bytes.len(),
                    offset,
                    shape: lease.shape().to_vec(),
                })
            })
            .collect::<PyResult<Vec<_>>>()?;
        let snapshot_id = store
            .snapshot()
            .content_id()
            .map_err(training_error)?
            .to_string();
        let profile = profile_name(store.snapshot().profile());
        drop(store);

        Ok(Self {
            artifact,
            profile,
            rows,
            snapshot_id,
        })
    }

    #[getter]
    fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    #[getter]
    fn profile(&self) -> &str {
        self.profile
    }

    #[getter]
    fn row_count(&self) -> usize {
        self.rows.len()
    }

    #[getter]
    fn row_ids(&self) -> Vec<&str> {
        self.rows
            .iter()
            .map(|row| row.logical_id.as_str())
            .collect()
    }

    fn row_numpy(&self, py: Python<'_>, logical_id: &str) -> PyResult<Py<PyAny>> {
        let row = self.row(logical_id)?;
        let dtype = super::numpy_dtype(row.element, row.byte_order)?;
        let width = row.element.byte_width().ok_or_else(|| {
            PyValueError::new_err("training row element has no fixed-width NumPy dtype")
        })?;
        let count = row
            .logical_bytes
            .checked_div(usize::try_from(width).map_err(|_| {
                PyValueError::new_err("training row element width exceeds the host")
            })?)
            .ok_or_else(|| PyValueError::new_err("training row has an invalid element width"))?;

        let numpy = PyModule::import_bound(py, "numpy")?;
        let kwargs = PyDict::new_bound(py);
        kwargs.set_item("dtype", dtype)?;
        kwargs.set_item("count", count)?;
        kwargs.set_item("offset", row.offset)?;
        let array = numpy.call_method("frombuffer", (self.artifact.bind(py),), Some(&kwargs))?;
        let shape = PyTuple::new_bound(py, row.shape.iter().copied());
        let reshaped = array.call_method("reshape", shape, None)?;
        reshaped.getattr("flags")?.setattr("writeable", false)?;
        Ok(reshaped.unbind())
    }

    /// Diagnostic pointer used by parity tests and zero-copy evidence.
    fn row_pointer(&self, py: Python<'_>, logical_id: &str) -> PyResult<usize> {
        let row = self.row(logical_id)?;
        Ok(self.artifact.bind(py).as_bytes().as_ptr() as usize + row.offset)
    }
}

impl PyTrainingWindowStore {
    fn row(&self, logical_id: &str) -> PyResult<&RowLocation> {
        self.rows
            .binary_search_by(|row| row.logical_id.as_str().cmp(logical_id))
            .ok()
            .map(|index| &self.rows[index])
            .ok_or_else(|| PyKeyError::new_err(logical_id.to_owned()))
    }
}

fn profile_name(profile: TrainingProfile) -> &'static str {
    match profile {
        TrainingProfile::Speed => "speed",
        TrainingProfile::Balanced => "balanced",
        TrainingProfile::Memory => "memory",
        TrainingProfile::Compact => "compact",
        TrainingProfile::UltraCompact => "ultra-compact",
        TrainingProfile::Stream => "stream",
    }
}

fn training_error(error: impl core::fmt::Display) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn key(seed: u8) -> ContentKey {
    ContentKey::new(ContentId::from_bytes([seed; 32]))
}

/// Deterministic private fixture for cross-language ownership and corruption tests.
#[pyfunction(name = "_training_fixture_bytes")]
pub(crate) fn training_fixture_bytes(py: Python<'_>) -> PyResult<Bound<'_, PyBytes>> {
    let payload = [1_u8, 0, 2, 0, 3, 0, 4, 0];
    let row = TrainingRow {
        byte_order: ByteOrder::Little,
        group: key(5),
        label: key(6),
        logical_bytes: payload.len() as u64,
        logical_id: key(7),
        payload: ContentKey::new(payload_content_id(ElementType::I16, &payload)),
        element: ElementType::I16,
        shape: vec![2, 2],
        split: key(8),
    };
    let snapshot = TrainingSnapshot::seal(
        vec![key(1)],
        key(2),
        TrainingProfile::Balanced,
        vec![row],
        key(3),
    )
    .map_err(training_error)?;
    let encoded = encode_snapshot(
        &snapshot,
        &[SemanticPayloadFrame::new(ElementType::I16, &payload)],
        ResourceBounds::default(),
    )
    .map_err(training_error)?;
    Ok(PyBytes::new_bound(py, &encoded))
}
