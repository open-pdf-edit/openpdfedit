//! End-to-end proof on a *real-world* subset-font PDF: the exact case
//! that previously refused to edit at all. Builds nothing synthetic —
//! uses whatever PDF is handed to it via OPENPDFEDIT_TEST_PDF, and skips
//! when that isn't set, so CI stays hermetic while this can still be run
//! against real user documents.
use openpdfedit_doc::Document;
use openpdfedit_engine::{Engine, PdfiumEngine};

fn dev_vendor_lib_dir() -> Option<std::path::PathBuf> {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.parent()?.parent()?;
    let dir = root.join(if cfg!(windows) {
        ".vendor/pdfium/bin"
    } else {
        ".vendor/pdfium/lib"
    });
    dir.exists().then_some(dir)
}

#[test]
fn editing_real_subset_font_text_round_trips_through_pdfium() {
    let Ok(src) = std::env::var("OPENPDFEDIT_TEST_PDF") else {
        eprintln!("skipping: set OPENPDFEDIT_TEST_PDF to a real PDF to run this");
        return;
    };
    let Ok(engine) = PdfiumEngine::new(dev_vendor_lib_dir().as_deref()) else {
        eprintln!("skipping: PDFium not available");
        return;
    };

    let mut doc = Document::open(&src).expect("should open the real PDF");
    let runs = openpdfedit_textedit::list_text_runs_in_page(&doc, 0).expect("should list runs");

    // Pick a substantial editable run — the kind a user would click.
    let target = runs
        .iter()
        .filter(|r| r.is_editable && r.text.trim().len() >= 5)
        .max_by_key(|r| r.text.trim().len())
        .expect("a real document should have at least one editable text run")
        .clone();
    println!("editing run: {:?}", target.text);

    // Replace with text drawn only from characters already in the
    // document (so the subset is guaranteed to contain their glyphs).
    let replacement: String = {
        let mut seen: Vec<char> = Vec::new();
        for c in target.text.chars() {
            if c.is_alphanumeric() && !seen.contains(&c) {
                seen.push(c);
            }
        }
        assert!(
            seen.len() >= 3,
            "need a few distinct characters to build a replacement"
        );
        seen.into_iter().take(6).collect()
    };
    println!("replacement: {replacement:?}");

    openpdfedit_textedit::edit_text_run(&mut doc, &target, &replacement)
        .expect("editing a real subset-font run must succeed");
    let saved = doc.save_incremental().expect("save should succeed");

    let out =
        std::env::temp_dir().join(format!("openpdfedit-real-edit-{}.pdf", std::process::id()));
    std::fs::write(&out, &saved).expect("write");

    // The authority is PDFium's own text extraction, not our reader.
    let handle = engine
        .open(&out)
        .expect("PDFium should open the edited file");
    let boxes = engine.page_char_boxes(handle, 0).expect("char boxes");
    assert!(
        !boxes.is_empty(),
        "the edited page must still contain extractable text"
    );

    // And our own reader must find the replacement back as real text.
    let reopened = Document::from_bytes(&saved).expect("reparse");
    let after = openpdfedit_textedit::list_text_runs_in_page(&reopened, 0).expect("list");
    assert!(
        after.iter().any(|r| r.text.contains(&replacement)),
        "replacement {replacement:?} must be readable back from the saved file"
    );

    engine.close(handle);
    let _ = std::fs::remove_file(&out);
}
