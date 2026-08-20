# openpdfedit-doc fuzz target

Fuzzes `Document::from_bytes` — the parser every untrusted PDF a user
opens goes through (see PLAN.md §9). A parse *error* on malformed input
is correct; a panic, OOM, or hang is a real bug worth a regression test.

## One-time setup

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
../../../scripts/fetch-test-corpus.sh   # from crates/openpdfedit-doc/fuzz/
../../../scripts/seed-fuzz-corpus.sh
```

## Running

Build/run off the shared VM mount if you're on it — see the repo's build
notes; `cargo fuzz` needs `--target-dir` pointed at local disk (it does
not honor `CARGO_TARGET_DIR`):

```sh
cargo +nightly fuzz run parse_document --target-dir ~/.cache/openpdfedit-fuzz-target
```

Run for a bounded time instead of forever:

```sh
cargo +nightly fuzz run parse_document --target-dir ~/.cache/openpdfedit-fuzz-target -- -max_total_time=120
```

A crash writes a minimized repro to `artifacts/parse_document/`; add it to
`../../../testdata/corpus/` (with a describing filename) once fixed, so
the corpus regression test (`tests/corpus.rs`) catches any future
reintroduction.
