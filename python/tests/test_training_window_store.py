import gc
import json
import os
from pathlib import Path
import subprocess
import sys

import abir
import numpy as np
import pytest


def test_training_window_store_opens_validated_bundle_and_lends_rows():
    artifact = abir._training_fixture_bytes()
    store = abir.TrainingWindowStore.open_bytes(artifact)

    assert store.profile == "balanced"
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
    assert not row.flags.writeable


def test_training_window_store_rejects_corrupt_path(tmp_path):
    malformed = bytearray(abir._training_fixture_bytes())
    malformed[-1] ^= 1
    path = tmp_path / "corrupt.bcs2"
    path.write_bytes(malformed)

    with pytest.raises(ValueError):
        abir.TrainingWindowStore.open_path(path)


def test_path_row_fails_closed_if_artifact_changes_after_validation(tmp_path):
    artifact = bytearray(abir._training_fixture_bytes())
    path = tmp_path / "changed.bcs2"
    path.write_bytes(artifact)
    store = abir.TrainingWindowStore.open_path(path)

    payload_offset = artifact.find(bytes([1, 0, 2, 0, 3, 0, 4, 0]))
    assert payload_offset >= 0
    with path.open("r+b") as changed:
        changed.seek(payload_offset)
        changed.write(bytes([9, 0, 2, 0, 3, 0, 4, 0]))

    with pytest.raises(ValueError, match="changed after validation"):
        store.row_numpy(store.row_ids[0])

    with path.open("r+b") as truncated:
        truncated.truncate(payload_offset + 2)
    with pytest.raises(OSError, match="read training row"):
        store.row_numpy(store.row_ids[0])


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
