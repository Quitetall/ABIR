#!/usr/bin/env python3
"""Validate forward-only, per-file contributor provenance in Git commits."""

from __future__ import annotations

import argparse
import base64
import binascii
import hashlib
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any, Sequence


FILE_ROLES = {
    "author",
    "editor",
    "formatter",
    "generator",
    "tester",
    "conflict-resolver",
    "integrator",
}
STATUS_OPERATIONS = {
    "A": "add",
    "M": "modify",
    "D": "delete",
    "T": "typechange",
    "U": "unmerged",
    "X": "unknown",
    "B": "broken-pair",
}
POLICY_VERSION = 1
ACTIVATION_PATH = ".provenance-policy.json"
CORRECTIONS_PATH = ".provenance-corrections.json"
PROVENANCE_PREFIXES = ("Contributor:", "AI-Assisted-By:", "File-Contribution:")
TRAILER_LINE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9-]*:\s+\S.*$")


@dataclass(frozen=True)
class ChangedFile:
    path: bytes
    operation: str


@dataclass(frozen=True)
class RawDiffEntry:
    old_mode: bytes
    new_mode: bytes
    old_oid: bytes
    new_oid: bytes
    status: str
    paths: tuple[bytes, ...]


def _run_git(repo: Path, *args: str, input_bytes: bytes | None = None) -> bytes:
    completed = subprocess.run(
        ["git", *args],
        cwd=repo,
        input=input_bytes,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode:
        detail = completed.stderr.decode("utf-8", errors="replace").strip()
        raise RuntimeError(f"git {' '.join(args)} failed: {detail}")
    return completed.stdout


def parse_raw_entries(raw: bytes) -> list[RawDiffEntry]:
    tokens = raw.split(b"\0")
    entries: list[RawDiffEntry] = []
    index = 0
    while index < len(tokens):
        header = tokens[index]
        index += 1
        if not header:
            continue
        if not header.startswith(b":"):
            raise ValueError(f"invalid raw diff header: {header!r}")

        fields = header[1:].split()
        if len(fields) != 5:
            raise ValueError(f"invalid raw diff field count: {header!r}")
        old_mode, new_mode, old_oid, new_oid, status_raw = fields
        if index >= len(tokens):
            raise ValueError("raw diff ended before path")
        path = tokens[index]
        index += 1
        status = status_raw[:1].decode("ascii", errors="strict")

        paths = [path]
        if status in {"R", "C"}:
            if index >= len(tokens):
                raise ValueError("rename/copy raw diff ended before destination path")
            paths.append(tokens[index])
            index += 1
        entries.append(
            RawDiffEntry(
                old_mode=old_mode,
                new_mode=new_mode,
                old_oid=old_oid,
                new_oid=new_oid,
                status=status,
                paths=tuple(paths),
            )
        )
    return entries


def parse_raw_diff(raw: bytes) -> list[ChangedFile]:
    """Parse ``git diff --raw -z --no-renames`` without losing path bytes."""

    changes: list[ChangedFile] = []
    for entry in parse_raw_entries(raw):
        # Callers disable rename detection. Handle unexpected R/C output safely
        # by expanding it to the same delete/add representation.
        if entry.status in {"R", "C"}:
            source, destination = entry.paths
            if entry.status == "R":
                changes.append(ChangedFile(source, "delete"))
            changes.append(ChangedFile(destination, "add"))
            continue

        operation = STATUS_OPERATIONS.get(entry.status)
        if operation is None:
            raise ValueError(f"unsupported raw diff status: {entry.status!r}")
        if entry.old_mode == b"160000" or entry.new_mode == b"160000":
            operation = "gitlink"
        changes.append(ChangedFile(entry.paths[0], operation))
    return changes


def staged_changes(repo: Path | str = Path(".")) -> list[ChangedFile]:
    repo_path = Path(repo)
    has_head = subprocess.run(
        ["git", "rev-parse", "--verify", "HEAD"],
        cwd=repo_path,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    ).returncode == 0
    if has_head:
        raw = _run_git(repo_path, "diff", "--cached", "--raw", "-z", "--no-renames")
    else:
        empty_tree = _run_git(
            repo_path, "hash-object", "-t", "tree", "--stdin", input_bytes=b""
        ).decode("ascii").strip()
        raw = _run_git(
            repo_path,
            "diff",
            "--cached",
            "--raw",
            "-z",
            "--no-renames",
            empty_tree,
        )
    return parse_raw_diff(raw)


def _commit_raw_diff(repo: Path, commit: str) -> bytes:
    repo_path = Path(repo)
    parents = _run_git(repo_path, "rev-list", "--parents", "-n", "1", commit).split()[
        1:
    ]
    if len(parents) > 1:
        return _run_git(
            repo_path,
            "diff",
            "--raw",
            "-z",
            "--no-renames",
            parents[0].decode("ascii"),
            commit,
        )
    return _run_git(
        repo_path,
        "diff-tree",
        "--root",
        "--no-commit-id",
        "--raw",
        "-r",
        "-z",
        "--no-renames",
        commit,
    )


def commit_changes(commit: str, repo: Path | str = Path(".")) -> list[ChangedFile]:
    return parse_raw_diff(_commit_raw_diff(Path(repo), commit))


def _json_trailers(message: str, key: str) -> tuple[list[dict[str, Any]], list[str]]:
    prefix = f"{key}:"
    values: list[dict[str, Any]] = []
    errors: list[str] = []
    for line_number, line in enumerate(message.splitlines(), start=1):
        if not line.startswith(prefix):
            continue
        payload = line[len(prefix) :].strip()
        try:
            value = json.loads(payload)
        except json.JSONDecodeError as exc:
            errors.append(f"line {line_number}: invalid {key} JSON: {exc.msg}")
            continue
        if not isinstance(value, dict):
            errors.append(f"line {line_number}: {key} must contain a JSON object")
            continue
        values.append(value)
    return values, errors


def _validate_trailer_placement(message: str) -> list[str]:
    lines = message.splitlines()
    significant = [
        index
        for index, line in enumerate(lines)
        if line.strip() and not line.lstrip().startswith("#")
    ]
    provenance_indexes = [
        index
        for index in significant
        if lines[index].startswith(PROVENANCE_PREFIXES)
    ]
    if not provenance_indexes:
        return []

    errors: list[str] = []
    last_content = significant[-1]
    separators = [
        index for index in range(last_content) if not lines[index].strip()
    ]
    if not separators:
        errors.append("final trailer block must be separated from commit body by a blank line")
        return errors
    tail_start = separators[-1] + 1
    tail_content = [index for index in significant if index >= tail_start]
    if any(not TRAILER_LINE.match(lines[index]) for index in tail_content):
        errors.append("final trailer block may contain only one-line Git trailers")
    outside = [index + 1 for index in provenance_indexes if index < tail_start]
    if outside:
        errors.append(
            "provenance records must be in the final trailer block; "
            f"misplaced line(s): {', '.join(map(str, outside))}"
        )
    return errors


def _nonempty_string(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def _string_list(value: Any) -> bool:
    return (
        isinstance(value, list)
        and bool(value)
        and all(_nonempty_string(item) for item in value)
    )


def _record_path(record: dict[str, Any], label: str, errors: list[str]) -> bytes | None:
    has_path = "path" in record
    has_b64 = "path_b64" in record
    if has_path == has_b64:
        errors.append(f"{label}: provide exactly one of path or path_b64")
        return None
    if has_path:
        path = record["path"]
        if not _nonempty_string(path):
            errors.append(f"{label}: path must be a non-empty UTF-8 string")
            return None
        return path.encode("utf-8")
    encoded = record["path_b64"]
    if not _nonempty_string(encoded):
        errors.append(f"{label}: path_b64 must be non-empty base64")
        return None
    try:
        return base64.b64decode(encoded, validate=True)
    except (binascii.Error, ValueError):
        errors.append(f"{label}: path_b64 is not valid base64")
        return None


def _display_path(path: bytes) -> str:
    try:
        return path.decode("utf-8")
    except UnicodeDecodeError:
        return f"base64:{base64.b64encode(path).decode('ascii')}"


def _register_actor(
    actor: dict[str, Any],
    label: str,
    actor_roles: dict[str, set[str] | None],
    errors: list[str],
) -> str | None:
    actor_id = actor.get("id")
    if not _nonempty_string(actor_id):
        errors.append(f"{label}: id must be non-empty")
        return None
    if actor_id in actor_roles:
        errors.append(f"{label}: duplicate actor id {actor_id!r}")
    roles = actor.get("roles")
    actor_roles[actor_id] = set(roles) if _string_list(roles) else set()
    if not _string_list(roles):
        errors.append(f"{label}: roles must be a non-empty string list")
    return actor_id


def _validate_contributor(
    actor: dict[str, Any],
    index: int,
    actor_roles: dict[str, set[str] | None],
    errors: list[str],
) -> None:
    label = f"Contributor[{index}]"
    actor_id = _register_actor(actor, label, actor_roles, errors)
    if actor_id is None:
        return
    kind = actor.get("kind")
    if actor_id == "human:author":
        errors.append(f"{label}: human:author is reserved for Git author metadata")
    if kind not in {"human", "automation", "bot"}:
        errors.append(f"{label}: kind must be human, automation, or bot")
    expected_prefix = {
        "human": "human:",
        "automation": "automation:",
        "bot": "bot:",
    }.get(kind)
    if expected_prefix is not None and not actor_id.startswith(expected_prefix):
        errors.append(f"{label}: {kind} id must start with {expected_prefix!r}")
    if not _nonempty_string(actor.get("name")):
        errors.append(f"{label}: name must be non-empty")
    if kind == "human" and not _nonempty_string(actor.get("email")):
        errors.append(f"{label}: human contributor requires email")


def _validate_ai_actor(
    actor: dict[str, Any],
    index: int,
    actor_roles: dict[str, set[str] | None],
    errors: list[str],
) -> None:
    label = f"AI-Assisted-By[{index}]"
    actor_id = _register_actor(actor, label, actor_roles, errors)
    if actor_id is None:
        return
    if not actor_id.startswith("ai:"):
        errors.append(f"{label}: id must start with 'ai:'")
    for field in ("provider", "product", "model", "agent"):
        if not _nonempty_string(actor.get(field)):
            errors.append(f"{label}: {field} must be non-empty")


def validate_message(message: str, changes: Sequence[ChangedFile]) -> list[str]:
    """Return every schema or coverage error; empty means valid."""

    contributors, errors = _json_trailers(message, "Contributor")
    errors.extend(_validate_trailer_placement(message))
    ai_actors, ai_errors = _json_trailers(message, "AI-Assisted-By")
    file_records, file_errors = _json_trailers(message, "File-Contribution")
    errors.extend(ai_errors)
    errors.extend(file_errors)

    actor_roles: dict[str, set[str] | None] = {"human:author": None}
    for index, actor in enumerate(contributors, start=1):
        _validate_contributor(actor, index, actor_roles, errors)
    for index, actor in enumerate(ai_actors, start=1):
        _validate_ai_actor(actor, index, actor_roles, errors)

    expected = {change.path: change.operation for change in changes}
    seen: dict[bytes, str] = {}
    for index, record in enumerate(file_records, start=1):
        label = f"File-Contribution[{index}]"
        path = _record_path(record, label, errors)
        if path is None:
            continue
        if path in seen:
            errors.append(f"{label}: duplicate File-Contribution for {_display_path(path)!r}")
        operation = record.get("operation")
        if not _nonempty_string(operation):
            errors.append(f"{label}: operation must be non-empty")
        seen[path] = operation if isinstance(operation, str) else ""

        summary = record.get("summary")
        if not _nonempty_string(summary):
            errors.append(f"{label}: summary must be non-empty")
        elif "TODO" in summary.upper():
            errors.append(f"{label}: summary still contains TODO")
        elif len(summary.strip()) < 8:
            errors.append(f"{label}: summary must contain at least 8 characters")

        by = record.get("by")
        if not isinstance(by, list) or not by:
            errors.append(f"{label}: by must be a non-empty contribution list")
            continue
        local_pairs: set[tuple[str, str]] = set()
        has_generator = False
        for by_index, contribution in enumerate(by, start=1):
            by_label = f"{label}.by[{by_index}]"
            if not isinstance(contribution, dict):
                errors.append(f"{by_label}: contribution must be an object")
                continue
            actor_id = contribution.get("actor")
            role = contribution.get("role")
            if not _nonempty_string(actor_id):
                errors.append(f"{by_label}: actor must be non-empty")
            elif actor_id not in actor_roles:
                errors.append(f"{by_label}: undeclared actor {actor_id!r}")
            if role not in FILE_ROLES:
                errors.append(
                    f"{by_label}: role must be one of {', '.join(sorted(FILE_ROLES))}"
                )
            elif (
                isinstance(actor_id, str)
                and actor_id in actor_roles
                and actor_roles[actor_id] is not None
                and role not in actor_roles[actor_id]
            ):
                errors.append(
                    f"{by_label}: role {role!r} is not declared by actor {actor_id!r}"
                )
            if role == "generator":
                has_generator = True
            if isinstance(actor_id, str) and isinstance(role, str):
                pair = (actor_id, role)
                if pair in local_pairs:
                    errors.append(f"{by_label}: duplicate actor/role pair {pair!r}")
                local_pairs.add(pair)

        generated_by = record.get("generated_by")
        if has_generator and not _nonempty_string(generated_by):
            errors.append(f"{label}: generator role requires generated_by command")
        elif generated_by is not None and not _nonempty_string(generated_by):
            errors.append(f"{label}: generated_by must be a non-empty command")

    missing = sorted(set(expected) - set(seen))
    unexpected = sorted(set(seen) - set(expected))
    if missing:
        errors.append(
            "missing File-Contribution for: "
            + ", ".join(repr(_display_path(path)) for path in missing)
        )
    if unexpected:
        errors.append(
            "unexpected File-Contribution for: "
            + ", ".join(repr(_display_path(path)) for path in unexpected)
        )
    for path in sorted(set(expected) & set(seen)):
        if seen[path] != expected[path]:
            errors.append(
                f"operation mismatch for {_display_path(path)!r}: "
                f"message={seen[path]!r}, git={expected[path]!r}"
            )
    return errors


def _compact_json(value: dict[str, Any]) -> str:
    return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"))


def _slug(value: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")
    return slug or "unknown"


def _path_fields(path: bytes) -> dict[str, str]:
    try:
        return {"path": path.decode("utf-8")}
    except UnicodeDecodeError:
        return {"path_b64": base64.b64encode(path).decode("ascii")}


def scaffold_message(
    changes: Sequence[ChangedFile], ai: dict[str, str] | None = None
) -> str:
    actor_id = "human:author"
    lines = ["<type>(<scope>): <summary>", "", "<why and outcome>", ""]
    if ai is not None:
        actor_id = (
            f"ai:{_slug(ai['provider'])}-{_slug(ai['product'])}-{_slug(ai['agent'])}"
        )
        actor = {
            "id": actor_id,
            "provider": ai["provider"],
            "product": ai["product"],
            "model": ai["model"],
            "agent": ai["agent"],
            "roles": ["author", "tester"],
        }
        lines.append(f"AI-Assisted-By: {_compact_json(actor)}")
    for change in changes:
        record: dict[str, Any] = {
            **_path_fields(change.path),
            "operation": change.operation,
            "by": [{"actor": actor_id, "role": "author"}],
            "summary": "TODO: describe this file change",
        }
        lines.append(f"File-Contribution: {_compact_json(record)}")
    return "\n".join(lines).rstrip() + "\n"


def _load_policy(path: Path) -> dict[str, Any]:
    try:
        policy = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise RuntimeError(f"provenance policy missing: {path}") from exc
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"invalid provenance policy JSON: {exc}") from exc
    if not isinstance(policy, dict) or policy.get("schema_version") != POLICY_VERSION:
        raise RuntimeError(f"provenance policy must use schema_version {POLICY_VERSION}")
    activated_at = policy.get("activated_at")
    if not _nonempty_string(activated_at):
        raise RuntimeError("provenance policy requires activated_at")
    try:
        activation_time = datetime.fromisoformat(activated_at.replace("Z", "+00:00"))
    except ValueError as exc:
        raise RuntimeError("provenance policy activated_at must be ISO-8601") from exc
    if activation_time.utcoffset() is None:
        raise RuntimeError("provenance policy activated_at must include a timezone")
    if policy.get("activation_path") != ACTIVATION_PATH:
        raise RuntimeError(f"provenance policy activation_path must be {ACTIVATION_PATH!r}")
    corrections_path = policy.get("corrections_path")
    if corrections_path is not None and corrections_path != CORRECTIONS_PATH:
        raise RuntimeError(f"provenance policy corrections_path must be {CORRECTIONS_PATH!r}")
    return policy


def _load_role_corrections(repo: Path, policy: dict[str, Any]) -> dict[str, dict[str, Any]]:
    if policy.get("corrections_path") is None:
        return {}
    path = repo / CORRECTIONS_PATH
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise RuntimeError(f"provenance corrections missing: {path}") from exc
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"invalid provenance corrections JSON: {exc}") from exc
    if not isinstance(document, dict) or document.get("schema_version") != 1:
        raise RuntimeError("provenance corrections must use schema_version 1")
    records = document.get("corrections")
    if not isinstance(records, list):
        raise RuntimeError("provenance corrections must contain a corrections array")
    corrections: dict[str, dict[str, Any]] = {}
    for index, record in enumerate(records, start=1):
        label = f"provenance correction[{index}]"
        if not isinstance(record, dict):
            raise RuntimeError(f"{label} must be an object")
        commit = record.get("commit")
        message_sha256 = record.get("message_sha256")
        reason = record.get("reason")
        actors = record.get("actors")
        if not isinstance(commit, str) or not re.fullmatch(r"[0-9a-f]{40}", commit):
            raise RuntimeError(f"{label} requires a full lowercase commit hash")
        if commit in corrections:
            raise RuntimeError(f"duplicate provenance correction for {commit}")
        if not isinstance(message_sha256, str) or not re.fullmatch(
            r"[0-9a-f]{64}", message_sha256
        ):
            raise RuntimeError(f"{label} requires message_sha256")
        if not _nonempty_string(reason):
            raise RuntimeError(f"{label} requires a reason")
        if not isinstance(actors, list) or not actors:
            raise RuntimeError(f"{label} requires actor role additions")
        for actor_index, actor in enumerate(actors, start=1):
            if not isinstance(actor, dict) or not _nonempty_string(actor.get("id")):
                raise RuntimeError(f"{label}.actors[{actor_index}] requires id")
            roles = actor.get("add_roles")
            if not _string_list(roles) or any(role not in FILE_ROLES for role in roles):
                raise RuntimeError(
                    f"{label}.actors[{actor_index}] has invalid add_roles"
                )
        corrections[commit] = record
    return corrections


def _apply_role_correction(
    message: str, commit: str, correction: dict[str, Any]
) -> tuple[str, list[str]]:
    errors: list[str] = []
    actual_digest = hashlib.sha256(message.encode("utf-8")).hexdigest()
    if actual_digest != correction["message_sha256"]:
        return message, [f"provenance correction message hash mismatch for {commit}"]
    additions = {
        actor["id"]: set(actor["add_roles"]) for actor in correction["actors"]
    }
    found: set[str] = set()
    lines = message.splitlines()
    for index, line in enumerate(lines):
        if not line.startswith("AI-Assisted-By:"):
            continue
        try:
            actor = json.loads(line.split(":", 1)[1].strip())
        except json.JSONDecodeError:
            continue
        actor_id = actor.get("id") if isinstance(actor, dict) else None
        if actor_id not in additions:
            continue
        roles = actor.get("roles")
        if not _string_list(roles):
            errors.append(f"provenance correction actor {actor_id!r} has invalid source roles")
            continue
        actor["roles"] = sorted(set(roles) | additions[actor_id])
        lines[index] = f"AI-Assisted-By: {_compact_json(actor)}"
        found.add(actor_id)
    missing = sorted(set(additions) - found)
    if missing:
        errors.append(f"provenance correction actor not found in {commit}: {missing}")
    corrected = "\n".join(lines)
    if message.endswith("\n"):
        corrected += "\n"
    return corrected, errors


def _policy_is_in_history(repo: Path, commit: str, policy: dict[str, Any]) -> bool:
    first_policy_commit = _run_git(
        repo, "rev-list", "-n", "1", commit, "--", ACTIVATION_PATH
    ).strip()
    return bool(first_policy_commit)


def validate_commit(
    commit: str,
    repo: Path,
    policy: dict[str, Any],
    *,
    enforce_all: bool = False,
) -> tuple[str, list[str]]:
    if not enforce_all and not _policy_is_in_history(repo, commit, policy):
        return "pre-policy", []
    resolved = _run_git(repo, "rev-parse", commit).decode("ascii").strip()
    message = _run_git(repo, "show", "-s", "--format=%B", resolved).decode(
        "utf-8", errors="replace"
    )
    correction_errors: list[str] = []
    correction = _load_role_corrections(repo, policy).get(resolved)
    if correction is not None:
        message, correction_errors = _apply_role_correction(message, resolved, correction)
    return "checked", correction_errors + validate_message(message, commit_changes(resolved, repo))


def validate_gitlink_range(
    base: str, head: str, root: Path, policy: dict[str, Any]
) -> tuple[int, list[str]]:
    checked = 0
    errors: list[str] = []
    checked_commits: set[tuple[str, str]] = set()

    def commit_exists(repo: Path, oid: str) -> bool:
        return subprocess.run(
            ["git", "cat-file", "-e", f"{oid}^{{commit}}"],
            cwd=repo,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        ).returncode == 0

    def visit_transition(
        parent_repo: Path,
        parent_display: str,
        entry: RawDiffEntry,
        owner_commit: str,
    ) -> None:
        nonlocal checked
        path = entry.paths[-1]
        local_display = _display_path(path)
        display_path = (
            f"{parent_display}/{local_display}" if parent_display else local_display
        )
        repo = parent_repo / os.fsdecode(path)
        if not repo.exists():
            errors.append(
                f"gitlink {display_path!r} in commit {owner_commit}: "
                "submodule checkout is missing"
            )
            return

        new_oid = entry.new_oid.decode("ascii")
        old_oid = entry.old_oid.decode("ascii")
        if not commit_exists(repo, new_oid):
            errors.append(f"gitlink {display_path!r}: new commit {new_oid} is unavailable")
            return
        if set(old_oid) == {"0"}:
            # Imported history is not rewritten. The newly pinned tip is the
            # boundary, and any gitlink transition in that tip is still walked.
            commits = [new_oid]
        elif not commit_exists(repo, old_oid):
            errors.append(f"gitlink {display_path!r}: old commit {old_oid} is unavailable")
            return
        else:
            commits = [
                item.decode("ascii")
                for item in _run_git(
                    repo, "rev-list", "--reverse", f"{old_oid}..{new_oid}"
                ).splitlines()
            ]

        for commit in commits:
            key = (str(repo.resolve()), commit)
            if key in checked_commits:
                continue
            checked_commits.add(key)
            checked += 1
            status, commit_errors = validate_commit(
                commit, repo, policy, enforce_all=True
            )
            if commit_errors:
                errors.extend(
                    f"gitlink {display_path!r} commit {commit}: {error}"
                    for error in commit_errors
                )
            else:
                print(
                    f"commit provenance: gitlink {display_path} "
                    f"{commit} {status} PASS"
                )

            # A submodule commit can itself introduce nested submodule commits.
            # Walk those transitions recursively so --no-verify cannot hide at
            # a deeper gitlink level.
            for nested_entry in parse_raw_entries(_commit_raw_diff(repo, commit)):
                if nested_entry.new_mode == b"160000":
                    visit_transition(repo, display_path, nested_entry, commit)

    root_commits = _run_git(root, "rev-list", "--reverse", f"{base}..{head}")
    for root_commit_raw in root_commits.splitlines():
        root_commit = root_commit_raw.decode("ascii")
        for entry in parse_raw_entries(_commit_raw_diff(root, root_commit)):
            if entry.new_mode == b"160000":
                visit_transition(root, "", entry, root_commit)
    return checked, errors


def _print_errors(errors: Sequence[str]) -> None:
    print("commit provenance FAILED:", file=sys.stderr)
    for error in errors:
        print(f"  - {error}", file=sys.stderr)
    print(
        "Generate a staged skeleton with: "
        "python3 tools/check_commit_provenance.py scaffold",
        file=sys.stderr,
    )


def _cmd_staged(args: argparse.Namespace) -> int:
    repo = Path(args.repo).resolve()
    message = Path(args.message_file).read_text(encoding="utf-8")
    errors = validate_message(message, staged_changes(repo))
    if errors:
        _print_errors(errors)
        return 1
    print("commit provenance: staged message PASS")
    return 0


def _cmd_commit(args: argparse.Namespace) -> int:
    repo = Path(args.repo).resolve()
    policy = _load_policy(Path(args.policy).resolve())
    status, errors = validate_commit(args.commit, repo, policy)
    if errors:
        _print_errors(errors)
        return 1
    print(f"commit provenance: {args.commit} {status} PASS")
    return 0


def _cmd_range(args: argparse.Namespace) -> int:
    repo = Path(args.repo).resolve()
    policy = _load_policy(Path(args.policy).resolve())
    commits = _run_git(repo, "rev-list", "--reverse", args.revision_range).splitlines()
    failures = 0
    for raw_commit in commits:
        commit = raw_commit.decode("ascii")
        status, errors = validate_commit(
            commit, repo, policy, enforce_all=args.enforce_all
        )
        if errors:
            failures += 1
            print(f"\n{commit}:", file=sys.stderr)
            _print_errors(errors)
        else:
            print(f"commit provenance: {commit} {status} PASS")
    if failures:
        print(f"commit provenance: {failures} commit(s) failed", file=sys.stderr)
        return 1
    print(f"commit provenance: range PASS ({len(commits)} commit(s))")
    return 0


def _cmd_scaffold(args: argparse.Namespace) -> int:
    ai_values = (args.ai_provider, args.ai_product, args.ai_model, args.ai_agent)
    ai: dict[str, str] | None = None
    if any(ai_values):
        if not all(ai_values):
            print(
                "scaffold: --ai-provider, --ai-product, --ai-model, and "
                "--ai-agent must be supplied together",
                file=sys.stderr,
            )
            return 2
        ai = {
            "provider": args.ai_provider,
            "product": args.ai_product,
            "model": args.ai_model,
            "agent": args.ai_agent,
        }
    print(scaffold_message(staged_changes(Path(args.repo).resolve()), ai=ai), end="")
    return 0


def _cmd_gitlinks(args: argparse.Namespace) -> int:
    root = Path(args.repo).resolve()
    policy = _load_policy(Path(args.policy).resolve())
    checked, errors = validate_gitlink_range(args.base, args.head, root, policy)
    if errors:
        _print_errors(errors)
        return 1
    print(f"commit provenance: gitlink range PASS ({checked} commit(s))")
    return 0


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Validate machine-readable contributor provenance in commits."
    )
    parser.add_argument("--repo", default=".", help="Git repository (default: cwd)")
    parser.add_argument(
        "--policy",
        default=".provenance-policy.json",
        help="forward-enforcement policy file",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    staged = subparsers.add_parser("staged", help="check message against index")
    staged.add_argument("message_file")
    staged.set_defaults(handler=_cmd_staged)

    commit = subparsers.add_parser("commit", help="check one commit")
    commit.add_argument("commit")
    commit.set_defaults(handler=_cmd_commit)

    revision_range = subparsers.add_parser("range", help="check a revision range")
    revision_range.add_argument("revision_range")
    revision_range.add_argument(
        "--enforce-all",
        action="store_true",
        help="enforce every commit in range, including branches predating activation",
    )
    revision_range.set_defaults(handler=_cmd_range)

    scaffold = subparsers.add_parser("scaffold", help="emit staged message skeleton")
    scaffold.add_argument("--ai-provider")
    scaffold.add_argument("--ai-product")
    scaffold.add_argument("--ai-model")
    scaffold.add_argument("--ai-agent")
    scaffold.set_defaults(handler=_cmd_scaffold)

    gitlinks = subparsers.add_parser(
        "gitlinks", help="check commits introduced by changed submodule gitlinks"
    )
    gitlinks.add_argument("base")
    gitlinks.add_argument("head")
    gitlinks.set_defaults(handler=_cmd_gitlinks)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        return args.handler(args)
    except (OSError, RuntimeError, ValueError) as exc:
        print(f"commit provenance ERROR: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
