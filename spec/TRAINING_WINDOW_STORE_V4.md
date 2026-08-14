# ABIR Training Window Store v4

## Scope and compatibility

Version 4 is a versioned extension of the v3 catalog and adds replay-ready
semantics. It reuses all v3 field semantics for `dataset_roots`, `rows`,
optional `label_payloads`, `spec`, and `spec_id`. It keeps `label_payloads`
semantics from v2, including non-empty optional arrays and typed concept
payload constraints. A v4 catalog is not valid as v3 because its replay
authority is required and its schema and hash domain are distinct.

Version 1 and version 2 catalogs remain readable and valid. Version 3 remains readable but is no longer sufficient for applications that require deterministic replay and distributed epoch reproducibility guarantees.

Version 4 extends v3 with embedded replay evidence:

- `program`: complete `TrainingProgram` closure matching the embedded `spec`.
- `program.execution_decisions`: closed, typed execution-only values referenced by the decision log.
- `program.stochastic_nodes`: sorted, unique Node identities authorized for
  stochastic seed derivation.
- `decision_log`: complete `DecisionLog` records matching `spec_id`.
- `decision_log_id`: still required and must equal `decision_log.content_id()`.

A v3 catalog may carry any of the same `bcs.training.*.v1` profile IDs and existing dataset conventions. A v4 catalog uses profile-specific read requirements and rejects incompatible profiles exactly as existing profile bindings define.

## Canonicalization and identity

A v4 snapshot is a canonical JSON document with:

- sorted unique dataset roots,
- sorted unique logical rows,
- sorted label associations,
- sorted semantic descriptors,
- closed schema constants,
- sealed flags set to `true`.

Canonicalization uses ABIR's restricted RFC 8785-compatible JSON domain:
lexicographically ordered object keys, UTF-8 strings, exact signed or unsigned
64-bit integers, booleans, null, arrays, and objects. Floating-point numbers
are rejected.

The snapshot hash domain becomes:

```text
BLAKE3("org.quitetall.abir.training.snapshot-v4\0" || canonical_json)
```

## Fixed content domains

Replay semantics bind eleven fixed semantic domains from `TrainingSemanticRole` and several fixed executable/runtime domains in `identity.rs`:

- semantic domains:
  - `org.quitetall.abir.training.semantic.augmentation-v1`
  - `org.quitetall.abir.training.semantic.cohort-v1`
  - `org.quitetall.abir.training.semantic.feature-v1`
  - `org.quitetall.abir.training.semantic.fitted-state-v1`
  - `org.quitetall.abir.training.semantic.grouping-v1`
  - `org.quitetall.abir.training.semantic.label-v1`
  - `org.quitetall.abir.training.semantic.policy-v1`
  - `org.quitetall.abir.training.semantic.preprocessing-v1`
  - `org.quitetall.abir.training.semantic.split-v1`
  - `org.quitetall.abir.training.semantic.view-v1`
  - `org.quitetall.abir.training.semantic.window-v1`

- artifact identity domain:
  - `org.quitetall.abir.training.artifact-v1`

- executable/runtime domains:
  - `org.quitetall.abir.training.program-v1`
  - `org.quitetall.abir.training.execution-decision-v1`
  - `org.quitetall.abir.training.sampler-v1`
  - `org.quitetall.abir.training.schedule-v1`
  - `org.quitetall.abir.training.epoch-v1`
  - `org.quitetall.abir.training.order-v1`
  - `org.quitetall.abir.training.stochastic-seed-v1`

The registry file `registries/training-content-domains-v1.json` must enumerate these fixed domains with generation `1`.

`artifact-v1` seals canonical logical artifact bytes supplied by a typed owner.
Physical locations, modification times, transfer framing, and integrity
checksums are excluded. This lets execution systems carry ABIR `ContentId`
without redefining semantic identity or mistaking storage integrity for meaning.
It is not a `TrainingSemanticRole` and does not add a twelfth program descriptor.
Incremental sealing is equivalent to sealing concatenated canonical bytes;
caller-selected chunk boundaries never enter identity.

## Embedded decision log and program

A v4 program must be sealed and contain exactly eleven role-tagged descriptors,
one for each role. Descriptor values use ABIR's restricted canonical JSON
domain: null, booleans, strings, signed or unsigned 64-bit integers, arrays,
and lexicographically keyed objects. Floating-point JSON numbers are rejected.
Runtime validation caps nesting at `ValidationLimits::max_nesting_depth` and
canonical bytes at `ResourceBounds::max_catalog_bytes`. Role-to-domain mapping
is fixed by `TrainingSemanticRole`; callers cannot select or mint domains. Any
descriptor missing or mismatched to the embedded spec is invalid.

A v4 decision log must be sealed and all records must be canonical
`DecisionRecord` objects (`activation_barrier`, `decision`,
`durable_before_activation: true`, `knob`, `rank: 0`, `sequence`). Each
`decision` ContentId must resolve to exactly one typed value in
`program.execution_decisions`; the record knob must equal the typed value knob.
Unreferenced decision values and unresolved records are invalid. A reader must
verify that `decision_log.spec_id` resolves to the embedded `spec_id` and that
replay integrity checks pass.

Generation 1 permits only execution-equivalent typed decisions: row grouping,
prefetch depth, payload access policy, cache budget, and portable-closure
policy. These compile into `PlanOverrides`; they cannot alter rows, sampling,
preprocessing, augmentation, stochastic seeds, or targets. Non-empty logs
require an explicit activation barrier. Replay applies the ordered prefix whose
barrier is less than or equal to that value, so decisions are never activated
early. Empty logs represent the static canonical plan.

Epoch execution compiles the rank-projected scientific schedule and physical
plan through one typed operation against the same embedded decision log and
activation barrier. The physical plan may change delivery only; it cannot
substitute rows or stochastic seeds.

## Replay-ready semantics

Replay-ready state requires both `program` and `decision_log` to be embedded. `decision_log_id` alone is insufficient; consumers must treat only snapshots with embedded replay evidence as replay-ready.

The snapshot is replay-ready if all of these are true:

- `decision_log` is present and valid,
- `program` is present and valid,
- `program`/`spec` closure is total,
- `decision_log` validates for embedded `spec`,
- `decision_log_id` matches embedded decision-log identity,
- every decision record resolves to a typed execution decision and compiles at its declared barrier,
- every stored row and optional label payload participates in validated frame closure under the active profile.

## Distributed epoch invariance

v4 snapshot semantics require distributed epoch invariance for the compiled schedule under fixed seed/program rows: a fixed row set and fixed embedded closure produce the same schedule identity across all worker ranks in one logical epoch. That invariant is violated by mutable or missing replay evidence, so replay-ready v4 snapshots are required where distributed invariance is part of scientific evidence.

Readers and producers keep row closure deterministic by validating compiled schedule ordering and stable seed inputs.
Every per-row seed binds the run seed, epoch, sampler, augmentation authority,
declared stochastic Node identity, logical row, and repeated-draw occurrence.
Changing worker count or delivery order cannot change it; changing the
stochastic Node identity must change it without changing the global schedule.

## Resource bounds

All catalog and closure fields are subject to existing ABIR catalog bounds (`ResourceBounds`) in the same way as prior versions. The decision-log and program objects are part of the catalog bytes and therefore participate in:

- catalog byte caps,
- frame closure checks,
- signature/identity validation,
- row/payload expected-ID derivation.

Missing or oversized catalog fields fail catalog validation before opening frame payloads.

## Conformance fixtures

Normative vectors live under `fixtures/training/v4/`:

- `valid-snapshot.json` is canonical output from the typed Rust sealer.
- `valid-snapshot.content-id` is its snapshot-v4 logical identity.
- `invalid-vectors.json` describes fail-closed JSON Pointer mutations against
  the positive vector.

Regenerate the positive vector by running:

```text
ABIR_DEVELOPMENT_BUILD=1 cargo run -p abir-training --example generate_training_v4_fixture
```

Rust tests prove typed-sealer byte and identity parity. Python tests validate
the same positive catalog and every declared negative mutation against the
frozen JSON Schema. Artifact SHA-256 values are pinned in
`spec/training-v4.manifest.json`.
