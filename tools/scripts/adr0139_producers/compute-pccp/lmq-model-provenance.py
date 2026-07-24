#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""ADR 0139 P3 PCCP model-provenance producer.\n\nEmits ONE PCCP artifact receipt as deterministic JSON on stdout. The neural\nLMQ profile requires typed model provenance on the wire; this encodes such a\nbundle and verifies the provenance decoded intact. The recorded PCCP status\nis whatever the artifact declares and is never upgraded here: verifying that\na typed attestation is present and bound is a different claim from asserting\nthat the model passed its PCCP gate."""

import json
import subprocess
import sys
from pathlib import Path

PRODUCER_CONTRACT = "compute-pccp"

_CASES = {
    "lmq-model-provenance": "lmq",
}
_SCHEMA = "lamquant.adr0139.compute-pccp-receipt/v1"


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
    """Verify typed model provenance survived the wire and return its receipt."""
    case = Path(__file__).stem
    if case not in _CASES:
        raise SystemExit(f"unknown case: {case}")
    measured = _probe(_CASES[case])
    bound = measured["model_provenance_bound"] is True
    checkpoint = measured["checkpoint_sha256"]
    verified = bound and len(checkpoint) == 64
    receipt = {}
    receipt["schema"] = _SCHEMA
    receipt["case_id"] = case
    receipt["profile_id"] = measured["profile_id"]
    receipt["status"] = "pass" if verified else "fail"
    receipt["typed_training_attestation_verified"] = verified
    receipt["checkpoint_sha256"] = checkpoint
    receipt["pccp_status"] = measured["pccp_status"]
    return receipt


def main():
    rendered = json.dumps(produce_evidence(), indent=2, sort_keys=True) + "\n"
    sys.stdout.write(rendered)


if __name__ == "__main__":
    main()
