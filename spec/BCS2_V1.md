# BCS2 Wire and Store Specification, Generation 2.0

Status: normative draft for ADR 0141 conformance.

BCS2 is the canonical serialization grammar for ABIR roots. It separates logical
identity (`ContentId`) from physical identity (`StorageId`) and uses profiles to
constrain one common envelope for archive, codec, training, stream, and forensic
uses.

## Authority

This prose, `registries/bcs2-profiles-v1.json`,
`schema/bcs2-profile-v1.schema.json`, binary-layout fixtures, and
`spec/bcs2-v1.manifest.json` jointly define generation 2. Unknown required
capabilities fail before allocation. Retired identifiers are never reused.

## Envelope

Every artifact begins with one 128-byte little-endian envelope:

| Offset | Width | Field |
|---:|---:|---|
| 0 | 8 | magic `ABIRBCS2` |
| 8 | 2 | major = 2 |
| 10 | 2 | minor |
| 12 | 4 | header bytes = 128 |
| 16 | 4 | profile identifier |
| 20 | 4 | ABIR semantic generation |
| 24 | 8 | required capability bitmap |
| 32 | 8 | optional capability bitmap |
| 40 | 1 | root kind |
| 41 | 1 | storage contract |
| 42 | 1 | privacy mode |
| 43 | 1 | integrity algorithm |
| 44 | 4 | maximum catalog bytes |
| 48 | 4 | maximum index entries |
| 52 | 4 | maximum frame bytes |
| 56 | 8 | catalog offset |
| 64 | 8 | catalog length |
| 72 | 8 | frame-index offset |
| 80 | 8 | frame-index length |
| 88 | 8 | latest generation-footer offset, or zero |
| 96 | 32 | logical root `ContentId` |

The only legal root kinds are Dataset (1), Recording (2), Stream (3), Atom (4),
Blob (5), and Bundle (6).
The only legal storage contracts are SealedImmutable, SealedGenerational,
UnsealedWorkspace, and RewriteCompact. Privacy is Plaintext,
EncryptedOpaque, or EncryptedDiscoverable. Generation 2 requires BLAKE3-256
integrity; new algorithms require a registered additive capability.

Offsets and lengths must be canonical, ordered, non-overlapping, within the
artifact, and within declared bounds. Reserved bits and values must be zero.

## Catalog and frames

Cold catalog, lineage, policy, and profile metadata use deterministic CBOR with
registered integer keys. Hot payloads use fixed frames and indexes. Each frame
declares kind, flags, logical object identifier, payload `ContentId`, byte
length, and payload digest. Padding bytes are zero and are excluded from logical
identity.

The generation-2 base catalog is a canonical three-entry CBOR map. Key 1 is the
RFC 8785 ABIR semantic debug projection as a byte string and key 2 is the
32-byte logical root `ContentId`. Key 3 is the sorted, duplicate-free array of
32-byte reachable-object `ContentId` references. Keys occur in ascending order.
This preserves one independently inspectable semantic projection while later
registered keys may add native catalog tables. Closure references are catalog
bytes and therefore contribute to `StorageId`; stores never accept out-of-band
reachability claims.

The empty frame index is 48 bytes: bytes 0–7 are `BCS2IDX\0`, bytes 8–11 are
the little-endian frame count, bytes 12–15 are zero, and bytes 16–47 are the
BLAKE3-256 catalog digest. Non-empty indexes append registered fixed-width frame
entries and set the frame count accordingly.

Profile identifiers reserve their high 16 bits for the family: 1 is LML, 2 is
LMQ, 3 is training, 4 is stream, and 5 is forensic. The low 16 bits identify a
profile within that family. Registry generation 1 is the first registry for BCS
wire major 2; these generation numbers are independent.

The catalog contains the canonical ABIR semantic projection and payload
descriptors. Physical handles, `StorageId`, observed execution, authorization
ledger state, encryption nonce, and storage location never affect `ContentId`.

## Generations

A generation footer is immutable and hash chained. It declares generation
number, previous-footer offset and digest, catalog/index locations, root
`ContentId`, and generation digest. `SealedImmutable` forbids a predecessor and
successor. `SealedGenerational` appends only. `UnsealedWorkspace` cannot claim
exact/audited sealing. `RewriteCompact` creates a new `StorageId` while retaining
the same logical root.

An external pin or signature is required to prove which footer was latest;
internal chaining alone cannot disprove tail truncation.

Generation footer is 160 bytes. Offsets 0–7 contain `BCS2GEN\0`; 8–11 contain
wire major/minor; 12–15 contain footer length; 16–23 generation number; 24–31
previous-footer offset; 32–63 previous-footer digest; 64–79 catalog offset and
length; 80–95 index offset and length; 96–127 root `ContentId`; and 128–159
generation digest. Digest is domain-separated BLAKE3-256 over first 128 footer
bytes followed by referenced catalog and index bytes. Generation zero requires
zero previous offset/digest. Later generations require both and decrement
without gaps while traversing backward. Verifiers take an external latest
offset and maximum generation count.

## Store and closure

`AbirStore` indexes loose objects and packs by both IDs, leases payloads, and
computes reachability from immutable roots. Garbage collection may delete only
unleased objects unreachable from every pinned root or generation. Portable
profiles include the full reachable closure unless their registry entry
explicitly permits external references.

## Privacy and forensic profiles

EncryptedOpaque reveals only magic, generation, algorithm identifiers, bounded
ciphertext extents, and data required to reject unsupported input safely.
EncryptedDiscoverable is explicit. Keys and grants are never ABIR payloads.

ForensicTree records observable path, type, mode, ownership, timestamps, ACLs,
xattrs, links, sparse extents, flags, and special-node declarations.
ForensicImage carries an exact image payload. Unsafe materialization requires a
sandboxed restore operation. Exact restore fails if required metadata cannot be
reproduced.

## Canonicality and failure

Unencrypted sealed artifacts are byte deterministic for identical semantic
input, profile, and payload bytes. Readers reject overflow, overlap, truncation,
duplicate index keys, digest mismatch, unsupported required capability, profile
violation, resource-limit excess, broken generation chain, incomplete portable
closure, and privacy declaration mismatch before exposing a root.
