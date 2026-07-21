# Contributing to ABIR

Use AGPL-3.0-or-later terms.

## Commit discipline

- Keep changes scoped to this repository.
- Ensure commit messages and history include provenance where enforced by this
  repository policy (`.provenance-policy.json`).
- Per-file roles are `author`, `editor`, `formatter`, `generator`, `tester`,
  `integrator`, and `conflict-resolver`. `tester` records test or evidence work
  materially performed on that path; generated artifacts still require an
  exact `generated_by` command.
- Validation command required before merge:
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets --all-features`
  - `cargo test --workspace`
  - `cargo check -p abir --no-default-features`
  - `python3 tools/check_commit_provenance.py --help`

## Project structure

- `crates/abir`: core Rust crate.
- `crates/abir-conformance`: conformance crate.
- `crates/abir-python`: PyO3 FFI crate.
- `spec`: normative prose and compatibility policy.
- `schema`: machine-readable schema.
- `registries`: stable semantic registries.
- `fixtures`: cross-language conformance fixtures.
- `evidence`: generated gate and performance evidence.
