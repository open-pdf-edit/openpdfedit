//! OCR pipeline (PLAN.md M4: "scanned doc becomes searchable locally").
//!
//! Three steps, each independently testable: render a page to pixels via
//! `openpdfedit-engine` (the same tile renderer the viewer uses, just at
//! OCR-appropriate DPI instead of screen DPI); recognize words and their
//! pixel-space bounding boxes by shelling out to a locally-installed
//! `tesseract` binary; append those words as an invisible ("sandwich")
//! text layer onto the page's content stream via
//! `openpdfedit-doc::Document::append_content_stream`, so the page
//! becomes searchable/selectable without changing a single visible pixel
//! of the scanned image underneath.
//!
//! ## Why a subprocess, not an FFI binding
//!
//! `leptess`/`tesseract-rs`-style bindings link against `libtesseract` +
//! `liblept` directly, which means the exact library version and its
//! transitive C dependency graph become part of this crate's build story
//! on every platform this app ships to. Shelling out to the `tesseract`
//! CLI (installed via Homebrew/apt/the official Windows installer,
//! exactly like this repo already treats `qpdf` as an optional external
//! tool elsewhere) keeps that version-matching problem outside this
//! crate entirely, at the cost of one process spawn per OCR'd page — a
//! fine trade at "a few pages a minute," the workload this feature
//! targets. Detecting a missing `tesseract` and failing with a clear
//! error (see [`OcrError::TesseractNotFound`]) rather than silently
//! degrading is deliberate: OCR is opt-in, not something to guess about.
//!
//! ## Scope: platform-native OCR (macOS Vision, Windows.Media.Ocr) is not
//! implemented
//!
//! PLAN.md's M4 milestone calls for "platform OCR engines + Tesseract
//! fallback." Only the Tesseract path exists here — platform-native
//! engines would each need their own FFI bridge (`objc2`-based Vision
//! bindings on macOS, `windows-rs`-based WinRT bindings on Windows), are
//! individually substantial undertakings, and critically **can't be
//! verified from this development environment**: this is a single macOS
//! sandbox, so a Windows.Media.Ocr integration could be written but never
//! actually run here. Shipping unverified platform-specific code alongside
//! a real, tested Tesseract path would be the same mistake as the lopdf
//! encryption issue documented in `openpdfedit-doc` — better to ship the
//! one path that's genuinely been exercised and flag the other as
//! deferred than to pretend both are done.
//!
//! ## Scope: ASCII-range text only
//!
//! Recognized words are written as PDF literal strings (`(text)` — raw
//! bytes, PDFDocEncoding/Latin-1-ish), not UTF-16BE hex strings with a
//! `ToUnicode` CMap. That's correct for `tesseract`'s `eng` language pack
//! (the only language data this dev environment has installed — see
//! `recognize_words`'s doc), which outputs plain ASCII for English text.
//! A word containing non-ASCII characters is skipped, not mis-encoded —
//! full Unicode support needs the CMap infrastructure and is future work.

#[cfg(not(target_arch = "wasm32"))]
use std::process::Command;

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Object, Stream};
use openpdfedit_doc::{DocError, Document};
use openpdfedit_engine::{DocHandle, EngineError, EngineHandle};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OcrError {
    #[error("Tesseract OCR was not found.\n\n{0}")]
    TesseractNotFound(String),
    #[error("tesseract exited with an error: {0}")]
    TesseractFailed(String),
    #[error("failed to encode the rendered page as an image: {0}")]
    ImageEncode(String),
    #[error("failed to build the text-layer content stream: {0}")]
    ContentEncode(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Doc(#[from] DocError),
    #[error("render engine error: {0}")]
    Engine(String),
}

impl From<EngineError> for OcrError {
    fn from(e: EngineError) -> Self {
        OcrError::Engine(e.to_string())
    }
}

/// One word Tesseract recognized, in *pixel* space relative to the image
/// that was OCR'd (top-left origin, y-down — image-space, not PDF's
/// bottom-left/y-up page space; [`add_text_layer`] converts).
#[derive(Debug, Clone, PartialEq)]
pub struct OcrWord {
    pub text: String,
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
    /// Tesseract's own 0-100 confidence for this word.
    pub confidence: f32,
}

// Everything from here to `add_text_layer` shells out to the tesseract
// binary, which wasm32 has no way to do. Gated rather than removed: the
// browser build reaches the same PDF-writing code below by handing in
// words its own recogniser produced (tesseract.js), so the two paths
// differ only in where the words come from.
#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(target_arch = "wasm32"))]
/// Runs `tesseract` over a rendered page tile (`width`×`height`,
/// row-major RGBA8 — exactly what `EngineHandle::render_page` returns)
/// and returns every recognized word with its pixel-space bounding box.
///
/// `lang` must name tesseract language data already installed locally
/// (`tesseract --list-langs`); this repo's dev setup only has `eng`
/// installed (`brew install tesseract` ships only `eng`/`osd`/`snum` —
/// see `brew info tesseract`'s caveat), so `"eng"` is the only value
/// exercised by this crate's own tests. `--psm 1` (automatic page
/// segmentation with orientation/script detection) is tesseract's
/// general-purpose "OCR everything, don't assume a single text block"
/// mode — the right default for a scanned document page rather than a
/// pre-cropped single word/line.
pub fn recognize_words(
    rgba: &[u8],
    width: u32,
    height: u32,
    lang: &str,
) -> Result<Vec<OcrWord>, OcrError> {
    let img = image::RgbaImage::from_raw(width, height, rgba.to_vec()).ok_or_else(|| {
        OcrError::ImageEncode(format!(
            "rgba buffer length {} does not match {width}x{height}x4",
            rgba.len()
        ))
    })?;

    let tmp_in = std::env::temp_dir().join(format!(
        "openpdfedit-ocr-{}-{:x}.png",
        std::process::id(),
        rgba.len() as u64 ^ ((width as u64) << 32 | height as u64)
    ));
    image::DynamicImage::ImageRgba8(img)
        .save(&tmp_in)
        .map_err(|e| OcrError::ImageEncode(e.to_string()))?;

    let result = run_tesseract(&tmp_in, lang);
    let _ = std::fs::remove_file(&tmp_in);
    result
}

#[cfg(not(target_arch = "wasm32"))]
/// Where to look for a `tesseract` binary, in order, when the process's
/// own `PATH` doesn't have one.
///
/// A GUI app launched from Finder/Dock does **not** inherit the shell's
/// `PATH` — macOS gives it a bare `/usr/bin:/bin:/usr/sbin:/sbin`. So a
/// perfectly working `brew install tesseract` is invisible to the
/// bundled app even though it resolves fine in a terminal, which is
/// exactly how OCR came to fail with "not installed or not on PATH" on a
/// machine that had it installed. These are the standard install
/// prefixes for the package managers that ship tesseract.
const TESSERACT_FALLBACK_PATHS: &[&str] = &[
    // Homebrew on Apple Silicon, then Intel.
    "/opt/homebrew/bin/tesseract",
    "/usr/local/bin/tesseract",
    // MacPorts.
    "/opt/local/bin/tesseract",
    // Linux distro packages.
    "/usr/bin/tesseract",
    "/snap/bin/tesseract",
    // The official Windows installer's default location.
    r"C:\Program Files\Tesseract-OCR\tesseract.exe",
];

#[cfg(not(target_arch = "wasm32"))]
/// The environment variable that overrides binary discovery entirely,
/// for an install in a location this crate doesn't know about.
const TESSERACT_PATH_ENV: &str = "OPENPDFEDIT_TESSERACT";

#[cfg(not(target_arch = "wasm32"))]
/// Resolves the `tesseract` executable to invoke: an explicit override
/// first, then whatever `PATH` provides, then the well-known install
/// prefixes above. Returns `None` when no candidate can be executed.
pub fn tesseract_path() -> Option<std::path::PathBuf> {
    let runs = |program: &std::path::Path| -> bool {
        Command::new(program)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    };

    if let Some(explicit) = std::env::var_os(TESSERACT_PATH_ENV) {
        let path = std::path::PathBuf::from(explicit);
        if runs(&path) {
            return Some(path);
        }
    }
    let bare = std::path::PathBuf::from("tesseract");
    if runs(&bare) {
        return Some(bare);
    }
    TESSERACT_FALLBACK_PATHS
        .iter()
        .map(std::path::PathBuf::from)
        .find(|candidate| candidate.exists() && runs(candidate))
}

#[cfg(not(target_arch = "wasm32"))]
/// What to tell the user when no `tesseract` can be found. Actionable by
/// design: the previous message ("not installed or not on PATH: No such
/// file or directory (os error 2)") was accurate and useless — it named
/// neither what to install nor how.
fn missing_tesseract_message() -> String {
    format!(
        "Searched PATH and {}.\n\n\
         OCR needs the free Tesseract engine installed on this machine:\n  \
         macOS:    brew install tesseract\n  \
         Debian:   sudo apt install tesseract-ocr\n  \
         Windows:  https://github.com/UB-Mannheim/tesseract/wiki\n\n\
         Already installed somewhere else? Set {TESSERACT_PATH_ENV} to its full path.",
        TESSERACT_FALLBACK_PATHS.join(", ")
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn run_tesseract(image_path: &std::path::Path, lang: &str) -> Result<Vec<OcrWord>, OcrError> {
    let program =
        tesseract_path().ok_or_else(|| OcrError::TesseractNotFound(missing_tesseract_message()))?;

    let output = Command::new(&program)
        .arg(image_path)
        .arg("stdout")
        .arg("-l")
        .arg(lang)
        .arg("--psm")
        .arg("1")
        .arg("tsv")
        .output()
        .map_err(|e| {
            OcrError::TesseractNotFound(format!("could not run {}: {e}", program.display()))
        })?;

    if !output.status.success() {
        return Err(OcrError::TesseractFailed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }

    Ok(parse_tsv(&String::from_utf8_lossy(&output.stdout)))
}

/// Parses tesseract's `tsv` output format: a header row naming columns,
/// then one row per recognized element at every segmentation level (page,
/// block, paragraph, line, word). Only `level == 5` (word) rows carry
/// real text; every other level's `text` column is empty. Malformed rows
/// (wrong column count, non-numeric fields) are skipped rather than
/// failing the whole parse — tesseract's own output format is stable
/// enough that this is defensive, not expected to trigger in practice.
fn parse_tsv(tsv: &str) -> Vec<OcrWord> {
    let mut words = Vec::new();
    let mut lines = tsv.lines();
    let Some(header) = lines.next() else {
        return words;
    };
    let columns: Vec<&str> = header.split('\t').collect();
    let col = |name: &str| columns.iter().position(|c| *c == name);
    let (
        Some(i_level),
        Some(i_left),
        Some(i_top),
        Some(i_width),
        Some(i_height),
        Some(i_conf),
        Some(i_text),
    ) = (
        col("level"),
        col("left"),
        col("top"),
        col("width"),
        col("height"),
        col("conf"),
        col("text"),
    )
    else {
        return words;
    };

    for line in lines {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() <= i_text || fields.get(i_level) != Some(&"5") {
            continue;
        }
        let text = fields[i_text].trim();
        if text.is_empty() {
            continue;
        }
        let (Ok(left), Ok(top), Ok(width), Ok(height), Ok(confidence)) = (
            fields[i_left].parse::<f32>(),
            fields[i_top].parse::<f32>(),
            fields[i_width].parse::<f32>(),
            fields[i_height].parse::<f32>(),
            fields[i_conf].parse::<f32>(),
        ) else {
            continue;
        };
        // Non-word rows (which this loop already filters via level == 5)
        // aside, tesseract also uses a confidence of -1 to mean "not
        // applicable"; a genuine word row should never hit that, but skip
        // rather than trust a nonsensical confidence if it somehow does.
        if confidence < 0.0 {
            continue;
        }
        words.push(OcrWord {
            text: text.to_string(),
            left,
            top,
            width,
            height,
            confidence,
        });
    }
    words
}

/// Builds an invisible text-layer content stream from `words` (in pixel
/// space, against an image of `image_width`×`image_height` px covering
/// the full `page_width_pt`×`page_height_pt` page) and appends it to
/// `page_index` via [`Document::append_content_stream`] — additive, never
/// touching the page's existing visible content. Uses Helvetica, one of
/// the 14 standard PDF fonts every conforming reader must support without
/// any embedded font file.
///
/// Text render mode 3 (`Tr`) is "invisible" — painted for
/// selection/search/copy purposes but never rasterized, which is exactly
/// the "sandwich" a scanned-page OCR layer needs: the visible page is
/// still the scanned image, unchanged.
pub fn add_text_layer(
    doc: &mut Document,
    page_index: u32,
    page_width_pt: f32,
    page_height_pt: f32,
    image_width_px: u32,
    image_height_px: u32,
    words: &[OcrWord],
) -> Result<usize, OcrError> {
    let ascii_words: Vec<&OcrWord> = words.iter().filter(|w| w.text.is_ascii()).collect();
    if ascii_words.is_empty() {
        return Ok(0);
    }

    let scale_x = page_width_pt / image_width_px.max(1) as f32;
    let scale_y = page_height_pt / image_height_px.max(1) as f32;

    let mut operations = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tr", vec![3.into()]),
    ];

    for word in &ascii_words {
        let font_size = (word.height * scale_y).max(1.0);
        // Image space is top-left origin, y-down; PDF page space is
        // bottom-left origin, y-up — flip, and anchor at the bottom of
        // the word's bounding box (where `Tm`'s baseline-ish origin
        // reads most naturally for a left-to-right Latin word).
        let x = word.left * scale_x;
        let y = page_height_pt - (word.top + word.height) * scale_y;

        operations.push(Operation::new(
            "Tf",
            vec!["OcrHelv".into(), font_size.into()],
        ));
        operations.push(Operation::new(
            "Tm",
            vec![1.into(), 0.into(), 0.into(), 1.into(), x.into(), y.into()],
        ));
        operations.push(Operation::new(
            "Tj",
            vec![Object::string_literal(word.text.clone())],
        ));
    }
    operations.push(Operation::new("ET", vec![]));

    let content = Content { operations };
    let encoded = content
        .encode()
        .map_err(|e| OcrError::ContentEncode(e.to_string()))?;

    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    }));
    let stream_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, encoded)));
    doc.append_content_stream(page_index, stream_id, "OcrHelv", font_id)?;

    Ok(ascii_words.len())
}

#[cfg(not(target_arch = "wasm32"))]
/// OCRs one page: renders it via `engine` at `dpi`, recognizes words, and
/// appends the invisible text layer to `doc`. Does not save — the caller
/// decides when, via [`Document::save_incremental`], same as every other
/// `openpdfedit-doc` mutation. Returns the number of words added.
pub fn ocr_page(
    engine: &EngineHandle,
    handle: DocHandle,
    doc: &mut Document,
    page_index: u32,
    dpi: u32,
    lang: &str,
) -> Result<usize, OcrError> {
    let page_sizes = engine.page_sizes(handle)?;
    let size = page_sizes.get(page_index as usize).ok_or_else(|| {
        OcrError::Engine(format!(
            "page {page_index} out of range ({} pages)",
            page_sizes.len()
        ))
    })?;

    // Points -> pixels at the requested scan DPI (72 points per inch is
    // the PDF-spec constant, not a magic number).
    let target_width = ((size.width / 72.0) * dpi as f32).round().max(1.0) as u32;
    let tile = engine.render_page(handle, page_index, target_width)?;

    let words = recognize_words(&tile.rgba, tile.width, tile.height, lang)?;
    add_text_layer(
        doc,
        page_index,
        size.width,
        size.height,
        tile.width,
        tile.height,
        &words,
    )
}

#[cfg(not(target_arch = "wasm32"))]
/// OCRs every page of `doc`, in order. Returns the total word count added
/// across all pages. A single page's failure (e.g. a transient tesseract
/// error) aborts the whole call rather than silently skipping that page —
/// callers that want best-effort partial OCR should call [`ocr_page`]
/// directly per page instead.
pub fn ocr_document(
    engine: &EngineHandle,
    handle: DocHandle,
    doc: &mut Document,
    dpi: u32,
    lang: &str,
) -> Result<usize, OcrError> {
    let page_count = engine.page_count(handle)?;
    let mut total_words = 0;
    for page_index in 0..page_count {
        total_words += ocr_page(engine, handle, doc, page_index, dpi, lang)?;
    }
    Ok(total_words)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The regression test for OCR failing inside the packaged app with
    /// "tesseract is not installed or not on PATH" on a machine where
    /// `brew install tesseract` had plainly worked.
    ///
    /// A GUI app launched from Finder gets `PATH=/usr/bin:/bin:/usr/sbin:
    /// /sbin` — Homebrew's prefix is not on it. Emptying `PATH` here
    /// reproduces that environment exactly: bare `Command::new
    /// ("tesseract")` can no longer resolve, and only the well-known-
    /// prefix search can find the binary.
    ///
    /// Skips (rather than fails) where tesseract genuinely isn't
    /// installed, matching how the other tesseract-dependent tests in
    /// this repo behave.
    #[test]
    fn tesseract_is_found_even_with_an_empty_path() {
        if tesseract_path().is_none() {
            eprintln!("skipping: tesseract not installed (brew install tesseract)");
            return;
        }

        let original = std::env::var_os("PATH");
        // SAFETY: single-threaded within this test, and the original
        // value is restored before returning.
        unsafe { std::env::set_var("PATH", "") };
        let found = tesseract_path();
        match original {
            Some(value) => unsafe { std::env::set_var("PATH", value) },
            None => unsafe { std::env::remove_var("PATH") },
        }

        assert!(
            found.is_some(),
            "with an empty PATH — the environment a Finder-launched app actually gets — \
             tesseract must still be found via its standard install prefix"
        );
    }

    /// The message a user sees has to tell them what to do about it, not
    /// just that something is missing.
    #[test]
    fn the_missing_tesseract_error_explains_how_to_install_it() {
        let message = OcrError::TesseractNotFound(missing_tesseract_message()).to_string();
        assert!(message.contains("brew install tesseract"));
        assert!(message.contains("apt install tesseract-ocr"));
        assert!(
            message.contains(TESSERACT_PATH_ENV),
            "the override variable should be mentioned as an escape hatch"
        );
    }

    /// A real (trimmed) sample of tesseract's `tsv` output shape: header
    /// row, then rows at every segmentation level (1=page, 2=block,
    /// 3=paragraph, 4=line, 5=word) — only level 5 carries real text.
    const SAMPLE_TSV: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
1\t1\t0\t0\t0\t0\t0\t0\t612\t792\t-1\t\n\
2\t1\t1\t0\t0\t0\t50\t50\t200\t30\t-1\t\n\
3\t1\t1\t1\t0\t0\t50\t50\t200\t30\t-1\t\n\
4\t1\t1\t1\t1\t0\t50\t50\t200\t30\t-1\t\n\
5\t1\t1\t1\t1\t1\t50\t50\t90\t30\t95.500000\tHello\n\
5\t1\t1\t1\t1\t2\t150\t50\t100\t30\t92.300000\tWorld\n";

    #[test]
    fn parse_tsv_extracts_only_word_level_rows() {
        let words = parse_tsv(SAMPLE_TSV);
        assert_eq!(words.len(), 2, "only the two level-5 rows carry real words");
        assert_eq!(words[0].text, "Hello");
        assert_eq!(words[0].left, 50.0);
        assert_eq!(words[0].top, 50.0);
        assert_eq!(words[0].width, 90.0);
        assert_eq!(words[0].height, 30.0);
        assert!((words[0].confidence - 95.5).abs() < 0.01);
        assert_eq!(words[1].text, "World");
        assert_eq!(words[1].left, 150.0);
    }

    #[test]
    fn parse_tsv_on_empty_input_returns_no_words_not_a_panic() {
        assert!(parse_tsv("").is_empty());
        assert!(parse_tsv("level\tleft\ttop\twidth\theight\tconf\ttext\n").is_empty());
    }

    #[test]
    fn parse_tsv_skips_rows_with_empty_text() {
        // A blank-text word row shouldn't happen from real tesseract
        // output, but a malformed/truncated line must not panic or
        // produce a bogus empty-string "word".
        let tsv = "level\tleft\ttop\twidth\theight\tconf\ttext\n5\t10\t10\t5\t5\t80\t\n";
        assert!(parse_tsv(tsv).is_empty());
    }

    #[test]
    fn add_text_layer_skips_non_ascii_words_but_keeps_ascii_ones() {
        let bytes = {
            let mut doc = lopdf::Document::with_version("1.5");
            let pages_id = doc.new_object_id();
            let content_id = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => content_id,
                "Resources" => dictionary! {},
            });
            doc.objects.insert(
                pages_id,
                Object::Dictionary(dictionary! {
                    "Type" => "Pages",
                    "Kids" => vec![page_id.into()],
                    "Count" => 1,
                }),
            );
            let catalog_id =
                doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
            doc.trailer.set("Root", catalog_id);
            let mut bytes = Vec::new();
            doc.save_to(&mut bytes).unwrap();
            bytes
        };
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        let words = vec![
            OcrWord {
                text: "Hello".into(),
                left: 10.0,
                top: 10.0,
                width: 50.0,
                height: 20.0,
                confidence: 90.0,
            },
            OcrWord {
                text: "café".into(), // non-ASCII — must be skipped, not miswritten
                left: 70.0,
                top: 10.0,
                width: 50.0,
                height: 20.0,
                confidence: 90.0,
            },
        ];

        let added = add_text_layer(&mut doc, 0, 612.0, 792.0, 612, 792, &words)
            .expect("add_text_layer should succeed");
        assert_eq!(added, 1, "only the ASCII word should be added");
    }

    #[test]
    fn add_text_layer_with_no_words_is_a_no_op() {
        let bytes = {
            let mut doc = lopdf::Document::with_version("1.5");
            let pages_id = doc.new_object_id();
            let content_id = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => content_id,
                "Resources" => dictionary! {},
            });
            doc.objects.insert(
                pages_id,
                Object::Dictionary(dictionary! {
                    "Type" => "Pages",
                    "Kids" => vec![page_id.into()],
                    "Count" => 1,
                }),
            );
            let catalog_id =
                doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
            doc.trailer.set("Root", catalog_id);
            let mut bytes = Vec::new();
            doc.save_to(&mut bytes).unwrap();
            bytes
        };
        let mut doc = Document::from_bytes(&bytes).expect("should parse");
        let added = add_text_layer(&mut doc, 0, 612.0, 792.0, 612, 792, &[])
            .expect("should succeed with nothing to add");
        assert_eq!(added, 0);
    }
}
