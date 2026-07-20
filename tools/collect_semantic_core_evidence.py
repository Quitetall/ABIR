#!/usr/bin/env python3
"""Collect reproducible ADR 0140 semantic-core baseline evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import statistics
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
HASHED_ARTIFACTS = (
    "spec/semantic-v1.manifest.json",
    "schema/abir-semantic-v1.schema.json",
    "fixtures/valid/canonical-tensor.json",
    "fixtures/valid/canonical-tensor.content-id",
)


def run(*command: str, env: dict[str, str] | None = None) -> str:
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=env,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return completed.stdout.strip()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def python_import_samples(python: str, iterations: int) -> list[float]:
    snippet = "import time;t=time.perf_counter();import abir;print(time.perf_counter()-t)"
    return [float(run(python, "-c", snippet)) for _ in range(iterations)]


def python_zero_copy(python: str) -> dict[str, object]:
    snippet = """
import json
import abir
payload = bytes(range(8))
dataset = abir.Dataset.canonical_fixture(payload)
array = dataset.numpy_view()
print(json.dumps({
    "pointer_identity": array.__array_interface__["data"][0] == dataset.payload_pointer(),
    "shape": list(array.shape),
    "dtype": str(array.dtype),
    "content_id": dataset.content_id(),
}))
"""
    return json.loads(run(python, "-c", snippet))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", default="evidence/semantic-core.json")
    parser.add_argument("--python", default=sys.executable)
    parser.add_argument("--iterations", type=int, default=100_000)
    parser.add_argument("--python-samples", type=int, default=10)
    args = parser.parse_args()
    if args.iterations <= 0 or args.python_samples <= 0:
        parser.error("iteration counts must be positive")

    rust_stdout = run(
        "cargo",
        "run",
        "-q",
        "-p",
        "abir-conformance",
        "--release",
        "--bin",
        "measure_semantic_core",
        env={**__import__("os").environ, "ABIR_BENCH_ITERS": str(args.iterations)},
    )
    rust_metrics = json.loads(rust_stdout)
    imports = python_import_samples(args.python, args.python_samples)
    zero_copy = python_zero_copy(args.python)

    evidence = {
        "schema_version": 1,
        "stage": "semantic-core",
        "status": "BASELINE",
        "collected_at": datetime.now(timezone.utc).isoformat(),
        "revision": run("git", "rev-parse", "HEAD"),
        "host": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "python": run(args.python, "--version"),
            "rustc": run("rustc", "--version"),
        },
        "rust": rust_metrics,
        "python": {
            "import_seconds": {
                "samples": imports,
                "mean": statistics.mean(imports),
                "maximum": max(imports),
            },
            "zero_copy": zero_copy,
        },
        "artifact_sha256": {
            relative: sha256(ROOT / relative) for relative in HASHED_ARTIFACTS
        },
        "commands": [
            f"ABIR_BENCH_ITERS={args.iterations} cargo run -q -p abir-conformance --release --bin measure_semantic_core",
            f"{args.python} -c <import-timer>",
            f"{args.python} -c <zero-copy-probe>",
        ],
        "regression_policy": {
            "structural_limits": "enforced-now",
            "performance_ceiling": "trusted-baseline-only-until-multi-host-samples",
        },
    }
    if not rust_metrics["view"]["pointer_identity"] or not zero_copy["pointer_identity"]:
        raise SystemExit("zero-copy pointer identity failed")

    output = ROOT / args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
    print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
