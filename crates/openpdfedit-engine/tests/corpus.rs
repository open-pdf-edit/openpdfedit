//! Corpus regression test for the PDFium-backed engine: every PDF in
//! `testdata/corpus/` (see `crates/openpdfedit-doc/tests/corpus.rs` for
//! provenance) must open, report a page count, and render its first page
//! without the render thread panicking. This is the render-path
//! counterpart to the doc crate's parse-path corpus test.
//!
//! One [`EngineHandle`] for the whole test — not one per file — because
//! PDFium's global init may only run once per process (see the crate's
//! module docs) and because reusing one handle is also just the realistic
//! usage pattern (a real app opens many documents through one engine).

use openpdfedit_engine::EngineHandle;
use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("testdata/corpus")
}

fn dev_vendor_lib_dir() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent()?.parent()?;
    let dir = workspace_root.join(if cfg!(windows) {
        ".vendor/pdfium/bin"
    } else {
        ".vendor/pdfium/lib"
    });
    dir.exists().then_some(dir)
}

#[test]
fn every_corpus_pdf_opens_and_renders_without_panicking() {
    let dir = corpus_dir();
    if !dir.exists() {
        eprintln!(
            "skipping corpus test: {} not present (run scripts/fetch-test-corpus.sh)",
            dir.display()
        );
        return;
    }

    let Some(engine) = EngineHandle::spawn(dev_vendor_lib_dir()).ok() else {
        eprintln!("skipping corpus test: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };

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

    let mut rendered_ok = 0;
    let mut failed = Vec::new();

    for path in &entries {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let result: Result<(), String> = (|| {
            let handle = engine.open(path).map_err(|e| e.to_string())?;
            let count = engine.page_count(handle).map_err(|e| e.to_string())?;
            if count > 0 {
                engine
                    .render_page(handle, 0, 100)
                    .map_err(|e| e.to_string())?;
            }
            engine.close(handle);
            Ok(())
        })();

        match result {
            Ok(()) => rendered_ok += 1,
            Err(e) => failed.push((name, e)),
        }
    }

    eprintln!(
        "corpus: {rendered_ok}/{} opened+rendered cleanly, {} failed (not a panic — see above)",
        entries.len(),
        failed.len()
    );
    for (name, err) in &failed {
        eprintln!("  {name}: {err}");
    }

    // Unlike the parse-only doc-crate test, PDFium is the reference
    // renderer for the wild — it should succeed on nearly everything in
    // a real-world-derived corpus. A hard floor here catches a real
    // regression (e.g. a bad PDFium upgrade) without demanding 100% on
    // the two deliberately-adversarial "*-fuzzed.pdf" fixtures.
    let success_rate = rendered_ok as f64 / entries.len() as f64;
    let success_pct = success_rate * 100.0;
    assert!(
        success_rate >= 0.75,
        "only {rendered_ok}/{} corpus files opened+rendered ({success_pct:.0}%) — expected >= 75%",
        entries.len()
    );
}
