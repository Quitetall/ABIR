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

`abir-biosignal` embeds its exact 40-character implementation revision. Clean
Git checkouts derive it from `HEAD`; dirty checkouts fail closed. Cargo Git
checkouts may contain Cargo's root `.cargo-ok` sentinel, which is not source.
`ABIR_DEVELOPMENT_BUILD=1` permits local dirty-tree builds but embeds forty
zeroes, never a claim to a reviewed revision. Source archives cannot claim an
implementation revision until an authenticated provenance mechanism exists.
The Python build script deliberately watches one absent sentinel, forcing Git
status revalidation even when Cargo would otherwise reuse an incremental build.

## Contributing

See `CONTRIBUTING.md`.

## License

AGPL-3.0-or-later.
