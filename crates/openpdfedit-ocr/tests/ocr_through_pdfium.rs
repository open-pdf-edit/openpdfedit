//! Cross-crate/cross-library validation: renders a page with real visible
//! text through real PDFium, OCRs the rendering through a real
//! `tesseract` binary, appends the resulting invisible text layer, saves
//! incrementally, and confirms PDFium's own text-extraction machinery
//! (not this crate's own bookkeeping) now finds more selectable
//! characters than before — the actual end-to-end promise of a
//! "sandwich" OCR layer, verified the same way `openpdfedit-annot` and
//! `openpdfedit-doc`'s own PDFium cross-validation tests are: by actually
//! rendering/reading the saved output through real PDFium, not just
//! re-parsing it with this workspace's own lopdf-based reader.

use openpdfedit_doc::Document;
use openpdfedit_engine::EngineHandle;

fn dev_vendor_lib_dir() -> Option<std::path::PathBuf> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/openpdfedit-ocr -> workspace root
    let workspace_root = manifest_dir.parent()?.parent()?;
    let dir = workspace_root.join(if cfg!(windows) {
        ".vendor/pdfium/bin"
    } else {
        ".vendor/pdfium/lib"
    });
    dir.exists().then_some(dir)
}

/// One engine for the whole test binary.
///
/// PDFium's global init is not safe to run concurrently across
/// independent bindings — a second `EngineHandle` in the same process
/// takes the whole run down with SIGTRAP, which is exactly what happened
/// when the second test here spawned its own. See `PdfiumEngine::new`'s
/// doc: one engine per process, shared.
fn shared_engine() -> Option<&'static EngineHandle> {
    static ENGINE: std::sync::OnceLock<Option<EngineHandle>> = std::sync::OnceLock::new();
    ENGINE
        .get_or_init(|| EngineHandle::spawn(dev_vendor_lib_dir()).ok())
        .as_ref()
}

fn tesseract_available() -> bool {
    std::process::Command::new("tesseract")
        .arg("--version")
        .output()
        .is_ok()
}

/// A one-page PDF whose content stream draws `text` with a real Helvetica
/// font resource at `font_size` — large, high-contrast, plain black text
/// on white, standing in for what a scanner's rasterizer would produce
/// (from the OCR pipeline's perspective, this page's content stream is
/// irrelevant; only its *rendered pixels* matter, exactly as they would
/// for an actual scanned image-only page).
fn text_page_pdf_bytes(text: &str, font_size: f64) -> Vec<u8> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Object, Stream};

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), font_size.into()]),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new("Tj", vec![Object::string_literal(text)]),
            Operation::new("ET", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn ocr_page_makes_a_scanned_looking_page_searchable_through_real_pdfium() {
    let Some(engine) = shared_engine() else {
        eprintln!("skipping: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };
    if !tesseract_available() {
        eprintln!("skipping: tesseract not installed (brew install tesseract)");
        return;
    }

    let tmp_path =
        std::env::temp_dir().join(format!("openpdfedit-ocr-test-{}.pdf", std::process::id()));
    std::fs::write(&tmp_path, text_page_pdf_bytes("HELLO WORLD", 48.0))
        .expect("should write temp file");

    let handle = engine
        .open(&tmp_path)
        .expect("engine should open the temp file");

    // Baseline: this fixture already has real (visible) text objects —
    // unlike an actual scanned page, which would be image-only with zero
    // extractable characters. What this test actually verifies is that
    // OCR-ing the *rendered pixels* (exactly as it would for a genuine
    // scan) and appending the resulting invisible layer produces
    // *additional* real, PDFium-extractable characters on top — proving
    // the pipeline's output is genuinely readable by PDFium's own text
    // engine, not just structurally present according to this crate's
    // own bookkeeping.
    let before_boxes = engine
        .page_char_boxes(handle, 0)
        .expect("char boxes should succeed");
    assert!(
        !before_boxes.is_empty(),
        "fixture should already have real text to compare against"
    );

    let mut doc = Document::open(&tmp_path).expect("doc crate should open the temp file");
    let words_added = openpdfedit_ocr::ocr_page(engine, handle, &mut doc, 0, 200, "eng")
        .expect("OCR should succeed");
    assert!(
        words_added >= 2,
        "should recognize at least HELLO and WORLD, got {words_added}"
    );

    let saved = doc
        .save_incremental()
        .expect("incremental save should succeed");
    std::fs::write(&tmp_path, &saved).expect("should overwrite with the OCR'd bytes");

    engine.close(handle);
    let new_handle = engine
        .open(&tmp_path)
        .expect("PDFium should reopen the OCR'd file");

    let after_boxes = engine
        .page_char_boxes(new_handle, 0)
        .expect("char boxes should succeed on the OCR'd page");
    assert!(
        after_boxes.len() > before_boxes.len(),
        "the invisible OCR text layer must add extractable characters on top of the \
         original text (before: {}, after: {})",
        before_boxes.len(),
        after_boxes.len()
    );

    // The invisible layer must not have broken rendering.
    let tile = engine
        .render_page(new_handle, 0, 300)
        .expect("should still render");
    assert!(tile.height > 0);

    engine.close(new_handle);
    let _ = std::fs::remove_file(&tmp_path);
}

/// The other half: a word PDFium has to read back out of a font that
/// carries no glyphs.
///
/// A Latin word can be written as a literal string in Helvetica, which
/// is what this crate did for everything and which cannot express a
/// character outside 256 codes. Chinese, Japanese, Cyrillic and even an
/// accented Latin word go through a composite `Type0` font instead, as
/// two-byte codes with a `ToUnicode` CMap — and the whole design rests on
/// PDFium reading that CMap rather than demanding an embedded font file
/// for glyphs the layer never draws. That is not a thing to assume: this
/// runs the real library over the real bytes and searches for the word.
///
/// No tesseract needed — the words are handed in directly, which is
/// exactly what the browser build does with the ones tesseract.js
/// produced.
#[test]
fn a_non_latin_word_is_searchable_through_real_pdfium() {
    let Some(engine) = shared_engine() else {
        eprintln!("skipping: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };

    let tmp_path = std::env::temp_dir().join(format!(
        "openpdfedit-ocr-unicode-{}.pdf",
        std::process::id()
    ));
    // A blank-ish page, standing in for a scan of one.
    std::fs::write(&tmp_path, text_page_pdf_bytes(".", 8.0)).expect("should write temp file");

    let mut doc = Document::open(&tmp_path).expect("doc crate should open the temp file");
    // 注意事项 arrives split in two, the way tesseract splits Chinese:
    // the script is not written with spaces, so where the recogniser
    // breaks a "word" is arbitrary. The phrase has to come back out
    // whole or searching for it finds nothing.
    let split_phrase = [("注意", 100.0_f32), ("事项", 180.0)]
        .into_iter()
        .map(|(text, left)| openpdfedit_ocr::OcrWord {
            text: text.to_string(),
            left,
            top: 100.0,
            width: 80.0,
            height: 40.0,
            confidence: 95.0,
        });

    // A year and a Chinese word, as a title sets them: Chinese
    // typesetting puts a space either side of Latin numerals, so the
    // document reads "2026 年" and that is what someone searching types.
    let mixed_scripts =
        [("2026", 100.0_f32), ("年四年级", 190.0)]
            .into_iter()
            .map(|(text, left)| openpdfedit_ocr::OcrWord {
                text: text.to_string(),
                left,
                top: 500.0,
                width: 80.0,
                height: 40.0,
                confidence: 95.0,
            });

    let words: Vec<openpdfedit_ocr::OcrWord> =
        [("Привет", 200.0_f32), ("café", 300.0), ("Hello", 400.0)]
            .into_iter()
            .map(|(text, top)| openpdfedit_ocr::OcrWord {
                text: text.to_string(),
                left: 100.0,
                top,
                width: 200.0,
                height: 40.0,
                confidence: 95.0,
            })
            .chain(split_phrase)
            .chain(mixed_scripts)
            .collect();

    let added = openpdfedit_ocr::add_text_layer(&mut doc, 0, 612.0, 792.0, 612, 792, &words)
        .expect("add_text_layer should succeed");
    assert_eq!(
        added, 5,
        "three separate words, plus each merged pair as one run"
    );

    let saved = doc.save_incremental().expect("incremental save");
    std::fs::write(&tmp_path, &saved).expect("overwrite with the OCR'd bytes");

    let handle = engine
        .open(&tmp_path)
        .expect("PDFium should reopen the file");
    for query in ["注意事项", "2026 年四年级", "Привет", "café", "Hello"] {
        let hits = engine
            .search_document(handle, query, Default::default(), 10)
            .unwrap_or_else(|e| panic!("search for {query} failed: {e}"));
        assert!(
            !hits.is_empty(),
            "PDFium found no {query} — the text layer is not readable, \
             which is the whole point of writing one"
        );
    }

    // And it is still invisible: the page must render as it did.
    let tile = engine.render_page(handle, 0, 300).expect("should render");
    assert!(tile.height > 0);

    engine.close(handle);
    let _ = std::fs::remove_file(&tmp_path);
}
