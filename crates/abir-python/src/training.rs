use abir_bcs::ResourceBounds;
#[cfg(feature = "test-fixtures")]
use abir_bcs::SemanticPayloadFrame;
use abir_core::{payload_content_id, ByteOrder, ContentId, ElementType};
#[cfg(feature = "test-fixtures")]
use abir_training::{encode_snapshot, ContentKey, TrainingRow, TrainingSnapshot};
use abir_training::{DecisionLogReplayState, TrainingProfile, TrainingWindowStore};
use memmap2::MmapOptions;
use pyo3::exceptions::{PyKeyError, PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyModule, PyTuple};
use std::fs::File;
use std::io::{copy, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Mutex;

enum ArtifactOwner {
    Bytes(Py<PyBytes>),
    PathFile { file: Mutex<File> },
}

impl ArtifactOwner {
    const fn backing(&self) -> &'static str {
        match self {
            Self::Bytes(_) => "bytes-zero-copy",
            Self::PathFile { .. } => "path-private-validation",
        }
    }

    const fn materializes_rows(&self) -> bool {
        matches!(self, Self::PathFile { .. })
    }
}

#[derive(Debug)]
struct RowLocation {
    byte_order: ByteOrder,
    element: ElementType,
    group: String,
    label: String,
    logical_id: String,
    logical_bytes: usize,
    offset: usize,
    payload_id: ContentId,
    shape: Vec<u64>,
    split: String,
}

/// A validated, immutable view of a sealed ABIR BCS2 training bundle.
///
/// The Python object owns the original `bytes` object. NumPy rows use that
/// object as their buffer owner, so a row remains valid after this store is
/// released without copying the frame payload.
#[pyclass(name = "TrainingWindowStore", frozen)]
pub(crate) struct PyTrainingWindowStore {
    artifact: ArtifactOwner,
    dataset_roots: Vec<String>,
    decision_log_id: String,
    profile: &'static str,
    rows: Vec<RowLocation>,
    snapshot_id: String,
    spec_id: String,
}

#[pymethods]
impl PyTrainingWindowStore {
    #[staticmethod]
    fn open_bytes(py: Python<'_>, artifact: Py<PyBytes>) -> PyResult<Self> {
        let artifact_bytes = artifact.bind(py).as_bytes();
        let metadata = inspect_artifact(artifact_bytes)?;

        Ok(Self {
            artifact: ArtifactOwner::Bytes(artifact),
            dataset_roots: metadata.dataset_roots,
            decision_log_id: metadata.decision_log_id,
            profile: metadata.profile,
            rows: metadata.rows,
            snapshot_id: metadata.snapshot_id,
            spec_id: metadata.spec_id,
        })
    }

    /// Open and validate a BCS2 training artifact through a read-only mmap.
    ///
    /// The artifact itself is never copied into the Python heap. Because the
    /// abi3-py310 buffer ABI cannot safely make the Rust mmap a NumPy owner,
    /// `row_numpy` materializes only the selected row and reports that policy
    /// through `materializes_rows` and `row_info`.
    #[staticmethod]
    fn open_path(path: PathBuf) -> PyResult<Self> {
        let file = File::open(&path)
            .map_err(|error| PyOSError::new_err(format!("open {}: {error}", path.display())))?;
        fs2::FileExt::lock_shared(&file).map_err(|error| {
            PyOSError::new_err(format!(
                "lock training artifact {} for shared reading: {error}",
                path.display()
            ))
        })?;
        let metadata = file
            .metadata()
            .map_err(|error| PyOSError::new_err(format!("inspect {}: {error}", path.display())))?;
        if !metadata.is_file() {
            return Err(PyValueError::new_err(
                "training artifact must be a regular file",
            ));
        }
        if metadata.len() == 0 {
            return Err(PyValueError::new_err("training artifact is empty"));
        }
        // A typed file-backed mmap is only sound when its inode cannot change.
        // Unix locks are advisory, so first stream the artifact through a
        // bounded I/O buffer into an anonymous private file. This does not
        // materialize the artifact in the Python or Rust heap, and no other
        // process can name or mutate the private validation inode.
        let mut private = tempfile::tempfile().map_err(|error| {
            PyOSError::new_err(format!("create private validation file: {error}"))
        })?;
        let copy_limit = metadata
            .len()
            .checked_add(1)
            .ok_or_else(|| PyValueError::new_err("training artifact size exceeds u64"))?;
        let mut source = (&file).take(copy_limit);
        let copied = copy(&mut source, &mut private)
            .map_err(|error| PyOSError::new_err(format!("copy training artifact: {error}")))?;
        if copied != metadata.len() {
            return Err(PyValueError::new_err(
                "training artifact changed size during validation",
            ));
        }
        private
            .seek(SeekFrom::Start(0))
            .map_err(|error| PyOSError::new_err(format!("rewind validation file: {error}")))?;
        // SAFETY: `private` is an anonymous process-private file and remains
        // alive, unchanged, until this temporary map is dropped below.
        let mmap = unsafe { MmapOptions::new().map(&private) }.map_err(|error| {
            PyOSError::new_err(format!("map training artifact {}: {error}", path.display()))
        })?;
        let metadata = inspect_artifact(&mmap)?;
        drop(mmap);

        Ok(Self {
            artifact: ArtifactOwner::PathFile {
                file: Mutex::new(file),
            },
            dataset_roots: metadata.dataset_roots,
            decision_log_id: metadata.decision_log_id,
            profile: metadata.profile,
            rows: metadata.rows,
            snapshot_id: metadata.snapshot_id,
            spec_id: metadata.spec_id,
        })
    }

    #[getter]
    fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    #[getter]
    fn spec_id(&self) -> &str {
        &self.spec_id
    }

    #[getter]
    fn dataset_roots<'py>(&self, py: Python<'py>) -> Bound<'py, PyTuple> {
        PyTuple::new_bound(py, self.dataset_roots.iter())
    }

    #[getter]
    fn decision_log_id(&self) -> &str {
        &self.decision_log_id
    }

    /// The snapshot binds the decision-log identity, but carries no records
    /// from which the decision log could be replayed.
    #[getter]
    fn decision_log_replay_state(&self) -> &'static str {
        DecisionLogReplayState::IdentityBound.as_str()
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
    fn backing(&self) -> &'static str {
        self.artifact.backing()
    }

    #[getter]
    fn materializes_rows(&self) -> bool {
        self.artifact.materializes_rows()
    }

    #[getter]
    fn row_ids(&self) -> Vec<&str> {
        self.rows
            .iter()
            .map(|row| row.logical_id.as_str())
            .collect()
    }

    fn row_info<'py>(&self, py: Python<'py>, logical_id: &str) -> PyResult<Bound<'py, PyDict>> {
        let row = self.row(logical_id)?;
        let info = PyDict::new_bound(py);
        info.set_item("logical_id", &row.logical_id)?;
        info.set_item("group", &row.group)?;
        info.set_item("label", &row.label)?;
        info.set_item("split", &row.split)?;
        info.set_item("payload", row.payload_id.to_string())?;
        info.set_item("element", element_name(row.element))?;
        info.set_item("byte_order", byte_order_name(row.byte_order))?;
        info.set_item("logical_bytes", row.logical_bytes)?;
        info.set_item("shape", &row.shape)?;
        info.set_item("materialized", self.artifact.materializes_rows())?;
        info.set_item("backing", self.artifact.backing())?;
        Ok(info)
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
        let (buffer, offset) = match &self.artifact {
            ArtifactOwner::Bytes(artifact) => (artifact.clone_ref(py).into_any(), row.offset),
            ArtifactOwner::PathFile { file } => {
                let offset = u64::try_from(row.offset)
                    .map_err(|_| PyValueError::new_err("training row offset exceeds u64"))?;
                let bytes = PyBytes::new_bound_with(py, row.logical_bytes, |destination| {
                    let mut file = file.lock().map_err(|_| {
                        PyOSError::new_err("training artifact file lock is poisoned")
                    })?;
                    file.seek(SeekFrom::Start(offset)).map_err(|error| {
                        PyOSError::new_err(format!("seek training row: {error}"))
                    })?;
                    file.read_exact(destination).map_err(|error| {
                        PyOSError::new_err(format!("read training row: {error}"))
                    })?;
                    let actual = payload_content_id(row.element, destination);
                    if actual != row.payload_id {
                        return Err(PyValueError::new_err(format!(
                            "training row payload changed after validation: expected {}, got {}",
                            row.payload_id, actual
                        )));
                    }
                    Ok(())
                })?;
                (bytes.unbind().into_any(), 0)
            }
        };
        let kwargs = PyDict::new_bound(py);
        kwargs.set_item("dtype", dtype)?;
        kwargs.set_item("count", count)?;
        kwargs.set_item("offset", offset)?;
        let array = numpy.call_method("frombuffer", (buffer.bind(py),), Some(&kwargs))?;
        let shape = PyTuple::new_bound(py, row.shape.iter().copied());
        let reshaped = array.call_method("reshape", shape, None)?;
        reshaped.getattr("flags")?.setattr("writeable", false)?;
        Ok(reshaped.unbind())
    }
}

struct SnapshotMetadata {
    dataset_roots: Vec<String>,
    decision_log_id: String,
    profile: &'static str,
    rows: Vec<RowLocation>,
    snapshot_id: String,
    spec_id: String,
}

fn inspect_artifact(artifact: &[u8]) -> PyResult<SnapshotMetadata> {
    let store =
        TrainingWindowStore::open(artifact, ResourceBounds::default()).map_err(training_error)?;
    let base = artifact.as_ptr() as usize;
    let artifact_len = artifact.len();
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
                group: lease.group().to_string(),
                label: lease.label().to_string(),
                logical_id: lease.metadata().logical_id.to_string(),
                logical_bytes: bytes.len(),
                offset,
                payload_id: lease.metadata().payload.content_id(),
                shape: lease.shape().to_vec(),
                split: lease.split().to_string(),
            })
        })
        .collect::<PyResult<Vec<_>>>()?;
    let snapshot_id = store
        .snapshot()
        .content_id()
        .map_err(training_error)?
        .to_string();
    Ok(SnapshotMetadata {
        dataset_roots: store
            .dataset_roots()
            .iter()
            .map(ToString::to_string)
            .collect(),
        decision_log_id: store.decision_log_id().to_string(),
        profile: profile_name(store.snapshot().profile()),
        rows,
        snapshot_id,
        spec_id: store.spec_id().to_string(),
    })
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

fn element_name(element: ElementType) -> &'static str {
    match element {
        ElementType::I8 => "i8",
        ElementType::I16 => "i16",
        ElementType::I24 => "i24",
        ElementType::I32 => "i32",
        ElementType::I64 => "i64",
        ElementType::U8 => "u8",
        ElementType::U16 => "u16",
        ElementType::U32 => "u32",
        ElementType::U64 => "u64",
        ElementType::F16 => "f16",
        ElementType::F32 => "f32",
        ElementType::F64 => "f64",
        ElementType::Bool => "bool",
        ElementType::Utf8 => "utf8",
        ElementType::Bytes => "bytes",
    }
}

fn byte_order_name(byte_order: ByteOrder) -> &'static str {
    match byte_order {
        ByteOrder::Little => "little",
        ByteOrder::Big => "big",
        ByteOrder::NotApplicable => "not-applicable",
    }
}

fn training_error(error: impl core::fmt::Display) -> PyErr {
    PyValueError::new_err(error.to_string())
}

#[cfg(feature = "test-fixtures")]
fn key(seed: u8) -> ContentKey {
    ContentKey::new(ContentId::from_bytes([seed; 32]))
}

/// Deterministic private fixture for cross-language ownership and corruption tests.
#[pyfunction(name = "_training_fixture_bytes", signature = (payload_bytes=8))]
#[cfg(feature = "test-fixtures")]
pub(crate) fn training_fixture_bytes(
    py: Python<'_>,
    payload_bytes: usize,
) -> PyResult<Bound<'_, PyBytes>> {
    if payload_bytes == 0 || payload_bytes % 2 != 0 {
        return Err(PyValueError::new_err(
            "training fixture payload size must be a positive multiple of two",
        ));
    }
    let pattern = [1_u8, 0, 2, 0, 3, 0, 4, 0];
    let payload: Vec<u8> = pattern
        .iter()
        .copied()
        .cycle()
        .take(payload_bytes)
        .collect();
    let shape = if payload_bytes == pattern.len() {
        vec![2, 2]
    } else {
        vec![(payload_bytes / 2) as u64]
    };
    let row = TrainingRow {
        byte_order: ByteOrder::Little,
        group: key(5),
        label: key(6),
        logical_bytes: payload_bytes as u64,
        logical_id: key(7),
        payload: ContentKey::new(payload_content_id(ElementType::I16, &payload)),
        element: ElementType::I16,
        shape,
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
