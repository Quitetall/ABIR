# ABIR Training Acceptance v1

## Scope

This specification defines verifier-produced evidence for the acceptance
claims in ADR 0144. It extends, but does not alter, Training Window Store v1 or
v2 catalogs, identities, profiles, or BCS2 payloads.

## Durable decision replay

A sealed decision log can be reopened only from its exact canonical JSON.
Reopening proves structural validity, not replay. A `DecisionReplayReceipt` is
created only after the caller supplies a `TrainingSpec` and an ordered replay:

- the spec identity must equal the log's bound spec;
- every decision must remain permitted by the spec;
- rank, durability, sequence, and activation-barrier rules are rechecked; and
- the replayed records must equal the durable log records.

The receipt binds the spec, decision-log identity, and record count. Receipt
types have no public deserialization constructor; a serialized receipt is
evidence output, never trusted proof input.

## Source-equivalent windows

Two validated `TrainingWindowStore` artifacts are source-equivalent when they
bind the same training spec, profile, decision log, ordered logical rows, typed
label associations, payload identities, and exact payload bytes. Dataset roots
and snapshot identities may differ because source lineage is intentionally
preserved.

A source-equivalence receipt binds both snapshot identities, separate digests
and counts for both dataset-root sets, and one digest of the common logical
window set. Comparing one artifact with itself is valid but is not sufficient
acceptance evidence for independent ingest paths.

## Continual promotion

The v1 `DatasetSubscription` event identity is unchanged. A
`ContinualPromotion` is a separate verifier-produced attestation over a closed,
non-empty subscription. For every event, in sequence order, promotion requires:

- the exact sealed snapshot named by the event;
- a snapshot spec matching the promotion spec;
- the exact durable decision log named by the snapshot; and
- a verifier-produced replay receipt matching that spec and decision log.

Missing, extra, reordered, mismatched, open, or empty inputs fail closed. Each
promotion entry binds sequence, watermark, logical generation, snapshot,
decision log, and replay-receipt identity. This makes the promotion identity a
complete ordered input for downstream model/PCCP evidence without changing any
already-consumed micro-snapshot.

## Identity domains

Verifier outputs use domain-separated BLAKE3 over canonical JSON:

```text
org.quitetall.abir.training.decision-replay-receipt-v1\0
org.quitetall.abir.training.source-equivalence-receipt-v1\0
org.quitetall.abir.training.continual-promotion-v1\0
org.quitetall.abir.training.dataset-root-set-v1\0
org.quitetall.abir.training.logical-window-set-v1\0
```

Performance measurements, signatures, authorization, and PCCP policy remain
integration-layer evidence. These semantic receipts bind their exact artifacts
but do not claim that an unmeasured benchmark or unsigned attestation passed.
