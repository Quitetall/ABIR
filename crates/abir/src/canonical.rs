use crate::{
    AbirDataset, Atom, ByteOrder, ElementType, ExactNumber, FidelityKind, Layout, ObjectKind,
    Presence, Rational, SemanticRef, TimeAxis,
};
use alloc::string::ToString;
use alloc::vec::Vec;
use serde_json::{json, Value};

const LOGICAL_HASH_DOMAIN: &[u8] = b"org.quitetall.abir.semantic-v1\0";

/// RFC 8785-compatible JSON for ABIR's restricted JSON domain.
///
/// ABIR emits no floating-point JSON numbers. Exact values are tagged strings,
/// and `serde_json::Map` supplies lexicographic key order without the
/// `preserve_order` feature.
pub fn canonical_debug_json(dataset: &AbirDataset) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&dataset_value(dataset, Projection::Debug))
}

/// Domain-separated logical identity, excluding physical representation and
/// observed execution.
pub fn logical_content_id(dataset: &AbirDataset) -> Result<crate::ContentId, serde_json::Error> {
    let bytes = serde_json::to_vec(&dataset_value(dataset, Projection::Logical))?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(LOGICAL_HASH_DOMAIN);
    hasher.update(&bytes);
    Ok(crate::ContentId::from_bytes(*hasher.finalize().as_bytes()))
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Projection {
    Debug,
    Logical,
}

fn dataset_value(dataset: &AbirDataset, projection: Projection) -> Value {
    let mut recordings: Vec<_> = dataset.recordings().iter().collect();
    recordings.sort_by_key(|value| value.id());
    let mut streams: Vec<_> = dataset.streams().iter().collect();
    streams.sort_by_key(|value| value.id());
    let mut atoms: Vec<_> = dataset.atoms().iter().collect();
    atoms.sort_by_key(|value| value.id());
    let mut clocks: Vec<_> = dataset.clocks().iter().collect();
    clocks.sort_by_key(|value| value.id());
    let mut frames: Vec<_> = dataset.coordinate_frames().iter().collect();
    frames.sort_by_key(|value| value.id());
    let mut bases: Vec<_> = dataset.channel_bases().iter().collect();
    bases.sort_by_key(|value| value.id());
    let mut policies: Vec<_> = dataset.policies().iter().collect();
    policies.sort_by_key(|value| value.id());
    let mut proofs: Vec<_> = dataset.proofs().iter().collect();
    proofs.sort_by_key(|value| value.id());
    let mut derivations: Vec<_> = dataset.derivations().iter().collect();
    derivations.sort_by_key(|value| value.id());
    let mut fidelity: Vec<_> = dataset.fidelity().iter().collect();
    fidelity.sort_by_key(|value| {
        (
            value.subject(),
            match value.kind() {
                FidelityKind::Exact => 0_u8,
                FidelityKind::Bounded => 1,
                FidelityKind::Transformed => 2,
            },
            value.metric().cloned(),
            value.bound(),
        )
    });
    let mut source_capsules: Vec<_> = dataset.source_capsules().iter().collect();
    source_capsules.sort_by(|a, b| {
        (
            a.source().namespace(),
            a.source().value(),
            a.content_id(),
            a.media_type(),
        )
            .cmp(&(
                b.source().namespace(),
                b.source().value(),
                b.content_id(),
                b.media_type(),
            ))
    });

    let mut root = serde_json::Map::new();
    root.insert("semantic_version".into(), Value::String("1".into()));
    root.insert("dataset_id".into(), Value::String(dataset.id().to_string()));
    root.insert(
        "recordings".into(),
        Value::Array(
            recordings
                .into_iter()
                .map(|recording| {
                    let mut stream_ids: Vec<_> = recording.streams().iter().collect();
                    stream_ids.sort();
                    let mut source_keys: Vec<_> = recording.source_keys().iter().collect();
                    source_keys.sort_by(|a, b| {
                        (a.namespace(), a.value()).cmp(&(b.namespace(), b.value()))
                    });
                    json!({
                        "id": recording.id().to_string(),
                        "source_keys": source_keys.into_iter().map(|key| json!({
                            "namespace": key.namespace(), "value": key.value()
                        })).collect::<Vec<_>>(),
                        "streams": stream_ids.into_iter().map(ToString::to_string).collect::<Vec<_>>()
                    })
                })
                .collect(),
        ),
    );
    root.insert(
        "streams".into(),
        Value::Array(
            streams
                .into_iter()
                .map(|stream| {
                    json!({
                        "id": stream.id().to_string(),
                        "recording_id": stream.recording_id().to_string(),
                        "modality": stream.modality().as_str(),
                        "atoms": stream.atoms().iter().map(ToString::to_string).collect::<Vec<_>>(),
                        "clock_id": stream.clock_id().map(|id| id.to_string()),
                        "channel_basis_id": stream.channel_basis_id().map(|id| id.to_string()),
                        "policy_id": stream.policy_id().map(|id| id.to_string())
                    })
                })
                .collect(),
        ),
    );
    root.insert(
        "atoms".into(),
        Value::Array(
            atoms
                .into_iter()
                .map(|atom| atom_value(atom, projection))
                .collect(),
        ),
    );
    root.insert(
        "clocks".into(),
        Value::Array(
            clocks
                .into_iter()
                .map(|clock| {
                    json!({
                        "id": clock.id().to_string(), "kind": clock.kind().as_str(),
                        "parent_id": clock.parent_id().map(|id| id.to_string()),
                        "offset": exact_value(ExactNumber::Rational(clock.offset())),
                        "rate": exact_value(ExactNumber::Rational(clock.rate())),
                        "uncertainty": exact_value(ExactNumber::Rational(clock.uncertainty()))
                    })
                })
                .collect(),
        ),
    );
    root.insert(
        "coordinate_frames".into(),
        Value::Array(
            frames
                .into_iter()
                .map(|frame| {
                    json!({
                        "id": frame.id().to_string(), "kind": frame.kind().as_str(),
                        "parent_id": frame.parent_id().map(|id| id.to_string()),
                        "transform": frame.transform().map(|values| values.iter().copied().map(exact_value).collect::<Vec<_>>()),
                        "uncertainty": exact_value(ExactNumber::Rational(frame.uncertainty()))
                    })
                })
                .collect(),
        ),
    );
    root.insert(
        "channel_bases".into(),
        Value::Array(
            bases
                .into_iter()
                .map(|basis| {
                    json!({
                        "id": basis.id().to_string(),
                        "reference": reference_name(basis.reference()),
                        "channels": basis.channels().iter().map(channel_value).collect::<Vec<_>>()
                    })
                })
                .collect(),
        ),
    );
    root.insert(
        "policies".into(),
        Value::Array(
            policies
                .into_iter()
                .map(|policy| {
                    json!({
                        "id": policy.id().to_string(),
                        "parent_id": policy.parent_id().map(|id| id.to_string()),
                        "restrictions": policy.restrictions().iter().map(|value| value.as_str()).collect::<Vec<_>>()
                    })
                })
                .collect(),
        ),
    );
    root.insert(
        "proofs".into(),
        Value::Array(
            proofs
                .into_iter()
                .map(|proof| {
                    json!({
                        "id": proof.id().to_string(), "kind": proof.kind().as_str(),
                        "subject": semantic_ref_value(proof.subject()),
                        "payload": proof.payload().to_string()
                    })
                })
                .collect(),
        ),
    );
    root.insert(
        "derivations".into(),
        Value::Array(
            derivations
                .into_iter()
                .map(|derivation| {
                    json!({
                        "id": derivation.id().to_string(),
                        "operation": derivation.operation().as_str(),
                        "inputs": derivation.inputs().iter().copied().map(semantic_ref_value).collect::<Vec<_>>(),
                        "outputs": derivation.outputs().iter().copied().map(semantic_ref_value).collect::<Vec<_>>()
                    })
                })
                .collect(),
        ),
    );
    root.insert(
        "fidelity".into(),
        Value::Array(
            fidelity
                .into_iter()
                .map(|statement| {
                    json!({
                        "subject": semantic_ref_value(statement.subject()),
                        "kind": match statement.kind() { FidelityKind::Exact => "exact", FidelityKind::Bounded => "bounded", FidelityKind::Transformed => "transformed" },
                        "metric": statement.metric().map(|value| value.as_str()),
                        "bound": statement.bound().map(exact_value)
                    })
                })
                .collect(),
        ),
    );
    root.insert(
        "source_capsules".into(),
        Value::Array(
            source_capsules
                .into_iter()
                .map(|capsule| {
                    json!({
                        "source": { "namespace": capsule.source().namespace(), "value": capsule.source().value() },
                        "content_id": capsule.content_id().to_string(),
                        "media_type": capsule.media_type()
                    })
                })
                .collect(),
        ),
    );
    if projection == Projection::Debug {
        root.insert(
            "observed_execution".into(),
            Value::Array(
                dataset
                    .observed_execution()
                    .iter()
                    .map(|record| {
                        json!({
                            "operation": record.operation().as_str(),
                            "implementation": record.implementation(),
                            "hardware": record.hardware()
                        })
                    })
                    .collect(),
            ),
        );
    }
    Value::Object(root)
}

fn atom_value(atom: &Atom, projection: Projection) -> Value {
    let (kind, time, calibration) = match atom {
        Atom::SignalBlock(block) => (
            "signal-block",
            Some(time_value(block.time_axis())),
            block.calibration().map(|value| {
                json!({
                    "scale": exact_value(ExactNumber::Rational(value.scale())),
                    "offset": exact_value(ExactNumber::Rational(value.offset())),
                    "unit": value.unit().as_str()
                })
            }),
        ),
        Atom::TemporalTable(_) => ("temporal-table", None, None),
        Atom::Table(_) => ("table", None, None),
        Atom::Tensor(_) => ("tensor", None, None),
        Atom::EncodedBlock(_) => ("encoded-block", None, None),
        Atom::BlobRef(_) => ("blob-ref", None, None),
    };
    json!({
        "id": atom.id().to_string(), "kind": kind, "presence": presence_name(atom.presence()),
        "payload": atom.payload().map(|value| payload_value(value, projection)),
        "time_axis": time, "calibration": calibration
    })
}

fn channel_value(channel: &crate::ChannelSpec) -> Value {
    let mut keys: Vec<_> = channel.source_keys().iter().collect();
    keys.sort_by(|a, b| (a.namespace(), a.value()).cmp(&(b.namespace(), b.value())));
    json!({
        "concept": channel.concept().as_str(),
        "coordinate_frame_id": channel.coordinate_frame_id().map(|id| id.to_string()),
        "source_keys": keys.into_iter().map(|key| json!({
            "namespace": key.namespace(), "value": key.value()
        })).collect::<Vec<_>>()
    })
}

fn payload_value(payload: &crate::PayloadDescriptor, projection: Projection) -> Value {
    if projection == Projection::Logical {
        return json!({
            "content_id": payload.content_id().to_string(),
            "element": element_name(payload.element()),
            "shape": payload.shape()
        });
    }
    json!({
        "content_id": payload.content_id().to_string(), "logical_bytes": payload.logical_bytes(),
        "element": element_name(payload.element()), "byte_order": byte_order_name(payload.byte_order()),
        "shape": payload.shape(), "layout": layout_value(payload.layout()),
        "encoding": payload.encoding().map(|value| value.as_str()), "media_type": payload.media_type()
    })
}

fn time_value(axis: &TimeAxis) -> Value {
    match axis {
        TimeAxis::Regular(segment) => json!({ "regular": segment_value(*segment) }),
        TimeAxis::Piecewise(segments) => {
            json!({ "piecewise": segments.iter().copied().map(segment_value).collect::<Vec<_>>() })
        }
        TimeAxis::Explicit { timestamps, count } => {
            json!({ "explicit": { "timestamps": timestamps.to_string(), "count": count } })
        }
    }
}

fn segment_value(segment: crate::TimeSegment) -> Value {
    json!({
        "start": exact_value(ExactNumber::Rational(segment.start())),
        "rate": exact_value(ExactNumber::Rational(segment.rate())), "samples": segment.samples()
    })
}

fn exact_value(value: ExactNumber) -> Value {
    match value {
        ExactNumber::Integer(value) => json!({ "$integer": value.to_string() }),
        ExactNumber::Rational(value) => rational_value(value),
    }
}

fn rational_value(value: Rational) -> Value {
    let (numerator, denominator) = value.parts();
    json!({ "$rational": [numerator.to_string(), denominator.to_string()] })
}

fn semantic_ref_value(reference: SemanticRef) -> Value {
    json!({ "kind": object_kind_name(reference.kind()), "id": hex(&reference.bytes()) })
}

fn layout_value(layout: &Layout) -> Value {
    match layout {
        Layout::DenseRowMajor => json!("dense-row-major"),
        Layout::DenseColumnMajor => json!("dense-column-major"),
        Layout::Ragged { rows } => json!({ "ragged": { "rows": rows } }),
        Layout::SparseCoo { nonzero } => json!({ "sparse-coo": { "nonzero": nonzero } }),
        Layout::SparseCsr { nonzero } => json!({ "sparse-csr": { "nonzero": nonzero } }),
        Layout::BlockFloatingPoint {
            block_len,
            mantissa_bits,
        } => {
            json!({ "bfp": { "block_len": block_len, "mantissa_bits": mantissa_bits } })
        }
    }
}

fn hex(bytes: &[u8]) -> alloc::string::String {
    use core::fmt::Write;
    let mut output = alloc::string::String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String");
    }
    output
}

fn presence_name(value: Presence) -> &'static str {
    match value {
        Presence::Present => "present",
        Presence::Missing => "missing",
        Presence::Unknown => "unknown",
        Presence::Redacted => "redacted",
        Presence::NotApplicable => "not-applicable",
    }
}
fn byte_order_name(value: ByteOrder) -> &'static str {
    match value {
        ByteOrder::Little => "little",
        ByteOrder::Big => "big",
        ByteOrder::NotApplicable => "not-applicable",
    }
}
fn element_name(value: ElementType) -> &'static str {
    match value {
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
fn reference_name(value: crate::ReferenceKind) -> &'static str {
    match value {
        crate::ReferenceKind::Absolute => "absolute",
        crate::ReferenceKind::Common => "common",
        crate::ReferenceKind::Differential => "differential",
        crate::ReferenceKind::Unknown => "unknown",
    }
}
fn object_kind_name(value: ObjectKind) -> &'static str {
    match value {
        ObjectKind::Dataset => "dataset",
        ObjectKind::Recording => "recording",
        ObjectKind::Stream => "stream",
        ObjectKind::Atom => "atom",
        ObjectKind::Clock => "clock",
        ObjectKind::CoordinateFrame => "coordinate-frame",
        ObjectKind::ChannelBasis => "channel-basis",
        ObjectKind::Policy => "policy",
        ObjectKind::Proof => "proof",
        ObjectKind::Derivation => "derivation",
    }
}
