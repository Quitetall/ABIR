#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""ADR 0139 P3 semantic-roundtrip producer.\n\nEmits ONE semantic roundtrip receipt as deterministic JSON on stdout. Each\ncase encodes canonical semantics into a codec bundle and re-reads them from\nthe encoded bytes, recording the ContentId going in and coming out. A bundle\nthat loses or rewrites semantics is therefore observable as a ContentId\nmismatch rather than hidden behind an assertion here."""

import json
import subprocess
import sys
from pathlib import Path

PRODUCER_CONTRACT = "compute-semantic"

_CASES = {
    "lml-semantic-roundtrip": "lml",
    "lmq-semantic-roundtrip": "lmq",
}
_SCHEMA = "lamquant.adr0139.compute-semantic-receipt/v1"


def _probe(profile):
    """Run the real codec-bundle probe and return its content-addressed report."""
    completed = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "abir-bcs",
            "--example",
            "codec_bundle_probe",
            "--",
            profile,
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    if completed.returncode != 0:
        raise SystemExit(f"codec bundle probe failed for {profile}")
    return json.loads(completed.stdout)


def produce_evidence():
    """Round-trip canonical semantics through this profile and return its receipt."""
    case = Path(__file__).stem
    if case not in _CASES:
        raise SystemExit(f"unknown case: {case}")
    measured = _probe(_CASES[case])
    receipt = {}
    receipt["schema"] = _SCHEMA
    receipt["case_id"] = case
    receipt["profile_id"] = measured["profile_id"]
    receipt["input_content_id"] = measured["input_content_id"]
    receipt["output_content_id"] = measured["output_content_id"]
    identical = measured["input_content_id"] == measured["output_content_id"]
    receipt["status"] = "pass" if identical else "fail"
    return receipt


def main():
    rendered = json.dumps(produce_evidence(), indent=2, sort_keys=True) + "\n"
    sys.stdout.write(rendered)


if __name__ == "__main__":
    main()
