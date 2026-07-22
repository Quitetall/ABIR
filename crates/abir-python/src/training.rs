use abir_bcs::{ResourceBounds, SemanticPayloadFrame};
use abir_core::{payload_content_id, ByteOrder, ContentId, ElementType, Presence};
use abir_training::{
    compile_execution_plan, encode_snapshot, ContentKey, ContinualPromotion, DatasetSubscription,
    DecisionLog, DecisionRecord, DecisionReplayReceipt, MicroSnapshot, PlanOverrides,
    SourceEquivalenceReceipt, SubscriptionCorrection, TrainingAssociatedPayload,
    TrainingLabelPayloadAssociation, TrainingRow, TrainingSnapshot, TrainingSpec,
};
use abir_training::{DecisionLogReplayState, TrainingProfile, TrainingWindowStore};
use memmap2::MmapOptions;
use pyo3::exceptions::{PyKeyError, PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyModule, PyString, PyTuple};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
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

#[derive(Debug)]
enum LabelPayloadLocation {
    Present {
        byte_order: ByteOrder,
        concept: String,
        element: ElementType,
        logical_bytes: usize,
        offset: usize,
        payload_id: ContentId,
        shape: Vec<u64>,
    },
    Unavailable {
        concept: String,
        presence: Presence,
    },
}

impl LabelPayloadLocation {
    fn concept(&self) -> &str {
        match self {
            Self::Present { concept, .. } | Self::Unavailable { concept, .. } => concept,
        }
    }

    const fn presence(&self) -> Presence {
        match self {
            Self::Present { .. } => Presence::Present,
            Self::Unavailable { presence, .. } => *presence,
        }
    }
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
    label_payloads: Vec<Vec<LabelPayloadLocation>>,
    physical_artifact_sha256: String,
    snapshot_id: String,
    spec_id: String,
}

#[pymethods]
impl PyTrainingWindowStore {
    #[staticmethod]
    fn open_bytes(py: Python<'_>, artifact: Py<PyBytes>) -> PyResult<Self> {
        let artifact_bytes = artifact.bind(py).as_bytes();
        let metadata = inspect_artifact(artifact_bytes)?;
        let physical_artifact_sha256 = format!("{:x}", Sha256::digest(artifact_bytes));

        Ok(Self {
            artifact: ArtifactOwner::Bytes(artifact),
            dataset_roots: metadata.dataset_roots,
            decision_log_id: metadata.decision_log_id,
            profile: metadata.profile,
            rows: metadata.rows,
            label_payloads: metadata.label_payloads,
            physical_artifact_sha256,
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
        let physical_artifact_sha256 = copy_and_hash_artifact(&file, &mut private, metadata.len())?;
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
                file: Mutex::new(private),
            },
            dataset_roots: metadata.dataset_roots,
            decision_log_id: metadata.decision_log_id,
            profile: metadata.profile,
            rows: metadata.rows,
            label_payloads: metadata.label_payloads,
            physical_artifact_sha256,
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

    /// SHA-256 of the exact immutable bytes retained by this native store.
    ///
    /// This is physical evidence only and never participates in ABIR semantic
    /// identity. Path-backed stores hash and retain their anonymous validation
    /// file, so replacing or unlinking the source pathname cannot change it.
    #[getter]
    fn physical_artifact_sha256(&self) -> &str {
        &self.physical_artifact_sha256
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
        numpy_from_location(
            &self.artifact,
            py,
            "row",
            row.element,
            row.byte_order,
            row.logical_bytes,
            row.payload_id,
            row.offset,
            &row.shape,
        )
    }

    fn row_label_payload_info<'py>(
        &self,
        py: Python<'py>,
        logical_id: &str,
        concept: &str,
    ) -> PyResult<Bound<'py, PyDict>> {
        let association = self.label_payload(logical_id, concept)?;
        let info = PyDict::new_bound(py);
        info.set_item("concept", association.concept())?;
        info.set_item("presence", presence_name(association.presence()))?;
        if let LabelPayloadLocation::Present {
            byte_order,
            element,
            logical_bytes,
            payload_id,
            shape,
            ..
        } = association
        {
            info.set_item("payload", payload_id.to_string())?;
            info.set_item("element", element_name(*element))?;
            info.set_item("byte_order", byte_order_name(*byte_order))?;
            info.set_item("logical_bytes", logical_bytes)?;
            info.set_item("shape", shape)?;
            info.set_item("materialized", self.artifact.materializes_rows())?;
            info.set_item("backing", self.artifact.backing())?;
        }
        Ok(info)
    }

    fn row_label_payload_numpy(
        &self,
        py: Python<'_>,
        logical_id: &str,
        concept: &str,
    ) -> PyResult<Py<PyAny>> {
        let association = self.label_payload(logical_id, concept)?;
        match association {
            LabelPayloadLocation::Present {
                byte_order,
                element,
                logical_bytes,
                offset,
                payload_id,
                shape,
                ..
            } => numpy_from_location(
                &self.artifact,
                py,
                "label",
                *element,
                *byte_order,
                *logical_bytes,
                *payload_id,
                *offset,
                shape,
            ),
            LabelPayloadLocation::Unavailable { presence, .. } => {
                Err(PyValueError::new_err(format!(
                    "label payload {concept:?} for row {logical_id} is {}",
                    presence_name(*presence)
                )))
            }
        }
    }
}

fn copy_and_hash_artifact(
    source: &File,
    destination: &mut File,
    expected_bytes: u64,
) -> PyResult<String> {
    let limit = expected_bytes
        .checked_add(1)
        .ok_or_else(|| PyValueError::new_err("training artifact size exceeds u64"))?;
    let mut source = source.take(limit);
    let mut buffer = [0_u8; 64 * 1024];
    let mut copied = 0_u64;
    let mut digest = Sha256::new();
    loop {
        let count = source
            .read(&mut buffer)
            .map_err(|error| PyOSError::new_err(format!("read training artifact: {error}")))?;
        if count == 0 {
            break;
        }
        destination.write_all(&buffer[..count]).map_err(|error| {
            PyOSError::new_err(format!("copy training artifact into private file: {error}"))
        })?;
        digest.update(&buffer[..count]);
        copied = copied
            .checked_add(u64::try_from(count).expect("buffer length fits in u64"))
            .ok_or_else(|| PyValueError::new_err("training artifact size exceeds u64"))?;
    }
    if copied != expected_bytes {
        return Err(PyValueError::new_err(
            "training artifact changed size during validation",
        ));
    }
    destination
        .flush()
        .map_err(|error| PyOSError::new_err(format!("flush private validation file: {error}")))?;
    Ok(format!("{:x}", digest.finalize()))
}

struct SnapshotMetadata {
    dataset_roots: Vec<String>,
    decision_log_id: String,
    profile: &'static str,
    rows: Vec<RowLocation>,
    label_payloads: Vec<Vec<LabelPayloadLocation>>,
    snapshot_id: String,
    spec_id: String,
}

#[allow(clippy::too_many_arguments)]
fn numpy_from_location(
    artifact: &ArtifactOwner,
    py: Python<'_>,
    kind: &str,
    element: ElementType,
    byte_order: ByteOrder,
    logical_bytes: usize,
    payload_id: ContentId,
    row_offset: usize,
    shape: &[u64],
) -> PyResult<Py<PyAny>> {
    let dtype = super::numpy_dtype(element, byte_order)?;
    let width = element.byte_width().ok_or_else(|| {
        PyValueError::new_err(format!(
            "training {kind} element has no fixed-width NumPy dtype"
        ))
    })?;
    let count = logical_bytes
        .checked_div(usize::try_from(width).map_err(|_| {
            PyValueError::new_err(format!("training {kind} element width exceeds the host"))
        })?)
        .ok_or_else(|| {
            PyValueError::new_err(format!("training {kind} has an invalid element width"))
        })?;
    let numpy = PyModule::import_bound(py, "numpy")?;
    let (buffer, offset) = match artifact {
        ArtifactOwner::Bytes(artifact) => (artifact.clone_ref(py).into_any(), row_offset),
        ArtifactOwner::PathFile { file } => {
            let offset = u64::try_from(row_offset).map_err(|_| {
                PyValueError::new_err(format!("training {kind} offset exceeds u64"))
            })?;
            let bytes = PyBytes::new_bound_with(py, logical_bytes, |destination| {
                let mut file = file
                    .lock()
                    .map_err(|_| PyOSError::new_err("training artifact file lock is poisoned"))?;
                file.seek(SeekFrom::Start(offset)).map_err(|error| {
                    PyOSError::new_err(format!("seek training {kind}: {error}"))
                })?;
                file.read_exact(destination).map_err(|error| {
                    PyOSError::new_err(format!("read training {kind}: {error}"))
                })?;
                let actual = payload_content_id(element, destination);
                if actual != payload_id {
                    return Err(PyValueError::new_err(format!(
                        "training {kind} payload changed after validation: expected {payload_id}, got {actual}"
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
    let shape = PyTuple::new_bound(py, shape.iter().copied());
    let reshaped = array.call_method("reshape", shape, None)?;
    reshaped.getattr("flags")?.setattr("writeable", false)?;
    Ok(reshaped.unbind())
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
    let mut associations = store.snapshot().label_payloads().iter().peekable();
    let label_payloads = store
        .snapshot()
        .rows()
        .iter()
        .map(|row| {
            let mut row_payloads = Vec::new();
            while associations
                .peek()
                .is_some_and(|association| association.logical_id == row.logical_id)
            {
                let association = associations
                    .next()
                    .expect("peeked validated label association");
                let lease = store
                    .label_payload(association.logical_id, &association.concept)
                    .expect("validated label association");
                let location = match (&association.payload, lease.bytes()) {
                    (Some(payload), Some(bytes)) => {
                        let offset = (bytes.as_ptr() as usize)
                            .checked_sub(base)
                            .filter(|offset| {
                                offset
                                    .checked_add(bytes.len())
                                    .is_some_and(|end| end <= artifact_len)
                            })
                            .ok_or_else(|| {
                                PyValueError::new_err(
                                    "validated label payload is outside its artifact",
                                )
                            })?;
                        Ok(LabelPayloadLocation::Present {
                            byte_order: payload.byte_order,
                            concept: lease.concept().to_owned(),
                            element: payload.element,
                            logical_bytes: bytes.len(),
                            offset,
                            payload_id: payload.payload.content_id(),
                            shape: payload.shape.clone(),
                        })
                    }
                    (None, None) => Ok(LabelPayloadLocation::Unavailable {
                        concept: lease.concept().to_owned(),
                        presence: lease.presence(),
                    }),
                    _ => Err(PyValueError::new_err(
                        "validated label presence conflicts with its payload",
                    )),
                }?;
                row_payloads.push(location);
            }
            Ok(row_payloads)
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
        label_payloads,
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

    fn label_payload(&self, logical_id: &str, concept: &str) -> PyResult<&LabelPayloadLocation> {
        let row_index = self
            .rows
            .binary_search_by(|row| row.logical_id.as_str().cmp(logical_id))
            .map_err(|_| PyKeyError::new_err(logical_id.to_owned()))?;
        self.label_payloads[row_index]
            .binary_search_by(|association| association.concept().cmp(concept))
            .ok()
            .map(|index| &self.label_payloads[row_index][index])
            .ok_or_else(|| PyKeyError::new_err(concept.to_owned()))
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

fn presence_name(presence: Presence) -> &'static str {
    match presence {
        Presence::Present => "present",
        Presence::AbsentAtSource => "absent-at-source",
        Presence::UnknownAtSource => "unknown-at-source",
        Presence::Withheld => "withheld",
        Presence::Redacted => "redacted",
        Presence::NotApplicable => "not-applicable",
    }
}

fn training_error(error: impl core::fmt::Display) -> PyErr {
    PyValueError::new_err(error.to_string())
}

struct BoundTrainingRow<'py> {
    metadata: TrainingRow,
    payload: Bound<'py, PyBytes>,
}

struct BoundLabelAssociation<'py> {
    metadata: TrainingLabelPayloadAssociation,
    payload: Option<Bound<'py, PyBytes>>,
}

fn required_item<'py>(dictionary: &Bound<'py, PyDict>, key: &str) -> PyResult<Bound<'py, PyAny>> {
    dictionary
        .get_item(key)?
        .ok_or_else(|| PyValueError::new_err(format!("missing required field {key:?}")))
}

fn required_string(dictionary: &Bound<'_, PyDict>, key: &str) -> PyResult<String> {
    required_item(dictionary, key)?.extract()
}

fn require_exact_keys(dictionary: &Bound<'_, PyDict>, expected: &[&str]) -> PyResult<()> {
    for (key, _) in dictionary.iter() {
        let key: String = key
            .extract()
            .map_err(|_| PyValueError::new_err("training metadata keys must be strings"))?;
        if !expected.contains(&key.as_str()) {
            return Err(PyValueError::new_err(format!(
                "unknown training metadata field {key:?}"
            )));
        }
    }
    for key in expected {
        if !dictionary.contains(key)? {
            return Err(PyValueError::new_err(format!(
                "missing required field {key:?}"
            )));
        }
    }
    Ok(())
}

fn required_shape(dictionary: &Bound<'_, PyDict>) -> PyResult<Vec<u64>> {
    required_item(dictionary, "shape")?
        .downcast::<PyList>()
        .map_err(|_| PyValueError::new_err("shape must be a list"))?
        .extract()
}

fn required_payload<'py>(dictionary: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyBytes>> {
    let value = required_item(dictionary, "payload")?;
    let bytes = value
        .downcast::<PyBytes>()
        .map_err(|_| PyValueError::new_err("payload must be immutable bytes"))?;
    let bounds = ResourceBounds::default();
    if bytes.as_bytes().len() > bounds.max_frame_bytes as usize {
        return Err(PyValueError::new_err(
            "payload exceeds the ABIR BCS2 frame resource bound",
        ));
    }
    Ok(bytes.clone())
}

fn parse_profile(value: &str) -> PyResult<TrainingProfile> {
    match value {
        "speed" => Ok(TrainingProfile::Speed),
        "balanced" => Ok(TrainingProfile::Balanced),
        "memory" => Ok(TrainingProfile::Memory),
        "compact" => Ok(TrainingProfile::Compact),
        "ultra-compact" => Ok(TrainingProfile::UltraCompact),
        "stream" => Ok(TrainingProfile::Stream),
        _ => Err(PyValueError::new_err("unknown training profile")),
    }
}

/// Compile one registered training profile through the canonical Rust compiler.
///
/// The returned JSON is the exact canonical byte representation used to derive
/// `plan_id`. Hardware observations are intentionally not compiler inputs.
#[pyfunction]
pub(crate) fn compile_training_execution_plan<'py>(
    py: Python<'py>,
    profile: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let plan = compile_execution_plan(parse_profile(profile)?, PlanOverrides::default())
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    let canonical_json = String::from_utf8(
        plan.canonical_json()
            .map_err(|error| PyValueError::new_err(error.to_string()))?,
    )
    .map_err(|_| PyValueError::new_err("canonical training plan is not UTF-8"))?;
    let plan_id = plan
        .content_id()
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    let result = PyDict::new_bound(py);
    result.set_item("canonical_json", canonical_json)?;
    result.set_item("plan_id", plan_id.to_string())?;
    Ok(result)
}

fn parse_presence(value: &str) -> PyResult<Presence> {
    match value {
        "present" => Ok(Presence::Present),
        "absent-at-source" => Ok(Presence::AbsentAtSource),
        "unknown-at-source" => Ok(Presence::UnknownAtSource),
        "withheld" => Ok(Presence::Withheld),
        "redacted" => Ok(Presence::Redacted),
        "not-applicable" => Ok(Presence::NotApplicable),
        _ => Err(PyValueError::new_err("unknown label presence")),
    }
}

fn preflight_metadata(
    dataset_root_count: usize,
    rows: &Bound<'_, PyList>,
    label_payloads: &Bound<'_, PyList>,
    bounds: ResourceBounds,
) -> PyResult<()> {
    if rows.len() > bounds.max_index_entries as usize
        || label_payloads.len() > bounds.max_index_entries as usize
    {
        return Err(PyValueError::new_err(
            "training snapshot metadata exceeds the ABIR BCS2 index resource bound",
        ));
    }
    // These are strict lower bounds on canonical JSON bytes: every dataset
    // root contributes a 64-digit ID, every row carries five such IDs, and
    // every label association carries one ID plus required field names. Reject
    // counts whose catalog cannot possibly fit before constructing Rust
    // metadata or serializing canonical JSON.
    let minimum_catalog_bytes = dataset_root_count
        .checked_mul(64)
        .and_then(|total| {
            rows.len()
                .checked_mul(256)
                .and_then(|rows| total.checked_add(rows))
        })
        .and_then(|total| {
            label_payloads
                .len()
                .checked_mul(96)
                .and_then(|labels| total.checked_add(labels))
        })
        .ok_or_else(|| PyValueError::new_err("training catalog size estimate overflow"))?;
    if minimum_catalog_bytes > bounds.max_catalog_bytes as usize {
        return Err(PyValueError::new_err(
            "training snapshot metadata exceeds the ABIR BCS2 catalog resource bound",
        ));
    }
    let mut shape_entries = 0_usize;
    let mut metadata_string_bytes = 0_usize;
    for value in rows.iter() {
        let dictionary = value
            .downcast::<PyDict>()
            .map_err(|_| PyValueError::new_err("each training row must be a dictionary"))?;
        for key in [
            "logical_id",
            "group",
            "label",
            "split",
            "element",
            "byte_order",
        ] {
            metadata_string_bytes = metadata_string_bytes
                .checked_add(required_string_length(dictionary, key)?)
                .ok_or_else(|| PyValueError::new_err("training metadata size overflow"))?;
        }
        let shape_value = required_item(dictionary, "shape")?;
        let shape = shape_value
            .downcast::<PyList>()
            .map_err(|_| PyValueError::new_err("shape must be a list"))?;
        shape_entries = shape_entries
            .checked_add(shape.len())
            .ok_or_else(|| PyValueError::new_err("training shape metadata count overflow"))?;
    }
    for value in label_payloads.iter() {
        let dictionary = value.downcast::<PyDict>().map_err(|_| {
            PyValueError::new_err("each training label association must be a dictionary")
        })?;
        for key in ["logical_id", "concept", "presence"] {
            metadata_string_bytes = metadata_string_bytes
                .checked_add(required_string_length(dictionary, key)?)
                .ok_or_else(|| PyValueError::new_err("training metadata size overflow"))?;
        }
        if let Some(shape) = dictionary.get_item("shape")? {
            let shape = shape
                .downcast::<PyList>()
                .map_err(|_| PyValueError::new_err("shape must be a list"))?;
            shape_entries = shape_entries
                .checked_add(shape.len())
                .ok_or_else(|| PyValueError::new_err("training shape metadata count overflow"))?;
        }
        for key in ["element", "byte_order"] {
            if dictionary.contains(key)? {
                metadata_string_bytes = metadata_string_bytes
                    .checked_add(required_string_length(dictionary, key)?)
                    .ok_or_else(|| PyValueError::new_err("training metadata size overflow"))?;
            }
        }
    }
    let catalog_bound = bounds.max_catalog_bytes as usize;
    if metadata_string_bytes > catalog_bound
        || shape_entries > catalog_bound.saturating_div(core::mem::size_of::<u64>())
    {
        return Err(PyValueError::new_err(
            "training snapshot metadata exceeds the ABIR BCS2 catalog resource bound",
        ));
    }
    Ok(())
}

fn required_string_length(dictionary: &Bound<'_, PyDict>, key: &str) -> PyResult<usize> {
    let value = required_item(dictionary, key)?;
    let value = value
        .downcast::<PyString>()
        .map_err(|_| PyValueError::new_err(format!("field {key:?} must be a string")))?;
    Ok(value.to_str()?.len())
}

fn parse_bound_row<'py>(dictionary: &Bound<'py, PyDict>) -> PyResult<BoundTrainingRow<'py>> {
    let payload = required_payload(dictionary)?;
    let element = super::parse_element(&required_string(dictionary, "element")?)?;
    let logical_bytes = u64::try_from(payload.as_bytes().len())
        .map_err(|_| PyValueError::new_err("training row payload is too large"))?;
    let metadata = TrainingRow {
        byte_order: super::parse_byte_order(&required_string(dictionary, "byte_order")?)?,
        group: ContentKey::new(super::parse_content_id(&required_string(
            dictionary, "group",
        )?)?),
        label: ContentKey::new(super::parse_content_id(&required_string(
            dictionary, "label",
        )?)?),
        logical_bytes,
        logical_id: ContentKey::new(super::parse_content_id(&required_string(
            dictionary,
            "logical_id",
        )?)?),
        payload: ContentKey::new(payload_content_id(element, payload.as_bytes())),
        element,
        shape: required_shape(dictionary)?,
        split: ContentKey::new(super::parse_content_id(&required_string(
            dictionary, "split",
        )?)?),
    };
    Ok(BoundTrainingRow { metadata, payload })
}

fn parse_bound_label<'py>(dictionary: &Bound<'py, PyDict>) -> PyResult<BoundLabelAssociation<'py>> {
    let presence = parse_presence(&required_string(dictionary, "presence")?)?;
    let logical_id = ContentKey::new(super::parse_content_id(&required_string(
        dictionary,
        "logical_id",
    )?)?);
    let concept = required_string(dictionary, "concept")?;
    if presence != Presence::Present {
        for key in ["payload", "element", "byte_order", "shape"] {
            if dictionary.contains(key)? {
                return Err(PyValueError::new_err(format!(
                    "label presence {} forbids a payload descriptor",
                    presence_name(presence)
                )));
            }
        }
        return Ok(BoundLabelAssociation {
            metadata: TrainingLabelPayloadAssociation {
                concept,
                logical_id,
                payload: None,
                presence,
            },
            payload: None,
        });
    }

    let payload = required_payload(dictionary)?;
    let element = super::parse_element(&required_string(dictionary, "element")?)?;
    let logical_bytes = u64::try_from(payload.as_bytes().len())
        .map_err(|_| PyValueError::new_err("training label payload is too large"))?;
    Ok(BoundLabelAssociation {
        metadata: TrainingLabelPayloadAssociation {
            concept,
            logical_id,
            payload: Some(TrainingAssociatedPayload {
                byte_order: super::parse_byte_order(&required_string(dictionary, "byte_order")?)?,
                element,
                logical_bytes,
                payload: ContentKey::new(payload_content_id(element, payload.as_bytes())),
                shape: required_shape(dictionary)?,
            }),
            presence,
        },
        payload: Some(payload),
    })
}

fn parse_training_spec(dictionary: &Bound<'_, PyDict>) -> PyResult<TrainingSpec> {
    require_exact_keys(
        dictionary,
        &[
            "augmentation",
            "authorized_purpose",
            "cohort",
            "feature",
            "fitted_state",
            "grouping",
            "label",
            "policy",
            "preprocessing",
            "sampler",
            "seed",
            "split",
            "view",
            "window",
            "allowed_adaptive_knobs",
        ],
    )?;
    let key = |name: &str| -> PyResult<ContentKey> {
        Ok(ContentKey::new(super::parse_content_id(&required_string(
            dictionary, name,
        )?)?))
    };
    let allowed_adaptive_knobs = required_item(dictionary, "allowed_adaptive_knobs")?
        .downcast::<PyList>()
        .map_err(|_| PyValueError::new_err("allowed_adaptive_knobs must be a list"))?
        .extract()?;
    Ok(TrainingSpec {
        augmentation: key("augmentation")?,
        authorized_purpose: required_string(dictionary, "authorized_purpose")?,
        cohort: key("cohort")?,
        feature: key("feature")?,
        fitted_state: key("fitted_state")?,
        grouping: key("grouping")?,
        label: key("label")?,
        policy: key("policy")?,
        preprocessing: key("preprocessing")?,
        sampler: key("sampler")?,
        seed: required_item(dictionary, "seed")?.extract()?,
        split: key("split")?,
        view: key("view")?,
        window: key("window")?,
        allowed_adaptive_knobs,
    })
}

fn preflight_acceptance_count(count: usize, kind: &str) -> PyResult<()> {
    if count > ResourceBounds::default().max_index_entries as usize {
        return Err(PyValueError::new_err(format!(
            "training acceptance {kind} exceeds the ABIR BCS2 index resource bound"
        )));
    }
    Ok(())
}

fn parse_decision_records(records: &Bound<'_, PyList>) -> PyResult<Vec<DecisionRecord>> {
    records
        .iter()
        .map(|value| {
            let record = value
                .downcast::<PyDict>()
                .map_err(|_| PyValueError::new_err("each decision record must be a dictionary"))?;
            require_exact_keys(
                record,
                &[
                    "activation_barrier",
                    "decision",
                    "durable_before_activation",
                    "knob",
                    "rank",
                    "sequence",
                ],
            )?;
            Ok(DecisionRecord {
                activation_barrier: required_item(record, "activation_barrier")?.extract()?,
                decision: ContentKey::new(super::parse_content_id(&required_string(
                    record, "decision",
                )?)?),
                durable_before_activation: required_item(record, "durable_before_activation")?
                    .extract()?,
                knob: required_string(record, "knob")?,
                rank: required_item(record, "rank")?.extract()?,
                sequence: required_item(record, "sequence")?.extract()?,
            })
        })
        .collect()
}

fn parse_micro_snapshots(events: &Bound<'_, PyList>) -> PyResult<Vec<MicroSnapshot>> {
    events
        .iter()
        .map(|value| {
            let event = value.downcast::<PyDict>().map_err(|_| {
                PyValueError::new_err("each micro-snapshot event must be a dictionary")
            })?;
            require_exact_keys(
                event,
                &[
                    "correction",
                    "generation",
                    "logical_id",
                    "sequence",
                    "snapshot_id",
                    "watermark",
                ],
            )?;
            let correction = match event.get_item("correction")? {
                None => None,
                Some(value) if value.is_none() => None,
                Some(value) => {
                    let correction = value.downcast::<PyDict>().map_err(|_| {
                        PyValueError::new_err("correction must be a dictionary or None")
                    })?;
                    require_exact_keys(correction, &["prior_generation", "prior_snapshot_id"])?;
                    Some(SubscriptionCorrection {
                        prior_generation: required_item(correction, "prior_generation")?
                            .extract()?,
                        prior_snapshot_id: ContentKey::new(super::parse_content_id(
                            &required_string(correction, "prior_snapshot_id")?,
                        )?),
                    })
                }
            };
            Ok(MicroSnapshot {
                correction,
                generation: required_item(event, "generation")?.extract()?,
                logical_id: ContentKey::new(super::parse_content_id(&required_string(
                    event,
                    "logical_id",
                )?)?),
                sequence: required_item(event, "sequence")?.extract()?,
                snapshot_id: ContentKey::new(super::parse_content_id(&required_string(
                    event,
                    "snapshot_id",
                )?)?),
                watermark: required_item(event, "watermark")?.extract()?,
            })
        })
        .collect()
}

#[pyfunction]
#[pyo3(signature = (*, spec, records))]
pub(crate) fn seal_training_decision_log<'py>(
    py: Python<'py>,
    spec: &Bound<'py, PyDict>,
    records: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyDict>> {
    preflight_acceptance_count(records.len(), "decision record count")?;
    let spec = parse_training_spec(spec)?;
    let log = DecisionLog::seal(&spec, parse_decision_records(records)?).map_err(training_error)?;
    let result = PyDict::new_bound(py);
    result.set_item(
        "spec_id",
        spec.content_id().map_err(training_error)?.to_string(),
    )?;
    result.set_item(
        "decision_log_id",
        log.content_id().map_err(training_error)?.to_string(),
    )?;
    result.set_item(
        "decision_log",
        PyBytes::new_bound(py, &log.canonical_json().map_err(training_error)?),
    )?;
    Ok(result)
}

#[pyfunction]
#[pyo3(signature = (*, snapshot, spec, decision_log, records))]
pub(crate) fn verify_training_decision_replay<'py>(
    py: Python<'py>,
    snapshot: &Bound<'py, PyBytes>,
    spec: &Bound<'py, PyDict>,
    decision_log: &Bound<'py, PyBytes>,
    records: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyDict>> {
    preflight_acceptance_count(records.len(), "decision replay record count")?;
    let spec = parse_training_spec(spec)?;
    let log = DecisionLog::from_canonical_json(decision_log.as_bytes()).map_err(training_error)?;
    let records = parse_decision_records(records)?;
    let store = TrainingWindowStore::open(snapshot.as_bytes(), ResourceBounds::default())
        .map_err(training_error)?;
    let receipt = store
        .verify_decision_replay(&spec, &log, &records)
        .map_err(training_error)?;
    let result = PyDict::new_bound(py);
    result.set_item("decision_log_id", receipt.decision_log_id().to_string())?;
    result.set_item("spec_id", receipt.spec_id().to_string())?;
    result.set_item("record_count", receipt.record_count())?;
    result.set_item(
        "receipt_id",
        receipt.content_id().map_err(training_error)?.to_string(),
    )?;
    result.set_item(
        "receipt",
        PyBytes::new_bound(py, &receipt.canonical_json().map_err(training_error)?),
    )?;
    Ok(result)
}

#[pyfunction]
pub(crate) fn verify_training_source_equivalence<'py>(
    py: Python<'py>,
    first: &Bound<'py, PyBytes>,
    second: &Bound<'py, PyBytes>,
) -> PyResult<Bound<'py, PyDict>> {
    let first = TrainingWindowStore::open(first.as_bytes(), ResourceBounds::default())
        .map_err(training_error)?;
    let second = TrainingWindowStore::open(second.as_bytes(), ResourceBounds::default())
        .map_err(training_error)?;
    let receipt = SourceEquivalenceReceipt::verify(&first, &second).map_err(training_error)?;
    let result = PyDict::new_bound(py);
    result.set_item("first_snapshot_id", receipt.first_snapshot_id().to_string())?;
    result.set_item(
        "second_snapshot_id",
        receipt.second_snapshot_id().to_string(),
    )?;
    result.set_item(
        "logical_windows_id",
        receipt.logical_windows_id().to_string(),
    )?;
    result.set_item(
        "first_dataset_roots_id",
        receipt.first_dataset_roots_id().to_string(),
    )?;
    result.set_item(
        "second_dataset_roots_id",
        receipt.second_dataset_roots_id().to_string(),
    )?;
    result.set_item("row_count", receipt.row_count())?;
    result.set_item(
        "first_dataset_root_count",
        receipt.first_dataset_root_count(),
    )?;
    result.set_item(
        "second_dataset_root_count",
        receipt.second_dataset_root_count(),
    )?;
    result.set_item(
        "receipt_id",
        receipt.content_id().map_err(training_error)?.to_string(),
    )?;
    result.set_item(
        "receipt",
        PyBytes::new_bound(py, &receipt.canonical_json().map_err(training_error)?),
    )?;
    Ok(result)
}

#[pyfunction]
#[pyo3(signature = (*, subscription_id, events, spec, snapshots, decision_logs, decision_replays))]
pub(crate) fn seal_training_continual_promotion<'py>(
    py: Python<'py>,
    subscription_id: &str,
    events: &Bound<'py, PyList>,
    spec: &Bound<'py, PyDict>,
    snapshots: &Bound<'py, PyList>,
    decision_logs: &Bound<'py, PyList>,
    decision_replays: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyDict>> {
    let bounds = ResourceBounds::default();
    for (count, kind) in [
        (events.len(), "micro-snapshot count"),
        (snapshots.len(), "snapshot count"),
        (decision_logs.len(), "decision log count"),
        (decision_replays.len(), "decision replay count"),
    ] {
        if count > bounds.max_generations as usize {
            return Err(PyValueError::new_err(format!(
                "training acceptance {kind} exceeds the continual generation bound"
            )));
        }
    }
    let spec = parse_training_spec(spec)?;
    let mut subscription =
        DatasetSubscription::new(ContentKey::new(super::parse_content_id(subscription_id)?));
    for event in parse_micro_snapshots(events)? {
        subscription.append(event).map_err(training_error)?;
    }
    let closed = subscription.close().map_err(training_error)?;
    let mut snapshot_values = Vec::with_capacity(snapshots.len());
    let mut total_rows = 0_usize;
    for value in snapshots.iter() {
        let bytes = value.downcast::<PyBytes>().map_err(|_| {
            PyValueError::new_err("each continual snapshot must be immutable bytes")
        })?;
        let store = TrainingWindowStore::open(bytes.as_bytes(), ResourceBounds::default())
            .map_err(training_error)?;
        total_rows = total_rows
            .checked_add(store.rows().len())
            .ok_or_else(|| PyValueError::new_err("continual row count overflow"))?;
        preflight_acceptance_count(total_rows, "aggregate snapshot row count")?;
        snapshot_values.push(store.verified_snapshot());
    }
    let mut decision_log_values = Vec::with_capacity(decision_logs.len());
    let mut total_records = 0_usize;
    for value in decision_logs.iter() {
        let bytes = value.downcast::<PyBytes>().map_err(|_| {
            PyValueError::new_err("each continual decision log must be immutable bytes")
        })?;
        let log = DecisionLog::from_canonical_json(bytes.as_bytes()).map_err(training_error)?;
        total_records = total_records
            .checked_add(log.records().len())
            .ok_or_else(|| PyValueError::new_err("continual decision record count overflow"))?;
        preflight_acceptance_count(total_records, "aggregate decision record count")?;
        decision_log_values.push(log);
    }
    let replay_values = decision_replays
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let records = value.downcast::<PyList>().map_err(|_| {
                PyValueError::new_err("each continual decision replay must be a list")
            })?;
            let log = decision_log_values.get(index).ok_or_else(|| {
                PyValueError::new_err("decision replay has no corresponding decision log")
            })?;
            if records.len() != log.records().len() {
                return Err(PyValueError::new_err(
                    "decision replay record count does not match its decision log",
                ));
            }
            preflight_acceptance_count(records.len(), "nested decision replay record count")?;
            DecisionReplayReceipt::verify(&spec, log, &parse_decision_records(records)?)
                .map_err(training_error)
        })
        .collect::<PyResult<Vec<_>>>()?;
    let promotion = ContinualPromotion::seal(
        &closed,
        &spec,
        &snapshot_values,
        &decision_log_values,
        &replay_values,
    )
    .map_err(training_error)?;
    let result = PyDict::new_bound(py);
    result.set_item(
        "closed_subscription_id",
        promotion.closed_subscription_id().to_string(),
    )?;
    result.set_item(
        "closed_subscription",
        PyBytes::new_bound(py, &closed.canonical_json().map_err(training_error)?),
    )?;
    result.set_item("entry_count", promotion.entry_count())?;
    result.set_item(
        "promotion_id",
        promotion.content_id().map_err(training_error)?.to_string(),
    )?;
    result.set_item(
        "promotion",
        PyBytes::new_bound(py, &promotion.canonical_json().map_err(training_error)?),
    )?;
    Ok(result)
}

/// Seal exact primary rows and typed label associations into a validated BCS2
/// Training Window Store artifact. The caller supplies semantic identities;
/// ABIR exclusively owns payload identities, canonical catalog identity, and
/// physical frame closure.
#[pyfunction]
#[pyo3(signature = (*, dataset_roots, spec_id, profile, rows, label_payloads, decision_log_id))]
pub(crate) fn seal_training_snapshot<'py>(
    py: Python<'py>,
    dataset_roots: &Bound<'py, PyList>,
    spec_id: &str,
    profile: &str,
    rows: &Bound<'py, PyList>,
    label_payloads: &Bound<'py, PyList>,
    decision_log_id: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let bounds = ResourceBounds::default();
    if dataset_roots.len() > bounds.max_index_entries as usize {
        return Err(PyValueError::new_err(
            "training dataset roots exceed the ABIR BCS2 index resource bound",
        ));
    }
    preflight_metadata(dataset_roots.len(), rows, label_payloads, bounds)?;
    let dataset_roots = dataset_roots
        .iter()
        .map(|value| {
            let value = value
                .downcast::<PyString>()
                .map_err(|_| PyValueError::new_err("dataset roots must be strings"))?;
            super::parse_content_id(value.to_str()?).map(ContentKey::new)
        })
        .collect::<PyResult<Vec<_>>>()?;
    let bound_rows = rows
        .iter()
        .map(|value| {
            let dictionary = value
                .downcast::<PyDict>()
                .map_err(|_| PyValueError::new_err("each training row must be a dictionary"))?;
            parse_bound_row(dictionary)
        })
        .collect::<PyResult<Vec<_>>>()?;
    let bound_labels = label_payloads
        .iter()
        .map(|value| {
            let dictionary = value.downcast::<PyDict>().map_err(|_| {
                PyValueError::new_err("each training label association must be a dictionary")
            })?;
            parse_bound_label(dictionary)
        })
        .collect::<PyResult<Vec<_>>>()?;
    let index_entries = bound_rows
        .len()
        .checked_add(
            bound_labels
                .iter()
                .filter(|association| association.payload.is_some())
                .count(),
        )
        .ok_or_else(|| PyValueError::new_err("training snapshot index count overflow"))?;
    if index_entries > bounds.max_index_entries as usize {
        return Err(PyValueError::new_err(
            "training snapshot exceeds the ABIR BCS2 index resource bound",
        ));
    }

    let snapshot = TrainingSnapshot::seal_with_label_payloads(
        dataset_roots,
        ContentKey::new(super::parse_content_id(spec_id)?),
        parse_profile(profile)?,
        bound_rows.iter().map(|row| row.metadata.clone()).collect(),
        bound_labels
            .iter()
            .map(|association| association.metadata.clone())
            .collect(),
        ContentKey::new(super::parse_content_id(decision_log_id)?),
    )
    .map_err(training_error)?;

    let mut payloads = BTreeMap::<ContentId, (ElementType, &[u8])>::new();
    for row in &bound_rows {
        payloads.insert(
            row.metadata.payload.content_id(),
            (row.metadata.element, row.payload.as_bytes()),
        );
    }
    for association in &bound_labels {
        if let (Some(descriptor), Some(payload)) =
            (&association.metadata.payload, &association.payload)
        {
            payloads.insert(
                descriptor.payload.content_id(),
                (descriptor.element, payload.as_bytes()),
            );
        }
    }
    let frames = payloads
        .values()
        .map(|(element, payload)| SemanticPayloadFrame::new(*element, payload))
        .collect::<Vec<_>>();
    let artifact = encode_snapshot(&snapshot, &frames, bounds).map_err(training_error)?;
    let result = PyDict::new_bound(py);
    result.set_item(
        "snapshot_id",
        snapshot.content_id().map_err(training_error)?.to_string(),
    )?;
    result.set_item("artifact", PyBytes::new_bound(py, &artifact))?;
    Ok(result)
}

#[cfg(feature = "test-fixtures")]
fn key(seed: u8) -> ContentKey {
    ContentKey::new(ContentId::from_bytes([seed; 32]))
}

/// Deterministic private fixture for cross-language ownership and corruption tests.
#[pyfunction(name = "_training_fixture_bytes", signature = (payload_bytes=8, label_presence=None))]
#[cfg(feature = "test-fixtures")]
pub(crate) fn training_fixture_bytes<'py>(
    py: Python<'py>,
    payload_bytes: usize,
    label_presence: Option<&str>,
) -> PyResult<Bound<'py, PyBytes>> {
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
    let mask = [0_u8, 1];
    let label_payloads = match label_presence {
        None => Vec::new(),
        Some(presence) => {
            let presence = match presence {
                "present" => Presence::Present,
                "absent-at-source" => Presence::AbsentAtSource,
                "unknown-at-source" => Presence::UnknownAtSource,
                "withheld" => Presence::Withheld,
                "redacted" => Presence::Redacted,
                "not-applicable" => Presence::NotApplicable,
                _ => return Err(PyValueError::new_err("unsupported label presence fixture")),
            };
            vec![TrainingLabelPayloadAssociation {
                concept: "org.quitetall.lamquant.label.seizure-mask-v1".to_owned(),
                logical_id: key(7),
                payload: (presence == Presence::Present).then(|| TrainingAssociatedPayload {
                    byte_order: ByteOrder::NotApplicable,
                    element: ElementType::U8,
                    logical_bytes: mask.len() as u64,
                    payload: ContentKey::new(payload_content_id(ElementType::U8, &mask)),
                    shape: vec![mask.len() as u64],
                }),
                presence,
            }]
        }
    };
    let snapshot = TrainingSnapshot::seal_with_label_payloads(
        vec![key(1)],
        key(2),
        TrainingProfile::Balanced,
        vec![row],
        label_payloads,
        key(3),
    )
    .map_err(training_error)?;
    let mask_frame = (label_presence == Some("present"))
        .then(|| SemanticPayloadFrame::new(ElementType::U8, &mask));
    let mut frames = vec![SemanticPayloadFrame::new(ElementType::I16, &payload)];
    frames.extend(mask_frame);
    let encoded =
        encode_snapshot(&snapshot, &frames, ResourceBounds::default()).map_err(training_error)?;
    Ok(PyBytes::new_bound(py, &encoded))
}
