# Invalid conformance corpus

- `schema/` contains documents that must fail the frozen JSON Schema.
- Contextual failures such as duplicate IDs, dangling references, cycles,
  policy relaxation, proof misuse, and payload mismatch are constructed in the
  Rust conformance tests because JSON Schema cannot decide them.
