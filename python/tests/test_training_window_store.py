import gc
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys

import abir
import numpy as np
import pytest
import jsonschema


def test_training_window_store_opens_validated_bundle_and_lends_rows():
    artifact = abir._training_fixture_bytes()
    store = abir.TrainingWindowStore.open_bytes(artifact)

    assert store.profile == "balanced"
    assert store.physical_artifact_sha256 == hashlib.sha256(artifact).hexdigest()
    assert len(store.snapshot_id) == 64
    assert store.row_count == 1
    assert len(store.row_ids) == 1

    row_id = store.row_ids[0]
    row = store.row_numpy(row_id)

    assert row.shape == (2, 2)
    assert row.dtype == np.dtype("<i2")
    assert row.tolist() == [[1, 2], [3, 4]]
    assert np.shares_memory(row.view(np.uint8), np.frombuffer(artifact, dtype=np.uint8))
    assert not row.flags.writeable

    del store
    del artifact
    gc.collect()

    assert row.tolist() == [[1, 2], [3, 4]]
    assert not row.flags.writeable


def test_training_window_store_exposes_snapshot_bound_semantics_without_source_format():
    artifact = abir._training_fixture_bytes()
    store = abir.TrainingWindowStore.open_bytes(artifact)

    assert store.dataset_roots == ("01" * 32,)
    assert store.spec_id == "02" * 32
    assert store.decision_log_id == "03" * 32
    assert store.decision_log_replay_state == "identity-bound"

    info = dict(store.row_info(store.row_ids[0]))
    payload = info.pop("payload")
    assert info == {
        "backing": "bytes-zero-copy",
        "byte_order": "little",
        "element": "i16",
        "group": "05" * 32,
        "label": "06" * 32,
        "logical_bytes": 8,
        "logical_id": "07" * 32,
        "materialized": False,
        "shape": [2, 2],
        "split": "08" * 32,
    }
    assert len(payload) == 64
    assert "source_format" not in info


def test_training_window_store_rejects_unbound_semantic_metadata():
    artifact = abir._training_fixture_bytes()
    malformed = bytearray(artifact)
    bound_spec = b'"spec_id":"' + (b"02" * 32) + b'"'
    offset = malformed.find(bound_spec)
    assert offset >= 0
    malformed[offset + len(b'"spec_id":"')] = ord("f")

    with pytest.raises(ValueError, match="CatalogDigestMismatch"):
        abir.TrainingWindowStore.open_bytes(bytes(malformed))


def test_training_window_store_rejects_corruption_and_unknown_rows():
    artifact = abir._training_fixture_bytes()
    store = abir.TrainingWindowStore.open_bytes(artifact)
    with pytest.raises(KeyError):
        store.row_numpy("00" * 32)

    malformed = bytearray(artifact)
    malformed[-1] ^= 1
    with pytest.raises(ValueError):
        abir.TrainingWindowStore.open_bytes(bytes(malformed))


def test_training_window_store_opens_path_without_materializing_artifact(tmp_path):
    path = tmp_path / "snapshot.bcs2"
    path.write_bytes(abir._training_fixture_bytes())

    store = abir.TrainingWindowStore.open_path(path)
    row_id = store.row_ids[0]
    info = store.row_info(row_id)

    assert store.backing == "path-private-validation"
    assert store.materializes_rows is True
    assert store.dataset_roots == ("01" * 32,)
    assert store.spec_id == "02" * 32
    assert store.decision_log_id == "03" * 32
    assert store.decision_log_replay_state == "identity-bound"
    assert info["materialized"] is True
    assert info["group"] == "05" * 32
    assert info["label"] == "06" * 32
    assert info["split"] == "08" * 32
    assert info["logical_bytes"] == 8

    first = store.row_numpy(row_id)
    second = store.row_numpy(row_id)
    assert first.tolist() == [[1, 2], [3, 4]]
    assert first.__array_interface__["data"][0] != second.__array_interface__["data"][0]


def test_path_row_outlives_store_and_unlinked_artifact(tmp_path):
    path = tmp_path / "snapshot.bcs2"
    path.write_bytes(abir._training_fixture_bytes())
    store = abir.TrainingWindowStore.open_path(path)
    row = store.row_numpy(store.row_ids[0])

    if os.name != "nt":
        path.unlink()
    del store
    gc.collect()

    assert row.tolist() == [[1, 2], [3, 4]]


def test_path_replacement_cannot_change_held_artifact_attestation(tmp_path):
    original = abir._training_fixture_bytes()
    path = tmp_path / "snapshot.bcs2"
    path.write_bytes(original)
    store = abir.TrainingWindowStore.open_path(path)
    original_row_id = store.row_ids[0]

    replacement = tmp_path / "replacement.bcs2"
    replacement.write_bytes(b"replacement-path-content")
    os.replace(replacement, path)

    assert store.physical_artifact_sha256 == hashlib.sha256(original).hexdigest()
    assert hashlib.sha256(path.read_bytes()).hexdigest() != store.physical_artifact_sha256
    assert store.row_numpy(original_row_id).tolist() == [[1, 2], [3, 4]]


def test_training_window_store_rejects_corrupt_path(tmp_path):
    malformed = bytearray(abir._training_fixture_bytes())
    malformed[-1] ^= 1
    path = tmp_path / "corrupt.bcs2"
    path.write_bytes(malformed)

    with pytest.raises(ValueError):
        abir.TrainingWindowStore.open_path(path)


def test_path_row_uses_private_validated_backing_after_source_inode_changes(tmp_path):
    artifact = bytearray(abir._training_fixture_bytes())
    path = tmp_path / "changed.bcs2"
    path.write_bytes(artifact)
    store = abir.TrainingWindowStore.open_path(path)

    payload_offset = artifact.find(bytes([1, 0, 2, 0, 3, 0, 4, 0]))
    assert payload_offset >= 0
    with path.open("r+b") as changed:
        changed.seek(payload_offset)
        changed.write(bytes([9, 0, 2, 0, 3, 0, 4, 0]))

    assert store.row_numpy(store.row_ids[0]).tolist() == [[1, 2], [3, 4]]

    with path.open("r+b") as truncated:
        truncated.truncate(payload_offset + 2)
    assert store.row_numpy(store.row_ids[0]).tolist() == [[1, 2], [3, 4]]


@pytest.mark.skipif(not Path("/proc/self/status").exists(), reason="Linux RSS evidence")
def test_path_open_does_not_allocate_a_second_artifact_copy(tmp_path):
    artifact = abir._training_fixture_bytes(32 * 1024 * 1024)
    path = tmp_path / "large-snapshot.bcs2"
    path.write_bytes(artifact)
    artifact_bytes = path.stat().st_size
    del artifact
    gc.collect()

    probe = """
import gc, json, pathlib, sys
import abir

def rss_bytes():
    for line in pathlib.Path('/proc/self/status').read_text().splitlines():
        if line.startswith('VmRSS:'):
            return int(line.split()[1]) * 1024
    raise RuntimeError('VmRSS is unavailable')

gc.collect()
before = rss_bytes()
store = abir.TrainingWindowStore.open_path(sys.argv[1])
after = rss_bytes()
print(json.dumps({'before': before, 'after': after, 'rows': store.row_count}))
"""
    completed = subprocess.run(
        [sys.executable, "-c", probe, os.fspath(path)],
        check=True,
        capture_output=True,
        text=True,
    )
    measurement = json.loads(completed.stdout)

    # Validation hashes the complete artifact, so its file-backed mmap can be
    # resident. A second full heap copy would push growth toward 2x.
    assert measurement["rows"] == 1
    assert measurement["after"] - measurement["before"] < artifact_bytes * 3 // 2


def test_typed_label_payload_preserves_present_and_unknown_semantics():
    concept = "org.quitetall.lamquant.label.seizure-mask-v1"
    present = abir.TrainingWindowStore.open_bytes(
        abir._training_fixture_bytes(label_presence="present")
    )
    row_id = present.row_ids[0]

    info = present.row_label_payload_info(row_id, concept)
    assert info["presence"] == "present"
    assert info["element"] == "u8"
    assert info["byte_order"] == "not-applicable"
    assert info["shape"] == [2]
    mask = present.row_label_payload_numpy(row_id, concept)
    assert mask.tolist() == [0, 1]
    assert not mask.flags.writeable

    unknown = abir.TrainingWindowStore.open_bytes(
        abir._training_fixture_bytes(label_presence="unknown-at-source")
    )
    unknown_row_id = unknown.row_ids[0]
    assert unknown.row_label_payload_info(unknown_row_id, concept) == {
        "concept": concept,
        "presence": "unknown-at-source",
    }
    with pytest.raises(ValueError, match="unknown-at-source"):
        unknown.row_label_payload_numpy(unknown_row_id, concept)


def test_public_training_sealer_round_trips_typed_labels_deterministically():
    concept = "org.quitetall.lamquant.label.seizure-mask-v1"
    row_ids = ["7" * 64, "8" * 64]
    rows = [
        {
            "logical_id": row_ids[1],
            "group": "5" * 64,
            "label": "6" * 64,
            "split": "9" * 64,
            "element": "f32",
            "byte_order": "little",
            "shape": [1, 2],
            "payload": struct.pack("<2f", 3.0, 4.0),
        },
        {
            "logical_id": row_ids[0],
            "group": "5" * 64,
            "label": "6" * 64,
            "split": "9" * 64,
            "element": "f32",
            "byte_order": "little",
            "shape": [1, 2],
            "payload": struct.pack("<2f", 1.0, 2.0),
        },
    ]
    labels = [
        {
            "logical_id": row_ids[1],
            "concept": concept,
            "presence": "unknown-at-source",
        },
        {
            "logical_id": row_ids[0],
            "concept": concept,
            "presence": "present",
            "element": "u8",
            "byte_order": "not-applicable",
            "shape": [2],
            "payload": bytes([0, 1]),
        },
    ]

    first = abir.seal_training_snapshot(
        dataset_roots=["2" * 64, "1" * 64],
        spec_id="3" * 64,
        profile="balanced",
        rows=rows,
        label_payloads=labels,
        decision_log_id="4" * 64,
    )
    second = abir.seal_training_snapshot(
        dataset_roots=["1" * 64, "2" * 64],
        spec_id="3" * 64,
        profile="balanced",
        rows=list(reversed(rows)),
        label_payloads=list(reversed(labels)),
        decision_log_id="4" * 64,
    )

    assert first["snapshot_id"] == second["snapshot_id"]
    assert first["artifact"] == second["artifact"]
    store = abir.TrainingWindowStore.open_bytes(first["artifact"])
    assert store.row_ids == sorted(row_ids)
    assert store.row_numpy(row_ids[0]).tolist() == [[1.0, 2.0]]
    assert store.row_label_payload_numpy(row_ids[0], concept).tolist() == [0, 1]
    assert store.row_label_payload_info(row_ids[1], concept) == {
        "concept": concept,
        "presence": "unknown-at-source",
    }


def test_public_training_sealer_rejects_payload_for_non_present_label():
    with pytest.raises(ValueError, match="forbids a payload"):
        abir.seal_training_snapshot(
            dataset_roots=["1" * 64],
            spec_id="2" * 64,
            profile="balanced",
            rows=[{
                "logical_id": "3" * 64,
                "group": "4" * 64,
                "label": "5" * 64,
                "split": "6" * 64,
                "element": "f32",
                "byte_order": "little",
                "shape": [1],
                "payload": struct.pack("<f", 1.0),
            }],
            label_payloads=[{
                "logical_id": "3" * 64,
                "concept": "org.quitetall.lamquant.label.seizure-mask-v1",
                "presence": "redacted",
                "element": "u8",
                "byte_order": "not-applicable",
                "shape": [1],
                "payload": bytes([1]),
            }],
            decision_log_id="7" * 64,
        )


@pytest.mark.parametrize(
    "presence",
    [
        "absent-at-source",
        "unknown-at-source",
        "withheld",
        "redacted",
        "not-applicable",
    ],
)
def test_public_training_sealer_preserves_every_unavailable_label_state(presence):
    logical_id = "3" * 64
    concept = "org.quitetall.lamquant.label.seizure-mask-v1"
    sealed = abir.seal_training_snapshot(
        dataset_roots=["1" * 64],
        spec_id="2" * 64,
        profile="balanced",
        rows=[{
            "logical_id": logical_id,
            "group": "4" * 64,
            "label": "5" * 64,
            "split": "6" * 64,
            "element": "f32",
            "byte_order": "little",
            "shape": [1],
            "payload": struct.pack("<f", 1.0),
        }],
        label_payloads=[{
            "logical_id": logical_id,
            "concept": concept,
            "presence": presence,
        }],
        decision_log_id="7" * 64,
    )
    store = abir.TrainingWindowStore.open_bytes(sealed["artifact"])
    assert store.row_label_payload_info(logical_id, concept) == {
        "concept": concept,
        "presence": presence,
    }


def test_public_training_sealer_fails_closed_on_extent_and_duplicate_translation():
    logical_id = "3" * 64
    row = {
        "logical_id": logical_id,
        "group": "4" * 64,
        "label": "5" * 64,
        "split": "6" * 64,
        "element": "f32",
        "byte_order": "little",
        "shape": [1],
        "payload": struct.pack("<f", 1.0),
    }
    common = {
        "dataset_roots": ["1" * 64],
        "spec_id": "2" * 64,
        "profile": "balanced",
        "decision_log_id": "7" * 64,
    }

    malformed = dict(row, shape=[2])
    with pytest.raises(ValueError, match="invalid logical extent"):
        abir.seal_training_snapshot(
            **common,
            rows=[malformed],
            label_payloads=[],
        )

    association = {
        "logical_id": logical_id,
        "concept": "org.quitetall.lamquant.label.seizure-mask-v1",
        "presence": "unknown-at-source",
    }
    with pytest.raises(ValueError, match="duplicate label association"):
        abir.seal_training_snapshot(
            **common,
            rows=[row],
            label_payloads=[association, dict(association)],
        )

    oversized_element = "x" * (16 * 1024 * 1024 + 1)
    with pytest.raises(ValueError, match="catalog resource bound"):
        abir.seal_training_snapshot(
            **common,
            rows=[row],
            label_payloads=[{
                "logical_id": logical_id,
                "concept": "org.quitetall.lamquant.label.seizure-mask-v1",
                "presence": "present",
                "element": oversized_element,
                "byte_order": "not-applicable",
                "payload": bytes([1]),
            }],
        )


def test_training_v2_schema_admits_typed_labels_and_v1_rejects_them():
    root = Path(__file__).parents[2]
    v1 = json.loads((root / "schema/training-snapshot-v1.schema.json").read_text())
    v2 = json.loads((root / "schema/training-snapshot-v2.schema.json").read_text())
    jsonschema.Draft202012Validator.check_schema(v2)
    content_id = "1" * 64
    catalog = {
        "dataset_roots": [content_id],
        "decision_log_id": "2" * 64,
        "label_payloads": [{
            "concept": "org.quitetall.lamquant.label.seizure-mask-v1",
            "logical_id": "3" * 64,
            "payload": {
                "byte_order": "not-applicable",
                "element": "u8",
                "logical_bytes": 2,
                "payload": "4" * 64,
                "shape": [2],
            },
            "presence": "present",
        }],
        "profile": "balanced",
        "rows": [{
            "byte_order": "little",
            "element": "i16",
            "group": "5" * 64,
            "label": "6" * 64,
            "logical_bytes": 4,
            "logical_id": "3" * 64,
            "payload": "7" * 64,
            "shape": [2],
            "split": "8" * 64,
        }],
        "schema": "org.quitetall.abir.training.snapshot-v2",
        "sealed": True,
        "spec_id": "9" * 64,
    }

    jsonschema.validate(catalog, v2)
    assert list(jsonschema.Draft202012Validator(v1).iter_errors(catalog))

    catalog["label_payloads"][0]["presence"] = "unknown-at-source"
    assert list(jsonschema.Draft202012Validator(v2).iter_errors(catalog))


def _acceptance_spec(*, knobs=("worker-count",)):
    return {
        "augmentation": "01" * 32,
        "authorized_purpose": "representation-learning",
        "cohort": "02" * 32,
        "feature": "03" * 32,
        "fitted_state": "04" * 32,
        "grouping": "05" * 32,
        "label": "06" * 32,
        "policy": "07" * 32,
        "preprocessing": "08" * 32,
        "sampler": "09" * 32,
        "seed": 42,
        "split": "0a" * 32,
        "view": "0b" * 32,
        "window": "0c" * 32,
        "allowed_adaptive_knobs": list(knobs),
    }


def _validate_acceptance_artifact(artifact):
    root = Path(__file__).parents[2]
    schema = json.loads((root / "schema/training-acceptance-v1.schema.json").read_text())
    jsonschema.Draft202012Validator.check_schema(schema)
    jsonschema.validate(json.loads(artifact), schema)


@pytest.mark.parametrize(
    ("profile", "expected"),
    [
        (
            "speed",
            {
                "cache_budget": {"bytes": 1_073_741_824},
                "closure": "allow-verified-external-references",
                "payload_access": "require-mmap",
                "prefetch": {"kind": "rows", "rows": 64},
                "profile": "speed",
                "row_grouping": {"bytes": 67_108_864, "kind": "target-bytes"},
                "schema": "org.quitetall.abir.training.execution-plan-v1",
            },
        ),
        (
            "balanced",
            {
                "cache_budget": {"bytes": 268_435_456},
                "closure": "allow-verified-external-references",
                "payload_access": "prefer-mmap",
                "prefetch": {"kind": "rows", "rows": 16},
                "profile": "balanced",
                "row_grouping": {"bytes": 16_777_216, "kind": "target-bytes"},
                "schema": "org.quitetall.abir.training.execution-plan-v1",
            },
        ),
        (
            "memory",
            {
                "cache_budget": {"bytes": 33_554_432},
                "closure": "allow-verified-external-references",
                "payload_access": "prefer-mmap",
                "prefetch": {"kind": "rows", "rows": 1},
                "profile": "memory",
                "row_grouping": {"kind": "fixed-rows", "rows": 1},
                "schema": "org.quitetall.abir.training.execution-plan-v1",
            },
        ),
        (
            "compact",
            {
                "cache_budget": {"bytes": 134_217_728},
                "closure": "portable",
                "payload_access": "materialize",
                "prefetch": {"kind": "rows", "rows": 4},
                "profile": "compact",
                "row_grouping": {"bytes": 8_388_608, "kind": "target-bytes"},
                "schema": "org.quitetall.abir.training.execution-plan-v1",
            },
        ),
        (
            "ultra-compact",
            {
                "cache_budget": {"bytes": 16_777_216},
                "closure": "portable",
                "payload_access": "stream",
                "prefetch": {"kind": "disabled"},
                "profile": "ultra-compact",
                "row_grouping": {"bytes": 67_108_864, "kind": "target-bytes"},
                "schema": "org.quitetall.abir.training.execution-plan-v1",
            },
        ),
        (
            "stream",
            {
                "cache_budget": {"bytes": 8_388_608},
                "closure": "allow-verified-external-references",
                "payload_access": "stream",
                "prefetch": {"kind": "rows", "rows": 2},
                "profile": "stream",
                "row_grouping": {"kind": "fixed-rows", "rows": 1},
                "schema": "org.quitetall.abir.training.execution-plan-v1",
            },
        ),
    ],
)
def test_training_execution_plan_binding_matches_canonical_rust_profiles(
    profile, expected
):
    compiled = abir.compile_training_execution_plan(profile)

    assert json.loads(compiled["canonical_json"]) == expected
    assert compiled["canonical_json"] == json.dumps(
        expected, separators=(",", ":"), sort_keys=True
    )
    assert len(compiled["plan_id"]) == 64


def test_training_execution_plan_binding_rejects_unknown_profile():
    with pytest.raises(ValueError, match="unknown training profile"):
        abir.compile_training_execution_plan("turbo")


def test_public_decision_replay_reopens_durable_log_and_fails_closed():
    spec = _acceptance_spec()
    records = [{
        "activation_barrier": 10,
        "decision": "0d" * 32,
        "durable_before_activation": True,
        "knob": "worker-count",
        "rank": 0,
        "sequence": 0,
    }]
    sealed = abir.seal_training_decision_log(spec=spec, records=records)
    snapshot = abir.seal_training_snapshot(
        dataset_roots=["31" * 32],
        spec_id=sealed["spec_id"],
        profile="balanced",
        rows=[{
            "logical_id": "32" * 32,
            "group": "33" * 32,
            "label": "34" * 32,
            "split": "35" * 32,
            "element": "i16",
            "byte_order": "little",
            "shape": [1],
            "payload": bytes([1, 0]),
        }],
        label_payloads=[],
        decision_log_id=sealed["decision_log_id"],
    )
    receipt = abir.verify_training_decision_replay(
        snapshot=snapshot["artifact"],
        spec=spec,
        decision_log=sealed["decision_log"],
        records=records,
    )

    assert receipt["decision_log_id"] == sealed["decision_log_id"]
    assert receipt["record_count"] == 1
    assert len(receipt["receipt_id"]) == 64
    _validate_acceptance_artifact(receipt["receipt"])

    changed = [dict(records[0], decision="0e" * 32)]
    with pytest.raises(ValueError, match="decision replay identity mismatch"):
        abir.verify_training_decision_replay(
            snapshot=snapshot["artifact"],
            spec=spec,
            decision_log=sealed["decision_log"],
            records=changed,
        )

    with pytest.raises(ValueError, match="unknown training metadata field"):
        abir.seal_training_decision_log(
            spec=dict(spec, optimizer_schedule="unbound"), records=records
        )
    with pytest.raises(ValueError, match="unknown training metadata field"):
        abir.seal_training_decision_log(
            spec=spec, records=[dict(records[0], worker_epoch=3)]
        )


def test_public_source_equivalence_requires_exact_validated_windows():
    row = {
        "logical_id": "21" * 32,
        "group": "22" * 32,
        "label": "23" * 32,
        "split": "24" * 32,
        "element": "i16",
        "byte_order": "little",
        "shape": [2],
        "payload": bytes([1, 0, 2, 0]),
    }
    common = {
        "spec_id": "25" * 32,
        "profile": "balanced",
        "rows": [row],
        "label_payloads": [],
        "decision_log_id": "26" * 32,
    }
    first = abir.seal_training_snapshot(dataset_roots=["27" * 32], **common)
    second = abir.seal_training_snapshot(dataset_roots=["28" * 32], **common)
    receipt = abir.verify_training_source_equivalence(
        first["artifact"], second["artifact"]
    )
    assert receipt["row_count"] == 1
    assert receipt["first_snapshot_id"] == first["snapshot_id"]
    assert receipt["second_snapshot_id"] == second["snapshot_id"]
    assert receipt["first_snapshot_id"] != receipt["second_snapshot_id"]
    assert receipt["first_dataset_roots_id"] != receipt["second_dataset_roots_id"]
    assert len(receipt["logical_windows_id"]) == 64
    assert len(receipt["receipt_id"]) == 64
    _validate_acceptance_artifact(receipt["receipt"])

    other = abir._training_fixture_bytes(payload_bytes=10)
    with pytest.raises(ValueError, match="source-equivalent windows"):
        abir.verify_training_source_equivalence(first["artifact"], other)


def test_public_continual_promotion_binds_closed_snapshot_and_log_sequence():
    spec = _acceptance_spec(knobs=())
    decision = abir.seal_training_decision_log(spec=spec, records=[])
    logical_id = "11" * 32
    snapshot = abir.seal_training_snapshot(
        dataset_roots=["12" * 32],
        spec_id=decision["spec_id"],
        profile="stream",
        rows=[{
            "logical_id": logical_id,
            "group": "13" * 32,
            "label": "14" * 32,
            "split": "15" * 32,
            "element": "i16",
            "byte_order": "little",
            "shape": [1],
            "payload": bytes([1, 0]),
        }],
        label_payloads=[],
        decision_log_id=decision["decision_log_id"],
    )
    result = abir.seal_training_continual_promotion(
        subscription_id="16" * 32,
        events=[{
            "correction": None,
            "generation": 0,
            "logical_id": "17" * 32,
            "sequence": 0,
            "snapshot_id": snapshot["snapshot_id"],
            "watermark": 100,
        }],
        spec=spec,
        snapshots=[snapshot["artifact"]],
        decision_logs=[decision["decision_log"]],
        decision_replays=[[]],
    )
    assert result["entry_count"] == 1
    assert len(result["promotion_id"]) == 64
    assert len(result["closed_subscription_id"]) == 64
    _validate_acceptance_artifact(result["closed_subscription"])
    _validate_acceptance_artifact(result["promotion"])

    with pytest.raises(ValueError, match="expected 1 snapshots"):
        abir.seal_training_continual_promotion(
            subscription_id="16" * 32,
            events=[{
                "correction": None,
                "generation": 0,
                "logical_id": "17" * 32,
                "sequence": 0,
                "snapshot_id": snapshot["snapshot_id"],
                "watermark": 100,
            }],
            spec=spec,
            snapshots=[],
            decision_logs=[],
            decision_replays=[],
        )
