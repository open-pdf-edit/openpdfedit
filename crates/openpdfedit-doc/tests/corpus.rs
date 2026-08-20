//! Corpus regression test: every PDF in `testdata/corpus/` (fetched by
//! `scripts/fetch-test-corpus.sh`, a curated slice of pdf.js's Apache-2.0
//! test suite — see `testdata/corpus/SOURCE.md`) must not panic
//! `openpdfedit_doc::Document`'s parser. Two of these files are literally
//! named `*-fuzzed.pdf`/`*-reduced.pdf` — pdf.js's own minimized repro
//! cases for past parser crashes, chosen deliberately as edge-case seeds.
//!
//! This is a "parse-don't-crash" test, not a "parse-always-succeeds"
//! test: a malformed input is allowed to come back as `Err`, just never
//! as a panic. Skips entirely (rather than failing) if the corpus hasn't
//! been fetched, so a fresh checkout without running the fetch script
//! doesn't break `cargo test`.

use openpdfedit_doc::Document;
use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("testdata/corpus")
}

#[test]
fn every_corpus_pdf_parses_without_panicking() {
    let dir = corpus_dir();
    if !dir.exists() {
        eprintln!(
            "skipping corpus test: {} not present (run scripts/fetch-test-corpus.sh)",
            dir.display()
        );
        return;
    }

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus dir should be readable")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "pdf"))
        .collect();
    entries.sort();

    assert!(
        !entries.is_empty(),
        "corpus dir exists but has no .pdf files"
    );

    let mut parsed_ok = 0;
    let mut parse_failed = Vec::new();

    for path in &entries {
        let bytes = std::fs::read(path).expect("corpus file should be readable");
        // The property under test: this must never panic, regardless of
        // how malformed `bytes` is. A parse error is a fine, expected
        // outcome for the deliberately-broken fixtures in this corpus.
        match Document::from_bytes(&bytes) {
            Ok(_) => parsed_ok += 1,
            Err(e) => {
                parse_failed.push((path.file_name().unwrap().to_string_lossy().into_owned(), e))
            }
        }
    }

    eprintln!(
        "corpus: {parsed_ok}/{} parsed cleanly, {} failed to parse (not a panic — see above)",
        entries.len(),
        parse_failed.len()
    );
    for (name, err) in &parse_failed {
        eprintln!("  {name}: {err}");
    }
}
