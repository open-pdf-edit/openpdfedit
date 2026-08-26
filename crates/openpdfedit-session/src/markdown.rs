//! The open document as Markdown.
//!
//! For getting a PDF's text into something that is itself editable — a
//! note, a wiki, an Obsidian vault — rather than into another PDF.
//!
//! The conversion is [`anydoc`](https://github.com/firecrawl/anydoc),
//! which is Rust and runs locally. That last part is not incidental:
//! every hosted converter would mean uploading the document, and this
//! product's one promise is that it never does.
//!
//! `anydoc` is linked on the desktop only — see this crate's
//! `Cargo.toml`. The browser build loads Firecrawl's own wasm package
//! when someone asks, and falls back to the same page text this module
//! produces. Both ends therefore behave the same; only where the
//! converter comes from differs.
//!
//! ## Two paths, because a scan is not a document
//!
//! `anydoc` reads a PDF's structure and gets headings, lists and tables
//! out of it. It also, deliberately, ignores text drawn in rendering
//! mode 3 — invisible text — which is exactly what this app's OCR
//! writes, because an OCR layer has to sit under the picture of the page
//! without changing a pixel of it. Handed a freshly OCR'd scan, `anydoc`
//! says the PDF has no extractable text and suggests running OCR.
//!
//! Since scans are most of the reason anyone wants a PDF as Markdown,
//! that answer is not good enough. So when `anydoc` finds nothing, the
//! text PDFium can see is used instead — which includes the invisible
//! layer, being the same extraction that makes an OCR'd page
//! searchable. What comes back is paragraphs rather than structure: no
//! headings, no tables, because nothing in a scan says which line was a
//! heading. It is the document's words, in order, in a file that can be
//! edited.

use openpdfedit_engine::{DocHandle, Engine};

use crate::{resolve_doc, SessionError, SessionState};

/// Converts the open document to GitHub-Flavored Markdown.
///
/// Reads the working copy from the store directly rather than through
/// [`working_copy_bytes`](crate::working_copy_bytes), which re-encrypts
/// a protected document on the way out. Markdown is text, and the only
/// way to reach this is with the document open — which took the
/// password. Re-encrypting first would hand `anydoc` a file it cannot
/// read, and produce nothing.
#[cfg(not(target_arch = "wasm32"))]
pub fn export_markdown_impl<E: Engine>(
    state: &SessionState<E>,
    handle: DocHandle,
) -> Result<String, SessionError> {
    let path = {
        let docs = state.docs.lock().expect("docs lock poisoned");
        resolve_doc(&docs, handle)?.path.clone()
    };
    let bytes = state.store.read(&path)?;

    match anydoc::to_markdown_bytes(&bytes, anydoc::Format::Pdf) {
        Ok(markdown) if !markdown.trim().is_empty() => Ok(markdown),
        // Either it found nothing, or it found nothing it would call
        // text. An OCR'd scan lands here; so does a page of pictures,
        // and that one comes back empty from the fallback too, which is
        // the honest answer.
        _ => markdown_from_page_text(state, handle),
    }
}

/// Every page's text, as PDFium reads it, in Markdown's plainest form.
///
/// Pages are separated by a rule, because a page break is the one piece
/// of structure a scan does carry and losing it runs the last line of
/// one page into the first line of the next.
pub fn markdown_from_page_text<E: Engine>(
    state: &SessionState<E>,
    handle: DocHandle,
) -> Result<String, SessionError> {
    let page_count = state.engine.page_count(handle)?;
    let mut out = String::new();

    for page_index in 0..page_count {
        let text: String = state
            .engine
            .page_chars(handle, page_index)?
            .iter()
            .collect();
        let cleaned = tidy(&text);
        if cleaned.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n---\n\n");
        }
        out.push_str(&cleaned);
        out.push('\n');
    }

    Ok(out)
}

/// PDFium's page text with its line breaks made into Markdown's.
///
/// A single newline means nothing in Markdown — two lines run together
/// into one paragraph — so every line break becomes a hard one. Keeping
/// the lines is the point: in a scan they are the only structure there
/// is, and joining them into prose would lose the shape of a form, a
/// table or a list of questions.
fn tidy(text: &str) -> String {
    text.replace('\r', "\n")
        .lines()
        .map(|line| line.trim_start_matches('\u{feff}').trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        // Two trailing spaces: Markdown's hard line break.
        .join("  \n")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;
    use crate::{open_document_bytes, MemWorkingStore, SessionState};

    /// A blank page with an OCR layer on it — invisible text, which is
    /// the only kind a scanned page can carry.
    fn ocr_layer_over_a_blank_page(words: &[(&str, f32)]) -> Vec<u8> {
        use lopdf::{dictionary, Object, Stream};

        let blank = {
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
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 }),
            );
            let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
            doc.trailer.set("Root", catalog);
            let mut bytes = Vec::new();
            doc.save_to(&mut bytes).unwrap();
            bytes
        };

        let mut doc = openpdfedit_doc::Document::from_bytes(&blank).unwrap();
        let words: Vec<openpdfedit_ocr::OcrWord> = words
            .iter()
            .map(|(text, left)| openpdfedit_ocr::OcrWord {
                text: text.to_string(),
                left: *left,
                top: 100.0,
                width: 90.0,
                height: 40.0,
                confidence: 95.0,
                chars: Vec::new(),
            })
            .collect();
        openpdfedit_ocr::add_text_layer(&mut doc, 0, 612.0, 792.0, 612, 792, &words).unwrap();
        doc.save_incremental().unwrap()
    }

    /// The case the whole fallback exists for.
    ///
    /// `anydoc` refuses this document — "PDF has no extractable text:
    /// OCR is required" — because the only text on it is drawn
    /// invisibly, and a structural converter skips that on purpose. It
    /// is not wrong about the structure; it is wrong about the
    /// document, which has just been OCR'd and is full of words. Since
    /// scans are most of the reason to want Markdown out of a PDF at
    /// all, refusing them would leave the feature useless where it is
    /// needed most.
    #[test]
    fn an_ocrd_scan_converts_even_though_its_text_is_invisible() {
        let Some(engine) = crate::test_support::shared_handle() else {
            return;
        };
        let state = SessionState {
            engine: engine.clone(),
            docs: Mutex::new(HashMap::new()),
            history: Mutex::new(HashMap::new()),
            store: Box::new(MemWorkingStore::default()),
        };

        let bytes = ocr_layer_over_a_blank_page(&[("HELLO", 100.0), ("WORLD", 220.0)]);
        // What `anydoc` alone makes of it, stated rather than assumed —
        // if this ever starts succeeding, the fallback below has stopped
        // being the thing under test.
        assert!(
            anydoc::to_markdown_bytes(&bytes, anydoc::Format::Pdf).is_err(),
            "anydoc reads invisible text now — this test needs rethinking"
        );

        let opened = open_document_bytes(&state, "scan.pdf", bytes).expect("should open");
        let markdown = export_markdown_impl(&state, opened.handle).expect("should convert");

        assert!(
            markdown.contains("HELLO") && markdown.contains("WORLD"),
            "the OCR'd words have to come out: {markdown:?}"
        );
    }

    #[test]
    fn pages_are_separated_and_blank_ones_left_out() {
        assert_eq!(tidy("  one \r\n\r\n two  "), "one  \ntwo");
        assert_eq!(tidy("\u{feff}first\nsecond"), "first  \nsecond");
        assert_eq!(tidy("   \n  "), "");
    }
}
