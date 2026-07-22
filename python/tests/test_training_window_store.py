import gc

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
    expected_pointer = store.row_pointer(row_id)
    row = store.row_numpy(row_id)

    assert row.shape == (2, 2)
    assert row.dtype == np.dtype("<i2")
    assert row.tolist() == [[1, 2], [3, 4]]
    assert row.__array_interface__["data"][0] == expected_pointer
    assert np.shares_memory(row.view(np.uint8), np.frombuffer(artifact, dtype=np.uint8))
    assert not row.flags.writeable

    del store
    del artifact
    gc.collect()

    assert row.tolist() == [[1, 2], [3, 4]]
    assert row.__array_interface__["data"][0] == expected_pointer


def test_training_window_store_rejects_corruption_and_unknown_rows():
    artifact = abir._training_fixture_bytes()
    store = abir.TrainingWindowStore.open_bytes(artifact)
    with pytest.raises(KeyError):
        store.row_numpy("00" * 32)

    malformed = bytearray(artifact)
    malformed[-1] ^= 1
    with pytest.raises(ValueError):
        abir.TrainingWindowStore.open_bytes(bytes(malformed))
