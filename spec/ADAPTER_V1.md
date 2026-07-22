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
