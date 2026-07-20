# ABIR Semantic Model v1

Status: **frozen candidate**

This document defines the logical semantics of ABIR v1. Storage containers,
chunking, compression, indexing, and transport are outside this version and
must not change logical identity.

## 1. Construction and validity

An ABIR document is constructed as a `DatasetDraft`. Validation applies a
caller-selected `ValidationLimits` profile and either returns an immutable
`AbirDataset` or a non-empty `ValidationReport`. A draft is never a valid
dataset and cannot produce semantic views.

Validation is fail-closed. It rejects duplicate or dangling identifiers,
malformed shapes and extents, invalid exact numbers or calibration, unresolved
clock and coordinate references, proof misuse, policy relaxation, non-finite
metadata, excessive nesting, and payload descriptor mismatches.

## 2. Identity

- `ObjectId<T>` is a typed 128-bit semantic identifier. Its bytes are stable
  within a logical dataset; the Rust type parameter prevents category mixing.
- `ContentId` is a domain-separated BLAKE3-256 digest of canonical logical
  content.
- `StorageId` identifies physical storage and is reserved for ADR 0141. It is
  never part of v1 logical identity.
- `Handle<T>` is a generation-local `u32` lookup key. It is never serialized or
  hashed.
- `SourceKey` preserves a foreign identifier without granting it ABIR identity.

## 3. Dataset root and catalog

`AbirDataset` is the first-class root. It owns immutable semantic catalogs and
payload `ContentId` references, never physical buffers. A dataset contains zero
or more recordings. Recordings contain streams; streams contain ordered atom
references and carry clock, channel-basis, modality, and policy semantics.

Catalog references are explicit. Unknown registered concepts are preserved as
opaque namespaced concept identifiers and are not silently coerced.

## 4. Atomic data forms

The atom algebra is sealed for semantic-v1:

1. `SignalBlock`: dense, ragged, sparse, or block-floating-point signal data.
2. `TemporalTable`: rows with explicit temporal anchors or intervals.
3. `Table`: typed non-temporal columnar data.
4. `Tensor`: labelled or unlabelled n-dimensional data.
5. `EncodedBlock`: bytes governed by a declared codec and decoded semantics.
6. `BlobRef`: opaque external or embedded content with media type and digest.

Every payload-bearing atom uses a `PayloadDescriptor` that declares content
identity, logical byte length, element type, byte order, shape/layout, and
encoding. Physical location and buffers are supplied only by `PayloadAccess`.

## 5. Time and presence

Time is exact. Rates and time values are reduced signed rational numbers with a
positive denominator. A stream may contain multiple rates and discontinuous
segments. A `TimeAxis` is either regular, explicit timestamps, or piecewise
regular. Clock identity and uncertainty are mandatory when absolute alignment
is claimed.

Presence is explicit: `Present`, `Missing`, `Unknown`, `Redacted`, or
`NotApplicable`. Missingness never implies a numeric zero or empty payload.

## 6. Calibration, coordinates, and channel bases

Calibration is an exact affine transform from stored values to declared units.
Coordinate frames form an acyclic parent graph with explicit transforms and
uncertainty. A channel basis declares the meaning of each channel, including
reference or differential construction. Common-mode information may be placed
in a separate channel or stream but must not be silently discarded.

## 7. Provenance, fidelity, policy, and proofs

Derivations identify input semantic objects and a declared operation. An
execution record may describe observed software, hardware, and timing, but
observed execution is excluded from logical `ContentId` unless explicitly
promoted to semantic parameters.

Fidelity statements declare whether content is exact, quantitatively bounded,
or transformed with known loss. Policy is inherited monotonically: a child may
add restrictions but cannot relax an ancestor. Authorization decisions and
current authorization-ledger state are observations, not logical content.

Proof records are typed claims over identified semantic objects. Unknown proof
kinds are preserved but never accepted as satisfying a known proof requirement.

## 8. Views and payload access

`RecordingView`, `StreamView`, `BlockView`, and `TensorView` borrow verified
dataset identity. `OpenedDataset<A>` combines an `AbirDataset` with a
`PayloadAccess` implementation. Payload resolution returns a lease whose
lifetime is bounded by the accessor. Borrowed and host in-memory accessors must
preserve pointer identity when the declared layout already matches the view.

## 9. Canonical debug form and logical hashing

The canonical debug form is RFC 8785 JSON with tagged exact numerics. Binary
identifiers are lower-case hexadecimal. Catalogs and maps are serialized in
lexicographic key order; semantically unordered collections are sorted by
canonical identity before serialization.

Logical hashing uses domain `org.quitetall.abir.semantic-v1\0` followed by the
canonical JSON bytes. Storage handles, `StorageId`, physical layout/location,
observed execution, and current authorization-ledger state are excluded.

## 10. Structured failures

Every failure has a stable registry code, severity, semantic path, and optional
related object identity. Implementations may add diagnostics but must not
reinterpret a registered code. Structural-limit failures are always errors.

## 11. Extension rule

Readers preserve unknown namespaced concepts and metadata. They must reject an
unknown atom kind, exact-number tag, proof used as a known authorization, or any
extension that changes a required semantic-v1 invariant.
