#!/usr/bin/env python3
"""Collect revision-bound ADR 0141 BCS2/store baseline evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ARTIFACTS = [
    "fixtures/bcs2/v1/manifest.json",
    "registries/bcs2-crypto-v1.json",
    "registries/bcs2-profiles-v1.json",
    "schema/bcs2-crypto-v1.schema.json",
    "schema/bcs2-profile-v1.schema.json",
    "spec/BCS2_V1.md",
    "spec/bcs2-v1.manifest.json",
    "tools/verify_bcs2_vectors.py",
]


def run(*command: str) -> str:
    completed = subprocess.run(
        command,
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    return completed.stdout.strip()


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--iterations", type=int, default=100_000)
    parser.add_argument("--fuzz-runs", type=int, default=1_000)
    parser.add_argument("--output", default="evidence/bcs2-store.json")
    arguments = parser.parse_args()
    if arguments.iterations <= 0 or arguments.fuzz_runs <= 0:
        parser.error("iteration and fuzz counts must be positive")

    revision = run("git", "rev-parse", "HEAD")
    measurements = json.loads(
        run(
            "cargo",
            "run",
            "--release",
            "-p",
            "abir-conformance",
            "--bin",
            "measure_bcs2_store",
            "--",
            str(arguments.iterations),
        )
    )
    evidence = {
        "schema_version": 1,
        "stage": "bcs2-store",
        "status": "BASELINE",
        "tested_revision": revision,
        "collected_at": datetime.now(timezone.utc).isoformat(),
        "commands": {
            "measurement": (
                "cargo run --release -p abir-conformance --bin "
                f"measure_bcs2_store -- {arguments.iterations}"
            ),
            "independent_vectors": "python3 tools/verify_bcs2_vectors.py fixtures/bcs2/v1",
            "fuzz": (
                "cargo +nightly fuzz run bcs2_wire <temporary-corpus> -- "
                f"-runs={arguments.fuzz_runs}"
            ),
        },
        "fuzz_runs": arguments.fuzz_runs,
        "measurements": measurements,
        "artifact_sha256": {
            relative: digest(ROOT / relative) for relative in ARTIFACTS
        },
    }
    output = ROOT / arguments.output
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
