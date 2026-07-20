# DatasetDraft seed corpus

`dataset_draft/` contains committed libFuzzer seeds spanning small valid,
truncated, zero-extent, oversized-rank, and mutated descriptor inputs. Verify
the corpus with:

```sh
(cd fuzz/corpus/dataset_draft && sha256sum -c ../dataset_draft.sha256)
cargo +nightly fuzz run dataset_draft fuzz/corpus/dataset_draft -- -runs=1000
```

CI and the LamQuant semantic-core gate execute the corpus as a bounded smoke
run. Longer fuzz campaigns may expand a temporary copy; generated discoveries
must be minimized, reviewed, and added to the manifest before landing.
