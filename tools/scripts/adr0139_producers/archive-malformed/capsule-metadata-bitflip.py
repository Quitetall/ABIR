#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""ADR 0139 P2 archive malformed-capsule rejection producer.

Emits ONE damaged-capsule receipt as deterministic JSON on stdout. The damage
site is this file's stem; all malformed producers are byte-identical (one
reviewed source, several names). Each wraps a deterministic biosignal fixture in
a BCS2 forensic capsule, flips one byte at a specific region of the encoded
capsule, and requires the reader to REJECT it before materializing anything. A
content-addressed archive that restored attacker-chosen bytes would fabricate
provenance for data the source never contained.
"""

import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

PRODUCER_CONTRACT = "archive-malformed"

# Damage site -> byte offset into the encoded capsule.
_CASES = {
    "capsule-header-bitflip": 8,
    "capsule-metadata-bitflip": 64,
    "capsule-payload-bitflip": 300,
    "capsule-trailer-bitflip": 1024,
}
_SCHEMA = "lamquant.adr0139.archive-malformed-receipt/v1"


def _field(target, start, width, value):
    encoded = value.encode("ascii")
    if len(encoded) > width:
        raise SystemExit(f"field too large: {value}")
    target[start : start + width] = encoded.ljust(width, b" ")


def _fixture():
    """Build the deterministic EDF+C payload that every case then damages."""
    labels = ["EEG Fp1", "AUX", "EDF Annotations"]
    columns = [
        (16, labels),
        (80, ["", "", ""]),
        (8, ["uV", "mV", ""]),
        (8, ["-100", "-10", "-1"]),
        (8, ["100", "10", "1"]),
        (8, ["-32768", "-32768", "-32768"]),
        (8, ["32767", "32767", "32767"]),
        (80, ["", "", ""]),
        (8, ["4", "2", "64"]),
        (32, ["", "", ""]),
    ]
    header_len = 256 + len(labels) * 256
    output = bytearray(b" " * header_len)
    _field(output, 0, 8, "0")
    _field(output, 8, 80, "patient")
    _field(output, 88, 80, "recording")
    _field(output, 168, 8, "22.07.26")
    _field(output, 176, 8, "13.00.00")
    _field(output, 184, 8, str(header_len))
    _field(output, 192, 44, "EDF+C")
    _field(output, 236, 8, "2")
    _field(output, 244, 8, "1")
    _field(output, 252, 4, str(len(labels)))
    cursor = 256
    for width, entries in columns:
        for value in entries:
            _field(output, cursor, width, value)
            cursor += width
    for record in range(2):
        eeg = [1, 2, 3, 4] if record == 0 else [5, 6, 7, 8]
        aux = [-3, 4] if record == 0 else [-5, 6]
        for value in eeg + aux:
            output.extend(int(value).to_bytes(2, "little", signed=True))
        tal = b"+0\x14\x14\0" if record == 0 else b"+1\x14\x14\0"
        annotation = bytearray(128)
        annotation[: len(tal)] = tal
        output.extend(annotation)
    return bytes(output)


def _probe(source, offset):
    """Damage the capsule at `offset`; nonzero exit means correct rejection."""
    completed = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "abir-bcs",
            "--example",
            "forensic_capsule_probe",
            "--",
            str(source),
            str(offset),
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return completed.returncode


def produce_evidence():
    """Damage one capsule region and return its rejection receipt."""
    case = Path(__file__).stem
    if case not in _CASES:
        raise SystemExit(f"unknown malformed capsule case: {case}")
    offset = _CASES[case]
    payload = _fixture()
    with tempfile.TemporaryDirectory(prefix="adr0139-capsule-") as temporary:
        source = Path(temporary) / "source.edf"
        source.write_bytes(payload)
        code = _probe(source, offset)
    rejected = code != 0
    receipt = {}
    receipt["schema"] = _SCHEMA
    receipt["case_id"] = case
    receipt["status"] = "pass" if rejected else "fail"
    receipt["rejected"] = rejected
    receipt["damage_offset"] = offset
    receipt["fixture_sha256"] = hashlib.sha256(payload).hexdigest()
    receipt["reader_exit_code"] = code
    return receipt


def main():
    rendered = json.dumps(produce_evidence(), indent=2, sort_keys=True) + "\n"
    sys.stdout.write(rendered)


if __name__ == "__main__":
    main()
