# ABIR spec

Normative prose for versioned ABIR semantics. Machine-readable artifacts live
in the sibling `schema/`, `registries/`, and `fixtures/` directories.

BCS2 wire and storage semantics are specified independently by `BCS2_V1.md`,
the BCS2 profile registry and schema, conformance fixtures, and
`bcs2-v1.manifest.json`.

Training Window Store generations are additive. Version 3 embeds complete
`TrainingSpec` authority and is pinned by `training-v3.manifest.json`; versions
1 and 2 remain readable compatibility generations.
