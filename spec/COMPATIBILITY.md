# Semantic-v1 compatibility

| Change | Reader behavior | Version effect |
|---|---|---|
| Add namespaced modality or concept | Preserve as unknown | Compatible |
| Add optional metadata outside logical identity | Preserve when possible | Compatible |
| Add registry alias without changing canonical identifier | Resolve to canonical identifier | Compatible |
| Add atom kind or exact-number tag | Reject under v1 | New semantic major |
| Change canonicalization or hash domain | Reject mixed identity | New semantic major |
| Relax validation, proof, or policy inheritance | Reject | Forbidden in v1 |
| Change storage/chunk layout only | Ignore for semantics | Storage-version change |

Writers declare `semantic_version: "1"`. Readers may accept a later compatible
minor only when every required feature is understood. Unknown concepts are data;
unknown structural semantics are errors.
