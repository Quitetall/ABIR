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


class MergeAuthorshipTests(unittest.TestCase):
    """A merge authors its conflict resolutions, never what it inherited.

    The hook enforced this on the index via MERGE_HEAD, but MERGE_HEAD is gone
    by the time CI re-checks the finished commit -- so merges that passed
    locally failed on the server, demanding a per-file trailer for every file
    the merged branch contributed. That claim would be false.
    """

    def _git(self, repo: Path, *args: str) -> str:
        import subprocess

        return subprocess.run(
            ["git", *args],
            cwd=repo,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            env={
                "GIT_AUTHOR_NAME": "T",
                "GIT_AUTHOR_EMAIL": "t@example.invalid",
                "GIT_COMMITTER_NAME": "T",
                "GIT_COMMITTER_EMAIL": "t@example.invalid",
                "GIT_CONFIG_GLOBAL": "/dev/null",
                "GIT_CONFIG_SYSTEM": "/dev/null",
                "PATH": __import__("os").environ.get("PATH", ""),
            },
        ).stdout.strip()

    def test_clean_merge_reports_only_what_it_reconciled(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            self._git(repo, "init", "-q", "-b", "main")
            (repo / "base.txt").write_text("base\n", encoding="utf-8")
            self._git(repo, "add", "base.txt")
            self._git(repo, "commit", "-q", "-m", "base")

            self._git(repo, "checkout", "-q", "-b", "side")
            (repo / "from_side.txt").write_text("side\n", encoding="utf-8")
            self._git(repo, "add", "from_side.txt")
            self._git(repo, "commit", "-q", "-m", "side adds a file")

            self._git(repo, "checkout", "-q", "main")
            (repo / "from_main.txt").write_text("main\n", encoding="utf-8")
            self._git(repo, "add", "from_main.txt")
            self._git(repo, "commit", "-q", "-m", "main adds a file")

            self._git(repo, "merge", "-q", "--no-ff", "-m", "merge side", "side")
            merge_sha = self._git(repo, "rev-parse", "HEAD")

            paths = {
                change.path.decode() for change in PROVENANCE.commit_changes(merge_sha, repo)
            }

            # from_side.txt arrived verbatim from the second parent: the merger
            # did not author it, so it must not demand a trailer here.
            self.assertNotIn("from_side.txt", paths)
            self.assertEqual(paths, set())

    def test_non_merge_commit_is_unaffected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            self._git(repo, "init", "-q", "-b", "main")
            (repo / "only.txt").write_text("x\n", encoding="utf-8")
            self._git(repo, "add", "only.txt")
            self._git(repo, "commit", "-q", "-m", "single parent")
            sha = self._git(repo, "rev-parse", "HEAD")

            paths = {
                change.path.decode() for change in PROVENANCE.commit_changes(sha, repo)
            }
            self.assertEqual(paths, {"only.txt"})


class ErrataTests(unittest.TestCase):
    """Errata excuse ONE operation mismatch, and only the exact one recorded."""

    def test_excuses_only_the_exact_recorded_mismatch(self) -> None:
        entry = {"declared_operation": "modify", "actual_operation": "add"}
        self.assertTrue(PROVENANCE._errata_excuses(entry, "modify", "add"))
        # A different mismatch on the same path is NOT excused.
        self.assertFalse(PROVENANCE._errata_excuses(entry, "add", "delete"))
        self.assertFalse(PROVENANCE._errata_excuses(entry, "modify", "delete"))
        # Absent record excuses nothing.
        self.assertFalse(PROVENANCE._errata_excuses(None, "modify", "add"))

    def test_absent_file_is_the_normal_state(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            self.assertEqual(PROVENANCE.load_errata(Path(directory)), {})

    def test_records_are_keyed_by_commit_and_path(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / PROVENANCE.ERRATA_PATH
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(
                json.dumps(
                    {
                        "corrections": [
                            {
                                "commit": "c" * 40,
                                "path": "contract.toml",
                                "declared_operation": "modify",
                                "actual_operation": "add",
                                "reason": "verified against the parent tree",
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )

            errata = PROVENANCE.load_errata(Path(directory))
            self.assertIn("contract.toml", errata["c" * 40])
            # A record for one commit never applies to another.
            self.assertNotIn("d" * 40, errata)


if __name__ == "__main__":
    unittest.main()
