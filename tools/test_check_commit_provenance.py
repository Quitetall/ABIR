#!/usr/bin/env python3
"""Regression tests for append-only commit provenance corrections."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("check_commit_provenance.py")
SPEC = importlib.util.spec_from_file_location("check_commit_provenance", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
PROVENANCE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = PROVENANCE
SPEC.loader.exec_module(PROVENANCE)


def message_digest(message: str) -> str:
    return hashlib.sha256(message.encode("utf-8")).hexdigest()


class CorrectionTests(unittest.TestCase):
    def test_loader_rejects_obsolete_schema(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / PROVENANCE.CORRECTIONS_PATH
            path.write_text(
                json.dumps({"schema_version": 1, "corrections": []}),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(RuntimeError, "must use schema_version 2"):
                PROVENANCE._load_corrections(
                    Path(directory),
                    {"corrections_path": PROVENANCE.CORRECTIONS_PATH},
                )

    def test_loader_requires_evidence_for_appended_records(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / PROVENANCE.CORRECTIONS_PATH
            path.write_text(
                json.dumps(
                    {
                        "schema_version": 2,
                        "corrections": [
                            {
                                "commit": "e" * 40,
                                "message_sha256": "f" * 64,
                                "reason": "Attempt an unsupported append.",
                                "add_file_contributions": [
                                    {
                                        "by": [
                                            {
                                                "actor": "human:author",
                                                "role": "author",
                                            }
                                        ],
                                        "operation": "add",
                                        "path": "unverified.rs",
                                        "summary": "Append attribution without evidence.",
                                    }
                                ],
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(
                RuntimeError, "must identify contemporaneous attribution evidence"
            ):
                PROVENANCE._load_corrections(
                    Path(directory),
                    {"corrections_path": PROVENANCE.CORRECTIONS_PATH},
                )

    def test_loader_accepts_role_only_correction_without_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            correction = {
                "commit": "a" * 40,
                "message_sha256": "b" * 64,
                "reason": "Restore a role omitted from an existing actor declaration.",
                "actors": [{"id": "ai:test", "add_roles": ["generator"]}],
            }
            path = Path(directory) / PROVENANCE.CORRECTIONS_PATH
            path.write_text(
                json.dumps({"schema_version": 2, "corrections": [correction]}),
                encoding="utf-8",
            )

            loaded = PROVENANCE._load_corrections(
                Path(directory),
                {"corrections_path": PROVENANCE.CORRECTIONS_PATH},
            )

            self.assertEqual(loaded, {"a" * 40: correction})

    def test_role_only_correction_remains_supported(self) -> None:
        message = (
            "fix: generated output\n\n"
            'AI-Assisted-By: {"agent":"root","id":"ai:test","model":"m",'
            '"product":"p","provider":"v","roles":["author"]}\n'
            'File-Contribution: {"by":[{"actor":"ai:test","role":"generator"}],'
            '"generated_by":"tool generate","operation":"add","path":"result.json",'
            '"summary":"Generate deterministic result fixture."}\n'
        )
        correction = {
            "message_sha256": message_digest(message),
            "actors": [{"id": "ai:test", "add_roles": ["generator"]}],
        }

        corrected, errors = PROVENANCE._apply_correction(message, "a" * 40, correction)

        self.assertEqual(errors, [])
        self.assertEqual(
            PROVENANCE.validate_message(
                corrected, [PROVENANCE.ChangedFile(b"result.json", "add")]
            ),
            [],
        )

    def test_correction_can_append_missing_actor_and_file_record(self) -> None:
        message = "feat: historical implementation\n\nOriginal implementation body.\n"
        correction = {
            "message_sha256": message_digest(message),
            "add_ai_actors": [
                {
                    "agent": "worker",
                    "id": "ai:test",
                    "model": "known-model",
                    "product": "test-product",
                    "provider": "test-provider",
                    "roles": ["author"],
                }
            ],
            "add_file_contributions": [
                {
                    "by": [{"actor": "ai:test", "role": "author"}],
                    "operation": "add",
                    "path": "src/new.rs",
                    "summary": "Add historically omitted implementation attribution.",
                }
            ],
        }

        corrected, errors = PROVENANCE._apply_correction(message, "b" * 40, correction)

        self.assertEqual(errors, [])
        self.assertEqual(
            PROVENANCE.validate_message(
                corrected, [PROVENANCE.ChangedFile(b"src/new.rs", "add")]
            ),
            [],
        )

    def test_appended_records_still_require_exact_changed_path_coverage(self) -> None:
        message = "feat: incomplete correction\n"
        correction = {
            "message_sha256": message_digest(message),
            "add_file_contributions": [
                {
                    "by": [{"actor": "human:author", "role": "author"}],
                    "operation": "add",
                    "path": "wrong.rs",
                    "summary": "Attempt attribution against the wrong path.",
                }
            ],
        }

        corrected, errors = PROVENANCE._apply_correction(message, "c" * 40, correction)
        validation = PROVENANCE.validate_message(
            corrected, [PROVENANCE.ChangedFile(b"right.rs", "add")]
        )

        self.assertEqual(errors, [])
        self.assertTrue(
            any("missing File-Contribution for: 'right.rs'" in item for item in validation)
        )
        self.assertTrue(
            any("unexpected File-Contribution for: 'wrong.rs'" in item for item in validation)
        )

    def test_message_hash_mismatch_rejects_correction(self) -> None:
        corrected, errors = PROVENANCE._apply_correction(
            "feat: changed message\n",
            "d" * 40,
            {
                "message_sha256": "0" * 64,
                "actors": [{"id": "ai:test", "add_roles": ["author"]}],
            },
        )

        self.assertEqual(corrected, "feat: changed message\n")
        self.assertEqual(
            errors,
            [f"provenance correction message hash mismatch for {'d' * 40}"],
        )


if __name__ == "__main__":
    unittest.main()
