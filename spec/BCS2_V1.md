# BCS2 Wire and Store Specification, Generation 2.0

Status: normative draft for ADR 0141 conformance.

BCS2 is the canonical serialization grammar for ABIR roots. It separates logical
identity (`ContentId`) from physical identity (`StorageId`) and uses profiles to
constrain one common envelope for archive, codec, training, stream, and forensic
uses.

## Authority

This prose, `registries/bcs2-profiles-v1.json`,
`schema/bcs2-profile-v1.schema.json`, `registries/bcs2-crypto-v1.json`,
`schema/bcs2-crypto-v1.schema.json`, binary-layout fixtures, and
`spec/bcs2-v1.manifest.json` jointly define generation 2. Unknown required
capabilities fail before allocation. Retired identifiers are never reused.
The binary-layout fixtures and their whole-artifact SHA-256 identities are
frozen by `fixtures/bcs2/v1/manifest.json` and must regenerate byte-for-byte.

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

Bundle roots may instead carry a profile-owned canonical catalog. The profile
verifier must recompute the Bundle root `ContentId` from that catalog, while the
BCS2 reader continues to verify every typed frame and its declared identity.

The empty frame index is 48 bytes: bytes 0–7 are `BCS2IDX\0`, bytes 8–11 are
the little-endian frame count, bytes 12–15 are zero, and bytes 16–47 are the
BLAKE3-256 catalog digest. Non-empty indexes append 128-byte entries sorted
strictly by logical object `ContentId`. Entry bytes 0–31 contain `ContentId`,
32–63 contain `StorageId`, 64–71 contain the frame offset, 72–79 contain frame
length, and byte 80 is frame kind: 1 is embedded BCS2, 2 is a raw blob, and 3
is a semantic payload. Byte 81 is zero for kinds 1 and 2; for kind 3 it is the
registered element-type code (`i8` through `bytes`, codes 1 through 15 in
`ElementType` declaration order). Bytes 82–95 are zero, and bytes 96–127
contain the raw BLAKE3-256 frame digest. Frame
payloads occur contiguously between catalog and index in entry order. Readers
verify both digests and recompute kind-specific identities. Embedded BCS2 frames
are parsed and their root `ContentId` must equal the entry. Raw frames use
domain-separated BLAKE3-256 logical and physical identities and may be empty.
Semantic payload frames recompute the ABIR semantic-v1 payload `ContentId` from
the declared element type and logical bytes; their physical identity uses the
raw-frame storage identity. A dataset-with-payloads encoding contains exactly
one typed frame for every sorted unique `AbirDataset::payload_content_ids()`
entry. Missing, extra, relabelled, truncated, or digest-mismatched payloads fail
closed. Stores index these frames by descriptor identity and lend their byte
extent directly through `PayloadAccess` without copying.
An embedded BCS2 artifact may contain raw frames but cannot contain another
embedded BCS2 frame; thus generation-2 verification depth remains bounded.

Profile identifiers reserve their high 16 bits for the family: 1 is LML, 2 is
LMQ, 3 is training, 4 is stream, and 5 is forensic. The low 16 bits identify a
profile within that family. Registry generation 1 is the first registry for BCS
wire major 2; these generation numbers are independent.

The training family defines six policy-selected physical profiles. Every
training profile accepts Dataset and Bundle roots and no other root kind.
Compact profiles are portable and therefore forbid unresolved external
references; the other profiles may retain externally resolved objects.

| Stable ID | Profile | Portable | External references |
|---:|---|:---:|:---:|
| `0x0003_0001` | `bcs.training.balanced.v1` | no | yes |
| `0x0003_0002` | `bcs.training.compact.v1` | yes | no |
| `0x0003_0003` | `bcs.training.speed.v1` | no | yes |
| `0x0003_0004` | `bcs.training.memory.v1` | no | yes |
| `0x0003_0005` | `bcs.training.ultra-compact.v1` | yes | no |
| `0x0003_0006` | `bcs.training.stream.v1` | no | yes |

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

Generation zero places its catalog and index at the envelope-declared extents,
then appends its footer. Each later generation appends catalog, index, and footer
without altering any earlier byte. Publication concludes by updating only the
envelope's latest-footer offset and root `ContentId`; readers treat those fields
as a mutable publication pointer and accept the root only after the referenced
footer and complete backward chain verify. A torn or stale pointer fails closed.
The artifact ends exactly after the latest footer. Readers bound traversal by a
caller-supplied generation limit before following the first footer.

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

A portable physical variant retains the semantic root kind, profile, catalog,
root `ContentId`, and direct reference set of its unpacked root. Every other
object in its transitive closure occurs exactly once as an embedded BCS2 frame;
missing and unreachable extra frames are both invalid. Frame order is canonical,
so input enumeration order cannot affect bytes. Import validates the entire
closure before publishing any logical root.

## Privacy and forensic profiles

EncryptedOpaque reveals only magic, wire generation, algorithm identifiers,
bounded ciphertext extents, and data required to reject unsupported input
safely. Profile, semantic generation, root kind, and root `ContentId` are zero.
EncryptedDiscoverable additionally discloses and authenticates those four inner
fields. Keys and grants are never ABIR payloads.

Generation-2 encryption uses registry algorithm 2, XChaCha20-Poly1305-IETF, with
a 24-byte nonce, 16-byte tag, and required capability bit zero. The outer
artifact is `SealedImmutable`; byte 43 is 2. Envelope field 44 is the exact
ciphertext-plus-tag length, fields 48 and 52 are nonce and tag lengths, fields
56 and 64 locate ciphertext, and fields 72 and 80 locate the nonce. The nonce is
bytes 128–151 and ciphertext begins at byte 152. The complete 128-byte envelope
is AEAD associated data. The decrypted plaintext is itself a fully verified BCS2
artifact; discoverable fields must equal its values. Callers must never reuse a
nonce with the same key. Nonce variation changes `StorageId` but never inner
`ContentId`.

ForensicTree records observable path, type, mode, ownership, timestamps, ACLs,
xattrs, links, sparse extents, flags, and special-node declarations.
ForensicImage carries an exact image payload. Unsafe materialization requires a
sandboxed restore operation. Exact restore fails if required metadata cannot be
reproduced.

A forensic tree is a Bundle-root artifact under `bcs.forensic.tree.v1`. Its
canonical semantic JSON identifies a single raw metadata frame, entry count,
and metadata generation. The metadata frame is deterministic CBOR: a fixed
three-element array containing version 1, a bounded platform identifier, and a
path-sorted entry array. Every entry is a fixed 15-element array containing, in
order, relative path bytes, node type, mode, optional uid/gid, four optional
nanosecond timestamps (access, modification, status-change, birth), optional
ACL bytes, sorted xattrs, optional hardlink and symlink targets, complete sparse
extent map, flags, optional device numbers, optional unknown-node declaration,
optional raw payload ContentId, and optional payload length. Indefinite CBOR,
non-minimal integer encodings, duplicate paths or xattrs, unsafe relative paths,
inconsistent hardlink metadata, incomplete sparse maps, and fields forbidden by
the declared node type are noncanonical.

Regular-file bytes are kind-2 raw frames. Equal file contents are stored once;
the metadata refers to the same ContentId from every path. The Bundle root
ContentId domain-separates and hashes the metadata-frame ContentId, so all
observable metadata and file identities contribute to logical identity. A
reader exposes file frames as borrowed slices. Tree artifacts contain exactly
the metadata and referenced file frames: missing or unreachable extra frames
fail closed.

Restore is a separate, explicit operation whose destination must already be an
empty real directory. Paths and link targets are preflighted before the first
write. Portable restore reports every intentionally omitted attribute. Exact
restore first rejects platform mismatch, unsupported metadata, unsafe links,
and unsupported node types; it never silently weakens an exact request.

A forensic image or exact source-file payload is a Blob-root BCS2 artifact under
`bcs.forensic.image.v1`. Its canonical semantic JSON records raw content ID,
byte length, and a restricted ASCII media type; its single kind-2 frame carries
the exact bytes zero-copy. The Blob root `ContentId` domain-separates and hashes
media type, length, and raw content ID, preventing metadata relabeling. Empty
files remain representable as zero-length raw frames.

## Canonicality and failure

Unencrypted sealed artifacts are byte deterministic for identical semantic
input, profile, and payload bytes. Readers reject overflow, overlap, truncation,
duplicate index keys, digest mismatch, unsupported required capability, profile
violation, resource-limit excess, broken generation chain, incomplete portable
closure, and privacy declaration mismatch before exposing a root.
