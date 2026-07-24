#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""ADR 0139 P3 registered-profile producer.\n\nEmits ONE BCS2 codec profile receipt as deterministic JSON on stdout. The\nprofile is this file's stem; all profile producers are byte-identical. Each\nencodes a bundle under that registered profile and reports the profile the\ndecoder read back, so a profile the wire cannot round-trip is observable."""

import json
import subprocess
import sys
from pathlib import Path

PRODUCER_CONTRACT = "compute-profile"

_CASES = {
    "bcs.lml.lossless.v1": "lml",
    "bcs.lmq.progressive.v1": "lmq",
}
_SCHEMA = "lamquant.adr0139.compute-profile-receipt/v1"


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
    """Encode under this registered profile and return its receipt."""
    case = Path(__file__).stem
    if case not in _CASES:
        raise SystemExit(f"unknown case: {case}")
    measured = _probe(_CASES[case])
    receipt = {}
    receipt["schema"] = _SCHEMA
    receipt["case_id"] = case
    receipt["profile_id"] = measured["profile_id"]
    receipt["status"] = "pass" if measured["profile_id"] == case else "fail"
    receipt["profile_code"] = measured["profile_code"]
    receipt["packet_count"] = measured["packet_count"]
    receipt["bundle_bytes"] = measured["bundle_bytes"]
    return receipt


def main():
    rendered = json.dumps(produce_evidence(), indent=2, sort_keys=True) + "\n"
    sys.stdout.write(rendered)


if __name__ == "__main__":
    main()
