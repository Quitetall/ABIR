# ABIR Training Window Store v3

## Scope and compatibility

Version 3 embeds the complete immutable `TrainingSpec` in each sealed training
snapshot. It uses schema `org.quitetall.abir.training.snapshot-v3` and a
distinct snapshot hash domain. Version 1 and version 2 catalogs and identities
remain unchanged and readable, but they do not provide resolved-spec
semantics. Production consumers that require preprocessing, fitted-state,
view, policy, or other `TrainingSpec` fields must reject v1/v2 rather than
accept those identities from an independent caller assertion.

Version 3 may carry zero or more typed label payload associations. Label
association semantics remain exactly those defined by Training Window Store
v2. The six `bcs.training.*.v1` profile IDs remain physical-profile contracts
and may carry any catalog generation supported by the reader.

Primary rows may carry the existing optional `encoding` descriptor that binds
stored payload identity, element type, byte length, and required capability
bits separately from logical row identity and extent. Readers lacking every
required capability reject the artifact before lending payload bytes.

## Embedded semantic authority

The catalog contains both:

- `spec`: complete canonical `TrainingSpec`;
- `spec_id`: `BLAKE3("org.quitetall.abir.training.spec-v1\0" ||
  canonical_json(spec))`.

Writers derive `spec_id` from `spec`. Readers recompute it and reject any
mismatch before exposing rows or semantic fields. Consequently,
`preprocessing`, `fitted_state`, `view`, `feature`, `policy`, `split`, and all
other spec members cannot be substituted independently of snapshot identity.
Writers sort and deduplicate `allowed_adaptive_knobs` before sealing; readers
reject a noncanonical embedded list.

Bindings may repeat resolved spec fields for dispatch, cache identity, or
attestation, but consumers must compare each repeated value to the embedded
spec before use.

## Identity

A v3 snapshot sorts unique dataset roots, primary rows, and label associations,
then hashes its canonical JSON as:

```text
BLAKE3("org.quitetall.abir.training.snapshot-v3\0" || canonical_json)
```

Schema and prose are pinned by `spec/training-v3.manifest.json`.
