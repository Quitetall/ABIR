# ABIR

ABIR is the Atomic Biosignal Intermediate Representation: a portable semantic
core for source-independent biosignal ingest, processing, and output.

This repository is implementing ADR 0140. Until the semantic-v1 manifest is
frozen and its conformance gate passes, the API and normative artifacts are
pre-release.

## Crates

- `abir`: core crate (`no_std` + `alloc` with default `std` feature)
- `abir-conformance`: conformance crate depending on `abir`
- `abir-python`: Python bindings crate (`cdylib`, `PyO3`, `abi3-py310`) for distribution `abir-biosignal`

Normative material is split across `spec/`, `schema/`, `registries/`, and
`fixtures/`. Generated validation evidence belongs in `evidence/`.

## Requirements

- Rust 1.81 (workspace MSRV)
- Python 3.10+

## Build

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features`
- `cargo test --workspace`
- `cargo check -p abir --no-default-features`

## Contributing

See `CONTRIBUTING.md`.

## License

AGPL-3.0-or-later.
