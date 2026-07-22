# ABIR Training Window Store v1

## Scope

This specification defines the semantic catalog carried by the six
`bcs.training.*.v1` BCS2 profiles. It does not define a second wire format.
Every artifact is a sealed BCS2 `Bundle`; the catalog is the bundle semantic
JSON and row bodies are typed `SemanticPayload` frames.

## Identities

All identifiers are lowercase 64-digit ABIR `ContentId` values. A
`TrainingSpec` is canonicalized with its `allowed_adaptive_knobs` sorted and
deduplicated, then hashed as:

```text
BLAKE3("org.quitetall.abir.training.spec-v1\0" || canonical_json)
```

A `TrainingSnapshot` sorts unique dataset roots and rows by logical row ID,
then hashes its canonical JSON as:

```text
BLAKE3("org.quitetall.abir.training.snapshot-v1\0" || canonical_json)
```

The result is the BCS2 bundle root `ContentId`. Physical layout, source file
format, delivery order, worker count, and storage location do not participate.

## Training input and specification

A training input is either a dataset root plus a complete `TrainingSpec`, or an
already sealed snapshot root. The spec binds cohort, grouping, split, window,
label, feature, view, sampler, augmentation, preprocessing graph, fitted state,
policy, authorized purpose, seed, and the set of permitted adaptive execution
knobs. An empty adaptive-knob set is valid and means execution is static.

## Snapshot catalog

The catalog schema is `org.quitetall.abir.training.snapshot-v1`. It contains:

- one or more unique dataset roots;
- the exact training-spec identity;
- one registered training profile;
- the exact decision-log identity;
- one or more rows sorted by logical row identity;
- `sealed: true`.

Each row binds logical row, group, split, label, payload, element type, byte
order, shape, and logical byte length. Fixed-width shapes must multiply exactly
to the byte length. Multi-byte fixed-width values require explicit little- or
big-endian order; one-byte and non-numeric values use `not-applicable`. Shared
payload IDs are permitted only when element, byte order, and length metadata
agree.

Opening a store recomputes the catalog root, rejects external references,
requires an exact payload-frame closure, verifies every payload ContentId,
element type, and length, and exposes borrowed frame bytes. Extra, missing,
duplicate, malformed, or mismatched frames fail closed.

## Profiles

Stable profile IDs are registered in `registries/bcs2-profiles-v1.json`:

| Profile | ID | Portable |
| --- | ---: | --- |
| `bcs.training.balanced.v1` | `0x00030001` | no |
| `bcs.training.compact.v1` | `0x00030002` | yes |
| `bcs.training.speed.v1` | `0x00030003` | no |
| `bcs.training.memory.v1` | `0x00030004` | no |
| `bcs.training.ultra-compact.v1` | `0x00030005` | yes |
| `bcs.training.stream.v1` | `0x00030006` | no |

Profile names are semantic policy contracts. A compiler must record the exact
physical plan and measurements before making performance claims.

## Adaptive decisions

A decision log is bound to one `TrainingSpec`. Records are consecutive,
rank-zero records with a nondecreasing activation barrier. Every record names a
knob allowed by that spec and asserts it was made durable before activation.
Replay requires the same spec and byte-identical ordered records. A missing,
corrupt, non-durable, out-of-order, disallowed, or worker-local decision fails
closed.

## Continual subscriptions

An open `DatasetSubscription` accepts consecutive micro-snapshots with
nondecreasing watermarks. Corrections must cite the immediately prior snapshot
and generation and create exactly the next generation. Closing replays the
event sequence through the same verifier and assigns a domain-separated content
identity. Open subscriptions are not promotion evidence.

## Source independence and ownership

Equivalent source datasets that resolve to the same logical roots, spec, rows,
and decision log produce the same snapshot identity. Trainers consume only the
store seam and never EDF, BIDS, DICOM, NWB, BCS, LMA, or legacy cache semantics.
Borrowed row bytes remain owned by the BCS2 artifact or store lease for the full
lifetime of every native or language-binding view.
