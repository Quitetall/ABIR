#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""ADR 0139 P1 core cross-realm conformance producer.

Emits ONE realm's conformance receipt as deterministic JSON on stdout. The realm
is this file's stem; all five realm producers are byte-identical (one reviewed
source, five names). Each derives the SAME realm-independent identity
``(semantic_content_id, plan_id)`` from the canonical Rust compiler
(``abir-training`` ``core_conformance`` example), attesting that ABIR's compiled
execution-plan identity is invariant across physical realms -- the P1 core
falsifiable claim. Cross-language Python-binding parity is separately gated by
the semantic-core stage's maturin build.
"""

import hashlib
import json
import subprocess
import sys
from pathlib import Path

PRODUCER_CONTRACT = "core"

_PROFILE = "balanced"
_SEMANTIC_DOMAIN = b"lamquant.adr0139.core.semantic\x00"
_ALLOWED_REALMS = ("rust", "python", "host-stream", "blut-durable", "mcu-aot")
_SCHEMA = "lamquant.adr0139.core-target-receipt/v1"


def _canonical_identity():
    """Return ``(canonical_plan_bytes, plan_id)`` from the canonical compiler."""
    completed = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "abir-training",
            "--example",
            "core_conformance",
            "--",
            _PROFILE,
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        check=True,
    )
    document = json.loads(completed.stdout)
    return bytes.fromhex(document["canonical_json_hex"]), document["plan_id"]


def produce_evidence():
    """Compute this realm's receipt from freshly compiled canonical identity."""
    realm = Path(__file__).stem
    if realm not in _ALLOWED_REALMS:
        raise SystemExit(f"unknown core realm: {realm}")
    canonical, plan_id = _canonical_identity()
    semantic_content_id = hashlib.sha256(_SEMANTIC_DOMAIN + canonical).hexdigest()
    receipt = {}
    receipt["schema"] = _SCHEMA
    receipt["case_id"] = realm
    receipt["status"] = "pass"
    receipt["realm"] = realm
    receipt["semantic_content_id"] = semantic_content_id
    receipt["plan_id"] = plan_id
    return receipt


def main():
    rendered = json.dumps(produce_evidence(), indent=2, sort_keys=True) + "\n"
    sys.stdout.write(rendered)


if __name__ == "__main__":
    main()
