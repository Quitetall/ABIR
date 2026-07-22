#!/usr/bin/env python3
"""Positive and malformed fixtures for the Adapter validation receipt."""

from __future__ import annotations

import copy
import hashlib
import json
import tempfile
import unittest
from pathlib import Path

from verify_adapter_contract import ROOT, receipt_errors


SHA_A = "a" * 64
SHA_B = "b" * 64
SHA_C = "c" * 64


def positive_receipt(*, expected: str = "accept") -> dict[str, object]:
    accepted = expected == "accept"
    return {
        "schema_version": 1,
        "profile": "bids.1.11.1.single-edf-eeg",
        "edition": "1.11.1",
        "adapter_revision": "1" * 40,
        "fixture": {
            "path": "fixtures/adapter/bids-valid",
            "sha256": SHA_A,
            "expected_outcome": expected,
        },
        "internal_valid": accepted,
        "independent_evidence": {
            "validator_name": "bids-validator",
            "validator_version": "1.15.0",
            "validator_executable_sha256": SHA_B,
            "schema_or_dictionary_sha256": SHA_C,
            "argv": ["bids-validator", "fixtures/adapter/bids-valid"],
            "executed_at_utc": "2026-07-22T14:05:06Z",
            "exit_code": 0 if accepted else 1,
            "stdout_sha256": SHA_A,
            "stderr_sha256": SHA_B,
            "error_count": 0 if accepted else 1,
            "warning_count": 0,
            "authority": "conformance",
            "observed_outcome": expected,
            "expected_outcome_observed": True,
        },
        "semantic_profile_promoted": True,
        "pass": True,
        "diagnostics": [],
    }


class AdapterValidationContractTests(unittest.TestCase):
    def test_positive_accept_receipt(self) -> None:
        self.assertEqual(receipt_errors(positive_receipt()), [])

    def test_positive_reject_receipt(self) -> None:
        self.assertEqual(receipt_errors(positive_receipt(expected="reject")), [])

    def test_explicit_missing_evidence_is_a_valid_failure(self) -> None:
        receipt = positive_receipt()
        receipt["independent_evidence"] = None
        receipt["semantic_profile_promoted"] = False
        receipt["pass"] = False
        receipt["diagnostics"] = ["independent validator unavailable"]
        self.assertEqual(receipt_errors(receipt), [])

    def test_parser_only_cannot_pass_or_promote(self) -> None:
        receipt = positive_receipt()
        evidence = copy.deepcopy(receipt["independent_evidence"])
        assert isinstance(evidence, dict)
        evidence["authority"] = "parser-only"
        receipt["independent_evidence"] = evidence
        errors = receipt_errors(receipt)
        self.assertTrue(any("pass" in error for error in errors), errors)
        self.assertTrue(any("semantic_profile_promoted" in error for error in errors), errors)

    def test_malformed_schema_fields_are_rejected(self) -> None:
        receipt = positive_receipt()
        evidence = copy.deepcopy(receipt["independent_evidence"])
        assert isinstance(evidence, dict)
        del evidence["validator_executable_sha256"]
        evidence["stdout_sha256"] = "not-a-sha256"
        evidence["executed_at_utc"] = "2026-07-22T10:05:06-04:00"
        receipt["independent_evidence"] = evidence
        errors = receipt_errors(receipt)
        self.assertTrue(errors)
        self.assertTrue(any("schema" in error for error in errors), errors)

    def test_cross_field_outcome_mismatch_is_rejected(self) -> None:
        receipt = positive_receipt()
        evidence = copy.deepcopy(receipt["independent_evidence"])
        assert isinstance(evidence, dict)
        evidence["observed_outcome"] = "reject"
        receipt["independent_evidence"] = evidence
        errors = receipt_errors(receipt)
        self.assertTrue(
            any("expected_outcome_observed" in error for error in errors), errors
        )
        self.assertTrue(any("pass must equal" in error for error in errors), errors)

    def test_fixture_digest_is_verified_when_root_is_supplied(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixture = root / "fixture.dat"
            fixture.write_bytes(b"ABIR adapter fixture")
            receipt = positive_receipt()
            fixture_record = copy.deepcopy(receipt["fixture"])
            assert isinstance(fixture_record, dict)
            fixture_record["path"] = fixture.name
            fixture_record["sha256"] = hashlib.sha256(fixture.read_bytes()).hexdigest()
            receipt["fixture"] = fixture_record
            self.assertEqual(receipt_errors(receipt, fixture_root=root), [])

            fixture.write_bytes(b"mutated")
            errors = receipt_errors(receipt, fixture_root=root)
            self.assertTrue(any("fixture sha256 mismatch" in error for error in errors))

    def test_manifest_tracks_this_test(self) -> None:
        manifest = json.loads((ROOT / "spec/adapter-v1.manifest.json").read_text())
        paths = {artifact["path"] for artifact in manifest["artifacts"]}
        self.assertIn("tools/test_adapter_validation_contract.py", paths)


if __name__ == "__main__":
    unittest.main()
