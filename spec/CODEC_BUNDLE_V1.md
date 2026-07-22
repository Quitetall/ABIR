# BCS2 Codec Bundle Catalog, Generation 1

This specification defines the profile-owned Bundle catalog used by
`bcs.lml.lossless.v1` and `bcs.lmq.progressive.v1`. It defines no codec
algorithm and changes no LML or LMQ packet grammar.

## Identity and closure

The canonical catalog is RFC 8785 JSON in ABIR's integer-and-tagged-value
subset and has schema name `org.quitetall.abir.bcs2.codec-bundle-v1`. Its root
identity is:

```
BLAKE3-256("org.quitetall.abir.bcs2.codec-bundle-v1\0" || canonical_catalog)
```

The encoder computes this identity; an API accepting a caller-selected Bundle
root is non-conforming. The BCS2 envelope and catalog must carry the computed
identity. The catalog names exactly one canonical ABIR semantic frame and one
or more ordered codec packet frames. Every listed frame is a BCS2 raw frame,
every frame has a unique raw `ContentId`, and no unlisted frame or external
reference is permitted. BCS2 keeps physical frames sorted by `ContentId`; the
catalog's contiguous zero-based ordinals define codec packet order.

Opening a bundle verifies BCS2 extents and digests, recomputes the catalog root,
checks exact frame closure and lengths, parses the semantic frame through the
typed ABIR semantic-v1 verifier, requires canonical re-encoding to be byte
identical, and recomputes both the source semantic and interchange ContentIds.
Missing, extra, duplicate, reordered physical, corrupt, or identity-incompatible
content fails closed.

## Exact codec contract

The catalog binds lexically sorted unique parameters using tagged exact
booleans, byte strings, integers, rationals, or text; floating-point JSON values
are forbidden. It also binds an implementation ContentId, kernel ID, build ID,
and an immutable fidelity contract ContentId.

LML requires exact fidelity and forbids model provenance. LMQ requires bounded
or transformed fidelity and a complete model binding: checkpoint ContentId,
checkpoint SHA-256, PCCP change ID, PCCP evidence ContentId, and captured status.
The immutable status vocabulary is `candidate`, `gate-pass`, or `rejected`.
Production promotion is deliberately not representable in this catalog because
current authorization-ledger state is external and mutable; a consumer must
resolve PCCP evidence and the current promotion registry before production use.

## Profile root rule

The two profiles retain their legacy Dataset, Recording, and (for LMQ) Stream
root permissions. A Bundle root is conforming only when this catalog and its
typed verifier establish the unambiguous closure above.
