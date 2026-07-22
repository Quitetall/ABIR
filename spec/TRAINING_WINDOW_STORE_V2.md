# ABIR Training Window Store v2

## Scope and compatibility

This specification extends the sealed BCS2 training catalog with typed label
payload associations. It uses schema `org.quitetall.abir.training.snapshot-v2`
and a distinct snapshot hash domain. Version 1 catalogs and identities remain
unchanged and continue to be readable; a v1 reader must reject v2 rather than
silently ignore label semantics.

All Training Window Store v1 identity, row, profile, decision-log, continual,
source-independence, and ownership rules continue to apply unless explicitly
changed below. The six `bcs.training.*.v1` profile IDs remain physical-profile
contracts and may carry either catalog generation.

## Typed label payload associations

The catalog adds the required non-empty `label_payloads` array, sorted by
logical row and namespaced concept. Each association binds one existing logical
row to a typed label payload and an explicit ABIR semantic-v1 presence state.

`Present` requires a payload descriptor. `AbsentAtSource`, `UnknownAtSource`,
`Withheld`, `Redacted`, and `NotApplicable` forbid one. None of those states
implies an empty or all-zero label. Duplicate row/concept associations and
associations to unknown rows fail closed.

Present associated payloads participate in the same exact BCS2 payload closure,
content hashing, extent validation, byte-order validation, and borrowed-lease
rules as primary rows. Consumers must require the exact concept, element, byte
order, shape, and scientific interpretation needed by their objective.

The row's existing `label` ContentId continues to bind the label ontology or
specification; it is not decoded per-sample label data and is not reinterpreted
by v2.

## Identity

A v2 snapshot sorts unique dataset roots, primary rows, and label associations,
then hashes its canonical JSON as:

```text
BLAKE3("org.quitetall.abir.training.snapshot-v2\0" || canonical_json)
```

The v2 schema and this prose are pinned by `spec/training-v2.manifest.json`.
