#!/usr/bin/env python3
"""Verify the normative Adapter v1 manifest and JSON schemas."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

import jsonschema


ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    manifest = json.loads((ROOT / "spec/adapter-v1.manifest.json").read_text())
    assert manifest["manifest_version"] == 1
    assert manifest["adapter_protocol_version"] == 1
    paths: set[str] = set()
    for artifact in manifest["artifacts"]:
        path = artifact["path"]
        assert path not in paths, f"duplicate manifest path: {path}"
        paths.add(path)
        actual = hashlib.sha256((ROOT / path).read_bytes()).hexdigest()
        assert actual == artifact["sha256"], f"hash mismatch: {path}"

    profile_schema = json.loads((ROOT / "schema/adapter-profile-v1.schema.json").read_text())
    mapping_schema = json.loads((ROOT / "schema/adapter-mapping-v1.schema.json").read_text())
    validation_schema = json.loads(
        (ROOT / "schema/adapter-validation-v1.schema.json").read_text()
    )
    for schema in (profile_schema, mapping_schema, validation_schema):
        jsonschema.Draft202012Validator.check_schema(schema)
    registry = json.loads((ROOT / "registries/adapter-profiles-v1.json").read_text())
    jsonschema.Draft202012Validator(profile_schema).validate(registry)

    ids = [profile["id"] for profile in registry["profiles"]]
    assert len(ids) == len(set(ids)), "duplicate Adapter profile identifier"
    semantic = {
        profile["id"]
        for profile in registry["profiles"]
        if profile["status"] == "semantic"
    }
    assert semantic == {
        "edfplus.1.signal",
        "bids.1.11.1.single-edf-eeg",
        "nwb.2.10.0.single-integer-timeseries",
        "dicom.ps3.2026c.ecg-i16",
    }
    print("adapter-v1 contract: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
