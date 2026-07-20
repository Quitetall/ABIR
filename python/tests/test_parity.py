import json
from pathlib import Path

import abir
import jsonschema
import numpy as np
import pytest


ROOT = Path(__file__).resolve().parents[2]


def test_python_matches_rust_canonical_goldens():
    payload = bytes(range(8))
    dataset = abir.Dataset.canonical_fixture(payload)
    assert dataset.canonical_json() == (ROOT / "fixtures/valid/canonical-tensor.json").read_bytes()
    assert dataset.content_id() == (
        ROOT / "fixtures/valid/canonical-tensor.content-id"
    ).read_text().strip()
    assert dataset.recording_count == 1
    assert dataset.stream_count == 1
    assert dataset.atom_count == 1


def test_python_preserves_full_rust_semantic_matrix():
    dataset = abir.Dataset.semantic_matrix_fixture()
    assert dataset.canonical_json() == (
        ROOT / "fixtures/valid/semantic-matrix.json"
    ).read_bytes()
    assert dataset.content_id() == (
        ROOT / "fixtures/valid/semantic-matrix.content-id"
    ).read_text().strip()
    assert dataset.atom_count == 17
    assert dataset.semantic_family_counts == (7, 1, 1, 1, 1, 1)


def test_numpy_view_is_zero_copy_over_original_python_bytes():
    payload = bytes(range(8))
    dataset = abir.Dataset.canonical_fixture(payload)
    array = dataset.numpy_view()
    assert array.shape == (4,)
    assert array.dtype == np.dtype("<i2")
    assert array.__array_interface__["data"][0] == dataset.payload_pointer()
    assert np.shares_memory(array, np.frombuffer(payload, dtype="<i2"))


def test_python_builder_uses_rust_validation_boundary():
    with pytest.raises(ValueError, match="ABIR-E005"):
        abir.Dataset.from_tensor(
            "01" * 16,
            "02" * 16,
            "03" * 16,
            "04" * 16,
            "05" * 32,
            "future:modality/custom",
            "i16",
            "little",
            "dense-row-major",
            [5],
            bytes(8),
        )


def test_rust_fixture_conforms_to_normative_json_schema():
    schema = json.loads((ROOT / "schema/abir-semantic-v1.schema.json").read_text())
    fixture = json.loads((ROOT / "fixtures/valid/canonical-tensor.json").read_text())
    jsonschema.Draft202012Validator.check_schema(schema)
    jsonschema.validate(fixture, schema)


def test_schema_negative_corpus_is_rejected():
    schema = json.loads((ROOT / "schema/abir-semantic-v1.schema.json").read_text())
    validator = jsonschema.Draft202012Validator(schema)
    for path in sorted((ROOT / "fixtures/invalid/schema").glob("*.json")):
        instance = json.loads(path.read_text())
        assert list(validator.iter_errors(instance)), f"negative fixture passed: {path.name}"
