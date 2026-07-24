#!/usr/bin/env python3
"""Verify the normative Adapter v1 manifest, schemas, and receipts."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any

import jsonschema


ROOT = Path(__file__).resolve().parents[1]


def _load_json(path: Path) -> Any:
    return json.loads(path.read_text())


def _receipt_schema() -> dict[str, Any]:
    return _load_json(ROOT / "schema/adapter-validation-v1.schema.json")


def _profile_registry() -> dict[str, Any]:
    return _load_json(ROOT / "registries/adapter-profiles-v1.json")


def fixture_sha256(path: Path, kind: str) -> str:
    """Hash one file or a complete symlink-free fixture tree."""

    if kind == "file":
        if not path.is_file() or path.is_symlink():
            raise OSError("file fixture is missing, non-regular, or a symlink")
        return hashlib.sha256(path.read_bytes()).hexdigest()
    if kind != "tree" or not path.is_dir() or path.is_symlink():
        raise OSError("tree fixture is missing, not a directory, or a symlink")
    digest = hashlib.sha256(b"abir.adapter.fixture-tree.v1\0")
    files: list[tuple[str, Path]] = []
    for entry in path.rglob("*"):
        if entry.is_symlink():
            raise OSError(f"fixture tree contains a symlink: {entry}")
        if entry.is_dir():
            continue
        if not entry.is_file():
            raise OSError(f"fixture tree contains a non-regular entry: {entry}")
        files.append((entry.relative_to(path).as_posix(), entry))
    if not files:
        raise OSError("fixture tree contains no regular files")
    for relative, entry in sorted(files):
        path_bytes = relative.encode("utf-8")
        content = entry.read_bytes()
        digest.update(len(path_bytes).to_bytes(8, "big"))
        digest.update(path_bytes)
        digest.update(len(content).to_bytes(8, "big"))
        digest.update(content)
    return digest.hexdigest()


def receipt_errors(
    receipt: dict[str, Any],
    *,
    fixture_root: Path | None = None,
    schema: dict[str, Any] | None = None,
    registry: dict[str, Any] | None = None,
) -> list[str]:
    """Return all structural and cross-field errors for one receipt."""

    schema = schema or _receipt_schema()
    registry = registry or _profile_registry()
    validator = jsonschema.Draft202012Validator(
        schema, format_checker=jsonschema.FormatChecker()
    )
    errors = [
        f"schema {error.json_path}: {error.message}"
        for error in sorted(validator.iter_errors(receipt), key=lambda item: list(item.path))
    ]
    if errors:
        return errors

    profiles = {profile["id"]: profile for profile in registry["profiles"]}
    profile = profiles.get(receipt["profile"])
    if profile is None:
        errors.append(f"profile is not registered: {receipt['profile']}")
    elif profile["edition"] != receipt["edition"]:
        errors.append(
            f"edition does not match registry: {receipt['edition']} != {profile['edition']}"
        )

    fixture = receipt["fixture"]
    if fixture_root is not None:
        fixture_path = fixture_root / fixture["path"]
        try:
            actual = fixture_sha256(fixture_path, fixture["kind"])
        except OSError as error:
            errors.append(f"fixture cannot be read: {fixture_path}: {error}")
        else:
            if actual != fixture["sha256"]:
                errors.append(
                    f"fixture sha256 mismatch: {fixture['sha256']} != {actual}"
                )

    expected = fixture["expected_outcome"]
    internal_matches = receipt["internal_valid"] == (expected == "accept")
    if not internal_matches:
        errors.append("internal_valid does not match fixture.expected_outcome")

    evidence = receipt["independent_evidence"]
    independent_matches = False
    conformance_authority = False
    if evidence is not None:
        observed_matches = evidence["observed_outcome"] == expected
        if evidence["expected_outcome_observed"] != observed_matches:
            errors.append(
                "expected_outcome_observed does not match observed_outcome"
            )
        independent_matches = observed_matches and evidence["expected_outcome_observed"]
        conformance_authority = evidence["authority"] == "conformance"

    expected_pass = internal_matches and independent_matches and conformance_authority
    if receipt["pass"] != expected_pass:
        errors.append(
            "pass must equal internal expected outcome plus independent conformance outcome"
        )

    expected_promotion = bool(
        expected_pass and profile is not None and profile["status"] == "semantic"
    )
    if receipt["semantic_profile_promoted"] != expected_promotion:
        errors.append(
            "semantic_profile_promoted must equal pass for a registered semantic profile"
        )
    return errors


def _verify_manifest() -> None:
    manifest = _load_json(ROOT / "spec/adapter-v1.manifest.json")
    assert manifest["manifest_version"] == 1
    assert manifest["adapter_protocol_version"] == 1
    paths: set[str] = set()
    for artifact in manifest["artifacts"]:
        path = artifact["path"]
        assert path not in paths, f"duplicate manifest path: {path}"
        paths.add(path)
        actual = hashlib.sha256((ROOT / path).read_bytes()).hexdigest()
        assert actual == artifact["sha256"], f"hash mismatch: {path}"


def _verify_schemas_and_registry() -> None:
    profile_schema = _load_json(ROOT / "schema/adapter-profile-v1.schema.json")
    mapping_schema = _load_json(ROOT / "schema/adapter-mapping-v1.schema.json")
    validation_schema = _receipt_schema()
    for schema in (profile_schema, mapping_schema, validation_schema):
        jsonschema.Draft202012Validator.check_schema(schema)
    registry = _profile_registry()
    jsonschema.Draft202012Validator(profile_schema).validate(registry)

    ids = [profile["id"] for profile in registry["profiles"]]
    assert len(ids) == len(set(ids)), "duplicate Adapter profile identifier"
    # Registry validation is intentionally structural here. Semantic promotion
    # is earned by the release repository's edition-wide fixture matrix and
    # independently bound validator receipts, not by this normative-schema
    # verifier. Keeping a permanent "no semantic profiles" assertion would make
    # every correctly promoted Adapter invalidate the frozen protocol itself.


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--validation-artifact",
        action="append",
        type=Path,
        default=[],
        help="validate an independent-validator receipt (repeatable)",
    )
    parser.add_argument(
        "--fixture-root",
        type=Path,
        default=ROOT,
        help="root used to resolve receipt fixture paths",
    )
    args = parser.parse_args(argv)

    _verify_manifest()
    _verify_schemas_and_registry()
    for path in args.validation_artifact:
        receipt = _load_json(path)
        errors = receipt_errors(receipt, fixture_root=args.fixture_root)
        assert not errors, f"invalid validation artifact {path}: " + "; ".join(errors)
    print("adapter-v1 contract: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
