# ABIR Adapter protocol v1

An Adapter implements `inspect`, `import`, `plan_export`, `export`, and
`validate`. Imports return a validated `AbirDataset`, a mapping report, exact
payload objects for BCS2 storage, quarantined meaning, and explicit semantic
coverage. Exports require a deterministic preflight plan and return a fidelity
receipt. Unsupported required meaning fails before output mutation.

`ForensicOnly` means the source bytes are preserved exactly but their domain
semantics are not yet first-class. It must never be relabeled as semantic
interchange. Independent-validator success and ABIR semantic completeness are
recorded separately.

Process plugins use ADR 0142's signed manifest and versioned request protocol;
native Rust dynamic-library ABI loading is outside this contract.

## Profile claim boundary

A standard edition and a semantic conformance profile are distinct identifiers.
Broad edition profiles remain `forensic` until every meaning named by that
profile is mapped. A narrower `semantic` profile may be released when its name,
mapping document, fixtures, and validator evidence state the supported subset
without ambiguity. Importers must reject inputs outside that subset instead of
silently falling back to a weaker mapping.

The normative mapping report records both promoted and quarantined source paths.
`exact-semantic` means every meaning declared by the selected profile is mapped;
it does not promote quarantined extensions or broaden the profile after import.

## Independent-validator receipts

An Adapter validation receipt is a deterministic, per-fixture evidence record
conforming to `schema/adapter-validation-v1.schema.json`. It binds all of the
following inputs and observations rather than recording a free-form validator
name or an unaudited Boolean:

- the Adapter profile identifier and the standard edition registered for it;
- the exact Adapter source revision (a 40- or 64-hex revision identifier);
- the repository-relative fixture path, SHA-256 digest, and expected `accept`
  or `reject` outcome;
- the independent validator name and version, executable SHA-256 digest, and
  the SHA-256 digest of the schema, dictionary, namespace, or equivalent
  conformance authority used for that execution;
- the exact argument vector and UTC execution time;
- the process exit code, SHA-256 digests of captured stdout and stderr, and
  structured error and warning counts; and
- the observed outcome, whether it matched the declared fixture expectation,
  and whether the tool has `parser-only` or `conformance` authority.

The receipt's `pass` value is derived, not discretionary. It is true only when
the ABIR Adapter and an independent validator with `conformance` authority both
produce the expected outcome for the bound fixture. For an expected rejection,
`internal_valid` is false and the independent observed outcome is `reject`;
that receipt still passes because both validators correctly rejected the
fixture. The contract verifier checks these cross-field rules in addition to
JSON Schema validation.

Unavailable independent evidence is represented as
`"independent_evidence": null`; its receipt must say `"pass": false` and
`"semantic_profile_promoted": false`. A successful parse by pyEDFlib, pydicom,
HDF5 tooling, or any comparable parser may be useful evidence, but it must be
recorded as `parser-only`. Parser-only evidence can neither pass the independent
conformance gate nor promote a semantic profile. Missing executable, version,
schema/dictionary, output, or fixture hashes are not replaced with placeholders:
the evidence remains null and the gate remains failed.

`semantic_profile_promoted` is true exactly when a receipt passes and the
profile registry declares that profile `semantic`. Receipts for forensic,
stream, or hardware profiles may pass their own conformance checks, but cannot
claim semantic-profile promotion. A set of receipts is sufficient for release
only when the profile's separately declared fixture matrix is complete; one
positive receipt does not establish edition-wide conformance.
